# 15 — FFI 分层策略：边界职责划分

> 设计文档 · 部分实现
> 配套阅读：[09-ffi.md](./09-ffi.md)（FFI 接口契约）、[../arch-ts/09-interceptors.md](../arch-ts/09-interceptors.md)（TS 拦截器）

---

## 核心原则：FFI 是通道，不是业务层

```
┌──────────────────────────────────────────────────────┐
│  JavaScript / Dart 调用方                             │
│                                                      │
│  ✅ 拦截器 (interceptors)                             │
│  ✅ 请求取消 (AbortSignal / CancelToken)               │
│  ✅ query 序列化 (params → URL)                       │
│  ✅ 配置合并 (instance + per-request)                  │
│  ✅ 错误包装 (CatcherHttpError)                        │
│  ✅ 响应类型选择 (json/text/bytes 解析)                │
│  ✅ 上传/下载进度回调 (JS 侧聚合)                      │
├──────────────────────────────────────────────────────┤
│  FFI 边界 — 仅传递最终确定的请求/响应数据               │
│                                                      │
│  传入: { method, url, headers, body, timeout,         │
│          cancel_token, on_progress }                  │
│  传出: { status, headers, body, elapsed_ms }          │
├──────────────────────────────────────────────────────┤
│  Rust 核心层                                          │
│                                                      │
│  ✅ 传输层 (reqwest / tokio-tungstenite)               │
│  ✅ 连接池 (pool_max_idle, pool_idle_timeout)          │
│  ✅ DNS 解析 (hickory-dns + host_mapping)              │
│  ✅ TLS (native-tls / rustls)                         │
│  ✅ 重试 (reqwest-retry + ExponentialBackoff)          │
│  ✅ 熔断 (cockatiel 同级逻辑)                          │
│  ✅ 自适应超时 (RTT 采样)                              │
│  ✅ 指标收集 (MetricsCollector)                        │
│  ✅ 进度通知 (回调至 FFI)                              │
└──────────────────────────────────────────────────────┘
```

---

## 为什么不在 FFI 做拦截器

### 一图胜千言

```
错误方案：每个拦截器跨 FFI

  JS call
    → napi::call_async
      → Rust 收到请求
        → napi::ThreadsafeFunction::call  ← 跨 FFI 回 JS 执行拦截器1
          → JS interceptor1 执行
        → napi::ThreadsafeFunction::call  ← 又跨 FFI 回 JS 执行拦截器2
          → JS interceptor2 执行
        → reqwest 发起请求
        → napi::ThreadsafeFunction::call  ← 又跨 FFI 回 JS 执行响应拦截器
      → Rust 返回结果
    → JS 收到响应

  一次请求 = (1 + 拦截器数量 × 2) 次跨界


正确方案：拦截器全在 JS 侧

  JS call
    → JS interceptor1 执行         ← 零跨界
    → JS interceptor2 执行         ← 零跨界
    → napi::call_async (1 次)     ← 唯一一次跨界
      → Rust 收到最终请求
      → reqwest 发起请求
      → Rust 返回结果
    → JS response interceptor     ← 零跨界
  → JS 调用方拿到结果

  一次请求 = 1 次跨界
```

### 量化分析

以 3 个请求拦截器 + 2 个响应拦截器为例：

| 指标 | FFI 拦截器 | TS 拦截器 |
|------|-----------|----------|
| 跨界次数 | 1 + 3 + 2 = 6 | 1 |
| 序列化开销 | 每拦截器序列化完整 request config | 仅在 FFI 边界序列化一次 |
| 内存分配 | 每次跨界分配 napi ref / buffer | 仅在 FFI 边界分配 |
| GC 压力 | 每次回调创建临时 JS 对象 | 无额外压力 |
| 异步开销 | 每次回调挂起 tokio task | JS event loop 原生调度 |

**结论**：ts 拦截器的本质是 JS 对象 → JS 对象的纯内存操作，放在 JS 侧是零开销。跨 FFI 做同样的事，性能开销成倍增长。

