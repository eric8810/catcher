# Catcher 项目反思：初心、复杂度与收益

> 日期：2026-06
> 基于 v0.2.2 全量代码审查

---

## 一、我们当初想做什么

Catcher 的核心定位用一句话概括：

> **在业务代码和网络之间插入一层韧性屏障，让网络故障在到达业务逻辑之前就被捕获和处理。**

这个定位回答了一个真实痛点：axios / dio / reqwest 这些库只管发请求，不管网络不好怎么办。开发者需要自己拼凑 retry、circuit breaker、timeout 策略，而且每个平台各搞一套。

实现路径是 **Rust 核心实现 → napi-rs 薄封装暴露给 Node.js → FFI 暴露给 Flutter/移动端**。TS 版本（基于 axios）的角色是对照实验——用纯 JS 实现同样的韧性栈，与 Rust 路径做 E2E 对比，验证 Rust 版本的有效性。

这个初心是对的。问题在于，我们在执行过程中走了一些弯路。

---

## 二、项目规模盘点

### 代码量

| 层 | 包数 | 源码行数 | 文件数 |
|----|------|---------|--------|
| Rust core/http/ws | 3 crate | ~7,110 行 | 50 文件 |
| Rust FFI/UniFFI | 2 crate | ~618 行 | 2 文件 |
| TS core/http/ws (对照版) | 3 包 | ~5,968 行 | 42 文件 |
| TS browser (web) | 1 包 | ~2,583 行 | 17 文件 |
| napi-rs 绑定 | 2 包 | ~1,153 行 | 12 文件 |
| Dart FFI | 1 包 | ~2,317 行 | 8 文件 |
| E2E 测试 | 1 包 | ~4,000+ 行 | 若干 |
| **功能代码合计** | **13 包** | **~23,749 行** | **~131 文件** |

### 文档量

| 目录 | 文件数 | 行数 |
|------|--------|------|
| docs/arch-rs/ | 17 | 3,081 |
| docs/arch-ts/ | 16 | 2,421 |
| docs/plan/ | 16 | 5,045 |
| docs/research/ | 7 | 2,065 |
| docs/issues/ | 32 | 2,815 |
| docs/user-manual/ | 19 | 4,599 |
| docs/test/ | 6 | 1,082 |
| **文档合计** | **113** | **21,108** |

---

## 三、做得好的部分（应该坚持的）

### 1. 韧性栈是核心价值

重试 + 熔断 + 自适应超时 + 网络质量评估 + 优先级队列 — 这一套组合在 axios/dio/reqwest 生态中确实没有同等方案。这不是过度工程，这是项目的护城河。

验证数据也支撑这一点：弱网场景下 catcher 100% 成功率 vs vanilla 60-80%。

### 2. Rust 核心的分层设计

```
catcher-core (零 I/O) → catcher-http / catcher-ws (I/O) → catcher-ffi (C ABI umbrella)
```

core 零 I/O 依赖，http/ws 各自独立，ffi 是薄聚合层。这个分层是对的。

### 3. 错误分类（Retryable vs NonRetryable）

`CatcherError::category()` 将错误二分为可重试/不可重试，这是整个重试系统的决策基础。简洁而有效。

### 4. SSE 支持

三种模式（one-shot stream / persistent auto-reconnect / Rust tokio_stream）覆盖了 AI streaming 和长连接推送两个主要场景。时机恰当（AI 浪潮中 SSE 是刚需）。

### 5. 文档驱动开发

docs/ 目录不是事后补的，而是开发前定义好的——架构设计、模块拆分、Phase 规划、API 设计都是先写文档再写代码。这种方式确保了开发方向可控、设计可追溯。21,000 行文档对应 24,000 行代码的比例在文档驱动开发模式下是合理的。

### 6. TS 对照组 + E2E 对比

TS 版本不是"另一套实现"，而是**对照实验**。它和 Rust 版本跑完全相同的 E2E 场景（S1-S16c + chaos），在弱网/良好/极端环境下做 vanilla vs catcher 对比。这种验证方式比单元测试更有说服力。

---

## 四、需要重新审视的部分

### 🟡 问题 1：FFI 层的"深"不是问题，但需要分清用户驱动和想象驱动

**现状**：25 个 C ABI 符号，覆盖 HTTP/SSE/WS/Codec/Quality 五个模块。

**FFI 层的深度本身不是问题**。Flutter 用户确实需要：
- SSE 客户端（AI streaming 在 Flutter 中没有好用的方案）
- 熔断器状态查询（UI 上显示网络健康度）
- 指标收集（运维监控）
- 网络质量历史（自适应策略的输入）
- 自适应超时配置（不同网络环境自动调节）

这些是真实需求，不是过度设计。

**真正需要警惕的是**：未来增加 FFI 符号时，应该有明确的用户场景驱动，而不是"Rust 已经实现了所以 FFI 也应该导出"。建议建立 FFI tier 机制——核心 CRUD 稳定不变，高级特性允许迭代。

### 🟡 问题 2：韧性库应该拥有哪些 HTTP/WS 配置？

