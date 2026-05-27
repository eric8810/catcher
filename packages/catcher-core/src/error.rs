use thiserror::Error;

/// 所有 catcher 操作返回的统一错误类型
#[derive(Error, Debug, Clone)]
pub enum CatcherError {
    #[error("connection timeout after {0}ms")]
    ConnectionTimeout(u64),

    #[error("request timeout after {0}ms")]
    RequestTimeout(u64),

    #[error("TLS error: {0}")]
    TlsError(String),

    #[error("DNS resolution failed for {host}: {reason}")]
    DnsError { host: String, reason: String },

    #[error("HTTP error: status {status}, body: {body}")]
    HttpError { status: u16, body: String },

    #[error("WS handshake timeout after {0}ms")]
    WsHandshakeTimeout(u64),

    #[error("WS disconnected: code={code}, reason={reason}")]
    WsDisconnected { code: u16, reason: String },

    #[error("all WS endpoints failed ({count} attempted)")]
    WsAllEndpointsFailed { count: usize },

    #[error("retry exhausted after {attempts} attempts: {last_error}")]
    RetryExhausted { attempts: u32, last_error: String },

    #[error("circuit breaker is OPEN, request rejected")]
    CircuitBreakerOpen,

    #[error("queue timeout after {0}ms")]
    QueueTimeout(u64),

    #[error("msgpack encode error: {0}")]
    EncodeError(String),

    #[error("msgpack decode error: {0}")]
    DecodeError(String),

    #[error("invalid configuration: {0}")]
    InvalidConfig(String),

    #[error("SSE stream timeout after {0}ms")]
    SseTimeout(u64),

    #[error("internal error: {0}")]
    Internal(String),
}

/// 错误分类：区分可重试和不可重试错误
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCategory {
    Retryable,
    NonRetryable,
}

