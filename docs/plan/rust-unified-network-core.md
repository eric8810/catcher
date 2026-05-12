# Rust 统一网络核心方案

> 撰写日期：2025-07-18
> 涉及仓库：`catcher` (TypeScript/Electron) + `echoo-flutter` (Dart/Flutter)
> 目标：用 Rust 编写一份网络韧性核心，通过 FFI 同时服务于两个平台的网络通信需求

---

## 1. 动机：为什么这个方案现在成立

### 1.1 现状：两套代码，两份维护

catcher 和 echoo-flutter 是为同一个产品（Moti/Klip）服务的两个客户端——桌面端（Electron/Node.js）和移动端（Android/iOS/HarmonyOS）。两者的网络通信需求高度重叠：

| 能力 | catcher (TS) | echoo-flutter (Dart) | 一致？ |
|------|-------------|---------------------|--------|
| HTTP 连接池 / KeepAlive | ✅ shared Agent | ❌ 每次新建 HttpClient | ✗ |
| DNS 缓存 | ✅ cacheable-lookup (300s TTL) | ❌ 无 | ✗ |
| 自动重试 | ✅ p-retry, 指数退避 + jitter | ⚠️ dio_smart_retry, 固定退避 | ✗ |
| 熔断器 | ✅ cockatiel CircuitBreaker | ❌ 无 | ✗ |
| 请求优先级队列 | ✅ p-queue | ❌ 无 | ✗ |
| 多端点竞速 | ✅ ws multi-endpoint racing | ❌ 无 | ✗ |
| WebSocket 压缩 | ✅ perMessageDeflate | ❌ 无 | ✗ |
| 网络质量评估 | ❌ 无 | ✅ ping + 链路类型 | ✗ |
| 自适应超时 | ❌ 无 | ⚠️ 基础设施就绪但未接入 | ✗ |
| 离线消息队列 | ❌ 不适用 | ⚠️ 部分实现 | — |
| 平台覆盖 | Windows / macOS / Linux | iOS / Android / HarmonyOS | 互补 |

**两个平台互补缺失的能力各占列出总项的 ~40%。** 如果分别改进，需要在 TypeScript 和 Dart 中各实现一遍，且保证行为一致。

### 1.2 核心论证：消除的不是技术债，是架构分裂

用 Rust 统一网络层的收益从四个维度展开：

#### 维度一：计算量与并发模型

- Node.js 的 libuv event loop 是单线程的。catcher 的 retry wrapper、circuit breaker、queue 等逻辑全部跑在同一个 JS 线程上。在高并发场景（如 50 个渲染进程共享一个主进程网络代理），GC 压力和 Promise 分配的开销不可忽略。
- Dart 在移动端同样单线程（主 Isolate）。NextWebSocketManager 虽然跑在独立 Isolate 内，但与主 Isolate 的通信靠 SendPort/ReceivePort 序列化，大量消息场景下有反序列化开销。
- **tokio 是 work-stealing 多线程调度器。** 1000 个连接的回调和状态机在多个 CPU 核上真正并行。这在高吞吐场景下的差异是数量级的。

#### 维度二：网络 I/O 栈的可靠性与可控性

- catcher 依赖 Node.js 的 `node:http` / `node:https` → libuv(TCP) + OpenSSL(TLS) + llhttp(HTTP parse)。这些底层都是成熟 C 库，但 Node.js 封装层在边缘情况存在行为差异——例如 `ECONNRESET` 在不同版本的处理方式不一致，`http.Agent` 连接池在特定场景下会复用已关闭的 socket。
- echoo-flutter 依赖 Dart 的 `dart:io` HttpClient → 在 iOS 上走 NSURLSession，在 Android 上走 OkHttp。同一行 Dart 代码在两个平台上走的是完全不同的协议栈，行为一致性无法保证。
- **用 Rust 直接管理 TCP/TLS（tokio + rustls/hyper），完全 bypass 平台差异。** 所有平台共享完全相同的协议栈实现。

#### 维度三：跨平台一致性（这是被低估的最大价值）

- `node:net` 在 Windows/Linux/macOS 上 `SO_KEEPALIVE` 默认值不同、`TCP_NODELAY` 行为差异
- `connectivity_plus` 在 Android/iOS/HarmonyOS 上返回的网络类型枚举不完全对齐
- Dart `HttpClient` 的 `connectionTimeout` 含义在平台上存在差异
- **HarmonyOS 的特殊性**：echoo-flutter 已有 `ohos/` 目录，HarmonyOS 是目标平台。但 Flutter 插件生态在 HarmonyOS 上远不如 iOS/Android 成熟，`dio`、`web_socket_client`、`connectivity_plus` 等核心网络库在 HarmonyOS 上的表现未经长期验证。Rust 通过 C FFI → HarmonyOS Native API 可以直接运行，完全 bypass Flutter 插件兼容性问题。

#### 维度四：消除重复代码

粗略估算，两个代码库中与网络韧性相关的逻辑：

| 模块 | catcher 行数 | echoo-flutter 行数 | 重叠度 |
|------|-------------|-------------------|--------|
| HTTP 客户端配置 / 连接池 | ~250 (agent + client) | ~200 (api_const.dart) | 高 |
| 重试策略 | ~100 (retry.ts) | ~80 (dio_smart_retry 配置) | 高 |
| WebSocket 重连 | ~300 (ws/) | ~200 (NextWebSocketManager) | 高 |
| 网络质量检测 | ~0 | ~200 (network_quality_service) | 互补 |
| 熔断器 | ~50 | ~0 | 互补 |
| 优先级队列 | ~100 | ~0 | 互补 |
| 多端点竞速 | ~100 | ~0 | 互补 |
| **合计** | **~900** | **~680** | **~1500 行可统一** |

**加上 catcher 历史 issues 中暴露的 14 个已知缺陷（分布在 keepalive、retry、circuit-breaker 等模块），** 在 Rust 层修一次 = 两个平台同步受益。

