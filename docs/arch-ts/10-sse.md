# 10 — Server-Sent Events (SSE) 客户端

> 新增模块 · 设计文档
> 基于 WHATWG Server-Sent Events 规范，面向 AI 流式响应场景
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

## SSE 协议格式（WHATWG 规范）

```
event: message_start
id: msg_001
data: {"type":"message_start","message":{"id":"msg_01","model":"gpt-4"}}

data: {"choices":[{"delta":{"content":"Hello"}}]}

data: {"choices":[{"delta":{"content":" world"}}]}

data: [DONE]
```

字段规范：

| 字段 | 格式 | 用途 |
|------|------|------|
| `data:` | `data: <payload>\n` | 事件数据，可多行拼接 |
| `event:` | `event: <type>\n` | 事件类型，默认 `"message"` |
| `id:` | `id: <string>\n` | 事件 ID，用于断点续传 |
| `retry:` | `retry: <ms>\n` | 服务端建议重连间隔 |
| 空行 | `\n` | 事件分隔符 |
| `: comment` | `: <text>\n` | 注释（心跳保活） |

- MIME 类型：`text/event-stream`
- 字符编码：UTF-8

---

## ⚠️ 非标准 SSE 格式（重要）

### 问题

**现实中的 SSE 流并非总是严格遵循 WHATWG 规范。** 以下是实际生产环境中遇到的传输层变体：

