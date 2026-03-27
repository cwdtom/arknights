use serde::Serialize;

pub(crate) const CREATE_TABLE_SQL: &str = r#"
create table if not exists timer_tasks
(
    id                TEXT primary key,
    prompt            TEXT not null,
    cron_expr         TEXT not null,
    remaining_runs    INTEGER not null,
    next_trigger_at   TEXT,
    last_completed_at TEXT,
    last_result       TEXT,
    created_at        TEXT not null,
    updated_at        TEXT not null
);
"#;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewTimerTask {
    pub id: String,
    pub prompt: String,
    pub cron_expr: String,
    pub remaining_runs: u32,
    pub next_trigger_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateTimerTask {
    pub id: String,
    pub prompt: String,
    pub cron_expr: String,
    pub remaining_runs: u32,
    pub next_trigger_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TimerTask {
    pub id: String,
    pub prompt: String,
    pub cron_expr: String,
    pub remaining_runs: u32,
    pub next_trigger_at: Option<String>,
    pub last_completed_at: Option<String>,
    pub last_result: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}
