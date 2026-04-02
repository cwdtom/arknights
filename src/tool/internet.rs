use crate::llm::base_llm::{Parameters, ToolCall};
use crate::tool::base_tool::{BaseTool, LlmTool};
use crate::{llm, util};
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;
use tracing::error;

const GROUP_NAME: &str = "internet";
const GROUP_DESC: &str = "Get internet info.";
const BASE_URL: &str = "https://api.bocha.cn/v1/web-search";
static BOCHA_API_KEY: LazyLock<String> =
    LazyLock::new(|| std::env::var("BOCHA_API_KEY").expect("BOCHA_API_KEY not set"));

#[derive(Serialize, Debug)]
pub struct Search {
    pub base_tool: BaseTool,
}

#[derive(Deserialize)]
struct SearchArgs {
    keyword: String,
    // yyyy-MM-dd
    start_date: Option<String>,
    // yyyy-MM-dd
    end_date: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct SearchBody {
    query: String,
    // search date range yyyy-MM-dd..yyyy-MM-dd
    #[serde(skip_serializing_if = "Option::is_none")]
    freshness: Option<String>,
    // show web summary
    summary: bool,
}

#[async_trait::async_trait]
impl LlmTool for Search {
    fn group_name(&self) -> &str {
        &self.base_tool.group_name
    }

    fn name(&self) -> &str {
        self.base_tool.name.as_str()
    }

    fn deep_seek_schema(&self) -> llm::base_llm::Function {
        llm::base_llm::Function {
            name: self.base_tool.name.clone(),
            description: self.base_tool.description.clone(),
            parameters: Parameters::new(
                serde_json::json!({
                    "keyword": {
                            "type": "string",
                            "description": "Search keyword"
                        },
                    "start_date": {
                            "type": "string",
                            "description": "start date, format: yyyy-MM-dd"
                        },
                    "end_date": {
                            "type": "string",
                            "description": "end date, format: yyyy-MM-dd"
                        }
                }),
                vec!["keyword".to_string()],
            ),
        }
    }

    async fn deep_seek_call(&self, tool_call: &ToolCall) -> String {
        let args: SearchArgs = match serde_json::from_str(&tool_call.function.arguments) {
            Ok(v) => v,
            Err(e) => {
                error!("failed to parse search arguments: {:?}", e);
                return format!("Error: invalid arguments: {}", e);
            }
        };

        let body = SearchBody {
            query: args.keyword.to_string(),
            freshness: build_freshness(args.start_date.as_deref(), args.end_date.as_deref()),
            summary: true,
        };

        let raw = util::http_utils::post(BASE_URL, &BOCHA_API_KEY, &body).await;

        raw.unwrap_or_else(|e| {
            error!("failed to search web: {:?}", e);
            format!("Error: search web: {}", e)
        })
    }
}

impl Search {
    pub fn new() -> Self {
        let base_tool = BaseTool {
            group_name: GROUP_NAME.to_string(),
            group_description: GROUP_DESC.to_string(),
            name: GROUP_NAME.to_string() + "_search",
            description: "Search internet by keyword.".to_string(),
        };

        Search { base_tool }
    }
}

fn build_freshness(start_date: Option<&str>, end_date: Option<&str>) -> Option<String> {
    match (start_date, end_date) {
        (Some(start_date), Some(end_date)) => Some(format!("{start_date}..{end_date}")),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::base_llm::{FunctionCall, ToolCall};

    #[test]
    fn search_tool_schema_has_keyword_and_date_fields() {
        let tool = Search::new();
        let schema = tool.deep_seek_schema();

        assert_eq!(schema.name, "internet_search");
        assert_eq!(schema.description, "Search internet by keyword.");
        assert_eq!(schema.parameters.required, vec!["keyword".to_string()]);
        assert!(schema.parameters.properties["keyword"].is_object());
        assert!(schema.parameters.properties["start_date"].is_object());
        assert!(schema.parameters.properties["end_date"].is_object());
    }

    #[tokio::test]
    async fn search_tool_returns_parse_error_for_invalid_arguments() {
        let tool = Search::new();
        let tool_call = ToolCall {
            id: "call_search".to_string(),
            r#type: "function".to_string(),
            function: FunctionCall {
                name: "internet_search".to_string(),
                arguments: "{".to_string(),
            },
        };

        let result = tool.deep_seek_call(&tool_call).await;
        assert!(result.starts_with("Error: invalid arguments:"));
    }

    #[test]
    fn build_freshness_returns_range_when_both_dates_exist() {
        let freshness = build_freshness(Some("2026-03-01"), Some("2026-03-19"));
        assert_eq!(freshness, Some("2026-03-01..2026-03-19".to_string()));
    }

    #[test]
    fn build_freshness_returns_none_when_date_is_missing() {
        assert_eq!(build_freshness(Some("2026-03-01"), None), None);
        assert_eq!(build_freshness(None, Some("2026-03-19")), None);
        assert_eq!(build_freshness(None, None), None);
    }
}
