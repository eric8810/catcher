# 10 — Server-Sent Events (SSE) 客户端

> 新增模块 · 设计文档
> 本文档同时覆盖 TypeScript 和 Rust 两侧的 SSE 设计

## 需求背景

AI 大模型 API（OpenAI / Anthropic / Google Gemini）的 chat completion 普遍采用 SSE 作为流式输出协议。SSE 在 AI 场景中优于 WebSocket 的关键点：

| 维度 | SSE | WebSocket |
|------|-----|-----------|
| 协议 | 基于 HTTP，无需升级 | 独立协议升级 |
| 方向 | 服务端 → 客户端（单向） | 双向 |
| 断点续传 | `Last-Event-ID` 原生支持 | 需自行实现 |
| AI 场景匹配 | 请求 → 流式响应，天然契合 | 过度设计 |
| 行业标准 | OpenAI / Anthropic / Gemini 全部采用 | 少数场景使用 |

---

## 设计理念：fetch + stream 体验

SSE 不是交互协议——没有握手、没有状态机、没有双向协商。它本质上就是：

```
HTTP 请求 → 服务端慢慢吐文本 → 读完了就完了
```

因此 Catcher 的 SSE 模块定位为：**一个会重连的文本流，不介入业务解析。**

### 用户的体验目标

```typescript
// 裸 fetch 流式读取
const res = await fetch(url, { method: 'POST', body, headers })
const reader = res.body!.getReader()
const decoder = new TextDecoder()
while (true) {
  const { done, value } = await reader.read()
  if (done) break
  const text = decoder.decode(value, { stream: true })  // 可能半行
  // 自己处理分片、自己切行...
}

// Catcher SSE — 同样的心智模型，但没有痛点
const stream = createSSEStream({ url, method: 'POST', body, headers })
for await (const line of stream) {
  // 完整的一行，没有半行分片问题
  // 没有心跳噪音，没有空行噪音
}
```

### 职责划分

```
Catcher 的本分（库静默处理）：            用户的事（用户自己处理）：
├── HTTP 连接管理                         ├── 读到内容行后怎么处理
├── chunk → 完整行缓冲                    ├── data: 前缀剥离
├── 自动重连 + 指数退避                   ├── [DONE] 判断
├── Last-Event-ID 静默提取 + 重连携带     ├── JSON.parse
├── retry: 间隔静默提取                   ├── event: 路由
├── : comment 心跳静默吃掉                ├── 任何业务逻辑
├── 空行静默吃掉
├── 连接状态跟踪
├── timeout / circuit breaker
└── AbortSignal 支持
```

### 库静默处理的行（不吐给用户）

| 服务端发送 | 库的行为 |
|-----------|---------|
| `: keepalive` | 静默吃掉，重置 idle timer |
| （空行） | 静默吃掉（SSE 事件分隔符） |
| `id: msg_003` | 静默记录 `lastEventId`，重连时自动携带 |
| `retry: 5000` | 静默调整重连间隔 |

这些是 SSE 协议自身的控制信令，跟 HTTP 的 `Content-Length`、`Transfer-Encoding` 一样——库处理掉，用户不需要知道。

### 吐给用户的行（内容行，原样输出）

| 服务端发送 | 用户拿到的 |
|-----------|-----------|
| `event: message_start` | `"event: message_start"` |
| `data: {"type":"start",...}` | `"data: {\"type\":\"start\",...}"` |
| `data: Hello` | `"data: Hello"` |
| `data:  world` | `"data:  world"` |
| `data: [DONE]` | `"data: [DONE]"` |

用户拿到的是 **内容行（content lines）**——所有以 `event:`、`data:` 开头的内容行，原样输出，不做任何结构化或解析。

---

## API

### `createSSEStream` — 一次性流式请求

```typescript
function createSSEStream(options: SSEStreamOptions): SSEStream
```

- 用途：AI 流式对话、一次性数据拉取
- 特点：`AsyncIterable<string>`，消费完即结束，不自动重连
- 退出方式：`for await` 自然结束 / `break` / `AbortSignal.abort()`

### `createSSEClient` — 长连接 + 自动重连

```typescript
function createSSEClient(options: SSEClientOptions): SSEClient
```

- 用途：服务端推送、实时通知、监控仪表盘
- 特点：`AsyncIterable<string>`，连接断开后自动重连，携带 `Last-Event-ID`
- 退出方式：`client.close()` / `AbortSignal.abort()`

---

## 使用示例

### AI 流式对话

