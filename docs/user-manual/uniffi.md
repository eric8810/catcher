# UniFFI 使用指南（Swift / Kotlin）

> 状态：✅ 已发布 — `catcher-uniffi` crate
> 代码位置：`packages/catcher-uniffi/`
> 目标平台：iOS (Swift) / Android (Kotlin)

---

## 一、概述

`catcher-uniffi` 使用 Mozilla UniFFI 0.28 proc-macro 模式（无需 UDL 文件）生成 Swift 和 Kotlin 绑定。底层 HTTP 引擎为 Rust `reqwest`，自带重试、熔断器、自适应超时。

**架构要点**：
- UniFFI 0.28 不支持 async，所有异步操作通过 `block_on_aux_thread()` 在独立线程同步等待
- 回调不会触发 `block_on()` 重入 panic（使用独立 tokio runtime）

---

## 二、构建

### 2.1 编译 Rust 库

```bash
# Android
cargo ndk -t arm64-v8a -t armeabi-v7a -t x86_64 -o ./jniLibs build --release -p catcher-uniffi

# iOS
cargo lipo --release -p catcher-uniffi
```

### 2.2 生成绑定代码

```bash
# Swift (iOS)
uniffi-bindgen generate \
  --library ../target/release/libcatcher_uniffi.dylib \
  --language swift \
  --out-dir generated/swift

# Kotlin (Android)
uniffi-bindgen generate \
  --library ../target/release/libcatcher_uniffi.so \
  --language kotlin \
  --out-dir generated/kotlin
```

---

## 三、HTTP 客户端

### 3.1 创建

```swift
// Swift
let configJson = """
{
  "base_url": "https://api.example.com",
  "retry": { "max_attempts": 3, "backoff": "Exponential" },
  "circuit_breaker": { "failure_threshold": 5, "reset_timeout_ms": 30000 }
}
"""
let client = try HttpClient(configJson: configJson)
```

```kotlin
// Kotlin
val configJson = """
{
  "base_url": "https://api.example.com",
  "retry": { "max_attempts": 3 },
  "circuit_breaker": { "failure_threshold": 5 }
}
"""
val client = HttpClient(configJson = configJson)
```

### 3.2 GET 请求

```swift
// Swift
let response = try client.get(
    url: "/users/1",
    headersJson: "{\"Authorization\": \"Bearer token\"}",
    timeoutMs: 5000
)
print("Status: \(response.status)")
print("Body: \(String(data: Data(response.body), encoding: .utf8) ?? "")")
```

```kotlin
// Kotlin
val response = client.get(
    url = "/users/1",
    headersJson = """{"Authorization": "Bearer token"}""",
    timeoutMs = 5000
)
println("Status: ${response.status}")
println("Body: ${String(response.body, Charsets.UTF_8)}")
```

### 3.3 POST 请求

```swift
// Swift
let body = "{\"message\": \"hello\"}".data(using: .utf8)!
let response = try client.post(
    url: "/messages",
    body: [UInt8](body),
    contentType: "application/json",
    headersJson: nil,
    timeoutMs: nil
)
```

```kotlin
// Kotlin
val body = """{"message": "hello"}""".toByteArray(Charsets.UTF_8)
val response = client.post(
    url = "/messages",
    body = body.toList(),
    contentType = "application/json",
    headersJson = null,
    timeoutMs = null
)
```

### 3.4 PUT / DELETE / PATCH

API 同 `get/post`，方法名对应 `put` / `delete` / `patch`。

### 3.5 运行时控制

```swift
// 取消所有请求
client.cancelAll()

// 查看熔断器状态 (JSON)
let cbState = client.circuitBreakerState()

// 查看指标 (JSON)
let metrics = client.metrics()

// 配置自适应超时
client.setAdaptiveTimeout(
    enabled: true,
    minTimeoutMs: 100,
    maxTimeoutMs: 30000,
    multiplier: 2500,  // 2.5 × 1000
    windowSize: 20
)
```

---

## 四、WebSocket 客户端

### 4.1 连接

```swift
// Swift
let observer = MyWsObserver()  // 实现 WsEventObserver 协议
let wsClient = try WsClient(
    configJson: """
    {
      "urls": ["wss://echo.example.com"],
      "reconnect": { "initial_delay_ms": 1000, "max_attempts": 20 },
      "heartbeat": { "interval_ms": 30000 }
    }
    """,
    observer: observer
)
```

### 4.2 事件观察者

```swift
// Swift
class MyWsObserver: WsEventObserver {
    func onConnected(url: String, latencyMs: UInt64) {
        print("Connected: \(url) (\(latencyMs)ms)")
    }

    func onMessage(data: [UInt8], isBinary: Bool) {
        if isBinary {
            // 处理二进制消息
        } else {
            let text = String(bytes: data, encoding: .utf8) ?? ""
            print("Message: \(text)")
        }
    }

    func onDisconnected(code: UInt16, reason: String) {
        print("Disconnected: \(code) \(reason)")
    }

    func onError(message: String) {
        print("Error: \(message)")
    }

    func onReconnecting(attempt: UInt64, delayMs: UInt64) {
        print("Reconnecting #\(attempt) in \(delayMs)ms")
    }

    func onHeartbeatRtt(rttMs: UInt64) {
        print("Heartbeat RTT: \(rttMs)ms")
    }
}
```

