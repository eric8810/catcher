# Android / iOS / Web 平台使用方案调研

> 调研时间：2026-05-12
> 前置决策：Dart/Flutter 使用 dart:ffi，不通过 flutter_rust_bridge

---

## 一、Android (Kotlin/Java 原生)

### 方案 A：UniFFI（推荐，与 iOS 共享同一套绑定）

[Mozilla UniFFI](https://github.com/mozilla/uniffi-rs) 从 Rust API 自动生成 Kotlin + Swift 绑定。

```
Rust API (uniffi proc-macro)
      │ 一套定义
      ├──→ Swift bindings (iOS)
      └──→ Kotlin bindings (Android)
```

```rust
// Rust 侧 — 用 UniFFI 宏标注，自动生成 Kotlin
#[uniffi::export]
pub fn create_http_client(config_json: String) -> Arc<HttpClient> {
    Arc::new(HttpTransport::new(serde_json::from_str(&config_json)?).unwrap())
}

#[uniffi::export]
pub async fn http_get(client: Arc<HttpClient>, url: String) -> HttpResponseDto {
    client.execute(HttpRequest::get(&url)).await.into()
}
```

```kotlin
// Kotlin 侧 — UniFFI 自动生成，类型安全
val client = CatcherHttp.createHttpClient(configJson)
val resp = client.httpGet("/channels")
println("${resp.status}: ${resp.body}")
```

### 方案 B：JNI（手动，完全控制）

```
Kotlin/Java App
      │ JNI (Java Native Interface)
      ▼
libcatcher_android.so (Rust cdylib)
      │ jni crate
      ▼
catcher-rs (Rust 核心)
```

### 对比

| | UniFFI ✅ | JNI 手动 ❌ |
|--|-----------|------------|
| 绑定生成 | 自动（proc-macro） | 手写 extern "system" |
| 类型安全 | ✅ 自动映射 Vec<T>, HashMap 等 | ❌ jlong 句柄 + JSON 序列化 |
| async 支持 | ✅ 自动生成 suspend | ❌ 手动回调桥接 |
| 与 iOS 的复用 | ✅ 同一套定义 | ❌ iOS 需另写 C ABI |
| 包体积 | +200KB (运行时) | 零额外 |

### 可行性

- ✅ Rust→JNI 是成熟的路径（`jni` crate 活跃维护）
- ✅ 可以复用同一套 `catcher-rs` 核心
- ⚠️ JNI 边界每次调用都有序列化开销（JString ↔ Rust String）
- ⚠️ 异步桥接需要手动管理（suspend ↔ tokio）
- 🟡 **建议优先级：低** — 大部分 Android 项目可通过 Flutter Module 间接使用，纯原生需求较少

---

## 二、iOS (Swift 原生)

### 方案 A：UniFFI（推荐，与 Android 共享同一套绑定）

与 Android 共用同一套 Rust API 定义，UniFFI 自动生成 Swift 绑定。

```swift
// Swift 侧 — UniFFI 自动生成
import CatcherCore

let client = CatcherHttp.createHttpClient(configJson: jsonString)
let resp = try await client.httpGet(url: "/channels")
print("\(resp.status): \(resp.body)")
```

### 方案 B：C ABI 手动（与 dart:ffi 共享）

手写 `extern "C"`，Swift 通过 C Interop 直接调用，SPM 分发。与 dart:ffi 共用同一套 C ABI。

### 对比

| | UniFFI ✅ | C ABI 手动 |
|--|-----------|-----------|
| 绑定生成 | 自动 | 手写 |
| 与 Android 复用 | ✅ 同一套定义 | ❌ Android 需另写 JNI |
| async 支持 | ✅ 自动 | ❌ 手动回调桥接 |
| 与 dart:ffi 共用 | ❌ 不同 ABI | ✅ 同一套 extern "C" |
    ├── libcatcher_core_ios.a      // aarch64-apple-ios
    └── libcatcher_core_sim.a      // aarch64-apple-ios-sim
```

Mozilla 有成熟实践：[Shipping Rust Components as Swift Packages](https://mozilla.github.io/application-services/book/design/swift-package-manager.html)。

### 可行性

- ✅ Swift 直接调用 C 函数，零额外依赖
- ✅ 可以复用同一套 C ABI（与 dart:ffi / napi-rs 完全一致）
- ✅ SPM 支持预编译 .a 分发
- ⚠️ 异步需要回调→async/await 桥接（类似 dart:ffi 的 ReceivePort 模式）
- ⚠️ 需要维护双架构（arm64 + simulator）预编译
- 🟡 **建议优先级：中** — iOS 纯原生需求比 Android 多

---

## 三、Web (Browser)

### 核心限制

WASM **不能直接访问网络**。编译到 WASM 的 Rust 代码没有 socket 权限，无法使用 `reqwest`、`tokio-tungstenite` 等。

```
❌ Rust (WASM) → reqwest → TCP socket   // 不行，WASM 没有 socket
✅ Rust (WASM) → web_sys::fetch()       // 可行，但走 JS bridge
✅ JS/TS → fetch() → HTTP                // 原生支持
```

### 两种方案

**方案 A：TS 纯实现**（推荐）

```
@catcher/web 包
  ├── 复用 @catcher/core 类型
  ├── fetch-based HTTP 客户端（替代 axios）
  ├── 内建 retry / CB / queue（纯 TS，复用 p-retry / cockatiel）
  └── WebSocket 基于浏览器 WebSocket API
```

```typescript
// @catcher/web
import { createWebClient } from '@catcher/web'

const client = createWebClient({
  baseURL: 'https://api.example.com',
  retry: { attempts: 3 },
  circuitBreaker: { failureThreshold: 5, resetTimeout: 30_000 },
})

// 底层是 fetch()，API 与 @catcher/http 一致
const data = await client.get('/users/1')
```

优点：
- ✅ 零依赖 Rust/WASM，包体积小
- ✅ 直接复用 TypeScript 韧性逻辑
- ✅ 浏览器原生 fetch + WebSocket

**方案 B：Rust → WASM（仅编解码/韧性逻辑）**

```
Rust (WASM)
  ├── pack/unpack (msgpack)   ✅ 纯计算
  ├── retry backoff 计算      ✅ 纯计算
  ├── CB 状态机               ✅ 纯计算
  └── HTTP 请求               ❌ 通过 JS bridge → fetch
```

```typescript
import init, { pack, unpack, RetryCalculator } from '@catcher/wasm'

await init()
const retry = new RetryCalculator({ maxAttempts: 3, backoff: 'exponential' })
const delay = retry.nextDelay()  // 指数退避计算在 WASM 内
```

优点：
- ✅ 编解码性能（真正的 Rust msgpack）
- ❌ 实际网络收发仍然走 JS fetch
- ❌ 引入 WASM 复杂度（异步加载、跨线程限制）
- 🟡 **仅编解码/韧性逻辑有价值，完整 HTTP 客户端不值得**

### 决策

| | 方案 A (纯 TS) | 方案 B (WASM) |
|--|:--:|:--:|
| 网络收发的韧性层 | ✅ | ❌ 还是要走 fetch |
| 编解码 | 🟡 JS msgpackr 够用 | ✅ Rust msgpack 更快 |
| 包体积 | ~30KB | ~200KB+ (WASM) |
| 复杂度 | 低 | 高 |
| **推荐** | ✅ | 🟡 仅 codec 场景 |

**结论**：Web 平台推荐**纯 TS 方案**。@catcher/http 的 axios 替换为 fetch，韧性层（retry/CB/queue）直接复用 p-retry + cockatiel，不引入 Rust/WASM。

---

## 四、平台全景总结

| 平台 | 网络层 | 韧性层 | 编解码 | 方案 | 优先级 |
|------|--------|--------|--------|------|--------|
| **Node.js (native)** | reqwest | catcher-rs | msgpack | ✅ `@catcher/napi-http` — Rust via napi-rs, 已编译 | P0 |
| **Node.js (TS)** | axios | p-retry/cockatiel | msgpackr | ✅ `@catcher/http` — API 更丰富（拦截器等） | P0 |
| **Rust crate** | reqwest | catcher-rs | msgpack | ✅ catcher-http/ws/core, 34 源文件, 未发布 | P0 |
| **Electron** | 同 Node.js | 同 Node.js | 同 Node.js | ✅ 直接用 | P0 |
| **Android + iOS** | reqwest | catcher-rs | msgpack | 📋 UniFFI → Swift + Kotlin | P1 |
| **Flutter** | reqwest | catcher-rs | msgpack | 📋 dart:ffi → C ABI（已实现，绑定待写）| P2 |
| **Web** | fetch (TS) | p-retry/cockatiel | msgpackr | ⚠️ `@catcher/web` — **唯一缺失的平台** | P1 |

> **Node.js**：双轨。napi native 是生产方案，TS 版提供更丰富的 API（拦截器、per-request options）。  
> **Rust crate**：已实现但未发布到 crates.io。  
> **Web**：唯一真正缺失的平台。WASM 无 socket，必须纯 TS + fetch。  
> **Android + iOS / Flutter**：Rust 核心已完成，绑定层待写。

## 五、建议优先级

| 优先级 | 平台 | 理由 |
|--------|------|------|
| P0 | Node.js ✅ | napi native + TS 双轨均已完成 |
| P0 | Rust crate ✅ | catcher-core/http/ws 已实现 |
| P1 | Web (`@catcher/web`) | 唯一缺失的平台，TS + fetch |
| P1 | UniFFI (Android + iOS) | Rust 核心已有，加 proc-macro 即可 |
| P2 | Flutter (dart:ffi) | C ABI 已有，dart:ffi 绑定层待写 |
