use crate::dao::kv_dao::KvDao;
use anyhow::anyhow;
use std::sync::LazyLock;

static KV_DAO: LazyLock<anyhow::Result<KvDao>> = LazyLock::new(KvDao::new);

const PERSONAL_KEY: &str = "PERSONAL";

fn kv_dao() -> anyhow::Result<&'static KvDao> {
    KV_DAO.as_ref().map_err(|err| anyhow!("{err:#}"))
}

pub async fn get_personal_value() -> anyhow::Result<String> {
    let dao = kv_dao()?;
    let kv = dao.get(PERSONAL_KEY).await?;

    match kv {
        Some(kv) => Ok(kv.value),
        None => Err(anyhow!("key not found")),
    }
}

pub async fn set_personal_value(value: &str) -> anyhow::Result<()> {
    let dao = kv_dao()?;
    dao.save(PERSONAL_KEY, value).await
}

#[cfg(test)]
#[path = "kv_service_tests.rs"]
mod tests;
