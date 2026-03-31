use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Serialize)]
pub struct BrowserErrorBody<'a> {
    pub code: &'a str,
    pub message: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrowserToolError {
    pub code: String,
    pub message: String,
}

impl BrowserToolError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
}

pub type BrowserToolResult = Result<Value, BrowserToolError>;
pub type BrowserToolUnitResult = Result<(), BrowserToolError>;

pub fn browser_error_json(code: &str, message: &str) -> String {
    serde_json::json!({
        "ok": false,
        "error": BrowserErrorBody { code, message },
    })
    .to_string()
}

pub fn browser_ok_json(result: Value) -> String {
    serde_json::json!({
        "ok": true,
        "result": result,
    })
    .to_string()
}

pub fn browser_tool_error_json(error: &BrowserToolError) -> String {
    browser_error_json(&error.code, &error.message)
}

pub fn browser_tool_result_json(result: BrowserToolResult) -> String {
    match result {
        Ok(value) => browser_ok_json(value),
        Err(error) => browser_tool_error_json(&error),
    }
}

pub fn browser_tool_unit_result_json(result: BrowserToolUnitResult) -> String {
    match result {
        Ok(()) => browser_ok_json(serde_json::json!({})),
        Err(error) => browser_tool_error_json(&error),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        BrowserToolError, BrowserToolResult, BrowserToolUnitResult, browser_error_json,
        browser_ok_json, browser_tool_error_json, browser_tool_result_json,
        browser_tool_unit_result_json,
    };
    use serde_json::Value;

    #[test]
    fn stale_element_error_uses_stable_error_code() {
        let raw = browser_error_json("element_id_stale", "call browser_snapshot again");
        let value: Value = serde_json::from_str(&raw).unwrap();

        assert_eq!(value["ok"], false);
        assert_eq!(value["error"]["code"], "element_id_stale");
    }

    #[test]
    fn browser_ok_wraps_payload_under_result() {
        let raw = browser_ok_json(serde_json::json!({ "url": "https://example.com" }));
        let value: Value = serde_json::from_str(&raw).unwrap();

        assert_eq!(value["ok"], true);
        assert_eq!(value["result"]["url"], "https://example.com");
    }

    #[test]
    fn browser_tool_error_json_keeps_stable_shape() {
        let err = BrowserToolError::new("element_id_stale", "call browser_snapshot again");
        let raw = browser_tool_error_json(&err);
        let value: Value = serde_json::from_str(&raw).unwrap();

        assert_eq!(value["ok"], false);
        assert_eq!(value["error"]["code"], "element_id_stale");
        assert_eq!(value["error"]["message"], "call browser_snapshot again");
    }

    #[test]
    fn browser_tool_result_json_preserves_ok_and_error_shapes() {
        let ok: BrowserToolResult = Ok(serde_json::json!({ "url": "https://example.com" }));
        let ok_raw = browser_tool_result_json(ok);
        let ok_value: Value = serde_json::from_str(&ok_raw).unwrap();
        assert_eq!(ok_value["ok"], true);
        assert_eq!(ok_value["result"]["url"], "https://example.com");

        let err: BrowserToolResult = Err(BrowserToolError::new(
            "element_id_stale",
            "call browser_snapshot again",
        ));
        let err_raw = browser_tool_result_json(err);
        let err_value: Value = serde_json::from_str(&err_raw).unwrap();
        assert_eq!(err_value["ok"], false);
        assert_eq!(err_value["error"]["code"], "element_id_stale");
        assert_eq!(err_value["error"]["message"], "call browser_snapshot again");
    }

    #[test]
    fn browser_tool_unit_result_json_success_wraps_empty_object() {
        let ok: BrowserToolUnitResult = Ok(());
        let raw = browser_tool_unit_result_json(ok);
        let value: Value = serde_json::from_str(&raw).unwrap();

        assert_eq!(value["ok"], true);
        assert_eq!(value["result"], serde_json::json!({}));
    }

    #[test]
    fn browser_tool_unit_result_json_error_reuses_stable_error_shape() {
        let err: BrowserToolUnitResult = Err(BrowserToolError::new(
            "session_not_found",
            "browser session already closed",
        ));
        let raw = browser_tool_unit_result_json(err);
        let value: Value = serde_json::from_str(&raw).unwrap();

        assert_eq!(value["ok"], false);
        assert_eq!(value["error"]["code"], "session_not_found");
        assert_eq!(value["error"]["message"], "browser session already closed");
    }
}
