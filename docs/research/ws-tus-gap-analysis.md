# WS 与 TUS 上传协议 API 差距分析

> 目的：审视 catcher 封装的 WebSocket 和 tus 上传协议 API，与主流实现对比，识别需要补充的能力。
>
> **相关文档**：
> - HTTP 层差距分析：[api-gap-analysis.md](./api-gap-analysis.md)
> - WS/TUS 是否应独立拆分：[ws-tus-split-analysis.md](./ws-tus-split-analysis.md)

---

## 一、catcher WebSocket 现状一览

### Rust 核心层 (`catcher-rs`)

| 组件 | API / 字段 |
|------|-----------|
| `WsClientConfig` | `urls`, `protocols`, `headers`, `tls`, `handshake_timeout_ms`, `max_payload_bytes`, `deflate`, `reconnect`, `heartbeat`, `race_count` |
| `WsState` | `Disconnected`, `Connecting`, `Connected`, `Reconnecting { attempt, delay_ms }` |
| `WsEvent` | `Connected { endpoint, latency_ms }`, `Disconnected { code, reason }`, `Reconnecting { attempt, delay_ms }`, `Message { data, is_binary }`, `Error { message }`, `HeartbeatRtt { rtt_ms }` |
| `ReconnectConfig` | `initial_delay_ms`, `max_delay_ms`, `backoff_multiplier`, `max_attempts`, `jitter` |
| `HeartbeatConfig` | `interval_ms`, `adaptive`, `pong_timeout_ms`, `max_missed_pongs` |
| `DeflateConfig` | `compression_level`, `mem_level`, `threshold_bytes` |
| `WsTransport` | `connect(url, config) → (WsHandle, Receiver<WsEvent>)` |
| `WsHandle` | `send_text()`, `send_binary()`, `close()` |

### TypeScript 封装层 (`catcher-ts`)

| API | 说明 |
|-----|------|
| `createResilientWS(options) → ResilientWS` | 创建自恢复 WS 客户端 |
| `createReconnectStrategy(opts?) → ReconnectStrategy` | 独立的重连策略工厂 |
| `raceEndpoints(urls, options, timeoutMs?) → Promise<RaceResult>` | 多端点竞速连接 |
| `ResilientWS` (extends `EventTarget`) | `send()`, `close()`, `readyState`, `url`, `status`, `addEventListener/removeEventListener` |

---

## 二、WebSocket 对比：catcher vs 主流库

### 2.1 连接建立

| 能力 | dart:io WebSocket | web_socket_channel | socket_io_client | catcher | 差距 |
|------|-------------------|--------------------|------------------|---------|------|
| 单 URL 连接 | ✅ | ✅ | ✅ | ✅ | — |
| 多端点竞速 | ❌ | ❌ | ❌ | ✅ `raceEndpoints()` | **catcher 领先** |
| 自定义 protocols | ✅ | ✅ | ✅ | ✅ `protocols[]` | — |
| 自定义 headers | ❌ (WebSocket API 限制) | ❌ | ✅ | ✅ `headers` | catcher 领先 |
| 握手超时 | ❌ | ❌ | ✅ | ✅ `handshake_timeout_ms` | — |
| TLS 配置 | ⚠️ SecurityContext | ⚠️ SecurityContext | ✅ | ✅ `tls: TlsConfig` | — |

### 2.2 消息收发

| 能力 | dart:io WebSocket | web_socket_channel | socket_io_client | catcher | 差距 |
|------|-------------------|--------------------|------------------|---------|------|
| 文本消息 | ✅ | ✅ | ✅ | ✅ `send_text()` | — |
| 二进制消息 | ✅ | ✅ | ✅ | ✅ `send_binary()` | — |
| 流式消息（背压） | ✅ Stream | ✅ Stream | ❌ | ❌ | **缺消息流控/背压** |
| 发送超时 | ❌ | ❌ | ❌ | ❌ | — |
| 发送队列/缓冲 | ❌ | ❌ | ❌ | ❌ | **缺发送缓冲区管理** |
| 批量发送 | ❌ | ❌ | ✅ emit batch | ❌ | — |

