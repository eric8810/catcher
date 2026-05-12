# WS 与 TUS 是否应独立拆分？

> 问题：WebSocket 和 TUS 的使用场景、生命周期都与 HTTP 客户端差异很大，是否应该从 catcher 主 crate 中拆分为独立 package/crate？

---

## 一、现状耦合分析

### 1.1 当前结构

```
catcher-rs (单 crate)
├── src/transport/
│   ├── http_client.rs          # HttpTransport: reqwest
│   └── ws_client.rs            # WsTransport: stream-tungstenite
├── src/ws/                     # WebSocket 高级功能
│   ├── reconnect.rs
│   ├── heartbeat.rs
│   ├── multi_endpoint.rs
│   └── compression.rs
├── src/upload/                 # TUS (尚未实现，但规划在同一 crate)
├── src/resilience/             # Retry / CircuitBreaker / Backoff / Timeout
├── src/scheduler/              # PriorityQueue / Concurrency
├── src/codec/                  # msgpack
├── src/observability/          # NetworkQuality / Metrics
├── src/ffi/
│   ├── http_ffi.rs
│   ├── ws_ffi.rs               # WS 独立 FFI
│   └── (tus_ffi.rs)            # 规划中
└── ...

catcher-ts (单 npm 包)
├── src/agent/                  # SharedAgent
├── src/http/                   # createHttpClient
├── src/ws/                     # createResilientWS
├── src/codec/                  # pack/unpack
└── src/queue/                  # createPriorityQueue
```

### 1.2 当前共享的东西

| 共享模块 | HTTP 用 | WS 用 | TUS 用 | 共享理由是否成立？ |
|---------|---------|-------|--------|-------------------|
| `CatcherError` | ✅ | ✅ | ✅ | ✅ 错误类型统一可降低调用方心智负担 |
| `TlsConfig` / `DnsConfig` | ✅ | ✅ (WS handshake) | ✅ (HTTP transport) | ✅ TLS/DNS 配置确实通用 |
| `RetryConfig` / `CircuitBreakerConfig` | ✅ | ✅ (reconnect) | ✅ | ✅ 韧性原语语义一致 |
| `NetworkQualityEvaluator` | ✅ | ✅ | ✅ | ⚠️ WS 和 HTTP 共享同一网络底层，有意义 |
| `MetricsCollector` | ✅ | ✅ | ✅ | ✅ 统一指标 |
| `msgpack codec` | — | ✅ | ❌ | ⚠️ 仅 WS 使用 |
| `PriorityQueue` | ✅ | ❌ | ⚠️ | ⚠️ 主要 HTTP 使用 |
| `ConnectionPoolConfig` | ✅ | ❌ | ✅ | ✅ TUS 复用 HTTP 连接池 |

---

## 二、生命周期对比

| 维度 | HTTP Client | WebSocket | TUS Upload |
|------|-------------|-----------|------------|
| **连接模型** | 短连接，请求-响应后释放 (keep-alive 复用) | 长连接，持续数分钟到数小时 | 中长连接，数秒到数十分钟 |
| **状态管理** | 无状态 | 多状态机 (Connecting/Connected/Reconnecting/Disconnected) | 多状态机 (Creating/Uploading/Paused/Completed/Failed) |
| **生命周期触发** | 调用方调用方法时 | `connect()` 到 `close()` | `create_upload()` 到 `upload()` 完成 |
| **资源占用** | 低，连接池可复用 | 高，每连接一个 tokio task + heartbeat timer | 中，流式读写 + chunk 状态维护 |
| **恢复策略** | 重试不同连接 | 重连同一或不同 endpoint，需要状态保持 | 从 offset 断点续传，需要 URL 存储 |
| **并发模型** | 高并发，大量并发短请求 | 低并发，1-3 个长连接 | 少量并行上传 |
| **终止方式** | 自然结束或取消本次请求 | 主动 close() 或异常断开 | 主动 terminate(DELETE) 或暂停 |

**结论**：HTTP 的生命周期完全不同于 WS 和 TUS。HTTP 是 fire-and-forget 的 RPC 模式，WS/TUS 是持久化的流模式。

---

## 三、用户视角分析

