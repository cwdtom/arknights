use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt::time::LocalTime;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

pub mod agent;
mod command;
pub mod dao;
pub mod im;
pub mod kv;
pub mod llm;
pub mod memory;
pub mod schedule;
#[cfg(test)]
pub(crate) mod test_support;
pub mod timer;
pub mod tool;
pub mod util;

#[tokio::main]
async fn main() {
    dotenvy::dotenv_override().expect("load .env failed");

    let file_appender = tracing_appender::rolling::daily("logs", "arknights.log");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

    let timer = LocalTime::rfc_3339();

    tracing_subscriber::registry()
        .with(EnvFilter::new("info"))
        .with(tracing_subscriber::fmt::layer().with_timer(timer.clone()))
        .with(
            tracing_subscriber::fmt::layer()
                .with_ansi(false)
                .with_timer(timer)
                .with_writer(non_blocking),
        )
        .init();

    // lark init
    im::base_im::init_lark();

    // timer init
    timer::timer_service::init_timer();

    // lark wss build and auto reconnect
    loop {
        match im::lark::build_wss().await {
            Ok(_) => tracing::warn!("lark websocket exited without error, reconnecting."),
            Err(err) => tracing::error!("lark websocket failed: {:?}, reconnecting.", err),
        }

        tokio::time::sleep(std::time::Duration::from_secs(10)).await;
    }
}