| 变体 | 描述 | 示例 |
|------|------|------|
| **`\r\n` 混用** | Windows 风格换行 | `data: foo\r\n\r\n` |
| **前缀变体** | `data:` 后无空格 | `data:foo` vs `data: foo` |
| **不完整 chunk** | 网络分片导致一行被切断 | 见 [elysiajs/eden#222](https://github.com/elysiajs/eden/issues/222) |
| **多行 data 拼接问题** | 多行 data 拼接时丢失 `\n` | 影响代码/Markdown 格式 |

> **注意**：裸 JSON、JSONL、自定义分隔符等属于**业务层协议**，Catcher 作为传输韧性库不介入。我们只处理 SSE 协议本身的传输层问题。

### 设计原则：传输层只做传输层的事

```
❌ 错误做法：在传输库里内置 OpenAI / Anthropic / JSONL 解析
   → 每多一个 API 变体就多一种策略，永无止境
   → 库的职责边界模糊，与业务耦合

✅ 正确做法：
   - 内置两种策略：standard（严格 WHATWG）+ lenient（容错传输层变体）
   - 提供干净的扩展接口，开发者按需实现业务解析
   - 库的职责：可靠地把 SSE 事件从网络层搬到调用方手里
```

### 解析器架构

```
原始字节流
    │
    ▼
┌──────────────────────────────┐
│ Chunk Buffer                 │  处理网络分片：按 \n 边界切行
│ (处理不完整 chunk)            │  保证每次 yield 一个完整行
└──────────────┬───────────────┘
               │  完整行
               ▼
┌──────────────────────────────┐
│ Line Classifier              │  识别行类型
│ (内置两种，可自定义扩展)       │  data / event / id / retry / comment / blank
└──────────────┬───────────────┘
               │
               ▼
┌──────────────────────────────┐
│ Event Assembler              │  组装完整事件
│ (空行分隔，WHATWG 标准)       │
└──────────────┬───────────────┘
               │
               ▼
         SSEEvent { event, data, id, retry }
```

### 内置策略（仅两种）

| | `standard`（默认） | `lenient` |
|---|---|---|
| 定位 | 严格 WHATWG 规范 | 容错真实世界的传输层毛刺 |
| `data:` 行解析 | 严格 `data: ` 前缀（有空格） | 允许 `data:` 后无空格 |
| 裸行（无 `data:` 前缀） | 忽略 | 当作 data |
| 分隔符 | `\n\n`（空行） | `\n\n`（空行） |
| `\r\n` | 不处理 | 自动 strip `\r` |
| 不完整 chunk | 缓冲至行完整 | 缓冲至行完整 |
| 未知字段 | 忽略 | 忽略 |

### 自定义策略接口

开发者遇到非标准服务端时，自行实现：

```typescript
interface SSEParseStrategy {
  /** 行分类器：判断一行是什么类型 */
  classifyLine?(line: string): 'data' | 'event' | 'id' | 'retry' | 'comment' | 'blank' | 'custom'

  /** 事件分隔判定：当前行是否标志一个事件结束 */
  isEventBoundary?(line: string): boolean

  /** data 解码：对原始 data 字符串做后处理 */
  decodeData?(raw: string): string

  /** 自定义字段处理：遇到无法识别的字段时 */
  onUnknownField?(fieldName: string, value: string, ctx: SSEParseContext): void
}
```

### 使用示例

```typescript
// 默认：严格 WHATWG
const stream = createSSEStream({ url: '/api/events' })

// lenient：容错传输层变体（\r\n、无空格等）
const stream = createSSEStream({
  url: '/api/stream',
  parseStrategy: 'lenient',
})

// 自定义：开发者自行处理业务格式
// 例：某服务端用 --- 分隔，每行是裸 JSON
const stream = createSSEStream({
  url: '/api/custom',
  parseStrategy: {
    classifyLine(line) {
      if (line === '---') return 'blank'
      if (line.startsWith('#')) return 'comment'
      return 'data'  // 所有其他行都当 data
    },
    isEventBoundary(line) { return line === '---' },
  },
})

// OpenAI 场景：开发者自己在业务层处理 [DONE] 和 delta 解析
const stream = createSSEStream({
  url: 'https://api.openai.com/v1/chat/completions',
  method: 'POST',
  headers: { Authorization: `Bearer ${apiKey}` },
  body: { model: 'gpt-4', messages: [{ role: 'user', content: 'Hi' }], stream: true },
  parseStrategy: 'lenient',
})
for await (const event of stream) {
  if (event.data === '[DONE]') break  // 业务逻辑：自行判断终止
  const chunk = JSON.parse(event.data)  // 业务逻辑：自行解析
  process.stdout.write(chunk.choices[0]?.delta?.content ?? '')
}
```

**Rust 自定义策略示例：**

```rust
use catcher_core::types::sse::{SseParseStrategy, LineType, SseStrategy, SseClientConfig};
use catcher_http::sse::SseStream;

// 自定义策略：用 "---" 分隔事件，# 开头为注释
struct CustomSeparator;

impl SseParseStrategy for CustomSeparator {
    fn classify_line(&self, line: &str) -> LineType {
        if line == "---" { return LineType::Blank; }
        if line.starts_with('#') { return LineType::Comment; }
        LineType::Data
    }

    fn is_event_boundary(&self, line: &str) -> bool {
        line == "---"
    }
}

let config = SseClientConfig {
    url: "https://api.example.com/custom".into(),
    parse_strategy: SseStrategy::Custom(Box::new(CustomSeparator)),
    ..Default::default()
};
let mut stream = SseStream::connect(config).await?;
while let Some(event) = stream.next().await {
    let event = event?;
    println!("data: {}", event.data);
}
```

---

## 全平台支持

SSE 模块覆盖所有 Catcher 平台：

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

/** 内置解析策略名称 */
export type SSEBuiltinStrategy = 'standard' | 'lenient'

/** 自定义解析策略 */
export interface SSEParseStrategy {
  classifyLine?(line: string): 'data' | 'event' | 'id' | 'retry' | 'comment' | 'blank' | 'custom'
  isEventBoundary?(line: string): boolean
  decodeData?(raw: string): string
  onUnknownField?(fieldName: string, value: string, ctx: SSEParseContext): void
}

export interface SSEClientConfig {
  /** SSE 端点 URL */
  url: string
  /** HTTP 方法，默认 'GET'。AI 场景通常用 'POST' */
  method?: 'GET' | 'POST'
  /** 请求 headers（如 Authorization） */
  headers?: Record<string, string>
  /** 请求 body（POST 场景） */
  body?: string | Record<string, unknown>

  /** 解析策略：内置名称 或 自定义对象。默认 'standard' */
  parseStrategy?: SSEBuiltinStrategy | SSEParseStrategy

  /** 连接层选项 */
  keepAlive?: boolean
  dnsCacheTtl?: number
  rejectUnauthorized?: boolean

  /** 自动重连配置 */
  reconnect?: {
    enabled?: boolean
    maxRetries?: number
    initialDelay?: number
    maxDelay?: number
    backoffMultiplier?: number
  }

  /** 重试配置（连接阶段的错误） */
  retry?: RetryOptions
  /** 熔断配置 */
  circuitBreaker?: { failureThreshold: number; resetTimeout: number }
  /** 请求超时 ms，默认 30_000 */
  timeout?: number
}

export interface SSEEvent {
  /** 事件类型，默认 'message' */
  event: string
  /** 事件数据（原始字符串） */
  data: string
  /** 事件 ID */
  id?: string
  /** 重连间隔（服务端建议） */
  retry?: number
}

export interface ISSEClient {
  readonly readyState: 'CONNECTING' | 'OPEN' | 'CLOSED'
  readonly lastEventId: string
  addEventListener(type: string, listener: (event: SSEEvent) => void): void
  removeEventListener(type: string, listener: (event: SSEEvent) => void): void
  close(): void
}

/** SSE 一次性消费流配置（SSEClientConfig 的简化版，无重连/熔断） */
export interface SSEStreamOptions {
  url: string
  method?: 'GET' | 'POST'
  headers?: Record<string, string>
  body?: string | Record<string, unknown>
  /** 解析策略 */
  parseStrategy?: SSEBuiltinStrategy | SSEParseStrategy
  timeout?: number
  keepAlive?: boolean
}

export interface SSEStream extends AsyncIterable<SSEEvent> {
  readonly lastEventId: string
  cancel(): void
}
```

### Rust 类型（catcher-core/src/types/sse.rs）

```rust
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// SSE 事件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SseEvent {
    /// 事件类型，默认 "message"
    pub event: String,
    /// 事件数据（原始字符串）
    pub data: String,
    /// 事件 ID（用于断点续传）
    pub id: Option<String>,
    /// 服务端建议的重连间隔（ms）
    pub retry: Option<u64>,
}

