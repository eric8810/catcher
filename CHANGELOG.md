# Changelog

All notable changes to this project will be documented in this file. See [release-please](https://github.com/googleapis/release-please) for automated management.

## 0.3.19 (2026-08-13)

> 修复 N-API HTTP 传输异常被压缩成 `GenericFailure`、导致上层无法识别失败阶段和最终重试原因的问题。

### 🐛 Bug Fixes

- **结构化传输异常**：连接拒绝、DNS、TLS、连接超时、请求超时及其他传输失败均提供稳定的 `code`、`phase`、`retryable` 和白名单 `details`。
- **保留重试根因**：`RETRY_EXHAUSTED` 记录实际执行总次数，并在 `details.lastError` 中保留最终结构化异常，不再只保存字符串。
- **N-API 错误类型**：JavaScript 调用方收到导出的 `CatcherError` / `HttpError`，同时保留旧版 HTTP 文本错误兼容解析。
- **安全诊断信息**：原生错误链序列化前移除请求 URL，避免查询参数和 token 泄漏。
- **跨平台契约覆盖**：CI 在 Linux、macOS 和 Windows 上验证非法配置、连接拒绝、重试耗尽和请求超时契约。

### 📦 Packaging

- 所有 npm、Rust、NAPI 和 Flutter 包统一同步到 `0.3.19`。

## 0.3.18 (2026-08-07)

> 修复代理/VPN/网络路径变化后服务端返回 HTTP 421（Misdirected Request）时客户端持续失败的问题，并为 NAPI 调用方提供结构化 HTTP 状态错误。

### 🐛 Bug Fixes

- **HTTP 421 自动恢复**：`catcher-http` 收到 421 后仅重建当前 transport 的连接池，并在新连接上重试一次；持续返回 421 时停止重试，避免无限循环。
- **保留在途请求**：421 恢复不会取消同一 transport 上其他正在执行的请求，也不会影响其他 `HttpTransport` 实例。
- **NAPI 结构化状态错误**：`@eric8810/catcher-napi-http` 将原生 HTTP 状态错误规范化为导出的 `HttpError`，上层可直接读取 `status`、`body` 和 `cause`。
- **跨层回归覆盖**：新增 Rust 与 NAPI 集成测试，覆盖 POST 421 后成功恢复、持续 421 只重试一次、在途请求不被取消，以及结构化状态字段。

### 📦 Packaging

- 所有 npm、Rust、NAPI 和 Flutter 包统一同步到 `0.3.18`。

## 0.3.17 (2026-06-15)

> 修复 Flutter WebSocket 直连可用性：默认简单直连场景改走 yawc native backend，高级网络配置继续走 reqwest backend；同时完善网络切换重连、Apple framework 元数据与 Flutter 兼容性处理。

### 🐛 Bug Fixes

- **Flutter WebSocket 直连恢复**：无 proxy / Catcher DNS / 高级 TLS 的默认场景改走 yawc native backend，避免 reqwest backend 在 Flutter 场景下的 socket 连接问题；需要 proxy、Catcher DNS 或高级 TLS 时仍走 reqwest backend。
- **WebSocket 网络切换恢复**：`network_changed()` 会立即丢弃半开连接、重置退避并重连；多端点配置下会重新竞速全部端点，退避期间缓存的发送命令在重连成功后重放。
- **Dart WebSocket 旧动态库兼容**：`catcher_ws_network_changed` 符号不存在时延迟到调用 `networkChanged()` 再报错，避免旧 native library 在客户端构造阶段直接失败。
- **Flutter Apple framework 元数据**：生成 iOS/macOS `catcher_ffi.xcframework` 时写入最低系统版本，并将 podspec 版本同步到本次发布。

### ⚠️ Behavior Changes

- **Rust `EndpointRacer` 内部化**：多端点竞速改为 `WsTransport::connect()` 的内部实现细节，不再作为 `catcher_ws::EndpointRacer` public API 暴露；Rust 调用方应通过 `WsClientConfig.urls` / `race_count` 使用多端点能力。

## 0.3.16 (2026-06-14)

> 修复 napi 绑定：0.3.15 的 `proxy.mode = "system"` 在发布的 npm 包中静默失效——两个 napi 绑定都漏启用了 `system-proxy` cargo feature，导致 `detect_system_proxy()` 编译为空操作 stub。同时同步了 napi TypeScript 类型（#19）。

### 🐛 Bug Fixes

- **napi `system-proxy` feature 未启用**：0.3.15 (#18) 引入了系统代理自动检测，但 `catcher-napi-http` / `catcher-napi-ws` 都未在依赖上启用 `system-proxy` cargo feature，导致 `detect_system_proxy()` 编译为 no-op stub——`proxy.mode = "system"` 在发布的 npm 包中静默不生效。现已为两个 napi crate 的依赖启用该 feature。
- **napi TypeScript 类型同步**：发布到 npm 的 `dist/types.d.ts` 仍是 0.3.15 之前的旧类型（`url` 必填、无 `mode` 字段）。现将 `ProxyConfig` 建模为判别联合（discriminated union）：
  - `ManualProxyConfig`（默认，省略 `mode` 或 `mode: 'manual'`）：`url` 必填。
  - `SystemProxyConfig`（`mode: 'system'`）：`url` 可省略，自动从 OS 检测。
  - 收紧了类型——`{ mode: 'manual' }` 或 `{ no_proxy: [...] }`（省略 `url`）不再通过类型检查（此前会进入 `transport_url()` 的 `url: None` panic）。

## 0.3.15 (2026-06-13)

> 系统代理自动检测：`proxy.mode = "system"`，跳过 0.3.14。
> **升级前请阅读「⚠️ 行为变更」** — `ProxyConfig.url` 从 `String` 变为 `Option<String>`。
> 📄 完整设计文档见 [`docs/plan/2026-06-13-system-proxy-detection.md`](./docs/plan/2026-06-13-system-proxy-detection.md)。

### ✨ Features

- **系统代理自动检测 `proxy.mode = "system"`**：调用方无需手动传入代理 URL，catcher 自动从 OS 读取系统代理配置。支持 macOS (SystemConfiguration)、Windows (WinINET 注册表 + WinHTTP)、Linux (环境变量 + /etc/sysconfig/proxy)。
  - `ProxyMode` enum：`Manual`（默认，向后兼容）/ `System`（自动检测）
  - `ProxyConfig.mode` 字段新增，`url` 改为 `Option<String>`
  - 共享实现位于 `catcher-dns/src/proxy.rs`，通过 `proxy-cfg` crate 检测
  - `networkChanged()` 时自动重新检测系统代理，变化后重建 reqwest client
- **HTTP/WS 统一支持 System 代理模式**：`build_middleware_client()` 和 `build_reqwest_client()` 在 `mode=System && url=None` 时安全跳过代理（退化直连），`networkChanged()` 后重检并应用新代理。

### ⚠️ Behavior Changes

- **`ProxyConfig.url`: `String` → `Option<String>`**：预 1.0 版本的 breaking change。旧 JSON `{"url":"..."}` 仍然可反序列化（`#[serde(default)]` 保证 `mode` 默认 `Manual`）。`transport_url()` 在 `url=None` 时会 panic — 调用方须确保 System 模式在 `detect_system_proxy()` 后 url 已被填充。

### 🔄 Internal

- `detect_system_proxy()` 移至 `catcher-dns` 共享，消除 `catcher-http` / `catcher-ws` 代码重复。
- WS `networkChanged()` 非 System 模式下消除多余 config clone。

## 0.3.13 (2026-06-13)

> 网络韧性大版本：主动网络切换恢复、移动端代理/VPN 兼容（含远端 DNS）、以及一组配置行为的对齐。
> **升级前请阅读「⚠️ 行为变更」** —— 多数变更对默认配置无影响，但 DNS、代理、TLS 几处默认行为有调整。
> 📄 完整发版说明（含 API 差距、升级清单）见 [`docs/releases/0.3.13.md`](./docs/releases/0.3.13.md)；
> 简明迁移表见 [`docs/user-manual/migration.md`](./docs/user-manual/migration.md) 的「版本升级：0.3.x → 0.3.13」一节。

### ✨ Features

- **主动网络切换恢复 `networkChanged()`**：网络环境切换（WiFi↔蜂窝、VPN 连接/断开/换节点）后，旧连接会静默变成半开连接。宿主 App 从 OS 拿到网络变化信号后调用 `networkChanged()`，立即完成恢复，不再等被动超时（WS 心跳 10–30s / TCP keepalive 20s / DNS 缓存最长 300s）。
  - **HTTP**（`HttpTransport::network_changed()` / `client.networkChanged()`）：清空 DNS 缓存并重建解析器、热替换底层客户端丢弃整个旧连接池、重置熔断器。
  - **WebSocket**（`WsHandle::network_changed()` / `ws.networkChanged()`）：立即丢弃半开连接（不发 Close 帧）、清 DNS、重置退避并立即重连、多端点重新竞速。
  - **catcher-dns**：新增 `DnsResolver::clear_cache()`，并在网络切换时重建 resolver 以读取新网络（如 VPN 下发）的 nameserver。
  - 全平台绑定：C ABI（`catcher_ws_network_changed` / `catcher_http_network_changed`）、napi、UniFFI（Kotlin/Swift）、Dart。
- **移动端显式代理 / VPN 兼容**：HTTP 与 WebSocket 现在共享 `catcher-core` 的 `ProxyConfig` / `TlsConfig`，一致支持 HTTP 代理、SOCKS5/SOCKS5h、HTTP CONNECT，以及 `no_proxy` 旁路。启用 `reqwest/socks` feature。
- **WebSocket 迁移到 reqwest 传输层**：使 WS 获得与 HTTP 一致的代理 / TLS / DNS 能力；新增 `WsClientConfig.proxy`、`WsClientConfig.tls`、`WsClientConfig.send_timeout_ms`（默认 10000ms，防半开发送阻塞事件循环）。

### ⚠️ Behavior Changes（升级敏感，请逐条确认）

- **DNS 改为按需启用（opt-in）**：此前 HTTP/WS **总是**构建 Catcher DNS 解析器（即使未配置 `dns`）。现在 **不配置 `dns` 即使用协议库原生解析**；仅当提供 `dns` 配置（`mode` 默认 `catcher`）或显式 `dns.mode = "catcher"` 时才启用 Catcher DNS（缓存 / 旧缓存兜底 / host mapping / 自定义 nameserver）。**影响**：此前依赖隐式 DNS 缓存的调用方，升级后若未显式配置 `dns` 将失去缓存。
- **不再静默回退到公共 DNS**：此前读取系统 DNS 配置失败时会静默回退到 hickory 默认（公共）nameserver（如 `8.8.8.8`）。现在会返回 `DnsError`，除非显式设置新增字段 `fallback_to_default_nameservers = true`（默认 `false`）。**影响**：移动端/受限网络下不再意外把 DNS 查询发往公共服务器。
- **`socks5://` 自动按 `socks5h://` 处理**：代理路径下目标域名交给代理远端解析，避免本地提前解析成 IP 破坏 Clash fake-ip / VPN 分流。**影响**：无法再通过 `socks5://` 强制本地解析（这是有意的修复）。
- **`tls_sni_override` 在原生 transport 改为显式报错**：此前在 Rust（catcher-http / catcher-ws）路径被**静默忽略**（reqwest 无法覆写 SNI 主机名）。现在设置该字段会在构建客户端时返回 `InvalidConfig`，不再假装生效。纯 TS 的 Node Agent 路径（`@eric8810/catcher-http` 的 `servername`）仍支持。
- **WebSocket 单连接多 IP 握手故障转移已移除**：随 WS 迁移到 reqwest，按 A 记录逐个 IP 重试握手的逻辑（`connect_with_resolved_addrs`）被移除，改由 reqwest 的连接层（happy-eyeballs）处理。**影响**：多 IP 主机在握手层的逐 IP 重试能力减弱（多端点竞速 `urls: [...]` 不受影响）。
- **WebSocket FFI 销毁语义修正**：`catcher_ws_destroy` 现在会取消事件循环并关闭连接，而非仅从注册表移除句柄。修复了销毁后事件循环仍运行、仍回调宿主 `user_data` 的 use-after-free 风险。

### 🐛 Bug Fixes

- **FFI 句柄 use-after-free**：句柄中直接编码 registry id，消除回收复用导致的 use-after-free。
- **网络切换期间的失败不再惩罚新网络**：在途请求启动时快照「网络代际」，切换后失败不计入熔断器（network-generation gating）。
- **DNS 缓存清空后不被旧网络结果回填**：`clear_cache()` 后，后台 stale 刷新与前台解析都按代际拒绝写入旧结果。
- **network-quality 取消订阅泄漏**：`unsubscribe()` 现在会 `abort()` 后台任务，而非仅发送取消信号。
- **WS 重连命令重放遵守 `send_timeout_ms`**：重放缓冲命令时不会因半开 sink 卡死事件循环。

### 🔄 Internal / 无公开 API 影响

- 本周期内曾引入又移除了 `network_path_id` / `networkPathId` 配置字段（先加于代理 PR、后移除）——**对外 API 净零变化**，因为它从未随正式版本发布。其职责由 `networkChanged()` 承担。
- 共享类型 `ProxyConfig` / `ProxyAuth` / `TlsConfig` / `TlsVersion` 上移至 `catcher-core` 并在 `catcher-http` / `catcher-ws` 重导出：Rust 调用方经 `catcher_http::types::http::*` 的导入路径**保持源码兼容**。
- 配置 JSON 反序列化未启用 `deny_unknown_fields`：传入未知/已移除字段会被静默忽略，故绑定层 JSON 调用方不会因字段移除而中断。

### 📝 Documentation

- 新增 issues #028–#031（已修复）与 #032（feature gap：WS 尚未实现 `pin_sha256` 证书固定）。
- `docs/user-manual/resilience.md` 第七节：`networkChanged()` 各平台用法、防抖建议，以及「在途请求不会被自动恢复，需配合 `cancelAll()`」说明。
- `docs/arch-rs/04-transport.md`：代理 × DNS 契约，及「代理路径目标域名不本地解析」对 reqwest 内部行为的依赖（升级 reqwest 必须重跑 `proxy_dns_behavior_test`）。

## 0.3.12 (2026-06-08)

### 🐛 Bug Fixes

- **iOS App Store**: Set `MinimumOSVersion` 15.0 in the bundled iOS `catcher_ffi.framework` Info.plist for App Store upload validation.

### 📝 Documentation / Packaging

- Add third-party license declaration for `yawc` (MPL-2.0).
- Rebuild bundled Apple native frameworks with updated minimum OS version.

## 0.3.11 (2026-06-01)

### ✨ Features

- **WebSocket yawc transport**: Migrated the Rust WebSocket client transport from `tokio-tungstenite` to `yawc`, enabling native permessage-deflate negotiation while preserving DNS failover, reconnect, heartbeat, custom headers, subprotocols, and application-level compression fallback.
- **Application-layer compression**: Added `application_compression` config to `WsClientConfig` with gzip and zstd support. When permessage-deflate is unavailable, messages above a configurable threshold are automatically compressed with a catcher-specific envelope so receiving servers can detect and decompress them. Negotiation headers (`X-Catcher-Application-Compression`) are sent during handshake.

### 🐛 Bug Fixes

- **Android native builds**: Export Android NDK `CC_*` and `AR_*` variables so native dependencies with C build scripts use the correct cross-compilation toolchain.
- **Reconnect message buffering**: Commands sent during reconnect are now drained and replayed after a successful reconnection instead of being silently dropped.
- **pong_timeout fast detection**: `HeartbeatManager::is_timed_out()` now detects single-pong timeout expiry in addition to `is_missed_pongs_exceeded()`, providing faster dead-connection detection within a single heartbeat cycle.
- **Close frame echo**: Receiving a Close frame now correctly echoes a Close frame back before disconnecting (RFC 6455 §5.5.1).
- **Reconnect latency measurement**: The `Connected` event now carries the actual latency of each successful reconnection instead of reporting 0 ms.
- **native-tls removal**: Dropped the `native-tls` feature and removed `rustls` as a direct dependency; `yawc/rustls-ring` handles all TLS provisioning.

### 🔄 Dependencies

- Replaced runtime `tokio-tungstenite` usage in `catcher-ws` with `yawc`; `tokio-tungstenite` remains only as a test dependency for local WebSocket server fixtures.

## 0.3.10 (2026-05-20)

### 📝 Documentation / Packaging

- Bump all packages to `0.3.10` so npm, crates.io, pub.dev, and native NAPI packages can be rebuilt and published from one clean release run.
- Repackage the NAPI platform packages and main NAPI packages under a fresh patch version after the `0.3.9` npm publish was blocked by npm token authentication.

## 0.3.9 (2026-05-20)

### ✨ Features

- **Dart DNS cache controls**: `DnsConfig` now exposes `cacheSize`, `negativeTtlSecs`, `staleTtlSecs`, and `staleOnError` so Flutter clients can configure the native DNS cache and stale fallback behavior.
- **Dart transport MessagePack switch**: `HttpClientConfig.msgpack` and `WsClientConfig.msgpack` enable native JSON ↔ MessagePack conversion for HTTP bodies and WebSocket messages.
- **Dart WebSocket DNS config**: `WsClientConfig.dns` passes DNS cache, nameserver, and host mapping settings into the native WebSocket client.
- **NAPI DNS and MessagePack options**: NAPI HTTP/WS configs now include the expanded DNS fields and `msgpack`; NAPI WS also exposes native `pack()` / `unpack()` helpers.
- **Shared Rust DNS crate**: Added `catcher-dns` so HTTP and WebSocket share DNS config, cache, host mapping, and stale fallback behavior without depending on each other.

### 🐛 Bug Fixes

- Fixed DNS cache config not being applied through Dart, FFI, and NAPI layers.
- Fixed built-in MessagePack config not being wired through Dart, FFI, and NAPI clients.
- Fixed WebSocket DNS failover so a TLS or WebSocket handshake failure can retry the next resolved IP address.

### 📝 Documentation / Packaging

- Documented Dart and NAPI DNS / MessagePack config as new 0.3.9 features.
- Reduced the Flutter `catcher_core` pub.dev package size so the bundled native libraries stay below pub.dev limits.

## 0.3.8 (2026-05-19)

### ✨ Features

- **Flutter FFI package**: Published `catcher_core` as a Flutter FFI plugin with Android, iOS, macOS, Linux, and Windows native bundle metadata.

### 📝 Documentation / Packaging

- Bundled prebuilt native libraries for `catcher_core` during pub.dev release.
- Updated Flutter installation and native library loading guidance.

## 0.3.7 (2026-05-18)

### 🐛 Bug Fixes

- **Windows native addon loading**: `native.ts` ABI suffix detection returned empty string on Windows, causing file lookup for `catcher-napi-http.win32-x64.node` instead of the correct `catcher-napi-http.win32-x64-msvc.node`. Added proper ABI detection: `msvc` for Windows, `gnu`/`musl` for Linux.

### ✨ Features

- **8-platform native addon support**: Expanded from 5 to 8 build targets — added `linux-arm64-gnu`, `linux-arm64-musl`, `win32-arm64-msvc`. ARM64 Linux targets use zig cross-compilation.
- **Platform sub-package distribution**: Native addons now publish as separate per-platform `optionalDependencies` packages (e.g., `@eric8810/catcher-napi-http-win32-x64-msvc`). Main package no longer bundles all `.node` files, reducing install size from ~56MB to ~10MB per platform.
- **Release binary size optimization**: Added `[profile.release]` with `lto = true`, `codegen-units = 1`, `strip = "symbols"` to reduce `.node` file sizes.

## 0.3.6 (2026-07-20)

### 🐛 Bug Fixes

- **catcher-ws missing TLS 1.2 support**: `catcher-ws` used `default-features = false` for rustls but did not enable the `tls12` feature. This caused `HandshakeFailure` on servers that only support TLS 1.2 (e.g. `ws-gateway.fazhiplus.com`). Added `features = ["tls12"]` to the rustls dependency, bringing 6 TLS 1.2 cipher suites alongside the 3 TLS 1.3 suites.

## 0.3.5 (2026-07-20)

### 🐛 Bug Fixes

- **rustls CryptoProvider panic on wss:// (critical)**: `catcher-ws` enabled `tokio-tungstenite/rustls-tls` but never installed a `CryptoProvider`. Since rustls 0.23, this must be done explicitly. Added `ensure_tls_provider()` with `OnceLock`-guarded `ring::default_provider()` initialization in `connect_stream()`. Fixes runtime panic when connecting to any `wss://` endpoint.
- **catcher-http tls_pinning test failure**: `make_mock_verifier()` called `WebPkiServerVerifier::builder().build()` without a `CryptoProvider` installed. Added `OnceLock`-guarded `aws_lc_rs::default_provider()` initialization in the test module.

## 0.3.3 (2026-07-20)

### 🐛 Bug Fixes

- **Linux native addon loading (critical)**: `native.ts` used `platform-arch` format (e.g., `linux-x64`) but napi prepublish generates files with full triple names (e.g., `linux-x64-gnu`). This caused all Linux users to get "native addon not found" errors. Fixed by adding libc suffix detection and multiple fallback path patterns.

## 0.3.2 (2026-07-20)

### 🐛 Bug Fixes

- **napi-http TS types (GitHub #3)**: Hand-written TypeScript types used `snake_case` but NAPI-RS auto-generates `camelCase`. Replaced hand-written types with re-exports from auto-generated `index.d.ts`. JSON config types (serde-based) correctly retain `snake_case` with `camelCase` alias support.
- **napi-ws TLS missing (GitHub #4)**: `catcher-ws` did not enable any TLS feature for `tokio-tungstenite`, causing all `wss://` connections to fail immediately. Added `rustls-tls` (webpki-roots) as default feature, consistent with `catcher-http`.
- **`catcher_core/rust` workspace isolation**: Added empty `[workspace]` table to `catcher_core/rust/Cargo.toml` to prevent it from incorrectly inheriting the monorepo workspace.
- **`normalizeOptions` cleanup**: Rewritten to build clean option objects instead of spreading raw input (which leaked stale snake_case properties).

## 0.3.1 (2026-05-18)

### 🐛 Bug Fixes

- Republish all packages from correct commit (v0.3.0 napi/crates.io packages were from stale tag).
- Clippy: ~40 fixes including redundant closures, auto-deref, saturating_sub, div_ceil, slice::from_ref, etc.
- FFI: `catcher_free_event_data` made `pub unsafe extern "C"` for correct visibility.
- FFI: Added safety documentation comments to all unsafe functions.
- `catcher_core/rust/Cargo.toml` version sync (was stuck at 0.2.2).
- Test files: wrapped unsafe FFI calls in `unsafe {}` blocks.

## 0.3.0 (2026-07)

### ⚠️ Breaking Changes

- **napi packages**: Entry point changed from `client.js` → `dist/client.js`. Config now accepts typed objects (not just JSON strings). Class names renamed: `JsHttpClient` → `HttpClient`, `JsWsClient` → `WsClient`. Callback events are now typed objects (auto-parsed) instead of JSON strings. WS message data uses `event.data_base64` (base64).
- **Rust crates**: `BackoffKind::default()` changed from `Exponential` → `Fixed`. WS config fields renamed: `deflate_threshold` → `deflate_threshold_bytes`, `max_message_size` → `max_payload_bytes`, `ping_timeout_ms` → `pong_timeout_ms`. All config structs now support `snake_case` + `camelCase` via `#[serde(alias)]`.
- **reqwest 0.13 + tungstenite 0.29**: Dependency upgrade; may affect custom TLS configurations.

### 🚀 New Features

- **Typed napi TS wrappers**: Auto-generated TypeScript sources replace hand-written wrappers. Full type safety for config, events, and responses.
- **Certificate pinning** (`pin_sha256`): Rust-side TLS certificate public key pinning for HTTP clients.
- **Multipart/form-data encoder** (Rust): Native multipart upload support in `catcher-http`.
- **DNS nameservers config**: Custom DNS resolver addresses for Rust HTTP client.
- **Web progress events**: Browser package (`catcher-web`) progress tracking for downloads/uploads.
- **ESM export fix**: All TS packages now correctly export ESM entry points.
- **Dart FFI config alignment**: Dart `HttpClientConfig` / `WsClientConfig` fields now match Rust config 1:1.
- **`#[serde(alias)]` on all configs**: Every config struct accepts both `snake_case` and `camelCase` JSON keys.

### 🐛 Bug Fixes

- Fixed 9 documented issues (#001–#006, #008–#010): memory leak in multipart, SSE O(n²) buffer, P90 repeated sort, config clone per-request, handle registry lock contention, circuit breaker TOCTOU, WS heartbeat RTT always zero, SSE reconnect recreates client, stream chunk copy.
- Fixed UniFFI issues (#011–#018): stream chunk to vec copy, priority queue single worker, uniffi block-on-aux-thread, catcher-free-data UB, evaluate-quality race panic, SSE stream buffer-all, FFI stream cancel, WS client drop no close.
- Fixed FFI body base64 encoding (#019, #021) and adaptive heartbeat timer (#020).
- Fixed UniFFI evaluate_quality take/put race (#023), stream chunk base64 (#022), SSE block_on panic (#024).
- Fixed `http_retries` metric wiring via custom `MetricsRetryMiddleware`.
- Fixed proxy bandwidth variable name bug.

### 🔄 Dependencies

- Upgraded `reqwest` 0.12 → 0.13
- Upgraded `tungstenite` 0.26 → 0.29, `tokio-tungstenite` 0.24 → 0.29

### 📝 Documentation

- Added breaking change notices to all package READMEs.
- Added `docs/arch-rs/17-dart-config-alignment.md` — Dart FFI config alignment design.
- Updated `docs/arch-rs/01-cargo.md` for v0.3 workspace.
- Updated all version references across docs.
- Added expansion research reports with citations.

## 0.2.2 (2026-06)

### 🚀 New Features — FFI

- **SSE C ABI** (FFI-02): 6 new `#[no_mangle]` symbols — `catcher_sse_connect`, `catcher_sse_stream`, `catcher_sse_ready_state`, `catcher_sse_last_event_id`, `catcher_sse_close`, `catcher_sse_destroy`. Dart `CatcherSseClient` + `sseStream()` bindings included.
- **Per-request headers + timeout** (FFI-01): `catcher_http_execute`, `catcher_http_get`, `catcher_http_post` now accept `headers_json` and `timeout_ms` parameters.
- **Circuit breaker state query** (FFI-05): `catcher_http_circuit_breaker_state` C ABI symbol. Dart `circuitBreakerState` getter.
- **Runtime metrics** (FFI-07): `catcher_http_metrics` returns `MetricsSnapshot` JSON. Dart `metrics` getter.
- **Cancel all in-flight requests** (FFI-04): `catcher_http_client_cancel_all`. Uses `Arc<Mutex<CancellationToken>>` with replacement semantics.
- **Adaptive timeout** (FFI-10): `catcher_http_adaptive_timeout_config` — P90 RTT-based auto-tuning.
- **Network quality history** (FFI-09): `catcher_quality_history` — persistent sliding window query.
- **TLS/DNS/Proxy config passthrough** (FFI-06): Full `TlsConfig`, `DnsConfig`, `ProxyConfig`, `RedirectConfig`, `Auth`, `default_headers` now passed via `HttpClientConfig` JSON from Dart.
- **WS config passthrough** (FFI-03/FFI-12): Dart `WsClientConfig` now includes `headers`, `protocols`, `deflateThresholdBytes`, `raceCount`.

### 🧪 Testing

- **14 new Rust FFI integration tests** (TEST-01): `catcher-ffi/tests/{http_test.rs, sse_test.rs, codec_quality_test.rs}` using wiremock mock servers.
- **19 new CatcherError unit tests** (TEST-08): Full error category + retryable classification coverage in `catcher-core/src/error.rs`.

### 📐 C ABI Symbol Count: 16 → 25

| Module  | Count | Symbols                                                                                                                       |
| ------- | :---: | ----------------------------------------------------------------------------------------------------------------------------- |
| HTTP    |   9   | client_create, client_destroy, execute, get, post, client_cancel_all, circuit_breaker_state, metrics, adaptive_timeout_config |
| SSE     |   6   | connect, stream, ready_state, last_event_id, close, destroy                                                                   |
| WS      |   5   | create, send_text, send_binary, close, destroy                                                                                |
| Codec   |   3   | pack, unpack, free_data                                                                                                       |
| Quality |   2   | evaluate_quality, quality_history                                                                                             |

### 📝 Documentation

- `docs/arch-rs/09-ffi.md` — Updated from 16 to 25 symbols, fixed file paths, signatures, and comparison tables.
- `docs/issues/ffi-uniffi-capability-gaps.md` — Marked FFI-01~07, 09~10, 12 as ✅; updated test coverage; refreshed roadmap.
- `docs/plan/10-ffi-capability-gap-design.md` — Added implementation status, updated test section, roadmap.
- `docs/plan/handoff.md` — Updated symbol counts and completion status.

### ✅ Verification

```
cargo check --workspace --all-targets    # 0 errors, 0 warnings
cargo clippy --workspace --all-targets -- -D warnings  # 0 warnings
cargo test --workspace                   # 88/90 passed (2 network-dependent)
pnpm test                                # 323 passed, 2 skipped
pnpm test:e2e                            # TS + Rust E2E scenarios
pnpm bench                               # 5 benchmark groups
```

## 0.2.1

- Initial public release.
- HTTP client with retries, circuit breaker, keep-alive, priority queue.
- WebSocket client with reconnection, multi-endpoint racing, perMessageDeflate.
- SSE client (TS) with auto-reconnect and one-shot stream.
- FFI bindings to Rust core (16 C ABI symbols).
- napi-rs bindings (Node.js HTTP + WS native addons).
- Dart FFI bindings (catcher_core pub.dev package).
- UniFFI bindings (Swift + Kotlin skeleton).

## 0.1.0

- Initial release.
- HTTP client with retries, timeouts, keep-alive.
- WebSocket client with reconnection and permessage-deflate.
- FFI bindings to Rust core.
