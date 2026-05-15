# @eric8810/catcher-ws API Reference

> Node.js WebSocket 客户端 — createResilientWS, pack/unpack, raceEndpoints

```bash
npm install @eric8810/catcher-ws
```

---

## 导出清单

```typescript
import {
  createResilientWS,
  createReconnectStrategy,
  raceEndpoints,
  pack,
  unpack,
  isBinary,
  decodeWSMessage,
} from '@eric8810/catcher-ws'
```

---

## createResilientWS

```typescript
function createResilientWS(options: ResilientWSOptions): ResilientWS
```

创建带完整韧性特性的 WebSocket 客户端。

### ResilientWSOptions

| 参数 | 类型 | 默认 | 说明 |
|------|------|------|------|
| `url` | `string \| string[]` | **必填** | 服务端 URL(s)。多值 = 多端点竞速 |
| `protocol` | `string \| string[]` | — | 子协议 |
| `perMessageDeflate` | `boolean \| { threshold }` | `true` | per-message deflate 压缩 |
| `handshakeTimeout` | `number` | `10000` | 握手超时（ms） |
| `maxPayload` | `number` | `1048576` | 最大载荷（字节，默认 1MB） |
| `reconnect` | `ReconnectOpts` | — | 自动重连 |
| `reconnect.initialDelay` | `number` | `1000` | 初始重连延迟（ms） |
| `reconnect.maxDelay` | `number` | `30000` | 最大重连延迟（ms） |
| `reconnect.backoffMultiplier` | `number` | `2` | 指数退避因子 |
| `reconnect.maxAttempts` | `number` | `20` | 最多重连次数 |
| `raceCount` | `number` | `3` | 多端点竞速数量 |
| `headers` | `Record<string, string>` | `{}` | 自定义请求头 |
| `cookie` | `string` | — | WebSocket 握手 Cookie |
| `rejectUnauthorized` | `boolean` | `true` | TLS 证书校验 |
| `proxy` | `boolean \| string \| ProxyConfig` | — | HTTP/SOCKS5 代理 |

### ResilientWS

```typescript
interface ResilientWS extends EventTarget {
  send(data: string | Uint8Array): void
  close(code?: number, reason?: string): void
  readonly readyState: number  // WebSocket.CONNECTING(0)/OPEN(1)/CLOSING(2)/CLOSED(3)
  readonly url: string
  readonly status: 'CONNECTING' | 'CONNECTED' | 'CLOSED'
  addEventListener(type: 'open' | 'close' | 'message' | 'error' | 'statuschange', listener: EventListener): void
  removeEventListener(type: string, listener: EventListener): void
}
```

### 事件类型

| 事件 | 触发时机 |
|------|---------|
| `open` | 连接成功 |
| `close` | 连接关闭（携带 `code` / `reason` 属性） |
| `message` | 收到消息（通过 `MessageEvent.data` 访问数据） |
| `error` | 发生错误（携带 `error` 属性） |
| `statuschange` | 状态变更（`CONNECTING` ↔ `CONNECTED` ↔ `CLOSED`） |

### 示例

```typescript
const ws = createResilientWS({
  url: ['wss://cn.example.com', 'wss://sg.example.com'],
  perMessageDeflate: true,
  reconnect: { initialDelay: 1000, maxDelay: 30000 },
})

ws.addEventListener('open', () => console.log('connected'))
ws.addEventListener('message', (e) => {
  const data = decodeWSMessage(e.data)
  console.log(data)
})
ws.addEventListener('close', (e) => {
  console.log(`closed: ${e.code} ${e.reason}`)
})

ws.send('hello')
ws.send(pack({ event: 'msg', data: { text: 'hi' } }))
ws.close()
```

---

## createReconnectStrategy

```typescript
function createReconnectStrategy(opts?: {
  initialDelay?: number    // 默认 1000
  maxDelay?: number         // 默认 30000
  backoffMultiplier?: number // 默认 2
  maxAttempts?: number      // 默认 20
}): ReconnectStrategy
```

创建可复用的重连策略对象。

```typescript
interface ReconnectStrategy {
  nextDelay(): number  // 返回下次延迟（-1 表示超过 maxAttempts）
  reset(): void
  readonly attemptCount: number
}
```

**退避算法**：`delay = min(initialDelay × multiplier^(attempt-1), maxDelay) + jitter(±25%)`

---

## raceEndpoints

```typescript
function raceEndpoints(
  urls: string[],
  options: WebSocket.ClientOptions,
  timeoutMs?: number   // 默认 15000
): Promise<{ socket: WebSocket; endpoint: string }>
```

同时连接多个端点，返回第一个成功的结果。其他连接自动关闭。

- 如果所有端点都失败 → `Error('All WebSocket endpoints failed')`
- 如果全局超时 → `Error('WebSocket race timeout after ...ms')`

---

## 编解码 API

### pack

```typescript
function pack(value: any): Buffer
```

将任意值编码为 msgpack 二进制（通过 msgpackr）。

### unpack

```typescript
function unpack(buffer: Buffer | Uint8Array): any
```

从 msgpack 二进制解码为 JS 值。

### isBinary

```typescript
function isBinary(data: any): data is Buffer
```

检查消息是否为二进制帧（`Buffer` / `ArrayBuffer` / `Uint8Array`）。

### decodeWSMessage

```typescript
function decodeWSMessage(data: any): any
```

自动检测并解码 WebSocket 消息帧：

- 二进制帧 → msgpack 解码
- 文本帧 → JSON 解码（fallback: 原样返回）

```typescript
// 发送端
ws.send(pack({ event: 'position', lat: 22.3, lng: 114.1 }))

// 接收端
ws.addEventListener('message', (e) => {
  const data = decodeWSMessage(e.data)
  // data = { event: 'position', lat: 22.3, lng: 114.1 }
})
```

---

## 默认值速查

| 参数 | 默认值 |
|------|--------|
| `handshakeTimeout` | `10000` ms |
| `maxPayload` | `1048576` (1MB) |
| `perMessageDeflate` | `true` |
| `perMessageDeflate.threshold` | `1024` 字节 |
| `reconnect.initialDelay` | `1000` ms |
| `reconnect.maxDelay` | `30000` ms |
| `reconnect.backoffMultiplier` | `2` |
| `reconnect.maxAttempts` | `20` |
| `raceCount` | `3` |
| `raceEndpoints timeoutsMs` | `15000` ms |
