use crate::dao::schedule_dao::{
    NewScheduleEvent, ScheduleDao, ScheduleEvent, UpdateScheduleEvent as DaoUpdateScheduleEvent,
};
use anyhow::anyhow;
use chrono::{DateTime, Local, SecondsFormat};
use std::sync::LazyLock;

static SCHEDULE_DAO: LazyLock<anyhow::Result<ScheduleDao>> = LazyLock::new(ScheduleDao::new);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateScheduleEvent {
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

fn schedule_dao() -> anyhow::Result<&'static ScheduleDao> {
    SCHEDULE_DAO.as_ref().map_err(|err| anyhow!("{err:#}"))
}

pub async fn create(input: CreateScheduleEvent) -> anyhow::Result<ScheduleEvent> {
    let dao = schedule_dao()?;
    let times = normalize_event_times(input.start_time, input.end_time)?;
    let event = NewScheduleEvent {
        content: input.content,
        tag: input.tag,
        start_time: times.start_time,
        end_time: times.end_time,
    };
    let id = dao.create(&event).await?;
    load_event(dao, &id).await
}

pub async fn get_by_id(id: i64) -> anyhow::Result<Option<ScheduleEvent>> {
    schedule_dao()?.get(id).await
}

pub async fn list_by_range(start: String, end: String) -> anyhow::Result<Vec<ScheduleEvent>> {
    let range = normalize_list_range(start, end)?;
    schedule_dao()?
        .list_by_range(&range.start_time, &range.end_time)
        .await
}

pub async fn search(keyword: String) -> anyhow::Result<Vec<ScheduleEvent>> {
    schedule_dao()?.search(&keyword).await
}

pub async fn list_by_tag(tag: String) -> anyhow::Result<Vec<ScheduleEvent>> {
    schedule_dao()?.list_by_tag(&tag).await
}

pub async fn update(input: UpdateScheduleEvent) -> anyhow::Result<ScheduleEvent> {
    let dao = schedule_dao()?;
    let times = normalize_event_times(input.start_time, input.end_time)?;
    let event = DaoUpdateScheduleEvent {
        id: input.id,
        content: input.content,
        tag: input.tag,
        start_time: times.start_time,
        end_time: times.end_time,
    };
    dao.update(&event).await?;
    load_event(dao, &input.id).await
}

pub async fn remove(id: i64) -> anyhow::Result<()> {
    schedule_dao()?.remove(id).await
}

async fn load_event(dao: &ScheduleDao, id: &i64) -> anyhow::Result<ScheduleEvent> {
    dao.get(*id)
        .await?
        .ok_or_else(|| anyhow!("schedule event not found: {id}"))
}

struct NormalizedScheduleTimes {
    start_time: String,
    end_time: Option<String>,
}

struct NormalizedScheduleRange {
    start_time: String,
    end_time: String,
}

fn normalize_event_times(
    start_time: String,
    end_time: Option<String>,
) -> anyhow::Result<NormalizedScheduleTimes> {
    let start_time = parse_schedule_time("start_time", &start_time)?;
    let end_time = end_time
        .as_deref()
        .map(|value| parse_schedule_time("end_time", value))
        .transpose()?;

    if let Some(end_time) = end_time.as_ref() {
        ensure_valid_time_range(start_time, *end_time)?;
    }

    Ok(NormalizedScheduleTimes {
        start_time: format_schedule_time(start_time),
        end_time: end_time.map(format_schedule_time),
    })
}

fn normalize_list_range(
    start_time: String,
    end_time: String,
) -> anyhow::Result<NormalizedScheduleRange> {
    let start_time = parse_schedule_time("start", &start_time)?;
    let end_time = parse_schedule_time("end", &end_time)?;
    ensure_valid_time_range(start_time, end_time)?;

    Ok(NormalizedScheduleRange {
        start_time: format_schedule_time(start_time),
        end_time: format_schedule_time(end_time),
    })
}

fn parse_schedule_time(label: &str, value: &str) -> anyhow::Result<DateTime<Local>> {
    DateTime::parse_from_rfc3339(value)
        .map(|time| time.with_timezone(&Local))
        .map_err(|err| anyhow!("invalid schedule {label}: {err}"))
}

fn ensure_valid_time_range(
    start_time: DateTime<Local>,
    end_time: DateTime<Local>,
) -> anyhow::Result<()> {
    if end_time < start_time {
        return Err(anyhow!(
            "schedule end_time must be greater than or equal to start_time"
        ));
    }

    Ok(())
}

fn format_schedule_time(value: DateTime<Local>) -> String {
    value.to_rfc3339_opts(SecondsFormat::Millis, false)
}