---

## 2. 语言选择：为什么是 Rust

在讨论方案前，先说明为什么 C / C++ / Zig 不是更优选择。

### 2.1 C

- **致命缺陷**：没有生产级异步 I/O 原语。需要手搓 epoll/kqueue/IOCP 事件循环，或者引入 libuv——但 libuv 就是 Node.js 的事件循环，又绕回去了。
- 没有包管理器、没有 HTTP/2 客户端库、napi 和 dart:ffi 都需要手动写 C ABI 样板。
- 内存安全完全靠人工审查，网络协议解析是内存安全 bug 的高发区。

**结论：不推荐。** C 的 ABI 是最通用的，但开发效率和生产安全性的代价不可接受。

### 2.2 C++

- **boost::beast + asio** 组合在纯网络 I/O 上与 tokio 正面竞争。
- **但绑定生态是致命短板**：napi-rs（Node.js 绑定）和 flutter_rust_bridge（Dart 绑定）都是 Rust 生态原生支持的，C++ 需要手写 C ABI 包装层 → napi-addon + dart:ffi 两套完全不同的样板代码。
- 包管理碎片化（CMake + vcpkg/conan），交叉编译配置复杂（MSVC/GCC/Clang ABI 差异）。

**结论：C++ 比 C 靠谱得多，但绑定的样板代码量是 Rust 的 3-5 倍，不值得为语言偏好付出这种维护成本。**

### 2.3 Zig

- C interop 是语言级内置、交叉编译原生支持。理论上是这个场景的理想选择。
- **现实**：HTTP 客户端、WebSocket 库都在 pre-1.0 阶段。Zig 的 async/await 已被移除。没有 napi binding 方案，没有 flutter binding 方案。
- 5 年后可能是最佳选择，但**现在用它等于同时铺铁轨和跑火车。**

### 2.4 Rust：生态恰好同时覆盖两个绑定目标

| 需求 | Rust 生态 | 成熟度 |
|------|-----------|--------|
| 异步运行时 | tokio | 生产级，1.0+ |
| HTTP 客户端 | reqwest / hyper | 生产级 |
| WebSocket | tokio-tungstenite | 生产级 |
| TLS | rustls (纯 Rust) / native-tls | 生产级 |
| DNS 缓存 | hickory-resolver | 生产级 |
| 序列化 | rmp-serde (msgpack) | 生产级 |
| Node.js 绑定 | napi-rs | 成熟，被 prisma/parcel/turbo 使用 |
| Dart 绑定 | flutter_rust_bridge | 成熟，被大量 Flutter 应用使用 |
| 交叉编译 | cargo + zigbuild / cross | 成熟 |

---

## 3. 架构设计

### 3.1 总体分层

```
                                    ┌─────────────────────────────────────────┐
                                    │            catcher-rs (Rust)            │
                                    │                                           │
                                    │  ┌─────────────────────────────────────┐  │
                                    │  │          Public FFI API              │  │
                                    │  │  (C ABI → napi-rs / dart:ffi)       │  │
                                    │  └────────────────┬────────────────────┘  │
                                    │                   │                       │
                                    │  ┌────────────────┴────────────────────┐  │
                                    │  │         Orchestration Layer         │  │
                                    │  │  HttpClient / WsClient / Queue      │  │
                                    │  │  组合 Resilience 原语为高层 API      │  │
                                    │  └────┬──────────┬──────────┬──────────┘  │
                                    │       │          │          │             │
                                    │  ┌────┴────┐ ┌───┴────┐ ┌───┴─────────┐  │
                                    │  │Retry    │ │Circuit │ │NetworkQual  │  │
                                    │  │Scheduler│ │Breaker │ │Evaluator    │  │
                                    │  └─────────┘ └────────┘ └─────────────┘  │
                                    │       │          │          │             │
                                    │  ┌────┴──────────┴──────────┴──────────┐  │
                                    │  │          Transport Layer             │  │
                                    │  │  tokio + hyper/reqwest + rustls      │  │
                                    │  │  + tokio-tungstenite                │  │
                                    │  └──────────────────────────────────────┘  │
                                    │                                           │
                                    └───────┬──────────────────────┬────────────┘
                                            │                      │
                              ┌─────────────▼──────┐   ┌──────────▼──────────────┐
                              │ napi-rs bindings    │   │ flutter_rust_bridge     │
                              │ (Node.js / Electron)│   │ (Flutter iOS/Android/   │
                              │                     │   │  HarmonyOS)             │
                              └─────────────────────┘   └─────────────────────────┘
                                            │                      │
                              ┌─────────────▼──────┐   ┌──────────▼──────────────┐
                              │ catcher (TypeScript)│   │ echoo-flutter (Dart)     │
                              │ Platform Adapter    │   │ Platform Adapter        │
                              │ • 业务 error 分类   │   │ • 业务 error 分类       │
                              │ • retryIf 条件      │   │ • retryIf 条件          │
                              │ • UI 状态绑定       │   │ • UI 状态绑定           │
                              └─────────────────────┘   └─────────────────────────┘
```

**核心原则：Rust 负责 "How"，上层负责 "What"。**
- Rust：管理 TCP/TLS/HTTP 连接、执行重试/熔断/竞速策略、心跳/超时控制
- 平台适配层：定义哪些错误可重试（业务语义）、UI 状态绑定、配置参数组装

### 3.2 模块设计

