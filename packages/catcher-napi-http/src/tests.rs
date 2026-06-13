use std::collections::HashMap;

use catcher_core::types::sse::SseClientConfig;
use catcher_http::{
    types::http::{HttpClientConfig, HttpMethod, StreamEvent},
    HttpTransport, MetricsSnapshot,
};

use super::client::JsMetrics;
use super::helpers::{parse_method, stream_event_to_json};
use super::sse::{sse_end_json, sse_error_json, sse_line_json};

/// Verify parse_method works for all supported methods
#[test]
fn parse_method_all() {
    assert!(matches!(parse_method("GET").unwrap(), HttpMethod::GET));
    assert!(matches!(parse_method("get").unwrap(), HttpMethod::GET));
    assert!(matches!(parse_method("POST").unwrap(), HttpMethod::POST));
    assert!(matches!(parse_method("PUT").unwrap(), HttpMethod::PUT));
    assert!(matches!(
        parse_method("DELETE").unwrap(),
        HttpMethod::DELETE
    ));
    assert!(matches!(parse_method("PATCH").unwrap(), HttpMethod::PATCH));
    assert!(parse_method("INVALID").is_err());
}

/// Verify JsMetrics conversion from MetricsSnapshot
#[test]
fn metrics_snapshot_to_js() {
    let snap = MetricsSnapshot {
        http_requests: 100,
        http_success_rate: 0.95,
        http_avg_latency_us: 1500,
        http_retries: 5,
        ws_connect_success_rate: 1.0,
        ws_disconnects: 2,
        ws_messages_sent: 50,
        ws_messages_received: 48,
        cb_open_count: 1,
        queue_timeouts: 0,
    };
    let js: JsMetrics = snap.into();
    assert_eq!(js.http_requests, 100);
    assert_eq!(js.http_success_rate, 0.95);
    assert_eq!(js.http_avg_latency_us, 1500);
    assert_eq!(js.http_retries, 5);
    assert_eq!(js.cb_open_count, 1);
}

/// Verify HttpClientConfig parsing with minimal config
#[test]
fn http_config_minimal() {
    let json = r#"{}"#;
    let config: HttpClientConfig = serde_json::from_str(json).unwrap();
    assert!(config.base_url.is_empty());
}

/// Verify HttpClientConfig parsing with full config
#[test]
fn http_config_full() {
    let json = r#"{
            "base_url": "https://api.example.com",
            "connect_timeout_ms": 5000,
            "response_timeout_ms": 30000,
            "retry": { "max_attempts": 3, "backoff": "Exponential" },
            "circuit_breaker": { "failure_threshold": 5, "reset_timeout_ms": 30000 }
        }"#;
    let config: HttpClientConfig = serde_json::from_str(json).unwrap();
    assert_eq!(config.base_url, "https://api.example.com");
    assert_eq!(config.connect_timeout_ms, 5000);
    assert_eq!(config.response_timeout_ms, 30000);
}

/// Verify SseClientConfig parsing
#[test]
fn sse_config_parse() {
    let json = r#"{
            "url": "https://example.com/events",
            "method": "GET",
            "headers": {"Authorization": "Bearer test"},
            "timeout_ms": 10000,
            "reconnect": {
                "max_retries": 5,
                "initial_delay_ms": 500,
                "max_delay_ms": 30000,
                "backoff_multiplier": 2.0
            }
        }"#;
    let config: SseClientConfig = serde_json::from_str(json).unwrap();
    assert_eq!(config.url, "https://example.com/events");
    assert_eq!(config.timeout_ms, 10000);
    let rc = config.reconnect.unwrap();
    assert_eq!(rc.max_retries, 5);
}

/// Verify StreamEvent variant matching
#[test]
fn stream_event_variants() {
    let evt = StreamEvent::Headers {
        status: 200,
        headers: HashMap::from([("content-type".into(), "text/plain".into())]),
    };
    match &evt {
        StreamEvent::Headers { status, .. } => assert_eq!(*status, 200),
        _ => panic!("Expected Headers"),
    }

    let evt = StreamEvent::Chunk(bytes::Bytes::from(vec![1, 2, 3]));
    match &evt {
        StreamEvent::Chunk(data) => assert_eq!(&data[..], &[1, 2, 3]),
        _ => panic!("Expected Chunk"),
    }

    assert!(matches!(StreamEvent::Done, StreamEvent::Done));

    let evt = StreamEvent::Error("test error".into());
    match &evt {
        StreamEvent::Error(msg) => assert_eq!(msg, "test error"),
        _ => panic!("Expected Error"),
    }
}

/// Verify HTTP transport can be created and metrics is accessible
#[test]
fn http_transport_metrics_initial() {
    let config = HttpClientConfig::default();
    let transport = HttpTransport::new(config).unwrap();
    let metrics = transport.metrics();
    assert_eq!(metrics.http_requests, 0);
    assert_eq!(metrics.http_success_rate, 0.0);
    assert_eq!(metrics.http_retries, 0);
}

/// Verify cancel_all and cancel_request don't panic on fresh client
#[test]
fn http_cancel_on_fresh_client() {
    let config = HttpClientConfig::default();
    let transport = HttpTransport::new(config).unwrap();
    transport.cancel_all();
    assert!(!transport.cancel_request(999));
}

/// Verify adaptive timeout set/disable
#[test]
fn http_adaptive_timeout_set_disable() {
    let config = HttpClientConfig::default();
    let transport = HttpTransport::new(config).unwrap();
    transport.set_adaptive_timeout(1000, 30000, 2.0, 50);
    transport.disable_adaptive_timeout();
}

/// Verify next_request_id returns monotonically increasing values
#[test]
fn http_request_id_monotonic() {
    let config = HttpClientConfig::default();
    let transport = HttpTransport::new(config).unwrap();
    let id1 = transport.next_request_id();
    let id2 = transport.next_request_id();
    assert!(id2 > id1, "request IDs should be monotonically increasing");
}

/// Verify base64 encoding of StreamEvent::Chunk
#[test]
fn stream_chunk_base64_encoding() {
    use base64::Engine;
    let data = vec![0xDE, 0xAD, 0xBE, 0xEF];
    let b64 = base64::engine::general_purpose::STANDARD.encode(&data);
    assert_eq!(b64, "3q2+7w==");
}

/// Verify stream_event_to_json produces valid JSON for each variant
#[test]
fn stream_event_json_output() {
    let json = stream_event_to_json(&StreamEvent::Done);
    assert_eq!(json, "{\"type\":\"Done\"}");

    let json = stream_event_to_json(&StreamEvent::Error("oops".into()));
    assert!(json.contains("\"type\":\"Error\""));
    assert!(json.contains("oops"));

    let json = stream_event_to_json(&StreamEvent::Headers {
        status: 200,
        headers: HashMap::new(),
    });
    assert!(json.contains("\"status\":200"));
}

/// Verify SSE cancel channel works
#[test]
fn sse_cancel_channel() {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<()>();
    assert!(tx.send(()).is_ok());
    assert!(rx.try_recv().is_ok());
}

/// Verify SSE JSON helpers
#[test]
fn sse_json_helpers() {
    let line = sse_line_json("data: hello");
    assert!(line.contains("\"type\":\"Line\""));
    assert!(line.contains("data: hello"));

    let err = sse_error_json("timeout");
    assert!(err.contains("\"type\":\"Error\""));
    assert!(err.contains("timeout"));

    let end = sse_end_json();
    assert_eq!(end, "{\"type\":\"End\"}");
}
