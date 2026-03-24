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

#[tokio::test]
async fn search_returns_closest_rows_in_distance_order() {
    let path = unique_db_path("search");
    let dao = ChatHistoryVecDao::with_path(&path, 2).unwrap();

    dao.upsert_embedding(1, vec![1.0, 0.0]).await.unwrap();
    dao.upsert_embedding(2, vec![0.8, 0.2]).await.unwrap();
    dao.upsert_embedding(3, vec![0.0, 1.0]).await.unwrap();

    let matches = dao.search(vec![1.0, 0.0], 2).await.unwrap();

    assert_eq!(matches.len(), 2);
    assert_eq!(matches[0].chat_history_id, 1);
    assert_eq!(matches[1].chat_history_id, 2);
    assert!(matches[0].distance <= matches[1].distance);

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
