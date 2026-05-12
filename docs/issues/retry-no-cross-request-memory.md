# Issue: Retry 无跨请求记忆，连续失败不从高退避开始

**发现来源**: 分析报告 S3 🟡弱网 catcher P50=8s vs vanilla P50=2s + 架构审查

**严重程度**: 🟡 中（部分被 circuit breaker 覆盖）

---

## 现状

每次请求的 retry 退避从零开始，不感知近期网络状况：

```
当前（无记忆）:
  请求1 → 1s 2s 4s → 失败
  请求2 → 1s 2s 4s → 失败    ← 又从 1s 开始，不管前面已经连续失败
  请求3 → 1s 2s 4s → 失败

理想（有记忆）:
  请求1 → 1s 2s 4s → 失败
  请求2 → 4s 8s 12s → 失败   ← 初始延迟增大
  请求3 → 🔴 熔断，不重试     ← circuit breaker 介入
```

## 调研结论

### 「退火」不是业界术语

「模拟退火」（Simulated Annealing）是优化算法，用于车辆路径规划、网络拓扑设计等问题，**不用于请求重试**。

生产系统中「跨请求状态记忆」的标准做法是 **Circuit Breaker + 简单指数退避**，而非在 retry 内部实现退火。

### 各生产系统的做法

| 系统 | Retry 策略 | 跨请求状态 | 状态粒度 |
|------|-----------|:---:|------|
| Envoy | 指数退避 + jitter | ✅ Circuit Breaker | 按 upstream cluster |
| gRPC | 指数退避 + hedging | ✅ Circuit Breaker | 按服务实例 |
| AWS SDK | 指数退避 + jitter | ✅ Retry Budget (token bucket) | 全局 |
| OkHttp | 指数退避 | ✅ ConnectionPool 健康检查 | 按连接 |
| **Cockatiel** (已安装) | 指数退避 | ✅ **Circuit Breaker** (内置三态) | 按策略实例 |

### 核心洞察

不需要在 retry 内部加退火/温度系统。**Circuit breaker 已经有三态记忆**，retry 保持简单即可：

```
                    ┌─────────────────────┐
                    │   Circuit Breaker    │  ← 有状态
                    │   Closed → Open      │     跟踪连续失败计数
                    │   → HalfOpen → Closed│     自动恢复探测
                    └─────────┬───────────┘
                              │
                    ┌─────────▼───────────┐
                    │      Retry           │  ← 无状态
                    │  1s → 2s → 4s        │     只管单请求重试
                    └─────────────────────┘
```

## 自建退火/温度系统的代价

| 收益 | 代价 |
|------|------|
| 连败时不必从零退避 | **全局可变状态**——多 Agent/多 client 需要锁或共享 |
| 更精准的延迟控制 | **状态同步**——不同模块的 HTTP client 是否共享温度？ |
| | **配置爆炸**——升温速率、冷却速率、初始温度、降温阈值... |
| | **调试困难**——"为什么退避用了 12s 而不是 1s？" 需要可视化 |
| | **与现有工具冲突**——Cockatiel 的 CB 和自建温度系统谁说了算？ |

## 建议

**不要自建退火。接入 Cockatiel CircuitBreakerPolicy（已安装依赖）。**

改动点：`src/http/client.ts` 约 20 行，在 retry wrapper 外面包一层 circuit breaker。

价值：
- 连续失败 → 熔断 → 后续请求立即拒绝 → 不浪费 CPU/连接
- 跨请求失败计数（CB 三态就是「温度」的简化版）
- zero-ops：无新依赖，无新架构对象
- 业界验证：Envoy、gRPC、AWS 都在用同一模式

## 关联

- [circuit-breaker-not-wired.md](./circuit-breaker-not-wired.md) — CB 已配置但未接入
- [retry-over-triggers.md](./retry-over-triggers.md) — 轻度弱网触发过多不必要重试
- [retry-reuses-bad-connection.md](./retry-reuses-bad-connection.md) — retry 复用坏连接
