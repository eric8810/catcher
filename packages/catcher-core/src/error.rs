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
    /// - 网络层错误（超时/DNS/TLS/断开）→ 可重试
    /// - 5xx HTTP 错误 → 可重试
    /// - 4xx HTTP 错误 → 不可重试（客户端错误重试无意义）
    /// - 编解码错误 → 不可重试（重试也不会变正确）
    /// - 熔断器打开 → 可重试（等恢复后重试）
    /// - 配置/内部错误 → 不可重试（需修复）
    pub fn category(&self) -> ErrorCategory {
        match self {
            CatcherError::ConnectionTimeout(_)
            | CatcherError::RequestTimeout(_)
            | CatcherError::TlsError(_)
            | CatcherError::DnsError { .. }
            | CatcherError::WsHandshakeTimeout(_)
            | CatcherError::WsDisconnected { .. }
            | CatcherError::WsAllEndpointsFailed { .. }
            | CatcherError::CircuitBreakerOpen => ErrorCategory::Retryable,

            CatcherError::HttpError { status, .. } => {
                if *status >= 500 {
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
            | CatcherError::QueueTimeout(_) => ErrorCategory::NonRetryable,
        }
    }
}
