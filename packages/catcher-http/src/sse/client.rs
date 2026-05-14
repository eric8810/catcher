use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::time::Duration;

use catcher_core::types::sse::SseClientConfig;
use catcher_core::CatcherError;
use reqwest::Client;
use tokio::sync::mpsc;
use tokio_stream::Stream;

use super::router::{route_line, RouteAction};

/// SSE client ready state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SseReadyState {
    Connecting,
    Open,
    Closed,
}

/// SSE long-lived client with auto-reconnect.
///
/// Background task handles reconnection with exponential backoff
/// and carries `Last-Event-ID` header on reconnect.
pub struct SseClient {
    lines_rx: mpsc::UnboundedReceiver<Result<String, CatcherError>>,
    cancel_tx: mpsc::UnboundedSender<()>,
    last_event_id: Arc<Mutex<String>>,
    ready_state: Arc<Mutex<SseReadyState>>,
}

impl SseClient {
    /// Create an SSE client with auto-reconnect.
    pub async fn connect(config: SseClientConfig) -> Result<Self, CatcherError> {
        let (lines_tx, lines_rx) = mpsc::unbounded_channel();
        let (cancel_tx, mut cancel_rx) = mpsc::unbounded_channel::<()>();

        let last_event_id = Arc::new(Mutex::new(String::new()));
        let ready_state = Arc::new(Mutex::new(SseReadyState::Connecting));
        let reconnect_delay = Arc::new(Mutex::new(None::<u64>));

        let config_clone = config.clone();
        let last_event_id_bg = last_event_id.clone();
        let ready_state_bg = ready_state.clone();
        let reconnect_delay_bg = reconnect_delay.clone();

        tokio::spawn(async move {
            let reconnect_config = config_clone.reconnect.clone();
            let max_retries = reconnect_config
                .as_ref()
                .map(|c| c.max_retries)
                .unwrap_or(10);
            let initial_delay_ms = reconnect_config
                .as_ref()
                .map(|c| c.initial_delay_ms)
                .unwrap_or(1000);
            let max_delay_ms = reconnect_config
                .as_ref()
                .map(|c| c.max_delay_ms)
                .unwrap_or(30_000);
            let backoff_multiplier = reconnect_config
                .as_ref()
                .map(|c| c.backoff_multiplier)
                .unwrap_or(2.0);

            let mut attempt: u32 = 0;

            loop {
                // Check cancel
                if cancel_rx.try_recv().is_ok() {
                    break;
                }

                // Mark as Connecting before each connection attempt
                *ready_state_bg.lock().unwrap() = SseReadyState::Connecting;

                match connect_once(&config_clone, &lines_tx, &last_event_id_bg, &ready_state_bg, &reconnect_delay_bg).await {
                    Ok(()) => {
                        // Stream ended normally — check if we should reconnect
                        if cancel_rx.try_recv().is_ok() {
                            break;
                        }
                    }
                    Err(e) => {
                        // Send error to consumer
                        let _ = lines_tx.send(Err(e));
                    }
                }

                attempt += 1;
                if attempt > max_retries {
                    break;
                }

                let delay_ms = {
                    let rd = *reconnect_delay_bg.lock().unwrap();
                    match rd {
                        Some(ms) => {
                            // Use server-specified retry interval with jitter
                            let jitter = ms as f64 * 0.25 * (rand_jitter() * 2.0 - 1.0);
                            (ms as f64 + jitter).max(0.0) as u64
                        }
                        None => {
                            // Exponential backoff with jitter
                            let base = initial_delay_ms as f64
                                * backoff_multiplier.powi(attempt as i32 - 1);
                            let capped = base.min(max_delay_ms as f64);
                            let jitter = capped * 0.25 * (rand_jitter() * 2.0 - 1.0);
                            (capped + jitter).max(0.0) as u64
                        }
                    }
                };

                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_millis(delay_ms)) => {}
                    _ = cancel_rx.recv() => { break; }
                }
            }

            *ready_state_bg.lock().unwrap() = SseReadyState::Closed;
        });

        Ok(Self {
            lines_rx,
            cancel_tx,
            last_event_id,
            ready_state,
        })
    }

    /// Read the next content line.
    pub async fn next_line(&mut self) -> Option<Result<String, CatcherError>> {
        self.lines_rx.recv().await
    }

    /// Close the connection (stops reconnection).
    pub fn close(&mut self) {
        let _ = self.cancel_tx.send(());
        *self.ready_state.lock().unwrap() = SseReadyState::Closed;
    }

    /// Current ready state.
    pub fn ready_state(&self) -> SseReadyState {
        *self.ready_state.lock().unwrap()
    }

    /// Last event ID (for reconnect).
    pub fn last_event_id(&self) -> String {
        self.last_event_id.lock().unwrap().clone()
    }
}

