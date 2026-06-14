# 迁移指南

> 从常用 HTTP / WebSocket / SSE 库迁移到 catcher，保持熟悉 API 的同时获得全套韧性能力。

---

## 版本升级：0.3.13 → 0.3.15

0.3.15 新增系统代理自动检测（`proxy.mode = "system"`），`ProxyConfig.url` 从 `String` 变为 `Option<String>`。**大多数项目无需改代码**，但请确认以下变更。完整说明见 [`CHANGELOG.md`](../../CHANGELOG.md) 和 [`docs/plan/2026-06-13-system-proxy-detection.md`](../plan/2026-06-13-system-proxy-detection.md)。

| 变更 | 0.3.13 旧行为 | 0.3.15 新行为 | 如何迁移 |
|------|-------------|-------------|---------|
| `ProxyConfig.url` 类型 | `String`（必填） | `Option<String>`（Manual 必填，System 忽略） | `url: "..."` → `url: Some("...".into())`；JSON 配置无变化 |
| `ProxyConfig.mode` 字段 | 无 | `Manual`（默认）/ `System` | 旧 JSON 自动默认 `Manual`，无需改动 |
| 系统代理 | 不支持，调用方须手动检测 | `proxy: { mode: 'system' }` 自动检测 | 参见下方「System 代理模式」 |

### System 代理模式

```typescript
// 0.3.13：调用方自行检测代理
const client = new HttpClient({
  proxy: { url: process.env.HTTPS_PROXY }  // 手动
})

// 0.3.15：catcher 自动检测
const client = new HttpClient({
  proxy: { mode: 'system' }
})
```

`networkChanged()` 时会自动重新检测系统代理，地址变化后重建连接池。详见 [`proxy.rs`](../../packages/catcher-dns/src/proxy.rs) 注释了解平台支持情况。

---

## 版本升级：0.3.12 → 0.3.13

| 变更 | 影响 | 如需保持旧行为 |
|------|------|----------------|
| **DNS 改为按需启用** | 不配置 `dns` 时使用协议库原生解析（此前总是启用 Catcher DNS 缓存） | 显式传入 `dns`（`mode` 默认 `catcher`），即可恢复缓存 / host mapping / 自定义 nameserver |
| **不再静默回退公共 DNS** | 读取系统 DNS 配置失败时返回错误，而非静默用 `8.8.8.8` 等公共 nameserver | 设置 `dns.fallback_to_default_nameservers = true`（默认 `false`） |
| **`socks5://` 按 `socks5h://` 处理** | 代理下目标域名交给代理远端解析（修复 Clash/VPN 分流） | 无 —— 有意移除本地解析 socks5 的路径 |
| **`tls_sni_override` 原生路径报错** | 在 catcher-http / catcher-ws（Rust/原生）设置该字段会返回 `InvalidConfig` | 移除该字段；若确需自定义 SNI，使用纯 TS 的 Node Agent（`servername`） |
| **WS 单连接多 IP 握手故障转移移除** | 多 IP 主机不再在握手层逐 IP 重试，改由 reqwest 连接层处理 | 用多端点竞速 `urls: ['a', 'b']` 获取端点级故障转移 |
| **WS FFI `destroy` 语义** | 销毁现在会取消事件循环并关闭连接（修复 use-after-free） | 无需处理（行为更安全） |

### 新能力（增量，无需迁移）

- **`networkChanged()`**：HTTP / WS / DNS 均新增。从 OS 网络回调中调用即可主动恢复连接。详见 [resilience.md](./resilience.md) 第七节。注意它只影响**之后发起的新请求**；若要连同在途请求一并恢复，调用后再 `cancelAll()`。
- **统一代理 / TLS 配置**：HTTP 与 WS 共享 `ProxyConfig` / `TlsConfig`，WS 新增 `proxy` / `tls` / `sendTimeoutMs`。

### 已移除字段

- **`networkPathId` / `network_path_id`**：从未随正式版本发布，0.3.15 中移除。网络切换恢复请改用 `networkChanged()`。传入该字段会被静默忽略，不会报错。

---

## 一、axios → @eric8810/catcher-http

### 为什么迁移

| | axios | catcher-http |
|--|-------|-------------|
| 重试 | ❌ 需手动实现 | ✅ 内置，支持 fixed/exponential 退避 |
| 熔断器 | ❌ | ✅ 内置三态熔断器 |
| 并发控制 | ❌ | ✅ 优先级队列 |
| 连接池 | 通过 Agent 配置 | ✅ `keepAlive: true` + DNS 缓存开箱即用 |
| 拦截器 | ✅ | ✅ 完全兼容 axios API |
| SSE 流 | ❌ | ✅ `createSSEStream` / `createSSEClient` |

### API 对照