```
catcher-rs/
├── Cargo.toml
├── src/
│   ├── lib.rs                          # crate 根，re-export
│   │
│   ├── ffi/                            # FFI 层：C ABI 导出
│   │   ├── mod.rs
│   │   ├── http.rs                     # HTTP 客户端的 FFI 接口
│   │   ├── ws.rs                       # WebSocket 的 FFI 接口
│   │   ├── codec.rs                    # 编解码的 FFI 接口
│   │   └── types.rs                    # 跨 FFI 的共享类型
│   │
│   ├── transport/                      # 传输层：TCP/TLS/HTTP 的真正收发
│   │   ├── mod.rs
│   │   ├── http_client.rs              # reqwest/hyper 封装
│   │   ├── ws_client.rs                # tokio-tungstenite 封装
│   │   ├── tls_config.rs              # TLS 配置（rustls / native-tls 切换）
│   │   ├── dns_cache.rs               # hickory-resolver DNS 缓存层
│   │   └── connection_pool.rs         # 连接池管理
│   │
│   ├── resilience/                     # 韧性原语：纯状态机，零 I/O
│   │   ├── mod.rs
│   │   ├── retry.rs                    # 指数退避 + jitter + maxAttempts
│   │   ├── circuit_breaker.rs          # 滑动窗口熔断器
│   │   ├── backoff.rs                 # 退避策略（fixed / exponential / decorrelated）
│   │   └── timeout.rs                 # 自适应超时（基于 RTT 滑动窗口）
│   │
│   ├── websocket/                      # WebSocket 高级功能
│   │   ├── mod.rs
│   │   ├── reconnect.rs               # 重连状态机（退避 + 断线原因区分）
│   │   ├── heartbeat.rs               # 自适应心跳（RTT 驱动间隔）
│   │   ├── multi_endpoint.rs          # 多端点竞速连接
│   │   └── compression.rs             # perMessageDeflate 配置
│   │
│   ├── codec/                          # 序列化
│   │   ├── mod.rs
│   │   └── msgpack.rs                 # rmp-serde msgpack 编解码
│   │
│   ├── scheduler/                      # 调度层
│   │   ├── mod.rs
│   │   ├── priority_queue.rs          # 优先级请求队列
│   │   └── concurrency.rs             # 动态并发控制（基于网络质量）
│   │
│   └── observability/                  # 可观测性
│       ├── mod.rs
│       ├── network_quality.rs         # ping/RTT/带宽评估
│       └── metrics.rs                 # 延迟/成功率/熔断状态上报
```

### 3.3 API 设计（面向 FFI 的接口契约）

#### HTTP Client

```rust
// FFI 导出：创建客户端配置
#[derive(Serialize, Deserialize)]
pub struct HttpClientConfig {
    pub base_url: String,
    pub keep_alive: bool,
    pub keep_alive_msecs: u64,
    pub dns_cache_ttl_secs: u32,
    pub max_idle_connections: u32,
    pub connect_timeout_ms: u64,
    pub response_timeout_ms: u64,
    pub reject_unauthorized: bool,
    // 韧性策略
    pub retry: Option<RetryConfig>,
    pub circuit_breaker: Option<CircuitBreakerConfig>,
    // 并发控制
    pub max_concurrency: u32,
}

#[derive(Serialize, Deserialize)]
pub struct RetryConfig {
    pub max_attempts: u32,
    pub backoff: BackoffStrategy,     // fixed | exponential | decorrelated
    pub min_backoff_ms: u64,
    pub max_backoff_ms: u64,
    pub retry_on_status: Vec<u16>,    // 哪些 HTTP status code 可重试
    pub jitter: bool,
}

#[derive(Serialize, Deserialize)]
pub struct CircuitBreakerConfig {
    pub failure_threshold: u32,        // 连续失败多少次 trip
    pub success_threshold: u32,        // half-open 下连续成功多少次恢复
    pub reset_timeout_ms: u64,         // trip 后多久进入 half-open
    pub half_open_max_requests: u32,   // half-open 期间允许的最大试探请求数
}

// FFI 导出：发起请求
pub struct HttpResponse {
    pub status: u16,
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
}
```

#### WebSocket Client

```rust
pub struct WsClientConfig {
    pub urls: Vec<String>,                     // 多端点列表
    pub protocols: Vec<String>,
    pub headers: HashMap<String, String>,
    pub per_message_deflate: bool,
    pub deflate_threshold_bytes: u32,
    pub handshake_timeout_ms: u64,
    pub max_payload_bytes: u64,
    pub reconnect: Option<ReconnectConfig>,
    pub heartbeat: Option<HeartbeatConfig>,
    pub race_count: u32,                       // 同时竞速的端点数
}

pub struct ReconnectConfig {
    pub initial_delay_ms: u64,
    pub max_delay_ms: u64,
    pub backoff_multiplier: f64,
    pub max_attempts: u32,
}

pub struct HeartbeatConfig {
    pub interval_ms: u64,                      // 初始心跳间隔
    pub adaptive: bool,                        // 是否根据 RTT 自适应
    pub pong_timeout_ms: u64,                  // pong 超时 → 认为断线
    pub max_missed_pongs: u32,                 // 连续丢失几个 pong 后断开
}

// 事件回调 → 通过 FFI 推送给上层
pub enum WsEvent {
    Connected { url: String, latency_ms: u64 },
    Disconnected { code: u16, reason: String },
    Reconnecting { attempt: u32, delay_ms: u64 },
    Message { data: Vec<u8>, is_binary: bool },
    Error { message: String },
    HeartbeatRtt { rtt_ms: u64 },
}
```

#### Network Quality Evaluator

```rust
pub struct NetworkQualityResult {
    pub level: NetworkQualityLevel,            // excellent / good / fair / poor / bad
    pub avg_rtt_ms: u64,
    pub jitter_ms: u64,
    pub packet_loss_rate: f64,
    pub connection_type: ConnectionType,       // wifi / cellular / ethernet / vpn / unknown
}
```

### 3.4 平台适配层职责（TypeScript / Dart）

平台适配层**不管理连接、不实现重试策略、不处理编解码**，只做：