### 2.3 重连与韧性

| 能力 | dart:io WebSocket | web_socket_channel | socket_io_client | catcher | 差距 |
|------|-------------------|--------------------|------------------|---------|------|
| 自动重连 | ❌ 需手动 | ❌ 需手动 | ✅ | ✅ `ReconnectConfig` | — |
| 指数退避 + jitter | ❌ | ❌ | ✅ | ✅ | — |
| 最大重试次数 | ❌ | ❌ | ✅ | ✅ `max_attempts` | — |
| 重连时事件通知 | ❌ | ❌ | ⚠️ | ✅ `WsEvent::Reconnecting` | — |
| 手动触发重连 | ❌ | ❌ | ✅ | ❌ | **缺 reconnect() API** |
| 暂停/恢复重连 | ❌ | ❌ | ❌ | ❌ | **缺 pauseReconnect/resumeReconnect** |
| 重连前 hook | ❌ | ❌ | ❌ | ❌ | **缺 onBeforeReconnect** |

### 2.4 心跳

| 能力 | dart:io WebSocket | web_socket_channel | socket_io_client | catcher | 差距 |
|------|-------------------|--------------------|------------------|---------|------|
| Ping/Pong | ✅ | ❌ | ✅ | ✅ `HeartbeatConfig` | — |
| 自适应心跳间隔 | ❌ | ❌ | ❌ | ✅ `adaptive: true` | **catcher 领先** |
| 心跳 RTT 上报 | ❌ | ❌ | ❌ | ✅ `WsEvent::HeartbeatRtt` | **catcher 领先** |
| pong 超时检测 | ❌ | ❌ | ✅ | ✅ `pong_timeout_ms` | — |
| 自定义心跳 payload | ❌ | ❌ | ✅ | ❌ | **缺自定义 ping 消息体** |

### 2.5 压缩

| 能力 | dart:io WebSocket | web_socket_channel | socket_io_client | catcher | 差距 |
|------|-------------------|--------------------|------------------|---------|------|
| perMessageDeflate | ✅ | ✅ | ❌ | ✅ `DeflateConfig` | — |
| 阈值控制 | ❌ | ❌ | — | ✅ `threshold_bytes` | catcher 领先 |

### 2.6 状态与生命周期

| 能力 | dart:io WebSocket | web_socket_channel | socket_io_client | catcher | 差距 |
|------|-------------------|--------------------|------------------|---------|------|
| 状态查询 | ✅ readyState | ✅ | ✅ | ✅ `WsState` / `status` | — |
| 状态变更事件 | ❌ | ❌ | ✅ | ✅ `EventTarget` | — |
| 连接延迟统计 | ❌ | ❌ | ❌ | ✅ `Connected { latency_ms }` | **catcher 领先** |
| 优雅关闭 | ✅ | ✅ | ✅ | ⚠️ `close()` 无 code/reason 参数 (Rust 层) | **Rust WsHandle.close() 缺参数** |
| 连接前取消 | ❌ | ❌ | ✅ | ❌ | **缺 AbortSignal/CancelToken** |

### 2.7 消息编解码

| 能力 | catcher-ts | catcher-rs | 差距 |
|------|-----------|------------|------|
| msgpack 编解码 | ✅ `pack/unpack` | ✅ | — |
| 自动二进制检测 | ✅ `isBinary` | — | — |
| WS 消息解码 | ✅ `decodeWSMessage` | — | — |
| 自定义 codec | ❌ | ❌ | **缺自定义编解码器注册** |

---

## 三、WebSocket 核心差距与建议

### 🔴 P0 — 严重影响可用性

#### WS-1. 连接取消 (CancelToken)

**现状**：连接过程中无法取消，特别是多端点竞速时，如果用户主动离开页面，竞速连接无法中止。

