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
#[path = "chat_history_dao_tests.rs"]
mod tests;
