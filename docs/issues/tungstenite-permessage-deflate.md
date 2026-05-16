# A-02: WebSocket per-message deflate 不可用

> 严重度: 🟡 中
> 创建: 2026-06-19
> 状态: 🔲

## 问题

`catcher-ws` 的 `compression.rs` 中 `per_message_deflate` 配置被显式忽略：

```rust
// compression.rs:18
let _ = config.per_message_deflate;
```

Rust WS 层不支持压缩，大数据消息（图片、文件）带宽浪费。Dart/UniFFI/napi 消费者的 WS 连接无压缩能力。

## 根因

### 原始判断不准确

代码注释和审计文档写的是"等 tungstenite 0.25+"，但经过核实：

**tungstenite 全版本（0.20 ~ 0.29）均未实现 RFC 7692 permessage-deflate。**

GitHub issue [snapview/tungstenite-rs#2](https://github.com/snapview/tungstenite-rs/issues/2) 从 2017 年开启至今未关闭。

CHANGELOG 从 0.24 → 0.29 的变更（Message 改用 Bytes payload、WebSocketConfig non-exhaustive、read_buffer_size 等）均与 deflate 无关。

### tungstenite 升级本身的价值

从 0.24 → 0.26+ 可获得：
- `read_buffer_size` 默认 128 KiB，高负载读取性能改善
- `Message` 使用 `Bytes` payload，零拷贝克隆
- `WebSocketConfig` non-exhaustive + builder 风格
- `write_buffer_size` 控制批量写入

但有 breaking change：
- `Message` 不再是 `String`/`Vec<u8>`，改为 `Utf8Bytes`/`Bytes`
- `CloseFrame::reason` 改为 `Utf8Bytes`
- `WebSocketConfig::max_send_queue` 被移除

## 解决方案选项

### 方案 A：升级 tungstenite 但不做 deflate（低成本）

将 `tokio-tungstenite` 从 0.24 → 0.26+，适配 API breaking change，但不增加 deflate 支持。

- 工作量：S（~1-2 天，主要是 Message/Config API 适配 + 测试）
- 收益：性能改善，消除技术债
- deflate：仍然不支持

### 方案 B：Fork tungstenite 添加 deflate 支持（中高成本）

见 [`tungstenite-deflate-fork-analysis.md`](./tungstenite-deflate-fork-analysis.md)（方案 B1/B2/B3 完整评估）。

### 方案 C：换用支持 deflate 的 WS 库（高成本）

如 `yawc`（vector 项目已采用）。需要重写整个 WS 传输层。

- 工作量：L（~1-2 周）
- 风险：新库成熟度、API 稳定性、社区活跃度

### 方案 D：不做（当前状态）

应用层自行压缩 payload（gzip/zstd），不走 WebSocket 扩展协商。

- 优点：不依赖 WS 层，跨平台一致
- 缺点：无法与标准 WS permessage-deflate 互操作；服务器可能不支持非标准压缩

## 建议

1. **短期**：执行方案 A — 升级到 0.26+，消除技术债，获得性能改善
2. **评估方案 B**：分析 fork + 实现 deflate 的可行性（见 fork analysis）
3. **远期**：根据用户需求和方案 B 评估结果决定是否投入

## 关联

- `arch-gap-audit-2026.md` A-02
- `compression.rs:13-18` — 当前忽略 per_message_deflate 的代码
- `ws_client.rs:170` — 注释说明 tungstenite 0.24 限制
- `native-layer-capability-gaps.md` — 无直接关联