**建议**：
```rust
impl WsTransport {
    pub async fn connect(
        url: &str,
        config: &WsClientConfig,
        cancel_token: Option<CancelToken>,  // 新增
    ) -> Result<(WsHandle, mpsc::UnboundedReceiver<WsEvent>), CatcherError>;
}
```

#### WS-2. 发送缓冲与流控

**现状**：`WsHandle::send_text/send_binary` 是 fire-and-forget，无背压、无队列、无发送确认。

**典型问题**：弱网下 `send()` 速度远大于实际发送速度，消息堆积导致内存暴涨。

**建议**：
```rust
pub struct WsHandle {
    // ...existing fields...
    send_buffer: mpsc::Sender<WsOutboundMessage>,
}

pub struct WsSendOptions {
    pub timeout_ms: Option<u64>,
    pub priority: Priority,           // 高优先级消息插队
    pub require_ack: bool,            // 等待服务端 ACK
    pub on_sent: Option<Arc<dyn Fn() + Send + Sync>>,  // 发送完成回调
}

impl WsHandle {
    /// 带背压的发送，缓冲区满时 await
    pub async fn send(&self, data: WsOutboundMessage, options: WsSendOptions) -> Result<(), CatcherError>;

    /// 非阻塞发送，缓冲区满时丢弃最旧消息
    pub fn send_or_drop(&self, data: WsOutboundMessage);
}
```

#### WS-3. 优雅关闭参数

**现状**：Rust 层 `WsHandle::close()` 无参数，无法传递 close code 和 reason。

**建议**：
```rust
impl WsHandle {
    pub fn close(&self, code: u16, reason: &str) -> Result<(), CatcherError>;
}
```

### 🟡 P1 — 影响开发体验

#### WS-4. 手动重连 API

**现状**：重连仅在连接断开后自动触发，无法手动控制。

**建议**：
```rust
impl WsHandle {
    /// 手动触发重连（如网络切换后）
    pub fn reconnect(&self) -> Result<(), CatcherError>;

    /// 暂停自动重连（如进入后台）
    pub fn pause_reconnect(&self);

    /// 恢复自动重连
    pub fn resume_reconnect(&self);
}
```

#### WS-5. 重连前 hook

**建议**：
```rust
pub struct WsClientConfig {
    // ...existing fields...
    pub on_before_reconnect: Option<Arc<dyn Fn(u32, &CatcherError) -> ReconnectDecision + Send + Sync>>,
}

pub enum ReconnectDecision {
    Proceed { delay_ms: u64 },
    Skip,
    Abort,
}
```

应用场景：收到 4403（鉴权失败）时 Abort，收到 4401（token 过期）时先刷新 token 再 Proceed。

#### WS-6. 自定义心跳 payload

**建议**：
```rust
pub struct HeartbeatConfig {
    // ...existing fields...
    pub ping_message: Option<Vec<u8>>,  // 默认空 ping，可自定义业务心跳帧
    pub expect_pong_match: bool,        // pong 是否需要与 ping 匹配
}
```

#### WS-7. 自定义编解码器注册

**建议**：
```rust
pub trait WsCodec: Send + Sync {
    fn encode(&self, data: &serde_json::Value) -> Result<Vec<u8>, CatcherError>;
    fn decode(&self, data: &[u8], is_binary: bool) -> Result<serde_json::Value, CatcherError>;
}

pub struct WsClientConfig {
    // ...existing fields...
    pub codec: Option<Arc<dyn WsCodec>>,
}
```

### 🟢 P2 — 锦上添花

#### WS-8. 消息去重

基于消息 ID 的去重窗口，防止重连后收到重复消息。

#### WS-9. 连接共享 (Multiplexing)

多个逻辑通道复用同一物理 WS 连接，类似 Socket.IO 的 namespace 机制。

---

## 四、catcher TUS 上传现状

### 当前实现（生产代码 `UploadHelper` / `tusUploader.ts`）

根据项目文档，当前已有：

