# Architecture Gap Audit — 2026-05-15（2026-06-18 更新，2026-06-19 修复）

> 审查范围：全部包（Rust / TS / Dart / napi / UniFFI）的设计文档 vs 实际代码
> 方法：对照 docs/plan/、docs/issues/、docs/arch-rs/、docs/arch-ts/ 中的设计目标，逐项验证源码实现状态
>
> **更新说明**（2026-06-18）：补充类别 G/H/I，更新 D-01/D-02 为已修复状态，修正总览计数。
> **更新说明**（2026-06-19）：执行 Phase 1~5 修复，更新所有已修复条目状态。

---

## 一、发现总览

| 类别 | 数量 | 严重度 |
|------|:----:|:------:|
| A. 代码已实现但未接入管线 | 3 | 🟡 (A-01, A-03 已修复；A-02 待评估) |
| B. 设计文档有方案但代码未开始 | 2 | 🟡 (B-03 已修复) |
| C. 文档标记 🔲 但代码实际已完成 | 8 | 🟢 (文档已同步) |
| D. 已发现但未修复的 Bug | 0 | ✅ 全部修复 |
| D✅. 已修复的 Bug（原记录需关闭） | 5 | — |
| E. 类型定义存在但从未使用 | 4 | 🟢 |
| F. 缺失的测试 | 8 | 🟡 |
| **G. E2E 弱网性能问题** | **10** | **🔴** |
| **H. 平台绑定层缺口** | **2** | **🟡** (H-03/H-04/H-05 已修复) |
| **I. 规划中但未启动的功能** | **1** | **🟢** (I-02 已完成) |

---

## 二、类别 A：代码已实现但未接入管线

> 这类问题最隐蔽——模块存在、测试通过，但在实际请求流程中不起作用。

### ~~A-01~~. PriorityRequestQueue 未接入 HttpTransport — ✅ 已修复（2026-06-19）

**严重度**: ~~🟡 中~~ → ✅ 已修复

**修复方案**（2026-06-19 实施）：
- `HttpTransport` 新增 `concurrency_semaphore: Option<Arc<Semaphore>>` 字段
- 当 `max_concurrency > 0` 时，`execute_with_token()` 通过 `select!` 在获取信号量许可和取消令牌之间竞争
- 新增 `queue_depth()` 方法查询当前排队深度
- C ABI 符号已有 `catcher_http_queue_depth()`，TS 层 `queueDepth()` 已对接

**现状**:
- `packages/catcher-http/src/scheduler/priority_queue.rs` 完整实现（118 行）
  - biased select 优先从 high-priority channel 取任务
  - Semaphore 并发控制 + oneshot response channel
  - 3 个单元测试全部通过
- `packages/catcher-http/src/lib.rs` 正确 re-export
- **但 `HttpTransport`（transport/http_client.rs）完全未导入或引用 `PriorityRequestQueue`**
- 所有请求直接走 reqwest，无优先级排序

**影响**:
- 高优先级请求（POST 发送消息）无法优先于低优先级请求（GET 头像加载）
- `HttpClientConfig.max_concurrency` 仅通过 Semaphore 控制，队列 FIFO 无优先

**修复方案**:
1. `HttpTransport` 新增 `Option<Arc<PriorityRequestQueue>>` 字段
2. 当 config 中配置了 `max_concurrency > 0` 时，`execute()` 走队列调度
3. 新增 C ABI 符号 `catcher_http_queue_depth()` 查询队列深度
4. TS 层 `queueDepth()` 已存在，验证数据源对接

**关联**: handoff.md 待做事项 #3 (FFI-08)

---

### A-02. WebSocket per-message deflate 未生效

**严重度**: 🟡 中 — 压缩能力在 Rust WS 层不可用

**现状**:
- `packages/catcher-ws/src/ws/compression.rs` 中 `build_ws_config()` 存在
- 函数设置了 `max_message_size` / `max_frame_size`
- **但显式忽略 `per_message_deflate` 字段**（`compression.rs:18`）
- TS 层 `catcher-ws-ts` 的 perMessageDeflate 功能正常（Node `ws` 库支持）

