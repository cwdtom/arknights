use std::fmt;
use tracing::field::{Field, Visit};
use tracing::{Event, Level, Metadata};
use tracing_subscriber::EnvFilter;
use tracing_subscriber::Layer;
use tracing_subscriber::fmt::time::LocalTime;
use tracing_subscriber::layer::{Context, Filter, SubscriberExt};
use tracing_subscriber::util::SubscriberInitExt;

pub mod agent;
mod command;
pub mod dao;
pub mod im;
pub mod kv;
pub mod llm;
#[cfg(test)]
mod main_tests;
pub mod memory;
pub mod schedule;
#[cfg(test)]
pub(crate) mod test_support;
pub mod timer;
pub mod tool;
pub mod util;

const CHROMIUMOXIDE_HANDLER_TARGET: &str = "chromiumoxide::handler";
const CHROMIUMOXIDE_INVALID_MESSAGE_PREFIX: &str = "WS Invalid message:";

#[derive(Clone, Debug, Default)]
struct SuppressChromiumoxideInvalidMessageFilter;

impl<S> Filter<S> for SuppressChromiumoxideInvalidMessageFilter {
    fn enabled(&self, _meta: &Metadata<'_>, _cx: &Context<'_, S>) -> bool {
        true
    }

    fn event_enabled(&self, event: &Event<'_>, _cx: &Context<'_, S>) -> bool {
        let message = event_message(event);
        let should_suppress = message.as_deref().is_some_and(|message| {
            should_suppress_chromiumoxide_invalid_message(
                event.metadata().target(),
                event.metadata().level(),
                message,
            )
        });
        !should_suppress
    }
}

fn event_message(event: &Event<'_>) -> Option<String> {
    let mut visitor = MessageVisitor::default();
    event.record(&mut visitor);
    visitor.message
}

fn should_suppress_chromiumoxide_invalid_message(
    target: &str,
    level: &Level,
    message: &str,
) -> bool {
    target == CHROMIUMOXIDE_HANDLER_TARGET
        && *level == Level::WARN
        && message
            .trim_matches('"')
            .starts_with(CHROMIUMOXIDE_INVALID_MESSAGE_PREFIX)
}

#[derive(Default)]
struct MessageVisitor {
    message: Option<String>,
}

impl Visit for MessageVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        if field.name() == "message" {
            self.message = Some(format!("{value:?}"));
        }
    }
}

#[tokio::main]
async fn main() {
    dotenvy::dotenv_override().expect("load .env failed");

    let file_appender = tracing_appender::rolling::daily("logs", "arknights.log");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

    let timer = LocalTime::rfc_3339();
    // Suppress only chromiumoxide's noisy invalid-message warning so real browser
    // warnings and errors still surface in both stdout and file logs.
    let stdout_layer = tracing_subscriber::fmt::layer()
        .with_timer(timer.clone())
        .with_filter(SuppressChromiumoxideInvalidMessageFilter);
    let file_layer = tracing_subscriber::fmt::layer()
        .with_ansi(false)
        .with_timer(timer)
        .with_writer(non_blocking)
        .with_filter(SuppressChromiumoxideInvalidMessageFilter);

    tracing_subscriber::registry()
        .with(EnvFilter::new("info"))
        .with(stdout_layer)
        .with(file_layer)
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
