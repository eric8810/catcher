# Bug: catcher-ws 缺少 rustls `tls12` feature，连接仅支持 TLS 1.2 的服务器时 HandshakeFailure

**严重程度**: 🔴 High — 所有仅支持 TLS 1.2 的 wss:// 服务器均无法连接

**状态**: Fixed (v0.3.5 → v0.3.6)

**影响包**: `catcher-ws`、`catcher-napi-ws`

**位置**: `packages/catcher-ws/Cargo.toml:25`

---

## 现象

`catcher-napi-ws@0.3.5` 连接 `wss://ws-gateway.fazhiplus.com/ws` 时 TLS 握手失败：

```
received fatal alert: HandshakeFailure
```

客户端没有发送任何 TLS 1.2 密码套件，服务器（仅支持 TLS 1.2）无法匹配 → 直接拒绝。

## 根因

### 直接原因

`catcher-ws/Cargo.toml` 中 `rustls` 依赖设为 `default-features = false`，仅启用了 `ring` feature，**缺少 `tls12` feature**：

```toml
# 修复前 — 没有 tls12
rustls = { version = "0.23", default-features = false, optional = true }
```

### rustls 0.23 的 feature 机制

根据 [rustls 0.23.40 源码](https://github.com/rustls/rustls/blob/v/0.23.40/rustls/Cargo.toml)，默认 features 为：

```toml
default = ["aws_lc_rs", "logging", "prefer-post-quantum", "std", "tls12"]
```

`tls12` 是一个**编译级 feature**，控制是否包含 TLS 1.2 密码套件的实现代码。当 `tls12` 未启用时：

- `CryptoProvider.cipher_suites` 只有 3 个 TLS 1.3 密码套件
- 完全没有 TLS 1.2 的 6 个密码套件（`TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256` 等）
- ClientHello 中不包含任何 TLS 1.2 密码套件

### 为什么 catcher-http 没有这个问题

`catcher-http` 的 rustls 依赖一直包含 `tls12`：

```toml
# packages/catcher-http/Cargo.toml:42
rustls = { version = "0.23", optional = true, default-features = false, 
           features = ["aws_lc_rs", "logging", "std", "tls12"] }
```

## 诊断过程

### 1. 确认服务器 TLS 配置

```bash
$ curl -v --head https://ws-gateway.fazhiplus.com/
SSL connection using TLSv1.2 / ECDHE-RSA-AES128-GCM-SHA256 / prime256v1 / rsaEncryption
```

服务器证书链：`*.fazhiplus.com` → `GeoTrust G2 TLS CN RSA4096 SHA256 2022 CA1` → `DigiCert Global Root G2`。仅支持 TLS 1.2，不支持 TLS 1.3。

### 2. 编写诊断 binary 检查 CryptoProvider

运行 `tls_debug` example，打印 `CryptoProvider` 的密码套件列表：

```
=== CryptoProvider (ring) ===
Cipher suites:
  TLS13_AES_256_GCM_SHA384
  TLS13_AES_128_GCM_SHA256
  TLS13_CHACHA20_POLY1305_SHA256
```

**只有 3 个 TLS 1.3 密码套件，零个 TLS 1.2 密码套件。**

### 3. 确认修复后

启用 `tls12` feature 后，密码套件增加到 9 个：

```
=== CryptoProvider (ring) ===
Cipher suites:
  TLS13_AES_256_GCM_SHA384
  TLS13_AES_128_GCM_SHA256
  TLS13_CHACHA20_POLY1305_SHA256
  TLS_ECDHE_ECDSA_WITH_AES_256_GCM_SHA384
  TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256
  TLS_ECDHE_ECDSA_WITH_CHACHA20_POLY1305_SHA256
  TLS_ECDHE_RSA_WITH_AES_256_GCM_SHA384
  TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256         ← 服务器需要这个
  TLS_ECDHE_RSA_WITH_CHACHA20_POLY1305_SHA256
```

直接连接验证成功：`SUCCESS! Response: HTTP/1.1 404 Not Found`（TLS 握手通过）。

## 修复

一行改动：

```toml
# packages/catcher-ws/Cargo.toml:25

# 修复前
rustls = { version = "0.23", default-features = false, optional = true }

# 修复后
rustls = { version = "0.23", default-features = false, optional = true, features = ["tls12"] }
```

## 验证

- `cargo check --all-targets --features rustls-tls` ✅
- `cargo clippy --all-targets -- -D warnings` ✅
- `cargo test --features rustls-tls` — 25 tests pass ✅
- `cargo test -p catcher-napi-ws` — 4 tests pass ✅
- Feature 隔离验证：`cargo tree -f "{p} {f}"` 确认 `rustls v0.23.40 ring,std,tls12` ✅
- 实际 wss:// 连接验证成功 ✅

## 经验教训

1. **rustls `default-features = false` 是危险的**：`tls12` 是默认 feature，关掉后 TLS 1.2 完全不可用。这在 2026 年仍然重要，因为大量生产服务器（尤其中国大陆的 CDN/Nginx）仍只支持 TLS 1.2。
2. **应该对齐同仓库其他包的 rustls 配置**：`catcher-http` 的配置是正确的参考模板。
3. **可以考虑补齐 `logging` feature**：便于未来调试 TLS 问题。当前不影响功能。

## 关联

- `catcher-http/Cargo.toml:42` — 正确的 rustls feature 配置参考
- `catcher-ws/src/transport/ws_client.rs:28-35` — `ensure_tls_provider()` ring CryptoProvider 初始化
- rustls 文档：https://docs.rs/rustls/0.23.40/rustls/#crate-features
