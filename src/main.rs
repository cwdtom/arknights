use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

pub mod agent;
pub mod llm;
pub mod tool;
pub mod util;

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();

    let file_appender = tracing_appender::rolling::daily("logs", "arknights.log");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(tracing_subscriber::fmt::layer().with_ansi(false).with_writer(non_blocking))
        .init();

    let mut plan = agent::plan::Plan::new("计算当前系统时间2天后的时间".to_string())
        .await
        .expect("plan初始化出错");
    plan.execute().await.expect("plan执行出错");
}
