use crate::dao::chat_history_dao::ChatHistoryDao;
use crate::dao::chat_history_vec_dao::ChatHistoryVecDao;
use crate::llm::{Message, Role};
use crate::memory::rag_embedder::{self, RagConfig, RagRuntimeConfig, SharedRagEmbeddingBackend};
use anyhow::anyhow;
use chrono::{DateTime, Duration, Local};
use std::sync::LazyLock;
use tracing::{error, info};

static CHAT_HISTORY_DAO: LazyLock<anyhow::Result<ChatHistoryDao>> =
    LazyLock::new(ChatHistoryDao::new);

fn chat_history_dao() -> anyhow::Result<&'static ChatHistoryDao> {
    CHAT_HISTORY_DAO.as_ref().map_err(|err| anyhow!("{err:#}"))
}

fn chat_history_vec_dao(config: &RagRuntimeConfig) -> anyhow::Result<ChatHistoryVecDao> {
    ChatHistoryVecDao::new(config.model.dimension())
}

const RAG_SEARCH_LIMIT: usize = 5;

pub async fn save_chat_history(user_content: &str, assistant_content: &str) -> anyhow::Result<i64> {
    save_chat_history_with_backend(
        user_content,
        assistant_content,
        rag_embedder::default_backend(),
    )
    .await
}

async fn save_chat_history_with_backend(
    user_content: &str,
    assistant_content: &str,
    backend: SharedRagEmbeddingBackend,
) -> anyhow::Result<i64> {
    let dao = chat_history_dao()?;
    let id = dao.insert(user_content, assistant_content).await?;
    spawn_index_chat_history(
        id,
        user_content.to_string(),
        assistant_content.to_string(),
        backend,
    );
    Ok(id)
}

pub async fn build_chat_history_messages(limit: usize) -> anyhow::Result<Vec<Message>> {
    let dao = chat_history_dao()?;
    let mut histories = dao.list(limit, 0).await?;
    histories.reverse();

    let mut messages = Vec::with_capacity(histories.len() * 2);
    let yesterday = Local::now() - Duration::hours(24);
    for history in histories {
        // create time must less than 24 hours
        let create_time = DateTime::parse_from_rfc3339(&history.created_at)?.with_timezone(&Local);
        if create_time < yesterday {
            continue;
        }

        let user_content = history.user_content.clone();
        let assistant_content = history.assistant_content.clone();
        let create_time_str = history.created_at;
        messages.push(Message::new(
            Role::User,
            format!("[{create_time_str}] {user_content}"),
        ));
        messages.push(Message::new(
            Role::Assistant,
            format!("[{create_time_str}] {assistant_content}"),
        ));
    }

    Ok(messages)
}

pub async fn fuzz_query(keywords: Vec<String>) -> anyhow::Result<String> {
    let dao = chat_history_dao()?;

    let mut histories = vec![];
    for k in keywords {
        let arr = dao.fuzzy_query(k.as_str(), 5, 0).await?;
        for a in arr {
            match serde_json::to_string(&a) {
                Ok(json) => histories.push(json),
                Err(_err) => continue,
            }
        }
    }

    Ok(histories.join("\n"))
}

pub async fn search_rag(keywords: Vec<String>) -> anyhow::Result<String> {
    search_rag_with_backend(keywords, rag_embedder::default_backend()).await
}

async fn search_rag_with_backend(
    keywords: Vec<String>,
    backend: SharedRagEmbeddingBackend,
) -> anyhow::Result<String> {
    let (query, keyword_count) = build_rag_query(keywords)?;

    info!(
        event = "rag_search_start",
        query = %query,
        keyword_count
    );

    let result = async {
        let config = match RagConfig::from_env()? {
            RagConfig::Enabled(config) => config,
            RagConfig::Disabled => anyhow::bail!("rag search requires ARKNIGHTS_RAG_MODEL"),
        };

        let query_embedding =
            rag_embedder::embed_text_with_backend(config.clone(), query.clone(), backend.as_ref())
                .await?;
        let dimension = query_embedding.len();
        let vec_dao = chat_history_vec_dao(&config)?;
        let matches = vec_dao.search(query_embedding, RAG_SEARCH_LIMIT).await?;
        let dao = chat_history_dao()?;
        let mut histories = Vec::with_capacity(matches.len());

        for matched in matches {
            let history = dao.get(matched.chat_history_id).await?.ok_or_else(|| {
                anyhow!(
                    "chat history {} missing for rag result",
                    matched.chat_history_id
                )
            })?;
            histories.push(serde_json::to_string(&history)?);
        }

        Ok((histories.join("\n"), dimension))
    }
    .await;

    match result {
        Ok((joined, dimension)) => {
            let result_count = if joined.is_empty() {
                0
            } else {
                joined.lines().count()
            };
            info!(
                event = "rag_search_success",
                query = %query,
                result_count,
                dimension
            );
            Ok(joined)
        }
        Err(err) => {
            error!(event = "rag_search_failed", query = %query, error = %err);
            Err(err)
        }
    }
}

fn build_rag_query(keywords: Vec<String>) -> anyhow::Result<(String, usize)> {
    let normalized_keywords = keywords
        .into_iter()
        .map(|keyword| keyword.trim().to_string())
        .filter(|keyword| !keyword.is_empty())
        .collect::<Vec<_>>();

    if normalized_keywords.is_empty() {
        anyhow::bail!("rag search keywords must not be empty");
    }

    Ok((normalized_keywords.join(" "), normalized_keywords.len()))
}

fn spawn_index_chat_history(
    chat_history_id: i64,
    user_content: String,
    assistant_content: String,
    backend: SharedRagEmbeddingBackend,
) {
    let config = match RagConfig::from_env() {
        Ok(RagConfig::Enabled(config)) => config,
        Ok(RagConfig::Disabled) => {
            info!(
                event = "rag_index_skipped",
                chat_history_id,
                reason = "model_not_configured"
            );
            return;
        }
        Err(err) => {
            error!(event = "rag_index_failed", chat_history_id, error = %err);
            return;
        }
    };

    info!(
        event = "rag_index_schedule",
        chat_history_id,
        model = config.model.model_name(),
        cache_dir = %config.cache_dir.display()
    );

    tokio::spawn(async move {
        match index_chat_history(
            chat_history_id,
            config,
            user_content,
            assistant_content,
            backend,
        )
        .await
        {
            Ok(dimension) => {
                info!(event = "rag_index_success", chat_history_id, dimension);
            }
            Err(err) => {
                error!(event = "rag_index_failed", chat_history_id, error = %err);
            }
        }
    });
}

async fn index_chat_history(
    chat_history_id: i64,
    config: RagRuntimeConfig,
    user_content: String,
    assistant_content: String,
    backend: SharedRagEmbeddingBackend,
) -> anyhow::Result<usize> {
    let embedding = rag_embedder::embed_chat_history_with_backend(
        config.clone(),
        &user_content,
        &assistant_content,
        backend.as_ref(),
    )
    .await?;
    let dimension = embedding.len();
    let dao = chat_history_vec_dao(&config)?;
    dao.upsert_embedding(chat_history_id, embedding).await?;
    Ok(dimension)
}

#[cfg(test)]
#[path = "chat_history_service_tests.rs"]
mod tests;
