# 10 — 错误处理策略

> 更新于 2026-05 — v3 调研闭环后修正：408/429/NXDOMAIN/TLS 证书分类均已对齐 RFC

## 错误映射表

| 底层错误源 | CatcherError 变体 | 可重试？ | 重试时动作 |
|-----------|-------------------|---------|-----------|
| reqwest timeout | RequestTimeout | Y | 销毁空闲连接 → 退避 → 重试 |
| reqwest connect failure | ConnectionTimeout | Y | 退避 → 重试 |
| HTTP 5xx | HttpError { status: 5xx } | Y | 退避 → 重试 |
| HTTP 408 Request Timeout | HttpError { status: 408 } | Y | **新建连接**重试（RFC 9110 §15.5.7） |
| HTTP 429 Too Many Requests | HttpError { status: 429 } | Y | 解析 Retry-After header → honor 退避（RFC 6585） |
| HTTP 4xx (其他) | HttpError { status: 4xx } | N | 直接返回 |
| TLS 握手超时 | TlsError | Y | 退避 → 重试 |
| TLS 证书错误 (自签名/域名不匹配/过期) | TlsError | N | 直接返回（配置错误，重试无效） |
| DNS NXDOMAIN | DnsError | N | 直接返回（永久性：域名不存在） |
| DNS SERVFAIL / Timeout / Refused | DnsError | Y | 切换备用解析器 → 退避 → 重试 |
| tungstenite ConnectionClosed | WsDisconnected | Y | 重连状态机 |
| tungstenite Protocol error | WsDisconnected | Y | 重连状态机 |
| rmp_serde encode error | EncodeError | N | 直接返回 |
| rmp_serde decode error | DecodeError | N | 直接返回 |
| circuitbreaker-rs reject | CircuitBreakerOpen | Y | 等待 reset_timeout |

## 分类实现方式

当前通过 `category()` 方法内的字符串/状态码匹配实现，未引入独立的 `kind` 枚举字段：

- **TLS**：`msg.to_lowercase()` 匹配 `certificate / cert / expired / mismatch / unknown issuer / self signed` → NonRetryable，其余 Retryable
- **DNS**：`reason.to_lowercase()` 匹配 `nxdomain` → NonRetryable，其余 Retryable
- **HTTP**：`status == 408 \|\| status == 429 \|\| status >= 500` → Retryable，其余 NonRetryable

> 未来演进方向：当匹配规则复杂到需要结构化的错误上下文时，可升级为 `TlsErrorKind` / `DnsErrorKind` 枚举字段（见 `v3-code-fixes.md` C2/C3）。

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

- **Retryable**：瞬时故障，重试有望成功。包括网络超时、DNS SERVFAIL/超时、TLS 握手超时、服务端 5xx/408/429、WebSocket 断开。
- **NonRetryable**：永久性故障或配置错误。包括客户端错误（4xx 除 408/429）、DNS NXDOMAIN、TLS 证书错误、序列化错误、协议错误。
- **CircuitBreakerOpen**：熔断器主动拒绝，不消耗重试配额，由上层决定等待或快速失败。
