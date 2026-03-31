use serde::Serialize;
use serde_json::Value;

#[derive(Serialize)]
pub struct BrowserErrorBody<'a> {
    pub code: &'a str,
    pub message: &'a str,
}

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

#[cfg(test)]
mod tests {
    use super::{browser_error_json, browser_ok_json};
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
}
