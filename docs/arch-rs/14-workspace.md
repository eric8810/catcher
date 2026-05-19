# 14 — Workspace 结构

> 代码位置：`crates/` (Rust) + `packages/` (TS/Dart)
> 4 个 lib crate + 1 个 cdylib umbrella + 3 个 binding crate

---

## 设计原则

| 原则 | 说明 |
|------|------|
| **按协议分层** | HTTP / WS 各自独立 crate，互不依赖 |
| **最小依赖** | `catcher-core` 零 I/O，`catcher-ws` 不带 reqwest |
| **直接引用** | 调用方按需 depend 子 crate，无 umbrella 中间层 |

## Workspace 结构

```
crates/                               # Rust workspace
├── Cargo.toml                        # [workspace] 8 members
│
├── catcher-core/                     # 共享核心（零 I/O）
│   ├── Cargo.toml                    # deps: thiserror, serde, serde_json
│   └── src/
│       ├── lib.rs
│       ├── error.rs
│       ├── ffi_types.rs
│       └── types/
│
├── catcher-http/                     # HTTP 客户端
│   ├── Cargo.toml                    # deps: catcher-core, reqwest, backon
│   └── src/
│       ├── transport/                # http_client, tls, dns
│       ├── resilience/               # retry, circuit_breaker, backoff
│       ├── scheduler/                # priority_queue, concurrency
│       └── observability/            # network_quality, metrics
│
├── catcher-ws/                       # WebSocket 客户端
│   ├── Cargo.toml                    # deps: catcher-core, tokio-tungstenite, backon
│   └── src/
│       ├── codec.rs                  # msgpack pack/unpack (内置)
│       ├── transport/ws_client.rs
│       └── ws/                       # reconnect, heartbeat, multi_endpoint
│
├── catcher-ffi/                      # cdylib umbrella — 25 C ABI symbols
│   ├── Cargo.toml                    # deps: catcher-http, catcher-ws, catcher-core
│   └── src/
│       ├── lib.rs                    # block_on_aux_thread + all exports
│       ├── http.rs                   # catcher_http_*
│       ├── ws.rs                     # catcher_ws_*
│       ├── codec.rs                  # catcher_pack / catcher_unpack
│       └── quality.rs                # catcher_evaluate_quality
│
├── catcher-napi-http/                # Node.js napi-rs HTTP
├── catcher-napi-ws/                  # Node.js napi-rs WS
└── catcher-uniffi/                   # UniFFI → Swift + Kotlin (WIP)

packages/                             # TS/Dart workspace
├── catcher-core-ts/                  # @eric8810/catcher-core (TS types)
├── catcher-http-ts/                  # @eric8810/catcher-http
├── catcher-ws-ts/                    # @eric8810/catcher-ws
├── catcher-web/                      # @eric8810/catcher-web (browser)
├── catcher-napi-http/                # @eric8810/catcher-napi-http
├── catcher-napi-ws/                  # @eric8810/catcher-napi-ws
├── catcher_core/                     # catcher_core (pub.dev)
│   ├── rust/                         # depends on catcher-ffi
│   └── lib/
│       ├── http_client.dart
│       ├── ws_client.dart
│       ├── codec.dart
│       └── quality.dart
└── test/                             # E2E test suite
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
     catcher-ffi (cdylib umbrella)
              |
     dart:ffi (Flutter)
     catcher_core (pub.dev)
```

## FFI 绑定映射

| 绑定 | 目录 | 依赖 | 状态 |
|------|------|------|------|
| Node.js HTTP | `catcher-napi-http/` | `catcher-http` | ✅ Published |
| Node.js WS | `catcher-napi-ws/` | `catcher-ws` | ✅ Published |
| Swift + Kotlin | `catcher-uniffi/` | `catcher-http`, `catcher-ws` | ⚠️ WIP |
| Flutter | `catcher_core/` | `catcher-ffi` (cdylib) → dart:ffi | ✅ Published |

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
npm i @eric8810/catcher-napi-http @eric8810/catcher-napi-ws

# Node.js — TS
npm i @eric8810/catcher-http @eric8810/catcher-ws

# Browser
npm i @eric8810/catcher-web

# Flutter
flutter pub add catcher_core
```
