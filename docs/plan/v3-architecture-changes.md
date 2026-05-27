# v3 调研 → 架构演进计划

> 来源：12+ 轮调研迭代、10 个独立实验、4 个被推翻的设计假设
> 原则：每次架构变更都有实验数据或权威标准支撑。不基于直觉改架构

---

## 一、核心洞察

v3 调研推翻了 4 个设计假设，揭示了当前架构需要演进的 5 个方向：

| 推翻的假设 | 正确方向 | 架构影响 |
|-----------|---------|---------|
| "统一指数退避" | 按错误类型差异化退避（瞬态快、节流慢） | `RetryConfig` 增加 per-category base_delay |
| "连接失败罕见" | 20% TCP 连接数据交换前终止 → 故障常态 | CB + retry 的设计从兜底转向持续应对 |
| "按技术分类够用" | 需场景分类补充（游戏行业按桌面/移动端分） | Profile 体系双分类法 |
| "退避与 RTT 无关" | `max_backoff ≥ RTT_p90 × 4` | 退避参数与实测 RTT 联动 |

---

## 二、A1: Rate-based Circuit Breaker（🔴 突破性发现）

### 问题

当前 CB 使用 **count-based** 模型（连续 N 次失败 → OPEN）。Experiment 2 证明在 Starlink 15s 周期性 RTT 突增场景下 **100% 误触发**。Experiment 8 证明 **rate-based CB 0% 误触发**。

### 实验数据

| CB 模型 | Starlink 误触发率 |
|---------|:------:|
| Count-based (threshold=3) | 100% |
| Count-based (threshold=5) | 100% |
| Count-based (threshold=10) | 100% |
| Count-based (threshold=20) | 100% |
| **Rate-based (rate=30%)** | **0%** |
| **Rate-based (rate=50%)** | **0%** |
| **Rate-based (rate=70%)** | **0%** |

### 原理

Starlink 的 2s RTT spike 只占 15s 周期的 13.3%。Rate-based CB 的滑动窗口失败率 ≈ 13.3%，远低于任何合理阈值（30-50%）。Count-based 只要 spike 期间连续 ≥ 5 个请求超时就 OPEN。

### 设计方案

```rust
// catcher-core/src/types/resilience.rs

/// CB 模式选择
pub enum CbMode {
    /// 连续失败计数（当前实现，适合独立故障场景）
    Count {
        failure_threshold: u32,
        /// 新增：失败必须发生在至少这个时间窗口内才算数
        min_failure_window_ms: Option<u64>,
    },
    /// 滑动窗口失败率（v3 新增，适合周期性抖动场景）
    Rate {
        /// 失败率阈值，如 0.5 = 50%
        failure_rate_threshold: f64,
        /// 滑动窗口大小
        window_seconds: f64,
    },
}

pub struct CircuitBreakerConfig {
    pub mode: CbMode,                          // 替代原 failure_threshold
    pub reset_timeout_ms: u64,
    pub half_open_max_requests: u32,
    pub half_open_max_successes: u32,
}
```

### 默认值

```rust
impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            mode: CbMode::Rate {
                failure_rate_threshold: 0.5,
                window_seconds: 30.0,
            },
            reset_timeout_ms: 30_000,
            half_open_max_requests: 3,
            half_open_max_successes: 2,
        }
    }
}
```

### 实现要点

1. **滑动窗口数据结构**：`VecDeque<(Instant, bool)>` — 每个请求插入 `(时间戳, 是否成功)`。计算失败率时遍历窗口内的记录
2. **窗口清理**：每次新请求前弹出窗口外的旧记录
3. **内存上限**：窗口内最多保留 `max_requests_per_window` 条记录（默认 10,000），超限时弹出最旧记录
4. **向后兼容**：`CbMode::Count` 保持现有行为不变，`CbMode::Rate` 为新默认

### 改动范围

