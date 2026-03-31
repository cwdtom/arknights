use serde::Serialize;

pub(crate) const CREATE_TABLE_SQL: &str = r#"
create table if not exists schedule_events
(
    id         INTEGER primary key autoincrement,
    content    TEXT not null,
    tag        TEXT,
    start_time TEXT not null,
    end_time   TEXT,
    created_at TEXT not null,
    updated_at TEXT not null
);
"#;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewScheduleEvent {
    pub content: String,
    pub tag: Option<String>,
    pub start_time: String,
    pub end_time: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateScheduleEvent {
    pub id: i64,
    pub content: String,
    pub tag: Option<String>,
    pub start_time: String,
    pub end_time: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ScheduleEvent {
    pub id: i64,
    pub content: String,
    pub tag: Option<String>,
    pub start_time: String,
    pub end_time: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}