```kotlin
// Kotlin
class MyWsObserver : WsEventObserver {
    override fun onConnected(url: String, latencyMs: ULong) { ... }
    override fun onMessage(data: List<UByte>, isBinary: Boolean) { ... }
    override fun onDisconnected(code: UShort, reason: String) { ... }
    override fun onError(message: String) { ... }
    override fun onReconnecting(attempt: ULong, delayMs: ULong) { ... }
    override fun onHeartbeatRtt(rttMs: ULong) { ... }
}
```

### 4.3 发送消息

```swift
// Swift
try wsClient.sendText("hello")
try wsClient.sendBinary([0x01, 0x02, 0x03])
try wsClient.close(code: 1000, reason: "normal")
```

```kotlin
// Kotlin
wsClient.sendText("hello")
wsClient.sendBinary(listOf(0x01.toUByte(), 0x02.toUByte()))
wsClient.close(code = 1000.toUShort(), reason = "normal")
```

---

## 五、SSE 客户端

### 5.1 创建持久连接

```swift
// Swift
let observer = MySseObserver()
let sseClient = try SseClient(
    configJson: """
    {
      "url": "https://api.example.com/events",
      "headers": { "Authorization": "Bearer token" },
      "reconnect": { "initial_delay_ms": 1000, "max_delay_ms": 30000 }
    }
    """,
    observer: observer
)
```

### 5.2 SSE 事件观察者

```swift
class MySseObserver: SseEventObserver {
    func onOpen() { print("SSE connected") }
    func onData(event: String?, data: String) { print("SSE: \(data)") }
    func onError(message: String) { print("SSE error: \(message)") }
    func onClose() { print("SSE closed") }
}
```

### 5.3 One-shot SSE Stream

```swift
let events = try sseClient.stream(
    method: "POST",
    url: "/v1/chat/completions",
    body: "{\"model\":\"gpt-4\",\"stream\":true}".data(using: .utf8).map { [UInt8]($0) },
    headersJson: "{\"Authorization\": \"Bearer sk-xxx\"}"
)
for event in events {
    if let dataEvent = event as? SseDataEvent {
        print(dataEvent.data)
    }
}
```

---

## 六、配置参考

### HttpClientConfig (JSON)

```json
{
  "base_url": "https://api.example.com",
  "connect_timeout_ms": 10000,
  "response_timeout_ms": 30000,
  "pool": {
    "keep_alive": true,
    "max_idle_per_host": 10,
    "idle_timeout_secs": 60
  },
  "retry": {
    "max_attempts": 3,
    "backoff": "Exponential",
    "min_backoff_ms": 500,
    "max_backoff_ms": 30000,
    "jitter": true
  },
  "circuit_breaker": {
    "failure_threshold": 5,
    "success_threshold": 2,
    "reset_timeout_ms": 30000,
    "half_open_max_requests": 5
  },
  "tls": {
    "reject_unauthorized": true
  },
  "dns": {
    "cache_ttl_secs": 300,
    "host_mapping": { "api.example.com": "10.0.0.5" }
  },
  "max_concurrency": 50,
  "default_headers": {}
}
```

### WsClientConfig (JSON)

```json
{
  "urls": ["wss://primary.example.com", "wss://fallback.example.com"],
  "headers": {},
  "protocols": [],
  "per_message_deflate": true,
  "reconnect": {
    "initial_delay_ms": 1000,
    "max_delay_ms": 30000,
    "backoff_multiplier": 2.0,
    "max_attempts": 20
  },
  "heartbeat": {
    "interval_ms": 30000,
    "pong_timeout_ms": 10000
  }
}
```

---

## 七、已知限制

| 限制 | 说明 |
|------|------|
| 无 async | UniFFI 0.28 不支持 async，所有操作同步阻塞 |
| 无 multipart | Rust 侧无 multipart 编码器，文件上传需自行编码 |
| 无 stream download | 流式下载未导出到 UniFFI（Dart FFI 已支持） |
| headers 格式 | UniFFI Record 不支持 Map，headers 为 `"key: value"` 字符串数组 |

---

## 八、错误处理

```swift
// Swift
do {
    let response = try client.get(url: "/test", headersJson: nil, timeoutMs: nil)
} catch let error as CatcherError {
    switch error {
    case .Network(let message):
        print("Network error: \(message)")
    case .Config(let message):
        print("Config error: \(message)")
    }
}
```

```kotlin
// Kotlin
try {
    val response = client.get(url = "/test")
} catch (e: CatcherError) {
    when (e) {
        is CatcherError.Network -> println("Network: ${e.message}")
        is CatcherError.Config -> println("Config: ${e.message}")
    }
}
```
