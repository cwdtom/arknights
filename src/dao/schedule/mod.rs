mod models;
mod queries;

use crate::dao::base_dao::BaseDao;
use anyhow::Context;
use queries::{
    create_with_conn, get_with_conn, list_by_range_with_conn, list_by_tag_with_conn,
    remove_with_conn, search_with_conn, update_with_conn,
};
use std::path::{Path, PathBuf};

pub use models::{NewScheduleEvent, ScheduleEvent, UpdateScheduleEvent};

#[derive(Clone)]
pub struct ScheduleDao {
    base: BaseDao,
}

impl ScheduleDao {
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

    pub async fn create(&self, event: &NewScheduleEvent) -> anyhow::Result<i64> {
        let event = event.clone();
        self.base
            .run_blocking(move |conn| create_with_conn(conn, &event))
            .await
    }

    pub async fn get(&self, id: i64) -> anyhow::Result<Option<ScheduleEvent>> {
        self.base
            .run_blocking(move |conn| get_with_conn(conn, id))
            .await
    }

    pub async fn list_by_range(
        &self,
        start: &str,
        end: &str,
    ) -> anyhow::Result<Vec<ScheduleEvent>> {
        let start = start.to_owned();
        let end = end.to_owned();
        self.base
            .run_blocking(move |conn| list_by_range_with_conn(conn, &start, &end))
            .await
    }

    pub async fn search(&self, keyword: &str) -> anyhow::Result<Vec<ScheduleEvent>> {
        let keyword = keyword.to_owned();
        self.base
            .run_blocking(move |conn| search_with_conn(conn, &keyword))
            .await
    }

    pub async fn list_by_tag(&self, tag: &str) -> anyhow::Result<Vec<ScheduleEvent>> {
        let tag = tag.to_owned();
        self.base
            .run_blocking(move |conn| list_by_tag_with_conn(conn, &tag))
            .await
    }

    pub async fn update(&self, event: &UpdateScheduleEvent) -> anyhow::Result<()> {
        let event = event.clone();
        self.base
            .run_blocking(move |conn| update_with_conn(conn, &event))
            .await
    }

    pub async fn remove(&self, id: i64) -> anyhow::Result<()> {
        self.base
            .run_blocking(move |conn| remove_with_conn(conn, id))
            .await
    }

    pub fn db_path(&self) -> &Path {
        self.base.db_path()
    }
}

fn init_schema(base: &BaseDao) -> anyhow::Result<()> {
    base.with_connection(|conn| {
        conn.execute(models::CREATE_TABLE_SQL, [])
            .context("create schedule_events table failed")?;
        Ok(())
    })?;
    Ok(())
}

#[cfg(test)]
#[path = "schedule_dao_tests.rs"]
mod tests;