```typescript
import { createSSEStream } from '@eric8810/catcher-http'

const stream = createSSEStream({
  url: 'https://api.openai.com/v1/chat/completions',
  method: 'POST',
  headers: { Authorization: `Bearer ${apiKey}` },
  body: { model: 'gpt-4', messages: [{ role: 'user', content: 'Hello' }], stream: true },
})

for await (const line of stream) {
  if (!line.startsWith('data:')) continue
  const payload = line.startsWith('data: ') ? line.slice(6) : line.slice(5)
  if (payload === '[DONE]') break
  const chunk = JSON.parse(payload)
  process.stdout.write(chunk.choices[0]?.delta?.content ?? '')
}
// 循环结束 = 连接关闭，无需手动清理
```

### 长连接推送

```typescript
import { createSSEClient } from '@eric8810/catcher-http'

const client = createSSEClient({
  url: 'https://api.example.com/events',
  headers: { Authorization: 'Bearer xxx' },
  reconnect: { initialDelay: 1000, maxDelay: 30_000 },
})

for await (const line of client) {
  if (line.startsWith('data: ')) {
    console.log(line.slice(6))
  }
}

// 永不主动断开。如需断开：
client.close()
```

### 中断请求

```typescript
const controller = new AbortController()
const stream = createSSEStream({
  url: 'https://api.example.com/stream',
  signal: controller.signal,
})

setTimeout(() => controller.abort(), 5000)  // 5 秒后中断
for await (const line of stream) {
  console.log(line)  // abort 后 for await 自然结束
}
```

### Rust

```rust
use catcher_http::sse::{SseStream, SseClientConfig};

let config = SseClientConfig {
    url: "https://api.openai.com/v1/chat/completions".into(),
    method: SseMethod::POST,
    headers: HashMap::from([("Authorization".into(), format!("Bearer {}", api_key))]),
    body: Some(serde_json::to_string(&body)?),
    ..Default::default()
};
let mut stream = SseStream::connect(config).await?;
while let Some(line) = stream.next().await {
    let line = line?;
    if let Some(payload) = line.strip_prefix("data: ") {
        if payload == "[DONE]" { break; }
        let chunk: Value = serde_json::from_str(payload)?;
        print!("{}", chunk["choices"][0]["delta"]["content"]);
    }
}
```

---

## 自动关闭连接

| 场景 | 行为 |
|------|------|
| 服务端正常关闭连接 | `for await` 自然结束（类似 fetch stream done） |
| 服务端返回 204 | 不再重连（SSE 规范），`for await` 结束 |
| 用户 `break` 出循环 | 库检测到消费者离开，关闭连接 |
| `AbortSignal.abort()` | 立即中断，`for await` 结束 |
| 超时 | throw `SSETimeoutError` |

`createSSEStream` 用户不需要手动关闭任何东西——`for await` 结束 = 连接关闭。

`createSSEClient` 因为可能永远不结束，需要 `client.close()` 或 `AbortSignal` 来主动断开。

---

## 全平台支持

```
                @eric8810/catcher-core (zero deps)
                /         |         \
               /          |          \
  @eric8810/catcher-http  @eric8810/catcher-ws  @eric8810/catcher-web
  (axios + SSE, Node)  (ws, msgpack) (fetch + SSE, Browser)
       │                    │
       └──── napi-rs ───────┤
              ↑              │
       catcher-http (Rust)   │
       (reqwest + SSE)       │
                            │
       catcher-ffi ─────────┤
       (cdylib, SSE FFI)    │
                            │
       catcher-uniffi ──────┘
       (Swift/Kotlin, SSE)
```

| 平台 | 包 | 底层 | 状态 |
|------|---|------|------|
| Node.js (TS) | `@eric8810/catcher-http` | `fetch` + `ReadableStream` | 设计中 |
| Browser | `@eric8810/catcher-web` | `fetch` + `ReadableStream` | 设计中 |
| Rust | `catcher-http` | `reqwest` + `tokio_stream` | 设计中 |
| Node.js (native) | `@eric8810/catcher-napi-http` | napi-rs 调用 Rust SSE | 设计中 |
| Flutter | `catcher-ffi` | dart:ffi 调用 Rust SSE | 设计中 |
| Swift/Kotlin | `catcher-uniffi` | UniFFI 调用 Rust SSE | 设计中 |

---

## 类型定义

### TS 类型（catcher-core-ts/src/types.ts）