| 能力 | 实现情况 |
|------|---------|
| tus Creation 扩展 | ✅ `POST` 创建上传资源 |
| tus Core 协议 | ✅ `HEAD` 查偏移 + `PATCH` 续传 |
| 断点续传 | ✅ 中断后从断点继续 |
| 进度回调 | ⚠️ 有基本进度，但不够精细 |
| 元数据 | ⚠️ 有限 |
| uploadToken 获取 | ⚠️ **无重试**（`tusUploader.ts:42-50`） |

---

## 五、TUS 对比：catcher 现有实现 vs tus-js-client vs dio

### 5.1 tus 协议扩展支持

| tus 扩展 | 协议要求 | tus-js-client | catcher 现有 | 建议 |
|----------|---------|---------------|-------------|------|
| **Core** (HEAD + PATCH) | MUST | ✅ | ✅ | — |
| **Creation** (POST) | SHOULD | ✅ | ✅ | — |
| **Creation With Upload** | 可选 | ✅ `uploadDataDuringCreation` | ❌ | **P0 — 首次 POST 即携带数据，省一次 RTT** |
| **Termination** (DELETE) | 可选 | ✅ | ❌ | **P1 — 取消上传、释放服务端资源** |
| **Checksum** | 可选 | ✅ `Upload-Checksum` | ❌ | **P1 — 弱网下保证数据完整性** |
| **Concatenation** | 可选 | ✅ `parallelUploads` | ❌ | **P1 — 大文件并行分片上传** |
| **Expiration** | 可选 | ⚠️ | ❌ | P2 — 过期上传清理 |

**IM 生产场景优先级**：Creation (已有) → Creation-With-Upload (刚需) → Termination (刚需) → Checksum (推荐) → Concatenation (大文件推荐) → Expiration (可选)

### 5.2 上传配置

| 配置项 | tus-js-client | dio (FormData 上传) | catcher | 差距 |
|--------|---------------|---------------------|---------|------|
| endpoint (创建 URL) | ✅ | ✅ | ✅ | — |
| chunkSize | ✅ `Infinity` (默认全量) | ❌ 无分片 | ❌ | **缺分片上传控制** |
| retryDelays | ✅ `[0, 1000, 3000, 5000]` | ❌ | ❌ | **缺上传专用重试策略** |
| metadata | ✅ `{ filename, filetype, ... }` | ✅ FormData fields | ⚠️ 有限 | **缺完整元数据支持** |
| headers | ✅ 全局 + 单次请求 | ✅ | ⚠️ | — |
| overridePatchMethod | ✅ | — | ❌ | P2 — 兼容不支持 PATCH 的环境 |
| uploadLengthDeferred | ✅ 流式上传 | ❌ | ❌ | **P1 — 未知大小的流式上传** |
| parallelUploads | ✅ 并行分片数 | ❌ | ❌ | 同 Concatenation 扩展 |
| parallelUploadBoundaries | ✅ 自定义分片边界 | ❌ | ❌ | 同上 |
| addRequestId | ✅ `X-Request-ID` | ❌ | ❌ | P2 — 关联客户端/服务端日志 |
| fingerprint | ✅ 自动生成 | ❌ | ❌ | **P0 — 避免重复上传** |
| storeFingerprintForResuming | ✅ | ❌ | ❌ | **P0 — 跨 session 续传** |
| removeFingerprintOnSuccess | ✅ | ❌ | ❌ | P2 |
| uploadUrl | ✅ 直接续传 | ❌ | ❌ | P1 — 已知 URL 跳过 Creation |

### 5.3 生命周期钩子

| 钩子 | tus-js-client | catcher | 差距 |
|------|---------------|---------|------|
| `onProgress(bytesSent, bytesTotal)` | ✅ | ⚠️ 基本进度 | **需标准化** |
| `onChunkComplete(chunkSize, bytesAccepted, bytesTotal)` | ✅ | ❌ | **缺 chunk 级完成回调** |
| `onSuccess(payload)` | ✅ 含 `lastResponse` | ❌ | **缺上传成功回调** |
| `onError(err)` | ✅ 含 `originalRequest/Response` | ⚠️ | **缺错误上下文** |
| `onShouldRetry(err, retryAttempt, options)` | ✅ 自定义重试决策 | ❌ | **缺重试决策 hook** |
| `onBeforeRequest(req)` | ✅ 修改请求（加 Auth 等） | ❌ | **缺请求拦截** |
| `onAfterResponse(res)` | ✅ 读取响应头（刷新 token） | ❌ | **缺响应拦截** |
| `onUploadUrlAvailable(url)` | ✅ | ❌ | P2 |