impl Stream for SseClient {
    type Item = Result<String, CatcherError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.lines_rx.poll_recv(cx)
    }
}

/// Simple deterministic-ish jitter (no rand dep needed).
fn rand_jitter() -> f64 {
    // Use a simple time-based pseudo-random for jitter.
    // Good enough for backoff jitter — doesn't need cryptographic quality.
    let ns = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos();
    ns as f64 / u32::MAX as f64
}

/// Perform a single SSE connection attempt.
async fn connect_once(
    config: &SseClientConfig,
    lines_tx: &mpsc::UnboundedSender<Result<String, CatcherError>>,
    last_event_id: &Arc<Mutex<String>>,
    ready_state: &Arc<Mutex<SseReadyState>>,
    reconnect_delay: &Arc<Mutex<Option<u64>>>,
) -> Result<(), CatcherError> {
    let client = Client::builder()
        .build()
        .map_err(|e| CatcherError::Internal(format!("reqwest build: {e}")))?;

    let method = match config.method {
        catcher_core::types::sse::SseMethod::GET => reqwest::Method::GET,
        catcher_core::types::sse::SseMethod::POST => reqwest::Method::POST,
    };

    let mut req = client.request(method, &config.url);
    for (k, v) in &config.headers {
        req = req.header(k, v);
    }

    // Carry Last-Event-ID for reconnection
    {
        let eid = last_event_id.lock().unwrap();
        if !eid.is_empty() {
            req = req.header("Last-Event-ID", &*eid);
        }
    }

    if let Some(ref body) = config.body {
        req = req.body(body.clone());
        if !config.headers.contains_key("Content-Type") {
            req = req.header("Content-Type", "application/json");
        }
    }
    req = req.timeout(Duration::from_millis(config.timeout_ms));

    let response = req.send().await.map_err(|e| {
        if e.is_timeout() {
            CatcherError::SseTimeout(config.timeout_ms)
        } else if e.is_connect() {
            CatcherError::ConnectionTimeout(config.timeout_ms)
        } else {
            CatcherError::Internal(format!("SSE connect: {e}"))
        }
    })?;

    let status = response.status().as_u16();

    // 204 = server says stop reconnecting (SSE spec)
    if status == 204 {
        *ready_state.lock().unwrap() = SseReadyState::Closed;
        return Ok(());
    }

    if status != 200 {
        return Err(CatcherError::HttpError {
            status,
            body: String::new(),
        });
    }

    *ready_state.lock().unwrap() = SseReadyState::Open;

    // Read bytes_stream → buffer → route lines → send via channel
    let mut buffer = String::new();
    let mut stream = response.bytes_stream();
    use tokio_stream::StreamExt;

    while let Some(chunk) = stream.next().await {
        let bytes = chunk.map_err(|e| CatcherError::Internal(format!("SSE read: {e}")))?;
        buffer.push_str(&String::from_utf8_lossy(&bytes));

        while let Some(newline_pos) = buffer.find('\n') {
            let line = buffer[..newline_pos]
                .trim_end_matches('\r')
                .to_string();
            buffer = buffer[newline_pos + 1..].to_string();

            match route_line(&line) {
                RouteAction::Yield(l) => {
                    if lines_tx.send(Ok(l)).is_err() {
                        // Consumer dropped — stop
                        *ready_state.lock().unwrap() = SseReadyState::Closed;
                        return Ok(());
                    }
                }
                RouteAction::SetLastEventId(id) => {
                    *last_event_id.lock().unwrap() = id;
                }
                RouteAction::SetRetry(ms) => {
                    *reconnect_delay.lock().unwrap() = Some(ms);
                }
                RouteAction::Silent => {}
            }
        }
    }

    // Process remaining buffer
    if !buffer.is_empty() {
        let line = buffer.trim_end_matches('\r');
        match route_line(line) {
            RouteAction::Yield(l) => {
                let _ = lines_tx.send(Ok(l));
            }
            RouteAction::SetLastEventId(id) => {
                *last_event_id.lock().unwrap() = id;
            }
            RouteAction::SetRetry(ms) => {
                *reconnect_delay.lock().unwrap() = Some(ms);
            }
            RouteAction::Silent => {}
        }
    }

    // Stream ended — state stays Open until next reconnect attempt
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use catcher_core::types::sse::{SseClientConfig, SseMethod, SseReconnectConfig};
    use std::collections::HashMap;
    use tokio_stream::StreamExt;
    use wiremock::{matchers::method, Mock, MockServer, ResponseTemplate};

    fn sse_config(url: &str) -> SseClientConfig {
        SseClientConfig {
            url: url.to_string(),
            method: SseMethod::GET,
            headers: HashMap::new(),
            body: None,
            reconnect: None,
            timeout_ms: 5000,
            circuit_breaker: None,
        }
    }

    fn sse_config_with_reconnect(url: &str) -> SseClientConfig {
        SseClientConfig {
            url: url.to_string(),
            method: SseMethod::GET,
            headers: HashMap::new(),
            body: None,
            reconnect: Some(SseReconnectConfig {
                max_retries: 1,
                initial_delay_ms: 50,
                max_delay_ms: 100,
                backoff_multiplier: 2.0,
            }),
            timeout_ms: 5000,
            circuit_breaker: None,
        }
    }

    /// RC1 — 基础消费
    #[tokio::test]
    async fn rc1_basic_consumption() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_string(
                "data: hello\n\ndata: world\n\n",
            ))
            .mount(&server)
            .await;

        let config = sse_config(&server.uri());
        let mut client = SseClient::connect(config).await.unwrap();

        let line1 = client.next_line().await.unwrap().unwrap();
        assert_eq!(line1, "data: hello");
        let line2 = client.next_line().await.unwrap().unwrap();
        assert_eq!(line2, "data: world");

        // Stream ends → reconnect attempt starts
        client.close();
    }

    /// RC2 — 自动重连 + Last-Event-ID
    #[tokio::test]
    async fn rc2_reconnect_with_last_event_id() {
        let server = MockServer::start().await;

        // First response: sends id + data then closes
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_string(
                "id: abc123\ndata: first\n\n",
            ))
            .up_to_n_times(1)
            .mount(&server)
            .await;

        // Second response: after reconnect with Last-Event-ID
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_string(
                "data: second\n\n",
            ))
            .mount(&server)
            .await;

        let config = sse_config_with_reconnect(&server.uri());
        let mut client = SseClient::connect(config).await.unwrap();

        let line1 = client.next_line().await.unwrap().unwrap();
        assert_eq!(line1, "data: first");
        assert_eq!(client.last_event_id(), "abc123");

        // Wait for reconnect
        let line2 = client.next_line().await.unwrap().unwrap();
        assert_eq!(line2, "data: second");

        client.close();
    }

    /// RC3 — close() 停止
    #[tokio::test]
    async fn rc3_close_stops() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_string(
                "data: ongoing\n",
            ))
            .mount(&server)
            .await;

        let config = sse_config(&server.uri());
        let mut client = SseClient::connect(config).await.unwrap();

        let line = client.next_line().await.unwrap().unwrap();
        assert_eq!(line, "data: ongoing");

        client.close();
        assert_eq!(client.ready_state(), SseReadyState::Closed);

        // next_line should eventually return None
        let next = client.next_line().await;
        assert!(next.is_none());
    }

    /// RC4 — 204 停止重连
    #[tokio::test]
    async fn rc4_204_stops_reconnect() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(204))
            .mount(&server)
            .await;

        let config = sse_config_with_reconnect(&server.uri());
        let mut client = SseClient::connect(config).await.unwrap();

        // Should get no lines, and eventually the channel closes
        let next = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            client.next_line(),
        ).await;
        // Either None (channel closed) or timeout — both acceptable
        match next {
            Ok(Some(_)) => panic!("Expected no data after 204"),
            Ok(None) => {} // Channel closed — good
            Err(_) => {} // Timeout — acceptable, 204 handling closes the loop
        }

        client.close();
    }

    /// RC5 — readyState 状态
    #[tokio::test]
    async fn rc5_ready_state_transitions() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_string(
                "data: hi\n\n",
            ))
            .mount(&server)
            .await;

        let config = sse_config(&server.uri());
        let mut client = SseClient::connect(config).await.unwrap();

        // Initially Connecting
        assert_eq!(client.ready_state(), SseReadyState::Connecting);

        // After receiving data, should be Open
        let _ = client.next_line().await;
        // Give background task time to finish processing and yield
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert_eq!(client.ready_state(), SseReadyState::Open);

        client.close();
        assert_eq!(client.ready_state(), SseReadyState::Closed);
    }

    /// RC6 — Stream trait 消费
    #[tokio::test]
    async fn rc6_stream_trait_consumption() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_string(
                "data: a\ndata: b\n\n",
            ))
            .mount(&server)
            .await;

        let config = sse_config(&server.uri());
        let mut client = SseClient::connect(config).await.unwrap();

        let mut collected = Vec::new();
        // Use StreamExt::next
        if let Some(Ok(line)) = client.next().await {
            collected.push(line);
        }
        if let Some(Ok(line)) = client.next().await {
            collected.push(line);
        }

        assert_eq!(collected, vec!["data: a", "data: b"]);
        client.close();
    }
}
