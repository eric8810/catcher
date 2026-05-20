# catcher-napi-http/ws 内置 msgpack codec

## 严重程度：P1

当前 msgpack 编解码要么走 JS 侧 `msgpackr`（与 NAPI 无关），要么走手动暴露的 `pack/unpack` NAPI 函数（跨边界开销 ~6x）。两种方式都没有利用 Rust transport 层的零拷贝优势。应该将 codec 内置到 transport 层，通过配置开关自动对所有 body/message 编解码。

## 现状

### HTTP

```
JS: JSON.stringify(body) → Buffer
     ↓ (跨 NAPI 边界)
Rust: 原样发送 Buffer → wire (JSON bytes)
     ↓
Server 返回 JSON bytes
     ↓ (跨 NAPI 边界)
JS: Buffer.toString() → JSON.parse()
```

请求体始终是 JSON，没有 msgpack 压缩。

### WS

```
JS: ws.send(JSON.stringify(obj)) → text frame
     ↓ (跨 NAPI 边界)
Rust: 原样发送 text frame → wire
```

消息始终是 JSON text frame，没有 binary msgpack 压缩。

### 手动 pack/unpack（本次 DNS PR 新增）

```
JS object
  → napi-rs serde_json::Value 转换（完整遍历 JS 对象树）
    → Rust rmp_serde::to_vec
      → Buffer 回 JS
```

Benchmark 结果：Rust rmp-serde via NAPI 比 JS msgpackr **慢 ~6x**。

| payload | JS msgpackr | Rust NAPI pack | 差距 |
|---|---|---|---|
| 300B encode | 2.33M/s | 404K/s | 5.8x |
| 20KB encode | 118K/s | 17K/s | 6.9x |
| 500KB encode | 2.3K/s | 352/s | 6.5x |

瓶颈不在 Rust 编码速度，而在 **JS→Rust 数据传递**：napi-rs 的 `serde_json::Value` 参数需要把整个 JS 对象序列化成 JSON 再反序列化成 Rust 结构体，等于做了两次序列化。而 JS `msgpackr` 用 C++ addon 直接读 V8 对象内存，接近零拷贝。

## 目标设计

### 配置

```ts
// HTTP — 默认不开，transport 不碰 body
const client = new HttpClient({
  base_url: 'https://api.example.com',
})
await client.post('/upload', imageBuffer, { content_type: 'image/png' })  // 原样发

// HTTP — msgpack: true → 自动 JSON→msgpack 编码
const msgpackClient = new HttpClient({
  base_url: 'https://api.example.com',
  msgpack: true,
})
await msgpackClient.post('/messages', jsonBuffer)  // 自动转 msgpack 发出

// WS — msgpack: true → 所有 send/receive 自动 binary msgpack
const ws = new WsClient({
  urls: ['wss://rt.example.com'],
  msgpack: true,
})
```

### HTTP 数据流（codec: 'msgpack'）

```
JS: JSON.stringify(body) → Buffer
     ↓ (跨 NAPI 边界，传 Buffer)
Rust transport 层:
  ├─ JSON bytes → serde_json::from_slice → Value
  ├─ Value → rmp_serde::to_vec → msgpack bytes  (在 Rust 内部，无边界开销)
  ├─ 设置 Content-Type: application/msgpack
  └─ 发送 msgpack bytes → wire（比 JSON 小 30-40%）

Server 返回:
  ├─ Content-Type: application/msgpack → rmp_serde::from_slice → Value
  ├─ Value → serde_json::to_vec → JSON bytes  (在 Rust 内部)
  └─ 返回 JSON Buffer 给 JS
     ↓ (跨 NAPI 边界)
JS: JSON.parse(Buffer.toString())  ← 调用方无感，始终拿到 JSON
```

### WS 数据流（codec: 'msgpack'）

```
JS: ws.send(JSON.stringify(obj))
     ↓ (跨 NAPI 边界，传 String)
Rust transport 层:
  ├─ JSON string → serde_json::from_str → Value
  ├─ Value → rmp_serde::to_vec → msgpack bytes
  └─ 发送 binary frame → wire

收消息:
  ├─ binary frame → rmp_serde::from_slice → Value
  ├─ Value → serde_json::to_string → JSON string
  └─ 回调传 JSON string 给 JS
     ↓
JS: onEvent 拿到 JSON string ← 调用方无感
```

### 关键点