1. **配置组装**：从平台配置/环境变量组装 `HttpClientConfig` / `WsClientConfig`
2. **业务错误分类**：实现 `retry_if()` 回调——哪些业务 error code 算可重试（如 token 过期不应重试，网关超时应重试）
3. **UI 状态绑定**：将 Rust 侧推送的连接状态/网络质量变化映射到 UI 组件
4. **拦截器链**：HTTP 请求/响应的业务拦截器（如自动注入 token、401 处理），**在 FFI 调用之前/之后执行**

```typescript
// catcher (TypeScript) 侧用法示例
import { HttpClient, WsClient, NetworkQuality } from 'catcher-rs'

// 用 TS 配置驱动 Rust 核心
const client = new HttpClient({
  baseUrl: 'https://api-gateway.example.com',
  keepAlive: true,
  dnsCacheTtlSecs: 300,
  retry: {
    maxAttempts: 3,
    backoff: 'exponential',
    retryIf: (error) => {                       // ← 业务逻辑留在 TS
      if (error.httpStatus === 401) return false
      if (error.httpStatus === 403) return false
      if (error.httpStatus >= 500) return true
      if (error.code === 'ECONNRESET') return true
      return false
    },
  },
  circuitBreaker: { failureThreshold: 50, resetTimeoutMs: 30_000 },
})

// API 与现有 IHttpClient 兼容
const data = await client.get('/users/1')
const result = await client.post('/messages', { text: 'hello' })

// WebSocket 同样用配置驱动
const ws = new WsClient({
  urls: ['wss://cn.example.com', 'wss://sg.example.com'],
  perMessageDeflate: true,
  reconnect: { maxAttempts: 20, backoffMultiplier: 2 },
})
ws.on('message', (data) => { /* 业务处理 */ })
```

```dart
// echoo-flutter (Dart) 侧用法示例
import 'package:catcher_core/catcher_core.dart';

final client = HttpClient(
  config: HttpClientConfig(
    baseUrl: 'https://api-gateway.example.com',
    keepAlive: true,
    dnsCacheTtlSecs: 300,
    retry: RetryConfig(
      maxAttempts: 3,
      backoff: BackoffStrategy.exponential,
    ),
  ),
);

final response = await client.get('/users/1');
```

---

## 4. 绑定策略

### 4.1 Node.js / Electron 侧：napi-rs

```
catcher-rs (Rust) → napi-rs → .node native addon → catcher (TypeScript)
```

- **选型理由**：napi-rs 是 Node.js N-API 的 Rust 封装，支持 async task、ThreadsafeFunction 回调、Buffer 零拷贝传递。已被 prisma、parcel、turbo、rspack 验证。
- **关键机制**：
  - 异步调用通过 `AsyncTask` + tokio runtime 实现，不阻塞 libuv event loop
  - WebSocket 消息推送通过 `ThreadsafeFunction` 回调到 JS 线程
  - Buffer/二进制数据通过 napi-rs 的 `Buffer` 类型零拷贝传递（不经过 JSON 序列化）
- **发布方式**：预编译 `.node` 二进制分发到 npm，用户 `npm install` 无需 Rust 工具链

### 4.2 Flutter 侧：flutter_rust_bridge

```
catcher-rs (Rust) → flutter_rust_bridge → Dart FFI → echoo-flutter (Dart)
```

- **选型理由**：flutter_rust_bridge 自动从 Rust 代码生成 Dart 绑定，支持 async/await、Stream（用于 WebSocket 事件推送）、复杂类型的自动序列化。
- **关键机制**：
  - Dart 侧调用 Rust async fn 自动转换为 `Future`
  - WebSocket 事件流通过 Rust `Stream` → Dart `Stream` 映射
  - 二进制数据通过 `Uint8List` 零拷贝传递
- **发布方式**：预编译 `.a` / `.so` 静态库分发到 pub.dev，通过 `flutter_rust_bridge` 的 codegen 生成 Dart 绑定
- **HarmonyOS 特殊处理**：Rust 通过 C FFI → HarmonyOS Native API（NAPI），flutter_rust_bridge 在 OHOS 上的适配社区已有方案

### 4.3 运行时架构（两个事件循环的协作）

```
┌─────────── Node.js ───────────┐     ┌─────────── Rust ────────────┐
│                                │     │                              │
│  libuv event loop              │     │  tokio runtime               │
│       │                        │     │       │                      │
│       │  napi::AsyncTask ──────┼────►│       │  tokio::spawn        │
│       │                        │     │       │  (多线程并发)         │
│       │                        │     │       │                      │
│       │  ThreadsafeFunction ◄──┼─────│       │  WS 消息/网络状态     │
│       │                        │     │       │  回调推送             │
│       │                        │     │                              │
└────────────────────────────────┘     └──────────────────────────────┘
```

- tokio 和 libuv 是两个独立的运行时，各自在自己的线程上调度
- HTTP 请求：JS 调用 Rust async fn → napi AsyncTask → tokio 执行 → 结果返回 JS
- WebSocket 消息：tokio 收到消息 → ThreadsafeFunction → JS callback
- 无阻塞：Rust 侧的 I/O 完全不占用 libuv 的 event loop 时间片

---

## 5. 编译与分发

### 5.1 目标平台矩阵

| 平台 | 目标三元组 | 绑定方式 | 产物 |
|------|-----------|---------|------|
| macOS x86_64 | x86_64-apple-darwin | napi-rs | .node |
| macOS arm64 | aarch64-apple-darwin | napi-rs | .node |
| Windows x86_64 | x86_64-pc-windows-msvc | napi-rs | .node |
| Linux x86_64 | x86_64-unknown-linux-gnu | napi-rs | .node |
| iOS arm64 | aarch64-apple-ios | flutter_rust_bridge | .a |
| iOS Simulator arm64 | aarch64-apple-ios-sim | flutter_rust_bridge | .a |
| Android arm64-v8a | aarch64-linux-android | flutter_rust_bridge | .so |
| Android x86_64 (Emulator) | x86_64-linux-android | flutter_rust_bridge | .so |
| HarmonyOS arm64 | aarch64-unknown-linux-ohos | flutter_rust_bridge | .so |

