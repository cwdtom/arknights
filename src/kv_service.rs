use crate::dao::kv_dao::KvDao;
use anyhow::anyhow;
use std::sync::LazyLock;

static KV_DAO: LazyLock<anyhow::Result<KvDao>> = LazyLock::new(KvDao::new);

const PERSONAL_KEY: &str = "PERSONAL";
const USER_PROFILE: &str = "USER_PROFILE";

fn kv_dao() -> anyhow::Result<&'static KvDao> {
    KV_DAO.as_ref().map_err(|err| anyhow!("{err:#}"))
}

pub async fn get_personal_value() -> anyhow::Result<String> {
    get_value(PERSONAL_KEY).await
}

pub async fn set_personal_value(value: &str) -> anyhow::Result<()> {
    let dao = kv_dao()?;
    dao.save(PERSONAL_KEY, value).await
}

pub async fn get_user_profile() -> anyhow::Result<String> {
    match get_value(USER_PROFILE).await {
        Ok(value) => Ok(value),
        // not found means no profile
        Err(_) => Ok("".to_string()),
    }
}

pub async fn set_user_profile(value: &str) -> anyhow::Result<()> {
    let dao = kv_dao()?;
    dao.save(USER_PROFILE, value).await
}

async fn get_value(key: &str) -> anyhow::Result<String> {
    let dao = kv_dao()?;
    let kv = dao.get(key).await?;

    match kv {
        Some(kv) => Ok(kv.value),
        None => Err(anyhow!("key not found")),
    }
}

#[cfg(test)]
#[path = "kv_service_tests.rs"]
mod tests;