```typescript
// === SSE ===

export interface SSEStreamOptions {
  /** SSE 端点 URL */
  url: string
  /** HTTP 方法，默认 'GET'。AI 场景通常用 'POST' */
  method?: 'GET' | 'POST'
  /** 请求 headers（如 Authorization） */
  headers?: Record<string, string>
  /** 请求 body（POST 场景）。对象会自动 JSON.stringify */
  body?: string | Record<string, unknown>
  /** 请求超时 ms，默认 30_000 */
  timeout?: number
  /** 中断信号 */
  signal?: AbortSignal
}

export interface SSEClientOptions extends SSEStreamOptions {
  /** 自动重连配置 */
  reconnect?: {
    enabled?: boolean
    maxRetries?: number
    initialDelay?: number
    maxDelay?: number
    backoffMultiplier?: number
  }
  /** 熔断配置 */
  circuitBreaker?: { failureThreshold: number; resetTimeout: number }
}

/**
 * SSE 内容流行 — yield 内容行，静默过滤控制行（心跳/空行）
 *
 * 库静默处理：
 * - `id:` → 记录 lastEventId，用于重连
 * - `retry:` → 调整重连间隔
 * - `: comment` → 心跳，吃掉
 * - 空行 → 事件分隔符，吃掉
 * - chunk 缓冲 → 保证每次 yield 完整一行
 */
export interface SSEStream extends AsyncIterable<string> {
  /** 库从 id: 行静默提取，用于重连时自动携带 Last-Event-ID */
  readonly lastEventId: string
}

export interface SSEClient extends AsyncIterable<string> {
  readonly readyState: 'CONNECTING' | 'OPEN' | 'CLOSED'
  readonly lastEventId: string
  /** 主动关闭连接（仅 createSSEClient 需要） */
  close(): void
}

export interface SSETimeoutError extends Error {
  readonly type: 'SSE_TIMEOUT'
}
```

### Rust 类型（catcher-core/src/types/sse.rs）

```rust
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// SSE 客户端配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SseClientConfig {
    pub url: String,
    #[serde(default)]
    pub method: SseMethod,
    #[serde(default)]
    pub headers: HashMap<String, String>,
    /// 请求 body（JSON 字符串，调用方自行序列化）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reconnect: Option<SseReconnectConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry: Option<RetryConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub circuit_breaker: Option<CircuitBreakerConfig>,
    #[serde(default = "default_timeout")]
    pub timeout_ms: u64,
}

impl Default for SseClientConfig {
    fn default() -> Self {
        Self {
            url: String::new(),
            method: SseMethod::default(),
            headers: HashMap::new(),
            body: None,
            reconnect: None,
            retry: None,
            circuit_breaker: None,
            timeout_ms: 30_000,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub enum SseMethod { #[default] GET, POST }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SseReconnectConfig {
    pub enabled: bool,
    pub max_retries: u32,
    pub initial_delay_ms: u64,
    pub max_delay_ms: u64,
    pub backoff_multiplier: f64,
}

fn default_timeout() -> u64 { 30_000 }
```

---

## 内部实现

### 核心管线

```
服务端字节流
    │
    ▼
┌──────────────────────────────────┐
│ Chunk Buffer                     │
│ 按 \n 边界切行                    │
│ 保证每次 yield 完整一行           │
│ （strip \r，容错 Windows 换行）   │
└──────────────┬───────────────────┘
               │  完整行
               ▼
┌──────────────────────────────────┐
│ Line Router                      │
│                                   │
│  空行           → 吃掉            │
│  : comment      → 吃掉 + 重置idle │
│  id: xxx        → 记录 lastEventId│
│  retry: xxx     → 调整重连间隔    │
│  其他内容行      → yield 给用户    │
└──────────────────────────────────┘
               │
               ▼
         string（原样输出）
```

没有策略、没有分类器、没有事件组装器。就是一个带规则的行过滤器。

### 新增源文件

```
packages/catcher-http/src/          packages/catcher-http-ts/src/
├── sse/                             ├── sse/
│   ├── mod.rs                       │   ├── router.ts    # 行路由（~80 行）
│   ├── router.rs    # 行路由 (~80)  │   ├── stream.ts    # SSEStream (~100 行)
│   ├── stream.rs    # SseStream     │   ├── client.ts    # SSEClient (~150 行)
│   └── client.rs    # SseClient     │   └── index.ts
└── lib.rs           # pub mod sse   └── index.ts

packages/catcher-web/src/
├── sse/
│   ├── router.ts    # 同 TS 版
│   ├── stream.ts    # 浏览器版
│   ├── client.ts    # 浏览器版（无 SharedAgent）
│   └── index.ts
└── index.ts

packages/catcher-core/src/types/
├── sse.rs           # SseClientConfig 等类型
└── mod.rs           # pub mod sse
```

注意：**删除了 `parser.rs` 和 `strategies.ts`**。不再需要解析器和策略。

### Rust 行路由（router.rs）

