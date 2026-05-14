# 11 — HTTP 客户端测试设计

> 测试设计文档 · 覆盖 catcher-http-ts 和 catcher-http（Rust）

## 测试范围

### TS 公共 API（catcher-http-ts）

| API | 源文件 | 当前测试 |
|-----|--------|---------|
| `createHttpClient(config)` | `http/client.ts` | ❌ 仅有集成测试（benchmark 性质） |
| `createRetryWrapper(instance, options)` | `http/retry.ts` | ❌ 无 |
| `createInterceptorManager<T>()` | `http/interceptors.ts` | ❌ 无 |
| `createSharedAgent(options)` | `agent/shared-agent.ts` | ❌ 无 |
| `clearDnsCache()` | `agent/shared-agent.ts` | ❌ 无 |
| `createPriorityQueue(options)` | `queue/priority-queue.ts` | ❌ 无 |
| `enqueueWithPriority(queue, priority, fn)` | `queue/priority-queue.ts` | ❌ 无 |

### Rust 公共 API（catcher-http）

| API | 源文件 | 当前测试 |
|-----|--------|---------|
| `retry_with_backoff()` | `resilience/retry.rs` | ✅ 5 个 |
| `CircuitBreaker::new/before_request/on_success/on_failure/reset` | `resilience/circuit_breaker.rs` | ✅ 7 个 |
| `AdaptiveTimeout::new/record/timeout_ms/compute/snapshot` | `resilience/timeout.rs` | ✅ 5 个 |
| `build_retry_policy()` | `resilience/backoff.rs` | ❌ 无 |
| `HttpTransport::new/execute/get/post` | `transport/http_client.rs` | ❌ 无 |
| `PriorityRequestQueue::new/submit` | `scheduler/priority_queue.rs` | ✅ 3 个 |
| `concurrency_for_quality()` | `scheduler/concurrency.rs` | ✅ 3 个 |
| `MetricsCollector::*` | `observability/metrics.rs` | ✅ 3 个 |
| `NetworkQualityEvaluator::new/measure_http_rtt/evaluate` | `observability/network_quality.rs` | ✅ 4 个 |

### 已有集成测试（packages/test/integration/http.test.ts）

| describe 块 | it 块 | 性质 |
|------------|-------|------|
| HTTP — keepAlive connection reuse | 3 个（good/weak/veryWeak） | 性能对比 |
| HTTP — auto-retry on failure | 1 个（弱网重试） | 功能验证 |
| HTTP — priority queue concurrency | 1 个（concurrency=10 × 50 请求） | 性能对比 |

> **问题**：这些集成测试是 benchmark 性质（vanilla vs catcher 对比），不是确定性功能测试。需要补充确定性单元测试。

---

## 测试分层

```
┌─────────────────────────────────────────────────────────────┐
│               集成测试（真实 HTTP Server）                     │
│  TS: vitest + node:http 创建测试服务器                        │
│  Rust: wiremock + tokio::test                                │
│  验证: createHttpClient / HttpTransport 完整请求流程            │
└─────────────────────────────────────────────────────────────┘
                            ▲
┌─────────────────────────────────────────────────────────────┐
│                  单元测试（纯函数 / Mock）                      │
│  Retry: 重试策略 + 退避 + 错误分类                              │
│  Interceptor: 洋葱模型 LIFO/FIFO                              │
│  Agent: 连接池配置                                            │
│  Queue: 优先级排序                                            │
│  TS: vitest, Rust: #[test]                                   │
└─────────────────────────────────────────────────────────────┘
```

### 测试工具

| 平台 | 框架 | Mock 方式 |
|------|------|-----------|
| TS | vitest | `vi.spyOn(axios, 'create')` 或 mock `globalThis.fetch` |
| Rust | `#[tokio::test]` + wiremock | wiremock MockServer |

### 测试文件结构

