# 阶段一：网络通讯协议场景调研

> 调研日期：2025-07-18
> 范围：HTTP、WebSocket、SSE 及相邻协议的生产级边界场景
> 目标：发现 catcher 当前未覆盖的协议级测试场景

---

## A1. HTTP 协议变体与边界

### A1.1 HTTP 版本协商与降级

**场景**：客户端和服务端之间的 HTTP 版本不匹配。

| 场景 | 描述 | catcher 覆盖 | 建议测试 |
|------|------|:----------:|---------|
| HTTP/1.1 与 HTTP/2 协商 | ALPN 协商失败时客户端行为 | ⚠️ 依赖 reqwest 默认 | 验证降级到 HTTP/1.1 后 retry/CB 行为 |
| HTTP/2 GOAWAY 帧处理 | 服务端发送 GOAWAY 后连接关闭 | ❌ 未覆盖 | 模拟 GOAWAY 后待处理请求的 retry 行为 |
| HTTP/2 连接合并 | 同 host 多请求复用同一 TCP 连接 | ✅ reqwest 处理 | 验证 keepAlive 统计不因多路复用失真 |
| HTTP/3 (QUIC) | 未来协议支持 | ❌ 超出范围 | 调研阶段：确认 reqwest 对 QUIC 的支持路线图 |
| HTTP/1.0 服务端 | 远古服务端不支持 keep-alive | ❌ 未测试 | `Connection: close` 后连接池驱逐行为 |

