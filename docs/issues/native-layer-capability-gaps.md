# 原生层能力缺口 — 超越 FFI 透传

> 日期：2026-06
> 来源：对照 `ffi-uniffi-capability-gaps.md` 已修复项目，逐项审查 Rust 原生层 vs TS 层的对等能力
> 结论：**4 个 TS 层已具备、但 Rust 原生层仍未对等覆盖的能力缺口**。非 FFI 透传问题，需要 Rust 核心层新增机制。

---

## 背景

`ffi-uniffi-capability-gaps.md` 的 P0/P1 目标是**最小可用**——让每个绑定层至少有一个方式完成操作。当前 C ABI 25 符号已全部导出，P0/P1 全部 ✅。

但细粒度对比 TS 层（axios/fetch 底层）与 Rust 原生层（reqwest 底层）的能力后，发现 4 个缺口：

| # | 缺口 | TS 层 | Rust 原生层 | 根因 |
|---|------|:-----:|:----------:|------|
| N-01 | Multipart/FormData 文件上传 | ✅ 借道 axios/fetch | ❌ 零支持 | Rust 无 multipart 编码器，FFI 无 multipart 语义 |
| N-02 | 流式文件下载 `responseType: stream` | ✅ axios stream / fetch ReadableStream | ❌ 仅 SSE 流 | `HttpTransport::do_execute` 硬编码 `response.bytes().await` |
| N-03 | 单请求级 cancel（非 `cancelAll`） | ✅ AbortController per-request | ❌ 仅全局 `cancel_all()` | `CancellationToken` 单一实例，无 per-request child handle |
| N-04 | 网络质量实时事件推送 | ✅ 可 Timer 轮询替代 | ⚠️ 仅 polling | `NetworkQualityEvaluator` 无 subscribe/push 机制 |

> **注意**：SSE headers 在 `catcher_sse_stream` 中已支持 `headers_json` 参数 ✅。不在此缺口清单中。

---

## 详细 Issue

### N-01: Multipart/FormData 文件上传

#### 现状

- **TS 层** (`catcher-http-ts`): `isFormDataBody()` 检测 → 跳过 Content-Type → axios 自动编码 multipart
- **TS 层** (`catcher-web`): `body instanceof FormData` → `fetch()` 原生处理
- **Rust 层**: `HttpRequest.body: Option<Vec<u8>>` + `content_type: Option<String>` — 接受裸字节，但无编码能力
- **FFI 层**: `catcher_http_post` 接受 `body + content_type`，理论上调用方可自行编码后传入

#### 为什么是缺口

当前 `catcher_http_post` 可以传任意 `content_type`（包括 `multipart/form-data; boundary=...`），但：

1. **无 Rust 侧 multipart 编码器**：调用方（Dart/Swift/Kotlin）需自行实现 RFC 7578 编码
2. **Dart 侧无现成工具**：`dart:io` 的 `HttpClient` 有 `MultipartRequest`，但 catcher 走 `dart:ffi` 不走 dart:io
3. **大型文件场景**：Rust 侧做编码可避免将整个文件 load 到 Dart 内存再跨 FFI 传递

#### 修复方案

**方案 A（推荐 P2）：调用方自行编码**

保持当前 FFI 签名不变，由 Dart/Swift/Kotlin 侧各自实现 multipart 编码后传入 `body + content_type`。

优点：
- FFI 无需改动
- multipart 编码是纯数据操作，放调用方符合 FFI 分层原则
- 单文件简单场景编码成本低

缺点：
- Dart 侧需引入或实现 multipart encoder（约 200-300 行，RFC 7578 boundary 编码细节较多）
- 大文件需在调用方加载到内存 → **方案 A 仅适用于小文件场景（<10MB），大文件需走方案 B**

**方案 B（P3 增强）：Rust 侧提供 multipart builder**

```rust
// catcher-http/src/multipart/builder.rs (新增模块)

pub struct MultipartBuilder {
    boundary: String,
    parts: Vec<MultipartPart>,
}

pub struct MultipartPart {
    pub name: String,
    pub filename: Option<String>,
    pub content_type: Option<String>,
    pub data: Vec<u8>,
}

impl MultipartBuilder {
    pub fn new() -> Self { ... }
    pub fn add_text(&mut self, name: &str, value: &str) -> &mut Self { ... }
    pub fn add_file(&mut self, name: &str, filename: &str, data: Vec<u8>, content_type: &str) -> &mut Self { ... }
    pub fn build(&self) -> (Vec<u8>, String) { ... }  // → (encoded_body, content_type)
}
```

FFI 暴露：

