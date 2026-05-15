# 韧性策略指南

> catcher 的网络韧性由四层策略组成：重试（Retry）、熔断器（Circuit Breaker）、超时（Timeout）、优先级队列（Priority Queue），并支持运行时自适应调整。

---

## 目录

- [请求管道总览](#请求管道总览)
- [一、重试（Retry）](#一重试retry)
- [二、熔断器（Circuit Breaker）](#二熔断器circuit-breaker)
- [三、超时（Timeout）](#三超时timeout)
- [四、优先级队列（Priority Queue）](#四优先级队列priority-queue)
- [五、自适应行为](#五自适应行为)
- [六、参数调优推荐](#六参数调优推荐)

---

## 请求管道总览

每个 HTTP 请求依次穿过四层韧性策略，由内向外执行：

```
请求发起 → [Retry] → [Circuit Breaker] → [Priority Queue] → 网络
```

| 层级 | 作用 | 关键问题 |
|------|------|---------|
| **Retry** | 瞬时故障自动重试 | "这次失败了，再试一次能成功吗？" |
| **Circuit Breaker** | 连续故障快速熔断 | "服务是不是挂了？别再打了。" |
| **Timeout** | 防止请求无限等待 | "多久没响应就该放弃？" |
| **Priority Queue** | 并发控制 + 优先级调度 | "哪些请求更紧急？" |

**交互关系**：

- Retry 在 Circuit Breaker 内部。如果重试全部耗尽仍失败，才算一次"熔断器失败"。
- Circuit Breaker 打开后，新请求直接被拒绝（fast fail），不再进入 Retry 和网络层。
- Priority Queue 是最外层，请求排队等待并发槽位，拿到槽位后才进入 Retry → Circuit Breaker → 网络。

---

## 一、重试（Retry）

### 1.1 工作原理

重试层负责处理瞬时故障（transient failures）——网络抖动、DNS 解析超时、服务端 5xx 等。不是所有错误都应该重试：4xx 客户端错误代表业务逻辑问题，重试不会改变结果。

**可重试条件**：

| 条件 | 说明 |
|------|------|
| `ECONNRESET` | 连接被对端重置（常见于 keepAlive 连接被服务端关闭） |
| `ETIMEDOUT` | 连接或响应超时 |
| `ENOTFOUND` | DNS 解析失败（可能是临时 DNS 故障） |
| `ECONNREFUSED` | 目标端口无进程监听 |
| HTTP `5xx` | 服务端错误 |

**不重试的条件**：所有 `4xx` 错误、用户主动取消（`AbortSignal`）、`AbortError`。

### 1.2 退避策略

 catcher 支持三种退避策略：

| 策略 | 行为 | 适用场景 |
|------|------|---------|
| **Fixed** | 每次重试间隔固定 `minTimeout` | 服务端有已知恢复周期的场景 |
| **Exponential** | 间隔翻倍：`minTimeout × 2^(attempt-1)` | 通用默认策略，逐步给服务端恢复时间 |
| **DecorrelatedJitter** | 指数退避 + 随机抖动 | 高并发下避免"惊群效应"（thundering herd） |

Exponential 退避的间隔示例（minTimeout=500ms, maxTimeout=30000ms, factor=2）：

```
第 1 次重试: 500ms
第 2 次重试: 1000ms
第 3 次重试: 2000ms
第 4 次重试: 4000ms
...
第 N 次重试: min(500 × 2^(N-1), 30000ms)
```

### 1.3 连接池清理

重试时，catcher 会**主动销毁连接池中的 stale keepAlive sockets**。这是因为：

1. 第一次请求失败时，底层 socket 可能已处于半死状态
2. 如果不销毁，p-retry 的下一次请求可能复用同一个坏 socket
3. 销毁后，axios/reqwest 会自动创建全新 TCP 连接

### 1.4 使用方式

#### TypeScript

```typescript
import { createHttpClient } from '@eric8810/catcher-http'

const client = createHttpClient({
  baseURL: 'https://api.example.com',
  retry: {
    attempts: 3,
    backoff: 'exponential',
    minTimeout: 500,
    maxTimeout: 30_000,
    onRetry: (attempt) => {
      console.log(`第 ${attempt} 次重试...`)
    },
  },
})
```

**Per-request 覆盖**：

```typescript
// 禁用重试（例如轮询接口，失败时希望立即知道）
await client.get('/health', { retry: false })

// 自定义重试参数（例如写操作需要更多次重试）
await client.post('/orders', orderData, {
  retry: { attempts: 5, backoff: 'fixed', minTimeout: 1000 },
})
```

#### Rust

```rust
use catcher_core::types::resilience::{RetryConfig, BackoffKind};
use catcher_core::CatcherError;
use catcher_http::retry_with_backoff;

let config = RetryConfig {
    max_attempts: 3,
    backoff: BackoffKind::Exponential,
    min_backoff_ms: 100,
    max_backoff_ms: 10_000,
    jitter: true,
};

let result = retry_with_backoff(
    &config,
    || async { some_fallible_operation().await },
    |err| err.category() == ErrorCategory::Retryable,
    |attempt, err| eprintln!("重试 #{attempt}: {err}"),
).await;
```

Rust 重试耗尽时返回 `CatcherError::RetryExhausted { attempts, last_error }`。

### 1.5 默认值

| 参数 | TS 默认值 | Rust 默认值 |
|------|----------|------------|
| 最大重试次数 | 3 | 3 |
| 退避策略 | `exponential` | `exponential` |
| 最小延迟 | 500ms | 100ms |
| 最大延迟 | 30,000ms | 10,000ms |
| Jitter | — | `true` |

---

## 二、熔断器（Circuit Breaker）

### 2.1 三态状态机

熔断器是一个经典的三态有限状态机：

```
              连续失败 ≥ threshold                任何失败
  CLOSED ──────────────────────▶ OPEN ──────────────────┐
     ▲                            │                      │
     │         reset_timeout      │                      │
     │            到期后          ▼                      │
     └──────── HALF_OPEN ◀───────┘                      │
              连续成功 ≥ success_threshold               │
                                                       (回到 OPEN)
```

| 状态 | 行为 |
|------|------|
| **CLOSED** | 正常状态。所有请求通过，但持续追踪连续失败次数。 |
| **OPEN** | 熔断状态。所有请求**立即被拒绝**（fast fail），不进入网络层。等待 `resetTimeout` 后自动转入 HALF_OPEN。 |
| **HALF_OPEN** | 试探状态。允许少量请求通过（受 `halfOpenMaxRequests` 限制），用于探测服务是否恢复。 |

### 2.2 状态转换规则

| 当前状态 | 事件 | 目标状态 | 说明 |
|---------|------|---------|------|
| CLOSED | 连续失败次数 ≥ `failureThreshold` | OPEN | 记录 `openedAt` 时间戳 |
| OPEN | 距 `openedAt` ≥ `resetTimeout` | HALF_OPEN | 进入试探模式 |
| HALF_OPEN | 连续成功次数 ≥ `successThreshold` | CLOSED | 服务恢复正常 |
| HALF_OPEN | 任何一次失败 | OPEN | 服务未恢复，重新熔断 |

### 2.3 使用方式

#### TypeScript

```typescript
import { createHttpClient } from '@eric8810/catcher-http'

const client = createHttpClient({
  baseURL: 'https://api.example.com',
  circuitBreaker: {
    failureThreshold: 5,   // 连续失败 5 次触发熔断
    resetTimeout: 30_000,  // 30 秒后进入试探
  },
})

// 查询当前熔断器状态
console.log(client.circuitBreakerState())
// → 'closed' | 'open' | 'half-open'
```

#### Rust

```rust
use catcher_core::types::resilience::CircuitBreakerConfig;

let config = CircuitBreakerConfig {
    failure_threshold: 5,
    success_threshold: 2,
    reset_timeout_ms: 30_000,
    half_open_max_requests: 5,
};
```

### 2.4 默认值

| 参数 | TS 默认值 | Rust 默认值 |
|------|----------|------------|
| 连续失败阈值 | `5` | `5` |
| 连续成功恢复阈值 | — (cockatiel 管理) | `2` |
| 熔断恢复时间 | `30_000ms` | `30_000ms` |
| 半开最大试探数 | — (cockatiel 管理) | `5` |

### 2.5 设计要点

- **"连续"** 而非 "累计"：一次成功就会重置失败计数。偶发的单次失败不会触发熔断。
- **Fast Fail**：OPEN 状态下请求不进入网络层，直接抛出 `CatcherError::CircuitBreakerOpen`，保护下游服务。
- **熔断器在 Retry 外层**：一次请求的所有重试耗尽后，如果仍然失败，才算一次熔断器级别的失败。

---

## 三、超时（Timeout）

### 3.1 超时层级

catcher 支持两级超时：

| 超时 | 含义 | 场景 |
|------|------|------|
| **连接超时** (`connect`) | TCP 连接建立的最长等待时间 | 目标不可达、DNS 解析慢 |
| **响应超时** (`response`) | 连接建立后，等待首个响应字节的最长时间 | 服务端处理慢、请求排队 |

### 3.2 使用方式

#### TypeScript

```typescript
// 简写：统一超时 30 秒
const client = createHttpClient({
  baseURL: 'https://api.example.com',
  timeout: 30_000,
})

// 精细控制
const client = createHttpClient({
  baseURL: 'https://api.example.com',
  timeout: {
    connect: 3_000,   // 3 秒内必须完成 TCP 握手
    response: 30_000, // 30 秒内必须收到响应
  },
})

// Per-request 覆盖
await client.get('/slow-report', { timeout: 60_000 })
```

#### Rust

Rust 除了静态超时外，还支持 **自适应超时（Adaptive Timeout）**。

```rust
// 静态超时
let config = HttpClientConfig {
    connect_timeout_ms: Some(5_000),
    response_timeout_ms: Some(30_000),
    ..Default::default()
};
```

### 3.3 自适应超时（Rust only）

自适应超时基于 **P90 RTT 滑动窗口**动态计算超时：

```
timeout = clamp(P90_RTT × multiplier, min_timeout, max_timeout)
```

**构造参数**：`AdaptiveTimeout::new(min_timeout_ms, max_timeout_ms, multiplier, window_size)`，所有参数均为必填。

**计算示例**（`min=500, max=30000, multiplier=3.0`）：

```
RTT 样本窗口: [120, 85, 200, 150, 90, 310, 175, 130, 95, 180]ms
排序后:       [85, 90, 95, 120, 130, 150, 175, 180, 200, 310]
P90 索引:     ceil(10 × 0.9) - 1 = 8
P90 值:       200ms
超时:         200 × 3.0 = 600ms → clamp(600, 500, 30000) = 600ms
```

当窗口为空时（冷启动），返回 `min_timeout_ms`。

### 3.4 默认值

| 参数 | TS 默认值 | Rust 默认值 |
|------|----------|------------|
| 连接超时 | 不单独设置（由 axios 管理） | 5,000ms |
| 响应超时 | 30,000ms | 30,000ms |
| 自适应超时 | ❌ 不支持 | ✅ 可配置 |

---

## 四、优先级队列（Priority Queue）

### 4.1 工作原理

优先级队列控制并发请求数量，防止瞬间大量请求压垮服务端或本地资源。同时按优先级调度，确保关键请求优先执行。

**默认优先级**：

| HTTP 方法 | 优先级 | 数值 | 含义 |
|-----------|--------|------|------|
| POST | 最高 | 1 | 写操作（创建订单、提交表单），优先处理 |
| PUT / PATCH | 中 | 2 | 更新操作 |
| GET / DELETE | 低 | 3 | 读操作，可容忍延迟 |

> 数值越小优先级越高（与 p-queue 和 Rust `Priority` enum 一致）。

### 4.2 使用方式

#### TypeScript

```typescript
const client = createHttpClient({
  baseURL: 'https://api.example.com',
  concurrency: 10,  // 最多 10 个并发请求
})

// 查看队列深度
console.log(`排队中: ${client.queueDepth()}`)
```

Per-request 自定义优先级：

```typescript
// 手动设置为最高优先级
await client.get('/urgent-data', { priority: 0 })
```

#### Rust

```rust
use catcher_http::PriorityRequestQueue;

let queue = PriorityRequestQueue::new(10);  // 并发数

let result = queue
    .enqueue(0, || async { do_http_request().await })
    .await?;  // priority: 0 = 最高

// 查询队列深度
println!("排队中: {}", queue.pending());
```

### 4.3 网络质量感知并发

Rust 版支持根据网络质量动态调整并发数：

| 网络质量 | 建议并发数 |
|---------|-----------|
| Excellent | 50 |
| Good | 25 |
| Fair | 10 |
| Poor | 5 |
| Bad | 2 |

### 4.4 队列超时

如果请求在队列中排队时间超过 `timeout`，直接返回超时错误（不入网络层）。这避免了"等了半天终于轮到自己，但业务层早已超时"的问题。

---

## 五、自适应行为

### 5.1 运行时配置热更新

无需重启客户端即可调整韧性参数：

```typescript
// 运行中调整重试和超时配置
client.updateConfig({
  retry: { attempts: 5, backoff: 'exponential' },
  timeout: 60_000,
})
```

`updateConfig` 会立即生效：
- `retry` 变更会影响后续所有请求的重试策略
- `timeout` 变更会同时更新 axios 实例的 `defaults.timeout`

### 5.2 事件系统

通过事件监听获取运行时韧性状态：

```typescript
// 监听重试事件
client.on('retry', (event) => {
  console.log(`重试第 ${event.attempt} 次`, event.url)
})

// 监听请求完成
client.on('requestComplete', (event) => {
  console.log(`${event.method} ${event.url} → ${event.status} (${event.durationMs}ms)`)
})

// 退订
const unsub = client.on('retry', handler)
unsub() // 移除监听
```

可用事件：

| 事件 | 字段 | 说明 |
|------|------|------|
| `retry` | `attempt`, `error`, `url` | 每次重试触发 |
| `requestComplete` | `method`, `url`, `status`, `durationMs` | 每次请求完成（含成功和失败） |

---

## 六、参数调优推荐

### 6.1 通用场景

| 参数 | 推荐值 | 说明 |
|------|--------|------|
| `retry.attempts` | 3 | 通用重试次数 |
| `retry.backoff` | `exponential` | 渐进式退避，给服务端恢复时间 |
| `retry.minTimeout` | 500ms | 首次重试不宜太快 |
| `retry.maxTimeout` | 30,000ms | 最大退避上限 |
| `circuitBreaker.failureThreshold` | 5 | 5 次连续失败才熔断 |
| `circuitBreaker.resetTimeout` | 30,000ms | 30 秒试探一次 |
| `concurrency` | 10 | 适中的并发控制 |
| `timeout` | 30,000ms | 通用响应超时 |

---

### 6.2 AI 流式对话（OpenAI / Anthropic / Gemini）

AI 场景的特点：SSE 长连接、Token 级流式响应、网络波动容忍度低、重试代价高（需要重新生成）。

```typescript
const client = createHttpClient({
  baseURL: 'https://api.openai.com',
  timeout: {
    connect: 5_000,
    response: 120_000, // AI 生成可能很慢
  },
  retry: {
    attempts: 2,         // 不宜过多，重新生成代价高
    backoff: 'exponential',
    minTimeout: 1_000,
    maxTimeout: 5_000,
  },
  circuitBreaker: {
    failureThreshold: 3,
    resetTimeout: 10_000, // AI 服务恢复较快
  },
  concurrency: 5,         // API 有 rate limit，不宜并发过高
})
```

| 参数 | 推荐值 | 原因 |
|------|--------|------|
| `timeout.response` | 120,000ms | AI 生成长文可能需要 1-2 分钟 |
| `retry.attempts` | 2 | 重试意味着重新生成 Token，代价高 |
| `circuitBreaker.failureThreshold` | 3 | AI API 不稳定时快速降级 |
| `circuitBreaker.resetTimeout` | 10,000ms | AI 服务通常快速恢复 |
| `concurrency` | 5 | 避免 rate limit 429 错误 |

---

### 6.3 高频交易 / 实时报价

特点：延迟极度敏感、数据新鲜度 > 完整性、失败时宁可丢弃旧数据。

```typescript
const client = createHttpClient({
  baseURL: 'wss://feed.example.com',
  timeout: {
    connect: 1_000,
    response: 3_000,   // 超过 3 秒的数据已经没有意义
  },
  retry: {
    attempts: 1,        // 几乎不重试，直接用下一个数据包
    backoff: 'fixed',
    minTimeout: 100,
    maxTimeout: 100,
  },
  circuitBreaker: {
    failureThreshold: 10,
    resetTimeout: 5_000,
  },
  concurrency: 20,
})
```

| 参数 | 推荐值 | 原因 |
|------|--------|------|
| `timeout.response` | 3,000ms | 旧数据无价值 |
| `retry.attempts` | 1 | 重试获得的数据已过期 |
| `retry.backoff` | `fixed` | 如果重试，必须极快 |
| `circuitBreaker.failureThreshold` | 10 | 数据流场景偶发丢包正常，阈值放宽 |
| `concurrency` | 20 | 多品种并行订阅 |

---

### 6.4 移动端 / 弱网环境

特点：网络频繁切换（WiFi ↔ 4G ↔ 离线）、高延迟、高丢包率。

```typescript
const client = createHttpClient({
  baseURL: 'https://api.example.com',
  timeout: {
    connect: 10_000,    // 弱网 TCP 握手慢
    response: 60_000,   // 大屏手机可能下载慢
  },
  retry: {
    attempts: 5,        // 弱网需要更多重试机会
    backoff: 'exponential',
    minTimeout: 1_000,
    maxTimeout: 30_000,
  },
  circuitBreaker: {
    failureThreshold: 8,   // 弱网误判概率高，阈值放宽
    resetTimeout: 15_000,
  },
  concurrency: 3,      // 弱网不宜并发太高
})
```

| 参数 | 推荐值 | 原因 |
|------|--------|------|
| `timeout.connect` | 10,000ms | 弱网 DNS + TCP 很慢 |
| `timeout.response` | 60,000ms | 大图/文件下载 |
| `retry.attempts` | 5 | 网络闪断常见，多给几次机会 |
| `circuitBreaker.failureThreshold` | 8 | 避免弱网抖动误触发熔断 |
| `concurrency` | 3 | 带宽有限，并发过高反而更慢 |

**Flutter / Dart 同理**：

```dart
final client = CatcherHttpClient(HttpClientConfig(
  baseUrl: 'https://api.example.com',
  connectTimeoutMs: 10000,
  responseTimeoutMs: 60000,
  retry: RetryConfig(maxAttempts: 5, backoff: 'exponential'),
  circuitBreaker: CircuitBreakerConfig(
    failureThreshold: 8,
    resetTimeoutMs: 15000,
  ),
));
```

---

### 6.5 后台批处理 / 数据同步

特点：不敏感延迟、需要保证数据完整性、可容忍较长时间。

```typescript
const client = createHttpClient({
  baseURL: 'https://internal-api.example.com',
  timeout: {
    connect: 5_000,
    response: 300_000,   // 批量任务可能 5 分钟
  },
  retry: {
    attempts: 10,         // 批处理需要尽最大努力
    backoff: 'exponential',
    minTimeout: 2_000,
    maxTimeout: 60_000,
  },
  circuitBreaker: {
    failureThreshold: 20,   // 内部服务，阈值放宽
    resetTimeout: 60_000,
  },
  concurrency: 2,        // 批处理不需要高并发
})
```

| 参数 | 推荐值 | 原因 |
|------|--------|------|
| `timeout.response` | 300,000ms | 批量任务耗时不确定 |
| `retry.attempts` | 10 | 数据完整性优先 |
| `retry.minTimeout` | 2,000ms | 批处理无需快速重试 |
| `circuitBreaker.failureThreshold` | 20 | 内部服务相对稳定 |
| `concurrency` | 2 | 避免对内部服务造成压力 |

---

### 6.6 微服务间调用（高可用）

特点：内网低延迟、高吞吐、服务降级需要快速反应。

```typescript
const client = createHttpClient({
  baseURL: 'http://order-service:8080',
  timeout: {
    connect: 2_000,
    response: 5_000,
  },
  retry: {
    attempts: 3,
    backoff: 'exponential',
    minTimeout: 100,
    maxTimeout: 1_000,
  },
  circuitBreaker: {
    failureThreshold: 3,    // 快速感知故障
    resetTimeout: 5_000,    // 快速试探恢复
  },
  concurrency: 50,
})
```

| 参数 | 推荐值 | 原因 |
|------|--------|------|
| `timeout.response` | 5,000ms | 内网 RTT 通常 < 100ms，5 秒已经很长 |
| `retry.minTimeout` | 100ms | 内网重试可以更快 |
| `circuitBreaker.failureThreshold` | 3 | 快速感知下游故障 |
| `circuitBreaker.resetTimeout` | 5,000ms | 快速试探恢复 |
| `concurrency` | 50 | 内网吞吐高 |

---

### 6.7 参数速查表

| 场景 | 重试次数 | 退避 | 超时 | 熔断阈值 | 并发 |
|------|---------|------|------|---------|------|
| 通用 | 3 | exponential | 30s | 5 / 30s | 10 |
| AI 流式 | 2 | exponential | 120s | 3 / 10s | 5 |
| 高频交易 | 1 | fixed 100ms | 3s | 10 / 5s | 20 |
| 移动弱网 | 5 | exponential | 60s | 8 / 15s | 3 |
| 批处理 | 10 | exponential | 300s | 20 / 60s | 2 |
| 微服务内网 | 3 | exponential | 5s | 3 / 5s | 50 |