/// 内置解析策略
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SseBuiltinStrategy {
    /// 严格 WHATWG 规范
    #[default]
    Standard,
    /// 宽松模式：容错传输层变体（\r\n、缺少空格等）
    Lenient,
}

/// 行分类结果
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineType {
    Data,
    Event,
    Id,
    Retry,
    Comment,
    Blank,
    Custom,
}

/// 自定义解析策略 trait（与 TS 侧 SSEParseStrategy 对等）
pub trait SseParseStrategy: Send + Sync {
    /// 行分类器：判断一行是什么类型
    fn classify_line(&self, line: &str) -> LineType;
    /// 事件分隔判定：当前行是否标志一个事件结束
    fn is_event_boundary(&self, line: &str) -> bool { line.is_empty() }
    /// data 解码：对原始 data 字符串做后处理
    fn decode_data(&self, raw: &str) -> &str { raw }
}

/// 解析策略：内置名称 或 自定义 trait 实现
pub enum SseStrategy {
    Builtin(SseBuiltinStrategy),
    Custom(Box<dyn SseParseStrategy>),
}

impl Default for SseStrategy {
    fn default() -> Self { Self::Builtin(SseBuiltinStrategy::default()) }
}

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
    /// 解析策略：内置名称 或 自定义 trait。默认 Standard
    #[serde(skip)]
    pub parse_strategy: SseStrategy,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reconnect: Option<SseReconnectConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry: Option<RetryConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub circuit_breaker: Option<CircuitBreakerConfig>,
    #[serde(default = "default_timeout")]
    pub timeout_ms: u64,
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