| 层 | 文件 | 改动 |
|----|------|------|
| `catcher-core` | `types/resilience.rs` | 新增 `CbMode` 枚举 + 修改 `CircuitBreakerConfig` |
| `catcher-core` | `resilience/circuit_breaker.rs` | 新增 Rate 模式状态机实现 |
| `catcher-http` | `http_client.rs` | 适配新配置结构 |
| `catcher-napi-http` | `lib.rs` | TS 侧暴露 mode 选择 |
| `catcher-http-ts` | `types.ts` | 类型定义同步 |

| 项目 | 内容 |
|------|------|
| **改动量** | ~150 行（核心 ~100 行 + TS 类型 ~50 行） |
| **测试** | 需单元测试覆盖 rate 计算精度、窗口边界、状态转换；复用 Exp8 的 Monte Carlo 场景 |
| **风险** | 低 — count 模式保持原有行为；rate 模式是纯新增 |
| **依赖于** | 无。可独立实施 |
| **状态** | ⬜（实验已验证，代码未落地） |

---

## 三、A2: DNS 多层 Fallback 架构（🔴 高优先）

### 问题

当前 Catcher 只有单一 DNS 解析路径。Experiment 3 证明 2 个解析器 = 100× 可靠性提升，4 个 = 1,000,000×。

### 设计方案

```
DNS 解析优先级链（从快到慢，从可信到兜底）:

  1. host_mapping          ← 本地 hostname→IP 直映射（企业内网、灰度发布）
  2. HTTPDNS server        ← 专用 HTTPS API（腾讯 HTTPDNS 4 亿+ 用户，60%+ 劫持减少）
  3. DoH (DNS-over-HTTPS)  ← Cloudflare 1.1.1.1 / Google 8.8.8.8（防污染）
  4. UDP DNS               ← 传统 UDP 53 端口（最终 fallback）
```

### 实现要点

1. **并行竞速**：2-4 不做串行 fallback → 同时发起，先用先得
2. **结果缓存**：遵守 TTL，正缓存 + 负缓存（RFC 9520，NXDOMAIN 缓存时间有上限）
3. **缓存胜者**：per host + per network 缓存最快的解析器路径
4. **SERVFAIL vs NXDOMAIN 区分**（与 C2 联动）：
   - NXDOMAIN → 不触发 fallback（域名不存在是确定性答案）
   - SERVFAIL / Timeout → 触发下一个解析器

### 配置设计

```rust
// catcher-core/src/types/dns.rs

pub struct DnsConfig {
    /// hostname → IP 直映射（最高优先级）
    pub host_mapping: HashMap<String, String>,

    /// HTTPDNS 服务器 URL（如 "https://httpdns.example.com/d"）
    pub httpdns_url: Option<String>,

    /// DoH 服务器列表（如 ["https://1.1.1.1/dns-query"]）
    pub doh_servers: Vec<String>,

    /// UDP DNS 服务器列表（如 ["8.8.8.8:53", "1.1.1.1:53"]）
    pub udp_nameservers: Vec<String>,

    /// 竞速超时（一个解析器超时后等待下一个）
    pub resolve_timeout_ms: u64,

    /// 正缓存 TTL（默认 300s）
    pub positive_ttl_secs: u64,

    /// 负缓存 TTL（默认 60s，上限 300s per RFC 9520）
    pub negative_ttl_secs: u64,

    /// 缓存大小（默认 256 条）
    pub cache_size: usize,
}
```

### 改动范围

| 层 | 改动 |
|----|------|
| `catcher-core` | 新增 `types/dns.rs` — `DnsConfig` 定义 |
| `catcher-http` | 新增 `dns/multi_resolver.rs` — 多层 DNS 解析器实现 |
| `catcher-napi-http` | TS 侧暴露 DNS 配置 |
| `catcher-http-ts` | 类型定义同步 |