```rust
#[no_mangle]
pub unsafe extern "C" fn catcher_multipart_create() -> *mut c_void

#[no_mangle]
pub unsafe extern "C" fn catcher_multipart_add_text(
    handle: *mut c_void,
    name: FfiString, value: FfiString,
)

#[no_mangle]
pub unsafe extern "C" fn catcher_multipart_add_file(
    handle: *mut c_void,
    name: FfiString, filename: FfiString,
    data: *const u8, data_len: usize,
    content_type: FfiString,
)

#[no_mangle]
pub unsafe extern "C" fn catcher_multipart_build(
    handle: *mut c_void,
    body_out: *mut *mut u8,
    body_len_out: *mut usize,
    content_type_out: *mut *mut c_char,
)

#[no_mangle]
pub unsafe extern "C" fn catcher_multipart_destroy(handle: *mut c_void)
```

#### 建议

**P2 阶段采用方案 A**：Dart 侧实现简单 multipart encoder（~200-300行），支持文本 + 单文件，满足 80% 场景（<10MB 文件）。**P3 阶段评估方案 B**：如果 Dart 侧编码成为性能/内存瓶颈（大文件多文件上传），再下沉到 Rust。

> 测试方案：[../test/native-gap-test-design.md](../test/native-gap-test-design.md) — N-01 节

#### 验收标准

- [ ] Dart 侧 `client.post('/upload', multipart: {file: bytes, 'field': 'value'})` 可上传文件
- [ ] 服务端收到正确的 `multipart/form-data; boundary=...` 请求
- [ ] 方案 A：小文件 <10MB 上传正常
- [ ] 方案 B：大文件 >100MB 上传不会 OOM（Rust 侧编码 + 流式发送）

---

### N-02: 流式文件下载 (`responseType: stream`)

#### 现状

- **TS 层** (`catcher-http-ts`): `responseType: 'stream'` → axios 返回 `data` 为 Node.js `Readable` stream
- **TS 层** (`catcher-web`): `responseType: 'stream'` → fetch `resp.body` 为 `ReadableStream`
- **Rust 层**: `HttpTransport::do_execute` 第 243 行硬编码 `response.bytes().await` — 强制全量读入内存
- **FFI 层**: 所有 callback 一次性返回完整 `HttpResponse { body: Vec<u8> }`

#### 为什么是缺口

1. **大文件下载 OOM 风险**：当前 `response.bytes().await` 将整个响应体加载到内存。100MB 文件 → 100MB 堆分配
2. **无法实现进度回调**：全量读取意味着无从汇报下载进度
3. **SSE stream 已验证可行**：`catcher_sse_stream` 已证明 callback 逐块推送模式在 C ABI 中可行

#### 修复方案

**新增流式下载 C ABI 符号**：

```rust
/// 流式 HTTP 请求 — 通过 callback 逐块推送响应数据。
/// callback 的 event_type:
///   "chunk"     — 响应体数据块 (event_data 是原始字节)
///   "headers"   — 响应状态行 + 头 (event_data 是 JSON)
///   "done"      — 流结束
///   "error"     — 流错误
///
/// chunk_size_hint: 建议缓冲区大小，0 = 默认 8192。
/// 注意：reqwest 的 bytes_stream() 由 TCP/HTTP 帧大小决定实际 chunk 尺寸，
/// chunk_size_hint 仅用于 Dart 侧读缓冲区大小，不控制底层分块。
#[no_mangle]
pub unsafe extern "C" fn catcher_http_execute_stream(
    handle: *mut c_void,
    method: FfiString,
    url: FfiString,
    body: *const u8,
    body_len: usize,
    content_type: FfiString,
    headers_json: *const c_char,
    timeout_ms: u32,
    chunk_size_hint: u32,
    callback: EventCallback,
    user_data: *mut c_void,
)
```

**Rust 层实现要点**（简化示例，实际 FFI 层用 `extern "C" fn` 而非 `impl Fn`）：

```rust
// HttpTransport 新增方法（内部使用，非 FFI 直接暴露）
impl HttpTransport {
    pub async fn execute_stream(
        &self,
        request: HttpRequest,
        chunk_callback: impl Fn(&[u8]) + Send + 'static,
    ) -> Result<HttpResponse, CatcherError> {
        // ...构建 reqwest 请求...
        // 注：完整实现需 select! 监听 cancel_token

        let response = req.send().await?;
        let status = response.status().as_u16();
        let headers = extract_headers(&response);

        // 通知 headers
        chunk_callback(header_json.as_bytes());

        // 逐 chunk 读取
        let mut stream = response.bytes_stream();
        use futures_util::StreamExt;
        while let Some(chunk_result) = stream.next().await {
            let chunk = chunk_result?;
            chunk_callback(&chunk);
        }

        Ok(HttpResponse { status, headers, body: vec![], elapsed_ms })
    }
}
```