```typescript
// === 创建实例 ===
// axios
import axios from 'axios'
const client = axios.create({ baseURL: 'https://api.example.com', timeout: 5000 })

// catcher
import { createHttpClient } from '@eric8810/catcher-http'
const client = createHttpClient({
  baseURL: 'https://api.example.com',
  timeout: 5000,
  retry: { attempts: 3 },                // 新增
  circuitBreaker: { failureThreshold: 5, resetTimeout: 30000 },  // 新增
  concurrency: 10,                       // 新增
})

// === 请求方法 (完全相同) ===
// axios
const data = await axios.get('/users/1')
await axios.post('/messages', { text: 'hello' })

// catcher
const data = await client.get('/users/1')
await client.post('/messages', { text: 'hello' })

// === Per-request 覆盖 ===
// axios
await axios.get('/analytics', { timeout: 5000 })

// catcher — 同样的 config，多了 retry 控制
await client.get('/analytics', { timeout: 5000, retry: false, priority: 1 })

// === 拦截器 (完全相同) ===
// axios
axios.interceptors.request.use(config => {
  config.headers['Authorization'] = `Bearer ${token}`
  return config
})

// catcher
client.interceptors.request.use(config => {
  config.headers['Authorization'] = `Bearer ${token}`
  return config
})

// === 取消请求 ===
// axios
const source = axios.CancelToken.source()
axios.get('/slow', { cancelToken: source.token })
source.cancel()

// catcher — 标准 AbortSignal
const controller = new AbortController()
client.get('/slow', { signal: controller.signal })
controller.abort()

// === 上传/下载进度 ===
// 完全相同
await client.post('/upload', formData, {
  onUploadProgress: e => console.log(`${e.loaded}/${e.total}`),
})
```

### 错误处理迁移

```typescript
// axios
try {
  await axios.get('/data')
} catch (error) {
  if (axios.isAxiosError(error)) {
    console.error(error.response?.status, error.message)
  }
}

// catcher
import { isCatcherError } from '@eric8810/catcher-core'

try {
  await client.get('/data')
} catch (error) {
  if (isCatcherError(error)) {
    // 丰富的错误上下文
    console.error(error.type, error.request.url, `attempt ${error.attempt}/${error.request.config.retry?.attempts}`)
    console.error(`耗时 ${error.elapsedMs}ms`)
    // 安全序列化（敏感头已脱敏）
    console.error(JSON.stringify(error.toJSON()))
  }
}
```

### 不需要改的地方

- `client.get/post/put/delete/patch` — 签名完全兼容
- `client.interceptors.request.use()` / `client.interceptors.response.use()` — API 完全兼容
- `config.params` / `config.headers` / `config.signal` — 完全兼容
- `config.onUploadProgress` / `config.onDownloadProgress` — 完全兼容
- `config.responseType: 'json' | 'text' | 'stream'` — 完全兼容

---

## 二、fetch → @eric8810/catcher-web

### 为什么迁移

| | 原生 fetch | catcher-web |
|--|----------|------------|
| 重试 | ❌ | ✅ 指数退避 |
| 熔断器 | ❌ | ✅ |
| 拦截器 | ❌ | ✅ axios 兼容 API |
| 超时 | 需手动 AbortController | ✅ 内置 `timeout` |
| JSON 自动解析 | ❌ 需手动 `.json()` | ✅ 自动 |

### API 对照

```typescript
// === fetch ===
const resp = await fetch('https://api.example.com/users/1', {
  method: 'GET',
  headers: { 'Authorization': 'Bearer xxx' },
})
const data = await resp.json()

// === catcher-web ===
import { createWebClient } from '@eric8810/catcher-web'

const client = createWebClient({
  baseURL: 'https://api.example.com',
  retry: { attempts: 3 },
})

const data = await client.get('/users/1', {
  headers: { 'Authorization': 'Bearer xxx' }
})
// data 已经是解析后的 JSON 对象

// === POST ===
// fetch
const resp = await fetch('/messages', {
  method: 'POST',
  headers: { 'Content-Type': 'application/json' },
  body: JSON.stringify({ text: 'hello' }),
})

// catcher-web
await client.post('/messages', { text: 'hello' })
// body 自动 JSON.stringify

// === 取消 ===
// fetch — 标准 AbortController，catcher 同样支持
const controller = new AbortController()
await client.get('/slow', { signal: controller.signal })
controller.abort()

// === CORS ===
// fetch
fetch(url, { mode: 'cors', credentials: 'include' })
// catcher-web
createWebClient({
  fetchMode: 'cors',
  credentials: 'include',
})
```

---

## 三、ws 库 → @eric8810/catcher-ws

### 为什么迁移

| | ws 库 | catcher-ws |
|--|------|-----------|
| 自动重连 | ❌ 需手 write | ✅ 指数退避 + jitter |
| 多端点竞速 | ❌ | ✅ 同时连接选最快 |
| msgpack 编解码 | ❌ | ✅ `pack` / `unpack` |
| per-message-deflate | ✅ | ✅ 默认开启 |
| 代理支持 | ❌ | ✅ HTTP/SOCKS5 |

### API 对照