| 项目 | 内容 |
|------|------|
| **改动量** | ~250 行（核心解析器 ~150 行 + 缓存 ~50 行 + 类型 ~50 行） |
| **依赖** | `hickory-resolver`（已有）、`reqwest`（已有，HTTPDNS/DoH 用） |
| **风险** | 中 — 需要处理 DoH/HTTPDNS 的超时策略、并发竞速的正确清理 |
| **状态** | ⬜ |

---

## 四、A3: 按错误类型差异化退避策略（🟡 中优先）

### 问题

当前所有 Retryable 错误使用统一的 `base_delay`。但 AWS SDK 和 Google SRE 的经验表明：
- **瞬态错误**（连接重置、DNS 瞬败、5xx）→ 快速重试（base 10-50ms），因为这些错误通常瞬间消失
- **节流错误**（429、503 with Retry-After）→ 慢速重试（base 1000ms），给服务端恢复时间
- **超时错误**→ 谨慎重试，因为可能只是服务端慢——重试 = 双倍负载

### 设计方案

```rust
// catcher-core/src/types/resilience.rs

pub enum ErrorRetryClass {
    /// 瞬态错误 — 快速重试
    Transient,
    /// 节流错误 — 慢速重试
    Throttle,
    /// 超时错误 — 谨慎重试
    Timeout,
}

impl ErrorRetryClass {
    pub fn recommended_base_delay_ms(&self) -> u64 {
        match self {
            Self::Transient => 50,
            Self::Throttle => 1000,
            Self::Timeout => 200,
        }
    }
}

// CatcherError::retry_class() — 新增方法
impl CatcherError {
    pub fn retry_class(&self) -> ErrorRetryClass {
        match self {
            // 瞬态
            CatcherError::ConnectionError(_)
            | CatcherError::DnsError { kind: DnsErrorKind::ServFail, .. }
            | CatcherError::HttpError { status, .. } if *status >= 500 => ErrorRetryClass::Transient,

            // 节流
            CatcherError::HttpError { status: 429, .. }
            | CatcherError::HttpError { status: 503, .. } => ErrorRetryClass::Throttle,

            // 超时
            CatcherError::Timeout { .. } => ErrorRetryClass::Timeout,

            // 默认瞬态
            _ if self.is_retryable() => ErrorRetryClass::Transient,
            _ => unreachable!(), // NonRetryable 不会到这里
        }
    }
}
```

`RetryAgent` 计算延迟时：
```
actual_delay = retry_class.recommended_base_delay_ms() * backoff_multiplier
```
如果 429 有 `Retry-After` header，则 `Retry-After` 值覆盖计算结果。

### 改动范围

| 项目 | 内容 |
|------|------|
| **改动量** | ~80 行（`ErrorRetryClass` ~20 行 + `retry_class()` ~40 行 + `RetryAgent` 适配 ~20 行） |
| **风险** | 低 — 新增方法不影响现有调用路径 |
| **依赖于** | C2、C3、C4 完成（DNS/TLS/429 区分后才能正确分类） |
| **状态** | ⬜ |

---

## 五、A4: 移动端平台感知退避（🟡 中优先，长周期）

### 问题

Android Doze（2h 维护窗口）+ CGNAT（120s 超时）+ WS 心跳中断 → 恢复时间可达 2h。当前退避策略完全不感知平台约束。

### 设计方向（预留 hook，非完整实现）

```rust
// catcher-core/src/types/platform.rs

/// 平台事件 — 由 FFI/UniFFI 层从原生 SDK 收集，传入 Rust 核心
pub enum PlatformEvent {
    /// 网络恢复可用
    NetworkAvailable,
    /// 网络丢失
    NetworkLost,
    /// 进入省电模式（Doze / Low Power Mode）
    PowerSavingEntered,
    /// 退出省电模式
    PowerSavingExited,
}

/// 平台通知回调 trait
pub trait PlatformCallbacks: Send + Sync {
    fn on_network_available(&self);
    fn on_network_lost(&self);
    fn on_power_saving_entered(&self);
    fn on_power_saving_exited(&self);
}
```

