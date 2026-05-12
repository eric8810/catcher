# 12 — 状态机图

## WebSocket 连接状态机

```
                          ┌──────────────┐
                          │ DISCONNECTED │
                          └──────┬───────┘
                                 │ connect()
                          ┌──────▼───────┐
                    ┌─────│  CONNECTING  │
                    │     └──────┬───────┘
                    │            │ handshake ok
                    │     ┌──────▼───────┐
                    │     │  CONNECTED   │──────────────┐
                    │     └──────┬───────┘              │
                    │            │ disconnect            │ heartbeat RTT update
                    │     ┌──────▼───────┐         ┌─────▼──────┐
                    │     │RECONNECTING  │         │ Heartbeat  │
                    │     │ attempt,delay│         │  Monitor   │
                    │     └──────┬───────┘         └────────────┘
                    │            │ max_attempts reached
                    │            │ or close() called
                    │            ▼
                    └──── DISCONNECTED
```

**状态说明**

| 状态 | 说明 |
|------|------|
| DISCONNECTED | 初始/终止状态，无活跃连接 |
| CONNECTING | TCP + TLS + WebSocket handshake 进行中 |
| CONNECTED | 连接就绪，可收发帧。Heartbeat Monitor 并行运行，持续更新 RTT |
| RECONNECTING | 连接断开后，按退避策略重试。有最大重试次数上限 |

**状态迁移触发**

- `DISCONNECTED → CONNECTING`：用户调用 `connect()` 或首次创建
- `CONNECTING → CONNECTED`：handshake 成功
- `CONNECTING → RECONNECTING`：handshake 失败（DNS/连接超时等）
- `CONNECTED → RECONNECTING`：监听到断开事件或心跳超时
- `RECONNECTING → CONNECTING`：退避等待结束，发起下一次尝试
- `RECONNECTING → DISCONNECTED`：超过最大重试次数，或主动调用 `close()`
- `CONNECTED → DISCONNECTED`：主动调用 `close()`

---

## 熔断器状态机

```
        ┌──────────┐
        │  CLOSED  │  ← 正常状态
        └────┬─────┘
             │ failures >= failure_threshold
        ┌────▼─────┐
        │   OPEN   │  请求立即拒绝
        └────┬─────┘
             │ reset_timeout 到期
        ┌────▼──────┐
        │ HALF_OPEN │  试探请求
        └──┬────┬───┘
           │    │
    成功 ──┘    └── 失败
     │               │
     ▼               ▼
  CLOSED          OPEN
  (success_threshold  (reset_timeout
   连续成功)            重新计时)
```

**状态说明**

| 状态 | 说明 |
|------|------|
| CLOSED | 正常通行。内部维护滑动窗口失败计数。 |
| OPEN | 拒绝所有请求，立即返回 `CircuitBreakerOpen`。 |
| HALF_OPEN | 允许有限试探请求通过。连续成功达到阈值则转 CLOSED，任一失败则转回 OPEN。 |

**参数**

| 参数 | 说明 | 典型值 |
|------|------|--------|
| `failure_threshold` | 触发 OPEN 的连续失败次数 | 5 |
| `success_threshold` | HALF_OPEN → CLOSED 所需的连续成功次数 | 2 |
| `reset_timeout` | OPEN → HALF_OPEN 的等待时间 | 30s |

---

## 请求处理管道

```
用户请求
  │
  ▼
PriorityQueue.submit(priority)
  │
  ▼
DynamicConcurrency.acquire()
  │
  ▼
CircuitBreaker.call()
  ├── OPEN → 等待 reset_timeout → 重试 call
  │
  ▼
RetryScheduler.execute()
  │
  ▼
HttpTransport.execute()  (一次 HTTP 收发)
  ├── 失败 → ErrorCategory::Retryable → 退避 → 重回 RetryScheduler
  │                                     └── NonRetryable → 直接返回
  ├── 成功 → 记录 RTT 到 AdaptiveTimeout
  │
  ▼
返回 HttpResponse
```

**管道阶段说明**

| 阶段 | 组件 | 作用 |
|------|------|------|
| 入队 | PriorityQueue | 按优先级排序，高优先级先出队 |
| 并发控制 | DynamicConcurrency | 获取并发槽位，避免过载 |
| 熔断保护 | CircuitBreaker | 检测下游健康状态，故障时快速失败 |
| 重试调度 | RetryScheduler | 瞬态故障时按退避策略重试 |
| 传输执行 | HttpTransport | 实际发出 HTTP 请求，记录 RTT |
| 超时自适应 | AdaptiveTimeout | 根据历史 RTT 滑动窗口动态调整超时 |

---

## 退避策略状态机

```
Attempt 1: delay = 0ms       (首次立即发送)
    │ 失败 (Retryable)
    ▼
Attempt 2: delay = base * 1  (100ms)
    │ 失败 (Retryable)
    ▼
Attempt 3: delay = base * 2  (200ms)
    │ 失败 (Retryable)
    ▼
Attempt 4: delay = base * 4  (400ms)
    │ 失败 (Retryable)
    ▼
Attempt 5: delay = base * 8  (800ms)  ← max_delay 截断
    │ 失败
    ▼
RetryExhausted
```

**退避参数**

| 参数 | 说明 | 典型值 |
|------|------|--------|
| `base_delay` | 基础退避间隔 | 100ms |
| `max_delay` | 退避上限 | 1s |
| `max_retries` | 最大重试次数 | 5 |
| `jitter` | 随机抖动范围 | ±25% |
