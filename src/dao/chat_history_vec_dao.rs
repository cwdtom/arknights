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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[tokio::test]
    async fn schema_initialization_creates_chat_history_vec_table() {
        let path = unique_db_path("schema");
        let dao = ChatHistoryVecDao::with_path(&path, 384).unwrap();

        let exists = dao
            .base
            .with_connection(|conn| {
                let count: i64 = conn.query_row(
                    "select count(*) from sqlite_master where type = 'table' and name = 'chat_history_vec'",
                    [],
                    |row| row.get(0),
                )?;
                Ok(count > 0)
            })
            .unwrap();

        assert!(exists);
        assert_eq!(dao.dimension(), 384);

        cleanup_db(&path);
    }

    #[tokio::test]
    async fn upsert_embedding_writes_one_vector_row() {
        let path = unique_db_path("upsert");
        let dao = ChatHistoryVecDao::with_path(&path, 384).unwrap();

        dao.upsert_embedding(7, vec![0.1; 384]).await.unwrap();

        assert!(dao.has_embedding(7).await.unwrap());
        assert_eq!(dao.count().await.unwrap(), 1);

        dao.upsert_embedding(7, vec![0.2; 384]).await.unwrap();

        assert!(dao.has_embedding(7).await.unwrap());
        assert_eq!(dao.count().await.unwrap(), 1);

        cleanup_db(&path);
    }

    #[tokio::test]
    async fn existing_table_with_different_dimension_returns_error() {
        let path = unique_db_path("mismatch");
        let _ = ChatHistoryVecDao::with_path(&path, 384).unwrap();

        let err = ChatHistoryVecDao::with_path(&path, 512).unwrap_err();
        assert!(
            err.to_string()
                .contains("chat_history_vec dimension mismatch")
        );

        cleanup_db(&path);
    }

    fn unique_db_path(prefix: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();

        std::env::temp_dir().join(format!("arknights_chat_history_vec_{prefix}_{nanos}.db"))
    }

    fn cleanup_db(path: &Path) {
        let _ = fs::remove_file(path);
        let _ = fs::remove_file(format!("{}-shm", path.to_string_lossy()));
        let _ = fs::remove_file(format!("{}-wal", path.to_string_lossy()));
    }
}
