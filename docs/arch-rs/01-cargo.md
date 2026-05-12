# 01 — Cargo.toml 依赖清单 (v0.2 Workspace)

## workspace Cargo.toml (`packages/Cargo.toml`)

```toml
[workspace]
resolver = "2"
members = [
    "catcher-core",
    "catcher-http",
    "catcher-ws",
    "catcher-napi-http",
    "catcher-napi-ws",
]
```

## catcher-core

```toml
[package]
name = "catcher-core"
version = "0.1.0"
edition = "2021"

[dependencies]
thiserror = "2"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
```

## catcher-http

```toml
[package]
name = "catcher-http"
version = "0.1.0"
edition = "2021"

[features]
default = ["rustls-tls", "hickory-dns"]
rustls-tls = ["reqwest/rustls-tls"]
native-tls = ["reqwest/native-tls"]
hickory-dns = ["reqwest/hickory-dns", "dep:hickory-resolver"]
napi = ["dep:napi", "dep:napi-derive"]

[dependencies]
catcher-core = { path = "../catcher-core" }
tokio = { version = "1", features = ["sync", "time", "net", "io-util", "macros"] }
reqwest = { version = "0.12", default-features = false, features = ["http2", "gzip", "brotli", "deflate", "stream", "charset"] }
reqwest-middleware = "0.4"
reqwest-retry = "0.7"
retry-policies = "0.4"
hickory-resolver = { version = "0.25", optional = true, features = ["tokio"] }
backon = "1"
trmp-serde = "1"
	rmpv = "1"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
parking_lot = "0.12"
napi = { version = "2", optional = true, default-features = false, features = ["napi4", "tokio_rt", "serde-json"] }
napi-derive = { version = "2", optional = true }
```

## catcher-ws

```toml
[package]
name = "catcher-ws"
version = "0.1.0"
edition = "2021"

[dependencies]
catcher-core = { path = "../catcher-core" }
tokio = { version = "1", features = ["sync", "time", "net", "io-util", "macros"] }
tokio-tungstenite = "0.24"
futures-util = "0.3"
backon = "1"
trmp-serde = "1"
	rmpv = "1"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
```