```rust
// packages/catcher-http/src/sse/router.rs

/// 行路由结果
pub enum RouteAction {
    /// 内容行，yield 给用户
    Yield(String),
    /// 控制行，静默处理（心跳/空行）
    Silent,
    /// id: 行，静默记录 last_event_id
    SetLastEventId(String),
    /// retry: 行，静默调整重连间隔
    SetRetry(u64),
}

/// 路由一行 SSE 文本
pub fn route_line(line: &str) -> RouteAction {
    if line.is_empty() {
        return RouteAction::Silent;
    }
    if line.starts_with(':') {
        // 心跳 / 注释，静默吃掉
        return RouteAction::Silent;
    }
    if let Some(id) = line.strip_prefix("id:") {
        return RouteAction::SetLastEventId(id.trim_start().to_string());
    }
    if let Some(retry) = line.strip_prefix("retry:") {
        if let Ok(ms) = retry.trim().parse::<u64>() {
            return RouteAction::SetRetry(ms);
        }
    }
    // 其他所有行（data:, event:, 或任意内容）原样 yield
    RouteAction::Yield(line.to_string())
}
```

### SSE 流（stream.rs）

```rust
// packages/catcher-http/src/sse/stream.rs

use reqwest::Client;
use tokio_stream::Stream;
use catcher_core::types::sse::SseClientConfig;

/// SSE 内容流行 — yield 内容行，静默过滤控制行
pub struct SseStream {
    // 内部持有 reqwest Response 的 bytes stream
    // chunk buffer: String（缓冲不完整行）
}

impl SseStream {
    /// 创建 SSE 流（一次性消费，不自动重连）
    pub async fn connect(config: SseClientConfig) -> Result<Self, CatcherError> {
        // 1. 构建 reqwest 请求
        // 2. 发送请求
        // 3. 获取 response.bytes_stream()
    }
}

impl Stream for SseStream {
    type Item = Result<String, CatcherError>;
    // bytes_stream → chunk buffer → 按 \n 切行 → route_line() → yield 内容行
}

// 内部方法：静默提取 last_event_id，用于断点续传
impl SseStream {
    pub fn last_event_id(&self) -> &str { ... }
}
```

### SSE 客户端（client.rs，长连接 + 自动重连）

```rust
// packages/catcher-http/src/sse/client.rs

use tokio::sync::mpsc;

/// SSE 长连接客户端（自动重连）
pub struct SseClient {
    lines_rx: mpsc::UnboundedReceiver<Result<String, CatcherError>>,
    cancel_tx: mpsc::UnboundedSender<()>,
    last_event_id: std::sync::Arc<std::sync::Mutex<String>>,
    ready_state: std::sync::Arc<std::sync::Mutex<SseReadyState>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SseReadyState { Connecting, Open, Closed }

impl SseClient {
    pub async fn connect(config: SseClientConfig) -> Result<Self, CatcherError> {
        // 1. 创建 mpsc channel
        // 2. spawn tokio task:
        //    loop {
        //      reqwest request with Last-Event-ID header
        //      bytes_stream → chunk buffer → route_line()
        //      Yield → send via channel
        //      SetLastEventId → 更新内部状态
        //      SetRetry → 更新重连间隔
        //      on disconnect → delay + reconnect (exponential backoff)
        //      on cancel signal → break
        //    }
    }

    pub async fn next_line(&mut self) -> Option<Result<String, CatcherError>> {
        self.lines_rx.recv().await
    }

    pub fn close(&mut self) { let _ = self.cancel_tx.send(()); }
    pub fn ready_state(&self) -> SseReadyState { ... }
    pub fn last_event_id(&self) -> String { ... }
}

impl Stream for SseClient {
    type Item = Result<String, CatcherError>;
    // 内部调用 next_line()，支持 while let line = client.next().await { ... }
}
```

### 对 reqwest 的改动

当前 `HttpTransport::execute()` 在 `response.bytes().await` 处一次性读取全部响应体。SSE 需要**流式读取**。

新增 `HttpTransport::execute_streaming()` 方法：

```rust
impl HttpTransport {
    /// 流式请求 — 返回 reqwest Response（不消费 body）
    pub async fn execute_streaming(
        &self,
        request: HttpRequest,
    ) -> Result<reqwest::Response, CatcherError> {
        // 与 execute() 相同的请求构建逻辑
        // 但不调用 response.bytes().await
        // 直接返回 response 让调用方按需消费 body stream
    }
}
```

SSE 模块复用 `HttpTransport` 的连接池、TLS、DNS、熔断等能力，只是 body 读取方式不同。

---

## 导出 API

### Node.js (`@eric8810/catcher-http`)

```typescript
export { createSSEClient, createSSEStream } from './sse/index.js'
```

### Browser (`@eric8810/catcher-web`)

```typescript
export { createSSEClient, createSSEStream } from './sse/index.js'
```

### Rust (`catcher-http`)

```rust
pub mod sse;

pub use sse::{SseClient, SseStream};
pub use catcher_core::types::sse::{SseClientConfig, SseMethod, SseReconnectConfig};
```

---

## 重连机制（仅 createSSEClient）

