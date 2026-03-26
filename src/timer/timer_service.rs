use crate::agent::plan::Plan;
use crate::dao::timer_dao::{TimerDao, TimerTask};
use anyhow::anyhow;
use chrono::{DateTime, Local};
use cron_lite::Schedule;
use std::str::FromStr;
use std::sync::LazyLock;
use std::time::Duration;
use tracing::{error};

static TIMER_DAO: LazyLock<anyhow::Result<TimerDao>> = LazyLock::new(TimerDao::new);

// 10s check once
const PERIOD: u64 = 10;
const DUE_TASK_LIMIT: usize = 32;

fn timer_dao() -> anyhow::Result<&'static TimerDao> {
    TIMER_DAO.as_ref().map_err(|err| anyhow!("{err:#}"))
}

pub async fn get_by_id(id: String) -> anyhow::Result<Option<TimerTask>> {
    let dao = timer_dao()?;
    dao.get(&id).await
}

pub fn init_timer() {
    tokio::spawn(async move {
        let dao = match TimerDao::new() {
            Ok(dao) => dao,
            Err(err) => {
                error!("init timer dao failed: {:?}", err);
                return;
            }
        };

        loop {
            if let Err(err) = execute_tasks(&dao, Local::now()).await {
                error!("timer tick failed: {:?}", err);
            }
            tokio::time::sleep(Duration::from_secs(PERIOD)).await;
        }
    });
}

pub async fn execute_tasks(dao: &TimerDao, now: DateTime<Local>) -> anyhow::Result<()> {
    let tasks = dao.list_due(now, DUE_TASK_LIMIT).await?;
    for task in tasks {
        let mut plan = Plan::new(task.prompt.clone(), Some(task.id.clone())).await?;
        let result = plan.execute().await?;

        let remaining_runs = task.remaining_runs.saturating_sub(1);
        let next_trigger_at = build_next_trigger_at(&task, now)?;
        let completed_at = now.to_rfc3339();

        dao.mark_run_completed(
            &task.id,
            remaining_runs,
            &next_trigger_at,
            &completed_at,
            &result,
        )
        .await?
    }

    Ok(())
}

fn build_next_trigger_at(task: &TimerTask, now: DateTime<Local>) -> anyhow::Result<String> {
    let cron = task.cron_expr.clone();
    let schedule = Schedule::from_str(&cron)?;
    let next = schedule
        .iter(&now)
        .find(|next| *next > now)
        .ok_or_else(|| anyhow!("build next trigger failed: {cron}"))?;
    Ok(next.to_rfc3339())
}
