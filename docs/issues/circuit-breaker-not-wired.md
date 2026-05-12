# Issue: Circuit breaker 已配置但未实际接入 HTTP 请求

**发现来源**: 代码审查 + 测试过程中观察到无熔断行为

**严重程度**: 🟡 中

---

## 现状

`HttpClientConfig` 定义了 circuit breaker 配置：

```typescript
// src/types.ts:43-45
circuitBreaker?: {
  failureThreshold: number
  resetTimeout: number
}
```

`createHttpClient` 接受这个配置但**没有把它接入请求流程**：

```typescript
// src/http/client.ts — 没有 circuit breaker 相关代码
const doRequest = retry
  ? createRetryWrapper(instance, retry)
  : (method: string, ...args: any[]) => (instance as any)[method](...args)
```

`cockatiel` 已在 `dependencies` 中（提供熔断器实现），但 `src/http/` 下没有引用它。

## 影响

- keepAlive 坏连接 + retry 反复重试 = **请求放大效应**
- 尤其在连续失败时，没有熔断保护会导致：
  - 大量无效请求阻塞队列
  - CPU 浪费在注定失败的 retry 上
  - 对其他共享 Agent 的请求产生竞争

## 当前请求放大路径

```
请求失败 → retry#1 → retry#1 失败 → retry#2 → retry#2 失败
→ 下一个请求 → 复用坏连接 → 失败 → retry#1 → ...
→ 再下一个请求 → ...
```

如果接入 circuit breaker：
```
请求失败 → retry#1 失败 → 计数+1
请求失败 → retry#1 失败 → 计数+2
请求失败 → 计数达 threshold → 🔴 熔断!
→ 后续请求立即返回错误（不建立连接，不 retry）
→ resetTimeout 后 → 🟢 半开探测
```

## 建议修复

在 `src/http/client.ts` 的 `doRequest` 前包裹 circuit breaker：

```typescript
import { CircuitBreakerPolicy } from 'cockatiel'

const breaker = circuitBreaker 
  ? new CircuitBreakerPolicy({ 
      halfOpenAfter: circuitBreaker.resetTimeout,
      consecutiveFailures: circuitBreaker.failureThreshold 
    })
  : null

const doRequest = breaker
  ? (method, ...args) => breaker.execute(() => rawDoRequest(method, ...args))
  : rawDoRequest
```