```
createSSEClient()
  │
  ├─ reqwest POST/GET (或 TS fetch) with headers + body
  │    │
  │    ├─ 200 OK
  │    │    ├─ bytes_stream → chunk buffer → route_line() → yield 内容行
  │    │    ├─ id: 行 → 静默记录 lastEventId
  │    │    ├─ retry: 行 → 静默调整重连间隔
  │    │    └─ 流结束 → scheduleReconnect()
  │    │
  │    ├─ 网络错误 → scheduleReconnect()
  │    ├─ 204 → close()（服务端要求停止重连）
  │    ├─ 301/307 → 跟随重定向
  │    └─ 其他错误 → error via channel → scheduleReconnect()
  │
  ▼ scheduleReconnect()
  ├─ delay = initialDelay × multiplier^(attempt-1) + jitter(±25%)
  ├─ headers['Last-Event-ID'] = lastEventId（自动携带）
  ├─ attempt++ ≤ maxRetries ? → 重新请求 : → close()
  └─ 成功连接 → attempt = 0, reset()
```

`createSSEStream` **不重连**——它就是一次 fetch，失败就 throw，让调用方决定是否重试（可以配合 Catcher 的 retry wrapper）。

---

## 韧性层次（SSE 场景）

```
                          ┌──────────────────┐
                          │   调用方代码       │
                          │  for await (line) │
                          └────────┬─────────┘
                                   │
                          ┌────────▼─────────┐
                          │  Line Router      │  ← 过滤控制行，yield 内容行
                          └────────┬─────────┘
                                   │
                   ┌───────────────┴───────────────┐
                   │ createSSEClient 才有：          │
                   │  Reconnect Layer               │  ← 指数退避 + Last-Event-ID 续传
                   │  Circuit Breaker               │  ← 熔断保护
                   └───────────────┬───────────────┘
                                   │
                          ┌────────▼─────────┐
                          │  Retry Wrapper    │  ← 连接阶段错误重试
                          └────────┬─────────┘
                                   │
       ┌───────────────────────────┼───────────────────────────┐
       │  Rust / Node.js           │                           │  Browser
       │  reqwest + Agent pool     │                           │  fetch (native)
       │  fetch + SharedAgent      │                           │
       └───────────────────────────┘                           └─────────┘
```

---

## 与现有模块的关系

| 复用模块 | 复用方式 | TS | Rust |
|---------|---------|-----|------|
| 连接池 / Agent | SSE 连接复用连接池和 DNS 缓存 | `SharedAgent` | `HttpTransport` (reqwest pool) |
| 重试 | 连接阶段错误重试 | `createRetryWrapper` (p-retry) | `RetryTransientMiddleware` |
| 熔断器 | SSE 端点级别的熔断保护 | `cockatiel` | `CircuitBreaker` |
| 类型 | 配置类型 | `catcher-core-ts` | `catcher-core` |
| 退避策略 | 指数退避 + jitter | 与 WS 共享算法 | 与 WS 共享算法 |

---

## 依赖

### TS 侧

| 依赖 | 用途 | 新增？ |
|------|------|--------|
| Node 18+ `fetch` | HTTP + ReadableStream | 否（内置） |
| Browser `fetch` | HTTP + ReadableStream | 否（内置） |
| `cockatiel` | 熔断器 | 否（现有） |
| `p-retry` | 连接重试 | 否（现有） |

**零新增依赖。**

### Rust 侧

| 依赖 | 用途 | 新增？ |
|------|------|--------|
| `reqwest` (stream feature) | SSE 流式读取 response body | 是（启用现有依赖的 feature） |
| `tokio-stream` | `Stream` trait 实现 | 是 |

---

## 不涉及的范围

- **不创建新 npm 包 / crate** — SSE 是现有 HTTP 包的扩展能力
- **不支持 IE** — 需要 `fetch` + `ReadableStream`，且 AI 场景需要 POST + 自定义 headers
- **不实现 SSE 服务端** — Catcher 是客户端库
- **不解析业务数据** — `[DONE]`、JSON 结构、event 路由等由调用方处理

---

## FFI / napi / UniFFI 扩展

SSE 通过 Rust 实现后，自动获得跨语言绑定路径：

| 绑定 | 入口 | 方式 |
|------|------|------|
| `catcher-napi-http` | `SseStream` / `SseClient` | napi-rs AsyncIterator |
| `catcher-ffi` | `sse_connect` / `sse_stream` | C ABI 回调函数 |
| `catcher-uniffi` | `SseClient` / `SseStream` | UniFFI 自动生成 Swift/Kotlin |
| Flutter (`catcher_core`) | `sseConnect()` | dart:ffi 调用 C ABI |

---

## 测试方案

### 测试分层

