# 14 — 浏览器端 WebSocket 客户端测试设计

> 测试设计文档 · 覆盖 `catcher-web/src/ws/client.ts`
> 用例编号规则：Browser WS `BW1-BWxx`。
> Mock 方式：Mock `WebSocket` 构造函数，拦截 `onopen/onclose/onmessage/onerror` 回调。

## 测试范围

### 公共 API（catcher-web）

| API | 源文件 | 说明 |
|-----|--------|------|
| `createWebSocketClient(options)` | `ws/client.ts` | 浏览器端 WebSocket 客户端 |
| `WebSocketClient.send/close/status/url` | `ws/client.ts` | 实例方法 |
| `WebSocketClient.addEventListener/removeEventListener` | `ws/client.ts` | 事件系统 |
| `createReconnectState(opts?)` | `ws/client.ts`（内部） | 退避策略 |

### 与 catcher-ws-ts 的区别

| 维度 | catcher-ws-ts (Node) | catcher-web (Browser) |
|------|---------------------|----------------------|
| 底层 | `ws` npm 包 | 原生 `WebSocket` API |
| 连接竞速 | `raceEndpoints()` 支持 | 不支持（顺序尝试） |
| 压缩 | `perMessageDeflate` 选项 | 不支持 |
| 自定义 headers | `headers` 选项 | 不支持 |
| Msgpack 编解码 | 集成 | 不在 WS 层 |

---

## 测试分层

```
┌─────────────────────────────────────────────────────────────┐
│           单元测试（Mock WebSocket 构造函数）                   │
│  Mock: 拦截 new WebSocket()，控制 onopen/onclose/onmessage    │
│  验证: 连接生命周期、重连、事件系统、退避策略                     │
└─────────────────────────────────────────────────────────────┘
```

### Mock 工具函数

```typescript
interface MockWebSocket {
  url: string
  protocols?: string | string[]
  binaryType: BinaryType
  readyState: number
  send: ReturnType<typeof vi.fn>
  close: ReturnType<typeof vi.fn>
  onopen: ((ev: Event) => void) | null
  onclose: ((ev: CloseEvent) => void) | null
  onmessage: ((ev: MessageEvent) => void) | null
  onerror: ((ev: Event) => void) | null
}

// Mock WebSocket 构造函数
function mockWebSocketCtor(): {
  instances: MockWebSocket[]
  ctor: typeof WebSocket
}
```

---

## 一、连接生命周期

| # | 测试名 | Mock 方式 | 断言 |
|---|--------|---------|------|
| BW1 | 成功连接 → open 事件 | mock WS, 触发 onopen | `status === 'CONNECTED'`, open listener 被调用 |
| BW2 | 连接时设置 binaryType | `{ binaryType: 'arraybuffer' }` | ws.binaryType === 'arraybuffer' |
| BW3 | 使用 protocols | `{ protocols: ['proto1'] }` | WebSocket 构造参数含 protocols |
| BW4 | 多 URL 时使用第一个 | `{ url: ['ws://a', 'ws://b'] }` | 构造参数 url === 'ws://a' |

## 二、消息收发

| # | 测试名 | 操作 | 断言 |
|---|--------|------|------|
| BW5 | send 发送文本 | 连接后 send('hello') | ws.send 被调用，参数 'hello' |
| BW6 | send 发送 ArrayBuffer | send(Uint8Array) | ws.send 被调用 |
| BW7 | 未连接时 send 不抛错 | 未 open 时 send | ws.send 未调用（静默忽略） |
| BW8 | 接收消息 → message 事件 | 触发 onmessage | message listener 被调用 |

## 三、关闭与重连

| # | 测试名 | 操作 | 断言 |
|---|--------|------|------|
| BW9 | close() → close 事件 | client.close() | ws.close 被调用, close listener 触发 |
| BW10 | close() 后不重连 | close() + 服务端观察 | 无第二次 WebSocket 构造 |
| BW11 | close(code, reason) 传递参数 | close(1000, 'done') | ws.close(1000, 'done') |
| BW12 | 服务端关闭后自动重连 | 触发 onclose | 第二次 WebSocket 构造 |
| BW13 | maxAttempts 耗尽停止 | 连续拒绝 | 构造次数 ≤ maxAttempts + 1 |
| BW14 | 重连成功后退避重置 | 第一次 close → 重连成功 → 再 close | 第二次重连从 initialDelay 开始 |
| BW15 | 握手超时 → close(4000) | 10s 后触发 onopen | ws.close(4000, 'Handshake timeout') |

## 四、事件系统

| # | 测试名 | 操作 | 断言 |
|---|--------|------|------|
| BW16 | addEventListener 注册 | 注册 open listener | 连接时被调用 |
| BW17 | removeEventListener 移除 | 注册 + 移除 | 不再被调用 |
| BW18 | 多 listener 同类型 | 注册 2 个 open listener | 都被调用 |
| BW19 | error 事件分发 | 触发 onerror | error listener 被调用 |
| BW20 | url 属性 | 多 URL 场景 | 返回实际连接的 URL |

## 五、退避策略（createReconnectState）

| # | 测试名 | 配置 | 断言 |
|---|--------|------|------|
| BW21 | 首次延迟 ≈ initialDelay | `{ initialDelay: 500 }` | 延迟约 500ms |
| BW22 | 指数增长 | `{ initialDelay: 100, backoffMultiplier: 2 }` | 100 → 200 → 400 |
| BW23 | maxDelay 上限 | `{ maxDelay: 5000 }` | 不超过 5000ms |
| BW24 | maxAttempts 后返回 -1 | `{ maxAttempts: 3 }` | 第 4 次 nextDelay() === -1 |
| BW25 | reset() 重置计数 | 失败 2 次 → reset() | 重新从 initialDelay 开始 |

---

## 六、测试覆盖矩阵

| 设计要点 | 生命周期 | 消息 | 关闭/重连 | 事件 | 退避 |
|---------|:-------:|:----:|:--------:|:----:|:----:|
| 连接建立 | BW1-BW4 | | | | |
| 消息收发 | | BW5-BW8 | | | |
| 主动关闭 | | | BW9-BW11 | | |
| 自动重连 | | | BW12-BW14 | | |
| 握手超时 | | | BW15 | | |
| 事件注册/移除 | | | | BW16-BW20 | |
| 退避计算 | | | | | BW21-BW25 |

### 不测试的范围

| 不测试 | 原因 |
|--------|------|
| 真实浏览器 WebSocket | 需要 Playwright E2E |
| 并发连接压力测试 | 非 Catcher WS 职责 |
| 二进制编解码 | 由 catcher-ws-ts codec 模块保证 |