```
packages/catcher-http-ts/src/
├── http/
│   ├── __tests__/
│   │   ├── client.test.ts       # createHttpClient 集成测试
│   │   ├── retry.test.ts        # createRetryWrapper 单元测试
│   │   └── interceptors.test.ts # createInterceptorManager 单元测试
├── agent/
│   ├── __tests__/
│   │   └── shared-agent.test.ts # createSharedAgent 单元测试
├── queue/
│   ├── __tests__/
│   │   └── priority-queue.test.ts # 优先级队列单元测试

packages/catcher-http/src/          # Rust 测试内联在 #[cfg(test)] mod tests
├── transport/
│   └── __tests/                     # → 或使用 tests/ 集成测试目录
│       └── http_transport.rs        # HttpTransport 集成测试（新增）
├── resilience/
│   └── backoff.rs                   # 补充 build_retry_policy 测试
```

> **用例编号规则**：HTTP Client `H1-Hxx`，Retry `R1-Rxx`，Interceptor `I1-Ixx`，Agent `A1-Axx`，Queue `Q1-Qxx`。Rust 对应用例加 `RH/RR/RI/RA/RQ` 前缀。

---

## 一、createRetryWrapper 单元测试

### 1.1 重试触发条件

| # | 测试名 | Mock 方式 | 断言 |
|---|--------|---------|------|
| R1 | 网络错误自动重试 | axios 抛出 `ECONNRESET`，第 3 次成功 | 3 次调用，最终成功 |
| R2 | 5xx 自动重试 | 返回 503，第 2 次返回 200 | 重试 1 次，返回成功 |
| R3 | 4xx 不重试 | 返回 403 | 只调用 1 次，抛出原错误 |
| R4 | ETIMEDOUT 重试 | axios 抛出 `ETIMEDOUT` | 重试 |
| R5 | ENOTFOUND 重试 | axios 抛出 `ENOTFOUND` | 重试 |
| R6 | ECONNREFUSED 重试 | axios 抛出 `ECONNREFUSED` | 重试 |

### 1.2 退避策略

| # | 测试名 | 配置 | 断言 |
|---|--------|------|------|
| R7 | 指数退避间隔 | `{ backoff: 'exponential', minTimeout: 100 }` | 间隔约 100→200→400ms |
| R8 | 固定退避间隔 | `{ backoff: 'constant', minTimeout: 200 }` | 间隔均约 200ms |
| R9 | maxTimeout 上限 | `{ maxTimeout: 500 }` | 退避不超过 500ms |

### 1.3 回调与边界

| # | 测试名 | 断言 |
|---|--------|------|
| R10 | `onRetry` 回调被调用 | 每次重试调用 `onRetry(attemptNumber)` |
| R11 | 重试次数达到上限抛错 | `attempts: 2` 失败 3 次后抛出最后一个错误 |
| R12 | 首次成功不重试 | 只调用 1 次，`onRetry` 未调用 |
| R13 | 重试时销毁空闲 socket | 检验 `destroyFreeSockets` 在 `attemptNum > 1` 时调用 |

---

## 二、createInterceptorManager 单元测试

### 2.1 注册与执行

| # | 测试名 | 操作 | 断言 |
|---|--------|------|------|
| I1 | use() 注册并执行 | 注册 fulfilled handler | handler 被调用，返回值传递 |
| I2 | 多个 handler 请求链 LIFO | 注册 A → B → C | 执行顺序 C → B → A |
| I3 | 多个 handler 响应链 FIFO | 注册 A → B → C | 执行顺序 A → B → C |
| I4 | eject() 移除 handler | use() → eject(id) | handler 不再执行 |
| I5 | clear() 清空所有 handler | use() × 3 → clear() | 全部移除 |
| I6 | use() 返回递增 ID | 连续 use() | ID 递增 |

### 2.2 错误处理

