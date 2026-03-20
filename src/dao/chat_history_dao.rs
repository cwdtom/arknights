use crate::dao::base_dao::BaseDao;
use anyhow::Context;
use chrono::Utc;
use rusqlite::OptionalExtension;
use rusqlite::{Row, params};
use serde::Serialize;
use std::path::{Path, PathBuf};

const CREATE_TABLE_SQL: &str = r#"
create table if not exists chat_history
(
    id                INTEGER
        primary key autoincrement,
    user_content      TEXT not null,
    assistant_content TEXT not null,
    created_at        TEXT not null
);
"#;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ChatHistory {
    pub id: i64,
    pub user_content: String,
    pub assistant_content: String,
    pub created_at: String,
}

#[derive(Debug, Clone)]
pub struct ChatHistoryDao {
    base: BaseDao,
}

impl ChatHistoryDao {
    pub fn new() -> anyhow::Result<Self> {
        let base = BaseDao::new()?;
        init_schema(&base)?;
        Ok(Self { base })
    }

    pub fn with_path<P>(db_path: P) -> anyhow::Result<Self>
    where
        P: Into<PathBuf>,
    {
        let base = BaseDao::with_path(db_path)?;
        init_schema(&base)?;
        Ok(Self { base })
    }

    pub async fn insert(&self, user_content: &str, assistant_content: &str) -> anyhow::Result<i64> {
        let user_content = user_content.to_owned();
        let assistant_content = assistant_content.to_owned();

        self.base
            .run_blocking(move |conn| insert_with_conn(conn, &user_content, &assistant_content))
            .await
    }

    pub async fn list(&self, limit: usize, offset: usize) -> anyhow::Result<Vec<ChatHistory>> {
        self.base
            .run_blocking(move |conn| list_with_conn(conn, limit, offset))
            .await
    }

    pub async fn get(&self, id: i64) -> anyhow::Result<Option<ChatHistory>> {
        self.base
            .run_blocking(move |conn| get_with_conn(conn, id))
            .await
    }

    pub async fn fuzzy_query(
        &self,
        keyword: &str,
        limit: usize,
        offset: usize,
    ) -> anyhow::Result<Vec<ChatHistory>> {
        let keyword = keyword.to_owned();
        self.base
            .run_blocking(move |conn| fuzzy_query_with_conn(conn, &keyword, limit, offset))
            .await
    }

    fn map_row(row: &Row<'_>) -> rusqlite::Result<ChatHistory> {
        Ok(ChatHistory {
            id: row.get(0)?,
            user_content: row.get(1)?,
            assistant_content: row.get(2)?,
            created_at: row.get(3)?,
        })
    }

    pub fn db_path(&self) -> &Path {
        self.base.db_path()
    }
}

fn init_schema(base: &BaseDao) -> anyhow::Result<()> {
    base.with_connection(|conn| {
        conn.execute(CREATE_TABLE_SQL, [])
            .context("create chat_history table failed")?;
        Ok(())
    })?;

    Ok(())
}

fn insert_with_conn(
    conn: &rusqlite::Connection,
    user_content: &str,
    assistant_content: &str,
) -> anyhow::Result<i64> {
    let created_at = Utc::now().to_rfc3339();

    conn.execute(
        "insert into chat_history (user_content, assistant_content, created_at)
         values (?1, ?2, ?3)",
        params![user_content, assistant_content, created_at],
    )
    .context("insert chat_history failed")?;

    Ok(conn.last_insert_rowid())
}

fn list_with_conn(
    conn: &rusqlite::Connection,
    limit: usize,
    offset: usize,
) -> anyhow::Result<Vec<ChatHistory>> {
    let mut stmt = conn.prepare(
        "select id, user_content, assistant_content, created_at
         from chat_history
         order by id desc
         limit ?1 offset ?2",
    )?;

    let rows = stmt.query_map(
        params![limit as i64, offset as i64],
        ChatHistoryDao::map_row,
    )?;
    let mut histories = Vec::new();
    for row in rows {
        histories.push(row?);
    }

    Ok(histories)
}