### 3.1 用户使用的典型组合

```
场景A：纯 HTTP API 调用（REST API）
  → 只需要 HTTP Client + resilience
  → 不需要 WS、不需要 TUS、不需要 codec
  → 但当前全打进一个包，增大了二进制体积和依赖面

场景B：IM 实时通信
  → 需要 HTTP（API 调用 + Token 刷新）+ WS（消息推送）
  → 可能不需要 TUS（文件走独立上传通道）

场景C：文件上传服务
  → 需要 HTTP（获取 uploadToken）+ TUS（断点续传）
  → 可能不需要 WS

场景D：全功能 IM
  → HTTP + WS + TUS 三个都要
```

### 3.2 拆分后的用户安装体验

| 场景 | 当前 | 拆分后 |
|------|------|--------|
| A: REST API | `npm i catcher` (全量依赖: ws, msgpackr, ...) | `npm i @catcher/http` (仅 reqwest) |
| B: IM 实时 | `npm i catcher` | `npm i @catcher/http @catcher/ws` |
| C: 文件上传 | `npm i catcher` | `npm i @catcher/http @catcher/tus` |
| D: 全功能 | `npm i catcher` | `npm i @catcher/http @catcher/ws @catcher/tus` 或 `npm i catcher` (umbrella) |

---

## 四、生态系统参考

审视主流生态，**没有任何一个库把 HTTP + WebSocket + 断点续传上传三个特性合并在一起**：

| 库 | HTTP | WebSocket | 断点续传上传 | 定位 |
|----|------|-----------|-------------|------|
| axios | ✅ | ❌ | ❌ | 单一：HTTP client |
| dio | ✅ | ❌ | ❌ (基础 download/upload) | HTTP client + 基础文件传输 |
| Socket.IO | ❌ | ✅ (WS + HTTP 长轮询 fallback) | ❌ | 单一：实时通信 |
| ws | ❌ | ✅ (纯 WS) | ❌ | 单一：WebSocket 底层库 |
| tus-js-client | ❌ | ❌ | ✅ (tus) | 单一：断点续传上传 |
| Uppy | 部分 | ❌ | ✅ (tus companion) | 文件上传 UI 框架 |
| **catcher** | ✅ | ✅ | 规划中 | **三者合一（仅此一家）** |

关键洞察：生态中所有库都是 **单一职责**。但它们之间的组合模式是清晰的：

- IM 类应用：`axios/dio (HTTP) + Socket.IO/ws (WS)`
- 文件上传：`axios/dio (获取 token) + tus-js-client (上传)`
- 三者都做的是少数，通常就是自行组合多个独立的库

---

## 五、拆分 vs 不拆分的利弊分析

### 5.1 保持单一 crate/package 的理由 (Monolith)

| 论点 | 说服力 |
|------|--------|
| **统一错误类型**：`CatcherError` 统一处理 HTTP 错误、WS 错误、TUS 错误 | ⭐⭐⭐ |
| **共享韧性原语**：RetryScheduler / CircuitBreaker 代码可直接复用 | ⭐⭐⭐ |
| **统一指标**：MetricsCollector 聚合三者的统计数据 | ⭐⭐ |
| **共享 TLS/DNS 配置**：同一份配置驱动所有协议 | ⭐⭐ |
| **网络质量联动**：NetworkQuality 可同时影响 HTTP 并发数、WS 心跳间隔、TUS chunkSize | ⭐⭐⭐⭐ |
| **调用方心智负担低**：一个依赖解决所有网络问题 | ⭐⭐⭐ |
| **减少 crate 维护成本**：一个 Cargo.toml / package.json 管所有 | ⭐⭐ |
| **内部模块边界已有**：`src/transport/http_client.rs` 和 `src/ws/` 已经分离 | ⭐⭐ |

### 5.2 拆分后的收益

