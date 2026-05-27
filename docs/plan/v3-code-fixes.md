# v3 调研 → 代码修复计划

> 来源：`docs/research/CODE-AUDIT-FINDINGS.md` + 10 个独立实验结论
> 范围：`catcher-core/src/error.rs`、`catcher-core/src/types/resilience.rs`、`catcher-http/src/`
> 原则：只修复与调研结论冲突的缺陷和实验验证过的改进，不做无依据的改动

---

## 一、P0 缺陷（立即修复，影响功能正确性）

### C1: HTTP 408 归类为 NonRetryable → Retryable

| 项目 | 内容 |
|------|------|
| **当前代码** | `error.rs:82-86` — 所有 4xx（含 408）走 `ErrorCategory::NonRetryable` |
| **调研结论** | RFC 9110 §15.5.7: 408 MAY be retried on a new connection。Nginx 75s / Apache 5s 默认 keepalive timeout → keepalive race 是高频事件 |
| **修复** | 1 行改动：408 特判为 `Retryable` |
| **验证** | 现有 29 个 error 测试全通过（已确认） |
| **改动量** | 1 行 |
| **状态** | ✅ 已实施（`error.rs:111`: `status == 408 \|\| status == 429 \|\| status >= 500 → Retryable`） |

### C2: DNS NXDOMAIN 归类为 Retryable → NonRetryable

| 项目 | 内容 |
|------|------|
| **当前代码** | `error.rs:76` — `DnsError { .. } => ErrorCategory::Retryable`（所有 DNS 错误一刀切） |
| **调研结论** | NXDOMAIN 占 ~15% DNS 查询，是**永久性**错误（域名不存在），重试无意义且浪费资源。SERVFAIL (~1%) 才是可重试的 |
| **测试也错了** | `error.rs:126-131` 用 NXDOMAIN 测试 `is_retryable`，断言为 true |
| **修复** | `DnsError` 增加 `kind` 字段区分 `NxDomain / ServFail / Timeout / Refused`。NXDOMAIN → NonRetryable，其余 → Retryable |
| **联动影响** | DNS 解析层需要区分并传递 `kind`；多解析器 fallback 也需要感知 NXDOMAIN 不重试 |
| **改动量** | ~30 行（已实施：`error.rs:101-107` 通过 `reason.to_lowercase().contains("nxdomain")` 字符串匹配） |
| **状态** | ✅ 已实施（字符串匹配方案。未来可升级为 `DnsErrorKind` 枚举 — 见 C2 演进） |

### C3: TLS 证书错误归类为 Retryable → NonRetryable

| 项目 | 内容 |
|------|------|
| **当前代码** | `error.rs:75` — `TlsError(_) => ErrorCategory::Retryable`（所有 TLS 错误一刀切） |
| **调研结论** | IMC 2016 测量：88% 无效证书是自签名，11.99% 是 hostname mismatch。这些都是**配置错误**，重试无法解决 |
| **修复** | `TlsError` 增加 `kind` 字段区分 `Certificate(自签名/域名不匹配/过期) / HandshakeTimeout / Other`。证书类 → NonRetryable，握手超时 → Retryable |
| **改动量** | ~20 行（已实施：`error.rs:85-98` 通过 `msg.to_lowercase()` 匹配 `certificate/cert/expired/mismatch/unknown issuer/self signed`） |
| **状态** | ✅ 已实施（字符串匹配方案。未来可升级为 `TlsErrorKind` 枚举） |

### C4: HTTP 429 Retry-After 全仓零命中

| 项目 | 内容 |
|------|------|
| **当前代码** | 全仓搜索 `RetryAfter\|retry_after\|429\|RateLimit` → 零命中 |
| **调研结论** | RFC 6585 + AWS SDK 最佳实践：~40% API 返回 1-5s Retry-After，~10% 返回 3600s。必须解析并 honor |
| **实验验证** | Exp12（`experiments/`）已完成 Retry-After 解析 + 退避联动验证 |
| **修复** | `RetryConfig` 增加 `respect_retry_after: bool`（默认 true）。解析 `Retry-After` header（支持 HTTP-date 和 delta-seconds 两种格式），覆盖当前退避延迟 |
| **改动量** | ~40 行 |
| **状态** | ⬜（设计+实验验证完成，代码未落地） |

### C5: 无 Retry Budget（Token Bucket）

| 项目 | 内容 |
|------|------|
| **当前代码** | 全仓搜索 `token_bucket\|retry_budget\|rate_limit` → 零命中 |
| **调研结论** | Google SRE + AWS SDK + RetryGuard 论文一致结论：retry budget 是防止重试风暴的**唯一**有效手段 |
| **实验验证** | Exp6（`experiments/`）：token bucket 减少 87% 无意义重试 |
| **修复** | 在 `catcher-core` 实现通用 `TokenBucket`（500 tokens 默认，对标 AWS SDK）。瞬态错误消耗 14 tokens/次，节流错误消耗 5 tokens/次。`RetryAgent` 在每次重试前 `try_consume()`，耗尽后拒绝重试 |
| **改动量** | ~100 行（`catcher-core/src/resilience/token_bucket.rs` + `RetryAgent` 接入） |
| **状态** | ⬜（设计+实验验证完成，代码未落地） |

### C6: Backoff 默认 Fixed → Exponential