fn get_with_conn(conn: &rusqlite::Connection, id: i64) -> anyhow::Result<Option<ChatHistory>> {
    conn.query_row(
        "select id, user_content, assistant_content, created_at
         from chat_history
         where id = ?1",
        params![id],
        ChatHistoryDao::map_row,
    )
    .optional()
    .map_err(Into::into)
}

fn fuzzy_query_with_conn(
    conn: &rusqlite::Connection,
    keyword: &str,
    limit: usize,
    offset: usize,
) -> anyhow::Result<Vec<ChatHistory>> {
    let pattern = build_like_pattern(keyword);
    let mut stmt = conn.prepare(
        "select id, user_content, assistant_content, created_at
         from chat_history
         where user_content like ?1 escape '\\'
            or assistant_content like ?1 escape '\\'
         order by id desc
         limit ?2 offset ?3",
    )?;

    let rows = stmt.query_map(
        params![pattern, limit as i64, offset as i64],
        ChatHistoryDao::map_row,
    )?;
    let mut histories = Vec::new();
    for row in rows {
        histories.push(row?);
    }

    Ok(histories)
}

fn build_like_pattern(keyword: &str) -> String {
    let mut pattern = String::with_capacity(keyword.len() + 2);
    pattern.push('%');

    for ch in keyword.chars() {
        match ch {
            '%' | '_' | '\\' => {
                pattern.push('\\');
                pattern.push(ch);
            }
            _ => pattern.push(ch),
        }
    }

    pattern.push('%');
    pattern
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[tokio::test]
    async fn insert_and_list_work() {
        let path = unique_db_path("list");
        let dao = ChatHistoryDao::with_path(&path).unwrap();

        dao.insert("hello", "world").await.unwrap();
        dao.insert("question", "answer").await.unwrap();

        let rows = dao.list(10, 0).await.unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].user_content, "question");
        assert_eq!(rows[1].user_content, "hello");

        cleanup_db(&path);
    }

    #[tokio::test]
    async fn fuzzy_query_matches_user_and_assistant_content() {
        let path = unique_db_path("fuzzy");
        let dao = ChatHistoryDao::with_path(&path).unwrap();

        dao.insert("deploy status", "done").await.unwrap();
        dao.insert("hello", "status is pending").await.unwrap();
        dao.insert("bye", "ok").await.unwrap();

        let rows = dao.fuzzy_query("status", 10, 0).await.unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].user_content, "hello");
        assert_eq!(rows[1].user_content, "deploy status");

        cleanup_db(&path);
    }

    #[tokio::test]
    async fn get_returns_row_when_id_exists() {
        let path = unique_db_path("get");
        let dao = ChatHistoryDao::with_path(&path).unwrap();

        let id = dao.insert("question", "answer").await.unwrap();

        let row = dao.get(id).await.unwrap().unwrap();
        assert_eq!(row.id, id);
        assert_eq!(row.user_content, "question");
        assert_eq!(row.assistant_content, "answer");

        cleanup_db(&path);
    }

    #[tokio::test]
    async fn fuzzy_query_escapes_like_wildcards() {
        let path = unique_db_path("escape");
        let dao = ChatHistoryDao::with_path(&path).unwrap();

        dao.insert("100% progress", "done").await.unwrap();
        dao.insert("1000 progress", "done").await.unwrap();

        let rows = dao.fuzzy_query("100%", 10, 0).await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].user_content, "100% progress");

        cleanup_db(&path);
    }

    #[tokio::test]
    async fn in_memory_database_reuses_same_connection() {
        let dao = ChatHistoryDao::with_path(":memory:").unwrap();

        dao.insert("hello", "world").await.unwrap();

        let rows = dao.list(10, 0).await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].user_content, "hello");
    }

    fn unique_db_path(prefix: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();

        std::env::temp_dir().join(format!("arknights_{prefix}_{nanos}.db"))
    }

    fn cleanup_db(path: &Path) {
        let _ = fs::remove_file(path);
        let _ = fs::remove_file(format!("{}-shm", path.to_string_lossy()));
        let _ = fs::remove_file(format!("{}-wal", path.to_string_lossy()));
    }
}