```
┌─────────────────────────────────────────────────────────────┐
│                     E2E / 手动验证                            │
│  （调用真实 SSE 端点，验证端到端体验，不纳入 CI）               │
└─────────────────────────────────────────────────────────────┘
                            ▲
┌─────────────────────────────────────────────────────────────┐
│               集成测试（Mock HTTP Server）                    │
│  TS: vitest + 拦截 fetch 返回模拟 ReadableStream             │
│  Rust: wiremock + tokio::test                                │
│  验证: createSSEStream / createSSEClient 完整流程             │
└─────────────────────────────────────────────────────────────┘
                            ▲
┌─────────────────────────────────────────────────────────────┐
│                  单元测试（纯函数，零网络）                     │
│  Line Router: route_line() 全路径覆盖                         │
│  TS: vitest                                                  │
│  Rust: #[test]                                               │
└─────────────────────────────────────────────────────────────┘
```

### 测试工具

| 平台 | 框架 | Mock 方式 | 已有依赖？ |
|------|------|-----------|-----------|
| TypeScript | vitest | 拦截 `globalThis.fetch`，返回模拟 ReadableStream | 是 |
| Rust | `#[tokio::test]` + wiremock | wiremock MockServer 返回 SSE 响应 | 是 |

### 测试文件结构

```
packages/catcher-http-ts/src/sse/
├── __tests__/
│   ├── router.test.ts       # Line Router 单元测试
│   ├── stream.test.ts       # createSSEStream 集成测试
│   └── client.test.ts       # createSSEClient 集成测试

packages/catcher-web/src/sse/
├── __tests__/
│   ├── router.test.ts       # 同 catcher-http-ts 版（代码相同）
│   ├── stream.test.ts       # 浏览器版 stream 测试
│   └── client.test.ts       # 浏览器版 client 测试

packages/catcher-http/src/sse/
├── router.rs                # #[cfg(test)] mod tests { } 内联
├── stream.rs                # #[cfg(test)] mod tests { } 内联
└── client.rs                # #[cfg(test)] mod tests { } 内联
```

> **设计决策**：TS 测试放在 `__tests__/` 子目录（vitest 默认发现），Rust 测试内联在 `#[cfg(test)] mod tests`。

### TS Mock 工具函数

```typescript
// 模拟 SSE 服务端响应
function mockSSEResponse(lines: string[], options?: { status?: number }) {
  const encoder = new TextEncoder()
  const stream = new ReadableStream({
    start(controller) {
      for (const line of lines) {
        controller.enqueue(encoder.encode(line + '\n'))
      }
      controller.close()
    }
  })
  vi.spyOn(globalThis, 'fetch').mockResolvedValueOnce({
    ok: (options?.status ?? 200) < 400,
    status: options?.status ?? 200,
    body: stream,
  } as Response)
}

// 模拟分片 SSE 响应（控制 chunk 边界）
function mockSSEChunked(chunks: string[], options?: { status?: number }) {
  const encoder = new TextEncoder()
  let i = 0
  const stream = new ReadableStream({
    pull(controller) {
      if (i < chunks.length) {
        controller.enqueue(encoder.encode(chunks[i++]))
      } else {
        controller.close()
      }
    }
  })
  vi.spyOn(globalThis, 'fetch').mockResolvedValueOnce({
    ok: (options?.status ?? 200) < 400,
    status: options?.status ?? 200,
    body: stream,
  } as Response)
}
```

---

### 一、Line Router 单元测试

纯函数，零网络，零 mock。TS 和 Rust 共享相同测试矩阵。

#### 1.1 控制行 → Silent

| # | 输入 | 期望 | 说明 |
|---|------|------|------|
| 1 | `""` | Silent | SSE 事件分隔符 |
| 2 | `": keepalive"` | Silent | 心跳 |
| 3 | `": this is a comment"` | Silent | 任意注释 |
| 4 | `":"` | Silent | 最短注释 |

#### 1.2 id: 行 → SetLastEventId

| # | 输入 | 期望值 | 说明 |
|---|------|--------|------|
| 5 | `"id: msg_001"` | `"msg_001"` | 标准格式 |
| 6 | `"id:msg_002"` | `"msg_002"` | 无空格 |
| 7 | `"id:  multi  space"` | `"multi  space"` | trimStart 只去前导空格 |
| 8 | `"id:"` | `""` | 空 id |
| 9 | `"id: 42"` | `"42"` | 数字 id |

#### 1.3 retry: 行 → SetRetry

| # | 输入 | 期望 | 说明 |
|---|------|------|------|
| 10 | `"retry: 5000"` | SetRetry(5000) | 标准格式 |
| 11 | `"retry:1000"` | SetRetry(1000) | 无空格 |
| 12 | `"retry: abc"` | Yield 原样 | 非数字 |
| 13 | `"retry: -1"` | Yield 原样 | 负数 |
| 14 | `"retry: 0"` | SetRetry(0) | 零合法 |

