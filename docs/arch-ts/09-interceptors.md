# 09 — 拦截器系统与 Per-request Options

> 设计文档 · 尚未实现
> 基于 [api-gap-analysis.md](../research/api-gap-analysis.md) 中的 P0-2 / P0-3 项

---

## 设计目标

1. **拦截器动态管理** — 运行时增删，支持短路（resolve/reject），不依赖 FFI
2. **Per-request Options** — 单次请求覆盖实例级 retry / timeout / responseType / signal
3. **分层正确** — 拦截器在 TS 侧执行，FFI 只传最终确定的请求体

---

## 为什么拦截器必须在 TS 层

```
❌ 错误方案：拦截器放 Rust / FFI 层
   JS call → FFI → Rust interceptor → FFI callback → JS handler → FFI → Rust → reqwest
   一次请求 = N 次跨界 (N = 拦截器数量 × 3)

✅ 正确方案：拦截器在 TS 层
   JS call → TS interceptors → 最终请求 → FFI (一次) → Rust → reqwest
   一次请求 = 1 次跨界
```

| 维度 | TS 层拦截器 | Rust/FFI 层拦截器 |
|------|-----------|------------------|
| 跨界次数 | 0 | 每个拦截器 2 次 |
| 序列化开销 | 无 | headers/body/url 反复序列化 |
| 异步支持 | 原生 Promise | tokio task 挂起等 JS event loop |
| 错误堆栈 | 完整 JS 调用链 | 堆栈在 Rust/JS 间跳跃 |
| 调试体验 | 断点/console | 两边各打一半 |

axios 本身就是这个模式 — 拦截器是 axios 实例上的纯 JS 逻辑，底层 http.Agent 完全不知情。我们保持一致。

---

## 拦截器系统设计

### 类型定义

```typescript
// packages/catcher-core-ts/src/types.ts 新增

export interface InterceptorFulfilled<T> {
  (value: T): T | Promise<T>
}

export interface InterceptorRejected {
  (error: any): any
}

export interface InterceptorHandler<T> {
  onFulfilled: InterceptorFulfilled<T>
  onRejected?: InterceptorRejected
  /** 条件执行：返回 true 才触发此拦截器 */
  runWhen?: (config: any) => boolean
  /** 同步模式：不等待 Promise */
  synchronous?: boolean
}

export interface InterceptorManager<T> {
  use(onFulfilled: InterceptorFulfilled<T>, onRejected?: InterceptorRejected, options?: {
    runWhen?: (config: any) => boolean
    synchronous?: boolean
  }): number

  eject(id: number): void
  clear(): void

  /** 遍历执行所有请求拦截器（request 链 LIFO） */
  forEachRequest(fn: (handler: InterceptorHandler<T>) => void): void

  /** 遍历执行所有响应拦截器（response 链 FIFO） */
  forEachResponse(fn: (handler: InterceptorHandler<T>) => void): void
}
```

### IHttpClient 扩展

```typescript
export interface IHttpClient {
  // 现有方法不变
  get<T = any>(url: string, config?: RequestConfig): Promise<T>
  post<T = any>(url: string, body?: any, config?: RequestConfig): Promise<T>
  put<T = any>(url: string, body?: any, config?: RequestConfig): Promise<T>
  delete<T = any>(url: string, config?: RequestConfig): Promise<T>
  patch<T = any>(url: string, body?: any, config?: RequestConfig): Promise<T>

  // 新增：拦截器管理器
  interceptors: {
    request: InterceptorManager<RequestConfig>
    response: InterceptorManager<HttpResponse>
  }

  // 新增：状态查询
  circuitBreakerState(): 'closed' | 'open' | 'half-open'
  queueDepth(): number
}
```

### 执行顺序

```
request 拦截器：LIFO（后注册的先执行，洋葱模型外层）
  interceptor N (最后注册)
    interceptor N-1
      ...
        interceptor 1 (最先注册)
          → 实际请求
        interceptor 1 响应
      ...
    interceptor N-1 响应
  interceptor N 响应

response 拦截器：FIFO（先注册的先处理）
```

与 axios 完全一致，减少迁移摩擦。

---

## Per-request Options 设计

### RequestConfig

```typescript
// packages/catcher-core-ts/src/types.ts 新增

export interface RequestConfig {
  /** 请求级 headers，与实例级合并（请求级覆盖） */
  headers?: Record<string, string>

  /** 请求级超时覆盖 (ms) */
  timeout?: number

  /** AbortController signal */
  signal?: AbortSignal

  /** 覆盖实例级重试策略 */
  retry?: RetryOptions | false   // false = 禁用本次请求的重试

  /** 响应解析方式 */
  responseType?: 'json' | 'text' | 'bytes'

  /** 自定义成功状态码判断 */
  validateStatus?: (status: number) => boolean

  /** 优先级覆盖 (0 = 最高) */
  priority?: number

  /** 透传元数据，不参与网络请求 */
  meta?: Record<string, unknown>

  /** query 参数，自动拼接到 URL */
  params?: Record<string, string | number | boolean | (string | number | boolean)[]>

  /** 自定义 params 序列化器 */
  paramsSerializer?: (params: Record<string, any>) => string

  /** 上传进度回调 */
  onUploadProgress?: (event: ProgressEvent) => void

  /** 下载进度回调 */
  onDownloadProgress?: (event: ProgressEvent) => void
}

export interface ProgressEvent {
  loaded: number
  total?: number
}
```

### 配置合并策略

```
最终配置 = 实例级 HttpClientConfig
           + 请求级 RequestConfig（后者字段覆盖前者）

合并规则：
- headers: 浅合并 (instance + request，request 同名 key 覆盖)
- retry:   全量替换 (request.retry 完全替代 instance.retry)
- timeout: 请求级优先
- signal:  仅请求级有
- 其余字段按值处理
```

