use crate::agent::plan::Plan;
use crate::dao::timer_dao::{
    NewTimerTask, TimerDao, TimerTask, UpdateTimerTask as DaoUpdateTimerTask,
};
use anyhow::anyhow;
use chrono::{DateTime, Local};
use cron_lite::Schedule;
use std::str::FromStr;
use std::sync::LazyLock;
use std::time::Duration;
use tracing::{error, info};

tokio::task_local! {
    static TIMER_ID_THREAD_LOCAL: String;
}

static TIMER_DAO: LazyLock<anyhow::Result<TimerDao>> = LazyLock::new(TimerDao::new);

// 10s check once
const PERIOD: u64 = 10;
const DUE_TASK_LIMIT: usize = 32;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateTimerTask {
    pub id: String,
    pub prompt: String,
    pub cron_expr: String,
    pub remaining_runs: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateTimerTask {
    pub id: String,
    pub prompt: String,
    pub cron_expr: String,
    pub remaining_runs: u32,
}

fn timer_dao() -> anyhow::Result<&'static TimerDao> {
    TIMER_DAO.as_ref().map_err(|err| anyhow!("{err:#}"))
}

pub async fn create(input: CreateTimerTask) -> anyhow::Result<TimerTask> {
    let dao = timer_dao()?;
    let next_trigger_at = build_next_trigger_at(&input.cron_expr, Local::now())?;
    let task = NewTimerTask {
        id: input.id.clone(),
        prompt: input.prompt,
        cron_expr: input.cron_expr,
        remaining_runs: input.remaining_runs,
        next_trigger_at,
    };
    dao.create(&task).await?;
    load_task(dao, &input.id).await
}

pub async fn get_by_id(id: String) -> anyhow::Result<Option<TimerTask>> {
    let dao = timer_dao()?;
    dao.get(&id).await
}

pub async fn list() -> anyhow::Result<Vec<TimerTask>> {
    let dao = timer_dao()?;
    dao.list().await
}

pub async fn update(input: UpdateTimerTask) -> anyhow::Result<TimerTask> {
    let dao = timer_dao()?;
    let next_trigger_at = build_next_trigger_at(&input.cron_expr, Local::now())?;
    let task = DaoUpdateTimerTask {
        id: input.id.clone(),
        prompt: input.prompt,
        cron_expr: input.cron_expr,
        remaining_runs: input.remaining_runs,
        next_trigger_at,
    };
    dao.update(&task).await?;
    load_task(dao, &input.id).await
}

pub async fn remove(id: String) -> anyhow::Result<()> {
    let dao = timer_dao()?;
    dao.remove(&id).await
}

async fn list_due(now: DateTime<Local>) -> anyhow::Result<Vec<TimerTask>> {
    let dao = timer_dao()?;
    dao.list_due(now, DUE_TASK_LIMIT).await
}

pub fn get_thread_local_timer_id() -> Option<String> {
    match TIMER_ID_THREAD_LOCAL.try_with(|v| v.clone()) {
        Ok(val) => Some(val),
        Err(_) => None,
    }
}

async fn mark_run_completed(
    id: &str,
    remaining_runs: u32,
    next_trigger_at: &str,
    completed_at: &str,
    result: &str,
) -> anyhow::Result<()> {
    let dao = timer_dao()?;
    dao.mark_run_completed(id, remaining_runs, next_trigger_at, completed_at, result)
        .await
}

pub fn init_timer() {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(PERIOD)).await;

            let now = Local::now();
            let tasks = match list_due(now).await {
                Ok(tasks) => tasks,
                Err(err) => {
                    error!("task list_due failed: {:?}", err);
                    continue;
                }
            };
            for task in tasks {
                TIMER_ID_THREAD_LOCAL
                    .scope(task.id.clone(), async {
                        match execute_task(task, now).await {
                            Ok(_) => info!("task executed successfully."),
                            Err(err) => {
                                error!("task execute failed: {:?}", err);
                            }
                        }
                    })
                    .await;
            }
        }
    });
}

async fn execute_task(task: TimerTask, now: DateTime<Local>) -> anyhow::Result<()> {
    info!("task executed: {:?}", task);

    let mut plan = Plan::new(task.prompt.clone()).await?;
    let result = plan.execute().await?;

    let remaining_runs = task.remaining_runs.saturating_sub(1);
    let next_trigger_at = build_next_trigger_at(&task.cron_expr, now)?;
    let completed_at = now.to_rfc3339();

    mark_run_completed(
        &task.id,
        remaining_runs,
        &next_trigger_at,
        &completed_at,
        &result,
    )
    .await
}

async fn load_task(dao: &TimerDao, id: &str) -> anyhow::Result<TimerTask> {
    dao.get(id)
        .await?
        .ok_or_else(|| anyhow!("timer task not found: {id}"))
}

fn build_next_trigger_at(cron: &str, now: DateTime<Local>) -> anyhow::Result<String> {
    let schedule = Schedule::from_str(cron)?;
    let next = schedule
        .iter(&now)
        .find(|next| *next > now)
        .ok_or_else(|| anyhow!("build next trigger failed: {cron}"))?;
    Ok(next.to_rfc3339())
}
