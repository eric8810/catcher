# Performance: UniFFI `block_on_aux_thread` 每次调用创建新线程 + 新 Runtime

**严重程度**: 🔴 High — 每次 HTTP/WS/SSE 调用创建 OS 线程 + tokio Runtime

**状态**: Open

**位置**: `packages/catcher-uniffi/src/lib.rs:44-56`

---

## 当前代码

```rust
fn block_on_aux_thread<F, T>(future: F) -> std::thread::JoinHandle<T>
where
    F: std::future::Future<Output = T> + Send + 'static,
    T: Send + 'static,
{
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("Failed to create aux tokio runtime");
        rt.block_on(future)
    })
}
```

此函数被所有 UniFFI 方法调用（`HttpClient.get/post/put/delete/patch/sse_stream`、`WsClient::new`、`SseClientHandle::connect`、`evaluate_quality`）。

## 问题

每次调用：
1. `std::thread::spawn` — 创建 OS 线程（~1-2ms + 栈分配）
2. `tokio::runtime::Builder::new_current_thread().enable_all().build()` — 构建 tokio Runtime（创建 I/O 驱动、定时器、调度器）
3. 调用结束后线程和 Runtime 一起销毁

对一个简单的 HTTP GET 请求（通常 5-50ms），线程创建 + Runtime 初始化开销（~2-5ms）可能占延迟的 10%-50%。

## 为何这样设计

代码注释解释了原因：
> UniFFI 0.28 does not support async methods. All async Rust operations are
> bridged synchronously via `block_on_aux_thread()` which dispatches work to
> a **separate std thread** with its own tokio runtime. This avoids the
> `block_on()` re-entrance panic that would occur if a WsEventObserver
> callback (running on a tokio thread) calls back into an HttpClient method.

设计意图是避免 re-entrance panic，但实现代价过高。

## 修复方案

### 方案 A：复用单个 aux 线程（推荐）

```rust
fn aux_runtime() -> &'static tokio::runtime::Runtime {
    static RT: std::sync::OnceLock<tokio::runtime::Runtime> = std::sync::OnceLock::new();
    RT.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .expect("Failed to create aux tokio runtime")
    })
}

fn block_on_aux_thread<F, T>(future: F) -> std::thread::JoinHandle<T>
where
    F: std::future::Future<Output = T> + Send + 'static,
    T: Send + 'static,
{
    let rt = aux_runtime();
    // 在 aux runtime 上 spawn + 用 oneshot 同步等待结果
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    rt.spawn(async move {
        let result = future.await;
        let _ = tx.send(result);
    });
    std::thread::spawn(move || rx.recv().unwrap())
}
```

复用 Runtime 避免每次重新初始化 I/O 驱动和调度器。但仍有线程创建开销。

### 方案 B：线程池

使用 `rayon` 或 `threadpool` crate 复用 OS 线程，配合持久化的 tokio Runtime。

### 方案 C：升级 UniFFI

UniFFI 后续版本可能支持 async。升级后可移除整个 `block_on_aux_thread` 机制。

## 影响范围

- 所有 UniFFI 方法调用（Swift/Kotlin 绑定）
- 每个 HTTP 请求、WS 连接、SSE 连接、质量评估