---

## FFI 的正确职责

FFI 是一个**数据搬运通道**，不是中间件层。它只做三件事：

### 1. 类型转换

```rust
// napi-rs: JS 类型 → Rust 类型 (零拷贝优先)
#[napi(object)]
pub struct JsHttpRequest {
    pub method: String,
    pub url: String,
    pub headers: HashMap<String, String>,
    pub body: Option<Buffer>,       // Buffer → Vec<u8> 零拷贝
    pub timeout_ms: Option<u32>,
    pub cancel_signal: Option<...>, // AbortSignal → CancellationToken
}

#[napi(object)]
pub struct JsHttpResponse {
    pub status: u16,
    pub headers: HashMap<String, String>,
    pub body: Buffer,               // Vec<u8> → Buffer 零拷贝
    pub elapsed_ms: u64,
}
```

### 2. 取消传播

```rust
// JS AbortController.signal → Rust CancellationToken
// napi-rs ThreadsafeFunction 监听 abort 事件
pub struct CancelBridge {
    token: tokio_util::sync::CancellationToken,
}

impl CancelBridge {
    pub fn from_abort_signal(env: &Env, signal: JsAbortSignal) -> Self {
        let token = CancellationToken::new();
        let child = token.child_token();
        // 当 JS 侧 signal.abort() 触发时，cancel child token
        // Rust 侧 select! { _ = child.cancelled() => ... }
        Self { token }
    }
}
```

### 3. 进度回调

```rust
// 上传/下载进度 — 从 Rust 侧定期回调到 JS
// 这是 FFI 层少数合理的"反向调用"场景
#[napi(ts_args_type = "callback: (event: ProgressEvent) => void")]
pub fn on_upload_progress(&self, callback: JsFunction) {
    let tsfn: ThreadsafeFunction<ProgressEvent, _> =
        callback.create_threadsafe_function(0, |mut ctx| {
            let event = ctx.value;
            ctx.env.create_int32(event.loaded)...
            Ok(vec![...])
        })?;
    // 存储 tsfn，在 reqwest 数据发送时调用
}
```

---

## napi-rs 薄封装模式

### 反面：napi 层做业务逻辑

```typescript
// ❌ napi-http 直接暴露给用户
import { HttpClient } from '@eric8810/catcher-napi-http'

const client = new HttpClient({ baseURL: '...' })
// 问题：没有拦截器、没有信号、没有 query 参数、没有错误包装
// 这些都是 JS 侧的职责，napi 层不该管
```

### 正面：TS 薄封装包一层

```typescript
// ✅ @eric8810/catcher-http 内部使用 napi 作为底层传输
// packages/catcher-http-ts/src/http/native-client.ts

import { HttpClient as NativeHttpClient } from '@eric8810/catcher-napi-http'

export function createNativeHttpClient(config: HttpClientConfig): IHttpClient {
  const native = new NativeHttpClient({
    base_url: config.baseURL,
    connect_timeout_ms: typeof config.timeout === 'number'
      ? config.timeout : config.timeout?.connect ?? 30_000,
    // ...
  })

  const interceptors = {
    request: createInterceptorManager<RequestConfig>(),
    response: createInterceptorManager<HttpResponse>(),
  }

  return {
    async get(url, reqConfig) {
      // 1. 合并配置
      const merged = mergeConfig(config, reqConfig)

      // 2. 运行请求拦截器 (TS 侧，零跨界)
      let request = { method: 'GET', url, headers: merged.headers, ... }
      request = await runRequestInterceptors(interceptors.request, request)

      // 3. 处理 query 参数 (TS 侧)
      const fullUrl = buildUrl(request.url, merged.params)

      // 4. 唯一一次跨界
      const rawResp = await native.execute({
        method: 'GET',
        url: fullUrl,
        headers: request.headers,
        signal: reqConfig?.signal,    // ← AbortSignal 直接传给 napi
        timeout_ms: merged.timeout,
      })

      // 5. 响应类型处理 (TS 侧)
      const body = merged.responseType === 'text'
        ? rawResp.body.toString('utf-8')
        : JSON.parse(rawResp.body.toString('utf-8'))

      // 6. 运行响应拦截器 (TS 侧，零跨界)
      const response = { status: rawResp.status, headers: rawResp.headers, body }
      return runResponseInterceptors(interceptors.response, response)
    },
    // ...
    interceptors,
  }
}
```

