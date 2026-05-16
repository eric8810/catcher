# 5-16 Gap Fix Handoff

> 基于 `docs/issues/arch-gap-audit-2026.md` 审计，记录截至 2026-06-19 的剩余待办项。
> 已完成项见审计文档中标记 ✅ / ~~删除线~~ 的条目。
> 最后代码验证更新：2026-06-19（对照全部 109 个文档 + 源码）

---

## 🔴 高优先级（影响功能正确性）

| # | 问题 | 说明 | 状态 |
|---|------|------|------|
| G-01 | Retry 复用坏连接 | reqwest 连接池中过期连接被 retry 反复使用 | 🟡 已缓解（见下方说明） |
| G-02 | keepAlive 无健康检查 | 连接池无 liveness probe，无法淘汰死连接 | 🟡 已缓解（见下方说明） |
| ~~G-06~~ | ~~代理延迟在连接时固化~~ | ✅ 已修复：代理层每 chunk 动态读 conditions + setConditions 后 disruptAll 断开旧连接 + 修复带宽变量名 bug |
| ~~NEW-1~~ | ~~G8 pin_sha256 证书固定~~ | ✅ 已实现（`PinningVerifier` 包装 `WebPkiServerVerifier`，SHA-256 DER 证书哈希 pin 检查，base64 编码 pin 值） |

## 🟡 中优先级（功能缺口/质量）

### 架构 & 功能

| # | 问题 | 说明 |
|---|------|------|
| A-02 | WS per-message deflate | `compression.rs` 仍忽略 per_message_deflate。根因：tungstenite 0.29 仍不支持 RFC 7692。**短期升级已完成**（0.24→0.29，仅解决版本/API 落后）；中期：评估 Signal fork / upstream PR #426 experimental 路线，yawc 因社区验证不足暂不推荐默认替换 |
| B-01 | Transport trait 抽象 | 架构级变更，TS 层已标 `@deprecated`，无直接需求 |
| ~~B-02~~ | ~~Multipart 编码器（Rust 侧）~~ | ✅ Rust FFI `catcher_http_multipart` + `MultipartForm` 编码器已实现；TS 侧通过 `FormData` + `post()` 自动 multipart/form-data |
| ~~N-04~~ | ~~网络质量实时事件推送~~ | ✅ Rust QualitySubscription callback + TS `networkQualityChange` event 均已实现 |
| ~~NEW-2~~ | ~~catcher-web 进度回调~~ | ✅ 下载进度通过 ReadableStream 流式跟踪，上传进度在响应头到达时报告 100%（fetch 限制） |
| ~~NEW-3~~ | ~~TS TLS 配置死代码~~ | ✅ 已接入 Node.js https.Agent（ca/cert/key/minVersion/SNI/PFX），pinSha256 仍 deferred |

### 文档与代码状态不同步（需更新）

| # | 问题 | 说明 |
|---|------|------|
| ~~G6~~ | ~~重定向控制~~ | ✅ 代码已完成（TS `client.ts:192-193` + Rust `http_client.rs:80-82`），api-gap-features.md 已从 🔲 改为 ✅ |
| ~~G8~~ | ~~HTTPS 配置增强~~ | ✅ 大部分完成（mTLS/SNI/TLS版本 ✅），仅 pin_sha256 待做，api-gap-features.md 已更新 |
| ~~E-credentials~~ | ~~catcher-web credentials/fetchMode~~ | ✅ `client.ts:275-276` 已传递到 fetch()，arch-gap-audit 已更新 |

### E2E 弱网性能（依赖 G-06 修复后重跑）

| # | 问题 |
|---|------|
| ~~G-03~~ | ~~Retry 触发过多~~ | ✅ 已缓解（minTimeout 500ms + retry 时销毁坏连接 + 连接池超时缩短） |
| ~~G-07~~ | ~~retry minTimeout 偏高~~ | ✅ 已修复（TS minTimeout=500ms, Rust min_backoff_ms=100ms） |
| ~~G-08~~ | ~~S5 大体积消息缺 retry~~ | ✅ 已修复（`retry: { attempts: 2 }` 已配置，scenarios.test.ts:287） |
| ~~G-09~~ | ~~S7 metric 滥用~~ | ✅ 已修复（time 用实际延迟 ms，清理了未用 msgFinishOrder） |
| ~~G-10~~ | ~~chaos parseInt 下划线~~ | ✅ 已修复（`'600000'` 无下划线，chaos.test.ts:28） |
| ~~G-11~~ | ~~reporter 统计缺陷~~ | ✅ 全失败 P50 返回 0/N/A + P50 增加绝对差值列 |
| ~~G-12~~ | ~~延迟对比跨重试次数混算~~ | ✅ 已按重试次数分桶（harness.ts retries 字段 + 0-retry/retried 分桶展示） |

### 其他低优先级

| # | 问题 |
|---|------|
| G6-beforeRedirect | TS beforeRedirect 类型存在但不生效（Axios 限制） |
| ~~DNS nameservers~~ | ~~types.ts 定义了 nameservers 但 TS/Rust 均未接入~~ | ✅ TS CacheableLookup + Rust hickory-resolver 自定义 nameservers |

## 🟢 测试缺口

