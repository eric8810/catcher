# Feature gap: WS 尚未实现 `pin_sha256` 证书固定（HTTP 已支持）

**类型**: 能力缺口（feature gap），**非 bug** —— WS 从未声称支持，代码 fail-closed 显式报错（不静默假装生效，无安全降级、无数据损坏）。归属与 `native-layer-capability-gaps.md` / `api-gap-features.md` 同类。

**优先级**: 🟡 P1–P2（取决于是否有 WS 上的证书固定真实需求）

**状态**: Open（未实现）

> 为何不是 bug：bug 是"承诺了 A 却做了 B / 误导"。WS 对 `pin_sha256` 既没承诺也没静默忽略，
> 而是明确拒绝并返回 `not supported yet`，行为诚实。它是"能力尚未实现"。
> 唯一带 bug 色彩的次要瑕疵：共享的 `TlsConfig` 类型暴露了该字段却无类型层标注 ——
> 属 API 一致性瑕疵，可由方案 A 顺带消除。

**影响包**: `catcher-ws`、`catcher-core`（共享类型）、绑定（napi-ws TS、Dart 共享 TlsConfig）

**位置**:
- `packages/catcher-ws/src/transport/ws_client.rs:296-304`（WS 硬报错）
- `packages/catcher-http/src/transport/tls.rs:26-34`（HTTP 入口分流）+ `tls.rs:110-176` `build_tls_with_pinning`
- `packages/catcher-http/src/transport/tls_pinning.rs`（`PinningVerifier` 实现，仅在 catcher-http crate）
- `packages/catcher-core/src/types/network.rs:60`（`pin_sha256` 字段，HTTP/WS 共享）
- 绑定暴露：`catcher-napi-ws/ts/types.ts:75`、`catcher-napi-http/ts/types.ts:58`、`catcher-core-ts/src/types.ts:72`、`catcher_core/lib/src/http_client.dart:979`（Dart `TlsConfig` 类，WS 复用同一类）

---

## 现象

`TlsConfig.pin_sha256`（SHA-256 公钥指纹固定，防 MITM）是 HTTP 与 WS **共享的同一个类型字段**。

- **HTTP**：完整实现。`build_tls_config` 检测到非空 `pin_sha256` 时分流到 `build_tls_with_pinning`，构建带 `PinningVerifier` 的 `rustls::ClientConfig` 并经 `use_preconfigured_tls` 注入。证书固定真实生效。
- **WS**：`build_reqwest_tls_config` 开头即拦截：

```rust
// ws_client.rs:296-304
if config
    .pin_sha256
    .as_ref()
    .is_some_and(|pins| !pins.is_empty())
{
    return Err(CatcherError::InvalidConfig(
        "ws tls.pin_sha256 is not supported yet".into(),
    ));
}
```

结果：用户用同一份 `TlsConfig` 配了证书固定，HTTP 客户端能建连，**WS 客户端在构建/建连那一刻直接失败**。该字段在 `napi-ws` 的 TS 类型、以及 Dart 共享的 `TlsConfig` 类上都暴露，用户没有任何静态提示表明 WS 不支持它，只能在运行时撞墙。

> 同类情况：`client_identity_pfx` 在 WS 也被拒绝（`ws_client.rs:347-352`），属于同一种"共享类型、能力分裂"的模式，可一并考虑。

## 根因

1. PR #14 把 WS 迁移到 reqwest 后，HTTP 与 WS 都基于 `reqwest + rustls`，但 pinning 的实现（`PinningVerifier` + `build_tls_with_pinning`）只落在 **catcher-http crate 内部**，没有抽取到共享层。
2. WS 不能反向依赖 catcher-http，于是无法复用该实现，只能先用一个 "not supported yet" 的占位报错挡住。
3. 类型却是共享的（`catcher-core::types::network::TlsConfig`），造成"类型承诺了 WS 运行时拒绝的能力"这一漏抽象（leaky abstraction）。

技术上 WS **完全有条件**支持 pinning：它也走 reqwest + rustls（`reqwest/rustls` feature），同样可以用 `use_preconfigured_tls(带 PinningVerifier 的 rustls config)`。唯一障碍是 `PinningVerifier` 的归属与依赖布局。

## 修复方案与工作量

### 方案 A（最低限度，小）：类型层 + 文档标注 WS 不支持
在 napi-ws TS 类型、Dart 共享 `TlsConfig`（或单独的 WS 文档段）明确标注 `pin_sha256` / `client_identity_pfx` 在 WS 上不支持，让用户在编码阶段就知道，而不是运行时才发现。

- **工作量**：小（注释 + 文档）。
- **影响范围**：无运行时改动。
- **权衡**：止血、消除"以为支持"的误解，但 WS 仍无证书固定能力。与 issue #028 的处理思路一致。

### 方案 B（彻底修，中）：抽取 pinning 到共享 crate，WS 真正支持
1. 把 `PinningVerifier`（+ `build_tls_with_pinning` 的核心逻辑）从 catcher-http 抽到共享 crate（`catcher-core` 或新建 `catcher-tls`）。
2. 给 catcher-ws 增加 `rustls` / `rustls-pki-types` / `sha2` / `webpki-roots` 直接依赖（目前只通过 `reqwest/rustls`、`yawc/rustls-ring` 间接引入）。
3. WS 的 `build_reqwest_tls_config` 在 `pin_sha256` 非空时走 `use_preconfigured_tls`，与 HTTP 对齐。
4. 补 WS pinning 单测（正确指纹放行 / 错误指纹拒绝）。

- **工作量**：中（共享抽取 + WS 接线 + 依赖调整 + 测试），非大重构。
- **影响范围**：catcher-http（迁出 PinningVerifier，保持对外行为不变）、catcher-ws（新增依赖与逻辑）、共享 crate。
- **权衡**：彻底消除类型不一致，WS 获得与 HTTP 对等的安全能力。

## 推荐

取决于是否有 **WS 上的证书固定真实需求**：
- 当前无需求 → 先做**方案 A**（标注 + 文档），把"运行时撞墙"降级为"编码期可见"。
- 有需求 → 直接做**方案 B**，顺带把 `client_identity_pfx` 的 WS 支持一并评估。

## 影响范围小结

| 维度 | 评估 |
|------|------|
| 是否大改 | 否 —— 方案 A 仅文档/注释；方案 B 为中等（共享抽取 + 接线） |
| 跨语言绑定 | 方案 A 触及 napi-ws TS / Dart 注释；方案 B 主要 Rust 侧 |
| 安全性质 | **fail-closed**：WS 不会"假装固定成功"，而是直接建连失败，不存在静默安全降级 |
| 触发条件 | 仅当用户实际为 WS 配置了非空 `pin_sha256`（或 `client_identity_pfx`） |

## 验证建议

- 方案 B：WS 单测 —— 正确公钥指纹的本地 wss server 可连；篡改指纹后建连被拒。
- `cargo clippy --workspace --all-targets -- -D warnings`、`pnpm typecheck`。
- 回归：确认 catcher-http 的现有 pinning 测试（迁出 PinningVerifier 后）仍通过。

## 关联

- [025-ws-missing-tls12-feature.md](./025-ws-missing-tls12-feature.md) — WS TLS 配置类历史问题
- [028-tls-sni-override-noop-rust-path.md](./028-tls-sni-override-noop-rust-path.md) — 同属"共享 TlsConfig 类型、原生 transport 能力不全"
- PR #14 review MAJOR M2（"WS TLS pinning 与 HTTP 分裂"）—— 本 issue 即该发现的独立化
- `catcher-http/src/transport/tls_pinning.rs` — 可被抽取复用的 `PinningVerifier`