| # | 测试名 | 操作 | 断言 |
|---|--------|------|------|
| I7 | onRejected 捕获错误 | handler 抛错 + 注册 onRejected | onRejected 被调用，恢复值传递 |
| I8 | 无 onRejected 时错误传播 | handler 抛错，无 onRejected | 错误传播到外层 |
| I9 | runWhen 条件过滤 | `runWhen: (config) => config.dryRun` | 条件不满足时跳过该 handler |

---

## 三、createHttpClient 集成测试

### 3.1 基础请求

| # | 测试名 | Mock 方式 | 断言 |
|---|--------|---------|------|
| H1 | GET 请求成功 | node:http 返回 200 + JSON | `status === 200`，data 正确 |
| H2 | POST + body | POST with JSON body | 服务端收到 body |
| H3 | PUT/DELETE/PATCH | 各方法 | method 正确 |
| H4 | 自定义 headers 透传 | `{ headers: { Authorization: 'Bearer xxx' } }` | 请求包含 header |
| H5 | query params 序列化 | `{ params: { a: 1, b: [2, 3] } }` | URL 含 `?a=1&b=2&b=3` |
| H6 | baseURL 拼接 | `baseURL: 'http://localhost:3000/api'` | URL 正确拼接 |
| H7 | timeout 生效 | 服务端延迟 10s，`timeout: 100` | 抛出超时错误 |

### 3.2 韧性层

| # | 测试名 | Mock 方式 | 断言 |
|---|--------|---------|------|
| H8 | retry 配置生效 | 前 2 次 502，第 3 次 200 | 最终成功 |
| H9 | per-request retry 覆盖 | 全局 retry: 2，请求 retry: 5 | 使用 5 次 |
| H10 | retry: false 禁用 | 全局 retry: 3，请求 retry: false | 不重试 |
| H11 | circuitBreaker 状态 | 连续失败 5 次 → open | `circuitBreakerState() === 'open'` |
| H12 | circuitBreaker 恢复 | open → 等待 reset → halfOpen → 成功 → closed | 状态完整迁移 |
| H13 | 并发队列限制 | `concurrency: 2`，10 个并发请求 | 同时最多 2 个在执行 |

### 3.3 拦截器集成

| # | 测试名 | 操作 | 断言 |
|---|--------|------|------|
| H14 | 请求拦截器修改 config | `interceptors.request.use(config => { config.headers.auth = 'xxx' })` | 请求含 auth header |
| H15 | 响应拦截器转换数据 | `interceptors.response.use(resp => resp.data)` | 返回 data 而非完整 response |
| H16 | 静态拦截器 seed | config 中 `interceptors.request` | 静态拦截器生效 |

### 3.4 辅助方法

| # | 测试名 | 断言 |
|---|--------|------|
| H17 | `circuitBreakerState()` 无 CB 时返回 'closed' | 默认 'closed' |
| H18 | `queueDepth()` 无队列时返回 0 | 默认 0 |
| H19 | `queueDepth()` 有队列时返回正确值 | pending 数量正确 |

---

## 四、createSharedAgent 单元测试

| # | 测试名 | 配置 | 断言 |
|---|--------|------|------|
| A1 | 默认配置创建 Agent | `{}` | 返回 https.Agent，`keepAlive === true` |
| A2 | keepAlive=false | `{ keepAlive: false }` | `keepAlive === false` |
| A3 | maxSockets 配置 | `{ maxSockets: 50 }` | `maxSockets === 50` |
| A4 | DNS 缓存启用 | `{ dnsCacheTtl: 300 }` | agent.lookup 被覆盖 |
| A5 | DNS 缓存禁用 | `{ dnsCacheTtl: 0 }` | agent.lookup 未被覆盖 |
| A6 | rejectUnauthorized=false | `{ rejectUnauthorized: false }` | Agent 创建成功 |
| A7 | clearDnsCache() | 调用后再次创建 | 新 Agent 使用新 DNS cache |

---

## 五、createPriorityQueue 单元测试