**Dart 侧封装**：

```dart
// packages/catcher_core/lib/src/http_client.dart
Stream<HttpStreamEvent> executeStream(
  String method,
  String url, {
  Map<String, String>? headers,
  Uint8List? body,
}) async* {
  final completer = StreamController<HttpStreamEvent>();
  _catcherHttpExecuteStream(
    _handle, method, url, body, headersJson, timeoutMs, chunkSize,
    Pointer.fromFunction(_streamCallback),
    completer.toNativePtr(),
  );
  yield* completer.stream;
}
```

#### 与 `catcher_http_execute` 的关系

| 特性 | `catcher_http_execute` | `catcher_http_execute_stream` |
|------|----------------------|------------------------------|
| 响应读取 | 全量 `bytes().await` | 逐 chunk `bytes_stream()` |
| 回调次数 | 1 次 | N+2 次 (headers + N chunks + done) |
| 内存占用 | O(response_size) | O(chunk_size) |
| 适合场景 | API 调用 (<1MB) | 文件下载 (>10MB) |
| cancel 支持 | `cancel_all()` | `cancel_all()` + 流中断 |

#### 验收标准

- [ ] 下载 100MB 文件时内存占用 <10MB
- [ ] `event_type="chunk"` 回调携带原始字节
- [ ] `event_type="headers"` 在第一个 chunk 之前触发
- [ ] `cancel_all()` 中断流式下载
- [ ] Dart 侧可通过 `Stream<HttpStreamEvent>` 消费

> 测试方案：[../test/native-gap-test-design.md](../test/native-gap-test-design.md) — N-02 节

---

### N-03: 单请求级 Cancel

#### 现状

- **Rust 层**: `HttpTransport` 只有一个全局 `cancel_token: Arc<Mutex<CancellationToken>>`
  - `cancel_all()` 取消全部飞行请求，替换新 token
  - 所有请求共享同一个 child token
- **FFI 层**: 只暴露 `catcher_http_client_cancel_all`
- **TS 层**: 每个请求独立的 `AbortController` → `signal`

#### 为什么是缺口

列表页场景：页面同时发起 3 个请求拉不同 Tab 数据，用户切换 Tab 时只需取消上一个 Tab 的未完成请求，其余 2 个应继续。

`cancelAll()` 会把所有 3 个都杀掉，不适用此场景。

#### 修复方案

**在 `HttpTransport` 中增加 per-request cancel 机制**：

```rust
use std::collections::HashMap;
use std::sync::Mutex;

pub struct HttpTransport {
    // ...existing fields...
    /// Per-request cancel tokens, keyed by request_id.
    /// cancel_all() 遍历并 cancel 全部，cancel_request(id) 只 cancel 单个。
    pending_requests: Mutex<HashMap<u64, tokio_util::sync::CancellationToken>>,
    next_request_id: AtomicU64,
}

impl HttpTransport {
    /// 执行请求并返回 (request_id, result)，用于后续单请求取消。
    /// 调用方通过 cancel_request(id) 取消特定请求。
    pub async fn execute(
        &self,
        request: HttpRequest,
    ) -> (u64, Result<HttpResponse, CatcherError>) {
        let request_id = self.next_request_id.fetch_add(1, Ordering::Relaxed);
        let cancel = CancellationToken::new();
        self.pending_requests.lock().unwrap().insert(request_id, cancel.clone());

        let result = tokio::select! {
            r = self.do_execute(request) => r,
            _ = cancel.cancelled() => {
                Err(CatcherError::Internal("request cancelled".into()))
            }
            _ = self.cancel_token.lock().unwrap().cancelled() => {
                Err(CatcherError::Internal("request cancelled".into()))
            }
        };

        self.pending_requests.lock().unwrap().remove(&request_id);
        (request_id, result)
    }

    /// 取消单个飞行请求
    pub fn cancel_request(&self, request_id: u64) -> bool {
        if let Some(token) = self.pending_requests.lock().unwrap().get(&request_id) {
            token.cancel();
            true
        } else {
            false
        }
    }

    /// 取消所有飞行请求（保持原有语义）
    pub fn cancel_all(&self) {
        // 取消全局 token
        let mut token = self.cancel_token.lock().unwrap();
        token.cancel();
        *token = CancellationToken::new();
        // 同时取消所有 per-request token
        let pending: Vec<CancellationToken> =
            self.pending_requests.lock().unwrap().drain().map(|(_, t)| t).collect();
        for t in &pending {
            t.cancel();
        }
    }
}
```

