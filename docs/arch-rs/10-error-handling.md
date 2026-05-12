# 10 — 错误处理策略

## 错误映射表

| 底层错误源 | CatcherError 变体 | 可重试？ | 重试时动作 |
|-----------|-------------------|---------|-----------|
| reqwest timeout | RequestTimeout | Y | 销毁空闲连接 → 退避 → 重试 |
| reqwest connect failure | ConnectionTimeout | Y | 退避 → 重试 |
| HTTP 5xx | HttpError { status: 5xx } | Y | 退避 → 重试 |
| HTTP 4xx | HttpError { status: 4xx } | N | 直接返回 |
| tungstenite ConnectionClosed | WsDisconnected | Y | 重连状态机 |
| tungstenite Protocol error | WsDisconnected | Y | 重连状态机 |
| rmp_serde encode error | EncodeError | N | 直接返回 |
| rmp_serde decode error | DecodeError | N | 直接返回 |
| hickory ResolveError | DnsError | Y | 退避 → 重试 |
| circuitbreaker-rs reject | CircuitBreakerOpen | Y | 等待 reset_timeout |

## 错误传播路径

```
底层 I/O Error
  → Transport 层映射为 CatcherError
  → Resilience 层判断 ErrorCategory
    ├── Retryable → 退避 → 重试 → ...
    │     └── 耗尽 → RetryExhausted → 上报到上层
    └── NonRetryable → 直接传播到上层

CircuitBreaker::call()
  ├── Closed → 执行 → 失败记录 → 达到阈值 → Open
  ├── Open   → 立即返回 CircuitBreakerOpen
  └── HalfOpen → 执行 → 成功记录 → Closed / 失败 → Open
```

## 错误分类依据

- **Retryable**：瞬时故障，重试有望成功。包括网络超时、DNS 解析失败、服务端 5xx、WebSocket 断开。
- **NonRetryable**：客户端错误（4xx）、序列化错误、协议错误。重试无效，直接返回。
- **CircuitBreakerOpen**：熔断器主动拒绝，不消耗重试配额，由上层决定等待或快速失败。