| # | 测试名 | 断言 |
|---|--------|------|
| Q1 | 基本入队出队 | `queue.add(fn)` → 结果正确 |
| Q2 | 优先级排序 | 高优先级任务先完成 |
| Q3 | concurrency 限制 | `concurrency: 1`，任务串行执行 |
| Q4 | timeout 超时 | 超时任务抛错 |
| Q5 | enqueueWithPriority 辅助 | 等价于 `queue.add(fn, { priority })` |

---

## 六、Rust 专项测试

### 6.1 补充 Resilience 测试

| # | 测试名 | 断言 |
|---|--------|------|
| RR1 | `build_retry_policy` Fixed 策略 | 返回 ExponentialBackoff，min==max |
| RR2 | `build_retry_policy` Exponential 策略 | retry_bounds 正确 |
| RR3 | `build_retry_policy` DecorrelatedJitter | jitter 启用 |

### 6.2 HttpTransport 集成测试（新增）

| # | 测试名 | MockServer | 断言 |
|---|--------|-----------|------|
| RH1 | GET 请求成功 | 200 + body | `status === 200`，body 正确 |
| RH2 | POST + JSON body | 200 | 请求 body + Content-Type 正确 |
| RH3 | 请求 headers 合并 | 检查默认+自定义 header | 合并正确 |
| RH4 | HTTP 错误 | 500 | `Err(HttpError { status: 500 })` |
| RH5 | 连接超时 | 不可达端口 | `Err(ConnectionTimeout)` |
| RH6 | 熔断器集成 | 连续 500 → open | `circuit_breaker_state() === Open` |
| RH7 | 重试中间件 | 前 2 次 503，第 3 次 200 | 最终成功 |
| RH8 | baseURL 拼接 | 检查实际请求 URL | 拼接正确 |

---

## 七、测试覆盖矩阵

| 设计要点 | Client | Retry | Interceptor | Agent | Queue | TS | Rust |
|---------|:------:|:-----:|:-----------:|:-----:|:-----:|:--:|:----:|
| 基础请求（GET/POST/PUT/DELETE） | ✅ | | | | | H1-H3 | RH1-RH2 |
| 自定义 headers | ✅ | | ✅ | | | H4, I1-I6 | RH3 |
| query params 序列化 | ✅ | | | | | H5 | |
| baseURL 拼接 | ✅ | | | | | H6 | RH8 |
| timeout | ✅ | | | | | H7 | RH5 |
| 网络错误重试 | | ✅ | | | | R1-R6 | ✅已有 |
| 退避策略 | | ✅ | | | | R7-R9 | ✅已有 |
| onRetry 回调 | | ✅ | | | | R10 | ✅已有 |
| 重试上限 | | ✅ | | | | R11 | ✅已有 |
| 重试时清理 socket | | ✅ | | | | R13 | |
| 洋葱模型 LIFO/FIFO | | | ✅ | | | I2-I3 | |
| eject/clear | | | ✅ | | | I4-I5 | |
| 错误恢复 | | | ✅ | | | I7-I8 | |
| runWhen 条件 | | | ✅ | | | I9 | |
| per-request retry | ✅ | | | | | H9-H10 | |
| 熔断器状态机 | ✅ | | | | | H11-H12 | RH6, ✅已有 |
| 并发队列 | ✅ | | | | ✅ | H13 | ✅已有 |
| 连接池 keepAlive | | | | ✅ | | A1-A2 | |
| DNS 缓存 | | | | ✅ | | A4-A7 | |
| 指标收集 | | | | | | | ✅已有 |
| 自适应超时 | | | | | | | ✅已有 |
| 网络质量评估 | | | | | | | ✅已有 |

### 不测试的范围

| 不测试 | 原因 |
|--------|------|
| 真实外部 API 调用 | 需要网络，不稳定 |
| Node.js Agent 内部行为 | 黑盒，由 Node runtime 保证 |
| 并发性能压测 | 非 Catcher HTTP 的职责 |
| TLS 证书验证细节 | 由 Node.js/reqwest 保证 |
