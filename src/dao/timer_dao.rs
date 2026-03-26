use crate::dao::base_dao::BaseDao;
use anyhow::{Context, anyhow};
use chrono::{DateTime, Local, SecondsFormat};
use rusqlite::{Connection, OptionalExtension, Row, params};
use std::path::{Path, PathBuf};

const CREATE_TABLE_SQL: &str = r#"
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

#[derive(Clone)]
pub struct TimerDao {
    base: BaseDao,
}

impl TimerDao {
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

    pub async fn create(&self, task: &NewTimerTask) -> anyhow::Result<()> {
        let task = task.clone();
        self.base
            .run_blocking(move |conn| create_with_conn(conn, &task))
            .await
    }

    pub async fn get(&self, id: &str) -> anyhow::Result<Option<TimerTask>> {
        let id = id.to_owned();
        self.base
            .run_blocking(move |conn| get_with_conn(conn, &id))
            .await
    }

    pub async fn list_active(&self) -> anyhow::Result<Vec<TimerTask>> {
        self.base.run_blocking(list_active_with_conn).await
    }

    pub async fn cancel(&self, id: &str) -> anyhow::Result<()> {
        let id = id.to_owned();
        self.base
            .run_blocking(move |conn| cancel_with_conn(conn, &id))
            .await
    }

    pub async fn list_due(
        &self,
        now: DateTime<Local>,
        limit: usize,
    ) -> anyhow::Result<Vec<TimerTask>> {
        self.base
            .run_blocking(move |conn| list_due_with_conn(conn, now, limit))
            .await
    }

    pub async fn mark_run_completed(
        &self,
        id: &str,
        remaining_runs: u32,
        next_trigger_at: &str,
        completed_at: &str,
        last_result: &str,
    ) -> anyhow::Result<()> {
        let id = id.to_owned();
        let next_trigger_at = next_trigger_at.to_owned();
        let completed_at = completed_at.to_owned();
        let last_result = last_result.to_owned();

        self.base
            .run_blocking(move |conn| {
                mark_run_completed_with_conn(
                    conn,
                    &id,
                    remaining_runs,
                    &next_trigger_at,
                    &completed_at,
                    &last_result,
                )
            })
            .await
    }

    pub fn db_path(&self) -> &Path {
        self.base.db_path()
    }

    fn map_row(row: &Row<'_>) -> rusqlite::Result<TimerTask> {
        let remaining_runs = row.get::<_, i64>(3)?;
        let remaining_runs = u32::try_from(remaining_runs).map_err(|err| {
            rusqlite::Error::FromSqlConversionFailure(
                3,
                rusqlite::types::Type::Integer,
                Box::new(err),
            )
        })?;

        Ok(TimerTask {
            id: row.get(0)?,
            prompt: row.get(1)?,
            cron_expr: row.get(2)?,
            remaining_runs,
            next_trigger_at: row.get(4)?,
            last_completed_at: row.get(5)?,
            last_result: row.get(6)?,
            created_at: row.get(7)?,
            updated_at: row.get(8)?,
        })
    }
}

fn init_schema(base: &BaseDao) -> anyhow::Result<()> {
    base.with_connection(|conn| {
        conn.execute(CREATE_TABLE_SQL, [])
            .context("create timer_tasks table failed")?;
        Ok(())
    })?;
    Ok(())
}

fn create_with_conn(conn: &Connection, task: &NewTimerTask) -> anyhow::Result<()> {
    if get_with_conn(conn, &task.id)?.is_some() {
        return Err(anyhow!("timer task already exists: {}", task.id));
    }

    let timestamp = current_timestamp();
    conn.execute(
        "insert into timer_tasks
         (id, prompt, cron_expr, remaining_runs, next_trigger_at, last_completed_at, last_result, created_at, updated_at)
         values (?1, ?2, ?3, ?4, ?5, null, null, ?6, ?6)",
        params![
            task.id,
            task.prompt,
            task.cron_expr,
            i64::from(task.remaining_runs),
            task.next_trigger_at,
            timestamp
        ],
    )
    .with_context(|| format!("insert timer task failed: {}", task.id))?;

    Ok(())
}

fn get_with_conn(conn: &Connection, id: &str) -> anyhow::Result<Option<TimerTask>> {
    conn.query_row(
        "select id, prompt, cron_expr, remaining_runs, next_trigger_at,
                last_completed_at, last_result, created_at, updated_at
         from timer_tasks
         where id = ?1",
        params![id],
        TimerDao::map_row,
    )
    .optional()
    .with_context(|| format!("select timer task failed: {id}"))
}

fn list_active_with_conn(conn: &Connection) -> anyhow::Result<Vec<TimerTask>> {
    let mut stmt = conn.prepare(
        "select id, prompt, cron_expr, remaining_runs, next_trigger_at,
                last_completed_at, last_result, created_at, updated_at
         from timer_tasks
         where remaining_runs > 0
         order by next_trigger_at asc, id asc",
    )?;
    let rows = stmt.query_map([], TimerDao::map_row)?;
    let mut tasks = Vec::new();

    for row in rows {
        tasks.push(row?);
    }
    Ok(tasks)
}

fn cancel_with_conn(conn: &Connection, id: &str) -> anyhow::Result<()> {
    let updated_at = current_timestamp();
    let rows = conn
        .execute(
            "update timer_tasks
             set remaining_runs = 0, updated_at = ?2
             where id = ?1",
            params![id, updated_at],
        )
        .with_context(|| format!("cancel timer task failed: {id}"))?;

    if rows == 0 {
        return Err(anyhow!("timer task not found for cancel: {id}"));
    }

    Ok(())
}

fn list_due_with_conn(
    conn: &Connection,
    now: DateTime<Local>,
    limit: usize,
) -> anyhow::Result<Vec<TimerTask>> {
    let now = now.to_rfc3339_opts(SecondsFormat::Secs, false);
    let mut stmt = conn.prepare(
        "select id, prompt, cron_expr, remaining_runs, next_trigger_at,
                last_completed_at, last_result, created_at, updated_at
         from timer_tasks
         where remaining_runs > 0 and (next_trigger_at <= ?1 or next_trigger_at is null)
         order by next_trigger_at asc, id asc
         limit ?2",
    )?;
    let rows = stmt.query_map(params![now, limit as i64], TimerDao::map_row)?;
    let mut tasks = Vec::new();

    for row in rows {
        tasks.push(row?);
    }

    Ok(tasks)
}

fn mark_run_completed_with_conn(
    conn: &Connection,
    id: &str,
    remaining_runs: u32,
    next_trigger_at: &str,
    completed_at: &str,
    last_result: &str,
) -> anyhow::Result<()> {
    let rows = conn
        .execute(
            "update timer_tasks
             set remaining_runs = ?2,
                 next_trigger_at = ?3,
                 last_completed_at = ?4,
                 last_result = ?5,
                 updated_at = ?4
             where id = ?1",
            params![
                id,
                i64::from(remaining_runs),
                next_trigger_at,
                completed_at,
                last_result
            ],
        )
        .with_context(|| format!("mark timer run completed failed: {id}"))?;

    if rows == 0 {
        return Err(anyhow!("timer task not found for completion: {id}"));
    }

    Ok(())
}

fn current_timestamp() -> String {
    Local::now().to_rfc3339_opts(SecondsFormat::Millis, false)
}

#[cfg(test)]
#[path = "timer_dao_tests.rs"]
mod tests;