### 5.2 CI/CD 策略

```
GitHub Actions / GitLab CI
├── Linux runner
│   ├── x86_64-unknown-linux-gnu       → .node (Electron Linux)
│   ├── x86_64-linux-android           → .so (Android Emulator)
│   └── aarch64-linux-android          → .so (Android)
├── macOS runner
│   ├── x86_64-apple-darwin            → .node (Electron macOS Intel)
│   ├── aarch64-apple-darwin           → .node (Electron macOS ARM)
│   ├── aarch64-apple-ios              → .a (iOS)
│   └── aarch64-apple-ios-sim          → .a (iOS Simulator)
├── Windows runner
│   └── x86_64-pc-windows-msvc         → .node (Electron Windows)
└── HarmonyOS runner (自建/DevEco Studio 镜像)
    └── aarch64-unknown-linux-ohos     → .so (HarmonyOS)
```

- 使用 `cross` 或 `cargo-zigbuild` 简化交叉编译配置
- napi-rs 产物通过 `npm publish` 分发（含平台预编译 .node 文件）
- Flutter 产物通过 `pub.dev` 分发（含各平台 .a/.so 文件）

### 5.3 版本管理

```
catcher-rs (Rust) 独立版本号：v0.1.0, v0.2.0, ...
├── catcher-rs-napi (npm 包)   → 语义版本与 Rust crate 同步
└── catcher_core (pub.dev 包)    → 语义版本与 Rust crate 同步
```

Rust crate 一个版本号，两个平台的 binding 包跟随。行为保证一致。

---

## 6. 迁移路线

### Phase 1：Codec 层迁移（1-2 周，收益验证）

**范围**：只迁移 msgpack 编解码模块

```
Rust: rmp-serde 编解码
 ↓
├── napi-rs → catcher 替换 msgpackr
└── flutter_rust_bridge → echoo-flutter 新增二进制序列化能力
```

- 风险最低：无连接状态、无异步 I/O、纯函数
- 验证整个 FFI + 编译 + 分发管道的可行性
- echoo-flutter 直接获得 FL-S1（二进制序列化）能力
- catcher 获得更高效的纯 Rust msgpack 实现

### Phase 2：WebSocket 传输层迁移（3-4 周，体验质变）

**范围**：用 Rust 管理 WebSocket 连接的全生命周期

```
Rust: tokio-tungstenite + 重连状态机 + 自适应心跳 + 多端点竞速 + perMessageDeflate
 ↓
├── napi-rs → catcher 替换 ws/* 全部模块
└── flutter_rust_bridge → echoo-flutter 替换 NextWebSocketManager
```

- **最大收益**：
  - echoo-flutter 一次性获得 perMessageDeflate、多端点竞速、自适应心跳、断线原因区分（FL-W1/W3/W6/G2）
  - catcher 一次性获得自适应心跳、网络质量感知、离线队列保护
  - 两个平台 WebSocket 行为完全一致
- **风险中等**：需要 Rust 管理 TCP 连接的完整生命周期，FFI 回调路径需要仔细测试

### Phase 3：HTTP 传输层迁移（2-3 周，基础设施收敛）

**范围**：用 Rust 管理 HTTP 连接池和传输

```
Rust: reqwest/hyper + 连接池 + DNS 缓存 + 重试 + 熔断
 ↓
├── napi-rs → catcher 替换 agent/* + http/* 全部模块
└── flutter_rust_bridge → echoo-flutter 替换 callDio() 的底层传输
```

- echoo-flutter 一次性获得连接池复用、DNS 缓存、熔断器（FL-H1/H2/H3/R5）
- catcher 获得网络质量自适应的动态并发控制
- 两个平台共享同一份 TLS 配置和证书校验逻辑

### Phase 4：调度与可观测性（1-2 周，锦上添花）

- 优先级队列、请求合并、网络质量驱动的降级策略
- 统一的 metrics 上报

### 迁移过程中的兼容性保证

```typescript
// Phase 2 期间，catcher 可以同时使用新旧两种 WS 实现
import { createResilientWS } from 'catcher'        // 旧实现（纯 TS）
import { WsClient } from 'catcher-rs'             // 新实现（Rust）

// 两个实现暴露相同的业务接口，灰度切换
```

```dart
// echoo-flutter 同理
// 旧的 NextWebSocketManager 和新的 catcher_core.WsClient 可以并存
```

---

## 7. 风险与缓解

| 风险 | 概率 | 影响 | 缓解措施 |
|------|------|------|---------|
| FFI 回调性能瓶颈 | 中 | 中 | WebSocket 消息推送用零拷贝 Buffer；高频事件合批（debounce）再推送 |
| 交叉编译链不稳定 | 中 | 高 | 在 CI 中固化工具链版本；HarmonyOS 交叉编译用 Docker 镜像锁定环境 |
| Rust 异步模型学习曲线 | 高 | 中 | 核心团队 1-2 人先掌握 tokio/napi-rs；编写内部 onboarding 文档 |
| 调试困难（Rust → FFI → TS/Dart） | 中 | 中 | 在 Rust 侧建立完整的单元测试 + integration test；FFI 层打详细日志 |
| 构建时间增加 | 高 | 低 | Rust 编译时间可控（< 2min incremental）；预编译二进制分发给非核心开发者 |
| 两套 event loop 资源竞争 | 低 | 中 | tokio runtime 配置独立的线程池，不与 libuv 共享 worker 线程 |

---

## 8. 工作量估算

