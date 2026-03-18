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

        let mut freshness = None;
        if args.start_date.is_some() && args.end_date.is_some() {
            freshness = Some(args.start_date.unwrap() + ".." + &args.end_date.unwrap());
        }
        let body = SearchBody {
            query: args.keyword.to_string(),
            freshness,
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

#[derive(Serialize, Debug)]
pub struct Curl {
    pub base_tool: BaseTool,
}

#[derive(Deserialize)]
struct CurlArgs {
    url: String,
}

#[async_trait::async_trait]
impl LlmTool for Curl {
    fn group_name(&self) -> &str {
        &self.base_tool.group_name
    }

    fn deep_seek_schema(&self) -> llm::base_llm::Function {
        llm::base_llm::Function {
            name: self.base_tool.name.clone(),
            description: self.base_tool.description.clone(),
            parameters: Parameters::new(
                serde_json::json!({
                    "url": {
                            "type": "string",
                            "description": "web url"
                        }
                }),
                vec!["url".to_string()],
            ),
        }
    }

    async fn deep_seek_call(&self, tool_call: &ToolCall) -> String {
        let args: CurlArgs = match serde_json::from_str(&tool_call.function.arguments) {
            Ok(v) => v,
            Err(e) => {
                error!("failed to parse curl arguments: {:?}", e);
                return format!("Error: invalid arguments: {}", e);
            }
        };

        let raw = util::http_utils::get(&args.url).await;

        raw.unwrap_or_else(|e| {
            error!("failed to search web: {:?}", e);
            format!("Error: search web: {}", e)
        })
    }
}

impl Curl {
    pub fn new() -> Self {
        let base_tool = BaseTool {
            group_name: GROUP_NAME.to_string(),
            group_description: GROUP_DESC.to_string(),
            name: GROUP_NAME.to_string() + "_curl",
            description: "Curl url.".to_string(),
        };

        Curl { base_tool }
    }
}
