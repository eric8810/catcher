# 01 — Cargo.toml 依赖清单 (v0.3 Workspace)

## workspace Cargo.toml (`packages/Cargo.toml`)

```toml
[workspace]
resolver = "2"
members = [
    "catcher-core",
    "catcher-dns",
    "catcher-http",
    "catcher-ws",
    "catcher-ffi",
    "catcher-napi-http",
    "catcher-napi-ws",
    "catcher-uniffi",
]
```

## catcher-core

```toml
[package]
name = "catcher-core"
version = "0.3.11"
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
version = "0.3.11"
edition = "2021"

[features]
default = ["rustls-tls", "hickory-dns"]
rustls-tls = ["reqwest/rustls", "dep:rustls", "dep:rustls-pki-types", "dep:sha2", "dep:base64", "dep:webpki-roots"]
hickory-dns = ["reqwest/hickory-dns", "dep:hickory-resolver", "dep:hickory-proto"]
napi = ["dep:napi", "dep:napi-derive"]

[dependencies]
catcher-core = { path = "../catcher-core", version = "0.3.11" }
tokio = { version = "1", features = ["sync", "time", "net", "io-util", "macros"] }
tokio-util = "0.7"
tokio-stream = "0.1"
bytes = "1"
reqwest = { version = "0.13", default-features = false, features = ["http2", "gzip", "brotli", "deflate", "stream", "charset", "query", "form"] }
reqwest-middleware = "0.5"
reqwest-retry = "0.9"
retry-policies = "0.5"
hickory-resolver = { version = "0.25", optional = true, features = ["tokio"] }
hickory-proto = { version = "0.25", optional = true }
rustls = { version = "0.23", optional = true, default-features = false, features = ["aws_lc_rs", "logging", "std", "tls12"] }
rustls-pki-types = { version = "1", optional = true, features = ["std"] }
sha2 = { version = "0.10", optional = true }
base64 = { version = "0.22", optional = true }
webpki-roots = { version = "0.26", optional = true }
backon = "1"
rmp-serde = "1"
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
version = "0.3.11"
edition = "2021"

[features]
default = ["rustls-tls"]
rustls-tls = ["yawc/rustls-ring"]

[dependencies]
catcher-core = { path = "../catcher-core", version = "0.3.11" }
catcher-dns = { path = "../catcher-dns", version = "0.3.11" }
tokio = { version = "1", features = ["rt-multi-thread", "sync", "time", "net", "io-util", "macros"] }
yawc = { version = "0.3.3", default-features = false }
futures-util = "0.3"
backon = "1"
rmp-serde = "1"
rmpv = "1"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
base64 = "0.22"
flate2 = "1"
zstd = "0.13"
http = "1"
url = "2"

[dev-dependencies]
tokio-tungstenite = { version = "0.29", default-features = false, features = ["handshake"] }
```
