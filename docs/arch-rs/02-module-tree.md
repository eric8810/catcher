# 02 — 模块树与文件清单 (v0.2 Workspace)

> 代码位置：`packages/`

## catcher-core (共享核心，零 I/O)

```
catcher-core/
├── Cargo.toml
└── src/
    ├── lib.rs                      # re-export error, config, types
    ├── error.rs                    # CatcherError, ErrorCategory
    ├── config.rs                   # CatcherConfig（顶层配置容器，无 ws/http 类型引用）
    └── types/
        ├── mod.rs
        ├── resilience.rs           # RetryConfig, CircuitBreakerConfig, BackoffKind
        ├── observability.rs        # NetworkQualityLevel, ConnectionType, RttSnapshot, NetworkQualityResult
        └── scheduler.rs            # Priority, QueueConfig, ConcurrencyMode
```

依赖：`thiserror`, `serde`, `serde_json`（零 I/O，无 tokio/reqwest）

## catcher-http (HTTP 客户端)

```
catcher-http/
├── Cargo.toml
└── src/
    ├── lib.rs                      # re-export HTTP client + resilience + scheduler + observability
    │
    ├── types/
    │   └── http.rs                 # HttpClientConfig, HttpRequest, HttpResponse, HttpMethod
    │
    ├── transport/                   # 传输层: TCP/TLS/HTTP 收发
    │   ├── mod.rs
    │   ├── http_client.rs          # HttpTransport: reqwest + reqwest-middleware 封装
    │   ├── tls.rs                   # build_tls_config: TlsConfig → reqwest ClientBuilder
    │   └── dns.rs                   # build_dns_resolver: DnsConfig → hickory-resolver
    │
    ├── resilience/                  # 韧性原语
    │   ├── mod.rs
    │   ├── retry.rs                # retry_with_backoff: backon 封装
    │   ├── circuit_breaker.rs      # CircuitBreaker: circuitbreaker-rs 封装, CbState
    │   ├── backoff.rs              # build_retry_policy: 统一退避构建器
    │   └── timeout.rs              # AdaptiveTimeout: P90 RTT * multiplier 自适应超时
    │
    ├── scheduler/                  # 调度层
    │   ├── mod.rs
    │   ├── priority_queue.rs       # PriorityRequestQueue
    │   └── concurrency.rs          # concurrency_for_quality(): 网络质量→并发数映射
    │
    ├── observability/               # 可观测性
    │   ├── mod.rs
    │   ├── network_quality.rs      # NetworkQualityEvaluator
    │   └── metrics.rs              # MetricsCollector
    │
    └── ffi/                        # HTTP + 质量 C ABI
        ├── mod.rs
        ├── http_ffi.rs             # HTTP C ABI
        ├── quality_ffi.rs          # 网络质量 C ABI
        └── types_ffi.rs            # FfiResult, FfiString, FfiBytes, EventCallback
```

依赖：`catcher-core`, `reqwest`, `reqwest-middleware`, `reqwest-retry`, `backon`, `tokio`, `parking_lot`, `hickory-resolver`(optional)

## catcher-ws (WebSocket 客户端)

```
catcher-ws/
├── Cargo.toml
└── src/
    ├── lib.rs                     # re-export WS client + reconnect + heartbeat + racing
    │
    ├── types/
    │   └── ws.rs                   # WsClientConfig, WsState, WsEvent, ReconnectConfig, HeartbeatConfig, DeflateConfig
    │
    ├── transport/
    │   ├── mod.rs
    │   └── ws_client.rs            # WsTransport: tokio-tungstenite 封装, WsHandle
    │
    ├── ws/                         # WebSocket 高级功能
    │   ├── mod.rs
    │   ├── reconnect.rs            # ReconnectManager: 重连状态机
    │   ├── heartbeat.rs            # HeartbeatManager: 自适应心跳
    │   ├── multi_endpoint.rs       # EndpointRacer: 多端点竞速
    │   └── compression.rs          # DeflateConfig 适配 tungstenite WebSocketConfig
    │
    └── ffi/
        └── ws_ffi.rs               # WS C ABI
```

依赖：`catcher-core`, `tokio-tungstenite`, `futures-util`, `backon`

## catcher-uniffi (UniFFI bindings)

```
catcher-uniffi/
├── Cargo.toml                      # deps: catcher-http, catcher-ws, uniffi
├── build.rs
└── src/
    └── lib.rs                      # #[uniffi::export] — 自动生成 Swift + Kotlin
```

## bindings

```
packages/
├── catcher-napi-http/               # npm 包 (napi-rs HTTP binding)
│   ├── package.json                 # @catcher/napi-http
│   ├── Cargo.toml                   # deps: catcher-http
│   ├── build.rs
│   ├── src/
│   │   └── lib.rs                   # JsHttpClient, JsHttpResponse
│   ├── index.js
│   └── index.d.ts
│
├── catcher-napi-ws/                 # npm 包 (napi-rs WS binding)
│   ├── package.json                 # @catcher/napi-ws
│   ├── Cargo.toml                   # deps: catcher-ws
│   ├── build.rs
│   ├── src/
│   │   └── lib.rs                   # JsWsClient
│   ├── index.js
│   └── index.d.ts
│
├── catcher-uniffi/                  # UniFFI crate → Swift + Kotlin
│   ├── Cargo.toml
│   ├── build.rs
│   └── src/
│       └── lib.rs
│
└── catcher_core/                    # pub.dev 包 (dart:ffi)
    ├── pubspec.yaml
    └── lib/
        ├── catcher_core.dart
        └── src/
            ├── native_loader.dart
            ├── ffi_bindings.dart
            └── http_client.dart
```
