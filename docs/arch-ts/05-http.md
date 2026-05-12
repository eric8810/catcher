# 05 — HTTP 客户端

> 对应源文件：`packages/catcher-ts/src/http/client.ts`（139 行）+ `packages/catcher-ts/src/http/retry.ts`（80 行）

## 职责

创建带多层韧性包装的 HTTP 客户端，封装 axios 实例：
- **连接层**：共享 Agent（连接池 + DNS 缓存）
- **拦截器层**：请求/响应拦截
- **重试层**：指数退避 + jitter + 错误过滤
- **熔断层**：跨请求失败记忆
- **调度层**：优先级并发控制

## 核心导出

### `createHttpClient(config) → IHttpClient`

```typescript
import { createHttpClient } from 'catcher/http'

const client = createHttpClient({
  baseURL: 'https://api.example.com',
  keepAlive: true,
  retry: {
    attempts: 3,
    backoff: 'exponential',
  },
  circuitBreaker: {
    failureThreshold: 5,
    resetTimeout: 30_000,
  },
  concurrency: 10,
  interceptors: {
    request: [(config) => { config.headers.Auth = token; return config }],
    response: [(resp) => resp, (err) => { /* 401 */ }],
  },
})

const data = await client.get('/users/1')
```

### `createRetryWrapper(instance, options) → function`

底层重试包装器，通常不直接使用，由 `createHttpClient` 内部调用。

```typescript
import { createRetryWrapper } from 'catcher/http'
import axios from 'axios'

const instance = axios.create({ baseURL: '...' })
const doRequest = createRetryWrapper(instance, { attempts: 3 })
const res = await doRequest('get', '/users/1')
```

## 韧性层次（由内向外）

```
               ┌─────────────────┐
               │   axios instance │  ← TCP/TLS 收发
               └────────┬────────┘
                        │
               ┌────────▼────────┐
               │  Retry Wrapper  │  ← p-retry 指数退避
               └────────┬────────┘
                        │
               ┌────────▼────────┐
               │ Circuit Breaker │  ← cockatiel 熔断
               └────────┬────────┘
                        │
               ┌────────▼────────┐
               │ Priority Queue  │  ← p-queue 并发控制
               └────────┬────────┘
                        │
               ┌────────▼────────┐
               │   IHttpClient   │  ← 对外暴露 (get/post/put/delete/patch)
               └─────────────────┘
```

构建顺序（`client.ts:createHttpClient`）：

```
1. createSharedAgent()           // 连接池 + DNS 缓存
2. axios.create({ httpsAgent })  // axios 实例
3. interceptors 注册             // 请求/响应勾子
4. createRetryWrapper()          // → rawDoRequest
5. CircuitBreakerPolicy()        // → doRequest
6. createPriorityQueue()         // → enqueue
7. return IHttpClient            // 对外接口
```

## 重试策略 (`retry.ts`)

### 默认重试条件

| 错误类型 | 是否重试 | 理由 |
|---------|---------|------|
| `ECONNRESET` | ✅ | 连接被重置，通常可恢复 |
| `ETIMEDOUT` | ✅ | 弱网下常见，配合熔断器防风暴 |
| `ENOTFOUND` | ✅ | DNS 临时故障 |
| `ECONNREFUSED` | ✅ | 服务暂时不可用 |
| HTTP `5xx` | ✅ | 服务端暂时错误 |
| HTTP `4xx` | ❌ | 客户端错误，重试无意义 |
| 其他 | ❌ | 包装为 `AbortError` 终止重试 |

### 退避参数

底层使用 `p-retry`，参数映射：

| `backoff` | `factor` | `minTimeout` | `maxTimeout` | 实际间隔 (attempt 1, 2, 3 ...) |
|-----------|----------|-------------|-------------|-------------------------------|
| `'fixed'` | 1 | 500ms | — | 500ms, 500ms, 500ms |
| `'exponential'` | 2 | 500ms | 30s | 500ms, 1s, 2s, 4s, 8s, 16s, 30s |

公式：`min(minTimeout × factor^(attempt-1), maxTimeout)`

### 重试时的连接清理 (Issue #1)

每次重试前调用 `destroyFreeSockets()`，遍历 Agent 中所有空闲 socket 并逐个 `socket.destroy()`，强制下一次请求创建新 TCP 连接，避免复用已断开的 keep-alive 连接。

```
retry attempt N (N > 1)
  → destroyFreeSockets()
    → agent.freeSockets[host] → forEach socket.destroy()
  → axios.request()
```

### `onRetry` 回调

每次重试时触发，接收 1-based 重试次数。同时内部会 `console.warn` 输出重试日志：

```
[catcher] Attempt 2/4 failed: ECONNRESET
```

## 熔断器 (`client.ts`)

使用 `cockatiel` 的 `CircuitBreakerPolicy` + `ConsecutiveBreaker`：

```
State machine:
  CLOSED ──连续失败≥threshold──→ OPEN
  OPEN   ──等待 resetTimeout───→ HALF_OPEN
  HALF_OPEN ──试探成功────────→ CLOSED
  HALF_OPEN ──试探失败────────→ OPEN
```

### 配置

```typescript
circuitBreaker: {
  failureThreshold: 5,   // 连续失败 5 次后熔断
  resetTimeout: 30_000,  // 30 秒后进入半开
}
```

### 限制

当前 `cockatiel` (^3.0.0) 的 `ExecuteWrapper` 未公开导出，`client.ts` 使用自建的 stub wrapper（`createExecutor()`）提供兼容接口。该 stub 实现了 `invoke` / `onSuccess` / `onFailure` / `clone` 等必要方法，但不提供事件通知能力。升级 `cockatiel` 版本后可能可去除。

## 优先级队列 (`client.ts`)

`concurrency` 参数 > 0 时启用 `p-queue`，优先级分配：

| 方法 | 优先级 | 说明 |
|------|--------|------|
| `POST` | 1 | 写操作优先 |
| `PUT` / `PATCH` | 2 | 更新操作 |
| `GET` / `DELETE` | 3 | 读操作 |

`concurrency` 为 `undefined` 或 `0` 时不创建队列，所有请求直接执行（无并发限制）。

## `HttpClientConfig` 完整参数

| 参数 | 类型 | 必填 | 默认 | 说明 |
|------|------|------|------|------|
| `baseURL` | `string` | ✅ | — | 请求基础 URL |
| `keepAlive` | `boolean` | ❌ | `true` | 透传至 SharedAgent |
| `dnsCacheTtl` | `number` | ❌ | `300` | 透传至 SharedAgent |
| `rejectUnauthorized` | `boolean` | ❌ | `false` | 透传至 SharedAgent |
| `timeout` | `number \| { connect?, response? }` | ❌ | `{ response: 30_000 }` | 超时配置 |
| `retry` | `{ attempts, backoff?, minTimeout?, maxTimeout?, onRetry? }` | ❌ | — | 重试配置 |
| `circuitBreaker` | `{ failureThreshold, resetTimeout }` | ❌ | — | 熔断配置 |
| `concurrency` | `number` | ❌ | — | 并发上限 |
| `interceptors` | `{ request?, response? }` | ❌ | — | axios 拦截器 |

## 依赖

| 依赖 | 用途 |
|------|------|
| `axios` (peer, ^1.0.0) | HTTP 底层引擎 |
| `p-retry` (^6.0.0) | 指数退避重试 |
| `cockatiel` (^3.0.0) | 熔断器 |
| `p-queue` (^8.0.0) | 优先级并发队列 |
| `cacheable-lookup` (^7.0.0) | DNS 缓存（透传至 SharedAgent） |