### 架构图

```
@eric8810/catcher-http (用户直接安装)
 ├── interceptors     ← TS 纯逻辑
 ├── query params     ← TS 纯逻辑
 ├── config merge     ← TS 纯逻辑
 ├── error wrapping   ← TS 纯逻辑
 ├── response parsing ← TS 纯逻辑
 ├── retry            ← p-retry (TS)
 ├── circuit breaker  ← cockatiel (TS)
 ├── priority queue   ← p-queue (TS)
 └── native client    ← @eric8810/catcher-napi-http (唯一 FFI 调用点)

@eric8810/catcher-napi-http (薄封装，用户不直接安装)
 └── napi-rs bindings → catcher-http crate (Rust)
```

---

## 哪些功能适合放 Rust / FFI 层

| 功能 | 放 Rust | 理由 |
|------|---------|------|
| TLS 握手 | ✅ | openssl/rustls 原生性能 |
| DNS 解析 + host_mapping | ✅ | hickory-dns 自定义解析器 |
| 连接池管理 | ✅ | reqwest 内置 hyper-util pool |
| TCP keep-alive | ✅ | 系统调用级别 |
| HTTP/2 多路复用 | ✅ | reqwest/h2 原生支持 |
| 重试（退避计算） | ✅ | 跨语言一致的退避策略 |
| 熔断 | ⚠️ 都可 | 简单状态机，Rust 实现可跨 FFI 暴露状态 |
| 自适应超时 | ✅ | RTT 采样在传输层最准确 |
| 进度回调 | ✅ 采集 | Rust 采集字节数 → 通过 FFI 推 JS |
| 指标收集 | ✅ | 原生性能 |
| msgpack 编解码 | ✅ | 2-4x faster than JS msgpackr |

| 功能 | 不放 Rust | 理由 |
|------|----------|------|
| 拦截器 | ❌ | 高频跨界，无性能收益 |
| 请求取消 (业务侧) | ❌ | JS AbortController 是标准 Web API |
| query 序列化 | ❌ | 纯字符串操作，无性能收益 |
| config 合并 | ❌ | 纯对象操作 |
| JSON 解析 | ❌ | JS 引擎内置 JSON.parse 比 FFI 回来快 |
| auth token 刷新 | ❌ | 业务逻辑，频繁变更 |

---

## 进度回调：FFI 反向调用的正确姿势

进度是少数需要 Rust → JS 方向通信的场景。但也要限制频率：

```rust
// Rust 侧：限制回调频率，避免 FFI 风暴
pub struct ProgressReporter {
    tsfn: ThreadsafeFunction<ProgressEvent, ErrorStrategy::Fatal>,
    last_report: Instant,
    min_interval: Duration,  // 默认 100ms，避免高频回调
}

impl ProgressReporter {
    pub fn report(&mut self, loaded: u64, total: Option<u64>) {
        if self.last_report.elapsed() < self.min_interval {
            return; // 节流
        }
        self.last_report = Instant::now();
        let event = ProgressEvent { loaded, total };
        self.tsfn.call(Ok(event), ThreadsafeFunctionCallMode::NonBlocking);
    }
}
```

```typescript
// JS 侧：标准 Web API 风格
const client = createHttpClient({ baseURL: '...' })
await client.post('/upload', formData, {
  onUploadProgress: (e) => console.log(`${e.loaded}/${e.total}`),
})
```

---

## 取消机制：边界处的 CancellationToken 桥接

### 设计原则

取消信号需要从 JS/Dart 侧传播到 Rust futures。方案：