这是反思中最重要的问题。api-gap-analysis.md 列出 19 个 gap 逐项对比 axios/dio，但更根本的问题是：**韧性库该管什么，不该管什么？**

#### 原则：直接影响"请求能否成功完成"的配置，韧性库应该拥有

用户不应该为了配置超时去理解底层 HTTP 客户端的 API，也不应该为了设置代理而绕过韧性层——因为代理路径的不同直接影响重试策略、熔断判断和超时计算。

#### 韧性库应该拥有的配置（用户不应手动设置底层客户端）

**超时与时间控制** — 直接影响重试决策和熔断窗口：
- 连接超时 (`connect_timeout_ms`)
- 响应超时 (`response_timeout_ms`)
- 自适应超时（P90 RTT 动态计算）

**连接池与保活** — 重试时复用死连接是真实故障源：
- 最大空闲连接数 (`max_idle_per_host`)
- 空闲超时 (`idle_timeout_secs`)
- TCP keepalive 间隔 (`keep_alive_interval_secs`)

**重试策略** — 核心韧性能力：
- 最大重试次数 (`max_attempts`)
- 退避策略 (`backoff` — exponential/fixed/linear)
- 可重试条件 (`retry_if` — 哪些错误码/状态码值得重试)
- 抖动 (`jitter` — 防雷群效应)

**熔断器** — 核心韧性能力：
- 失败阈值 (`failure_threshold`)
- 恢复超时 (`reset_timeout_ms`)
- 半开状态下的探测数量

**并发与调度** — 过载保护：
- 最大并发数 (`max_concurrency`)
- 优先级 (`priority`)
- 队列容量

**网络路径** — 不同的路径有不同的故障特征：
- 代理 (`proxy` — URL + 认证 + no_proxy)
- DNS 配置 (`dns` — 自定义 DNS 服务器 + host mapping)
- TLS 配置 (`tls` — 证书验证 + mTLS + SNI)

**重定向** — 重定向循环是常见的故障模式：
- 是否跟随 (`follow`)
- 最大次数 (`max_redirects`)

**认证** — token 过期导致 401 是最常见的重试触发器：
- Bearer token（配合 token 刷新拦截器）
- Basic auth

**默认请求头** — 每次请求自动携带：
- 用于注入 auth、trace id、客户端标识等

**请求取消** — 网络恢复策略的一部分：
- 单请求取消（页面切换、用户中断）
- 批量取消（组件卸载）

**per-request 覆盖** — 不同请求有不同的韧性需求：
- 单次请求的超时覆盖（大文件上传 vs 普通查询）
- 单次请求的重试策略覆盖（写操作不重试）
- 单次请求的 headers（临时 token）

**SSE** — 韧性层的长连接场景：
- 自动重连配置
- Last-Event-ID 恢复
- 超时管理

**WebSocket** — 韧性层的长连接场景：
- 多端点竞速（故障转移）
- 重连策略（指数退避 + 抖动）
- 心跳超时（检测死连接）
- 压缩（减少传输失败概率）

#### 韧性库不应该拥有的配置（属于数据格式和便捷功能）

| 配置 | 为什么不该由韧性库管 | 谁管 |
|------|---------------------|------|
| 响应类型选择 (json/text/blob) | 这是数据格式，不影响请求成败 | 调用方自行反序列化 |
| Query 参数序列化 | URL 构建是调用方的职责 | 调用方拼 URL |
| FormData / Multipart 构建 | 数据格式封装，不影响传输韧性 | 调用方构建 body |
| 文件上传/下载进度 | UX 功能，不是韧性功能 | 调用方自行处理 |
| transformRequest / transformResponse | 数据转换管道，不是网络策略 | 调用方 |
| 实例克隆 | DX 便捷功能 | — |
| 配置合并策略 | DX 便捷功能 | — |
| Headers API (大小写不敏感等) | 属于 HTTP 规范细节 | — |

**一句话总结**：韧性库拥有所有影响"请求能否成功"的配置，不拥有"数据怎么表示"的配置。用户只需要提供 URL + body，韧性层负责如何可靠地送达。

#### 当前 HttpClientConfig 的配置审计

| 字段 | 该不该有 | 判断 |
|------|---------|------|
| `base_url` | ✅ | 路由基础 |
| `connect_timeout_ms` | ✅ | 直接影响重试/熔断 |
| `response_timeout_ms` | ✅ | 直接影响重试/熔断 |
| `pool` (keep_alive/idle_timeout/...) | ✅ | 死连接是真实故障源 |
| `tls` (全套) | ✅ | TLS 握手失败是常见重试触发器 |
| `dns` (nameservers/host_mapping) | ✅ | DNS 解析失败直接影响请求成败 |
| `retry` | ✅ | 核心韧性能力 |
| `circuit_breaker` | ✅ | 核心韧性能力 |
| `max_concurrency` | ✅ | 过载保护 |
| `default_headers` | ✅ | auth/trace 注入 |
| `hostname_override` | ✅ | HTTP DNS 场景 |
| `proxy` | ✅ | 网络路径影响故障特征 |
| `redirect` | ✅ | 重定向循环是故障模式 |
| `auth` / `bearer_token` | ✅ | token 过期是最常见的 401 触发器 |

