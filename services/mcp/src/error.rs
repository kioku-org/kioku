use rmcp::model::{CallToolResult, Content, JsonObject};
use serde_json::Value;
use std::sync::Arc;

pub fn schema(value: Value) -> Arc<JsonObject> {
    match value {
        Value::Object(map) => Arc::new(map),
        _ => Arc::new(JsonObject::new()),
    }
}

pub fn text_result(msg: impl Into<String>) -> CallToolResult {
    CallToolResult::success(vec![Content::text(msg.into())])
}

pub fn error_result(msg: impl Into<String>) -> CallToolResult {
    CallToolResult::error(vec![Content::text(msg.into())])
}

pub fn to_result(r: Result<Value, String>) -> CallToolResult {
    match r {
        Ok(v) => text_result(serde_json::to_string_pretty(&v).unwrap_or_default()),
        Err(e) => error_result(e),
    }
}