| # | 内容 | 需要环境 |
|---|------|----------|
| TEST-02 | Dart 集成测试 CI 兼容 | Dart SDK + `CATCHER_FFI_PATH`（已有 `ffi_roundtrip_test.dart`，需 CI 跑通） |
| TEST-03 | Napi binding 集成测试 | Node.js + native build |
| TEST-04 | UniFFI 绑定测试 | Swift/Kotlin 工具链 |
| TEST-05 | WsTransport 测试 | tokio-tungstenite echo server |
| TEST-06 | multi_endpoint 多端点竞速测试 | 同上 |
| TEST-07 | compression (perMessageDeflate) 测试 | 同上 |
| TEST-09 | Dart 单元测试补充 FFI 调用 | Dart SDK |
| TEST-10 | TS 测试未覆盖 Napi 路径 | Node.js + native build |

## 🟢 远期规划

| # | 问题 |
|---|------|
| I-01 | catcher-tus 断点续传 — 完整设计文档已就绪，零行代码 |

---

## 本次已完成的修复（2026-06-19）

- ✅ D-01~D-05: Bug 全部修复
- ✅ A-01: Semaphore 并发控制接入 + 优先级队列完整接入（见下方续篇）
- ✅ A-03: DNS host_mapping URL 改写 + Host header
- ✅ B-03: circuitBreakerChange + networkQualityChange 事件推送
- ✅ B-04: 用户文档同步
- ✅ H-01: Flutter dart:ffi roundtrip 验证（19/19 符号确认，集成测试 + CI 已写）
- ✅ H-02: UniFFI SSE/codec/quality 导出已存在（审计文档过时已纠正）
- ✅ H-03: Napi WS binary send
- ✅ H-04: Dart FFI executeStream
- ✅ H-05: Dart FFI cancelRequest
- ✅ I-02: proxy.ts 损伤模型（已存在）

## 本次文档验证发现（2026-06-19）

- ✅ G2 rawData: 已实现（`error.ts:85`）
- ✅ G6 重定向: TS+Rust 均已实现，`api-gap-features.md` 已从 🔲→✅
- ✅ G8 HTTPS: Rust 大部分已实现（mTLS/SNI/版本），pin_sha256 待做，`api-gap-features.md` 已更新
- ✅ catcher-web credentials/fetchMode: 已实现（`client.ts:275-276`）
- ✅ TS 拦截器: 已实现 `createInterceptorManager`，支持动态 add/remove
- ✅ catcher-http-ts 进度回调: 已实现（`client.ts:429-433`）
- ❌ catcher-web 进度回调: 确认缺失（fetch() 不支持进度事件）
- ✅ TS TLS 配置: ✅ 已接入 Node.js https.Agent（NEW-3 已修复）
- ❌ WS deflate: 确认忽略（`compression.rs:18`，tungstenite 全版本不支持 RFC 7692，评估文档已完成）

## 本次续篇修复（2026-06-19 续）

### G-01/G-02 缓解措施（🟡 部分完成）

**问题**：reqwest 连接池中过期/损坏连接被 retry 反复使用；keepAlive 无主动 liveness probe。

**已实施的缓解措施**（Rust 侧 `catcher-http/src/types/http.rs`）：
- `idle_timeout_secs`: 90 → **30s** — 缩短坏连接在池中存活时间
- `keep_alive_interval_secs`: 60 → **20s** — 更频繁探测连接活性（reqwest 0.13 升级后同时接入 `tcp_keepalive_interval` + `tcp_keepalive_retries(3)`）
- 效果：降低 retry 复用已死连接的概率

**TS 侧已有措施**（`catcher-http-ts/src/agent/shared-agent.ts`）：
- `freeSocketTimeout`: 35s 自动驱逐空闲连接
- socket error 自动驱逐：`agent.on('free')` 中监听并 `socket.destroy()`
- FIFO 调度避免连接囤积

**仍未解决**：
- 复用前主动 liveness probe（ping/timeout 检查）
- 最大连接复用次数限制
- retry 时强制新建连接（reqwest `Client` 不暴露 pool eviction API）
- 根治需要 G-06 E2E 基础设施修复后重跑验证

### A-01 优先级队列完整接入

在 Semaphore 并发控制基础上，进一步实现了优先级队列调度：

- `PriorityRequestQueue<T>` 通用双通道 biased select 实现（`scheduler/priority_queue.rs`）
- `HttpRequest.priority` 字段（`Priority` 枚举：Critical/High/Normal/Low/Background）
- `HttpTransport::new()` 中 `max_concurrency > 0` 时创建 `PriorityRequestQueue` 替代简单 semaphore
- `execute_with_token` 三路径：priority queue → semaphore fallback → 无并发控制
- napi-http 2 处 `HttpRequest` 构造已补 `priority: Normal`
- uniffi 5 处 `HttpRequest` 构造已补 `..Default::default()`
- FFI 层 5 处已有 `..Default::default()` 自动填充（无需改动）

### G2 rawData 补全

- TS `catcher-http-ts/src/http/error.ts` — `toRawData()` helper + `createCatcherError()` 填充 `rawData`
- `catcher-web/src/http/client.ts` — `parseBodyFromRaw()`，5xx 路径读 raw body，4xx+ 路径捕获 `rawData`

### 测试验证

- Rust: `cargo check --workspace --all-targets` ✅
- Rust tests: catcher-core 19 + catcher-http 105 + catcher-ffi 17 = **141 passed** ✅
- TS tests: 31 files, **323 passed**, 2 skipped ✅
- 已知 flaky: `catcher-ffi/tests/sse_test::s01_sse_stream_basic`（wiremock timing，与本次改动无关）
