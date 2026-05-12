use catcher_core::CatcherError;
use crate::transport::ws_client::{WsHandle, WsTransport};
use crate::types::ws::{WsClientConfig, WsEvent};

/// 多端点竞速连接器
///
/// 同时向多个端点发起连接，取最先成功返回的一个，
/// 其余连接通过取消令牌中止。
pub struct EndpointRacer {
    urls: Vec<String>,
    race_count: u32,
}

impl EndpointRacer {
    pub fn new(urls: Vec<String>, race_count: u32) -> Self {
        Self { urls, race_count }
    }

    /// 并发连接所有端点，返回最先成功的那个
    ///
    /// 使用 tokio::select! 等待最先完成者。
    pub async fn race(
        &self,
        config: &WsClientConfig,
    ) -> Result<
        (
            String,
            WsHandle,
            tokio::sync::mpsc::UnboundedReceiver<WsEvent>,
        ),
        CatcherError,
    > {
        // 注意：WsEvent 已通过 use 语句在文件顶部导入

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

        if urls.len() == 1 {
            let url = urls.into_iter().next().unwrap();
            let (handle, rx) = WsTransport::connect(&url, config).await?;
            return Ok((url, handle, rx));
        }

        // 多端点：竞速连接
        let mut handles = Vec::new();

        for url in &urls {
            let url_clone = url.clone();
            let config_clone = config.clone();
            handles.push(tokio::spawn(async move {
                WsTransport::connect(&url_clone, &config_clone).await
            }));
        }

        // 等待最先完成者
        let mut first_ok: Option<(
            String,
            WsHandle,
            tokio::sync::mpsc::UnboundedReceiver<WsEvent>,
        )> = None;
        let mut errors = Vec::new();

        while !handles.is_empty() {
            let (result, _, remaining) = futures_util::future::select_all(handles).await;
            handles = remaining;

            match result {
                Ok(Ok((handle, rx))) => {
                    first_ok = Some((handle.url().to_string(), handle, rx));
                    break; // 第一个成功，跳出
                }
                Ok(Err(e)) => {
                    errors.push(e);
                }
                Err(join_err) => {
                    errors.push(CatcherError::Internal(format!(
                        "race task panicked: {join_err}"
                    )));
                }
            }
        }

        // 取消剩余连接任务
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