| Phase | 内容 | Rust 核心 | napi 绑定 | Dart 绑定 | 平台适配 | 测试 | 合计 |
|-------|------|----------|----------|----------|---------|------|------|
| 1 — Codec | msgpack 编解码 | 3d | 1d | 1d | 1d | 2d | **8d** |
| 2 — WebSocket | 传输+重连+心跳+竞速 | 10d | 4d | 4d | 4d | 6d | **28d** |
| 3 — HTTP | 连接池+DNS+重试+熔断 | 8d | 3d | 3d | 4d | 5d | **23d** |
| 4 — 调度/可观测 | 队列+质量评估+metrics | 5d | 2d | 2d | 2d | 3d | **14d** |
| CI/CD + 文档 | 编译 + 分发 + 上手指南 | 5d | 2d | 2d | — | — | **9d** |
| **总计** | | **31d** | **12d** | **12d** | **11d** | **16d** | **~82 人天** |

> 以上为 1 人全职估算。建议 2 人并行：一人主攻 Rust 核心 + napi，一人主攻 Rust 核心 + Dart 绑定。预计 **8-10 周** 完成全部 4 个 Phase，Phase 1 独立可在 **2 周** 内出 PoC。

---

## 9. 决策建议

### 推荐路径：Phase 1 PoC → 决策关口 → 全量推进

```
Week 1-2:  Phase 1 Codec PoC
           ├── 验证 FFI 管道可行性
           ├── 验证交叉编译到 iOS/Android
           ├── 拿到真实性能对比数据
           └── 决策关口：Go / No-Go
                         │
                     Go  │
                         ▼
Week 3-10: Phase 2-4 全量推进
```

### 如果 PoC 验证失败或团队决定不走 Rust

备选方案：

- **方案 B**：catcher 继续纯 TypeScript 演进 + echoo-flutter 独立用 Dart 补齐缺失能力（但两份代码长期不一致）
- **方案 C**：只把 Codec 层（Phase 1）用 Rust，传输层维持各平台原生方案（收益降低但风险可控）

---

## 10. 参考

- 现有 catcher 设计文档：`docs/network-kit-design.md`
- echoo-flutter 网络需求：`echoo-flutter/docs/research/weak-network-requirements.md`
- echoo-flutter 代码分析：`echoo-flutter/docs/research/weak-network-codebase-analysis.md`
- catcher 已知问题清单：`docs/issues/README.md`
- napi-rs: https://napi.rs
- flutter_rust_bridge: https://cjycode.com/flutter_rust_bridge

---

## 附录 A：Rust 开源库调研

> 调研日期：2025-07-18
> 目标：评估各模块是否有成熟开源库可直接使用，减少造轮子周期

### A.1 总览：推荐库清单

| 模块 | 推荐库 | 版本 | 下载量 | 自研必要性 | 说明 |
|------|--------|------|--------|-----------|------|
| 异步运行时 | **tokio** | 1.x | 150M+ | ❌ 零 | 事实标准 |
| HTTP 客户端 | **reqwest** | 0.12 | 120M+ | ❌ 零 | 基于 hyper，自带连接池/TLS/HTTP2 |
| HTTP 连接池 | **hyper-util pool** | 0.1 | — | ❌ 零 | reqwest 内置，可独立配置 |
| WebSocket | **stream-tungstenite** | 0.6 | ~5K | ❌ 低 | 自带重连/退避/状态管理 |
| 底层 WS 协议 | **tokio-tungstenite** | 0.24 | 30M+ | ❌ 零 | stream-tungstenite 的底层依赖 |
| TLS | **rustls** | 0.23 | 50M+ | ❌ 零 | 纯 Rust，跨平台一致 |
| DNS 缓存 | **hickory-resolver** | 0.25 | 10M+ | ❌ 零 | 进程内 DNS 解析+缓存 |
| 重试策略 | **backon** | 1.x | 5M+ | ❌ 低 | 指数退避+jitter+自定义策略 |
| 重试策略(备选) | **retry-policies** | 0.4 | 2M+ | ❌ 低 | 配合 reqwest-retry 使用 |
| HTTP 重试中间件 | **reqwest-retry** | 0.7 | 5M+ | ❌ 低 | reqwest 中间件，直接可用 |
| 熔断器 | **circuitbreaker-rs** | 0.1 | 330K | ⚠️ 可能 | 新库，下载量低但 API 完整 |
| 熔断器(备选) | **tower-resilience** | 0.8 | — | ⚠️ 可能 | Tower Service 中间件体系 |
| 序列化 | **rmp-serde** | 1.x | 15M+ | ❌ 零 | msgpack 事实标准 |
| 并发控制 | **tokio::sync::Semaphore** | 内置 | — | ❌ 零 | 标准库级别 |
| Ping/ICMP | **tokio-ping** | 0.5 | 500K | ⚠️ 可能 | ICMP ping，但需要 root / cap |
| Ping(备选) | **surfping** / 自研 HTTP ping | — | — | ⚠️ 中等 | HTTP HEAD 测 RTT 更便携，无需权限 |

### A.2 各模块详细调研

#### A.2.1 HTTP 传输层

**推荐方案：reqwest + reqwest-middleware + reqwest-retry**

```
reqwest (超时/连接池/TLS/DNS 委托给 hickory)
  └── reqwest-middleware (中间件链)
       ├── reqwest-retry (重试中间件)
       │    └── retry-policies (退避策略：指数/固定/抖动)
       └── 自研熔断中间件 (circuitbreaker-rs 包装成 reqwest middleware)
```

- **reqwest** 自身功能：
  - 连接池：内置，通过 `ClientBuilder::pool_max_idle_per_host()` 配置
  - keep-alive：默认启用
  - HTTP/2：通过 `http2_prior_knowledge()` 或 ALPN 协商
  - TLS：支持 rustls（推荐跨平台）和 native-tls
  - 超时：`connect_timeout()` / `timeout()` / `pool_idle_timeout()`
  - DNS：默认使用系统 DNS，可替换为 hickory-resolver
- **reqwest-retry**：
  - 可配置哪些 HTTP status / 错误类型触发重试
  - 读超时（read timeout）和瞬时错误自动重试
  - 配合 `retry-policies` 提供指数退避+jitter
  - 自带 `default_on_request_failure` 和 `on_5xx` 等预置策略
