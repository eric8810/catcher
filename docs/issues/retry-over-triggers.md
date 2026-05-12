# Issue: 轻度弱网下 retry 触发过多，放大延迟

**发现来源**: E2E 场景 S3 🟡弱网 — catcher P50=8s vs vanilla P50=2s，但双方成功率均为 100%

**严重程度**: 🟡 中

---

## 根因

在轻度弱网（5% 丢包 + 2% 断连），vanilla 已经能 100% 成功。但 catcher 的一条请求中：
- 第一个请求遇到 `socket hang up` → 触发 retry#1
- retry#1 遇到 `socket hang up`（复用坏连接）→ 触发 retry#2
- retry#2 成功

vanilla 用 2s 完成的事，catcher 用了 8s，其中 6s 是不必要的重试等待。

## 当前 retry 策略

```typescript
// src/http/retry.ts:20-41
retries: 2,
factor: 2,         // exponential backoff
minTimeout: 1000,  // 最小退避 1s
maxTimeout: 30000,
```

退避序列: 1s → 2s → (放弃)

每个重试等待都算在用户体验里。3 个请求并发时（S1），累积等待时间可超过 20s。

## 影响

| 场景 | Vanilla P50 | Catcher P50 | 放大倍数 |
|------|:----------:|:----------:|:------:|
| S1 🟡弱网 | 2.0s | 2.0s | 1x |
| S3 🟡弱网 | 2.0s | 8.0s | **4x** |
| S3 🔴极弱网 | 2.0s | 2.1s | 1x |

S3 弱网下放大 4 倍，但双方成功率都是 100%——说明这些重试是**不必要**的。

## 建议修复

1. **自适应 retry 阈值**: 根据 RTT 和近期成功率动态决定是否 retry
    - 高成功率(>80%) → 减少 retry 次数
    - 低成功率(<50%) → 保持或增加 retry
2. **首次快速判断**: 第一个请求有 `ECONNRESET` 时才 retry，`ETIMEDOUT` 可能是服务器问题不应重试
3. **retry budget**: 在时间窗口内限制总 retry 次数，避免雪崩
4. **区分可重试错误**: 只对 `ECONNRESET`/`ENOTFOUND` 重试，对 timeout 不重试（可能服务端过载）
