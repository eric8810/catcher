# 14 — Workspace 结构

> 代码位置：`packages/`
> 4 个 lib crate + 3 个 binding crate

---

## 设计原则

| 原则 | 说明 |
|------|------|
| **按协议分层** | HTTP / WS 各自独立 crate，互不依赖 |
| **最小依赖** | `catcher-core` 零 I/O，`catcher-ws` 不带 reqwest |
| **直接引用** | 调用方按需 depend 子 crate，无 umbrella 中间层 |

## Workspace 结构

```
packages/
├── Cargo.toml                          # [workspace] 7 members
│
├── catcher-core/                       # 共享核心（零 I/O）
│   ├── Cargo.toml                      # deps: thiserror, serde, serde_json
│   └── src/
│       ├── lib.rs
│       ├── error.rs
│       ├── ffi_types.rs
│       └── types/
│           ├── resilience.rs
│           ├── observability.rs
│           └── scheduler.rs
│
├── catcher-http/                       # HTTP 客户端
│   ├── Cargo.toml                      # deps: catcher-core, reqwest, reqwest-middleware, reqwest-retry, backon
│   └── src/
│       ├── lib.rs
│       ├── types/http.rs
│       ├── transport/                  # http_client, tls, dns
│       ├── resilience/                 # retry, circuit_breaker, backoff, timeout
│       ├── scheduler/                  # priority_queue, concurrency
│       ├── observability/              # network_quality, metrics
│       └── ffi/                        # http_ffi, quality_ffi
│
├── catcher-ws/                         # WebSocket 客户端
│   ├── Cargo.toml                      # deps: catcher-core, tokio-tungstenite, futures-util, backon
│   └── src/
│       ├── lib.rs
│       ├── codec.rs                    # msgpack pack/unpack (内置)
│       ├── types/ws.rs
│       ├── transport/ws_client.rs
│       ├── ws/                         # reconnect, heartbeat, multi_endpoint, compression
│       └── ffi/ws_ffi.rs
│
├── catcher-napi-http/                  # Node.js napi-rs HTTP
│   ├── Cargo.toml                      # deps: catcher-http, napi, napi-derive
│   ├── build.rs
│   ├── src/lib.rs
│   ├── index.js
│   └── index.d.ts
│
├── catcher-napi-ws/                    # Node.js napi-rs WS
│   ├── Cargo.toml                      # deps: catcher-ws, napi, napi-derive
│   ├── build.rs
│   ├── src/lib.rs
│   ├── index.js
│   └── index.d.ts
│
├── catcher-uniffi/                     # UniFFI → Swift + Kotlin
│   ├── Cargo.toml                      # deps: catcher-http, catcher-ws, uniffi
│   ├── build.rs
│   └── src/lib.rs                      # #[uniffi::export]
│
└── catcher_core/                       # pub.dev 包 (dart:ffi)
    ├── pubspec.yaml
    └── lib/
        ├── catcher_core.dart
        └── src/
            ├── native_loader.dart
            ├── ffi_bindings.dart
            └── http_client.dart
```

## 依赖关系图

```
                catcher-core (零 I/O)
               /              \
              /                \
      catcher-http          catcher-ws
       |       |             |       |
  napi-http  uniffi     napi-ws   uniffi
 (Node.js)  (Swift+     (Node.js) (Swift+
            Kotlin)                Kotlin)
       |
  dart:ffi (Flutter)
  catcher_core
```

## FFI 绑定映射

| 绑定 | 目录 | 依赖 | 状态 |
|------|------|------|------|
| Node.js HTTP | `catcher-napi-http/` | `catcher-http` | ✅ |
| Node.js WS | `catcher-napi-ws/` | `catcher-ws` | ✅ |
| Swift + Kotlin | `catcher-uniffi/` | `catcher-http`, `catcher-ws` | ✅ |
| Flutter | `catcher_core/` | C ABI → dart:ffi | ✅ |

## 按需安装

```toml
# Rust — HTTP
[dependencies]
catcher-http = "0.1"

# Rust — WebSocket (codec 内置)
[dependencies]
catcher-ws = "0.1"
```

```bash
# Node.js — native
npm i @eric8810/napi-http @eric8810/napi-ws

# Node.js — TS
npm i @eric8810/http @eric8810/ws

# Browser
npm i @eric8810/web

# Flutter
flutter pub add catcher_core
```
