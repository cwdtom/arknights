mod models;
mod queries;

use crate::dao::base_dao::BaseDao;
use anyhow::Context;
use chrono::{DateTime, Local};
use queries::{
    cancel_with_conn, create_with_conn, get_with_conn, list_active_with_conn, list_due_with_conn,
    list_with_conn, mark_run_completed_with_conn, remove_with_conn, update_with_conn,
};
use std::path::{Path, PathBuf};

pub use models::{NewTimerTask, TimerTask, UpdateTimerTask};

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

    pub async fn list(&self) -> anyhow::Result<Vec<TimerTask>> {
        self.base.run_blocking(list_with_conn).await
    }

    pub async fn list_active(&self) -> anyhow::Result<Vec<TimerTask>> {
        self.base.run_blocking(list_active_with_conn).await
    }

    pub async fn update(&self, task: &UpdateTimerTask) -> anyhow::Result<()> {
        let task = task.clone();
        self.base
            .run_blocking(move |conn| update_with_conn(conn, &task))
            .await
    }

    pub async fn cancel(&self, id: &str) -> anyhow::Result<()> {
        let id = id.to_owned();
        self.base
            .run_blocking(move |conn| cancel_with_conn(conn, &id))
            .await
    }

    pub async fn remove(&self, id: &str) -> anyhow::Result<()> {
        let id = id.to_owned();
        self.base
            .run_blocking(move |conn| remove_with_conn(conn, &id))
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
}

fn init_schema(base: &BaseDao) -> anyhow::Result<()> {
    base.with_connection(|conn| {
        conn.execute(models::CREATE_TABLE_SQL, [])
            .context("create timer_tasks table failed")?;
        Ok(())
    })?;
    Ok(())
}

#[cfg(test)]
#[path = "timer_dao_tests.rs"]
mod tests;
