use std::collections::VecDeque;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

use bytes::Bytes;
use catcher_core::types::sse::SseClientConfig;
use catcher_core::CatcherError;
use reqwest::Client;
use tokio_stream::Stream;

use super::router::{route_line, RouteAction};

/// SSE content line stream — yields content lines, silently filters control lines.
///
/// One-shot consumption, no auto-reconnect.
/// Implements `Stream` for `while let Some(line) = stream.next().await { ... }`.
pub struct SseStream {
    bytes_stream: Pin<Box<dyn Stream<Item = Result<Bytes, reqwest::Error>> + Send>>,
    buffer: String,
    pending_lines: VecDeque<String>,
    last_event_id: String,
    reconnect_delay: Option<u64>,
    done: bool,
}

impl SseStream {
    /// Create an SSE stream (one-shot, no auto-reconnect).
    pub async fn connect(config: SseClientConfig) -> Result<Self, CatcherError> {
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
        if status != 200 {
            return Err(CatcherError::HttpError {
                status,
                body: String::new(),
            });
        }

        let bytes_stream = Box::pin(response.bytes_stream());

        Ok(Self {
            bytes_stream,
            buffer: String::new(),
            pending_lines: VecDeque::new(),
            last_event_id: String::new(),
            reconnect_delay: None,
            done: false,
        })
    }

    /// Process complete lines from the buffer.
    fn process_buffer(&mut self) {
        while let Some(newline_pos) = self.buffer.find('\n') {
            let line = self.buffer[..newline_pos]
                .trim_end_matches('\r')
                .to_string();
            self.buffer = self.buffer[newline_pos + 1..].to_string();
            self.process_line(&line);
        }
    }

    /// Route a single line and accumulate content lines.
    fn process_line(&mut self, line: &str) {
        match route_line(line) {
            RouteAction::Yield(l) => self.pending_lines.push_back(l),
            RouteAction::SetLastEventId(id) => self.last_event_id = id,
            RouteAction::SetRetry(ms) => self.reconnect_delay = Some(ms),
            RouteAction::Silent => {}
        }
    }

    /// Last event ID (extracted from `id:` lines, for reconnect).
    pub fn last_event_id(&self) -> &str {
        &self.last_event_id
    }
}

impl Stream for SseStream {
    type Item = Result<String, CatcherError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        loop {
            // Return pending content lines first
            if let Some(line) = self.pending_lines.pop_front() {
                return Poll::Ready(Some(Ok(line)));
            }

            if self.done {
                return Poll::Ready(None);
            }

            // Extract complete lines from buffer
            self.process_buffer();
            if !self.pending_lines.is_empty() {
                continue;
            }

            // Need more data from the bytes stream
            match self.bytes_stream.as_mut().poll_next(cx) {
                Poll::Ready(Some(Ok(bytes))) => {
                    let text = String::from_utf8_lossy(&bytes);
                    self.buffer.push_str(&text);
                    // Loop back to process new data
                }
                Poll::Ready(Some(Err(e))) => {
                    return Poll::Ready(Some(Err(CatcherError::Internal(format!(
                        "SSE read error: {e}"
                    )))));
                }
                Poll::Ready(None) => {
                    // Stream ended — process remaining buffer
                    self.done = true;
                    if !self.buffer.is_empty() {
                        let line = self.buffer.trim_end_matches('\r').to_string();
                        self.buffer.clear();
                        self.process_line(&line);
                        if !self.pending_lines.is_empty() {
                            continue;
                        }
                    }
                    return Poll::Ready(None);
                }
                Poll::Pending => {
                    return Poll::Pending;
                }
            }
        }
    }
}
