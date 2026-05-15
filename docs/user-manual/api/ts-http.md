# @eric8810/catcher-http API Reference

> Node.js HTTP 客户端 — createHttpClient, SSE, queue, agent, interceptors

```bash
npm install @eric8810/catcher-http
```

---

## 导出清单

```typescript
import {
  // HTTP 客户端
  createHttpClient,

  // SSE
  createSSEStream,
  createSSEClient,

  // 韧性组件
  createRetryWrapper,
  createInterceptorManager,

  // 连接池
  createSharedAgent,
  clearDnsCache,

  // 队列
  createPriorityQueue,
  enqueueWithPriority,

  // 错误
  createCatcherError,
  classifyAxiosError,
} from '@eric8810/catcher-http'

// 从 @eric8810/catcher-core 导入类型
import { isCatcherError } from '@eric8810/catcher-http'
import type {
  IHttpClient,
  HttpClientConfig,
  RequestConfig,
  HttpResponse,
  ProgressEvent,
  InterceptorManager,
  InterceptorFulfilled,
  InterceptorRejected,
  InterceptorHandler,
  SSEStream,
  SSEClient,
  SSEStreamOptions,
  SSEClientOptions,
  SSETimeoutError,
  CatcherHttpError,
  CatcherErrorType,
  ClientEvent,
  ProxyConfig,
  DnsConfig,
  TlsConfig,
  RedirectInfo,
  TransportAdapter,
} from '@eric8810/catcher-http'  // re-exports from catcher-core
```

---

## createHttpClient

```typescript
function createHttpClient(config: HttpClientConfig): IHttpClient
```

创建带完整韧性管道的 HTTP 客户端。

**请求管道**（内→外）：`axios → retry → circuit breaker → concurrency queue`

### HttpClientConfig

| 参数 | 类型 | 默认 | 说明 |
|------|------|------|------|
| `baseURL` | `string` | **必填** | 所有请求的基础 URL |
| `keepAlive` | `boolean` | `true` | TCP keep-alive 连接池 |
| `dnsCacheTtl` | `number` | `300` | DNS 缓存 TTL（秒） |
| `rejectUnauthorized` | `boolean` | `true` | 拒绝未授权 TLS 证书 |
| `timeout` | `number \| { connect, response }` | `30000` | 超时（ms） |
| `retry` | `RetryConfig` | — | 重试配置 |
| `retry.attempts` | `number` | — | 最多重试次数（含首次） |
| `retry.backoff` | `'fixed' \| 'exponential'` | `'exponential'` | 退避策略 |
| `retry.minTimeout` | `number` | `500` | 最小重试间隔（ms） |
| `retry.maxTimeout` | `number` | `30000` | 最大重试间隔（ms） |
| `retry.retryIf` | `(error) => boolean` | — | 自定义可重试条件 |
| `retry.onRetry` | `(attempt) => void` | — | 每次重试回调 |
| `concurrency` | `number` | — | 并发限制 |
| `circuitBreaker` | `CBConfig` | — | 熔断器配置 |
| `circuitBreaker.failureThreshold` | `number` | — | 连续失败 N 次后 OPEN |
| `circuitBreaker.resetTimeout` | `number` | — | OPEN→HALF_OPEN 等待时间（ms） |
| `interceptors` | `{ request, response }` | — | 静态拦截器（向后兼容） |
| `credentials` | `'include' \| 'same-origin' \| 'omit'` | — | 浏览器 credentials 策略 |
| `fetchMode` | `'cors' \| 'no-cors' \| 'same-origin' \| 'navigate'` | — | 浏览器 fetch mode |
| `proxy` | `boolean \| string \| ProxyConfig` | — | HTTP/SOCKS5 代理 |
| `redirect` | `{ follow, maxRedirects, beforeRedirect }` | — | 重定向控制 |
| `dns` | `DnsConfig` | — | 自定义 DNS |
| `tls` | `TlsConfig` | — | TLS 客户端证书、SNI、密钥固定 |
| `auth` | `{ username, password }` | — | Basic 认证 |
| `bearerToken` | `string \| () => string \| Promise<string>` | — | Bearer token（支持动态刷新） |

### IHttpClient

```typescript
interface IHttpClient {
  get<T>(url: string, config?: RequestConfig): Promise<T>
  post<T>(url: string, body?: any, config?: RequestConfig): Promise<T>
  put<T>(url: string, body?: any, config?: RequestConfig): Promise<T>
  delete<T>(url: string, config?: RequestConfig): Promise<T>
  patch<T>(url: string, body?: any, config?: RequestConfig): Promise<T>

  interceptors: {
    request: InterceptorManager<RequestConfig>
    response: InterceptorManager<HttpResponse>
  }

  circuitBreakerState(): 'closed' | 'open' | 'half-open'
  queueDepth(): number
  on?(event: ClientEvent['type'], listener: (event: ClientEvent) => void): () => void
  off?(event: ClientEvent['type'], listener?: (event: ClientEvent) => void): void
  updateConfig?(updates: Partial<Pick<HttpClientConfig, 'retry' | 'timeout'>>): void
}
```

### RequestConfig（Per-request 覆盖）

| 参数 | 类型 | 说明 |
|------|------|------|
| `headers` | `Record<string, string>` | 请求头 |
| `timeout` | `number` | 覆盖超时（ms） |
| `signal` | `AbortSignal` | 取消信号 |
| `retry` | `RetryConfig \| false` | 覆盖重试；`false` 禁用 |
| `responseType` | `'json' \| 'text' \| 'bytes' \| 'stream'` | 响应体解析模式 |
| `validateStatus` | `(status) => boolean` | 自定义成功状态码 |
| `priority` | `number` | 优先级（0 最高） |
| `meta` | `Record<string, unknown>` | 透传给拦截器的元数据 |
| `params` | `Record<string, string\|number>` | 查询参数 |
| `paramsSerializer` | `(params) => string` | 自定义序列化器 |
| `onUploadProgress` | `(event: ProgressEvent) => void` | 上传进度回调 |
| `onDownloadProgress` | `(event: ProgressEvent) => void` | 下载进度回调 |