## Rust 侧实现设计

### 新增源文件

```
packages/catcher-http/src/
├── sse/
│   ├── mod.rs              # 模块导出
│   ├── parser.rs           # SSE 解析器（策略化） (~200 行)
│   ├── client.rs           # SseClient（长连接 + 自动重连） (~250 行)
│   └── stream.rs           # SseStream（一次性消费） (~150 行)
├── transport/
│   ├── http_client.rs      # 现有
│   └── ...
├── resilience/
│   └── ...
└── lib.rs                  # 新增 pub mod sse

packages/catcher-core/src/types/
├── sse.rs                  # SSE 类型定义（SseEvent, SseClientConfig 等）
└── mod.rs                  # 新增 pub mod sse
```

### SSE 解析器（Rust）

```rust
// packages/catcher-http/src/sse/parser.rs

use catcher_core::types::sse::{SseStrategy, SseEvent};

/// SSE 文本流解析器
///
/// 设计要点：
/// - 策略化：支持 standard / lenient / 自定义 trait
/// - 缓冲区：处理网络分片导致的不完整行
/// - 零拷贝：尽可能避免不必要的 String clone
pub struct SseParser {
    strategy: SseStrategy,
    data_buffer: String,
    event_type_buffer: String,
    last_event_id_buffer: String,
    retry_buffer: Option<u64>,
}

impl SseParser {
    pub fn new(strategy: SseStrategy) -> Self { ... }

    /// 处理一行文本，返回已组装完成的 SseEvent（可能有多个）
    /// 在空行（或自定义分隔符）时触发事件组装
    pub fn process_line(&mut self, line: &str) -> Option<SseEvent> { ... }

    /// 重置缓冲区
    pub fn reset(&mut self) { ... }

    /// 获取当前 last_event_id
    pub fn last_event_id(&self) -> &str { &self.last_event_id_buffer }
}
```

### SSE 流（Rust）

```rust
// packages/catcher-http/src/sse/stream.rs

use reqwest::Client;
use tokio_stream::Stream;
use catcher_core::types::sse::{SseClientConfig, SseEvent};
use crate::sse::parser::SseParser;

/// SSE 一次性消费流
///
/// 基于 reqwest 的 streaming response：
/// - response.bytes_stream() → tokio Stream<Bytes>
/// - 逐 chunk → 按 \n 切行 → SseParser → yield SseEvent
pub struct SseStream {
    parser: SseParser,
    // 内部持有 reqwest Response 的 bytes stream
}

impl SseStream {
    /// 创建 SSE 流（一次性消费，不自动重连）
    pub async fn connect(config: SseClientConfig) -> Result<Self, CatcherError> {
        // 1. 构建 reqwest 请求
        // 2. 发送请求，检查 Content-Type
        // 3. 获取 response.bytes_stream()
        // 4. 初始化 SseParser
    }
}

impl Stream for SseStream {
    type Item = Result<SseEvent, CatcherError>;
    // 逐行从 bytes_stream 读取 → parser.process_line() → yield event
}

// 使用示例
// let mut stream = SseStream::connect(config).await?;
// while let Some(event) = stream.next().await {
//     let event = event?;
//     println!("data: {}", event.data);
// }
```

### SSE 客户端（Rust，长连接 + 自动重连）

