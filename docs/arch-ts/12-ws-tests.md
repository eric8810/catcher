# 12 — WebSocket 客户端测试设计

> 测试设计文档 · 覆盖 catcher-ws-ts

## 测试范围

### TS 公共 API（catcher-ws-ts）

| API | 源文件 | 当前测试 |
|-----|--------|---------|
| `createResilientWS(options)` | `ws/client.ts` | ⚠️ 仅集成测试（benchmark 性质） |
| `createReconnectStrategy(opts?)` | `ws/reconnect.ts` | ❌ 无 |
| `raceEndpoints(urls, options, timeout?)` | `ws/multi-endpoint.ts` | ❌ 无 |
| `pack(value)` | `codec.ts` | ❌ 无 |
| `unpack(buffer)` | `codec.ts` | ❌ 无 |
| `isBinary(data)` | `codec.ts` | ❌ 无 |
| `decodeWSMessage(data)` | `codec.ts` | ❌ 无 |

### 已有集成测试（packages/test/integration/ws.test.ts）

| describe 块 | it 块 | 性质 |
|------------|-------|------|
| WS — message latency with perMessageDeflate | 2 个（good/weak） | 性能对比 |
| WS — reconnection with exponential backoff | 1 个 | 功能验证 |
| WS — multi-endpoint racing | 1 个 | 功能验证 |

> **问题**：所有测试均为 benchmark 性质（vanilla vs catcher 对比），无确定性单元测试。codec 模块和 reconnect 策略零覆盖。

---

## 测试分层

```
┌─────────────────────────────────────────────────────────────┐
│               集成测试（真实 WebSocket Server）                 │
│  vitest + ws.createServer 或 WebSocket.Server               │
│  验证: createResilientWS 完整连接/消息/断线/重连流程            │
└─────────────────────────────────────────────────────────────┘
                            ▲
┌─────────────────────────────────────────────────────────────┐
│                  单元测试（纯函数 / Mock）                      │
│  ReconnectStrategy: 退避计算 + jitter + maxAttempts           │
│  Codec: pack/unpack/isBinary/decodeWSMessage                │
│  raceEndpoints: Mock WebSocket                               │
└─────────────────────────────────────────────────────────────┘
```

### 测试文件结构

```
packages/catcher-ws-ts/src/
├── ws/
│   ├── __tests__/
│   │   ├── reconnect.test.ts     # createReconnectStrategy 单元测试
│   │   ├── multi-endpoint.test.ts # raceEndpoints 单元测试
│   │   └── client.test.ts        # createResilientWS 集成测试
├── __tests__/
│   └── codec.test.ts             # pack/unpack/decodeWSMessage 单元测试
```

> **用例编号规则**：Reconnect `RC1-RCxx`，Multi-endpoint `ME1-MExx`，Client `W1-Wxx`，Codec `C1-Cxx`。

---

## 一、createReconnectStrategy 单元测试

### 1.1 退避计算

| # | 测试名 | 配置 | 断言 |
|---|--------|------|------|
| RC1 | 首次延迟 = initialDelay | `{ initialDelay: 500 }` | 第 1 次 `nextDelay()` ≈ 500ms |
| RC2 | 指数增长 | `{ initialDelay: 100, backoffMultiplier: 2 }` | 100 → 200 → 400 → ... |
| RC3 | maxDelay 上限 | `{ maxDelay: 5000 }` | 延迟不超过 5000ms |
| RC4 | jitter ±25% | 默认配置 | 每次延迟在 ±25% 范围内波动 |

### 1.2 边界条件

| # | 测试名 | 操作 | 断言 |
|---|--------|------|------|
| RC5 | maxAttempts 达到后返回 -1 | `{ maxAttempts: 3 }`，调用 4 次 | 第 4 次 `nextDelay() === -1` |
| RC6 | reset() 重置计数 | 失败 2 次 → reset() → nextDelay() | 重新从 initialDelay 开始 |
| RC7 | attemptCount 正确递增 | 连续 nextDelay() | 1, 2, 3, ... |
| RC8 | 默认配置合理 | `createReconnectStrategy()` | 不抛错，延迟合理 |
| RC9 | maxAttempts=0 立即停止 | `{ maxAttempts: 0 }` | 第 1 次 `nextDelay() === -1` |

---

## 二、raceEndpoints 单元测试

| # | 测试名 | Mock 方式 | 断言 |
|---|--------|---------|------|
| ME1 | 第一个 endpoint 成功 | 两个 WS Server，第一个立即连接 | 返回第一个 socket |
| ME2 | 第一个失败，第二个成功 | 第一个拒绝，第二个正常 | 返回第二个 socket |
| ME3 | 全部失败 | 两个都拒绝 | reject `All WebSocket endpoints failed` |
| ME4 | 全局超时 | 所有 endpoint 挂起 | reject `WebSocket race timeout` |
| ME5 | 失败 socket 被关闭 | 第一个成功 | 其余 socket 被 close() |

---

## 三、createResilientWS 集成测试

### 3.1 连接生命周期