**关键发现**：
- reqwest 0.13 使用 hyper 1.x，HTTP/2 GOAWAY 由 hyper 自动处理，但 catcher 的 retry/CB 层不感知 GOAWAY
- HTTP/2 下的连接池行为与 HTTP/1.1 完全不同：一个连接复用多个并发请求，`pool_max_idle_per_host` 语义变化
- [reqwest#2283](https://github.com/seanmonstar/reqwest/issues/2283) 报告 HTTP/2 升级后超时问题，说明协议版本切换是真实风险

### A1.2 Content-Type 与 Body 处理

**场景**：各种 Content-Type 和 body 编码组合。

| 场景 | 描述 | catcher 覆盖 | 建议测试 |
|------|------|:----------:|---------|
| `Transfer-Encoding: chunked` | 分块传输响应 | ✅ reqwest 处理 | 验证大 chunk 与 SSE 行解析的交互 |
| `Content-Encoding: gzip/brotli/deflate` | 压缩传输 | ✅ reqwest 处理 | 验证压缩+分块的组合场景 |
| 空 body 响应 (204/304) | 无 body 的响应 | ⚠️ 部分 | 验证 body 为空的 JSON parse 不会 panic |
| `Content-Type: application/octet-stream` | 二进制响应 | ⚠️ | 验证 `unpack()` 正确处理非 msgpack 二进制 |
| 超大 body (OOM 风险) | 超过内存的响应体 | ⚠️ 已有 execute_stream | 验证 stream 模式下取消请求后资源释放 |
| `Content-Type` 缺失 | 无类型声明 | ❌ 未测试 | 验证不崩溃，fallback 行为清晰 |
| 多值 Content-Type | `text/html; charset=utf-8` | ❌ 未测试 | 解析 charset 参数 |

### A1.3 HTTP 状态码边界

**场景**：各种不常见 HTTP 状态码的客户端行为。

| 状态码范围 | 关键码 | catcher 当前行为 | 缺失场景 |
|-----------|--------|-----------------|---------|
| 1xx Informational | 100 Continue, 103 Early Hints | reqwest 自动处理 | 验证 100 Continue 超时行为 |
| 3xx Redirection | 301, 302, 307, 308 | ✅ 已有 redirect control | 301 将 POST 改 GET 的行为一致性 |
| 4xx Client Error | 408 Request Timeout, 429 Too Many Requests | ⚠️ 408 未特殊处理 | 408 → 应重试（keepalive race），429 → 应读取 Retry-After |
| 5xx Server Error | 502, 503, 504 | ✅ retryable | 504 超时与连接超时的区分 |

**关键发现**：
- **HTTP 408 Request Timeout** 是 keepalive race condition 的关键信号（[RFC 7231](https://datatracker.ietf.org/doc/html/rfc7231#section-6.5.7)）。当客户端在服务端已关闭的连接上发送请求时，服务端返回 408。catcher 目前将 4xx 全部归为 NonRetryable，这会导致 keepalive race 时无法自动恢复。
- **HTTP 429 Too Many Requests** 应尊重 `Retry-After` header，catcher 的 retry 策略未考虑此 header。
- **HTTP 425 Too Early** — 0-RTT 请求被拒绝时应自动重试（HTTP/3 相关）。

### A1.4 重定向边界

| 场景 | catcher 覆盖 | 建议 |
|------|:----------:|------|
| 重定向循环检测 | ✅ maxRedirects | 验证 maxRedirects=0 时返回 302 而非跟随 |
| 跨协议重定向 (HTTP→HTTPS) | ⚠️ 依赖 reqwest | 验证 TLS 配置在重定向后依然生效 |
| 跨域重定向 | ⚠️ | 验证 Authorization header 是否被 reqwest 剥离 |
| 相对路径 Location header | ✅ reqwest 处理 | 验证 baseURL 拼接场景 |
| 重定向时 body 丢失 | ❌ | POST→302→GET 时原 body 被丢弃，验证行为文档化 |

---

## A2. WebSocket 高级特性与边界

### A2.1 连接生命周期极端情况

| 场景 | 描述 | catcher 覆盖 | 建议测试 |
|------|------|:----------:|---------|
| 握手阶段超时 | TCP 连接成功但 WS upgrade 无响应 | ✅ handshakeTimeout | 验证超时后不进入重连（NonRetryable） |
| 握手 401/403 | 认证失败 | ✅ | 验证不重连，直接返回错误 |
| 握手 3xx 重定向 | WS upgrade 阶段收到 302 | ❌ 未测试 | 验证是否跟随重定向 |
| 帧大小超限 | 服务端发送超过 max_payload_bytes | ✅ | 验证超限后连接关闭，自动重连 |
| 控制帧与数据帧交织 | Ping/Pong/Close 与数据帧并发 | ⚠️ | 验证心跳与 send() 并发的帧交错正确性 |
| 分片消息 (Fragmented) | 单消息跨多个帧 | ✅ tungstenite 处理 | 验证分片消息与 perMessageDeflate 组合 |
| 保留位 (RSV1/2/3) 错误 | 收到非法 RSV 位 | ✅ tungstenite 处理 | 验证错误后连接关闭和重连 |
| Close frame 无 body | 不合法但常见 | ⚠️ | 验证 close code 默认为 1005（No Status Rcvd） |

### A2.2 Backpressure 与发送队列

**关键发现**：tokio-tungstenite 的 [Issue #35](https://github.com/snapview/tokio-tungstenite/issues/35) 指出 `Sink` 实现不施加背压——当发送速度超过网络吞吐时，内部 `send_queue` 无限增长导致 OOM。

| 场景 | catcher 覆盖 | 建议 |
|------|:----------:|------|
| 发送速度 > 网络带宽 | ❌ | 需要发送端背压或 max_pending_frames 限制 |
| 连接断开时发送 | ⚠️ | send() 应快速失败，不应无限排队 |
| 重连期间积压消息 | ❌ | 重连期间 send() 行为需明确：丢弃/disconnect/排队 |

### A2.3 多端点竞速

| 场景 | catcher 覆盖 | 建议 |
|------|:----------:|------|
| 所有端点同时失败 | ✅ | 验证错误信息包含所有端点的失败原因 |
| 多个端点同时成功 | ✅ 取最快 | 验证其他连接正确关闭无泄漏 |
| 端点 IP 不同但同一服务 | ❌ | 验证不会因 DNS 缓存导致多端点指向同一 IP |
| 竞速超时后的重试 | ⚠️ | 竞速失败后是否进入正常重连 |

### A2.4 WebSocket 压缩

**关键发现**：[ws#1617](https://github.com/websockets/ws/issues/1617) 和多个生产案例显示 perMessageDeflate 存在**内存泄漏风险**——zlib 上下文在连接断开后未释放。

| 场景 | catcher 覆盖 | 建议 |
|------|:----------:|------|
| 长时间连接压缩内存增长 | ❌ | 定期监控 deflate context 内存 |
| 频繁重连时的 zlib context 泄漏 | ❌ | 每次重连确认旧 context 已释放 |
| 压缩阈值边界 | ⚠️ | 验证 threshold_bytes 正好等于消息大小时的压缩/不压缩行为 |

---

## A3. SSE 协议边界与互操作

### A3.1 代理兼容性问题

**关键发现**：[Mike Talbot 的生产事故报告](https://dev.to/miketalbot/server-sent-events-are-still-not-production-ready-after-a-decade) 揭示 SSE 在**不受控网络环境**中存在致命问题：老旧代理/防火墙在看到 `Transfer-Encoding: chunked` 且无 `Content-Length` 时会缓冲整个响应直到连接关闭才转发。

| 场景 | catcher 覆盖 | 建议 |
|------|:----------:|------|
| 代理缓冲 SSE | ❌ | 文档中警告；考虑 heartbeat comment 频率提高以触发代理 flush |
| HTTPS 下代理缓冲 | ⚠️ | 透明代理无法解密 HTTPS，但企业环境有 TLS 中间人代理 |
| HTTP/2 SSE 多路复用 | ❌ | 验证 HTTP/2 下多个 SSE 流共存 |

### A3.2 SSE 重连与去重

| 场景 | catcher 覆盖 | 建议 |
|------|:----------:|------|
| `Last-Event-ID` 携带 | ✅ | 验证重连后首条消息的 ID 与 lastEventId 一致 |
| 服务端不支持 ID 时重连 | ❌ | 重连后可能收到重复/遗漏消息，需文档说明 |
| `retry:` 间隔为 0 | ⚠️ | 验证不会导致立即重连风暴 |
| 重连时 204 响应 | ✅ 不重连 | 验证 204 终止循环 |
| 重连时 5xx/连接错误 | ⚠️ | 验证 CB/retry 正常工作 |

### A3.3 SSE 行协议边界

| 场景 | 输入 | catcher 覆盖 | 建议 |
|------|------|:----------:|------|
| 超长行 (>64KB) | 单行超大 JSON | ❌ | 可能需要可配置的 max_line_length |
| UTF-8 非法序列 | 非 UTF-8 字节 | ⚠️ Issue #22 | 验证 UTF-8 lossy 行为 |
| `\r`, `\r\n`, `\n` | 混合换行格式 | ✅ chunk buffer | 验证 Windows 格式 |
| 跨 chunk 的 UTF-8 码点 | 3 字节字符被切在 chunk 边界 | 🔀 TS 已测 / Rust 有 Bug | **TS**：`stream.test.ts` S8 已用 `Uint8Array` 验证跨 chunk "é" 正确组装。<br>**Rust**：`stream.rs:131` 用 `String::from_utf8_lossy`，在码点边界仍会损坏字符。需改用字节级 buffering。 |
| BOM (U+FEFF) | 流开头有 BOM | ❌ | 验证 BOM 是否被过滤 |

---

## A4. 编码格式与序列化边界

### A4.1 msgpack 编码边界

| 场景 | catcher 覆盖 | 建议 |
|------|:----------:|------|
| 深度嵌套对象 (>100 层) | ❌ | 防止栈溢出 |
| 包含 NaN/Infinity 的 f64 | ❌ | JSON 不支持，msgpack 支持；cross-codec fallback 行为 |
| Timestamp extension type | ❌ | msgpack ext 类型的处理 |
| 空 body 解码 | ❌ | 验证 `unpack([])` 行为 |
| 超大数据包 (>2GB) | ❌ | msgpack 理论上限，实际内存边界 |

### A4.2 JSON ↔ msgpack 互操作

| 场景 | catcher 覆盖 | 建议 |
|------|:----------:|------|
| JSON null vs msgpack nil | ✅ | 验证 roundtrip 一致 |
| JSON number 精度丢失 | ⚠️ | `serde_json::Number` vs msgpack integer/float |
| JSON array 作为 WS body (Issue #19, #21) | ✅ 已修复 | 回归测试 |

---

## A5. 相邻协议与未来方向（调研性）

以下协议不在 catcher 当前范围内，但值得调研以评估未来支持价值：

| 协议 | 与 catcher 的关系 | 市场规模 | 集成难度 |
|------|-----------------|---------|---------|
| **gRPC-web** | HTTP/2 + protobuf，浏览器端需 grpc-web proxy | 大（微服务主流） | 高（需 protobuf 支持） |
| **WebTransport** | 基于 QUIC 的双向流，替代 WebSocket | 新兴（Chrome 97+） | 中（需 QUIC 协议栈） |
| **MQTT over WebSocket** | IoT 场景，已有 WS 即可传输 | 大（IoT 标准） | 低（WS 已支持） |
| **GraphQL over SSE** | 实时订阅 | 大 | 低（SSE 已支持） |
| **TCP raw socket** | 自定义协议场景 | 中 | 低（可评估暴露 raw TCP trait） |

---

## 阶段一总结：关键缺失场景

### 高优先级（可直接产生测试用例）

1. **HTTP 408 应重试** — 当前 4xx 全部 NonRetryable，但 408 是 keepalive race 信号（已验证：`error.rs:82-88`）
2. **HTTP 429 Retry-After** — retry 策略需支持读取 `Retry-After` header（已验证：全仓零命中）
3. **WS send 后立即 close** — 发送缓冲区未刷新时关闭的竞态
4. **WS 连接断开时 send()** — 不应无限排队（已验证：无 `max_pending_frames`）
5. **Rust SSE 跨 chunk UTF-8** — `stream.rs` 用 `String::from_utf8_lossy` 在码点边界损坏字符（TS 侧已有测试 S8）
6. **HTTP/2 GOAWAY 重试** — 确保 GOAWAY 前的请求被正确重试
7. **SSE BOM 处理** — 流开头 BOM 字符过滤（已验证：全仓零命中）
8. **重定向时 auth header 剥离** — 跨域重定向安全性验证
9. **多端点竞速中 DNS 指向同一 IP** — 去重检测（已验证：无去重逻辑）
10. **超大 msgpack 嵌套深度** — 栈溢出防护（已验证：`codec.rs` 中 `rmpv_to_json` 无界递归）

### 中优先级（文档或设计层面）

11. **WS perMessageDeflate 内存监控** — 长连接和频繁重连时的内存泄漏风险
12. **SSE 代理缓冲警告** — 文档说明企业网络兼容性
13. **HTTP/3 (QUIC) 路线图** — 调研 reqwest 的 QUIC 支持计划
14. **WebTransport 调研** — 作为 WS 的下一代替代方案评估