| 项目 | 内容 |
|------|------|
| **当前代码** | `resilience.rs:7-8` — `#[default] Fixed` |
| **调研结论** | Google SRE + AWS Builders Library + 学术 SLR 一致结论：指数退避 + Jitter 是唯一正确的默认策略。Full Jitter 错误率 6% vs 无 Jitter 17% |
| **注意** | Jitter 默认已启用 ✅（`resilience.rs:36,59 — jitter: default_true()`），仅需改退避种类 |
| **修复** | `#[default]` 从 `Fixed` 改为 `DecorrelatedJitter`（已实施：`resilience.rs:12-13` — `#[default] DecorrelatedJitter`） |
| **改动量** | 1 行 |
| **状态** | ✅ 已实施 |

---

## 二、P1 改进（短期，影响鲁棒性和极端场景）

### C7: connect_timeout 硬编码 15s

| 项目 | 内容 |
|------|------|
| **当前行为** | 依赖 OS TCP SYN 重试默认值（Linux = 127s）→ BGP 黑洞下无意义等待 2 分钟+ |
| **调研结论** | Cloudflare 用 19s，Akamai 用 5s。BGP 事件中位持续时间 2.75h（Hubble 数据），每起日均 ~230 起 |
| **修复** | `HttpClientConfig.connect_timeout_ms` 硬编码默认值 15,000ms（覆盖 OS 默认） |
| **改动量** | ~5 行 |
| **状态** | ⬜ |

### C8: CB 增加 min_failure_window

| 项目 | 内容 |
|------|------|
| **当前代码** | `resilience.rs:67-81` — CB 仅有连续失败计数，无时间窗口 |
| **调研结论** | Starlink 15s 周期性抖动 → 连续 5 次失败触发 CB → 可能误熔断。Exp2 证明 count-based window 扩大到 60s 仍无法消除误触发（需 rate-based CB 从根本上解决 — 见架构计划） |
| **修复** | `CircuitBreakerConfig` 增加 `min_failure_window_ms: Option<u64>`。即使在连续失败模式下，失败必须发生在至少 `min_failure_window_ms` 时间窗口内才算数 |
| **改动量** | ~30 行 |
| **状态** | ⬜（此为缓解措施；根本解决需 C10 Rate-based CB） |

### C9: RTT 感知退避联动

| 项目 | 内容 |
|------|------|
| **当前行为** | `max_backoff_ms` 默认 10,000ms，对 GEO 卫星（600ms RTT）只够 3-4 次重试就封顶 |
| **调研结论** | Exp9: RTT > 500ms 时 time-budget 重试显著更优。phase-final-synthesis.md E3: max_backoff 应与 RTT 联动 |
| **修复** | `RetryConfig.max_backoff_ms` 下限 = max(10,000, RTT_p90 × 4)。初始 RTT 来自首次请求测量或 `initial_rtt_estimate` 配置 |
| **改动量** | ~15 行 |
| **状态** | ⬜ |

---

## 三、已确认无需改动（代码正确 ✅）

| 项目 | 代码位置 | 调研验证 |
|------|---------|---------|
| Jitter 默认启用 ✅ | `resilience.rs:37` — `jitter: default_true()` | Exp5: No Jitter = 同步风暴 |
| max_attempts = 3 ✅ | `resilience.rs:56` — `default_max_attempts()` | Exp7: 50% 丢包下 93.75%，n→5 增益 < 1pp |
| CB 三态 (Closed/Open/HalfOpen) ✅ | `resilience.rs:110-118` | 行业标准 |
| keepAlive = 30s 默认 ✅ | 连接池配置 | Exp4: 覆盖所有 CGNAT 场景 |
| 5xx → Retryable, 4xx(除 408/429) → NonRetryable ✅ | `error.rs:111` | 逻辑正确 |
| Backoff 默认 DecorrelatedJitter ✅ | `resilience.rs:12-13` | C6 已实施 |

---

## 四、执行顺序

```
第 1 批 (已完成 ✅):
  ├── C1: 408 → Retryable ✅
  ├── C6: Backoff 默认 DecorrelatedJitter ✅
  ├── C2: DNS NXDOMAIN/SERVFAIL 区分 ✅ (字符串匹配)
  └── C3: TLS 证书/握手超时区分 ✅ (字符串匹配)

第 2 批 (待实施):
  ├── C4: 429 Retry-After 解析 (~40 行)
  ├── C7: connect_timeout = 15s (5 行)
  └── C9: RTT 感知退避 (~15 行)

第 3 批 (待实施):
  ├── C5: Retry Budget token bucket (~100 行)
  └── C8: CB min_failure_window (~30 行)
```

---

## 五、验证要求

- 每个修复必须有对应单元测试（参照现有 `error.rs` 测试风格）
- C2/C3 修复后**必须修正**现有错误测试（NXDOMAIN 不再断言 `is_retryable`）
- C5 token bucket 必须有容量/消耗/refill 的独立测试
- 全部修复完成后跑 `cargo test --workspace` + `pnpm test`

---

## 六、关联文档

| 文档 | 关系 |
|------|------|
| `docs/research/CODE-AUDIT-FINDINGS.md` | 缺陷发现来源 |
| `docs/research/QUANTITATIVE-ANALYSIS.md` | 定量模型支撑 |
| `docs/research/phase-final-synthesis.md` | 7 个 P0 缺口完整上下文 |
| `docs/plan/v3-architecture-changes.md` | C8 的根治方案（Rate-based CB） |
| `docs/plan/v3-verification-closure.md` | 修复后的验证闭环 |