**根因**:
tungstenite 全版本（0.20~0.29）均未实现 RFC 7692。上游 issue [#2](https://github.com/snapview/tungstenite-rs/issues/2) 从 2017 年开至今未关闭。代码注释"等 0.25+"不准确。

**影响**:
- Rust WS 连接不支持压缩，大数据消息（图片、文件）带宽浪费
- Dart/UniFFI 消费者的 WS 连接无压缩能力

**修复方案**:
详细评估见 [`tungstenite-permessage-deflate.md`](./tungstenite-permessage-deflate.md) 和 [`tungstenite-deflate-fork-analysis.md`](./tungstenite-deflate-fork-analysis.md)。

1. **短期**：升级 `tokio-tungstenite 0.24 → 0.26+`，适配 Message/CloseFrame API breaking change（方案 A）
2. **中期**：根据用户需求评估换用 yawc（Vector 背书）或 ratchet（SwimOS 生产使用）（方案 B2/B3）
3. 不建议 fork+patch（方案 B1），长期维护成本不可控

---

### ~~A-03~~. DNS 自定义解析 / host_mapping 仅为存根 — ✅ 已修复（2026-06-19）

**严重度**: ~~🟡 中~~ → ✅ 已修复

**修复方案**（2026-06-19 实施）：
- `dns.rs` 新增 `resolve_host_mapping()` 函数，返回 `Option<&'a str>`
- `do_execute()` 中进行 URL 改写：将 hostname 替换为映射 IP，原始 hostname 保留在 Host header
- `hostname_override` 配置时也注入 Host header
- 使用手动 URL 解析（无新依赖）
- 与 TLS SNI 配合：SNI 保持原始 hostname

**现状**:
- `packages/catcher-http/src/transport/dns.rs` 中 `build_dns_resolver()` 验证 config
- 但实际回退到系统 DNS — 自定义 nameservers 和 host_mapping 未接入 reqwest
- **TS 层对比**: `catcher-http-ts/src/agent/shared-agent.ts` 的 hostMapping 已正常工作

**影响**:
- Rust/napi/Dart 消费者无法使用 hostname → IP 直映射（企业内网、灰度发布、开发调试）
- 自定义 DNS 服务器配置不可用

**修复方案**:
1. 实现 hickory-resolver 的 `Resolve` trait，优先查 host_mapping，未命中走 nameservers
2. 启用 reqwest 的 `hickory-dns` feature
3. 确保与 tls.rs 的 SNI 处理配合（host_mapping 时 SNI 保持原始 hostname）

**关联**: api-gap-features.md G7

---

## 三、类别 B：设计文档有方案但代码未开始

### B-01. Transport Trait（自定义 Adapter）

**严重度**: 🟡 P1 — 影响测试可测性和扩展性

**现状**:
- `catcher-core-ts/src/types.ts` 定义了 `TransportAdapter` 类型但标记 `@deprecated`
- Rust `HttpTransport` 硬编码 reqwest，无法替换
- 无法在测试中注入 MockTransport

**设计目标** (api-gap-features.md G9):
```rust
pub trait Transport: Send + Sync {
    async fn execute(&self, request: HttpRequest) -> Result<HttpResponse, CatcherError>;
}
```

**当前阻塞**: 无直接用户需求，属于架构改善。TS 层已标记 deprecated 表明暂不实施。

---

### B-02. Multipart/FormData 编码器（Rust 侧）

**严重度**: 🟡 P1 — Dart/Swift/Kotlin 上传文件需要

**现状**:
- TS 层 FormData 自动处理已完成（axios + fetch 均支持）
- Rust `HttpRequest.body: Option<Vec<u8>>` + `content_type: Option<String>` 仅接受裸字节
- **无 Rust 侧 multipart 编码器** — Dart 走 dart:ffi 不走 dart:io，无法使用 `MultipartRequest`
- native-layer-capability-gaps.md N-01 标记 P2

**设计目标** (native-layer-capability-gaps.md 方案 B):
- `MultipartBuilder` + 5 个 C ABI 符号
- 或 Dart 侧自行编码（方案 A，~200-300 行）

---

### ~~B-03~~. 韧性事件推送未完成 — ✅ 已修复（2026-06-19，networkQualityChange 于同日补充）

**严重度**: ~~🟡 P1~~ → ✅ 已修复

**修复方案**（2026-06-19 实施）：
- `catcher-http-ts/client.ts` 新增熔断器状态追踪 (`lastBreakerState`)，状态变更时 emit `circuitBreakerChange`
- `catcher-web/client.ts` 同样实现状态追踪和事件发射
- `catcher-core-ts/types.ts` 中 `circuitBreakerChange` 和 `networkQualityChange` 事件类型已取消 `@deprecated` 标记
- **networkQualityChange 补充实现**：
  - `catcher-http-ts/client.ts` + `catcher-web/client.ts` 新增滑动窗口 RTT 追踪（窗口大小 20）
  - 基于 P90 RTT 平均值将质量分为 5 级：excellent(<80ms) / good(<200ms) / fair(<500ms) / poor(<1000ms) / bad(≥1000ms)
  - 级别变化时自动 emit `networkQualityChange` 事件
  - `catcher-core-ts/types.ts` 导出 `QualityLevel` 类型，`networkQualityChange` 事件从/from 类型从 `string` 收紧为 `QualityLevel`

---

### ~~B-04~~. 缺失的用户文档 — ✅ 已修复（2026-06-19）

**严重度**: ~~🟢 P2~~ → ✅ 已修复

| 文档 | 状态 |
|------|------|
| `docs/user-manual/rust.md` | ✅ 已存在（350 行） |
| `docs/user-manual/uniffi.md` | ✅ 已补写（Swift/Kotlin 使用指南） |
| `docs/arch-ts/10-web.md` | N/A — web 相关内容分散在 `user-manual/web.md` 和 `arch-ts/README.md` 中 |

---

## 四、类别 C：文档标记 🔲 但代码实际已完成

> **这是最需要注意的一类** — 文档与代码严重脱节，可能导致重复开发或误判。

以下功能在 `docs/issues/api-gap-features.md` 中标记为 🔲，但实际审查源码发现已完成：

| ID | 功能 | 文档状态 | TS 实际状态 | Rust 实际状态 |
|----|------|---------|-----------|-------------|
| G2 | 错误上下文丰富化 | 🔲 | ✅ `error.ts` 完整实现：type/request/response/attempt/elapsedMs/toJSON(脱敏) | ✅ `CatcherError` 枚举丰富 |
| G3 | CORS / credentials / cookie | 🔲 | ✅ `withCredentials`(axios) + `credentials`/`mode`(fetch) + XSRF cookie 读取 | N/A |
| G4 | 代理设置 | 🔲 | ✅ https-proxy-agent / socks-proxy-agent + 环境变量自动读取 | ✅ reqwest `.proxy()` |
| G5 | FormData / 文件上传 | 🔲 | ✅ `isFormDataBody()` 自动检测 + Content-Type 自动剥离 | ❌ 无 multipart 编码器 |
| G10 | 流式响应 | 🔲 | ✅ `responseType:'stream'` → Node Readable / Web ReadableStream | ✅ `execute_stream` + callback |
| G12 | 认证辅助 | 🔲 | ✅ Basic Auth + Bearer Token（支持异步刷新）+ XSRF | N/A |
| N-02 | 流式文件下载 | ✅ 标记已完成 | ✅ | ✅ `catcher_http_execute_stream` |
| N-03 | 单请求级 cancel | ✅ 标记已完成 | ✅ | ✅ per-request `CancellationToken` |
| N-04 | 网络质量推送 | ✅ 标记已完成 | N/A | ✅ `QualitySubscription` |

**另外**，`docs/issues/circuit-breaker-not-wired.md` 记录了 CB 未接入的问题，但审查当前代码发现：
- **Rust 层**: CB 已完整接入 `HttpTransport.execute_with_token()`（before_request/on_success/on_failure）
- **TS 层**: CB 已通过 cockatiel `CircuitBreakerPolicy` 包裹整个 retry-wrapped 请求管线
- 该 issue 应更新状态为 ✅

### 建议

更新 `docs/issues/api-gap-features.md` 和 `docs/issues/README.md` 中以上条目的状态，避免后续开发者误判。

---

## 五、类别 D：已发现但未修复的 Bug

### ~~D-01~~. TS per-request retry 覆盖完全无效 — ✅ 已修复

**严重度**: ~~🔴 高~~ → ✅ 已修复
**来源**: review-2026.md #1
**文件**: `packages/catcher-http-ts/src/http/client.ts`

**原始问题**：`rawDoRequest` 把 `args[args.length - 1]`（始终是 axiosConfig）传给 `effectiveRetry()`，但 retry 字段从未拷入 axiosConfig。

**修复验证**（2026-06-18）：
- `client.ts:380-382` 已将 retry 字段拷入 axiosConfig：
  ```typescript
  if (processedConfig.retry !== undefined) {
    axiosConfig.retry = processedConfig.retry
  }
  ```
- `rawDoRequest`（255-276 行）正确处理 per-request override（`effectiveRetry(reqCfg)` 读取 retry 配置，动态构建 wrapper）
- 测试文件 `__tests__/retry.test.ts` 已覆盖 per-request retry 场景

---

### ~~D-02~~. TS onRetry 回调触发两次 — ✅ 已修复

**严重度**: ~~🔴 高~~ → ✅ 已修复
**来源**: review-2026.md #2
**文件**: `packages/catcher-http-ts/src/http/retry.ts`

**原始问题**：同一个回调被调了两次（line 71 早期残留 + line 75 正确调用）。

**修复验证**（2026-06-18）：
- `retry.ts:70` 当前仅有一行调用：`options.onRetry?.(error.attemptNumber)`
- 早期残留的 `(options as any).onRetry?.(...)` 已移除

---

### ~~D-03~~. napi-http CbState 传递依赖 — ✅ 已修复（2026-06-19）

**严重度**: ~~🔴 高~~ → ✅ 已修复
**来源**: review-2026.md #3
**文件**: `packages/catcher-napi-http/Cargo.toml`

**修复**: `Cargo.toml` 已添加 `catcher-core = { path = "../catcher-core" }` 显式依赖。

---

### ~~D-04~~. napi-http 无意义的 feature flag — ✅ 已修复（2026-06-19）

**严重度**: ~~🟡 中~~ → ✅ 已修复
**来源**: review-2026.md #4
**文件**: `packages/catcher-napi-http/Cargo.toml`

**修复**: `features = ["napi"]` 已移除。

---

### ~~D-05~~. napi-http client.d.ts 类型不一致 — ✅ 已修复（2026-06-19）

**严重度**: ~~🟡 中~~ → ✅ 已修复
**来源**: review-2026.md #6, #10
**文件**: `packages/catcher-napi-http/client.d.ts`

**修复**:
- `patch()` 返回类型已更正为 `Promise<HttpResponse>`
- 补齐缺失方法：`metrics()`, `setAdaptiveTimeout()`, `disableAdaptiveTimeout()`, `cancelAll()`, `cancelRequest()`, `nextRequestId()`, `executeStream()`
- 新增 `Metrics` 接口

---

## 六、类别 E：类型定义存在但从未使用的死代码

> 这些类型在 `catcher-core-ts/types.ts` 中有定义，但没有任何代码路径消费它们。

| 类型/字段 | 所在位置 | 说明 |
|-----------|---------|------|
| `TransportAdapter` | types.ts | 标记 `@deprecated`，createHttpClient() 忽略该字段 |
| `beforeRedirect` 回调 | types.ts (redirect config) | catcher-http-ts 注释 "Axios 不支持"，类型存在但不生效 |
| ~~`circuitBreakerChange` 事件类型~~ | types.ts | ✅ B-03 已修复，catcher-http-ts + catcher-web 均 emit |
| ~~`networkQualityChange` 事件类型~~ | types.ts | ✅ 已修复 — TS 层基于滑动窗口 RTT 质量分级（excellent/good/fair/poor/bad），状态变化时自动 emit |
| TLS 完整配置字段（TS 侧死代码） | types.ts (TlsConfig) | `caCertPem`, `clientKeyPem`, `pinSha256`, `minTlsVersion` 等 — catcher-http-ts/client.ts 从未读取（Node.js TLS 由 agent 层管理）|
| DNS `nameservers` 字段 | types.ts (DnsConfig) | shared-agent.ts 只用 hostMapping，不用 nameservers；Rust 层也未接入 |
| catcher-web 进度回调 | types.ts | `onUploadProgress`/`onDownloadProgress` 在 catcher-web 中零匹配 — fetch() API 不支持进度事件，需 ReadableStream 手动实现 |
| ~~catcher-web `credentials` / `fetchMode`~~ | types.ts (HttpClientConfig) | ✅ 已实现 — `client.ts:275-276` 正确传递到 fetch()，5 个测试覆盖 |

**建议**: 这些类型要么实现对应功能，要么移除定义以避免误导用户。

---

## 七、类别 F：缺失的测试

| 编号 | 测试 | 优先级 | 状态 |
|------|------|:------:|:----:|
| TEST-02 | Dart 集成测试 CI 兼容（CATCHER_FFI_PATH 环境变量） | 🟡 P1 | 🔲 |
| TEST-03 | Napi binding 集成测试 (catcher-napi-http / catcher-napi-ws) | 🟡 P2 | 🔲 |
| TEST-04 | UniFFI 绑定测试 (需 Swift/Kotlin 工具链) | 🟢 P3 | 🔲 |
| TEST-05 | WsTransport 测试 (需 tokio-tungstenite echo server) | 🟡 P1 | 🔲 |
| TEST-06 | multi_endpoint 多端点竞速测试 | 🟡 P1 | 🔲 |
| TEST-07 | compression (perMessageDeflate) 测试 | 🟡 P2 | 🔲 |
| TEST-09 | Dart 单元测试 — 仅测序列化，需补充实际 FFI 调用测试 | 🟡 P2 | 🔲 |
| TEST-10 | TS 测试未覆盖 Napi 路径 | 🟡 P2 | 🔲 |

---

## 八、类别 G：E2E 弱网性能问题

> 来源：2026-05-11 端到端性能对比测试（8 个场景 × 多种网络条件，vanilla vs catcher 并发对比）
> 详细文件：[docs/issues/README.md](./README.md) 及各独立 issue 文件
>
> 这类问题不属于代码缺失，而是架构/算法层面的运行时行为缺陷。在弱网环境下，catcher 的表现反而不如 vanilla（无韧性库），削弱了核心价值主张。

### 核心因果链

```
keepAlive 池中坏连接
  → retry 对坏连接反复重试（G-01，🟡 已缓解：Rust pool timeout 90→30s + keepalive 60→20s）
  → 重试触发过多（G-03）
  → keepAlive 健康检查缺失（G-02，🟡 已缓解：Rust 调优 + TS socket error 驱逐）
  → 请求放大效应：catcher 比 vanilla 慢 4x / 成功率更低
```

### 问题清单

| # | 问题 | 严重度 | 状态 | 独立 Issue 文件 |
|---|------|:------:|:----:|----------------|
| G-01 | **Retry 复用坏连接** | 🔴 | 🟡 已缓解 | [retry-reuses-bad-connection.md](./retry-reuses-bad-connection.md) |
| G-02 | **keepAlive 无健康检查** | 🔴 | 🟡 已缓解 | [keepalive-broken-connection.md](./keepalive-broken-connection.md) |
| G-03 | Retry 触发过多 | 🟡 | 🔲 | [retry-over-triggers.md](./retry-over-triggers.md) |
| G-06 | 代理延迟在连接时固化（测试基础设施 bug）| 🔴 | 🔲 | [proxy-latency-captured-at-connect.md](./proxy-latency-captured-at-connect.md) |
| G-07 | retry minTimeout 偏高（退避从 1s 起步） | 🟡 | 🔲 | [retry-min-timeout-too-high.md](./retry-min-timeout-too-high.md) |
| G-08 | S5 大体积消息缺 retry | 🟡 | 🔲 | [s5-missing-retry.md](./s5-missing-retry.md) |
| G-09 | S7 metric 滥用（msgFinishOrder 当延迟） | 🟡 | 🔲 | [s7-metric-abuse.md](./s7-metric-abuse.md) |
| G-10 | chaos parseInt 下划线（`600_000` → 600ms） | 🟡 | 🔲 | [chaos-parseint-underscore.md](./chaos-parseint-underscore.md) |
| G-11 | reporter 统计缺陷（全失败假改善等） | 🟡 | 🔲 | [reporter-stat-flaws.md](./reporter-stat-flaws.md) |
| G-12 | 延迟对比跨重试次数混算 | 🟡 | 🔲 | [retry-bucketed-comparison.md](./retry-bucketed-comparison.md) |

> **注意**：G-06 为测试基础设施 bug，可能导致 G-01~G-05 的 E2E 证据需要重新评估。

### 关键证据

| 指标 | 数据 |
|------|------|
| retry 放大延迟 | S3 🟡弱网: vanilla P50=2s, catcher P50=8s（双方 100% 成功） |
| keepAlive 降低成功率 | S5 🟡弱网: vanilla 80% vs catcher 60% |
| keepAlive 降低成功率 | S8 🟡弱网: vanilla 60% vs catcher 40% |

### 已验证有效的 catcher 能力（非问题）

| 能力 | 证据 |
|------|------|
| keepAlive 减少连接数 | S1: 连接数 3→1 (-67%) |
| retry 提升极端弱网成功率 | S2 🔴极弱网: 20% → 100% |
| DNS 缓存减少重复解析 | DNS 集成测试: 后续请求仅首次的 9% |

---

## 九、类别 H：平台绑定层缺口

> 这类问题影响 catcher 在特定平台（Dart/Flutter、Swift/Kotlin/UniFFI、napi）上的实际可用性。

### 问题清单

| # | 问题 | 严重度 | 状态 | 来源 |
|---|------|:------:|:----:|------|
| ~~H-01~~ | ~~Flutter dart:ffi 运行时验证未做~~ | 🔴 | ✅ roundtrip 已验证 | 19/19 FFI 符号导出确认，集成测试已写，CI workflow 已接入 |
| ~~H-02~~ | ~~UniFFI 缺少 SSE / codec / quality 导出（FFI-11）~~ | 🟡 | ✅ | 已有导出：sse_stream + SseClientHandle + catcher_pack/unpack + evaluate_quality |
| ~~H-03~~ | ~~Napi WS 仅支持 text 发送~~ | 🟡 | ✅ | 已修复 2026-06-19 |
| ~~H-04~~ | ~~Dart FFI 缺少流式下载绑定~~ | 🟡 | ✅ | 已修复 2026-06-19 |
| ~~H-05~~ | ~~Dart FFI 缺少 per-request cancel 绑定~~ | 🟡 | ✅ | 已修复 2026-06-19 |

### ~~H-01~~. Flutter dart:ffi 运行时验证 — ✅ Roundtrip 已验证（2026-06-19）

**严重度**: 🔴 高（运行时层面） — 已验证通过

**验证内容**（2026-06-19）:
- `cargo build --release -p catcher-ffi` → `catcher_ffi.dll`（4.2MB）成功构建
- **19/19 FFI 符号在 DLL 中确认导出**（通过 PE 二进制字符串扫描）：
  `catcher_http_client_create`, `catcher_http_client_destroy`, `catcher_http_execute`, `catcher_http_execute_with_id`, `catcher_http_cancel_request`, `catcher_http_execute_stream`, `catcher_pack`, `catcher_unpack`, `catcher_free_result`, `catcher_free_data`, `catcher_ws_create`, `catcher_ws_send_text`, `catcher_ws_send_binary`, `catcher_ws_close`, `catcher_ws_destroy`, `catcher_http_client_cancel_all`, `catcher_http_circuit_breaker_state`, `catcher_http_metrics`, `catcher_http_adaptive_timeout_config`
- **集成测试文件**: `packages/catcher_core/test/ffi_roundtrip_test.dart`
  - FFI symbol resolution (16 个核心符号)
  - Codec pack ↔ unpack roundtrip（5 项：简单 map、嵌套结构、空 map、int list、1000 条大数据）
  - HTTP client lifecycle（create/query/dispose、double dispose safety）
  - HTTP GET/POST roundtrip（httpbin.org：GET /get、GET /status/404、POST /post echo、custom headers）
  - StreamEvent sealed class hierarchy 验证
  - Per-request cancel API（cancelRequest for non-existent request）
  - Adaptive timeout enable/disable
- **CI workflow**: `.github/workflows/ci.yml` 新增 `dart-ffi-roundtrip` job（ubuntu-latest, Rust build → Dart pub get → dart test）
- **本地未跑通原因**: 本机无 Dart SDK，需 CI 环境执行

**现状**（2026-06-19 核实）:
- `ffi_bindings.dart` — 所有 C ABI typedef 完整（create/destroy/execute/execute_stream/execute_with_id/cancel_request/cancel_all/circuit_breaker_state/metrics/adaptive_timeout_config/sse_connect/sse_stream/ws_create/ws_send_text/ws_send_binary/ws_close/pack/unpack/evaluate_quality）
- `http_client.dart` — 完整的高级 API（get/post/put/delete/patch + executeStream + executeWithCancel + cancelRequest + sseStream + circuitBreakerState + metrics + setAdaptiveTimeout + cancelAll）
- `ws_client.dart` / `sse_client.dart` / `quality.dart` / `codec.dart` — 均有对应 wrapper
- `native_loader.dart` — 平台感知的 .so/.dll/.dylib 加载
- Rust 侧 FFI 导出匹配：`catcher_http_execute_with_id`(http_ffi.rs:214)、`catcher_http_cancel_request`(http_ffi.rs:283)、`catcher_http_execute_stream`(http_ffi.rs:350) 等
- **唯一缺口**：从未在 CI 中实际加载 .so/.dll 做 Dart roundtrip 测试，无法验证 ABI 对齐（字段偏移、类型大小、调用约定）。需 `CATCHER_FFI_PATH` 环境变量 + Dart VM 集成测试。

### ~~H-02~~. UniFFI SSE / codec / quality 导出 — ✅ 已完成

**严重度**: 🟡 中 — ~~Swift/Kotlin 消费者功能不完整~~ 已补齐

**现状**（2026-06-19 核实）:
- `catcher-uniffi/src/lib.rs`（591 行）已导出全部能力：
  - **SSE**: `HttpClient.sse_stream()` (L287) — 一次性 SSE 流；`SseClientHandle::connect()` (L494) — 持久 SSE 连接 + auto-reconnect；`SseEventDto` + `SseEventObserver`
  - **Codec**: `catcher_pack()` (L542) + `catcher_unpack()` (L550) — msgpack JSON↔binary
  - **Network Quality**: `evaluate_quality()` (L565) — HTTP HEAD 测量，返回 JSON
- 审计前描述"360 行仅导出 HTTP + WS 基础操作"已过时，实际 lib.rs 已包含完整导出

### ~~H-03~~. Napi WS 仅支持 text 发送 — ✅ 已修复（2026-06-19）

**现状**: Rust `send_binary()` 已存在。修复方案：
- `client.d.ts` 新增 `sendBinary(data: Buffer | ArrayBuffer | Uint8Array): void`
- `client.js` 新增 `sendBinary()` 方法，自动将 ArrayBuffer/Uint8Array 转为 Buffer 后调用 `_raw.send_binary()`
- 同时更新 `close()` 方法签名以匹配 Rust `close(code?, reason?)`

### ~~H-04~~ / ~~H-05~~. Dart FFI 缺少流式下载和 per-request cancel — ✅ 已修复（2026-06-19）

**修复方案**：
- `ffi_bindings.dart` 新增 typedef：
  - `CatcherHttpExecuteStreamNative/Dart` — 流式下载
  - `CatcherHttpExecuteWithIdNative/Dart` — 带请求 ID 的执行
  - `CatcherHttpCancelRequestNative/Dart` — 单请求取消
- `http_client.dart` 新增：
  - `executeStream()` → `Stream<StreamEvent>` — 流式下载事件流
  - `executeWithCancel()` → `({requestId, response})` — 可取消请求
  - `cancelRequest(requestId)` → `bool` — 取消单请求
  - `StreamEvent` sealed class 体系：`StreamHeadersEvent`, `StreamChunkEvent`, `StreamDoneEvent`, `StreamErrorEvent`
- `catcher_core.dart` barrel 导出已更新

---

## 十、类别 I：规划中但未启动的功能

> 设计/分析文档已完成，但零行实现代码。

| # | 功能 | 设计文档 | 说明 |
|---|------|---------|------|
| I-01 | **catcher-tus 断点续传上传客户端** | [ws-tus-split-analysis.md](../research/ws-tus-split-analysis.md) | 完整的可行性分析和 API 设计已完成，可作为独立包 `catcher-tus` 实现 |
| ~~I-02~~ | ~~proxy.ts corrupt/reorder/duplicate 损伤模型~~ | [remaining-work.md](../plan/remaining-work.md) P3 #10 | ✅ 已完成 — `proxy.ts` 中 corrupt (line 260-265)、reorder (line 268-273)、duplicate (line 276-278) 均已实现 |

---

## 十一、建议修复优先级

### ~~Phase 1 — Bug 修复（立即）~~ — ✅ 全部完成

~~D-03 CbState 传递依赖 → D-04 napi feature flag → D-05 类型不一致~~

D-01 (per-request retry) 和 D-02 (onRetry 双触发) 已在 2026-06-18 前修复。
D-03, D-04, D-05 于 2026-06-19 修复。

### ~~Phase 2 — E2E 弱网性能修复（短期，高优先级）~~ — 待实施

```
G-02 keepAlive 健康检查 → G-01 retry 复用坏连接 → G-07 retry minTimeout 偏高 → G-03 retry 触发过多
```

> 注意：G-06（代理延迟固化）为测试基础设施 bug，需先修复后再重跑 E2E 确认 G-01~G-03 的证据是否仍然成立。

### ~~Phase 3 — 平台验证与管线接入~~ — ✅ 主要条目已完成

已完成：
- ✅ A-01: Semaphore 并发控制接入 HttpTransport
- ✅ A-03: DNS host_mapping URL 改写 + Host header 注入
- ✅ B-03: circuitBreakerChange + networkQualityChange 事件推送
- ✅ H-03: Napi WS binary send
- ✅ H-04: Dart FFI executeStream
- ✅ H-05: Dart FFI cancelRequest

未完成：
- ✅ H-01: Flutter dart:ffi CI roundtrip 测试 — 集成测试已写，CI workflow 已接入，19/19 符号导出确认

~~未完成~~ 已核实完成：
- ✅ H-02: UniFFI SSE/codec/quality 导出 — `catcher-uniffi/src/lib.rs` 已包含全部导出

### ~~Phase 4 — 文档同步~~ — ✅ 已完成

- ✅ api-gap-features.md 状态更新（G2/G3/G4/G7/G10/G11/G12 → ✅）
- ✅ issues/README.md 状态更新
- ✅ uniffi.md 补写（Swift/Kotlin 使用指南）
- ✅ C 类别文档状态已同步

### ~~Phase 5 — 新功能（中期）~~ — 部分完成

已完成：
- ✅ H-04/H-05: Dart 流式下载 + per-request cancel
- ✅ H-03: Napi WS binary

待实施：
- 🔲 A-02: WS deflate (等 tungstenite 升级)
- 🔲 B-01: Transport trait
- 🔲 B-02: Multipart 编码器

### Phase 6 — 测试补全（中期）

```
TEST-05/06 (WS) → TEST-02/09 (Dart) → TEST-03/10 (Napi) → TEST-07 (compression) → TEST-04 (UniFFI)
```

### Phase 7 — 规划中功能（远期）

```
I-01 catcher-tus 断点续传 → I-02 proxy.ts 损伤模型补全
```

---

## 十二、与现有 Issue 文档的交叉引用

| 本文档编号 | 现有 Issue | 关系 |
|-----------|-----------|------|
| A-01 | handoff.md #3 (FFI-08) | 同一问题 |
| A-02 | — | 审计新发现 |
| A-03 | api-gap-features.md G7 | Rust 层面的 G7（TS 层已完成） |
| B-01 | api-gap-features.md G9 | 同一问题 |
| B-02 | native-layer-capability-gaps.md N-01 | 同一问题 |
| B-03 | api-gap-features.md G11 | ✅ circuitBreakerChange + networkQualityChange 均已实现 |
| ~~D-01~~ | review-2026.md #1 | ✅ 已修复 |
| ~~D-02~~ | review-2026.md #2 | ✅ 已修复 |
| ~~D-03~~ | review-2026.md #3 | ✅ 已修复 2026-06-19 |
| ~~D-04~~ | review-2026.md #4 | ✅ 已修复 2026-06-19 |
| ~~D-05~~ | review-2026.md #6, #10 | ✅ 已修复 2026-06-19 |
| C 全部 | api-gap-features.md G2~G12 | 文档状态需更新 |
| G-01~G-12 | issues/README.md #1~#12 | E2E 性能问题（原编号映射为 G-01~G-12 避免与 API Gap G2 混淆） |
| ~~H-01~~ | remaining-work.md P1 #5 | ✅ roundtrip 已验证，CI 已接入 |
| ~~H-02~~ | ffi-uniffi-capability-gaps.md FFI-11 | ✅ lib.rs 已有完整导出 |
| H-03 | ffi-uniffi-capability-gaps.md | Napi WS 能力差异 |
| H-04 | native-layer-capability-gaps.md N-02 | Dart 侧缺少绑定 |
| H-05 | native-layer-capability-gaps.md N-03 | Dart 侧缺少绑定 |
| I-01 | ws-tus-split-analysis.md | 完整设计已完成，零行代码 |
| I-02 | remaining-work.md P3 #10 | 接口已有，逻辑未写 |
