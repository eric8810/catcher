# Bug: `networkPathId` 是死字段 —— 配置项被接受但从未被任何逻辑读取

**严重程度**: 🟢 Low — 无功能损害，但是误导性 API 表面：调用方以为设置它能触发网络变化处理，实则完全无效

**状态**: Fixed — 采用方案 A：从 Rust 配置、测试及全部绑定（TS / Dart）移除 `network_path_id` / `networkPathId`；网络切换恢复统一由 `networkChanged()` 承担

**影响包**: `catcher-http`、`catcher-ws`、全部绑定（napi / TS / Dart）

**位置**:
- `packages/catcher-http/src/types/http.rs:207`（定义）、`:239`（默认值）
- `packages/catcher-ws/src/types/ws.rs:239`（定义）、`:278`（默认值）
- 绑定：`catcher-core-ts/src/types.ts:154,297`、`catcher-napi-http/ts/types.ts:177`、`catcher-napi-ws/ts/types.ts:128`、`catcher_core/lib/src/http_client.dart:1109`、`catcher_core/lib/src/ws_client.dart:425`

---

## 现象

`HttpClientConfig` 和 `WsClientConfig` 都有 `network_path_id: Option<String>` 字段，并在所有绑定层暴露。字段语义暗示："宿主传入一个网络路径标识（如 `wifi` / `cellular` / `vpn-on`），当它变化时库自动重建连接、刷新 DNS"。

实际上：**catcher-http / catcher-ws 的运行逻辑中没有任何地方读取这个字段**。

```
$ grep -rn "network_path_id" packages --include="*.rs"
packages/catcher-http/src/types/http.rs:207:    pub network_path_id: Option<String>,   # 定义
packages/catcher-http/src/types/http.rs:239:            network_path_id: None,          # 默认
packages/catcher-ws/src/types/ws.rs:239:    pub network_path_id: Option<String>,     # 定义
packages/catcher-ws/src/types/ws.rs:278:            network_path_id: None,            # 默认
packages/catcher-ws/src/transport/ws_client.rs:1620:  ...network_path_id...               # 仅一个断言测试
```

设置它**不触发任何行为**：不重建连接池、不清 DNS、不重置熔断器。

## 根因

字段在 PR #14 引入，意图是承载"网络路径变化 → 库自动重建"的语义。但 PR #13 已经通过显式 API `networkChanged()` 实现了完整的网络切换恢复（清 DNS、热替换连接池、重置熔断器）。`networkChanged()` 是更明确、更可控的机制，于是 `network_path_id` 的预期职责被它完全覆盖，字段沦为没有读取方的"装饰"。

它给了 API 使用者"已经在处理网络变化"的**错觉**，而真正的开关是 `networkChanged()`。

## 修复方案与工作量

### 方案 A（推荐）：移除字段
从 core 配置结构、http/ws 配置、以及全部绑定类型中删除 `network_path_id` / `networkPathId`。

- **工作量**：中-小（纯机械删除，但跨 Rust + 3 套绑定 + 文档，触及面较广）。
- **破坏性**：
  - JSON 反序列化层**不破坏** —— 仓库未使用 `#[serde(deny_unknown_fields)]`（已确认），旧调用方继续传 `network_path_id` 会被静默忽略。
  - 强类型绑定（TS / Dart）层面是 **source-breaking**：显式构造该字段的代码需删除。但因字段是 optional 且无行为，实际使用者预计极少。
- **影响范围**：见下表。

### 方案 B：补上语义，让字段名副其实
在客户端持有"上次 network_path_id"，每次请求/重连前比较，若变化则内部调用 `network_changed()`。

- **工作量**：中（需在 HttpTransport/WsClient 增加状态 + 比较逻辑 + 测试）。
- **权衡**：与显式 `networkChanged()` API **功能冗余**，等于提供两套做同一件事的入口，增加维护面与语义歧义。除非明确想支持"声明式 path id"风格，否则不建议。

### 方案 C（最小）：保留但标注 reserved
在字段上加 `#[doc]` 注明"当前未实现，预留；如需网络切换恢复请调用 `networkChanged()`"。

- **工作量**：最小（仅文档注释）。
- **权衡**：零风险，但保留了一个长期没人实现的"占位字段"，技术债不消失。

## 推荐

**方案 A 移除**。理由：`networkChanged()` 已是更好的机制，保留一个永远不生效的同义字段只会持续误导调用方。若团队对未来"声明式 path id"有明确规划，则退而取**方案 C**（显式标注 reserved），但不要维持现状（既不实现也不标注）。

## 影响范围小结

| 维度 | 评估 |
|------|------|
| 是否大改 | 否 —— 删除/标注为主，无逻辑重写 |
| 跨语言绑定 | 是 —— Rust + TS(core/napi×2) + Dart(http/ws) 共 ~9 处 |
| 破坏性 | JSON 层不破坏（无 deny_unknown_fields）；强类型绑定 source-breaking（预计影响极小） |
| 运行时风险 | 无 —— 字段当前无任何行为 |

## 验证建议

- 方案 A：删除后 `cargo build --workspace`、`pnpm typecheck`、`dart analyze` 全绿；grep 确认无残留读取点。
- 删除 `ws_client.rs:1620` 附近引用该字段的断言测试。
- 文档（`docs/user-manual/resilience.md` 等）若提及该字段需同步移除。

## 关联

- [027-proxy-vpn-network-compatibility-research.md](./027-proxy-vpn-network-compatibility-research.md) — 字段引入背景
- `networkChanged()` API（PR #13）—— 真正承载网络切换恢复的机制，本字段的替代者
- [030-network-changed-inflight-requests.md](./030-network-changed-inflight-requests.md) — 同属 `networkChanged()` 语义范畴
