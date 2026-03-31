use super::should_suppress_chromiumoxide_invalid_message;
use tracing::Level;

#[test]
fn chromiumoxide_invalid_message_warn_is_suppressed() {
    assert!(should_suppress_chromiumoxide_invalid_message(
        "chromiumoxide::handler",
        &Level::WARN,
        "WS Invalid message: data did not match any variant of untagged enum Message",
    ));
}

#[test]
fn other_chromiumoxide_warnings_are_not_suppressed() {
    assert!(!should_suppress_chromiumoxide_invalid_message(
        "chromiumoxide::handler",
        &Level::WARN,
        "WS Connection error: boom",
    ));
}

#[test]
fn quoted_messages_are_also_matched() {
    assert!(should_suppress_chromiumoxide_invalid_message(
        "chromiumoxide::handler",
        &Level::WARN,
        "\"WS Invalid message: data did not match any variant of untagged enum Message\"",
    ));
}