| 论点 | 说服力 |
|------|--------|
| **按需安装**：用户只装需要的部分，减少依赖膨胀 | ⭐⭐⭐⭐ |
| **独立版本号**：WS 可以快速迭代修 bug 而不影响 HTTP 的稳定版本 | ⭐⭐⭐⭐ |
| **独立依赖树**：`@catcher/http` 不需要 `tokio-tungstenite`，`@catcher/tus` 不需要 `stream-tungstenite` | ⭐⭐⭐⭐⭐ |
| **独立测试范围**：每个包只测自己的生命周期，CI 更快 | ⭐⭐⭐ |
| **独立文档**：每个包有自己清晰的 use case，文档更聚焦 | ⭐⭐⭐ |
| **独立安全审计**：WS 的 CVE 不影响 HTTP 用户 | ⭐⭐⭐ |
| **团队分工**：不同人负责不同包，不互相阻塞 | ⭐⭐ |
| **FFI 独立**：Dart 侧可以只生成需要的 FFI 绑定 | ⭐⭐⭐ |

### 5.3 拆分的代价

| 代价 | 严重程度 |
|------|---------|
| **共享类型需要独立 crate**：`TlsConfig`, `CatcherError`, `RetryConfig` 等需抽出到 `catcher-core` | 中 |
| **网络质量联动实现变复杂**：NetworkQuality 在 core 中，需要事件总线通知各子包 | 中 |
| **版本协调**：如果 `catcher-core` 有一个 breaking change，三个子包都要跟进 | 中 |
| **调用方学习成本**：需要了解要装哪些包 | 低 (umbrella 包可缓解) |
| **测试需要跨包 mock**：更复杂的集成测试 | 低 |

---

## 六、推荐方案：分层拆分

### 6.1 目标结构

```
catcher-rs/                          # Rust workspace
├── crates/
│   ├── catcher-core/                # 共享核心（零 I/O）
│   │   ├── src/
│   │   │   ├── error.rs             # CatcherError
│   │   │   ├── types/
│   │   │   │   ├── config.rs        # TlsConfig, DnsConfig, ConnectionPoolConfig
│   │   │   │   ├── resilience.rs    # RetryConfig, CircuitBreakerConfig
│   │   │   │   └── observability.rs # NetworkQualityLevel, RttSnapshot
│   │   │   └── observability/
│   │   │       └── metrics.rs       # MetricsCollector
│   │   └── Cargo.toml
│   │       deps: thiserror, serde, parking_lot (极简)
│   │
│   ├── catcher-http/                # HTTP 客户端
│   │   ├── src/
│   │   │   ├── transport/           # HttpTransport (reqwest)
│   │   │   ├── resilience/          # RetryScheduler, CircuitBreaker, AdaptiveTimeout
│   │   │   ├── scheduler/           # PriorityQueue, ConcurrencyControl
│   │   │   └── ffi/                 # HTTP C ABI
│   │   └── Cargo.toml
│   │       deps: catcher-core, reqwest, reqwest-middleware, backon, etc.
│   │
│   ├── catcher-ws/                  # WebSocket 客户端
│   │   ├── src/
│   │   │   ├── transport/           # WsTransport (stream-tungstenite)
│   │   │   ├── ws/                  # reconnect, heartbeat, multi_endpoint
│   │   │   └── ffi/                 # WS C ABI
│   │   └── Cargo.toml
│   │       deps: catcher-core, stream-tungstenite, backon, etc.
│   │
│   ├── catcher-tus/                 # TUS 上传客户端
│   │   ├── src/
│   │   │   ├── client.rs            # TusClient (依赖 catcher-http)
│   │   │   ├── extensions/          # creation, termination, checksum, concatenation
│   │   │   ├── storage/             # UrlStorage trait + MemoryStorage
│   │   │   └── ffi/                 # TUS C ABI
│   │   └── Cargo.toml
│   │       deps: catcher-core, catcher-http, sha2, etc.
│   │
│   └── catcher-codec/               # 编解码
│       ├── src/
│       │   └── msgpack.rs           # pack / unpack
│       └── Cargo.toml
│           deps: rmp-serde
│
├── bindings/
│   ├── catcher-napi/                # Node.js 绑定 (napi-rs)
│   │   deps: catcher-http, catcher-ws, catcher-tus, catcher-codec
│   │
│   └── catcher-flutter/             # Dart 绑定 (flutter_rust_bridge)
│       deps: catcher-http, catcher-ws, catcher-tus, catcher-codec
│
└── catcher/                         # Umbrella crate (聚合 re-export)
    deps: catcher-http, catcher-ws, catcher-tus, catcher-codec
    → 为不想细选依赖的用户提供一站式入口
```

