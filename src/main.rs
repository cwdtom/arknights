use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;

pub mod agent;
pub mod llm;
pub mod tool;
pub mod util;
pub mod im;

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();

    let file_appender = tracing_appender::rolling::daily("logs", "arknights.log");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

    tracing_subscriber::registry()
        .with(EnvFilter::new("info"))
        .with(tracing_subscriber::fmt::layer())
        .with(tracing_subscriber::fmt::layer().with_ansi(false).with_writer(non_blocking))
        .init();

    im::lark::send("send test".to_string()).await.expect("sending test error");

    // lark wss
    im::lark::build_wss().await.expect("building wss error");

    tokio::signal::ctrl_c().await.expect("signaling error");
}