**结论：当前 `HttpClientConfig` 的所有字段都合理，没有"管多了"的情况。**

api-gap-analysis.md 中列出的 P0 gap（请求取消、拦截器、per-request options）确实是基本能力缺失——不是在追赶 axios，而是韧性库本身就需要这些能力来做好自己的工作。P1/P2 中与数据格式相关的项目（FormData、文件上传、响应类型选择）则确实不属于韧性库。

### 🟡 问题 3：架构应该是 Rust 核心 + napi-rs 薄封装，不是双轨并行

**现状**：存在两条 Node.js 路径：
- `@eric8810/catcher-http`（TS，基于 axios，~4,508 行）
- `@eric8810/catcher-napi-http`（Rust via napi-rs，~871 行）

**正确的理解**：napi-rs 是生产路径，Rust 核心通过 napi-rs 暴露为 `.node` addon，TS wrapper 只做类型转换和 JS 惯用 API 适配（~100-200 行的薄层）。TS 版本（axios 版）的角色是**对照实验**——用纯 JS 实现同样的韧性栈，在 E2E 测试中与 Rust 路径做对比验证。

**对照实验已经完成了它的使命**：E2E 数据证实了 Rust 版在弱网下 100% vs vanilla 60-80%。对照版可以保留在代码库中供参考和测试，但不应再作为"另一条生产路径"维护。

**理想架构**：

```
catcher-core (Rust, 零 I/O)
     │
 ├──→ catcher-http / catcher-ws (Rust, 韧性实现)
 │       │
 │       ├──→ napi-rs (薄封装，~200 行 Rust) → JS wrapper (~100 行 TS) → npm
 │       ├──→ catcher-ffi (C ABI) → dart:ffi → Flutter
 │       └──→ catcher-uniffi → Swift / Kotlin
 │
 └──→ catcher-http-ts (对照实验，E2E benchmark 用途)
```

### 🟡 问题 4：平台节奏需要更务实

**现状**：13 个包同时存在，但 Flutter 集成验证未完成、UniFFI 需要额外工具链。

**这不是说这些平台不该做**，而是应该按用户需求分批交付：
- **已验证可用**：Rust crate + napi-rs Node.js + Browser（纯 TS + fetch）
- **需运行时验证**：Flutter dart:ffi（代码写好了，还没加载 `.so` 跑过）
- **框架就绪**：UniFFI（代码骨架完成，需要 Swift/Kotlin 工具链验证）

原则：每个平台通道只有在通过完整的构建-测试-发布闭环后才算"已支持"。

---

## 五、真正走得太远的部分

重新审视后，只有以下两项属于"做多了"：

### 1. Browser 包独立实现了完整的韧性栈

`@eric8810/catcher-web`（~2,583 行）用纯 TS + fetch 重新实现了 retry/CB/queue/SSE，和 Rust 核心完全独立。

**问题**：浏览器不能跑 Rust，所以纯 TS 实现是唯一选择，这本身没错。但 2,583 行是一个完整的 HTTP 客户端，如果定位是"Rust 核心 + napi-rs 为主"，那 browser 包的优先级和投入需要匹配实际使用场景。

**建议**：Browser 包保留，但明确它是"无 Rust 运行时环境的降级方案"，API 和功能可以精简。

### 2. 文档的组织结构可以更高效

文档驱动开发是对的，但 113 个文件散布在 7 个目录中，跨目录引用多，查找成本高。具体建议：
- arch-rs 和 arch-ts 合并为统一架构文档（减少重复翻译）
- issues/ 迁移到 GitHub Issues（保留交互能力）
- 已完成的 Phase 归档，只保留 remaining-work.md

这不是说文档多了，而是说同样的信息可以用更少的文件表达。

---

## 六、当前最应该做的三件事

1. **补齐 P0 基本能力** — 请求取消、动态拦截器、Per-request Options。这些是韧性库的基本功，用户需要用这些能力来控制请求生命周期。

2. **完成 Flutter 运行时验证** — 代码已经写好，需要实际加载 `.so` 并跑通 roundtrip。FFI 层的 25 个符号是 Flutter 用户需要的，但需要验证它们真的能用。

3. **明确 napi-rs 为生产路径** — TS wrapper 保持薄封装，对照版标记为 benchmark 用途，不再作为并行维护的生产路径。

---

## 七、结语

Catcher 的初心 — "让网络故障在到达业务逻辑之前就被捕获" — 是对的。

架构也是对的：Rust 核心实现韧性策略，napi-rs/FFI/UniFFI 薄封装暴露到各平台。FFI 层的深度由用户需求驱动，文档由开发流程驱动，TS 对照版由验证需求驱动。

真正需要调整的不是"做了什么"，而是"节奏"——先验证一个平台的完整闭环（构建→测试→发布→真实项目集成），再扩展到下一个。