```typescript
// === ws 库 ===
import WebSocket from 'ws'
const ws = new WebSocket('wss://api.example.com')
ws.on('open', () => ws.send('hello'))
ws.on('message', data => console.log(data.toString()))

// === catcher-ws ===
import { createResilientWS, pack, decodeWSMessage } from '@eric8810/catcher-ws'

const ws = createResilientWS({
  url: ['wss://cn.example.com', 'wss://sg.example.com'],  // 多端点
  reconnect: { initialDelay: 1000, maxDelay: 30000 },
})

// 事件监听 — EventTarget 兼容 API
ws.addEventListener('open', () => console.log('connected'))
ws.addEventListener('message', e => {
  const data = decodeWSMessage(e.data)
  console.log(data)
})

ws.send('hello')                        // 文本
ws.send(pack({ event: 'msg', data }))   // binary (msgpack)
ws.close()
```

### 关键区别

| 行为 | ws | catcher-ws |
|------|-----|-----------|
| `close()` 后重连 | 需要手动重新 new | 自动，除非到达 `maxAttempts` |
| 多 URL 故障转移 | 不支持 | `url: ['a', 'b', 'c']` 竞速 + 失败轮换 |
| 心跳 | 手动实现 ping/pong | ❌ TS 版未集成（Rust 版支持自适应心跳） |

---

## 四、EventSource → catcher SSE

### 为什么迁移

| | EventSource | catcher SSE |
|--|-----------|------------|
| HTTP method | 仅 GET | GET / POST |
| POST + body | ❌ | ✅ (AI 流式 API) |
| 重连策略 | 浏览器固定 | 指数退避 + jitter + Last-Event-ID |
| 熔断器 | ❌ | ✅ |
| 取消 | `close()` | `close()` + AbortSignal |
| Node.js | ❌ (需 polyfill) | ✅ 原生支持 |

### API 对照

```typescript
// === EventSource (浏览器) ===
const es = new EventSource('/events')
es.onmessage = (e) => console.log(e.data)

// === catcher SSE (Node.js + 浏览器) ===
import { createSSEClient } from '@eric8810/catcher-http'  // Node.js
// import { createSSEClient } from '@eric8810/catcher-web' // 浏览器

const client = createSSEClient({
  url: 'https://api.example.com/events',
  reconnect: { initialDelay: 1000, maxDelay: 30000 },
  circuitBreaker: { failureThreshold: 5, resetTimeout: 30000 },
})

for await (const line of client) {
  if (line.startsWith('data: ')) {
    console.log(line.slice(6))
  }
}

client.close()
```

### AI 流式对话 — 原生 EventSource 不支持的场景

```typescript
// OpenAI 兼容 — EventSource 无法发 POST
import { createSSEStream } from '@eric8810/catcher-http'

const stream = createSSEStream({
  url: 'https://api.openai.com/v1/chat/completions',
  method: 'POST',
  headers: { Authorization: `Bearer ${apiKey}` },
  body: { model: 'gpt-4', messages, stream: true },
})

for await (const line of stream) {
  // 处理 data: 行...
}
```

---

## 五、Dart http → catcher_core

```dart
// === http 包 ===
import 'package:http/http.dart' as http;
final resp = await http.get(Uri.parse('https://api.example.com/users/1'));

// === catcher_core ===
import 'package:catcher_core/catcher_core.dart';
final client = CatcherHttpClient(HttpClientConfig(
  baseUrl: 'https://api.example.com',
  retry: RetryConfig(maxAttempts: 3),
));
final data = await client.get('/users/1');

// === http.post ===
await http.post(Uri.parse('/messages'), body: jsonEncode({'text': 'hello'}));

// === catcher_core.post ===
await client.post('/messages', body: {'text': 'hello'});
```

---

## 六、Dart web_socket_channel → catcher_core

```dart
// === web_socket_channel ===
final channel = WebSocketChannel.connect(Uri.parse('wss://example.com'));
channel.stream.listen((data) => print(data));

// === catcher_core ===
final ws = CatcherWsClient(WsClientConfig(
  urls: ['wss://example.com'],
  reconnect: ReconnectConfig(initialDelayMs: 1000, maxDelayMs: 30000),
));
ws.events.listen((event) {
  if (event is WsMessageEvent) print(event.text);
});
```

---

## 快速对照总表

| 原库 | catcher 替代 | 新增能力 |
|------|------------|---------|
| `axios` | `createHttpClient()` | retry, CB, queue, keepAlive, SSE |
| `fetch()` | `createWebClient()` | retry, CB, interceptor, auto JSON parse |
| `ws` | `createResilientWS()` | auto-reconnect, multi-endpoint, msgpack |
| `EventSource` | `createSSEClient()` | POST support, exp backoff, CB |
| `http` (Dart) | `CatcherHttpClient()` | retry, CB, keepAlive |
| `web_socket_channel` | `CatcherWsClient()` | auto-reconnect, multi-endpoint, heartbeat |