### 退避行为变更

| 平台事件 | 退避行为 |
|---------|---------|
| `PowerSavingEntered` | 暂停所有非关键重试，保持 CB 状态 |
| `PowerSavingExited` | 立即发起一次探测请求（不等待退避计时器） |
| `NetworkAvailable` | 重置退避计数器，立即尝试重连 |
| `NetworkLost` | 不进行无意义重试（等 `NetworkAvailable` 触发） |

### 改动范围

| 项目 | 内容 |
|------|------|
| **改动量** | ~150 行（trait ~20 行 + `RetryAgent` 感知 ~50 行 + FFI 层事件桥接 ~80 行） |
| **风险** | 高 — 需要在 Android（JNI/UniFFI）和 iOS（C ABI/UniFFI）两侧接入原生 API（`ConnectivityManager.NetworkCallback` / `NWPathMonitor`） |
| **依赖于** | UniFFI 绑定成熟度、移动端集成进度 |
| **建议** | 先实现 Rust 侧的 hook 和 `RetryAgent` 感知逻辑，FFI 事件桥接留到移动端集成时再做 |
| **状态** | ⬜ |

---

## 六、A5: 架构路径明确化 — napi-rs 为主生产路径（🟢 低优先，已是事实）

### 当前状态

存在两条 Node.js 路径：
- `@eric8810/catcher-napi-http`（Rust via napi-rs，~871 行）
- `@eric8810/catcher-http`（TS 基于 axios，~4,508 行）

### 调研结论

retrospective.md 明确了 TS 版的角色是**对照实验**（benchmark 用途），不是并行生产路径。E2E 对比数据已证实 Rust 版在弱网下 100% vs vanilla 60-80%，对照实验使命已完成。

### 行动

| 行动 | 说明 |
|------|------|
| `catcher-napi-http` TS wrapper 精简 | 从 ~871 行降到 ~200 行薄封装（类型转换 + JS 惯用 API 适配） |
| `catcher-http` TS 版标记 `@deprecated` | README/package.json 注明 benchmark-only 用途 |
| `catcher-web` (Browser) 明确为降级方案 | 无 Rust 运行时环境的降级路径，功能可精简 |
| 文档更新 | 更新 README 和用户手册的平台选择指引 |

| 项目 | 内容 |
|------|------|
| **改动量** | 文档为主，代码 ~100 行精简 |
| **风险** | 低 — 不影响现有用户（npm 包名不变） |
| **状态** | ⬜ |

---

## 七、执行路线图

```
第 1-2 周:
  └── A1: Rate-based CB (~150 行) ← 实验最充分，改动独立

第 3-5 周:
  ├── A3: 按错误类型差异化退避 (~80 行) ← 依赖 C2/C3/C4
  └── A2 第一阶段: DNS host_mapping + 多 UDP fallback (~120 行)

第 6-10 周:
  ├── A2 第二阶段: DoH + HTTPDNS (~130 行)
  ├── A4 第一阶段: Rust 侧 hook + RetryAgent 感知 (~70 行)
  └── A5: 文档 + 路径明确 (~50 行)

远期（移动端集成时）:
  └── A4 第二阶段: FFI 事件桥接 (~80 行)
```

---

## 八、关联文档

| 文档 | 关系 |
|------|------|
| `docs/research/RATE-BASED-CB.md` | A1 的实验依据 |
| `docs/research/BEYOND-RETRY.md` | A2 DNS 多层 fallback 依据 |
| `docs/research/phase-final-synthesis.md` | 推翻的假设 1（A3 依据） |
| `docs/research/phase0-discovery-report.md` | E5 极端场景（A4 依据） |
| `docs/research/retrospective.md` | A5 路径决策依据 |
| `docs/plan/v3-code-fixes.md` | C2/C3/C4 是 A3 的前置条件 |
| `docs/plan/v3-verification-closure.md` | A1/A2/A3 的验证方案 |
