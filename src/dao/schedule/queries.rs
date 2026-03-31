use super::{NewScheduleEvent, ScheduleEvent, UpdateScheduleEvent};
use anyhow::{Context, anyhow};
use chrono::{Local, SecondsFormat};
use rusqlite::{Connection, OptionalExtension, Row, params};

const SELECT_SQL: &str = "
select id, content, tag, start_time, end_time, created_at, updated_at
from schedule_events
";

pub(crate) fn create_with_conn(conn: &Connection, event: &NewScheduleEvent) -> anyhow::Result<()> {
    let timestamp = current_timestamp();
    conn.execute(
        "insert into schedule_events
         (id, content, tag, start_time, end_time, created_at, updated_at)
         values (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
        params![
            event.id,
            event.content,
            event.tag,
            event.start_time,
            event.end_time,
            timestamp
        ],
    )
    .with_context(|| format!("insert schedule event failed: {}", event.id))?;

    Ok(())
}

pub(crate) fn get_with_conn(conn: &Connection, id: &str) -> anyhow::Result<Option<ScheduleEvent>> {
    conn.query_row(&format!("{SELECT_SQL} where id = ?1"), params![id], map_row)
        .optional()
        .with_context(|| format!("select schedule event failed: {id}"))
}

pub(crate) fn list_by_range_with_conn(
    conn: &Connection,
    start: &str,
    end: &str,
) -> anyhow::Result<Vec<ScheduleEvent>> {
    query_events(
        conn,
        &format!("{SELECT_SQL} where start_time >= ?1 and start_time <= ?2 order by start_time asc, id asc"),
        params![start, end],
    )
}

pub(crate) fn search_with_conn(
    conn: &Connection,
    keyword: &str,
) -> anyhow::Result<Vec<ScheduleEvent>> {
    let pattern = format!("%{keyword}%");
    query_events(
        conn,
        &format!("{SELECT_SQL} where content like ?1 order by start_time asc, id asc"),
        params![pattern],
    )
}

pub(crate) fn list_by_tag_with_conn(
    conn: &Connection,
    tag: &str,
) -> anyhow::Result<Vec<ScheduleEvent>> {
    query_events(
        conn,
        &format!("{SELECT_SQL} where tag = ?1 order by start_time asc, id asc"),
        params![tag],
    )
}

pub(crate) fn update_with_conn(
    conn: &Connection,
    event: &UpdateScheduleEvent,
) -> anyhow::Result<()> {
    let rows = conn
        .execute(
            "update schedule_events
             set content = ?2,
                 tag = ?3,
                 start_time = ?4,
                 end_time = ?5,
                 updated_at = ?6
             where id = ?1",
            params![
                event.id,
                event.content,
                event.tag,
                event.start_time,
                event.end_time,
                current_timestamp()
            ],
        )
        .with_context(|| format!("update schedule event failed: {}", event.id))?;

    if rows == 0 {
        return Err(anyhow!("schedule event not found for update: {}", event.id));
    }

    Ok(())
}

pub(crate) fn remove_with_conn(conn: &Connection, id: &str) -> anyhow::Result<()> {
    let rows = conn
        .execute("delete from schedule_events where id = ?1", params![id])
        .with_context(|| format!("remove schedule event failed: {id}"))?;

    if rows == 0 {
        return Err(anyhow!("schedule event not found for remove: {id}"));
    }

    Ok(())
}

fn query_events<P>(conn: &Connection, sql: &str, params: P) -> anyhow::Result<Vec<ScheduleEvent>>
where
    P: rusqlite::Params,
{
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map(params, map_row)?;
    let mut events = Vec::new();

    for row in rows {
        events.push(row?);
    }

    Ok(events)
}

fn map_row(row: &Row<'_>) -> rusqlite::Result<ScheduleEvent> {
    Ok(ScheduleEvent {
        id: row.get(0)?,
        content: row.get(1)?,
        tag: row.get(2)?,
        start_time: row.get(3)?,
        end_time: row.get(4)?,
        created_at: row.get(5)?,
        updated_at: row.get(6)?,
    })
}

fn current_timestamp() -> String {
    Local::now().to_rfc3339_opts(SecondsFormat::Millis, false)
}