| # | 测试名 | Mock 方式 | 断言 |
|---|--------|---------|------|
| W1 | 成功连接 + open 事件 | WS Server 正常 | `status === 'CONNECTED'`，open 事件触发 |
| W2 | 发送文本消息 | `ws.send('hello')` | 服务端收到消息 |
| W3 | 接收消息 | 服务端发送消息 | message 事件触发，data 正确 |
| W4 | 关闭连接 | `ws.close()` | close 事件触发，`status === 'CLOSED'` |
| W5 | 关闭后不重连 | `ws.close()` + 服务端观察 | 无第二次连接 |

### 3.2 重连

| # | 测试名 | Mock 方式 | 断言 |
|---|--------|---------|------|
| W6 | 服务端关闭后自动重连 | 连接后服务端 close，第二次接受 | 2 次 open 事件 |
| W7 | 握手超时触发重连 | 服务端延迟 open | 4000 关闭码 + 重连 |
| W8 | maxAttempts 耗尽停止 | `{ maxAttempts: 2 }`，持续拒绝 | 重连 2 次后停止 |
| W9 | 重连后 reset() 退避 | 第一次关闭后重连成功 | attemptCount 重置 |

### 3.3 多端点竞速

| # | 测试名 | 配置 | 断言 |
|---|--------|------|------|
| W10 | 多端点连接 | `url: [url1, url2]` | 连接到其中一个 |
| W11 | raceCount 限制 | `url: [1,2,3], raceCount: 2` | 只尝试 2 个 |

### 3.4 配置验证

| # | 测试名 | 配置 | 断言 |
|---|--------|------|------|
| W12 | perMessageDeflate 压缩 | `{ perMessageDeflate: true }` | 连接正常，消息正确 |
| W13 | 自定义 headers | `{ headers: { Authorization: 'xxx' } }` | 服务端收到 header |
| W14 | handshakeTimeout 生效 | 服务端延迟，`handshakeTimeout: 100` | 超时后重连 |
| W15 | addEventListener/removeEventListener | 注册+移除+触发 | 事件分发正确 |
| W16 | readyState 同步 | 连接/关闭时 | readyState 正确变化 |
| W17 | url 属性 | 多端点 | 返回实际连接的端点 |

---

## 四、Codec 单元测试

### 4.1 pack / unpack

| # | 测试名 | 输入 | 断言 |
|---|--------|------|------|
| C1 | pack 对象 | `{ type: 'msg', text: 'hello' }` | 返回 Buffer |
| C2 | unpack 恢复 | pack 结果 → unpack | 深度相等 |
| C3 | pack 数组 | `[1, 2, 3]` | round-trip 正确 |
| C4 | pack 嵌套对象 | `{ a: { b: { c: 1 } } }` | round-trip 正确 |
| C5 | pack 空对象 | `{}` | round-trip 正确 |
| C6 | unpack 接受 Uint8Array | `new Uint8Array(pack(...))` | 正确解码 |

### 4.2 isBinary

| # | 测试名 | 输入 | 断言 |
|---|--------|------|------|
| C7 | Buffer → true | `Buffer.from('hi')` | `true` |
| C8 | ArrayBuffer → true | `new ArrayBuffer(8)` | `true` |
| C9 | Uint8Array → true | `new Uint8Array(8)` | `true` |
| C10 | string → false | `"hello"` | `false` |

### 4.3 decodeWSMessage

| # | 测试名 | 输入 | 断言 |
|---|--------|------|------|
| C11 | 二进制 msgpack 解码 | pack 结果 | 自动 unpack |
| C12 | JSON 字符串解码 | `'{"type":"msg"}'` | `JSON.parse` 结果 |
| C13 | 非法字符串原样返回 | `'not json'` | 返回原字符串 |
| C14 | Buffer 输入 | `Buffer` | msgpack 解码 |

---

## 五、测试覆盖矩阵

| 设计要点 | Reconnect | Multi-endpoint | Client | Codec | TS 测试 |
|---------|:---------:|:--------------:|:------:|:-----:|:------:|
| 退避计算 + jitter | ✅ | | | | RC1-RC4 |
| maxAttempts 限制 | ✅ | | ✅ | | RC5, W8 |
| reset() 重置 | ✅ | | | | RC6-RC7 |
| 竞速成功/失败 | | ✅ | ✅ | | ME1-ME5, W10-W11 |
| 连接生命周期 | | | ✅ | | W1-W5 |
| 断线自动重连 | | | ✅ | | W6-W9 |
| 握手超时 | | | ✅ | | W7, W14 |
| 压缩 | | | ✅ | | W12 |
| 自定义 headers | | | ✅ | | W13 |
| 事件系统 | | | ✅ | | W15-W17 |
| msgpack round-trip | | | | ✅ | C1-C6 |
| 二进制检测 | | | | ✅ | C7-C10 |
| 自动解码 | | | | ✅ | C11-C14 |

### 不测试的范围

| 不测试 | 原因 |
|--------|------|
| 真实外部 WS 服务 | 需要网络，不稳定 |
| perMessageDeflate 压缩率 | 依赖 zlib 行为 |
| 并发连接压力测试 | 非 Catcher WS 职责 |
| 浏览器 WebSocket 兼容性 | 需要 Playwright E2E |
