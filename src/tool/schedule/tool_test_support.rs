use crate::llm::base_llm::{FunctionCall, ToolCall};

pub(super) fn tool_call(name: &str, arguments: &str) -> ToolCall {
    ToolCall {
        id: format!("call_{name}"),
        r#type: "function".to_string(),
        function: FunctionCall {
            name: name.to_string(),
            arguments: arguments.to_string(),
        },
    }
}