#### 1.4 内容行 → Yield 原样输出

| # | 输入 | 期望输出 | 说明 |
|---|------|---------|------|
| 15 | `"data: Hello"` | `"data: Hello"` | 标准数据行 |
| 16 | `"data: {\"type\":\"start\"}"` | 原样 | JSON payload |
| 17 | `"data:  world"` | `"data:  world"` | 两个空格保留 |
| 18 | `"data: [DONE]"` | `"data: [DONE]"` | 终止标记原样 |
| 19 | `"event: message_start"` | 原样 | event 行 |
| 20 | `"data:"` | `"data:"` | 空 data |
| 21 | `"custom: value"` | 原样 | 非标准前缀 |
| 22 | `"just text"` | 原样 | 无前缀行 |
| 23 | `"ID: uppercase"` | 原样 | 大写不是 id: |
| 24 | `" "` | 原样 | 空格非控制前缀 |

> **关键断言**：Yield 行完全原样——库不做任何前缀剥离或结构化解析。

---

### 二、createSSEStream 集成测试

#### 2.1 基础流式消费

| # | 测试名 | 模拟输入 | 断言 |
|---|--------|---------|------|
| S1 | 完整 SSE 事件 | `"data: Hello\n\ndata: World\n\n"` | `["data: Hello", "data: World"]` |
| S2 | 混合控制行和内容行 | `": comment\ndata: A\nid: 1\n\ndata: B\n"` | `["data: A", "data: B"]`，lastEventId=`"1"` |
| S3 | 心跳行被过滤 | `": ping\n: pong\ndata: real\n\n"` | `["data: real"]` |
| S4 | 空行被过滤 | `"data: X\n\n\ndata: Y\n\n"` | `["data: X", "data: Y"]` |

#### 2.2 Chunk 分片处理

| # | 测试名 | 模拟方式 | 断言 |
|---|--------|---------|------|
| S5 | 跨 chunk 的行 | chunk1:`"data: Hel"`, chunk2:`"lo\n"` | `["data: Hello"]`，无半行 |
| S6 | 单 chunk 多行 | `"data: A\ndata: B\n"` | `["data: A", "data: B"]` |
| S7 | 空 chunk + 数据 chunk | chunk1:`""`, chunk2:`"data: X\n"` | `["data: X"]` |

#### 2.3 行尾处理

| # | 测试名 | 模拟输入 | 断言 |
|---|--------|---------|------|
| S8 | `\r\n` 换行 | `"data: A\r\n\r\n"` | `["data: A"]` |
| S9 | 混合 `\n` 和 `\r\n` | `"data: A\ndata: B\r\n"` | `["data: A", "data: B"]` |
| S10 | 最后一行无 `\n` | `"data: end"` | `["data: end"]` |

#### 2.4 id: 和 retry: 提取

| # | 测试名 | 模拟输入 | 断言 |
|---|--------|---------|------|
| S11 | lastEventId 提取 | `"id: msg_42\ndata: X\n"` | `stream.lastEventId === "msg_42"` |
| S12 | 多次 id 覆盖 | `"id: first\ndata: A\n\nid: second\ndata: B\n"` | 最终 `lastEventId === "second"` |

#### 2.5 错误处理

| # | 测试名 | 模拟方式 | 断言 |
|---|--------|---------|------|
| S13 | HTTP 非 200 | fetch 返回 500 | throw，含 `"HTTP 500"` |
| S14 | AbortSignal 中断 | 读到一半 abort | `for await` 正常结束 |

#### 2.6 只能迭代一次

| # | 测试名 | 断言 |
|---|--------|------|
| S15 | 第二次迭代抛错 | 第二次 `[Symbol.asyncIterator]()` throw |

---

### 三、createSSEClient 集成测试

#### 3.1 基础连接和消费

| # | 测试名 | 断言 |
|---|--------|------|
| C1 | 连接并消费内容行 | mock 返回 SSE → 收到正确内容行数组 |
| C2 | readyState 变化 | CONNECTING → OPEN(收到数据时) → CLOSED(close后) |
| C3 | close() 停止迭代 | close() 后 `for await` 结束 |
| C4 | lastEventId 提取 | 从 `id:` 行提取，`.lastEventId` 可访问 |

#### 3.2 自动重连

| # | 测试名 | 模拟方式 | 断言 |
|---|--------|---------|------|
| C5 | 流结束后自动重连 | 第一次响应结束 → 延迟 → 第二次响应 | 两次内容都收到 |
| C6 | 重连携带 Last-Event-ID | 第一次含 `id: abc`，第二次请求 headers 含 `Last-Event-ID: abc` | headers 正确 |
| C7 | 网络错误后重连 | 第一次 fetch throw → 第二次成功 | 最终收到数据 |
| C8 | 达到 maxRetries 停止 | `maxRetries: 2`，连续失败 3 次 | `for await` 结束 |
| C9 | enabled: false 不重连 | `enabled: false` | 流结束后不重连 |

