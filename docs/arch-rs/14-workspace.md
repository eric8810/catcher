# 14 — Workspace 结构 (v0.2 最终形态)

> 代码位置：`packages/`
> 四个独立 crate，无 umbrella

---

## 设计原则

| 原则 | 说明 |
|------|------|
| **按协议分层** | HTTP / WS / Codec 各自独立 crate，互不依赖 |
| **最小依赖** | `catcher-core` 零 I/O，`catcher-ws` 不带 reqwest |
| **直接引用** | 调用方按需 depend 子 crate，无 umbrella 中间层 |

## Workspace 结构

```
packages/
├── Cargo.toml                          # [workspace] members: core, http, ws, codec
│
├── catcher-core/                       # 共享核心（零 I/O）
│   ├── Cargo.toml                      # deps: thiserror, serde, serde_json
│   └── src/
│       ├── lib.rs
│       ├── error.rs                    # CatcherError, ErrorCategory
│       ├── ffi_types.rs                # FfiResult, FfiString, FfiBytes, EventCallback
│       └── types/
│           ├── mod.rs
│           ├── resilience.rs           # RetryConfig, CircuitBreakerConfig, BackoffKind
│           ├── observability.rs        # NetworkQualityLevel, RttSnapshot, Priority, ConnectionType
│           └── scheduler.rs            # QueueConfig, ConcurrencyMode
│
├── catcher-http/                       # HTTP 客户端
│   ├── Cargo.toml                      # deps: catcher-core, reqwest, reqwest-middleware, reqwest-retry, backon
│   └── src/
│       ├── lib.rs
│       ├── types/http.rs               # HttpClientConfig, HttpRequest, HttpResponse, HttpMethod
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
│       ├── types/ws.rs                 # WsClientConfig, WsState, WsEvent, ReconnectConfig, etc.
│       ├── transport/ws_client.rs      # WsTransport, WsHandle
│       ├── ws/                         # reconnect, heartbeat, multi_endpoint, compression
│       └── ffi/ws_ffi.rs               # WS C ABI
│
│   ├── Cargo.toml                      # deps: catcher-core, rmp-serde, rmpv
│   └── src/
│       ├── lib.rs
│       ├── msgpack.rs                  # pack / unpack / unpack_value
│       └── ffi/mod.rs                  # Codec C ABI
│
├── catcher-rs-napi/                    # ❌ 已删除（拆为三个独立包）
│
├── catcher-napi-http/                  # Node.js napi-rs — HTTP only
│   └── Cargo.toml                      # deps: catcher-http
│
│   └── Cargo.toml                      # deps: catcher-ws
│
├── catcher_core/                       # Dart FFI binding (flutter_rust_bridge)
│
└── catcher-tus/                        # 🔜 TUS 客户端（规划中）
```

## 依赖关系图

```
                catcher-core (零 I/O)
               /         |          \
              /          |           \
```

## 各 crate 变更记录

| crate | 来源 | 变更 |
|-------|------|------|
| `catcher-core` | 新建 | 从原 catcher-rs 移入 error.rs, types/{resilience, observability, scheduler}.rs, ffi/types_ffi.rs |
| `catcher-http` | 新建 | 从原 catcher-rs 移入 types/http.rs, transport/{http_client, tls, dns}.rs, resilience/, scheduler/, observability/, ffi/{http, quality}_ffi.rs |
| `catcher-ws` | 新建 | 从原 catcher-rs 移入 types/ws.rs, transport/ws_client.rs, ws/, ffi/ws_ffi.rs |
| `catcher-rs` | **已删除** | 原单 crate 和 umbrella 均不再保留 |

## FFI 绑定映射

| 绑定 | 目录 | 依赖 |
|------|------|------|
| Node.js HTTP | `catcher-napi-http/` | `catcher-http` |

## 按需安装示例

```toml
# 只需要 HTTP 客户端
[dependencies]
catcher-http = "0.1"

# 只需要 WebSocket
[dependencies]
catcher-ws = "0.1"

# 只需要编解码
[dependencies]

# 全要
[dependencies]
catcher-http = "0.1"
catcher-ws = "0.1"
```
