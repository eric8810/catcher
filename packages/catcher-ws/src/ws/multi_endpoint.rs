use crate::transport::ws_client::{connect_stream_with_client, WsStream};
use crate::types::ws::WsClientConfig;
use catcher_core::CatcherError;

/// 多端点竞速连接器
///
/// 同时向多个端点发起底层连接，取最先成功返回的一个。
/// 其余连接通过 abort 取消。
pub struct EndpointRacer {
    urls: Vec<String>,
    race_count: u32,
}

impl EndpointRacer {
    pub fn new(urls: Vec<String>, race_count: u32) -> Self {
        Self { urls, race_count }
    }

    /// 并发连接多个端点，返回最先成功的 `(url, stream, latency_ms)`。
    pub async fn race(
        &self,
        config: &WsClientConfig,
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
            let (stream, lat) = connect_stream_with_client(&url, config).await?;
            return Ok((url, stream, lat));
        }

        // 多端点 — 并发竞速
        // spawn 的任务返回 (url, stream, lat) 以便识别是哪个端点
        type RaceResult = Result<(String, WsStream, u64), CatcherError>;

        let mut handles: Vec<tokio::task::JoinHandle<RaceResult>> = Vec::with_capacity(urls.len());

        for url in urls {
            let config_c = config.clone();
            handles.push(tokio::spawn(async move {
                let (stream, lat) = connect_stream_with_client(&url, &config_c).await?;
                Ok((url, stream, lat))
            }));
        }

        // 用 select_all 逐个等待，取最先成功者
        let mut first_ok: Option<(String, WsStream, u64)> = None;
        let mut errors = Vec::new();

        while !handles.is_empty() {
            let (result, _, remaining) = futures_util::future::select_all(handles).await;
            handles = remaining;

            match result {
                Ok(Ok(triple)) => {
                    first_ok = Some(triple);
                    break;
                }
                Ok(Err(e)) => errors.push(e),
                Err(join_err) => errors.push(CatcherError::Internal(format!(
                    "race task panicked: {join_err}"
                ))),
            }
        }

        // 取消剩余任务
        for h in handles {
            h.abort();
        }

        match first_ok {
            Some(result) => Ok(result),
            None => Err(CatcherError::WsAllEndpointsFailed {
                count: self.urls.len(),
            }),
        }
    }
}