TypeScript 侧对应：

```
packages/
├── catcher-core-ts/                 # @catcher/core: 共享类型
│   exports: types, errors, MetricsCollector port
│
├── catcher-http-ts/                 # @catcher/http
│   deps: @catcher/core, axios, p-retry, cockatiel, p-queue
│
├── catcher-ws-ts/                   # @catcher/ws
│   deps: @catcher/core, ws
│
├── catcher-tus-ts/                  # @catcher/tus
│   deps: @catcher/core, @catcher/http
│
├── catcher-codec-ts/                # @catcher/codec
│   deps: msgpackr
│
└── catcher-ts/                      # @catcher (umbrella)
    deps: all of above
```

### 6.2 依赖关系图

```
                        catcher-core (类型 + 错误 + 指标)
                       /        |         \
                      /         |          \
              catcher-http  catcher-ws  catcher-codec
                  |
                  |
              catcher-tus
                  |
                  |
         bindings (napi / flutter)
```

关键约束：
- `catcher-core` **零 I/O 依赖**，只包含类型定义和纯数据结构
- `catcher-http` 和 `catcher-ws` 互不依赖
- `catcher-tus` 依赖 `catcher-http`（tus 协议走 HTTP transport）
- `catcher-codec` 独立（`catcher-ws` 可选依赖，或调用方自行组合）

### 6.3 共享层设计 (catcher-core)

```rust
// catcher-core/src/lib.rs
// 不依赖 reqwest/tokio-tungstenite/任何 I/O 框架

pub mod error {
    pub enum CatcherError { ... }
    pub enum ErrorCategory { ... }
}

pub mod config {
    pub struct TlsConfig { ... }
    pub struct DnsConfig { ... }
    pub struct ConnectionPoolConfig { ... }
}

pub mod resilience {
    pub struct RetryConfig { ... }
    pub struct CircuitBreakerConfig { ... }
    pub enum BackoffKind { ... }
}

pub mod observability {
    pub enum NetworkQualityLevel { ... }
    pub struct RttSnapshot { ... }
    pub struct NetworkQualityResult { ... }

    /// 线程安全的指标收集器，供上层聚合
    pub struct MetricsCollector { ... }

    /// event bus trait，各子包通过此 trait 发布事件
    pub trait EventBus: Send + Sync {
        fn emit(&self, event: CoreEvent);
    }

    pub enum CoreEvent {
        HttpRequestCompleted { duration_ms: u64, status: u16 },
        WsStateChanged { from: WsState, to: WsState },
        TusUploadProgress { upload_id: String, offset: u64, total: u64 },
        NetworkQualityChanged { from: NetworkQualityLevel, to: NetworkQualityLevel },
    }
}
```

网络质量联动问题的解决：
```rust
// catcher-core 中定义 NetworkQualityObserver trait
pub trait NetworkQualityObserver: Send + Sync {
    fn on_quality_changed(&self, quality: NetworkQualityLevel);
}

// catcher-http 中：
// HttpTransport 注册为 observer，quality 变化时自动调节并发数
// catcher-ws 中：
// HeartbeatManager 注册为 observer，quality 变化时自动调节心跳间隔
// catcher-tus 中：
// TusClient 注册为 observer，quality 变化时自动调节 chunkSize
```

---

## 七、拆分时机

### 7.1 ✅ 前提条件已满足

Phase 5 (FFI Bindings) 已经开发完成。对照此前提出的触发条件：

1. ✅ `catcher-core` 的类型定义稳定 — `CatcherError`, `RetryConfig` 等已收敛，FFI 层已在用
2. ✅ HTTP 客户端通过完整 e2e 测试
3. ✅ WebSocket 客户端状态机稳定
4. ✅ FFI 契约稳定 — C ABI (`src/ffi/`) + napi-rs + flutter_rust_bridge 均已就位
5. ⚠️ TUS 客户端尚未实现 — 这是唯一未满足的条件，但不应阻塞拆分（TUS 可作为新 crate 从零开始）

