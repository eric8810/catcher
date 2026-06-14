# Bug: `tlsSniOverride` 在 Rust/native 路径静默失效，且与纯 TS 路径行为不一致

**严重程度**: 🟡 Medium — 配置项被接受但 Rust 路径完全不生效（静默错误行为）；与 TS 路径行为分裂，安全/MITM/域前置场景下会误导调用方

**状态**: Fixed — 采用方案 A：原生 transport 在设置 `tls_sni_override` 时显式返回 `InvalidConfig`，并更新过时的 reqwest 注释

**影响包**: `catcher-http`、`catcher-ws`、`catcher-core`（类型）、全部绑定（napi / TS / Dart / UniFFI）

**位置**:
- `packages/catcher-http/src/transport/tls.rs:101-105`（非 pinning 路径）
- `packages/catcher-http/src/transport/tls.rs:167-171`（pinning 路径）
- `packages/catcher-ws/src/transport/ws_client.rs:361-363`
- `packages/catcher-core/src/types/network.rs:50`（字段定义）
- `packages/catcher-http-ts/src/agent/shared-agent.ts:147-148`（TS 路径，**生效**）

---

## 现象

调用方设置 `TlsConfig.tls_sni_override = "custom-sni.example.com"`，期望 TLS 握手时发送的 SNI 主机名被覆写为该值（域前置、MITM 代理、证书域名与连接域名不一致等场景的核心用途）。

- **Rust / native 路径（catcher-http、catcher-ws）**：该值被**完全忽略**，SNI 仍然取自 URL 的 host。
- **纯 TS 路径（catcher-http-ts 的 Node Agent）**：该值**生效**（`tlsAgentOpts.servername = tls.tlsSniOverride`）。

同一个配置字段在两条 transport 上行为不同，且 Rust 路径无任何报错或日志 —— 调用方无法察觉它没生效。

## 根因

reqwest 0.13 的 `ClientBuilder::tls_sni(bool)` 只控制**是否发送 SNI 扩展**（enable/disable），**不接受主机名**。当前代码：

```rust
// tls.rs:101-105 — 非 pinning 路径
// NOTE: reqwest 0.12 tls_sni() takes a bool (enable/disable SNI), not a hostname.
if let Some(ref _sni) = config.tls_sni_override {   // 注意：变量名是 _sni —— 值被丢弃
    builder = builder.tls_sni(true);                 // 而 SNI 本来就默认开启，此调用近乎 no-op
}
```

```rust
// tls.rs:167-171 — pinning 路径
if config.tls_sni_override.is_some() {
    tls_config.enable_sni = true;                    // 同样只是开关，不设主机名
}
```

```rust
// ws_client.rs:361-363 — WS 路径，同样问题
if config.tls_sni_override.is_some() {
    builder = builder.tls_sni(true);
}
```

注释里还残留 `reqwest 0.12` 字样，但仓库实际已升级到 **reqwest 0.13**（`catcher-http/Cargo.toml:30`、`catcher-ws/Cargo.toml:29`），注释过时。

reqwest 0.13 没有提供"用与 URL host 不同的主机名做 SNI"的直接 API —— SNI 的 `ServerName` 在连接时由 URL authority 决定，`ClientConfig` 层无法覆写单连接的 ServerName。

## 修复方案与工作量

### 方案 A（推荐，小）：显式拒绝，消除静默失效
在 Rust 路径构建时，若 `tls_sni_override.is_some()` 直接返回 `CatcherError::InvalidConfig("tls_sni_override is not supported on the native transport")`。

- **工作量**：小（~3 处各 2-3 行 + 单测）。
- **影响范围**：不改类型、不改绑定签名。行为变化 —— 之前静默忽略的调用方现在会在构建客户端时收到明确错误，便于及早发现。
- **权衡**：把"假装支持"换成"诚实不支持"，是这 4 个问题里性价比最高的。

### 方案 B（中）：用 resolve + host 改写模拟覆写
把请求 URL 的 host 改成 SNI 主机名，再用 `reqwest::ClientBuilder::resolve(sni_host, real_addr)` 把它指向真实目标 IP（域前置式）。

- **工作量**：中。
- **影响范围**：会**连带改变 `Host` 请求头和证书校验对象**，语义与"仅覆写 SNI"不同，不通用；对 WS 同样要改 URL，复杂度高。不推荐作为通用实现。

### 方案 C（大）：自定义 rustls Connector
绕过 reqwest 的连接层（或直接基于 hyper + 自定义 `tokio-rustls` connector），在建立 TLS 时显式传入自定义 `ServerName`。

- **工作量**：大（需重写 HTTP 与 WS 的连接建立路径，且与现有 pinning/代理/DNS 注入逻辑交织）。
- **影响范围**：连接层重构，回归风险高。仅当确有强需求（如必须支持 SNI 与 Host 分离的企业代理）才值得。

## 推荐

先做**方案 A**（止血，让行为诚实），并把 TS 路径的能力差异写进文档。是否需要方案 B/C 取决于是否有真实客户场景要求 SNI 与 URL host 分离 —— 目前无证据，暂不投入。

## 影响范围小结

| 维度 | 评估 |
|------|------|
| 是否大改 | 方案 A 否（局部）；方案 C 是（连接层重构） |
| 跨语言绑定 | 类型字段已在所有绑定暴露；方案 A 不动绑定，仅 Rust 侧校验 |
| 破坏性 | 方案 A 是行为变化（静默→报错），非编译破坏 |
| 数据/安全相关 | 是 —— SNI 覆写常用于证书校验与流量分流，静默失效可能导致错误的安全预期 |

## 验证建议

- 单测：构造带 `tls_sni_override` 的配置，断言方案 A 下 `HttpTransport::new` / WS 客户端构建返回 `InvalidConfig`。
- 若实现真实覆写：用本地 TLS server 抓 ClientHello，断言 SNI 字段等于覆写值（而非 URL host）。
- 跑 `cargo clippy --workspace --all-targets -- -D warnings`、`pnpm typecheck`。

## 关联

- `packages/catcher-http-ts/src/agent/shared-agent.ts:147-148` — TS 路径正确实现（`servername`），可作为行为对齐参考
- [025-ws-missing-tls12-feature.md](./025-ws-missing-tls12-feature.md) — 同属 TLS 配置类问题
- reqwest `tls_sni` 文档：https://docs.rs/reqwest/0.13/reqwest/struct.ClientBuilder.html#method.tls_sni