#### 3.3 204 停止重连

| # | 测试名 | 模拟方式 | 断言 |
|---|--------|---------|------|
| C10 | 204 停止重连 | fetch 返回 204 | `for await` 结束 |

#### 3.4 熔断器

| # | 测试名 | 模拟方式 | 断言 |
|---|--------|---------|------|
| C11 | circuitBreaker 集成 | `failureThreshold: 2`，连续失败后停止重连 | `for await` 结束 |

---

### 四、Rust 专项测试

使用 `wiremock::MockServer`，与 TS 测试用例一一对应。

#### 4.1 Line Router 单元测试

同「一、Line Router」全部用例（#1 ~ #24），以 `#[test]` 内联在 `router.rs`。

#### 4.2 SseStream 集成测试

| # | 测试名 | MockServer 响应 | 断言 |
|---|--------|----------------|------|
| RS1 | 完整事件消费 | `"data: Hello\n\ndata: World\n\n"` | 收集 2 行 |
| RS2 | 控制行过滤 | `": comment\ndata: A\nid: 1\n"` | 仅 `["data: A"]`，last_event_id=`"1"` |
| RS3 | `\r\n` 容错 | `"data: X\r\n\r\n"` | 正确切行 |
| RS4 | HTTP 错误 | 500 | `Err(HttpError { status: 500 })` |
| RS5 | Stream trait 消费 | 正常 SSE 响应 | `while let Some(line) = stream.next().await` 编译+正确 |

#### 4.3 SseClient 集成测试

| # | 测试名 | MockServer 配置 | 断言 |
|---|--------|----------------|------|
| RC1 | 基础消费 | 单次 SSE 响应 | 正确收集内容行 |
| RC2 | 自动重连 + Last-Event-ID | 第一次响应结束 → 第二次请求到达 | headers 含 `Last-Event-ID` |
| RC3 | close() 停止 | 发数据 → close() | 迭代结束 |
| RC4 | 204 停止重连 | 返回 204 | 迭代结束，无第二次请求 |
| RC5 | readyState 状态 | 各阶段验证 | Connecting → Open → Closed |
| RC6 | Stream trait 消费 | 正常 SSE | `while let Some(line) = client.next().await` 编译+正确 |

---

### 五、测试覆盖矩阵

| 设计要点 | Router | Stream | Client | TS | Rust |
|---------|:------:|:------:|:------:|:--:|:----:|
| 空行 → Silent | ✅ | | | ✅ | ✅ |
| `:` 注释 → Silent | ✅ | | | ✅ | ✅ |
| `id:` → SetLastEventId | ✅ | ✅ | ✅ | ✅ | ✅ |
| `retry:` → SetRetry | ✅ | | ✅ | ✅ | ✅ |
| `data:`/`event:` → Yield 原样 | ✅ | ✅ | ✅ | ✅ | ✅ |
| Chunk 缓冲（无半行） | | ✅ | | ✅ | ✅ |
| `\r\n` 容错 | | ✅ | | ✅ | ✅ |
| 最后一行无 `\n` | | ✅ | | ✅ | |
| createSSEStream 一次性 | | ✅ | | ✅ | ✅ |
| AbortSignal 中断 | | ✅ | ✅ | ✅ | |
| HTTP 错误 throw | | ✅ | ✅ | ✅ | ✅ |
| SSETimeoutError | | ✅ | | ✅ | |
| 单次迭代限制 | | ✅ | | ✅ | |
| createSSEClient 自动重连 | | | ✅ | ✅ | ✅ |
| Last-Event-ID 携带 | | | ✅ | ✅ | ✅ |
| 204 停止重连 | | | ✅ | ✅ | ✅ |
| maxRetries 限制 | | | ✅ | ✅ | |
| reconnect.enabled 控制 | | | ✅ | ✅ | |
| close() 停止 | | | ✅ | ✅ | ✅ |
| readyState 状态 | | | ✅ | ✅ | ✅ |
| circuitBreaker 集成 | | | ✅ | ✅ | |
| 无业务解析 | ✅ | ✅ | ✅ | ✅ | ✅ |

### 六、不测试的范围

| 不测试 | 原因 |
|--------|------|
| 真实 OpenAI API 调用 | 需要 API key，不稳定，属于 E2E |
| napi-rs / UniFFI 绑定 | SSE FFI 尚未实现 |
| 浏览器 DOM 兼容性 | 需要 Playwright，属于 E2E |
| 并发性能压测 | 非 Catcher SSE 的职责 |
| SSE 服务端实现 | Catcher 是客户端库 |
