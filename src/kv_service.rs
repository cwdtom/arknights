use crate::dao::kv_dao::KvDao;
use anyhow::anyhow;
#[cfg(test)]
use std::path::PathBuf;
use std::sync::LazyLock;

#[cfg(not(test))]
static KV_DAO: LazyLock<anyhow::Result<KvDao>> = LazyLock::new(KvDao::new);
#[cfg(test)]
static KV_DAO: LazyLock<anyhow::Result<KvDao>> =
    LazyLock::new(|| KvDao::with_path(test_db_path()));

const PERSONAL_KEY: &str = "PERSONAL";

#[cfg(test)]
static TEST_DB_PATH: LazyLock<PathBuf> = LazyLock::new(|| {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();

    std::env::temp_dir().join(format!("arknights_kv_service_{nanos}.db"))
});

fn kv_dao() -> anyhow::Result<&'static KvDao> {
    KV_DAO.as_ref().map_err(|err| anyhow!("{err:#}"))
}

#[cfg(test)]
fn test_db_path() -> &'static PathBuf {
    &TEST_DB_PATH
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
pub(crate) async fn clear_personal_value_for_test() -> anyhow::Result<()> {
    let dao = kv_dao()?;

    if dao.get(PERSONAL_KEY).await?.is_some() {
        dao.delete(PERSONAL_KEY).await?;
    }

    Ok(())
}

#[cfg(test)]
#[path = "kv_service_tests.rs"]
mod tests;
