# Bug: `networkChanged()` 不恢复在途请求，且注释「飞行中的请求不受影响」言过其实

**严重程度**: 🟡 Medium — 注释误导 + 功能缺口：网络切换后正在进行的长请求/流式下载仍会卡在旧半开连接上直到自身超时

**状态**: Fixed — 修正 `http_client.rs` 误导注释，并在 `resilience.md` 补充 `networkChanged()` + `cancelAll()` 组合用法（方案 A）；未引入 API 变化

**影响包**: `catcher-http`（核心）；`catcher-ws` 行为类似需一并评估

**位置**:
- `packages/catcher-http/src/transport/http_client.rs:140`（误导注释）
- `packages/catcher-http/src/transport/http_client.rs:131-167`（`network_changed` 实现）
- `packages/catcher-http/src/transport/http_client.rs:497`（`execute_stream`，已支持 `CancellationToken`）
- `packages/catcher-http/src/transport/http_client.rs:425`（`cancel_all`，已存在）

---

## 现象

`networkChanged()` 通过热替换 `RwLock<Client>` 丢弃整个旧连接池。这只让**之后发起的新请求**走新连接，**对调用时正在进行中的请求无效**：

- 一个正在 `execute_stream` 流式下载的请求，已捕获旧 client 的连接（`http_client.rs:124` 的 `client()` 克隆在请求开始时取一次），会继续阻塞在网络切换后变成半开的 socket 上，直到它自己的 `response_timeout_ms` 超时。
- 非流式但耗时较长的请求同理。

而代码注释却写：

```rust
// http_client.rs:140
/// 飞行中的请求不受影响：其内部重试会自动建立新连接。
```

这句只在**配了 `retry` 且失败可重试**时部分成立 —— 流式下载、未配重试、或正卡在读取阶段的请求都不会自动恢复。注释把"无能为力"描述成了"不受影响"，会让使用者误以为 `networkChanged()` 已覆盖在途请求。

## 根因

连接池热替换是"换池子"，不是"中断正在用旧池子的人"。在途请求持有旧 `ClientWithMiddleware` 的克隆（这本身是正确的设计 —— 避免请求中途被抽走连接），因此不受新池子影响，也就得不到主动恢复。库目前**没有把已有的取消能力接入 `network_changed()`**。

值得注意：恢复在途请求所需的原材料**已经齐全**：
- `execute_stream` 已接受 `CancellationToken`（`:497`）
- `cancel_all()` 已实现，可取消所有 `pending_requests`（`:425`）

只是 `network_changed()` 没有调用它们。

## 修复方案与工作量

### 必做（tiny）：修正注释
把 `:140` 的注释改为如实描述：连接池热替换只影响新请求；在途请求（尤其流式下载）不会被主动中断，需调用方配合 `cancelAll()` 或等待其自身超时。

- **工作量**：极小（1 处注释 + 同步文档 `resilience.md`）。

### 方案 A（推荐，小）：文档化组合用法
不改 API，在用户手册中明确：「若希望网络切换时连同在途请求一起恢复，请调用 `networkChanged()` 后再调用 `cancelAll()`（在途请求会失败/重试，由上层决定是否重发）」。

- **工作量**：小（纯文档）。
- **影响范围**：零代码风险，零 API 变化。
- **权衡**：把决策权留给调用方（取消在途请求是有副作用的行为，不应默认发生）。

### 方案 B（小-中）：给 `network_changed` 增加可选的在途取消
新增带参签名，例如 `network_changed(cancel_inflight: bool)`（默认 `false`），为 `true` 时在重建连接池后调用 `cancel_all()`。

- **工作量**：小-中（Rust 侧复用现成 `cancel_all`；但参数要贯通 C ABI / napi / UniFFI / Dart 四套绑定）。
- **影响范围**：API 签名变化 → 跨全部绑定（见 PR #13 中 `networkChanged()` 的暴露面）。需保持向后兼容（默认 false / 重载）。
- **权衡**：把"是否取消在途"做成显式开关，比让调用方手动两步调用更内聚，但扩大了绑定改动面。

## 推荐

**必做修注释 + 方案 A 文档化**先行（零风险，立即消除误导）。若后续有客户明确反馈"网络切换时希望一键连在途请求一起恢复"，再上**方案 B**（默认 `false`，避免改变现有语义）。

## 影响范围小结

| 维度 | 评估 |
|------|------|
| 是否大改 | 否 —— 修注释/文档为主；方案 B 也只是复用现成 `cancel_all` |
| 跨语言绑定 | 修注释/文档：无；方案 B：是（4 套绑定签名） |
| 破坏性 | 方案 A 无；方案 B 用默认参数可保持兼容 |
| WS 是否同样问题 | 需评估 —— WS 的 `network_changed()` 立即丢连接并重连，半开 send 已被 `send_timeout_ms` 兜底，缺口比 HTTP 小，但长 streaming 接收同样值得检查 |

## 验证建议

- 复现：发起一个慢响应/流式请求 → 中途调用 `networkChanged()` → 断言该请求仍在等待（证明缺口），调用 `cancelAll()` 后立即结束（证明组合有效）。
- 方案 B：单测断言 `network_changed(true)` 后在途请求被取消、新请求正常（可参考 `ns03_cancel_all_then_new_requests_work`，`http_client.rs:1320`）。

## 关联

- PR #13 `networkChanged()` 实现
- `http_client.rs:425` `cancel_all` / `:446` `cancel_request`（N-03 单请求取消能力）
- [017-ffi-stream-cancel-broken.md](./017-ffi-stream-cancel-broken.md) — 流式取消相关历史问题
- [029-network-path-id-dead-field.md](./029-network-path-id-dead-field.md)
