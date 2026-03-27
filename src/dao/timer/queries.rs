use super::{NewTimerTask, TimerTask, UpdateTimerTask};
use anyhow::{Context, anyhow};
use chrono::{DateTime, Local, SecondsFormat};
use rusqlite::{Connection, OptionalExtension, Row, params};

const SELECT_SQL: &str = "
select id, prompt, cron_expr, remaining_runs, next_trigger_at,
       last_completed_at, last_result, created_at, updated_at
from timer_tasks
";

pub(crate) fn create_with_conn(conn: &Connection, task: &NewTimerTask) -> anyhow::Result<()> {
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

pub(crate) fn get_with_conn(conn: &Connection, id: &str) -> anyhow::Result<Option<TimerTask>> {
    conn.query_row(&format!("{SELECT_SQL} where id = ?1"), params![id], map_row)
        .optional()
        .with_context(|| format!("select timer task failed: {id}"))
}

pub(crate) fn list_with_conn(conn: &Connection) -> anyhow::Result<Vec<TimerTask>> {
    query_tasks(
        conn,
        &format!("{SELECT_SQL} order by next_trigger_at asc, id asc"),
        [],
    )
}

pub(crate) fn list_active_with_conn(conn: &Connection) -> anyhow::Result<Vec<TimerTask>> {
    query_tasks(
        conn,
        &format!("{SELECT_SQL} where remaining_runs > 0 order by next_trigger_at asc, id asc"),
        [],
    )
}

pub(crate) fn update_with_conn(conn: &Connection, task: &UpdateTimerTask) -> anyhow::Result<()> {
    let rows = conn
        .execute(
            "update timer_tasks
             set prompt = ?2,
                 cron_expr = ?3,
                 remaining_runs = ?4,
                 next_trigger_at = ?5,
                 updated_at = ?6
             where id = ?1",
            params![
                task.id,
                task.prompt,
                task.cron_expr,
                i64::from(task.remaining_runs),
                task.next_trigger_at,
                current_timestamp()
            ],
        )
        .with_context(|| format!("update timer task failed: {}", task.id))?;

    if rows == 0 {
        return Err(anyhow!("timer task not found for update: {}", task.id));
    }

    Ok(())
}

pub(crate) fn cancel_with_conn(conn: &Connection, id: &str) -> anyhow::Result<()> {
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

pub(crate) fn remove_with_conn(conn: &Connection, id: &str) -> anyhow::Result<()> {
    let rows = conn
        .execute("delete from timer_tasks where id = ?1", params![id])
        .with_context(|| format!("remove timer task failed: {id}"))?;

    if rows == 0 {
        return Err(anyhow!("timer task not found for remove: {id}"));
    }

    Ok(())
}

pub(crate) fn list_due_with_conn(
    conn: &Connection,
    now: DateTime<Local>,
    limit: usize,
) -> anyhow::Result<Vec<TimerTask>> {
    let now = now.to_rfc3339_opts(SecondsFormat::Secs, false);
    let mut stmt = conn.prepare(&format!(
        "{SELECT_SQL}
         where remaining_runs > 0 and (next_trigger_at <= ?1 or next_trigger_at is null)
         order by next_trigger_at asc, id asc
         limit ?2"
    ))?;
    let rows = stmt.query_map(params![now, limit as i64], map_row)?;
    let mut tasks = Vec::new();

    for row in rows {
        tasks.push(row?);
    }

    Ok(tasks)
}

pub(crate) fn mark_run_completed_with_conn(
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

fn query_tasks<P>(conn: &Connection, sql: &str, params: P) -> anyhow::Result<Vec<TimerTask>>
where
    P: rusqlite::Params,
{
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map(params, map_row)?;
    let mut tasks = Vec::new();

    for row in rows {
        tasks.push(row?);
    }

    Ok(tasks)
}

fn map_row(row: &Row<'_>) -> rusqlite::Result<TimerTask> {
    let remaining_runs = row.get::<_, i64>(3)?;
    let remaining_runs = u32::try_from(remaining_runs).map_err(|err| {
        rusqlite::Error::FromSqlConversionFailure(3, rusqlite::types::Type::Integer, Box::new(err))
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

fn current_timestamp() -> String {
    Local::now().to_rfc3339_opts(SecondsFormat::Millis, false)
}