### 在 client.ts 中的使用

```typescript
// client.ts 伪代码
function createHttpClient(config: HttpClientConfig): IHttpClient {
  // ... 现有构建逻辑 ...

  const interceptors = {
    request: createInterceptorManager<RequestConfig>(),
    response: createInterceptorManager<HttpResponse>(),
  }

  const buildRequest = (method: string, url: string, body?: any, reqConfig?: RequestConfig) => {
    // 1. 合并配置
    const merged = mergeConfig(config, reqConfig)

    // 2. 处理 params → URL
    const fullUrl = buildUrl(url, merged.params, merged.paramsSerializer)

    // 3. 构建请求对象
    let request: RequestConfig & { method: string; url: string; body?: any } = {
      method, url: fullUrl, body, ...merged,
    }

    // 4. 运行请求拦截器链 (LIFO)
    const runRequestInterceptors = (req: typeof request): Promise<typeof request> => {
      const handlers = collectRequestHandlers(interceptors.request)
      return handlers.reduceRight(
        (promise, handler) => promise.then(val => handler.onFulfilled(val)),
        Promise.resolve(req),
      )
    }

    return runRequestInterceptors(request)
  }

  return {
    async get(url, reqConfig) {
      const req = await buildRequest('GET', url, undefined, reqConfig)
      if (req.signal?.aborted) throw new CatcherAbortError(req.url)
      // ... 现有 retry + circuitBreaker + queue 逻辑 ...
    },
    // ...
    interceptors,
    circuitBreakerState: () => breaker?.state ?? 'closed',
    queueDepth: () => queue?.size ?? 0,
  }
}
```

---

## 完整韧性层次（更新后）

```
                          ┌──────────────────────┐
                          │   调用方代码           │
                          └──────────┬───────────┘
                                     │
                          ┌──────────▼───────────┐
                          │  Request Interceptors │  ← TS 层，洋葱模型 LIFO
                          │  (auth/token/header)  │
                          └──────────┬───────────┘
                                     │
                          ┌──────────▼───────────┐
                          │  Per-request merge    │  ← TS 层，配置合并
                          └──────────┬───────────┘
                                     │
          ┌──────────────────────────┼──────────────────────────┐
          │  TS 韧性层               │                           │
          │  ┌───────────────────┐   │                           │
          │  │ Retry Wrapper     │   │  p-retry, 指数退避         │
          │  └─────────┬─────────┘   │                           │
          │            │             │                           │
          │  ┌─────────▼─────────┐   │                           │
          │  │ Circuit Breaker   │   │  cockatiel, 跨请求记忆     │
          │  └─────────┬─────────┘   │                           │
          │            │             │                           │
          │  ┌─────────▼─────────┐   │                           │
          │  │ Priority Queue    │   │  p-queue, 并发控制         │
          │  └─────────┬─────────┘   │                           │
          └────────────┼─────────────┘                           │
                       │                                         │
          ┌────────────▼─────────────┐                           │
          │  axios instance           │  ← 仅在此处跨 FFI (napi)  │
          │  (或 napi-rs 原生调用)     │                           │
          └────────────┬─────────────┘                           │
                       │                                         │
          ┌────────────▼─────────────┐                           │
          │  Shared Agent             │  HTTP连接池 + DNS缓存     │
          │  (或 Rust reqwest pool)   │                           │
          └──────────────────────────┘                           │
```

---

## Response Interceptors 执行流程

```
实际响应
  → response 拦截器 1 (FIFO, 先注册先执行)
  → response 拦截器 2
  → ...
  → response 拦截器 N
  → 返回给调用方

任一级别 throw → 调用方 catch
```

---

## 与现有 interceptors 配置的兼容

现有的 `HttpClientConfig.interceptors` 是静态数组：

```typescript
// 现有方式（v0.1，保持兼容）
const client = createHttpClient({
  baseURL: '...',
  interceptors: {
    request: [(config) => { config.headers.Auth = '...'; return config }],
    response: [(resp) => resp, (err) => Promise.reject(err)],
  },
})
```

新方式（v0.2 目标）：

```typescript
// 创建时注册（等价于旧方式）
const client = createHttpClient({ baseURL: '...' })

// 动态增删
const id = client.interceptors.request.use((config) => {
  config.headers.Auth = getToken()
  return config
})

// 按条件执行
client.interceptors.request.use(
  refreshToken,
  undefined,
  { runWhen: (config) => config.url !== '/auth/login' }
)

// 移除
client.interceptors.request.eject(id)

// 清空全部
client.interceptors.response.clear()

// 单次请求禁用重试
await client.post('/analytics', event, { retry: false })
```

---

## 依赖

| 依赖 | 用途 | 现有/新增 |
|------|------|----------|
| `axios` (peer) | 底层引擎，拦截器在 axios 之上实现 | 现有 |
| 无新增依赖 | 拦截器管理器纯手写（~80 行），不引入第三方 | — |

---

## 与 axios InterceptorManager 的差异

axios 的 `InterceptorManager` 是内部实现，未公开类型。我们独立实现，复用相同语义：

| 行为 | axios | catcher |
|------|-------|---------|
| `use()` 返回 ID | ✅ | ✅ |
| `eject(id)` | ✅ | ✅ |
| `clear()` | ✅ | ✅ |
| Request 链 LIFO | ✅ | ✅ |
| Response 链 FIFO | ✅ | ✅ |
| `runWhen` | ✅ | ✅ |
| `synchronous` | ✅ | ✅ |
| 错误处理回调 | ✅ | ✅ |
| `forEach` | 无公开 API | ✅ 提供 |