**结论：拆分应现在启动。** 延迟拆分的唯一理由（类型不稳定）已不再成立。

### 7.2 新增的系统约束

考虑到架构已成熟，拆分时必须保护以下已有资产：

| 约束 | 说明 |
|------|------|
| C ABI (`src/ffi/`) 不变 | napi-rs 和 Dart 两侧都依赖 `extern "C"` 导出函数，拆分时保持签名兼容 |
| napi-rs binding 路径不变 | `catcher-rs-napi/` 的 `Cargo.toml` deps 从 `catcher-rs` 改为 `catcher-http + catcher-ws + catcher-codec` |
| Dart binding 路径不变 | `catcher_core/` 的 `pubspec.yaml` 保持 cattcher_core.dart 类名不变，内部改为流控到新 crate 的 FFI |
| TS umbrella 包兼容 | 存量 code `import { createHttpClient } from 'catcher'` 必须继续工作 |
| umbrella crate 名称不变 | `catcher-rs` 作为 umbrella，内部 deps 拆成子 crate，对下游透明 |

### 7.3 具体拆分步骤

| Step | 动作 | 产出 | 风险 |
|------|------|------|------|
| **1** | 创建 `crates/catcher-core/`，将 `error.rs`, `config.rs`, `types/` 移入 | catcher-core v0.1.0 | 低，纯类型移动 |
| **2** | 将 `catcher-rs` 的 `Cargo.toml` 改为 `dep: catcher-core`，删掉已移走的源文件 | 单 crate 引 core | 中，需确保所有 import 路径修完 |
| **3** | 运行已有集成测试，确保不回归 | 测试全绿 | 低 |
| **4** | 创建 `crates/catcher-codec/`，将 `src/codec/` 移入 | catcher-codec v0.1.0 | 低 |
| **5** | 创建 `crates/catcher-ws/`，将 `src/transport/ws_client.rs`, `src/ws/` 移入 | catcher-ws v0.1.0 | 中，WS 依赖 tokio-tungstenite |
| **6** | catcher-rs 更新为 umbrella crate，re-export 所有子 crate | 对外 API 不变 | 低 |
| **7** | 更新 `catcher-rs-napi/Cargo.toml` deps | Node.js 绑定指向子 crate | 低 |
| **8** | 更新 `catcher_core/` Dart 侧 deps（如有需要） | Dart 绑定，实际仅改 Cargo.toml | 低 |
| **9** | TS 侧同步拆分：`@catcher/core` → `@catcher/http` / `@catcher/ws` / `@catcher/codec` | 独立 npm 包 | 中，需改 package.json exports |
| **10** | 创建 `crates/catcher-tus/`（全新 crate，依赖 catcher-http） | TUS 从零在新 crate 中建设 | 低，全新代码 |

---

## 八、结论 (更新)

**应该拆分，现在就是合适的时机。**

Phase 5 完成意味着此前阻塞拆分的最大风险（类型和 FFI 契约不稳定）已消除。现有的 C ABI 层、napi-rs 绑定、Dart 绑定均已有清晰的所有者 crate，拆分为独立 crate 后不需要改变任何暴露的 API。

拆分的核心理由不变：

1. **依赖体积**：`catcher-ws` 需要 `stream-tungstenite`，拆分后纯 HTTP 场景的编译产物不含 WS 相关依赖。Flutter/Dart FFI 二进制体积直接受益。

2. **生命周期根本不同**：HTTP 无状态 RPC vs WS 长连接流 vs TUS 流式续传，应该各自有独立的 crate 和独立文档。

3. **生态验证了单一职责**：axios (HTTP) / Socket.IO (WS) / tus-js-client (upload)，三者从未被合并——用户场景就是不重叠的。

4. **TUS 从零开始更是优势**：不需要在已有的 `catcher-rs/src/upload/` 下跟 HTTP/WS 代码混战，直接在 `catcher-tus` 新 crate 中以独立生命周期开发。

**唯一不立即拆的部分**：TUS 客户端尚未实现，恰好在 `catcher-tus` 中从零开始，这是拆分带来的正向收益而非阻塞项。