- **reqwest-middleware**：提供类似 axios interceptor 的洋葱模型

**自研量评估：极低**
- reqwest 开箱即用，一个 `ClientBuilder` 覆盖 catcher 90% 的 agent/client.ts 代码
- reqwest-retry 覆盖 retry.ts
- 唯一需要自研：将熔断器封装为 reqwest middleware（~50 行代码）

#### A.2.2 WebSocket 传输层

**推荐方案：stream-tungstenite**

- **stream-tungstenite** 是一个相对新但功能完备的库（v0.6.1, Jan 2026）：
  - ✅ 自动重连：支持指数退避等多种重试策略
  - ✅ 连接状态管理：实时追踪 CONNECTING / CONNECTED / DISCONNECTED / RECONNECTING
  - ✅ 扩展系统：可 hook 生命周期事件和消息处理
  - ✅ 应用层握手：支持 auth / subscribe 等自定义握手
  - ✅ 背压感知发送：有界发送队列（容量可配置），支持非阻塞/阻塞/超时三种模式
  - ✅ Builder 模式 API：链式配置
  - ❌ 未内置 perMessageDeflate 配置（需确认底层 tungstenite 是否支持）
  - ❌ 未内置多端点竞速（需自研）

- **底层依赖**：tokio-tungstenite → tungstenite-rs，后者支持 `deflate` feature 开启 `permessage-deflate` 扩展

**自研量评估：中等**
- stream-tungstenite 覆盖重连/心跳/状态管理（对应 catcher 的 ws/ 模块 ~60% 代码）
- 需要自研：
  - 多端点竞速连接器（~150 行）：对多个 endpoint 同时发起 stream-tungstenite 连接，取最先 ready 者
  - 自适应心跳（~100 行）：基于 WS ping/pong RTT 动态调整心跳间隔
  - perMessageDeflate 配置适配（~30 行）：启用 tungstenite 的 deflate feature

#### A.2.3 重试与退避

**推荐方案：backon**

- **backon** (v1.0+) 设计简洁，API 符合人体工学：
  ```rust
  use backon::{Retryable, ExponentialBuilder};
  
  let result = (|| async { reqwest::get("https://api.example.com").await })
      .retry(ExponentialBuilder::default())
      .sleep(tokio::time::sleep)
      .when(|e| e.status() == Some(StatusCode::INTERNAL_SERVER_ERROR))
      .await;
  ```
  - ✅ 指数退避：自带 jitter（默认 decorrelated jitter）
  - ✅ 常量退避
  - ✅ 自定义退避策略（实现 `BackoffBuilder` trait）
  - ✅ 条件重试：`when()` 闭包按需决定是否重试
  - ✅ 通知回调：`notify()` 在每次重试时触发日志/上报
  - ✅ 同时支持阻塞和异步 API

**备选方案：retry-policies + reqwest-retry**
- 如果已在 HTTP 层使用 reqwest-middleware，直接用 reqwest-retry 更自然
- 非 HTTP 的重试场景（如 DNS 解析、WebSocket 连接）用 backon

**自研量评估：极低**
- backon 覆盖 catcher 的 retry.ts 和 echoo-flutter 的 dio_smart_retry 配置
- 唯一需要自研：`retry_if` 的业务条件组合器（但 Rust 闭包`when()`已覆盖）

#### A.2.4 熔断器

**推荐方案：circuitbreaker-rs（备选：tower-resilience）**

- **circuitbreaker-rs**：
  ```rust
  use circuitbreaker_rs::CircuitBreaker;
  
  let cb = CircuitBreaker::builder()
      .failure_threshold(5)
      .success_threshold(2)
      .half_open_timeout(Duration::from_secs(30))
      .build();
  
  let result = cb.call(|| async { client.get("/api").await }).await;
  ```
  - ✅ 三态：Closed / Open / HalfOpen
  - ✅ 滑动窗口计数（非固定窗口）
  - ✅ 半开状态探测定量请求
  - ✅ 状态变更回调（用于可观测性）
  - ⚠️ 新库（v0.1.1），下载量 330K（偏低但增长快）

- **tower-resilience** (v0.8+)：
  - ✅ 完整 Resilience4j 风格：circuit-breaker + retry + bulkhead + rate-limit + concurrency-limit
  - ✅ Tower Service 体系（如果使用 hyper 底层可用 Tower 层层包装）
  - ❌ 绑定 Tower 生态，如果上层用 reqwest 需要额外适配

**推荐理由**：如果 HTTP 用 reqwest + reqwest-middleware，选 circuitbreaker-rs 包装为 middleware（~50 行）。如果未来需要更完整的 resilience 组合（bulkhead + rate-limit + concurrency-limit），考虑 tower-resilience。

**自研量评估：低**
- circuitbreaker-rs API 与 catcher 的 cockatiel 接近，功能完备
- 唯一需要自研：封装为 reqwest Middleware trait 实现（~50 行）

#### A.2.5 DNS 缓存

**推荐方案：hickory-resolver**

```rust
use hickory_resolver::TokioAsyncResolver;

let resolver = TokioAsyncResolver::builder_tokio()
    .cache_size(512)
    .positive_ttl(Some(Duration::from_secs(300)))  // 成功记录缓存 5min
    .negative_ttl(Some(Duration::from_secs(60)))    // 失败记录缓存 1min
    .build()?;

// 注入到 reqwest
let client = reqwest::Client::builder()
    .dns_resolver(Arc::new(resolver))
    .build()?;
```

- ✅ 100% 进程内 DNS 解析，不依赖操作系统 DNS
- ✅ DNS 记录缓存（positive + negative），TTL 可配置
- ✅ 支持 DNS over HTTPS / DNS over TLS
- ✅ 可配置并行查询多个 nameserver
- ✅ 异步（tokio 版本）
- ⚠️ reqwest 的 `dns_resolver()` 方法需要启用 `hickory-dns` feature

