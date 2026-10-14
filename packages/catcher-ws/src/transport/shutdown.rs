use tokio::sync::watch;

/// 从首次握手之前就可用的终止控制。一个实例只用于一次连接生命周期。
#[derive(Clone)]
pub struct WsShutdown {
    cancelled: watch::Sender<bool>,
    stopped: watch::Sender<bool>,
}

impl Default for WsShutdown {
    fn default() -> Self {
        Self {
            cancelled: watch::channel(false).0,
            stopped: watch::channel(false).0,
        }
    }
}

impl WsShutdown {
    /// 立即请求终止握手、发送和重连；可重复调用。
    pub fn cancel(&self) {
        self.cancelled.send_replace(true);
    }

    /// 返回是否已请求终止。
    pub fn is_cancelled(&self) -> bool {
        *self.cancelled.borrow()
    }

    /// 等待终止请求。
    pub async fn cancelled(&self) {
        let mut receiver = self.cancelled.subscribe();
        let _ = receiver.wait_for(|value| *value).await;
    }

    /// 等待该 WS 握手/连接管理器退出；不包括共享 DNS 后台任务或绑定层回调。
    /// 须先启动 connect_with_shutdown。
    pub async fn wait_closed(&self) {
        let mut receiver = self.stopped.subscribe();
        let _ = receiver.wait_for(|value| *value).await;
    }

    pub(crate) fn completion_guard(&self) -> CompletionGuard {
        CompletionGuard(self.stopped.clone())
    }
}

pub(crate) struct CompletionGuard(watch::Sender<bool>);
impl Drop for CompletionGuard {
    fn drop(&mut self) {
        self.0.send_replace(true);
    }
}