### 5.4 URL 存储 / 续传模型

| 能力 | tus-js-client | catcher | 差距 |
|------|---------------|---------|------|
| localStorage 持久化 | ✅ | ❌ | **缺客户端 URL 存储** |
| 自定义 urlStorage | ✅ `UrlStorage` 接口 | ❌ | **缺可插拔存储** |
| findPreviousUploads() | ✅ | ❌ | **缺上传历史查询** |
| resumeFromPreviousUpload() | ✅ | ❌ | **缺跨 session 续传** |
| Node.js 文件存储 | ✅ | ❌ | — |

### 5.5 与 dio 上传能力对比

| 能力 | dio | catcher 需补充 |
|------|-----|---------------|
| `FormData` + `MultipartFile` | ✅ | 需在 HTTP 层补齐 |
| `dio.download(url, savePath)` | ✅ | 需补齐文件下载 |
| `onSendProgress` / `onReceiveProgress` | ✅ | 需补齐进度回调 |
| `CancelToken` 取消上传 | ✅ | 需补齐请求取消 |
| 断点续传上传 | ❌ (非 tus) | catcher 的 tus 实现应超越 |
| 流式下载 | ✅ `ResponseType.stream` | 需补齐 |

---

## 六、TUS 上传核心差距与建议

### 🔴 P0 — 严重影响 IM 场景可用性

#### TUS-1. 完整的 tus 客户端实现（Rust 核心层）

**现状**：tus 逻辑散落在 TS 层的 `UploadHelper` / `tusUploader` 中，未纳入 catcher-rs 核心。

**问题**：
- 无法复用 Rust 层的重试/熔断/并发控制
- FFI/Dart 端无法直接使用 tus 能力
- uploadToken 获取无重试，弱网下极易失败

**建议**：在 `catcher-rs` 中实现完整 tus 客户端：

```rust
// src/upload/tus_client.rs

pub struct TusClient {
    http: HttpTransport,
    config: TusConfig,
    url_storage: Arc<dyn TusUrlStorage>,
}

pub struct TusConfig {
    pub endpoint: String,
    pub chunk_size: Option<u64>,           // 分片大小，None = 全量
    pub retry_delays: Vec<u64>,            // 如 [0, 1000, 3000, 5000]
    pub metadata: HashMap<String, String>,  // filename, filetype, etc.
    pub headers: HashMap<String, String>,   // Auth 等
    pub creation_with_upload: bool,        // Creation-With-Upload 扩展
    pub parallel_uploads: u32,             // 并行分片数
    pub fingerprint_fn: Option<Arc<dyn Fn(&[u8]) -> String + Send + Sync>>,
}

pub struct TusUpload {
    pub id: String,
    pub url: String,
    pub file_size: u64,
    pub offset: u64,
    pub status: TusUploadStatus,
}

pub enum TusUploadStatus {
    Creating,
    Uploading,
    Paused,
    Completed,
    Failed(CatcherError),
}

impl TusClient {
    /// 创建新上传
    pub async fn create_upload(&self, file: TusFile) -> Result<TusUpload, CatcherError>;

    /// 恢复已有上传
    pub async fn resume_upload(&self, url: &str) -> Result<TusUpload, CatcherError>;

    /// 执行上传（含重试 + 进度回调）
    pub async fn upload(
        &self,
        upload: &mut TusUpload,
        on_progress: Option<Arc<dyn Fn(u64, u64) + Send + Sync>>,
        cancel_token: Option<CancelToken>,
    ) -> Result<(), CatcherError>;

    /// 暂停上传
    pub fn pause(&self, upload: &mut TusUpload);

    /// 终止上传（DELETE）
    pub async fn terminate(&self, upload: &TusUpload) -> Result<(), CatcherError>;

    /// 查找之前的上传（fingerprint 匹配）
    pub async fn find_previous_uploads(&self, file: &TusFile) -> Result<Vec<TusUpload>, CatcherError>;

    /// 获取上传偏移（HEAD）
    pub async fn get_offset(&self, url: &str) -> Result<u64, CatcherError>;
}

pub struct TusFile {
    pub data: Vec<u8>,           // 或 AsyncRead
    pub filename: String,
    pub content_type: String,
    pub size: u64,
}

// 可插拔的 URL 存储
pub trait TusUrlStorage: Send + Sync {
    fn store(&self, fingerprint: &str, url: &str) -> Result<(), CatcherError>;
    fn retrieve(&self, fingerprint: &str) -> Result<Option<String>, CatcherError>;
    fn remove(&self, fingerprint: &str) -> Result<(), CatcherError>;
    fn list_all(&self) -> Result<Vec<(String, String)>, CatcherError>;
}
```

