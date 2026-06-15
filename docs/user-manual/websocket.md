# WebSocket 指南

> catcher 的 WebSocket 客户端为实时通信场景提供多端点竞速、自动重连、per-message-deflate 压缩、应用层压缩（gzip/zstd）和 msgpack 编解码能力。  
> 覆盖平台：Node.js (TS) / Rust / Flutter (dart:ffi)

---

## 目录

- [功能概览](#功能概览)
- [基本用法](#基本用法)
- [多端点竞速](#多端点竞速)
- [自动重连策略](#自动重连策略)
- [编解码：msgpack vs JSON](#编解码msgpack-vs-json)
- [Per-Message Deflate 压缩](#per-message-deflate-压缩)
- [心跳与 RTT 检测](#心跳与-rtt-检测)
- [代理与 TLS](#代理与-tls)
- [跨平台 API 对照](#跨平台-api-对照)

---

## 功能概览

| 特性 | 说明 |
|------|------|
| 多端点竞速 | 同时连接多个 URL，取最快建立的连接，其余立即关闭 |
| 自动重连 | 指数退避 + ±25% 抖动，可配置最大重试次数 |
| Per-Message Deflate | zlib 压缩（RFC 7692），可设阈值，仅压缩大于阈值的帧 |
| 应用层压缩 | gzip/zstd envelope 压缩，不依赖 WebSocket 扩展协商 |
| Msgpack 编解码 | 二进制序列化，比 JSON 更小的体积 |
| 心跳检测 | 自适应 ping/pong，连续丢包自动判定断线 |
| 握手超时 | 默认 10s，防止 TCP 连通但握手卡死 |
| 代理支持 | HTTP/HTTPS/SOCKS5 代理，自动读取环境变量 |
| 自定义 Headers / Cookie | 适用于鉴权握手场景 |

---

## 基本用法

### Node.js / TypeScript

```typescript
import { createResilientWS } from '@eric8810/catcher-ws'

const ws = createResilientWS({
  url: 'wss://api.example.com/ws',
  perMessageDeflate: true,
})

ws.addEventListener('open', () => {
  console.log('已连接', ws.url)
})

ws.addEventListener('message', (e) => {
  console.log('收到消息', e.data)
})

ws.addEventListener('statuschange', () => {
  console.log('状态变更', ws.status) // 'CONNECTING' | 'CONNECTED' | 'CLOSED'
})

ws.send('hello')
ws.send(new Uint8Array([1, 2, 3]))

// 主动关闭（不会触发自动重连）
ws.close(1000, 'bye')
```

`createResilientWS` 返回的 `ResilientWS` 对象实现了 `EventTarget` 接口：

| 属性 / 方法 | 类型 | 说明 |
|-------------|------|------|
| `send(data)` | `(string \| Uint8Array) => void` | 发送文本或二进制消息 |
| `close(code?, reason?)` | `(number?, string?) => void` | 主动关闭，不触发重连 |
| `readyState` | `number` | 底层 WebSocket 的 readyState |
| `url` | `string` | 当前活跃连接的端点 URL |
| `status` | `'CONNECTING' \| 'CONNECTED' \| 'CLOSED'` | catcher 管理的连接状态 |
| `addEventListener(type, listener)` | — | 监听事件 |
| `removeEventListener(type, listener)` | — | 移除事件 |

事件类型：

| 事件 | 说明 |
|------|------|
| `'open'` | 连接建立成功 |
| `'close'` | 连接关闭（`event.code` + `event.reason`） |
| `'message'` | 收到消息（`event.data`） |
| `'error'` | 连接错误（`event.error`） |
| `'statuschange'` | `status` 属性发生变化 |

### 完整配置项

```typescript
const ws = createResilientWS({
  url: 'wss://api.example.com/ws',     // 或多端点数组
  protocol: 'v2',                       // WebSocket 子协议
  perMessageDeflate: true,              // 或 { threshold: 1024 }
  handshakeTimeout: 10_000,             // 握手超时 (ms)
  maxPayload: 1024 * 1024,             // 最大帧大小 (1MB)
  reconnect: {                          // 重连策略
    initialDelay: 1000,
    maxDelay: 30_000,
    backoffMultiplier: 2,
    maxAttempts: 20,
  },
  raceCount: 3,                         // 多端点时同时竞速的数量
  headers: { Authorization: 'Bearer ...' }, // 自定义握手 headers
  cookie: 'session=abc123',             // Cookie header
  proxy: 'socks5://127.0.0.1:1080',    // 代理
  rejectUnauthorized: true,            // TLS 证书验证
})
```

---

## 多端点竞速

在分布式部署场景中，WebSocket 服务通常有多个地域节点。传统做法是顺序尝试每个端点，延迟叠加。catcher 采用**竞速连接**：同时向多个端点发起连接，取最先成功的，其余连接立即关闭。

### 工作原理

```
时间 ──────────────────────────────────────────────►

客户端  ┌─── 连接 wss://cn.example.com ──── TCP 握手 ──── WS 握手 ──✗ 失败
        │
        ├─── 连接 wss://sg.example.com ──── TCP 握手 ──── WS 握手 ──✓ 成功 ✓ 选用
        │
        └─── 连接 wss://us.example.com ──── TCP 握手 ... → 取消关闭
                                                        ↑
                                                   最快者胜出
```

### 使用方式

**方式一：传 URL 数组（推荐）**

```typescript
import { createResilientWS } from '@eric8810/catcher-ws'

const ws = createResilientWS({
  url: [
    'wss://cn.example.com/ws',
    'wss://sg.example.com/ws',
    'wss://us.example.com/ws',
    'wss://eu.example.com/ws',
    'wss://jp.example.com/ws',
  ],
  raceCount: 3,  // 最多同时竞速 3 个端点
})
```

- `url` 传入数组时，取前 `raceCount` 个端点同时连接
- 默认 `raceCount = 3`，避免一次性打开过多连接
- 最先完成 WebSocket 握手的端点被选中，其余连接立即关闭
- 如果竞速的全部端点都失败，触发自动重连

**方式二：独立函数**

```typescript
import { raceEndpoints } from '@eric8810/catcher-ws'

const { socket, endpoint } = await raceEndpoints(
  ['wss://a.example.com', 'wss://b.example.com'],
  { handshakeTimeout: 5000 },
  15_000,  // 全局竞速超时 (ms)
)
```

`raceEndpoints` 是独立的底层函数，返回 `{ socket, endpoint }`。全局超时默认 15 秒，超时后拒绝所有连接。

### 竞速与重连的配合

当多端点竞速全部失败时，`createResilientWS` 会自动调度重连策略（下一节），再次以竞速方式尝试连接，直到成功或重试次数耗尽。

```
竞速失败 → 等待退避延迟 → 再次竞速 → 竞速失败 → ... → 成功 → 连接稳定
                ↑                                              |
                └────── 断开后重新触发 ◄────── 连接断开 ────────┘
```

---

## 自动重连策略

catcher 的重连策略使用**指数退避 + 随机抖动**，避免雪崩效应。

### 核心算法

```
delay = min(initialDelay × backoffMultiplier^(attempt-1), maxDelay)
delay = delay ± 25% (jitter)
```

### 时间线示例

以默认参数为例（`initialDelay=1000ms, maxDelay=30000ms, backoffMultiplier=2, maxAttempts=20`）：

```
连接断开
  │
  ├── attempt 1:  delay ≈  1000ms ± 25%  → [750ms .. 1250ms]
  ├── attempt 2:  delay ≈  2000ms ± 25%  → [1500ms .. 2500ms]
  ├── attempt 3:  delay ≈  4000ms ± 25%  → [3000ms .. 5000ms]
  ├── attempt 4:  delay ≈  8000ms ± 25%  → [6000ms .. 10000ms]
  ├── attempt 5:  delay ≈ 16000ms ± 25%  → [12000ms .. 20000ms]
  ├── attempt 6:  delay ≈ 30000ms ± 25%  → [22500ms .. 37500ms]  ← 触达上限
  │  ...
  ├── attempt 20: delay ≈ 30000ms ± 25%
  │
  └── attempt 21: 返回 -1 → 停止重连
```

### 自定义重连参数

```typescript
import { createReconnectStrategy } from '@eric8810/catcher-ws'

const strategy = createReconnectStrategy({
  initialDelay: 500,       // 首次重试延迟 (ms)
  maxDelay: 60_000,        // 最大延迟 (ms)
  backoffMultiplier: 2,    // 退避乘数
  maxAttempts: 10,         // 最大重试次数
})

// 也可独立使用
console.log(strategy.nextDelay())   // ~500ms
console.log(strategy.nextDelay())   // ~1000ms
console.log(strategy.attemptCount)  // 2

// 连接成功后重置
strategy.reset()
```

| 参数 | 默认值 | 说明 |
|------|--------|------|
| `initialDelay` | 1000 | 首次退避延迟 (ms) |
| `maxDelay` | 30000 | 退避上限 (ms) |
| `backoffMultiplier` | 2 | 每次延迟的倍增系数 |
| `maxAttempts` | 20 | 最大重试次数，超出后停止 |

**关键行为**：

- `nextDelay()` 返回 `-1` 表示重试次数已耗尽，应停止重连
- `reset()` 在连接成功后调用，将计数器归零
- 抖动范围 ±25%，防止多客户端同时重连导致服务端过载

### Rust 端状态机

Rust 实现通过 `ReconnectManager` 维护更完整的状态机：

```
DISCONNECTED → CONNECTING → (成功) → CONNECTED → (断开) → RECONNECTING
RECONNECTING → CONNECTING → ...                          → (耗尽) → DISCONNECTED
```

`on_disconnect()` 返回 `Option<u64>` — `Some(delay)` 表示继续重连，`None` 表示耗尽。`on_connected()` 重置所有状态。

---

## 编解码：msgpack vs JSON

catcher 提供 msgpack 二进制编解码工具，适用于高频小消息场景。相比 JSON，msgpack 编码更紧凑、解析更快。

### API

```typescript
import { pack, unpack, isBinary, decodeWSMessage } from '@eric8810/catcher-ws'
```

| 函数 | 签名 | 说明 |
|------|------|------|
| `pack(value)` | `(any) => Buffer` | 编码为 msgpack 二进制 |
| `unpack(buffer)` | `(Buffer \| Uint8Array) => any` | 解码 msgpack |
| `isBinary(data)` | `(any) => boolean` | 判断是否为二进制数据 |
| `decodeWSMessage(data)` | `(any) => any` | 自动检测并解码（二进制→msgpack，文本→JSON） |

### 使用示例

**发送 msgpack 编码消息**：

```typescript
import { createResilientWS, pack } from '@eric8810/catcher-ws'

const ws = createResilientWS({ url: 'wss://api.example.com/ws' })

ws.addEventListener('open', () => {
  // 发送 msgpack 二进制帧
  const payload = { type: 'chat', text: '你好', ts: Date.now() }
  ws.send(pack(payload))
})
```

**接收并自动解码**：

```typescript
import { decodeWSMessage } from '@eric8810/catcher-ws'

ws.addEventListener('message', (e) => {
  // 自动检测：二进制 → msgpack 解码，文本 → JSON 解析
  const data = decodeWSMessage(e.data)
  console.log(data)
})
```

**手动判断类型**：

```typescript
import { isBinary, unpack } from '@eric8810/catcher-ws'

ws.addEventListener('message', (e) => {
  if (isBinary(e.data)) {
    // msgpack 二进制帧
    const decoded = unpack(e.data)
  } else {
    // 文本帧 → JSON.parse
    const decoded = JSON.parse(e.data)
  }
})
```

### 体积对比

以一个典型的聊天消息为例：

```typescript
const msg = {
  event: 'message',
  id: 'msg_001',
  from: 'user_001',
  to: 'channel_general',
  text: 'Hello Hello Hello ...',  // 180 字符
  ts: 1700000000,
}
```

| 编码 | 大小 | 对比 |
|------|------|------|
| JSON | ~215 bytes | 基准 |
| Msgpack | ~185 bytes | 约减少 14% |

嵌套越深、数值字段越多，msgpack 的体积优势越明显。

### Rust 端编解码

Rust 使用 `rmp-serde` + `rmpv` 实现，API 略有不同：

```rust
use catcher_ws::codec::{pack, unpack, unpack_value};

// 强类型编解码
let payload = MyStruct { /* ... */ };
let bytes = pack(&payload)?;
let decoded: MyStruct = unpack(&bytes)?;

// 解码为通用 serde_json::Value
let value: serde_json::Value = unpack_value(&bytes)?;
```

---

## 应用层压缩（Flutter / Rust）

Rust/Flutter 路径支持应用层 gzip/zstd 压缩。它不依赖 WebSocket 扩展协商，而是在二进制消息中包一层 catcher envelope；服务端完成适配后，可以和客户端双向发送压缩帧。

### Flutter 启用方式

```dart
final ws = CatcherWsClient(WsClientConfig(
  urls: ['wss://api.example.com/ws'],
  applicationCompression: WsApplicationCompressionConfig(
    algorithm: WsApplicationCompressionAlgorithm.zstd,
    thresholdBytes: 2048,
  ),
));
```

默认行为：

| 参数 | 默认 | 说明 |
|------|------|------|
| `enabled` | `true` | 只要传入 `applicationCompression` 即启用 |
| `algorithm` | `gzip` | 可选 `gzip` / `zstd` |
| `thresholdBytes` | `1024` | 小于阈值的消息保持原始 text/binary 帧 |

### Wire format

压缩后的消息总是以二进制帧发送。服务端按下面的 envelope 解析：

```text
bytes 0..12   magic: "CATCHER-CMP-1"
byte 13       algorithm: 1 = gzip, 2 = zstd
byte 14       original kind: 1 = text, 2 = binary
bytes 15..18  uncompressed length, uint32 big-endian
bytes 19..    compressed payload
```

服务端适配规则：

1. 握手时可读取 `X-Catcher-Application-Compression`、`X-Catcher-Application-Compression-Format`、`X-Catcher-Application-Compression-Threshold` 判断客户端能力。
2. 收到二进制帧时，先判断是否以 `CATCHER-CMP-1` 开头。
3. 如果命中 envelope，根据 algorithm 解压 payload。
4. 根据 original kind 把解压后的 bytes 还原为文本消息或二进制消息。
5. 未命中 envelope 的 text/binary 帧按原协议处理。
6. 服务端也可以用相同 envelope 回发压缩消息；客户端会自动解压并恢复 `WsMessageEvent.isBinary`。

当前实现参数：gzip 使用 level 6，zstd 使用 level 3。客户端会校验解压后的长度不能超过 `maxPayloadBytes`。

> `perMessageDeflate` 开启时，Rust/Flutter 会优先使用标准 WebSocket 扩展压缩，并跳过应用层 envelope，避免双重压缩。应用层 gzip/zstd 适合作为旧服务端或非标准网关的 fallback。

---

## Per-Message Deflate 压缩

WebSocket 的 per-message-deflate 扩展（RFC 7692）可在帧级别对消息进行 zlib 压缩，减少传输量。

Node.js 使用 `ws` 包协商 `permessage-deflate`；Rust/Flutter/napi 现在使用 `yawc` 协商同一个 RFC 7692 扩展。服务端只需要按标准 WebSocket `Sec-WebSocket-Extensions: permessage-deflate` 握手和 RSV1 数据帧处理，不需要为 Flutter 另做私有协议。

如需单独发给服务端同事，可参考 [`websocket-permessage-deflate-server-integration.md`](./websocket-permessage-deflate-server-integration.md)。

### 启用方式

```dart
final ws = CatcherWsClient(WsClientConfig(
  urls: ['wss://api.example.com/ws'],
  perMessageDeflate: true, // 默认启用
));
```

启用后，catcher 使用以下压缩参数：

| 参数 | 值 | 说明 |
|------|-----|------|
| `level` | 6 | zlib 压缩级别（1-9，6 为速度/压缩比平衡） |
| `context takeover` | negotiated | 默认保留上下文，与 Node.js `ws` 默认行为一致 |

### Node.js 自定义阈值

```typescript
// Node.js ws 路径支持 threshold 对象配置
const ws = createResilientWS({
  url: 'wss://api.example.com/ws',
  perMessageDeflate: { threshold: 4096 },
})
```

Flutter/Rust 当前公开的是布尔开关；`deflateThresholdBytes` 字段保留为兼容配置，但标准 RFC 7692 后端不会用应用层阈值去改写每帧压缩行为。

### 关闭压缩

```dart
final ws = CatcherWsClient(WsClientConfig(
  urls: ['wss://api.example.com/ws'],
  perMessageDeflate: false,
));
```

> **注意**：压缩需要服务端支持 per-message-deflate 扩展。如果服务端在握手响应中未协商该扩展，即使客户端启用也不会生效。

---

## 心跳与 RTT 检测

> 心跳功能目前仅在 Rust 原生端（Rust crate / Flutter / napi）可用。TS 纯 JS 版本尚未集成心跳管理器。

Rust 端的 `HeartbeatManager` 提供：

| 配置项 | 默认值 | 说明 |
|--------|--------|------|
| `interval_ms` | 30000 | ping 间隔 (ms) |
| `adaptive` | true | 是否根据 RTT 自适应调整间隔 |
| `pong_timeout_ms` | 10000 | pong 超时 (ms) |
| `max_missed_pongs` | 3 | 连续丢失 pong 上限 |

### 自适应间隔

当 `adaptive = true` 时，心跳间隔根据 P90 RTT 动态调整：

```
effective_interval = max(P90_RTT × 2, configured_interval)
```

- RTT 采样窗口：最近 20 次 pong 响应
- 高延迟环境下自动加大间隔，避免误判
- 低延迟环境下保持配置的 `interval_ms`

### 断线判定

```
ping → 等待 pong → 超时 → missed_pongs++
                         → missed_pongs >= max_missed_pongs → 判定断线 → 触发重连
```

Flutter 端通过事件流暴露 RTT 数据：

```dart
if (event is WsHeartbeatRttEvent) {
  print('RTT: ${event.rttMs}ms');
}
```

---

## 代理与 TLS

### 代理支持（Node.js TS 版）

```typescript
const ws = createResilientWS({
  url: 'wss://api.example.com/ws',
  proxy: true,  // 自动读取 HTTPS_PROXY / HTTP_PROXY 环境变量
})

// 或指定代理 URL
const ws = createResilientWS({
  url: 'wss://api.example.com/ws',
  proxy: 'socks5://127.0.0.1:1080',  // SOCKS5
})

// 或完整配置
const ws = createResilientWS({
  url: 'wss://api.example.com/ws',
  proxy: {
    url: 'http://proxy.example.com:8080',
    auth: { username: 'user', password: 'pass' },
    noProxy: ['internal.example.com'],
  },
})
```

依赖的可选包：
- HTTP/HTTPS 代理：`https-proxy-agent`
- SOCKS5 代理：`socks-proxy-agent`

如果未安装对应包，catcher 会打印警告并跳过代理。

### TLS 证书验证

```typescript
// 开发环境跳过自签名证书验证
const ws = createResilientWS({
  url: 'wss://localhost:8443/ws',
  rejectUnauthorized: false,
})
```

### 自定义 Headers 与 Cookie

```typescript
const ws = createResilientWS({
  url: 'wss://api.example.com/ws',
  headers: {
    'Authorization': 'Bearer token123',
    'X-Custom-Header': 'value',
  },
  cookie: 'session=abc123; theme=dark',
})
```

Cookie 会被注入到 WebSocket 握手的 HTTP 请求头中。

---

## 跨平台 API 对照

| 特性 | Node.js (TS) | Rust | Flutter (dart:ffi) |
|------|-------------|------|---------------------|
| **包名** | `@eric8810/catcher-ws` | `catcher-ws` | `catcher_core` |
| **入口** | `createResilientWS()` | `WsTransport::connect()` | `CatcherWsClient()` |
| **配置类型** | `ResilientWSOptions` | `WsClientConfig` | `WsClientConfig` |
| **多端点竞速** | `url: string[]` + `raceCount` | `WsClientConfig.urls` + `race_count`（`WsTransport::connect()` 内部处理） | `urls: List<String>` + `raceCount` |
| **独立竞速函数** | `raceEndpoints()` | —（`EndpointRacer` 为内部实现细节） | — |
| **重连策略** | `createReconnectStrategy()` | `ReconnectManager` | 内建（`WsReconnectConfig`） |
| **压缩** | `perMessageDeflate` | `per_message_deflate` | `perMessageDeflate` |
| **应用层压缩** | — | `application_compression` | `applicationCompression` |
| **Msgpack** | `pack()` / `unpack()` | `pack()` / `unpack()` | `CatcherCodec.pack()` / `.unpack()` |
| **心跳** | ❌ 未集成 | `HeartbeatManager` | ✅ `WsHeartbeatConfig` |
| **事件模型** | `EventTarget` 回调 | `mpsc::channel` | `Stream<WsEvent>` |
| **代理** | `proxy` 选项 | — | — |
| **Cookie** | `cookie` 选项 | — | — |
| **自定义 Headers** | `headers` 选项 | `headers: HashMap` | `headers: Map<String, String>` |

### Flutter 事件流

Flutter 使用 Dart `Stream` 暴露 WebSocket 事件，通过 `is` 类型判断区分：

```dart
import 'package:catcher_core/catcher_core.dart';

final ws = CatcherWsClient(WsClientConfig(
  urls: ['wss://api.example.com/ws'],
  reconnect: WsReconnectConfig(initialDelayMs: 1000),
  heartbeat: WsHeartbeatConfig(intervalMs: 30000),
));

ws.events.listen((event) {
  if (event is WsConnectedEvent) {
    print('已连接: ${event.url} (${event.latencyMs}ms)');
  } else if (event is WsMessageEvent) {
    if (event.isBinary) {
      // 处理二进制帧（msgpack）
      print('二进制: ${event.data}');
    } else {
      print('文本: ${event.text}');
    }
  } else if (event is WsDisconnectedEvent) {
    print('断开: ${event.code} ${event.reason}');
  } else if (event is WsReconnectingEvent) {
    print('重连中: 第 ${event.attempt} 次, ${event.delayMs}ms 后');
  } else if (event is WsHeartbeatRttEvent) {
    print('RTT: ${event.rttMs}ms');
  } else if (event is WsErrorEvent) {
    print('错误: ${event.message}');
  }
});

ws.sendText('hello');
ws.sendBinary([1, 2, 3, 4]);
```

### Rust 事件枚举

Rust 使用 `WsEvent` 枚举通过 `mpsc` channel 传递：

```rust
use catcher_ws::types::ws::WsEvent;

while let Some(event) = rx.recv().await {
    match event {
        WsEvent::Connected { url, latency_ms } => { /* ... */ }
        WsEvent::Disconnected { code, reason } => { /* ... */ }
        WsEvent::Reconnecting { attempt, delay_ms } => { /* ... */ }
        WsEvent::Message { data, is_binary } => { /* ... */ }
        WsEvent::Error { message } => { /* ... */ }
        WsEvent::HeartbeatRtt { rtt_ms } => { /* ... */ }
    }
}
```

### 默认值对照

| 配置项 | TS 默认值 | Rust 默认值 | Dart 默认值 |
|--------|----------|------------|------------|
| 握手超时 | 10,000ms | 15,000ms | 15,000ms |
| 最大 payload | 1MB | 64MB | 64MB |
| 初始退避延迟 | 1,000ms | 500ms | 500ms |
| 最大退避延迟 | 30,000ms | 30,000ms | 30,000ms |
| 退避乘数 | 2 | 2.0 | 2.0 |
| 最大重试次数 | 20 | 20 | 20 |
| 压缩阈值 | 1,024 bytes | 1,024 bytes | 256 bytes |
| 竞速数量 | 3 | 1 | 1 |
| 心跳间隔 | — | 30,000ms | 30,000ms |
| pong 超时 | — | 10,000ms | 10,000ms |
| 连续丢失上限 | — | 3 | 3 |

> **注意**：不同平台的默认值可能存在差异（如 TS 的 `initialDelay=1000ms` vs Rust/Dart 的 `500ms`）。跨平台使用时建议显式指定参数以保持一致行为。
