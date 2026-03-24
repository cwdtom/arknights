use crate::dao::base_dao::BaseDao;
use anyhow::{Context, anyhow};
use rusqlite::OptionalExtension;
use std::path::{Path, PathBuf};
use zerocopy::IntoBytes;

#[derive(Debug, Clone)]
pub struct ChatHistoryVecDao {
    base: BaseDao,
    dimension: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ChatHistoryVectorMatch {
    pub chat_history_id: i64,
    pub distance: f64,
}

impl ChatHistoryVecDao {
    pub fn new(dimension: usize) -> anyhow::Result<Self> {
        let base = BaseDao::new()?;
        init_schema(&base, dimension)?;
        Ok(Self { base, dimension })
    }

    pub fn with_path<P>(db_path: P, dimension: usize) -> anyhow::Result<Self>
    where
        P: Into<PathBuf>,
    {
        let base = BaseDao::with_path(db_path)?;
        init_schema(&base, dimension)?;
        Ok(Self { base, dimension })
    }

    pub async fn upsert_embedding(
        &self,
        chat_history_id: i64,
        embedding: Vec<f32>,
    ) -> anyhow::Result<()> {
        let dimension = self.dimension;
        self.base
            .run_blocking(move |conn| {
                upsert_embedding_with_conn(conn, chat_history_id, embedding, dimension)
            })
            .await
    }

    pub async fn has_embedding(&self, chat_history_id: i64) -> anyhow::Result<bool> {
        self.base
            .run_blocking(move |conn| has_embedding_with_conn(conn, chat_history_id))
            .await
    }

    pub async fn count(&self) -> anyhow::Result<i64> {
        self.base.run_blocking(count_with_conn).await
    }

    pub async fn search(
        &self,
        query_embedding: Vec<f32>,
        limit: usize,
    ) -> anyhow::Result<Vec<ChatHistoryVectorMatch>> {
        let expected_dimension = self.dimension;

        self.base
            .run_blocking(move |conn| {
                search_with_conn(conn, query_embedding, expected_dimension, limit)
            })
            .await
    }

    pub fn db_path(&self) -> &Path {
        self.base.db_path()
    }

    pub fn dimension(&self) -> usize {
        self.dimension
    }
}

fn init_schema(base: &BaseDao, dimension: usize) -> anyhow::Result<()> {
    base.with_connection(|conn| match existing_table_dimension(conn)? {
        Some(existing_dimension) if existing_dimension != dimension => Err(anyhow!(
            "chat_history_vec dimension mismatch: existing {}, requested {}",
            existing_dimension,
            dimension
        )),
        Some(_) => Ok(()),
        None => {
            conn.execute(create_table_sql(dimension).as_str(), [])
                .context("create chat_history_vec table failed")?;
            Ok(())
        }
    })?;

    Ok(())
}

fn create_table_sql(dimension: usize) -> String {
    format!(
        "create virtual table if not exists chat_history_vec using vec0(
            chat_history_id integer primary key,
            embedding float[{dimension}] distance_metric=cosine
        );"
    )
}

fn existing_table_dimension(conn: &rusqlite::Connection) -> anyhow::Result<Option<usize>> {
    let sql: Option<String> = conn
        .query_row(
            "select sql from sqlite_master where type = 'table' and name = 'chat_history_vec'",
            [],
            |row| row.get(0),
        )
        .optional()?;

    sql.map(|sql| parse_dimension_from_sql(&sql)).transpose()
}

fn parse_dimension_from_sql(sql: &str) -> anyhow::Result<usize> {
    let start = sql
        .find("float[")
        .ok_or_else(|| anyhow!("chat_history_vec schema missing float dimension: {sql}"))?
        + "float[".len();
    let end = sql[start..]
        .find(']')
        .map(|offset| start + offset)
        .ok_or_else(|| anyhow!("chat_history_vec schema missing closing bracket: {sql}"))?;

    sql[start..end]
        .parse::<usize>()
        .context("parse chat_history_vec dimension failed")
}

fn upsert_embedding_with_conn(
    conn: &rusqlite::Connection,
    chat_history_id: i64,
    embedding: Vec<f32>,
    expected_dimension: usize,
) -> anyhow::Result<()> {
    if embedding.len() != expected_dimension {
        anyhow::bail!(
            "chat_history_vec embedding dimension mismatch: expected {}, got {}",
            expected_dimension,
            embedding.len()
        );
    }

    conn.execute(
        "delete from chat_history_vec where chat_history_id = ?1",
        rusqlite::params![chat_history_id],
    )
    .context("delete existing chat_history_vec failed")?;

    conn.execute(
        "insert into chat_history_vec (chat_history_id, embedding)
         values (?1, ?2)",
        rusqlite::params![chat_history_id, embedding.as_slice().as_bytes()],
    )
    .context("insert chat_history_vec failed")?;

    Ok(())
}

fn has_embedding_with_conn(
    conn: &rusqlite::Connection,
    chat_history_id: i64,
) -> anyhow::Result<bool> {
    let count: i64 = conn.query_row(
        "select count(*) from chat_history_vec where chat_history_id = ?1",
        rusqlite::params![chat_history_id],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

fn count_with_conn(conn: &rusqlite::Connection) -> anyhow::Result<i64> {
    let count: i64 = conn.query_row("select count(*) from chat_history_vec", [], |row| {
        row.get(0)
    })?;
    Ok(count)
}

fn search_with_conn(
    conn: &rusqlite::Connection,
    query_embedding: Vec<f32>,
    expected_dimension: usize,
    limit: usize,
) -> anyhow::Result<Vec<ChatHistoryVectorMatch>> {
    if query_embedding.len() != expected_dimension {
        anyhow::bail!(
            "chat_history_vec query dimension mismatch: expected {}, got {}",
            expected_dimension,
            query_embedding.len()
        );
    }

    let mut stmt = conn.prepare(
        "select chat_history_id, distance
         from chat_history_vec
         where embedding match ?1 and k = ?2
         order by distance",
    )?;

    let rows = stmt.query_map(
        rusqlite::params![query_embedding.as_slice().as_bytes(), limit as i64],
        |row| {
            Ok(ChatHistoryVectorMatch {
                chat_history_id: row.get(0)?,
                distance: row.get(1)?,
            })
        },
    )?;
    let mut matches = Vec::new();
    for row in rows {
        matches.push(row?);
    }

    Ok(matches)
}

#[cfg(test)]
#[path = "chat_history_vec_dao_tests.rs"]
mod tests;