```rust
// packages/catcher-http/src/sse/client.rs

use tokio::sync::mpsc;
use catcher_core::types::sse::{SseClientConfig, SseEvent};
use crate::sse::parser::SseParser;

/// SSE 长连接客户端（自动重连）
///
/// 内部 spawn 一个 tokio task 管理：
/// - fetch → read stream → parse → send events via channel
/// - 断连时按退避策略重连，携带 Last-Event-ID
pub struct SseClient {
    events_rx: mpsc::UnboundedReceiver<Result<SseEvent, CatcherError>>,
    cancel_tx: mpsc::UnboundedSender<()>,
    last_event_id: std::sync::Arc<std::sync::Mutex<String>>,
    ready_state: std::sync::Arc<std::sync::Mutex<SseReadyState>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SseReadyState { Connecting, Open, Closed }

impl SseClient {
    /// 创建 SSE 客户端（后台自动连接 + 重连）
    pub async fn connect(config: SseClientConfig) -> Result<Self, CatcherError> {
        // 1. 创建 mpsc channel
        // 2. spawn tokio task:
        //    loop {
        //      reqwest request with Last-Event-ID header
        //      bytes_stream → SseParser → send events via channel
        //      on disconnect: delay + reconnect (exponential backoff)
        //      on cancel signal: break
        //    }
    }

    /// 接收下一个事件
    pub async fn next_event(&mut self) -> Option<Result<SseEvent, CatcherError>> {
        self.events_rx.recv().await
    }

    /// 关闭连接
    pub fn close(&mut self) { let _ = self.cancel_tx.send(()); }

    pub fn ready_state(&self) -> SseReadyState { ... }
    pub fn last_event_id(&self) -> String { ... }
}
```

### 对 reqwest 的改动

当前 `HttpTransport::execute()` 在 `response.bytes().await` 处一次性读取全部响应体。SSE 需要**流式读取**。

新增 `HttpTransport::execute_streaming()` 方法：

```rust
impl HttpTransport {
    /// 流式请求 — 返回 reqwest Response（不消费 body）
    /// SSE 和其他流式场景使用
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

这样 SSE 模块可以复用 `HttpTransport` 的连接池、TLS、DNS、熔断等能力，只是 body 读取方式不同。

### Cargo.toml 变更

```toml
# catcher-http/Cargo.toml 新增依赖
[dependencies]
# 现有不变
catcher-core = { path = "../catcher-core" }
reqwest = { version = "0.12", features = ["stream"] }  # 新增 stream feature
reqwest-middleware = "0.4"
reqwest-retry = "0.7"
tokio = { version = "1", features = ["full"] }
tokio-stream = "0.1"     # 新增：SSE stream

# catcher-core/Cargo.toml 无新增依赖
```

---

## TS 侧实现设计

### Node.js (`@eric8810/catcher-http`)

```
packages/catcher-http-ts/src/
├── sse/
│   ├── parser.ts       # SseParser 类（策略化） (~200 行)
│   ├── strategies.ts   # 内置策略实现 (~150 行)
│   ├── client.ts       # ISSEClient 实现（长连接） (~200 行)
│   ├── stream.ts       # SSEStream 实现（一次性） (~120 行)
│   └── index.ts
└── index.ts            # 新增 SSE 导出
```

### Browser (`@eric8810/catcher-web`)

```
packages/catcher-web/src/
├── sse/
│   ├── parser.ts       # 同 TS 版解析器
│   ├── strategies.ts   # 同 TS 版策略
│   ├── client.ts       # 浏览器版（无 SharedAgent）
│   ├── stream.ts       # 浏览器版
│   └── index.ts
└── index.ts            # 新增 SSE 导出
```

### SSE 解析器核心逻辑（TS / Rust 共享设计）

```
SseParser {
  data_buffer: String
  event_type_buffer: String
  last_event_id_buffer: String
  retry_buffer: Option<u64>
  strategy: SseBuiltinStrategy

  process_line(line) → Option<SseEvent>:
    switch strategy:
      standard:
        if line is empty → assemble_event()
        if line starts with "data:" → append data_buffer (+ trim space)
        if line starts with "event:" → set event_type_buffer
        if line starts with "id:" → set last_event_id_buffer
        if line starts with "retry:" → parse retry value
        if line starts with ":" → ignore (comment)
        else → ignore

      lenient:
        same as standard, but:
          - strip \r from line before processing
          - lines without recognized prefix → treat as data
          - allow "data:" without trailing space
          - treat single \n as event boundary if no blank line seen after 1 event

  assemble_event() → Option<SseEvent>:
    if data_buffer is empty → reset buffers, return null
    data = data_buffer (strip trailing \n)
    event = SseEvent { event: event_type_buffer, data, id: last_event_id_buffer, retry }
    reset buffers
    return event
}
```

---

## 导出 API

### Node.js (`@eric8810/catcher-http`)

```typescript
export { createSSEClient, createSSEStream, SseParser } from './sse/index.js'
```

### Browser (`@eric8810/catcher-web`)

```typescript
export { createSSEClient, createSSEStream, SseParser } from './sse/index.js'
```

### Rust (`catcher-http`)

```rust
pub mod sse;