```
JS/Dart                  FFI 边界                    Rust
  │                         │                         │
  │── cancelAll() ─────────▶│                         │
  │                         │── cancel_token.cancel()─▶│
  │                         │                         │── select! {
  │                         │                         │     result = reqwest
  │                         │                         │     _ = token.cancelled()
  │                         │                         │   }
```

### Rust 层实现

```rust
use tokio_util::sync::CancellationToken;

pub struct HttpTransport {
    client: Client,
    config: HttpClientConfig,
    cancel_token: CancellationToken,  // 根 token
    // ...
}

impl HttpTransport {
    pub async fn execute(&self, request: HttpRequest) -> Result<HttpResponse, CatcherError> {
        let cancel = self.cancel_token.child_token();
        tokio::select! {
            result = self.execute_inner(request) => result,
            _ = cancel.cancelled() => Err(CatcherError::Cancelled),
        }
    }

    pub fn cancel_all(&self) {
        self.cancel_token.cancel();
        // 重置 token 以允许后续请求
        // self.cancel_token = CancellationToken::new();
    }
}
```

### C ABI 暴露

```rust
#[no_mangle]
pub extern "C" fn catcher_http_client_cancel_all(
    handle: *mut c_void,
)
```

### 为什么不在 JS 侧做取消

JS 的 `AbortController.abort()` 只能取消 JS 侧的 Promise chain，无法传播到已进入 FFI 的 Rust future。一旦请求 body 已跨过 FFI 边界传送，必须由 Rust 侧的 CancellationToken 中断飞行请求。

**正确流程**：JS `AbortController` → napi `ThreadsafeFunction` 监听 abort → Rust `CancellationToken.cancel()` → `select!` 返回 `Cancelled`。

---

## SSE 流式响应的 FFI 分层

### 场景分类

| 场景 | 模式 | 适用方法 | 连接生命周期 |
|------|------|---------|------------|
| OpenAI streaming API | POST SSE | `catcher_sse_stream` | 一次性请求-响应 |
| Anthropic streaming API | POST SSE | `catcher_sse_stream` | 一次性请求-响应 |
| 持久事件订阅 | GET SSE | `catcher_sse_connect` | 长连接 + 自动重连 |
| 服务端推送通知 | GET SSE | `catcher_sse_connect` | 长连接 + 自动重连 |

### 为什么 SSE 需要 Rust 侧实现

1. SSE 行解析需要理解 `data:`/`event:`/`id:`/`retry:` 协议
2. 自动重连 + `Last-Event-ID` 需要持久状态
3. `SseClient` 的 cancel 通道需要在 Rust 侧维护
4. 避免每行 SSE 跨 FFI 回调的性能损耗（只在完整行完成时回调）

### SSE FFI 回调频率控制

```rust
// 限制回调频率 —— SSE 行可能非常密集
pub struct SseReporter {
    last_report: Instant,
    min_interval: Duration,  // 默认 16ms (~60fps)
}
```

---

## 总结

```
┌──────────────────────────────────────────────┐
│  思考方式                                     │
│                                              │
│  "这个功能需要知道 HTTP/wire format 吗？"      │
│    需要   → Rust 层                          │
│    不需要 → TS/Dart 层                        │
│                                              │
│  "这个功能每次请求都执行吗？"                   │
│    是 → 尽可能放在 FFI 同侧，减少跨界           │
│    否 → 无所谓                               │
│                                              │
│  "这个功能需要回调到 JS/Dart 吗？"              │
│    是 → 限制频率，批量推送                     │
│    否 → Rust 独立完成                         │
│                                              │
│  "Cancel/Abort 需要传播到 Rust 吗？"            │
│    是 → CancellationToken 桥接               │
│    否 → JS 侧 Promise.reject 即可             │
└──────────────────────────────────────────────┘
```

**FFI 边界是性能热路径，每多一次跨界就多一次序列化 + 多一次 napi 锁 + 多一次 GC 压力。设计时始终追问：这个数据真的需要跨 FFI 吗？**