**FFI 改动**：

```rust
// execute 返回 request_id
#[no_mangle]
pub unsafe extern "C" fn catcher_http_execute(
    handle: *mut c_void,
    method: FfiString,
    url: FfiString,
    body: *const u8, body_len: usize,
    content_type: FfiString,
    headers_json: *const c_char,
    timeout_ms: u32,
    callback: EventCallback,
    user_data: *mut c_void,
) -> u64  // 返回 request_id (0 = error)

/// 取消单个请求
#[no_mangle]
pub unsafe extern "C" fn catcher_http_cancel_request(
    handle: *mut c_void,
    request_id: u64,
) -> i32  // 0=成功, -1=未找到
```

#### 向后兼容

- `catcher_http_client_cancel_all` 保持不变，内部逻辑升级为同时清理 per-request tokens
- `catcher_http_execute` 返回值从 `void` → `u64`，**C ABI 签名不兼容变更**。Dart 侧需同步更新 typedef：`Void Function(...)` → `Uint64 Function(...)`
- 参数列表不变，仅返回值类型变化。原有不关心 request_id 的调用方忽略返回值即可

#### 验收标准

- [ ] `execute()` 返回唯一 `request_id`
- [ ] `cancel_request(id)` 只取消指定请求，其他飞行请求不受影响
- [ ] `cancel_all()` 取消所有飞行请求（保持原行为）
- [ ] 已取消请求的 callback 收到 error JSON `{"type":"cancelled","request_id":42}`
- [ ] Dart FFI typedef 已验证从 `Void` → `Uint64` 变更后正常编译运行
- [ ] Dart 侧 `client.get('/tab1')` + `client.get('/tab2')` → `client.cancel(tab1Request)` 只取消 tab1

> 测试方案：[../test/native-gap-test-design.md](../test/native-gap-test-design.md) — N-03 节

---

### N-04: 网络质量实时事件推送

#### 现状

- **Rust 层**: `NetworkQualityEvaluator` 有同步查询方法 `rtt_snapshot()` / `evaluate()`
- **FFI 层**:
  - `catcher_evaluate_quality(host, callback)` — 一次性触发测量 + 回调
  - `catcher_quality_history()` — 一次性 JSON 查询
- **缺失**: 无持久订阅机制，每次需要质量数据时需主动轮询

#### 使用场景

```
场景 A: 视频播放器自适应码率
  质量从 Good → Poor 时自动切换到低码率 → 需要即时通知

场景 B: 上传队列管理
  质量恢复到 Good 时自动恢复上传 → 需要即时通知

场景 C: 运维仪表盘
  实时展示当前网络质量 + 历史趋势 → 需要持续推送
```

当前只能用 Timer 每隔 N 秒调用 `catcher_quality_history()` 轮询。这在以下场景有问题：
- 质量突变时响应延迟 = Timer interval
- 高频轮询浪费资源
- 无法获取趋势变化事件（如 `improving` / `degrading`）

#### 修复方案

**新增订阅式 C ABI 符号**：

```rust
/// 订阅网络质量变化事件。
/// 内部启动后台 tokio task，每 `interval_ms` 测量一次，
/// 质量等级变化时触发 callback。首次订阅立即测量并回调。
///
/// callback event_type="quality_change", event_data JSON:
/// {
///   "level": "good",           // 当前等级
///   "previous_level": "fair",  // 上一次等级（首次为 null）
///   "trend": "improving",      // "improving"|"degrading"|"stable"|"unknown"
///   "avg_rtt_ms": 85,
///   "jitter_ms": 12,
///   "sample_count": 5
/// }
///
/// 返回订阅句柄，通过 catcher_quality_unsubscribe 取消。
#[no_mangle]
pub unsafe extern "C" fn catcher_quality_subscribe(
    host: FfiString,
    interval_ms: u32,
    callback: EventCallback,
    user_data: *mut c_void,
) -> *mut c_void  // subscription handle
```

```rust
/// 取消质量订阅
#[no_mangle]
pub unsafe extern "C" fn catcher_quality_unsubscribe(
    sub_handle: *mut c_void,
)
```

**内部实现要点**：

