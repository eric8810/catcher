# A-02 Fork Analysis: tungstenite permessage-deflate 方案评估

> 创建: 2026-06-19
> 状态: 评估完成
> 关联: `tungstenite-permessage-deflate.md` (主 issue)

---

## 背景

catcher-ws 使用 `tokio-tungstenite 0.24` 作为 WebSocket 传输层。tungstenite 是 Rust 生态中**星标最多**（2.3k stars）的 WebSocket 库，由 snapview 维护，被 tokio-tungstenite / async-tungstenite 等异步适配器广泛依赖。

**核心问题**：tungstenite 自 2017 年起的 [issue #2](https://github.com/snapview/tungstenite-rs/issues/2) 请求支持 RFC 7692 permessage-deflate，至今（2026-06）**仍未合并**。catcher 的 `compression.rs:18` 显式忽略 `per_message_deflate` 配置。

---

## 为什么选了 tungstenite？

1. **生态主流地位**：tokio-tungstenite 是 tokio 官方推荐的 WebSocket 库，reqwest/warp/axum 等均间接依赖
2. **API 稳定**：从 0.20 到 0.29 API 变化缓慢，破坏性变更少且有清晰的迁移路径
3. **零依赖 TLS**：通过 `native-tls` / `rustls` feature flag 自选 TLS 后端
4. **全平台支持**：wasm32（web-sys）、嵌入式（no_std 部分支持）均有覆盖
5. **2017~2024 年没有替代品**：在 catcher 立项时（2024），tungstenite 是唯一生产级异步 WS 客户端库

---

## 方案评估

### 方案 A：升级 tungstenite 但不做 deflate

| 项目 | 详情 |
|------|------|
| 操作 | `tokio-tungstenite 0.24 → 0.26+`，适配 API breaking change |
| 工作量 | S（~1-2 天） |
| 收益 | `Message` 改用 `Bytes` 零拷贝、`read_buffer_size`/`write_buffer_size` 控制、性能改善 |
| Breaking changes | `Message::Text(String)` → `Message::Text(Utf8Bytes)`；`Message::Binary(Vec<u8>)` → `Message::Binary(Bytes)`；`CloseFrame::reason` → `Utf8Bytes`；`max_send_queue` 移除 |
| 风险 | 低 — API 迁移机械化 |
| deflate | ❌ 仍不支持 |

**影响范围**（catcher-ws 内部）：
- `compression.rs` — 无需改动（不涉及 Message 类型）
- `ws_client.rs` — 31 处 tungstenite 引用，约 12 处需要适配（Message 匹配分支、CloseFrame 构建）
- `Cargo.toml` — 版本号更新

**结论**：值得做，是方案 B 的前置条件。

---

### 方案 B1：Fork tungstenite + NextGraph PR #426 deflate 补丁

| 项目 | 详情 |
|------|------|
| 来源 | [git.nextgraph.org/NextGraph/tungstenite-rs](https://git.nextgraph.org/Nextgraph/tungstenite-rs/src/branch/permessage-deflate) — 基于 tungstenite 的 permessage-deflate 分支 |
| 实现成熟度 | 25 commits，完整的 RFC 7692 实现，包含 server/client 双端支持、context takeover、window bits 协商 |
| 维护状态 | NextGraph 项目活跃维护至 2025，但 deflate 分支已 1+ 年未更新 |
| 发布 | **未发布到 crates.io**，需 git dependency 或自行 fork 发布 |

**成本评估**：

| 子任务 | 工作量 | 说明 |
|--------|--------|------|
| Fork + rebase 到 0.26+ | M（~2-3 天） | NextGraph 补丁基于 0.21，需 rebase 到 0.26，可能冲突 |
| 自测 + Autobahn 测试套件 | M（~1-2 天） | 需运行 Autobahn 测试验证 deflate 行为正确性 |
| catcher-ws 适配 | S（~1 天） | 在 `compression.rs` 中接入 deflate 配置 |
| 持续维护 | L（长期） | 每次 tungstenite 上游更新都需 rebase，永久维护负担 |

**风险**：
- 🔴 上游 merge 无望（issue #2 已开 9 年）
- 🟡 每次升级 tungstenite 需手动 rebase deflate 补丁
- 🟡 NextGraph 补丁未经大规模生产验证

---

### 方案 B2：换用 ratchet + ratchet_deflate

| 项目 | 详情 |
|------|------|
| 来源 | [github.com/graphform/ratchet](https://github.com/graphform/ratchet) — 专门为 RFC 6455 + RFC 7692 设计的 WS 库 |
| 维护者 | graphform（SwimOS 团队） |
| 星标 | 58 stars（远小于 tungstenite 2.3k） |
| 特性 | 原生 permessage-deflate、扩展框架（`ratchet_ext`）、split 支持、全 Autobahn 测试通过 |
| crates.io | `ratchet_rs`（核心）、`ratchet_deflate`（压缩扩展）、`ratchet_ext`（扩展 trait） |

**成本评估**：

| 子任务 | 工作量 | 说明 |
|--------|--------|------|
| 重写 catcher-ws 传输层 | L（~3-5 天） | API 完全不同：基于 `BytesMut` 缓冲区而非 `Message` 枚举 |
| TLS 适配 | S（~0.5 天） | ratchet 使用 `tokio-rustls`，需验证 TLS 连接方式 |
| Heartbeat/重连适配 | M（~1-2 天） | 现有的 ping/pong/重连逻辑基于 tungstenite API，需重写 |
| napi-ws 适配 | M（~1 天） | napi 层通过 catcher-ws crate 间接使用，需验证兼容性 |

**风险**：
- 🟡 社区较小（58 stars），长期维护不确定
- 🟡 API 不如 tungstenite 成熟，可能有边界情况
- 🟢 SwimOS 是生产级项目，实际使用量大

---

### 方案 B3：换用 yawc

| 项目 | 详情 |
|------|------|
| 来源 | [crates.io/crates/yawc](https://crates.io/crates/yawc) — 零拷贝 WebSocket 客户端 |
| 采用者 | **Vector**（Datadog）— [PR #24654](https://github.com/vectordotdev/vector/pull/24654) 正在从 tungstenite 迁移到 yawc |
| 特性 | 原生 permessage-deflate、零拷贝、Frame/OpCode API |
| 成熟度 | 较新，但 Vector 的采用是强背书 |

**成本评估**：

| 子任务 | 工作量 | 说明 |
|--------|--------|------|
| 重写传输层 | L（~3-5 天） | 类似 ratchet，API 完全不同 |
| TLS 适配 | S（~0.5 天） | 需验证 TLS 支持方式 |
| Vector PR 经验可参考 | 🟢 | Vector PR 有详细的迁移指南和测试策略 |

**风险**：
- 🟡 Vector PR #24654 尚未合并（2026-02 开启，仍 Open）
- 🟡 yawc 较新，crates.io 版本号低
- 🟢 零拷贝设计，性能可能更好

---

### 方案 D：应用层压缩（不走 WS 扩展）

| 项目 | 详情 |
|------|------|
| 操作 | 发送前 gzip/zstd 压缩 payload，接收后解压 |
| 工作量 | S（~0.5 天） |
| 优点 | 不依赖 WS 库，跨平台一致（Rust/TS/Dart） |
| 缺点 | ❌ 无法与标准 WS permessage-deflate 互操作 |
| 缺点 | ❌ 服务器可能不支持非标准压缩 |
| 缺点 | ❌ 需要额外的压缩/解压协商协议（自定义 header 或首条消息约定） |

**结论**：作为临时方案可用，但无法替代标准 permessage-deflate。

---

## 对比总览

| 维度 | A: 升级无 deflate | B1: Fork+patch | B2: ratchet | B3: yawc | D: 应用层压缩 |
|------|:--:|:--:|:--:|:--:|:--:|
| **工作量** | S | M+L(长期) | L | L | S |
| **deflate 支持** | ❌ | ✅ | ✅ | ✅ | ⚠️ 非标准 |
| **标准兼容** | ✅ | ✅ | ✅ | ✅ | ❌ |
| **维护成本** | 低 | 高 | 中 | 中 | 低 |
| **上游依赖风险** | 低 | 高 | 中 | 中 | 无 |
| **性能影响** | +（Bytes 零拷贝） | + | + | ++（零拷贝） | -（额外 CPU） |
| **API 变化** | 小 | 小 | 大 | 大 | 无 |

---

## 建议

### 短期（v0.3.x）

执行**方案 A**：升级 `tokio-tungstenite 0.24 → 0.26+`，获得性能改善，消除技术债。

- 即使后续选方案 B2/B3，升级也是必要前置
- 工作量最小，风险最低
- 至少修复 `compression.rs` 中的误导性注释

### 中期（v0.4.x）

根据用户需求评估**方案 B2（ratchet）或 B3（yawc）**：

- 如果有明确的 WS 压缩需求，优先考虑 **yawc**（Vector 背书，API 现代化）
- 如果 yawc 的 Vector PR 一直未合并或库停滞，回退到 **ratchet**（SwimOS 生产使用）
- **不建议 Fork+patch（B1）**：长期维护成本不可控

### 判断标准

何时触发方案 B 的实施：
1. 有用户反馈 WS 大数据传输带宽问题
2. 对端服务器强制要求 permessage-deflate 扩展
3. E2E 性能测试显示 WS 是带宽瓶颈

---

## 行动项

- [ ] 方案 A：升级 tokio-tungstenite 到 0.26+，适配 Message/CloseFrame API
- [ ] 方案 A：更新 `compression.rs` 注释，明确说明不支持 deflate 的原因
- [ ] 方案 A：补充 TEST-05（WsTransport 测试）验证升级后行为
- [ ] 评估：监控 yawc Vector PR #24654 合并状态，作为 B3 决策依据
- [ ] 评估：监控 tungstenite issue #2，如果上游有进展可重新评估