#### TUS-2. Fingerprint 去重 + URL 存储

**现状**：完全缺失。同一文件重复上传时无法识别续传。

**建议**：
```rust
/// 默认指纹：基于文件内容 hash + 文件名 + 文件大小
pub fn default_fingerprint(file: &TusFile) -> String {
    use std::collections::hash_map::DefaultHasher;
    let mut hasher = DefaultHasher::new();
    std::hash::Hash::hash(&file.size, &mut hasher);
    std::hash::Hash::hash(&file.filename, &mut hasher);
    // 可选：采样前 N 字节内容 hash
    format!("tus::{:016x}::{}", hasher.finish(), file.filename)
}

/// 内存存储（默认）
pub struct MemoryUrlStorage { /* HashMap */ }

/// 持久化存储（可由调用方实现 SQLite / SharedPreferences 等）
/// FFI/Dart 层可提供 DartUrlStorage 通过回调到 Dart 侧持久化
```

#### TUS-3. Creation-With-Upload 扩展

**现状**：每次上传需先 POST 创建 → 再 PATCH 传数据，两次 RTT。

**tus 协议**：Creation-With-Upload 允许在 POST 创建时就携带文件数据，省掉一次 RTT。对 IM 场景的小图片/语音消息上传，减少 30-50% 延迟。

**建议**：
```rust
pub struct TusConfig {
    // ...existing fields...
    pub creation_with_upload: bool,  // POST 时同时发送数据
}
```

### 🟡 P1 — 影响 IM 场景体验

#### TUS-4. 上传重试策略 + onShouldRetry hook

**现状**：uploadToken 获取无重试（已明确为 bug），上传过程中的重试也缺失。

**tus-js-client**：`retryDelays: [0, 1000, 3000, 5000]`，失败后按间隔重试，最多 4 次。`onShouldRetry` 允许自定义重试决策。

**建议**：
```rust
pub struct TusConfig {
    pub retry_delays: Vec<u64>,  // 上传专用重试间隔
    pub on_should_retry: Option<Arc<dyn Fn(&CatcherError, u32) -> bool + Send + Sync>>,
}
```

#### TUS-5. Termination 扩展 (DELETE)

**现状**：无法取消进行中的上传，服务端资源泄漏。

**建议**：
```rust
impl TusClient {
    /// 发送 DELETE 请求终止上传，释放服务端资源
    pub async fn terminate(&self, upload: &TusUpload) -> Result<(), CatcherError> {
        self.http.execute(HttpRequest {
            method: HttpMethod::DELETE,
            url: upload.url.clone(),
            ..Default::default()
        }).await?;
        Ok(())
    }
}
```

#### TUS-6. Checksum 扩展

**现状**：上传数据无完整性校验，弱网下可能上传损坏数据而不自知。

**tus 协议**：`Upload-Checksum: sha256 <base64-hash>` header。