1. **编解码在 Rust transport 内部完成**，不跨 NAPI 边界，消除 6x 开销
2. **JS 侧始终是 JSON**——调用方不需要知道 wire format
3. **无运行时协商**：`msgpack: true` 意味着双端都支持，server 返回非 msgpack 则报错
4. **HTTP**：请求带 `Content-Type: application/msgpack`，期望响应也是 `application/msgpack`
5. **WS**：双端约定
6. **向后兼容**：默认不开（passthrough），transport 不碰 body

## Rust 侧改动

### HttpClientConfig

```rust
pub struct HttpClientConfig {
    // ... 现有字段
    /// true → 自动 JSON↔msgpack 编解码; false/不设 → passthrough
    #[serde(default)]
    pub msgpack: bool,
}
```

### HttpTransport::execute — 编码路径

```rust
// 发送前
if self.config.msgpack {
    let value: serde_json::Value = serde_json::from_slice(&body)?;
    let msgpack = rmp_serde::to_vec(&value)?;
    body_bytes = msgpack;
    content_type = "application/msgpack";
}
// msgpack=false → 不动 body，原样发

// 接收后
if self.config.msgpack {
    let value: serde_json::Value = rmp_serde::from_slice(&body)
        .map_err(|e| CatcherError::DecodeError(format!("expected msgpack response: {e}")))?;
    body = serde_json::to_vec(&value)?;
}
// msgpack=false → 不动 body，原样返回
```

### WsTransport — 编码路径

```rust
// 发送
WsCommand::Text(t) if config.msgpack => {
    let value: serde_json::Value = serde_json::from_str(&t)?;
    let binary = rmp_serde::to_vec(&value)?;
    writer.send(Message::Binary(binary)).await;
}

// 接收
Message::Binary(d) if config.msgpack => {
    let value: serde_json::Value = rmp_serde::from_slice(&d)?;
    let json = serde_json::to_string(&value)?;
    event_tx.send(WsEvent::Message { data: json.into_bytes(), is_binary: false });
}
```

## NAPI TS 类型

```ts
interface HttpClientConfig {
  // ... 现有字段
  /** 启用 msgpack 编解码. 默认 false（passthrough） */
  msgpack?: boolean
}

interface WsClientConfig {
  // ... 现有字段
  /** 启用 msgpack 编解码. 默认 false（passthrough） */
  msgpack?: boolean
}
```

## 预期收益

| 指标 | codec: 'json' | codec: 'msgpack' | 改善 |
|---|---|---|---|
| Wire size (300B msg) | ~300B | ~200B | -33% |
| Wire size (20KB list) | ~20KB | ~13KB | -35% |
| 弱网吞吐 | baseline | +30-40% (更小的包) | 显著 |
| 编解码开销 | JSON.stringify/parse | Rust 内部 rmp_serde（不跨 NAPI 边界） | 零额外开销 |

## 与现有 pack/unpack 的关系

`@eric8810/catcher-napi-ws/codec` 中暴露的 `pack/unpack` 是独立的 utility 函数，供需要手动控制 msgpack 的场景使用（如自定义二进制协议）。内置 codec 做完后，绝大多数场景不再需要手动调用 pack/unpack。

建议：
- 内置 codec 完成前，保留 pack/unpack
- 内置 codec 完成后，标记 pack/unpack 为 `@deprecated`，引导用户使用 `codec: 'msgpack'` 配置

## 工作量估算

| 模块 | 内容 | 代码量 |
|---|---|---|
| Config | `msgpack: bool` 字段 | ~5 行 |
| HTTP encode/decode | execute 前后插入编解码 | ~30 行 |
| WS encode/decode | send/receive 路径插入编解码 | ~30 行 |
| NAPI TS 类型 | codec 字段 | ~5 行 |
| 测试 | JSON vs msgpack roundtrip + wire size 验证 | ~100 行 |
| benchmark | 内置 codec vs 手动 pack/unpack vs JSON baseline | ~50 行 |

总计约 **220 行**，核心逻辑 ~60 行。

## 实施清单

- [ ] `HttpClientConfig` 增加 `msgpack: bool` 字段
- [ ] `HttpTransport::execute` 发送前编码 + 接收后解码
- [ ] HTTP `Content-Type: application/msgpack`，响应非 msgpack 报错
- [ ] `WsClientConfig` 增加 `msgpack: bool` 字段
- [ ] WS send/receive 路径编解码
- [ ] NAPI TS 类型更新
- [ ] 集成测试（HTTP + WS msgpack roundtrip）
- [ ] Benchmark（内置 codec vs JSON vs 手动 pack/unpack）
- [ ] 内置 codec 完成后 deprecate 独立 pack/unpack
