use napi::threadsafe_function::{ErrorStrategy, ThreadsafeFunction};
use catcher_http::types::http::{HttpMethod, StreamEvent};

pub(crate) type Tsfn = ThreadsafeFunction<String, ErrorStrategy::CalleeHandled>;

pub(crate) fn parse_method(s: &str) -> napi::Result<HttpMethod> {
    match s.to_uppercase().as_str() {
        "GET" => Ok(HttpMethod::GET),
        "POST" => Ok(HttpMethod::POST),
        "PUT" => Ok(HttpMethod::PUT),
        "DELETE" => Ok(HttpMethod::DELETE),
        "PATCH" => Ok(HttpMethod::PATCH),
        other => Err(napi::Error::from_reason(format!(
            "Unknown HTTP method: {other}"
        ))),
    }
}

pub(crate) fn stream_event_to_json(event: &StreamEvent) -> String {
    match event {
        StreamEvent::Headers { status, headers } => serde_json::json!({
            "type": "Headers",
            "status": status,
            "headers": headers,
        })
        .to_string(),
        StreamEvent::Chunk(data) => {
            use base64::Engine;
            serde_json::json!({
                "type": "Chunk",
                "data": base64::engine::general_purpose::STANDARD.encode(data),
            })
            .to_string()
        }
        StreamEvent::Done => serde_json::json!({"type": "Done"}).to_string(),
        StreamEvent::Error(msg) => serde_json::json!({
            "type": "Error",
            "message": msg,
        })
        .to_string(),
    }
}