**建议**：
```rust
pub struct TusConfig {
    pub checksum_algorithm: Option<ChecksumAlgorithm>,
}

pub enum ChecksumAlgorithm {
    Sha256,
    Md5,
    Sha1,
}

// PATCH 请求中附加 Upload-Checksum header
```

#### TUS-7. Concatenation 扩展 + 并行上传

**现状**：大文件只能串行上传，无法利用多连接并行加速。

**tus 协议**：文件分 N 片并行上传，最后 POST 合并。

**建议**：
```rust
pub struct TusConfig {
    pub parallel_uploads: u32,                   // 并行分片数
    pub parallel_upload_boundaries: Option<Vec<UploadBoundary>>,  // 自定义分片边界
}

pub struct UploadBoundary {
    pub start: u64,
    pub end: u64,
}
```

#### TUS-8. 流式上传 (uploadLengthDeferred)

**现状**：必须提前知道文件大小。对于录制中的视频/音频流上传，无法边录边传。

**建议**：
```rust
pub struct TusConfig {
    pub upload_length_deferred: bool,  // 使用 Upload-Defer-Length: 1
}

// 上传结束时发送 Upload-Length header 确认最终大小
```

#### TUS-9. 请求拦截 (onBeforeRequest / onAfterResponse)

**现状**：无法在 tus 请求中注入/修改 header（如刷新过期的 Auth token）。

**tus-js-client**：
```js
onBeforeRequest: (req) => { req.setHeader('Authorization', `Bearer ${token}`) },
onAfterResponse: (res) => { if (res.getStatus() === 401) refreshToken() },
```

**建议**：与 HTTP 层的拦截器系统统一设计。

### 🟢 P2 — 锦上添花

#### TUS-10. overridePatchMethod

使用 `POST + X-HTTP-Method-Override: PATCH` 兼容不支持 PATCH 的代理/网关。

#### TUS-11. addRequestId

每个 tus HTTP 请求添加 `X-Request-ID: <uuid>` header，便于客户端/服务端日志关联。

#### TUS-12. 上传限速

弱网下避免上传占满带宽导致 IM 消息收发阻塞：

```rust
pub struct TusConfig {
    pub max_upload_bandwidth_bytes_per_sec: Option<u64>,
}
```

---

## 七、FFI 层补充

当前 FFI 层（`09-ffi.md`）对 WS 和 TUS 的暴露严重不足：

### WS FFI

| 缺失 | 建议 |
|------|------|
| 无连接取消 | `catcher_ws_connect_cancel(handle)` |
| close 无参数 | `catcher_ws_close(handle, code, reason)` |
| 无状态查询 | `catcher_ws_get_state(handle) → WsState` |
| 无发送确认 | 增加 `on_sent` 回调 |
| 无缓冲区信息 | `catcher_ws_get_buffer_size(handle)` |

### TUS FFI（完全缺失）

```rust
// 新增 src/ffi/tus_ffi.rs

#[no_mangle]
pub extern "C" fn catcher_tus_create(config_json: *const c_char) -> *mut c_void;

#[no_mangle]
pub extern "C" fn catcher_tus_upload(
    handle: *mut c_void,
    file_path: FfiString,
    file_size: u64,
    metadata_json: *const c_char,
    progress_callback: EventCallback,
    user_data: *mut c_void,
) -> FfiResult;

#[no_mangle]
pub extern "C" fn catcher_tus_pause(handle: *mut c_void, upload_id: FfiString);
#[no_mangle]
pub extern "C" fn catcher_tus_resume(handle: *mut c_void, upload_id: FfiString);
#[no_mangle]
pub extern "C" fn catcher_tus_terminate(handle: *mut c_void, upload_id: FfiString);
#[no_mangle]
pub extern "C" fn catcher_tus_find_previous(handle: *mut c_void, fingerprint: FfiString) -> FfiResult;
#[no_mangle]
pub extern "C" fn catcher_tus_destroy(handle: *mut c_void);
```

Dart 侧 (flutter_rust_bridge)：