```rust
struct QualitySubscription {
    host: String,
    interval_ms: u64,
    callback: EventCallback,
    user_data: usize,
    cancel_tx: tokio::sync::watch::Sender<bool>,
    _task: tokio::task::JoinHandle<()>,
}

impl QualitySubscription {
    fn start(
        host: String,
        interval_ms: u64,
        callback: EventCallback,
        user_data: usize,
    ) -> Self {
        let (cancel_tx, cancel_rx) = tokio::sync::watch::channel(false);
        let mut evaluator = NetworkQualityEvaluator::new(50);
        let host_clone = host.clone();

        let task = tokio::spawn(async move {
            let mut interval = tokio::time::interval(
                Duration::from_millis(interval_ms)
            );
            let mut previous_level: Option<NetworkQualityLevel> = None;

            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        if let Ok(_) = evaluator.measure_http_rtt(&host_clone, "/").await {
                            let result = evaluator.evaluate();
                            let trend = compute_trend(&result.level, &previous_level);
                            // 仅在质量等级变化时触发回调
                            if previous_level != Some(result.level) || previous_level.is_none() {
                                let json = build_quality_event_json(&result, &previous_level, &trend);
                                invoke_callback(callback, "quality_change", &json, user_data);
                                previous_level = Some(result.level);
                            }
                        }
                    }
                    _ = cancel_rx.changed() => {
                        if *cancel_rx.borrow() { break; }
                    }
                }
            }
        });

        Self { host, interval_ms, callback, user_data, cancel_tx, _task: task }
    }
}
```

#### 与现有 API 的关系

| API | 模式 | 用途 |
|-----|------|------|
| `catcher_evaluate_quality` | 一次性触发 | 手动单次检测 |
| `catcher_quality_history` | 同步查询 | 拉取滑动窗口历史 |
| **`catcher_quality_subscribe`** | 持久推送 | **质量变化时自动通知** |

三者共存，互不取代。

#### 验收标准

- [ ] `quality_subscribe('https://example.com', 5000, callback)` 启动后台测量
- [ ] 质量等级变化时触发 callback（Excellent→Good, Good→Poor 等）
- [ ] callback JSON 包含 `level` / `previous_level` / `trend` / `avg_rtt_ms` / `jitter_ms`
- [ ] `quality_unsubscribe(handle)` 停止后台 task
- [ ] 多个订阅者独立工作，互不干扰
- [ ] 每个订阅者拥有独立的 `NetworkQualityEvaluator` 和窗口数据（可选优化：同 host 共享 evaluator 避免重复测量）
- [ ] Dart 侧可通过 `CatcherQuality.subscribe()` 获取 `Stream<NetworkQualityEvent>`

> 测试方案：[../test/native-gap-test-design.md](../test/native-gap-test-design.md) — N-04 节

---

## 工作量估算

| # | Issue | 工作量 | Rust 核心 | FFI 层 | Dart 绑定 | 风险 |
|---|-------|:------:|:---------:|:------:|:---------:|------|
| N-01 | Multipart (方案A) | S | — | — | Dart encoder ~100行 | 低 |
| N-01 | Multipart (方案B) | M | MultipartBuilder ~200行 | 5 C ABI 符号 | Dart 封装 ~50行 | 低 |
| N-02 | 流式下载 | M | `execute_stream` ~80行 | 1 C ABI 符号 | Dart Stream 封装 ~80行 | reqwest `bytes_stream` API 稳定 |
| N-03 | Per-request cancel | M | per-request token map ~60行 | 1 C ABI 符号 + execute 返回值变更 | Dart cancel(id) ~50行 | execute 返回值变更需确认 Dart FFI 兼容 |
| N-04 | Quality push | M | subscribe/unsubscribe ~120行 | 2 C ABI 符号 | Dart Stream 封装 ~60行 | 多个订阅者内存管理 |

---

## 实施路线图

```
Phase 1 — N-03: Per-request cancel (P2)
  影响最广，列表页场景常用。实现后立即提升 Dart 端可用性。

Phase 2 — N-02: 流式下载 (P2)
  大文件下载场景刚需。可与 N-03 并行开发。

Phase 3 — N-04: Quality push (P3)
  增强型功能，Timer 轮询在 P2 阶段可工作。

Phase 4 — N-01: Multipart 方案 A (P2)
  Dart 侧 encoder 低风险。方案 B 视需求决定。
```

---

## 与现有 Issue 的关系

| 现有 Issue | 状态 | 本次缺口 |
|-----------|:----:|---------|
| FFI-04 "取消/Abort 机制" | ✅ `cancelAll` | N-03 单请求级 cancel |
| FFI-09 "Network quality sliding window" | ✅ `qualityHistory()` | N-04 实时推送事件 |
| （无对应） | — | N-01 Multipart 上传 |
| （无对应） | — | N-02 流式下载 |
| FFI-01/FFI-02 SSE headers | ✅ | 已覆盖，不在此清单 |
