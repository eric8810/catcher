use catcher_core::{CatcherError, ErrorCategory};
use serde::Serialize;

pub(crate) const NATIVE_ERROR_PREFIX: &str = "CATCHER_ERROR:";

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct NativeErrorDetails {
    #[serde(skip_serializing_if = "Option::is_none")]
    status: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    body: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    timeout_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    host: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    attempts: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_error: Option<Box<NativeErrorPayload>>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct NativeErrorPayload {
    code: &'static str,
    phase: &'static str,
    retryable: bool,
    message: String,
    details: NativeErrorDetails,
}

impl From<&CatcherError> for NativeErrorPayload {
    fn from(error: &CatcherError) -> Self {
        let retryable = error.category() == ErrorCategory::Retryable;
        let (code, phase, details) = match error {
            CatcherError::ConnectionTimeout(timeout_ms) => (
                "CONNECTION_TIMEOUT",
                "connect",
                NativeErrorDetails {
                    timeout_ms: Some(*timeout_ms),
                    ..Default::default()
                },
            ),
            CatcherError::RequestTimeout(timeout_ms) => (
                "REQUEST_TIMEOUT",
                "request",
                NativeErrorDetails {
                    timeout_ms: Some(*timeout_ms),
                    ..Default::default()
                },
            ),
            CatcherError::TlsError(reason) => (
                "TLS_ERROR",
                "tls",
                NativeErrorDetails {
                    reason: Some(reason.clone()),
                    ..Default::default()
                },
            ),
            CatcherError::DnsError { host, reason } => (
                "DNS_ERROR",
                "dns",
                NativeErrorDetails {
                    host: Some(host.clone()),
                    reason: Some(reason.clone()),
                    ..Default::default()
                },
            ),
            CatcherError::ConnectionError { host, reason } => (
                "CONNECTION_ERROR",
                "connect",
                NativeErrorDetails {
                    host: Some(host.clone()),
                    reason: Some(reason.clone()),
                    ..Default::default()
                },
            ),
            CatcherError::TransportError(reason) => (
                "TRANSPORT_ERROR",
                "request",
                NativeErrorDetails {
                    reason: Some(reason.clone()),
                    ..Default::default()
                },
            ),
            CatcherError::HttpError { status, body } => (
                "HTTP_ERROR",
                "response",
                NativeErrorDetails {
                    status: Some(*status),
                    body: Some(body.clone()),
                    ..Default::default()
                },
            ),
            CatcherError::WsHandshakeTimeout(timeout_ms) => (
                "WS_HANDSHAKE_TIMEOUT",
                "connect",
                NativeErrorDetails {
                    timeout_ms: Some(*timeout_ms),
                    ..Default::default()
                },
            ),
            CatcherError::WsDisconnected { code, reason } => (
                "WS_DISCONNECTED",
                "request",
                NativeErrorDetails {
                    status: Some(*code),
                    reason: Some(reason.clone()),
                    ..Default::default()
                },
            ),
            CatcherError::WsAllEndpointsFailed { count } => (
                "WS_ALL_ENDPOINTS_FAILED",
                "connect",
                NativeErrorDetails {
                    attempts: u32::try_from(*count).ok(),
                    ..Default::default()
                },
            ),
            CatcherError::RetryExhausted {
                attempts,
                last_error,
            } => (
                "RETRY_EXHAUSTED",
                "request",
                NativeErrorDetails {
                    attempts: Some(*attempts),
                    last_error: Some(Box::new(NativeErrorPayload::from(last_error.as_ref()))),
                    ..Default::default()
                },
            ),
            CatcherError::CircuitBreakerOpen => (
                "CIRCUIT_BREAKER_OPEN",
                "request",
                NativeErrorDetails::default(),
            ),
            CatcherError::QueueTimeout(timeout_ms) => (
                "QUEUE_TIMEOUT",
                "queue",
                NativeErrorDetails {
                    timeout_ms: Some(*timeout_ms),
                    ..Default::default()
                },
            ),
            CatcherError::EncodeError(reason) => (
                "ENCODE_ERROR",
                "encode",
                NativeErrorDetails {
                    reason: Some(reason.clone()),
                    ..Default::default()
                },
            ),
            CatcherError::DecodeError(reason) => (
                "DECODE_ERROR",
                "decode",
                NativeErrorDetails {
                    reason: Some(reason.clone()),
                    ..Default::default()
                },
            ),
            CatcherError::InvalidConfig(reason) => (
                "INVALID_CONFIG",
                "config",
                NativeErrorDetails {
                    reason: Some(reason.clone()),
                    ..Default::default()
                },
            ),
            CatcherError::SseTimeout(timeout_ms) => (
                "SSE_TIMEOUT",
                "request",
                NativeErrorDetails {
                    timeout_ms: Some(*timeout_ms),
                    ..Default::default()
                },
            ),
            CatcherError::Internal(reason) => (
                "INTERNAL_ERROR",
                "internal",
                NativeErrorDetails {
                    reason: Some(reason.clone()),
                    ..Default::default()
                },
            ),
        };

        Self {
            code,
            phase,
            retryable,
            message: error.to_string(),
            details,
        }
    }
}

pub(crate) fn to_napi_error(error: CatcherError) -> napi::Error {
    let payload = NativeErrorPayload::from(&error);
    match serde_json::to_string(&payload) {
        Ok(json) => napi::Error::from_reason(format!("{NATIVE_ERROR_PREFIX}{json}")),
        Err(serialize_error) => napi::Error::from_reason(format!(
            "failed to serialize catcher error: {serialize_error}; original error: {error}"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retry_exhausted_payload_preserves_attempts_and_last_error() {
        let payload = NativeErrorPayload::from(&CatcherError::RetryExhausted {
            attempts: 2,
            last_error: Box::new(CatcherError::ConnectionError {
                host: "api.example.com".into(),
                reason: "socket closed".into(),
            }),
        });
        let value = serde_json::to_value(payload).unwrap();

        assert_eq!(value["code"], "RETRY_EXHAUSTED");
        assert_eq!(value["phase"], "request");
        assert_eq!(value["details"]["attempts"], 2);
        assert_eq!(value["details"]["lastError"]["code"], "CONNECTION_ERROR");
        assert_eq!(value["details"]["lastError"]["phase"], "connect");
        assert_eq!(value["details"]["lastError"]["retryable"], true);
        assert_eq!(
            value["details"]["lastError"]["details"]["reason"],
            "socket closed"
        );
    }

    #[test]
    fn http_payload_preserves_status_and_body() {
        let payload = NativeErrorPayload::from(&CatcherError::HttpError {
            status: 421,
            body: "misdirected".into(),
        });
        let value = serde_json::to_value(payload).unwrap();

        assert_eq!(value["code"], "HTTP_ERROR");
        assert_eq!(value["details"]["status"], 421);
        assert_eq!(value["details"]["body"], "misdirected");
    }
}