```rust
pub fn create_tus_client(config: TusClientConfigDto) -> TusClientHandle;
pub async fn tus_upload(handle: TusClientHandle, file_path: String, options: TusUploadOptionsDto) -> Result<TusUploadResultDto, String>;
pub fn tus_pause(handle: TusClientHandle, upload_id: String);
pub fn tus_resume(handle: TusClientHandle, upload_id: String);
pub async fn tus_terminate(handle: TusClientHandle, upload_id: String) -> Result<(), String>;
```

---

## 八、优先级路线图

| 优先级 | 补充能力 | 影响范围 | 工作量 |
|--------|---------|---------|--------|
| **WS P0-1** | 连接取消 (CancelToken) | Rust + TS + FFI | 中 |
| **WS P0-2** | 发送缓冲与流控 | Rust + TS + FFI | 大 |
| **WS P0-3** | 优雅关闭参数 (code, reason) | Rust | 小 |
| **TUS P0-1** | 完整 tus 客户端实现 (Rust 核心) | Rust + TS + FFI | 大 |
| **TUS P0-2** | Fingerprint 去重 + URL 存储 | Rust + TS + FFI | 中 |
| **TUS P0-3** | Creation-With-Upload 扩展 | Rust + TS | 中 |
| **WS P1-1** | 手动重连 API | Rust + TS | 小 |
| **WS P1-2** | 重连前 hook (onBeforeReconnect) | Rust + TS | 小 |
| **WS P1-3** | 自定义心跳 payload | Rust + TS | 小 |
| **WS P1-4** | 自定义编解码器注册 | Rust + TS | 中 |
| **TUS P1-1** | 上传重试策略 + onShouldRetry | Rust + TS | 中 |
| **TUS P1-2** | Termination 扩展 (DELETE) | Rust + TS | 小 |
| **TUS P1-3** | Checksum 扩展 | Rust + TS | 中 |
| **TUS P1-4** | Concatenation + 并行上传 | Rust + TS + FFI | 大 |
| **TUS P1-5** | 流式上传 (uploadLengthDeferred) | Rust + TS | 中 |
| **TUS P1-6** | 请求拦截 (onBeforeRequest/onAfterResponse) | Rust + TS | 中 |
| **WS P2-1** | 消息去重 | Rust + TS | 中 |
| **WS P2-2** | 连接共享 (Multiplexing) | Rust + TS | 大 |
| **TUS P2-1** | overridePatchMethod | Rust | 小 |
| **TUS P2-2** | addRequestId (X-Request-ID) | Rust | 小 |
| **TUS P2-3** | 上传限速 | Rust | 中 |

---

## 九、总结

### WebSocket：catcher 已是领先者，需补齐基础设施

catcher 的 WS 实现在多端点竞速、自适应心跳、心跳 RTT、perMessageDeflate 等方面已显著超越 dart:io WebSocket、web_socket_channel 等主流库。核心短板在**基础设施层**：

1. **发送侧缺失**：无缓冲、无背压、无发送确认 — 弱网下极易丢消息或内存暴涨
2. **控制力不足**：无法取消连接、无法手动重连、无法自定义重连决策
3. **关闭不规范**：Rust 层 close 缺少 code/reason，不满足 WebSocket 规范

### TUS 上传：需从零建设 Rust 核心层

当前 tus 上传仅存在于 TS 层的 `UploadHelper`，功能残缺且无法被 FFI/Dart 端使用。需要：

1. **在 Rust 核心层实现完整 tus 客户端**，复用已有的重试/熔断/并发控制
2. **补齐关键协议扩展**：Creation-With-Upload (省 RTT)、Termination (取消上传)、Checksum (数据完整性)、Concatenation (并行上传)
3. **建设续传基础设施**：fingerprint 去重 + URL 存储，实现跨 session 续传
4. **暴露完整 FFI 接口**：创建/上传/暂停/恢复/终止/查询历史

这些能力补齐后，catcher 将成为首个**内建 tus 断点续传的跨平台网络韧性库**，对 IM 场景的弱网体验提升是决定性的。
