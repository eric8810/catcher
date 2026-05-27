# 代码审计报告 — 对照 12 轮调研发现

> 审计日期：2026-05
> 审计范围：`catcher-core/src/error.rs`, `catcher-core/src/types/resilience.rs`

---

## P0 缺陷（与调研结论直接冲突）

### 1. HTTP 408 归为 NonRetryable ❌

**代码位置**: `error.rs:82-86`
```rust
CatcherError::HttpError { status, .. } => {
    if *status >= 500 {
        ErrorCategory::Retryable
    } else {
        ErrorCategory::NonRetryable  // ← 408 也走这里
    }
}
```

**调研结论**: RFC 9110 §15.5.7: 408 MAY be retried on a new connection。Nginx 75s/Apache 5s 默认 keepalive → keepalive race 是高频事件。

**修复**: 1 行改动 — 408 特判为 Retryable。

---

### 2. DNS NXDOMAIN 归为 Retryable ❌

**代码位置**: `error.rs:76`
```rust
| CatcherError::DnsError { .. } => ErrorCategory::Retryable,
```

**调研结论**: NXDOMAIN 占 ~15% DNS 查询，是**永久性**错误（域名不存在），重试无意义且浪费资源。SERVFAIL (~1%) 才是可重试的。

**测试也错了**: `error.rs:126-131` 用 NXDOMAIN 测试 `is_retryable`，断言为 true。

**修复**: DnsError 需增加 `kind` 字段区分 NXDOMAIN/SERVFAIL/Timeout/Refused。

---

### 3. TLS 错误全部 Retryable ❌

**代码位置**: `error.rs:75`
```rust
| CatcherError::TlsError(_) => ErrorCategory::Retryable,
```

**调研结论**: IMC 2016: 88% 无效证书是自签名，11.99% 是 hostname mismatch。这些都是**配置错误**，重试无法解决。

**修复**: TLS 错误需区分：证书类（NonRetryable）vs 握手超时（Retryable）。

---

### 4. 429 Retry-After 全仓零命中 ❌

**搜索**: `RetryAfter|retry_after|429|RateLimit` → 全仓零命中。

**调研结论**: RFC 6585 + AWS SDK 最佳实践（节流→1000ms base delay）。~40% API 返回 1-5s Retry-After，~10% 返回 3600s。

**修复**: RetryConfig 增加 `respect_retry_after: bool`，解析 `Retry-After` header。

---

### 5. 无 Retry Budget ❌

**搜索**: 无 token bucket、rate limit、retry budget 相关代码。

**调研结论**: Google SRE + AWS SDK + RetryGuard 论文一致结论：retry budget 是防止重试风暴的唯一有效手段。

**修复**: 引入 token bucket（500 tokens, 瞬态 14/次, 节流 5/次，对标 AWS SDK）。

---

### 6. Backoff 默认 Fixed 而非 Exponential ❌

**代码位置**: `resilience.rs:7-8`
```rust
#[default]
Fixed,
```

**调研结论**: Google SRE + AWS Builders Library + Academia SLR 一致结论：指数退避+Jitter 是唯一正确的默认策略。Full Jitter 错误率 6% vs 无 Jitter 17%。

**修复**: 改默认为 `Exponential` 或 `DecorrelatedJitter`。

---

### 7. 无 connect_timeout 独立配置 ⚠️

**代码位置**: `error.rs:7` — `ConnectionTimeout(u64)` 存在但未验证默认值是否合理。

**调研结论**: Linux 默认 127s → BGP 黑洞下等待 2min+。Cloudflare 用 19s，Akamai 用 5s。

**修复**: 确认实际 connect_timeout 默认值，如依赖 OS 则硬编码 15s。

---

### 8. CB time_window 缺 min_failure_window ⚠️

**代码位置**: `resilience.rs:67-81` — CB 无时间窗口概念，仅有连续失败计数。

**调研结论**: Starlink 15s 周期性抖动 → 连续 5 次失败触发 CB（5 × 15s = 75s）→ 可能导致误熔断。

**修复**: CB 增加 `min_failure_window_ms` 防止瞬时抖动误触发。

---

## 已有正确实现 ✅

| 项目 | 状态 | 代码位置 |
|------|:---:|------|
| Jitter 默认启用 | ✅ | `resilience.rs:36,59` — `jitter: default_true()` |
| min/max_backoff 合理 | ✅ | 100ms/10,000ms |
| max_attempts=3 | ✅ | 行业标准 |
| CB 三态 (Closed/Open/HalfOpen) | ✅ | `resilience.rs:109-116` |
| 5xx→Retryable, 4xx→NonRetryable | ⚠️ | 逻辑正确但缺 408/429 例外 |
