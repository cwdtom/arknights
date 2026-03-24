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
