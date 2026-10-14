use crate::transport::ws_client::{connect_stream_with_context, WsConnectContext, WsStream};
use crate::types::ws::WsClientConfig;
use catcher_core::CatcherError;

/// 多端点竞速连接器
///
/// 同时向多个端点发起底层连接，取最先成功返回的一个。
/// 其余连接通过 abort 取消。
pub(crate) struct EndpointRacer {
    urls: Vec<String>,
    race_count: u32,
}

impl EndpointRacer {
    pub(crate) fn new(urls: Vec<String>, race_count: u32) -> Self {
        Self { urls, race_count }
    }

    /// 并发连接多个端点，返回最先成功的 `(url, stream, latency_ms)`。
    pub(crate) async fn race(
        &self,
        config: &WsClientConfig,
        connect_ctx: &WsConnectContext,
    ) -> Result<(String, WsStream, u64), CatcherError> {
        let urls: Vec<String> = self
            .urls
            .iter()
            .take(self.race_count as usize)
            .cloned()
            .collect();

        if urls.is_empty() {
            return Err(CatcherError::InvalidConfig(
                "no WS endpoints configured".into(),
            ));
        }

        // 单端点 — 直接连接
        if urls.len() == 1 {
            let url = urls.into_iter().next().unwrap();
            let (stream, lat) = connect_stream_with_context(&url, config, connect_ctx).await?;
            return Ok((url, stream, lat));
        }

        // 直接持有握手 future；竞速被取消或获胜时，丢弃其余 future，避免遗留后台任务。
        use futures_util::{stream::FuturesUnordered, StreamExt};
        let mut attempts = FuturesUnordered::new();
        for url in urls {
            attempts.push(async move {
                let (stream, latency) =
                    connect_stream_with_context(&url, config, connect_ctx).await?;
                Ok::<_, CatcherError>((url, stream, latency))
            });
        }
        while let Some(result) = attempts.next().await {
            if let Ok(connected) = result {
                return Ok(connected);
            }
        }
        Err(CatcherError::WsAllEndpointsFailed {
            count: self.urls.len(),
        })
    }
}