impl CatcherError {
    /// 判断此错误是否可重试
    ///
    /// - 网络层错误（超时/DNS瞬态/TLS握手/断开）→ 可重试
    /// - 5xx HTTP 错误 → 可重试
    /// - HTTP 408 Request Timeout → 可重试（RFC 9110 §15.5.7: MAY retry on new connection）
    /// - HTTP 429 Too Many Requests → 可重试（遵循 Retry-After）
    /// - 其他 4xx HTTP 错误 → 不可重试（客户端错误重试无意义）
    /// - DNS NXDOMAIN（域名不存在）→ 不可重试（永久性错误）
    /// - TLS 证书错误 → 不可重试（配置错误，重试无法修复）
    /// - 编解码错误 → 不可重试（重试也不会变正确）
    /// - 熔断器打开 → 可重试（等恢复后重试）
    /// - 配置/内部错误 → 不可重试（需修复）
    pub fn category(&self) -> ErrorCategory {
        match self {
            CatcherError::ConnectionTimeout(_)
            | CatcherError::RequestTimeout(_)
            | CatcherError::WsHandshakeTimeout(_)
            | CatcherError::WsDisconnected { .. }
            | CatcherError::WsAllEndpointsFailed { .. }
            | CatcherError::CircuitBreakerOpen => ErrorCategory::Retryable,

            // ── TLS: 区分握手超时（可重试）和证书错误（不可重试）──
            CatcherError::TlsError(msg) => {
                let msg_lower = msg.to_lowercase();
                if msg_lower.contains("certificate")
                    || msg_lower.contains("cert")
                    || msg_lower.contains("expired")
                    || msg_lower.contains("mismatch")
                    || msg_lower.contains("unknown issuer")
                    || msg_lower.contains("self signed")
                {
                    ErrorCategory::NonRetryable
                } else {
                    ErrorCategory::Retryable
                }
            }

            // ── DNS: 区分 NXDOMAIN（不可重试）和 SERVFAIL/超时（可重试）──
            CatcherError::DnsError { reason, .. } => {
                if reason.to_lowercase().contains("nxdomain") {
                    ErrorCategory::NonRetryable
                } else {
                    ErrorCategory::Retryable
                }
            }

            // ── HTTP: 408/429/5xx 可重试，其他 4xx 不可重试 ──
            CatcherError::HttpError { status, .. } => {
                if *status == 408 || *status == 429 || *status >= 500 {
                    ErrorCategory::Retryable
                } else {
                    ErrorCategory::NonRetryable
                }
            }

            CatcherError::RetryExhausted { .. } => ErrorCategory::NonRetryable,

            CatcherError::EncodeError(_)
            | CatcherError::DecodeError(_)
            | CatcherError::InvalidConfig(_)
            | CatcherError::Internal(_)
            | CatcherError::QueueTimeout(_)
            | CatcherError::SseTimeout(_) => ErrorCategory::NonRetryable,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connection_timeout_is_retryable() {
        let err = CatcherError::ConnectionTimeout(5000);
        assert_eq!(err.category(), ErrorCategory::Retryable);
        assert!(err.to_string().contains("5000"));
    }

    #[test]
    fn request_timeout_is_retryable() {
        let err = CatcherError::RequestTimeout(30000);
        assert_eq!(err.category(), ErrorCategory::Retryable);
    }

    #[test]
    fn tls_handshake_error_is_retryable() {
        let err = CatcherError::TlsError("handshake timed out".into());
        assert_eq!(err.category(), ErrorCategory::Retryable);
    }

    #[test]
    fn tls_cert_error_is_non_retryable() {
        // Self-signed → permanent config error, retry won't help
        let err = CatcherError::TlsError("self signed certificate".into());
        assert_eq!(err.category(), ErrorCategory::NonRetryable);
    }

    #[test]
    fn tls_expired_cert_is_non_retryable() {
        let err = CatcherError::TlsError("certificate expired".into());
        assert_eq!(err.category(), ErrorCategory::NonRetryable);
    }

    #[test]
    fn tls_hostname_mismatch_is_non_retryable() {
        let err = CatcherError::TlsError("hostname mismatch in certificate".into());
        assert_eq!(err.category(), ErrorCategory::NonRetryable);
    }

    #[test]
    fn dns_error_is_retryable_for_servfail() {
        let err = CatcherError::DnsError {
            host: "example.com".into(),
            reason: "SERVFAIL".into(),
        };
        assert_eq!(err.category(), ErrorCategory::Retryable);
    }

    #[test]
    fn dns_nxdomain_is_non_retryable() {
        let err = CatcherError::DnsError {
            host: "example.com".into(),
            reason: "NXDOMAIN".into(),
        };
        assert_eq!(err.category(), ErrorCategory::NonRetryable);
    }

    #[test]
    fn http_5xx_is_retryable() {
        let err = CatcherError::HttpError {
            status: 503,
            body: "Service Unavailable".into(),
        };
        assert_eq!(err.category(), ErrorCategory::Retryable);
    }

    #[test]
    fn http_4xx_is_non_retryable() {
        let err = CatcherError::HttpError {
            status: 404,
            body: "Not Found".into(),
        };
        assert_eq!(err.category(), ErrorCategory::NonRetryable);
    }

    #[test]
    fn http_408_is_retryable() {
        // RFC 9110 §15.5.7: 408 MAY be retried on a new connection
        let err = CatcherError::HttpError {
            status: 408,
            body: "Request Timeout".into(),
        };
        assert_eq!(err.category(), ErrorCategory::Retryable);
    }

    #[test]
    fn http_429_is_retryable() {
        // RFC 6585: 429 should be retried after Retry-After
        let err = CatcherError::HttpError {
            status: 429,
            body: "Too Many Requests".into(),
        };
        assert_eq!(err.category(), ErrorCategory::Retryable);
    }

    #[test]
    fn http_401_is_non_retryable() {
        let err = CatcherError::HttpError {
            status: 401,
            body: "Unauthorized".into(),
        };
        assert_eq!(err.category(), ErrorCategory::NonRetryable);
    }

    #[test]
    fn circuit_breaker_open_is_retryable() {
        let err = CatcherError::CircuitBreakerOpen;
        assert_eq!(err.category(), ErrorCategory::Retryable);
    }

    #[test]
    fn retry_exhausted_is_non_retryable() {
        let err = CatcherError::RetryExhausted {
            attempts: 5,
            last_error: "timeout".into(),
        };
        assert_eq!(err.category(), ErrorCategory::NonRetryable);
    }

    #[test]
    fn encode_error_is_non_retryable() {
        let err = CatcherError::EncodeError("bad data".into());
        assert_eq!(err.category(), ErrorCategory::NonRetryable);
    }

    #[test]
    fn decode_error_is_non_retryable() {
        let err = CatcherError::DecodeError("corrupt".into());
        assert_eq!(err.category(), ErrorCategory::NonRetryable);
    }

    #[test]
    fn invalid_config_is_non_retryable() {
        let err = CatcherError::InvalidConfig("bad url".into());
        assert_eq!(err.category(), ErrorCategory::NonRetryable);
    }

    #[test]
    fn internal_error_is_non_retryable() {
        let err = CatcherError::Internal("unexpected".into());
        assert_eq!(err.category(), ErrorCategory::NonRetryable);
    }

    #[test]
    fn ws_disconnected_is_retryable() {
        let err = CatcherError::WsDisconnected {
            code: 1006,
            reason: "abnormal".into(),
        };
        assert_eq!(err.category(), ErrorCategory::Retryable);
    }

    #[test]
    fn ws_all_endpoints_failed_is_retryable() {
        let err = CatcherError::WsAllEndpointsFailed { count: 3 };
        assert_eq!(err.category(), ErrorCategory::Retryable);
    }

    #[test]
    fn queue_timeout_is_non_retryable() {
        let err = CatcherError::QueueTimeout(5000);
        assert_eq!(err.category(), ErrorCategory::NonRetryable);
    }

    #[test]
    fn sse_timeout_is_non_retryable() {
        let err = CatcherError::SseTimeout(30000);
        assert_eq!(err.category(), ErrorCategory::NonRetryable);
    }

    #[test]
    fn clone_preserves_category() {
        let err = CatcherError::ConnectionTimeout(1000);
        let cloned = err.clone();
        assert_eq!(cloned.category(), err.category());
    }

    #[test]
    fn display_does_not_panic() {
        // All variants should format without panic
        let errors = vec![
            CatcherError::ConnectionTimeout(1000),
            CatcherError::RequestTimeout(5000),
            CatcherError::TlsError("test".into()),
            CatcherError::DnsError { host: "h".into(), reason: "r".into() },
            CatcherError::HttpError { status: 500, body: "b".into() },
            CatcherError::WsHandshakeTimeout(3000),
            CatcherError::WsDisconnected { code: 1000, reason: "normal".into() },
            CatcherError::WsAllEndpointsFailed { count: 1 },
            CatcherError::RetryExhausted { attempts: 3, last_error: "e".into() },
            CatcherError::CircuitBreakerOpen,
            CatcherError::QueueTimeout(1000),
            CatcherError::EncodeError("e".into()),
            CatcherError::DecodeError("d".into()),
            CatcherError::InvalidConfig("c".into()),
            CatcherError::SseTimeout(5000),
            CatcherError::Internal("i".into()),
        ];
        for err in &errors {
            let s = err.to_string();
            assert!(!s.is_empty(), "Display should produce non-empty string for {:?}", err);
        }
    }
}