// Re-export
pub use sse::{SseClient, SseStream, SseParser};

// catcher-core re-exports
pub use catcher_core::types::sse::{
    SseEvent, SseBuiltinStrategy, SseStrategy, SseParseStrategy, LineType,
    SseClientConfig, SseStreamOptions,
};
```

---

## 重连机制设计

```
createSSEClient()
  │
  ├─ reqwest POST/GET (或 TS fetch) with headers + body
  │    │
  │    ├─ 200 OK + text/event-stream (或非标准)
  │    │    ├─ bytes_stream → 逐 chunk → 按 \n 切行 → SseParser → emit events
  │    │    ├─ 记录 last_event_id（用于断点续传）
  │    │    └─ 流结束 → scheduleReconnect()
  │    │
  │    ├─ 网络错误 → scheduleReconnect()
  │    ├─ 204 → close()（服务端要求停止重连）
  │    ├─ 301/307 → 跟随重定向
  │    └─ 其他错误 → error event → scheduleReconnect()
  │
  ▼ scheduleReconnect()
  ├─ delay = initialDelay × multiplier^(attempt-1) + jitter(±25%)
  ├─ headers['Last-Event-ID'] = lastEventId
  ├─ attempt++ ≤ maxRetries ? → 重新请求 : → close()
  └─ 成功连接 → attempt = 0, reset()
```

## 韧性层次（SSE 场景）

```
                          ┌──────────────────┐
                          │   调用方代码       │
                          └────────┬─────────┘
                                   │
                          ┌────────▼─────────┐
                          │  SseParser        │  ← 策略化解析 data/event/id/retry
                          └────────┬─────────┘
                                   │
                          ┌────────▼─────────┐
                          │  Reconnect Layer  │  ← 指数退避 + Last-Event-ID 续传
                          └────────┬─────────┘
                                   │
                          ┌────────▼─────────┐
                          │  Circuit Breaker  │  ← 熔断保护
                          └────────┬─────────┘
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
| 类型 | 配置和事件类型 | `catcher-core-ts` | `catcher-core` |
| 退避策略 | 指数退避 + jitter | 与 WS 共享算法 | 与 WS 共享算法 |

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

## 不涉及的范围

- **不创建新 npm 包 / crate** — SSE 是现有 HTTP 包的扩展能力
- **不支持 IE** — 需要 `fetch` + `ReadableStream`，且 AI 场景需要 POST + 自定义 headers
- **不实现 SSE 服务端** — Catcher 是客户端库

## FFI / napi / UniFFI 扩展

SSE 通过 Rust 实现后，自动获得跨语言绑定路径：

| 绑定 | 入口 | 方式 |
|------|------|------|
| `catcher-napi-http` | `SseStream` / `SseClient` | napi-rs AsyncIterator |
| `catcher-ffi` | `sse_connect` / `sse_stream` | C ABI 回调函数 |
| `catcher-uniffi` | `SseClient` / `SseStream` | UniFFI 自动生成 Swift/Kotlin |
| Flutter (`catcher_core`) | `sseConnect()` | dart:ffi 调用 C ABI |