**自研量评估：极低**
- hickory-resolver 替换 catcher 的 `cacheable-lookup`，功能更强
- echoo-flutter 当前无 DNS 缓存，引入直接补齐 FL-H2 需求

#### A.2.6 序列化

**推荐方案：rmp-serde（备选：zerompk）**

- **rmp-serde**：Rust msgpack 生态的事实标准
  - ✅ serde 兼容，零样板代码
  - ✅ 成熟稳定（v1.x, 15M+ 下载）
  - ✅ 支持 `#[serde]` 宏自动派生编解码
- **zerompk**：新库，声称 2-4x 快于 rmp-serde
  - ⚠️ 无 std 依赖，API 与 serde 不完全兼容
  - ⚠️ 较新（2024 年发布），生态不如 rmp-serde

**推荐 rmp-serde**，稳定性优先。性能差异对 catcher 场景不构成瓶颈。

**自研量评估：极低**
- rmp-serde 直接替换 catcher 的 msgpackr + echoo-flutter 缺失的二进制序列化能力

#### A.2.7 网络质量评估

**推荐方案：tokio-ping + 自研 HTTP HEAD RTT 测量**

- **tokio-ping**：异步 ICMP ping
  - ✅ 可测量真实网络 RTT
  - ❌ 需要 raw socket 权限（Linux `CAP_NET_RAW` / macOS root）
  - ❌ 移动端（iOS/Android）不可用

- **备选方案：HTTP HEAD RTT 测量**（自研，~50 行）
  ```rust
  let start = Instant::now();
  let resp = client.head("https://api.example.com/health").send().await?;
  let rtt = start.elapsed();
  ```
  - ✅ 跨平台零权限
  - ✅ 与实际业务请求使用相同协议栈，RTT 更准确
  - ⚠️ 包含服务端处理时间（但 health check endpoint 通常 <5ms）

**综合推荐**：Desktop 端（Electron）用 tokio-ping（有权限），Mobile 端用 HTTP HEAD RTT。统一封装为 `NetworkQualityEvaluator` trait，按平台选择实现。

**自研量评估：中低**
- RTT 采集器自研 ~100 行
- 综合评估逻辑（RTT + jitter + 丢包率 → excellent/good/fair/poor）自研 ~150 行

#### A.2.8 优先级队列与并发控制

**推荐方案：tokio::sync::Semaphore（内置）**

```rust
use tokio::sync::Semaphore;
use std::sync::Arc;

let limiter = Arc::new(Semaphore::new(10));  // 最多 10 并发

// 发送消息（高优先级）
let permit = limiter.acquire().await?;
tokio::spawn(async move {
    let _permit = permit;
    send_message().await;
});

// 头像加载（低优先级）
let permit = limiter.acquire().await?;  // 同一个信号量控制
```

- ✅ tokio 内置，零额外依赖
- ✅ `Semaphore::acquire()` 是 FIFO 顺序
- ⚠️ 不支持优先级插队（需要自研）

**如果需要真正的优先级队列**：
- 方案 A：`tokio::sync::Semaphore` + 多个 `mpsc::channel`（不同优先级不同 channel）
- 方案 B：使用 `priority-queue` crate（非异步 native，需要配合 tokio）
- 方案 C：自研（~100 行），两个 `mpsc::channel`（high/low），优先从 high channel 取任务

**推荐方案 B/C**，取决于优先级粒度。catcher 当前只有 2-3 个优先级，自研很轻量。

**自研量评估：低**
- 单并发槽位：0 行（Semaphore 内置）
- 2 级优先级队列：~100 行
- 动态槽位（基于网络质量调整并发数）：~80 行

### A.3 自研工作量汇总

基于以上库选型，与原计划（纯手写）对比：

| 模块 | 原估算 | 用库后 | 节省 | 说明 |
|------|--------|--------|------|------|
| HTTP 客户端 + 连接池 | 8d | **2d** | -6d | reqwest 内置 |
| HTTP 重试 | 包含在上项 | **0.5d** | — | reqwest-retry 直接可用 |
| WebSocket 传输 | 10d | **5d** | -5d | stream-tungstenite 覆盖 60% |
| 重连/退避策略 | 包含在上项 | **1d** | — | stream-tungstenite + backon |
| 熔断器 | 包含在 HTTP | **1d** | — | circuitbreaker-rs |
| 优先级队列 | 包含在 Phase 4 | **1d** | — | tokio Semaphore + 简单封装 |
| DNS 缓存 | 包含在 HTTP | **0.5d** | — | hickory-resolver 即插即用 |
| 序列化 | 3d | **0.5d** | -2.5d | rmp-serde 开箱即用 |
| 网络质量评估 | 包含在 Phase 4 | **3d** | — | ping + HTTP HEAD + 综合评分逻辑 |
| 多端点竞速 | 包含在 WS | **2d** | — | 需自研，stream-tungstenite 不内置 |
| **Phase 1-4 合计** | **~31d** | **~16.5d** | **-14.5d (47%)** | Rust 核心层 |

**总工作量从 82 人天降至 ~55 人天（节省 33%）。** 4 个 Phase 预计 **6-7 周**（2 人并行）。

### A.4 唯一需要完全自研的模块

| 模块 | 理由 | 估算 |
|------|------|------|
| 多端点竞速 | 无现成库，需对多个 stream-tungstenite 实例做竞速 | ~150 行 |
| 自适应心跳 | 需在 WS 心跳上采集 RTT 并动态调整间隔 | ~100 行 |
| 网络质量综合评分 | 需整合 ping RTT + jitter + 连接类型 → 5 级评分 | ~150 行 |
| 动态并发控制 | 基于网络质量评分动态调整 Semaphore permit 数 | ~80 行 |
| perMessageDeflate 适配 | stream-tungstenite 底层 tungstenite 已支持，需配置封装 | ~30 行 |