### 方法默认优先级

| 方法 | 默认优先级 |
|------|:---------:|
| `post()` | `1`（最高） |
| `put()` / `patch()` | `2` |
| `get()` / `delete()` | `3` |

---

## InterceptorManager

```typescript
interface InterceptorManager<T> {
  use(onFulfilled, onRejected?, options?): number
  eject(id: number): void
  clear(): void
}
```

Request 链执行顺序为 **LIFO**（后注册先执行），Response 链执行顺序为 **FIFO**（先注册先执行）。

```typescript
client.interceptors.request.use(config => {
  config.headers['Authorization'] = `Bearer ${getToken()}`
  return config
})

client.interceptors.response.use(
  response => response,
  error => {
    if (error.response?.status === 401) refreshToken()
    throw error
  }
)
```

---

## createSSEStream

```typescript
function createSSEStream(options: SSEStreamOptions): SSEStream
```

一次性 SSE 流式请求（AI 对话等场景）。

| 参数 | 类型 | 默认 | 说明 |
|------|------|------|------|
| `url` | `string` | **必填** | SSE 端点 |
| `method` | `'GET' \| 'POST'` | `'GET'` | HTTP 方法 |
| `headers` | `Record<string, string>` | `{}` | 请求头 |
| `body` | `string \| Record<string, unknown>` | — | POST body |
| `timeout` | `number` | `30000` | 超时（ms） |
| `signal` | `AbortSignal` | — | 取消信号 |

SSEStream 只能迭代一次，重复迭代抛出错误。

```typescript
const stream = createSSEStream({
  url: 'https://api.openai.com/v1/chat/completions',
  method: 'POST',
  headers: { Authorization: `Bearer ${key}` },
  body: { model: 'gpt-4', messages, stream: true },
})

for await (const line of stream) {
  // line 是原始 SSE 行（含 data: 前缀）
}
```

## createSSEClient

```typescript
function createSSEClient(options: SSEClientOptions): SSEClient
```

长连接 SSE，自动重连 + Last-Event-ID 断点续传。

继承 SSEStreamOptions，额外参数：

| 参数 | 类型 | 默认 | 说明 |
|------|------|------|------|
| `reconnect.enabled` | `boolean` | `true` | 启用自动重连 |
| `reconnect.maxRetries` | `number` | `Infinity` | 最大重连次数 |
| `reconnect.initialDelay` | `number` | `1000` | 初始延迟（ms） |
| `reconnect.maxDelay` | `number` | `30000` | 最大延迟（ms） |
| `reconnect.backoffMultiplier` | `number` | `2` | 指数退避因子 |
| `circuitBreaker` | `{ failureThreshold, resetTimeout }` | — | 熔断器 |

SSEClient 属性：

| 属性 | 类型 | 说明 |
|------|------|------|
| `readyState` | `'CONNECTING' \| 'OPEN' \| 'CLOSED'` | 连接状态 |
| `lastEventId` | `string` | 最后收到的事件 ID |
| `close()` | `() => void` | 主动断开 |

---

## createSharedAgent

```typescript
function createSharedAgent(options: SharedAgentOptions): http.Agent
```

创建共享 HTTP Agent（连接池 + DNS 缓存）。

| 参数 | 类型 | 默认 | 说明 |
|------|------|------|------|
| `keepAlive` | `boolean` | `true` | TCP keep-alive |
| `keepAliveMsecs` | `number` | `30000` | keep-alive 空闲时间（ms） |
| `maxSockets` | `number` | `25` | 每 host 最大 socket |
| `maxFreeSockets` | `number` | `10` | 每 host 最大空闲 socket |
| `timeout` | `number` | `60000` | socket 超时（ms） |

## clearDnsCache

```typescript
function clearDnsCache(): void
```

清除共享 Agent 的 DNS 缓存。

---

## createPriorityQueue

```typescript
function createPriorityQueue(options?: PriorityQueueOptions): PQueue
```

创建优先级队列。

| 参数 | 类型 | 默认 | 说明 |
|------|------|------|------|
| `concurrency` | `number` | `10` | 并发任务数 |
| `timeout` | `number` | — | 队列超时（ms） |

## enqueueWithPriority

```typescript
function enqueueWithPriority(queue, priority: number, fn: () => Promise<any>): Promise<any>
```

按优先级入队（0 = 最高）。

---

## Client Events

```typescript
type ClientEvent =
  | { type: 'retry'; attempt: number; error: Error; url: string }
  | { type: 'requestComplete'; method: string; url: string; status: number; durationMs: number }
```

```typescript
const unsub = client.on('retry', ({ attempt, url }) => {
  console.log(`Retry #${attempt} for ${url}`)
})

client.on('requestComplete', ({ method, url, status, durationMs }) => {
  console.log(`${method} ${url} → ${status} (${durationMs}ms)`)
})
```

---

## CatcherHttpError

```typescript
interface CatcherHttpError extends Error {
  readonly type: CatcherErrorType
  readonly request: { method, url, headers, config }
  readonly response?: { status, headers, data }
  readonly attempt: number
  readonly elapsedMs: number
  toJSON(): Record<string, unknown>
}
```

使用 `isCatcherError()` 类型守卫。`toJSON()` 自动脱敏 Authorization/Cookie 等敏感头。
