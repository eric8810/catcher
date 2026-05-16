# 依赖升级与选型分析：reqwest / tungstenite / yawc

> 日期：2026-05-16
> 目标：评估 reqwest 0.13 升级、tungstenite 升级、yawc 替换的可行性、工作量和收益，给出 PR 分解建议。

---

## 一、现状

### 1.1 当前依赖版本

| 依赖 | 声明版本 | 锁定版本 | 引入方式 |
|------|---------|---------|---------|
| `reqwest` | `0.13` (default-features=false) | 0.13.3 | 直接依赖 |
| `reqwest-middleware` | `0.5` | 0.5.1 | 直接依赖 |
| `reqwest-retry` | `0.9` | 0.9.1 | 直接依赖 |
| `retry-policies` | `0.5` | 0.5.2 | 直接依赖 |
| `hyper` | — | 1.9.0 | reqwest 传递依赖 |
| `hyper-util` | — | 0.1.20 | reqwest 传递依赖 |
| `tower` | — | 0.5.3 | reqwest 传递依赖 |
| `tokio-tungstenite` | `0.29` | 0.29.0 | 直接依赖 |
| `tungstenite` | — | 0.29.0 | tokio-tungstenite 传递依赖 |

**关键点**：项目不直接依赖 hyper / hyper-util / tower / http，全部通过 reqwest 间接引入。

### 1.2 API 使用规模

| 模块 | reqwest API 引用 | tungstenite API 引用 |
|------|:---:|:---:|
| `catcher-http` transport/ | ~30 | — |
| `catcher-http` sse/ | ~8 | — |
| `catcher-http` observability/ | ~6 | — |
| `catcher-http` resilience/ | ~5 | — |
| `catcher-ws` transport/ | — | ~25 |
| `catcher-ws` compression/ | — | ~3 |
| **合计** | **~49** | **~28** |

### 1.3 reqwest 核心使用点

| API 分类 | 方法 | 文件 | 说明 |
|---------|------|------|------|
| Client 构建 | `Client::builder()` | http_client.rs | 连接池配置（idle_timeout, keepalive, max_conns） |
| TLS | `.use_rustls_tls()` / `.tls_built_in_root_certs()` | tls.rs | rustls 集成 |
| Proxy | `.proxy(reqwest::Proxy)` | http_client.rs | HTTP/SOCKS5 代理 |
| 请求构建 | `.request(method, &url)` | http_client.rs | 请求发送 |
| 流式 | `.send().bytes_stream()` | sse/stream.rs | SSE 流式读取 |
| 中间件 | `reqwest-middleware::ClientBuilder` | resilience/retry.rs | retry 中间件层 |
| DNS | `.hickory_dns()` | http_client.rs | 自定义 DNS |

### 1.4 tungstenite 核心使用点

| API 分类 | 类型/方法 | 文件 | 说明 |
|---------|----------|------|------|
| 连接建立 | `connect_async()` / `connect_async_tls_with_config()` | ws_client.rs | 带/不带 TLS |
| 消息类型 | `Message::Text` / `Binary` / `Ping` / `Pong` / `Close` / `Frame` | ws_client.rs | 6 种变体 |
| Stream | `WebSocketStream<MaybeTlsStream<TcpStream>>` | ws_client.rs | 类型别名 |
| 配置 | `WebSocketConfig` | compression.rs | max_message_size, max_frame_size |
| 压缩 | `per_message_deflate` | compression.rs | **显式忽略**（第 18 行） |

---

## 二、升级选项分析

### 2.1 reqwest 0.12 → 0.13 — ✅ 已实施

#### 2.1.1 上游变更

reqwest 0.13 于 2025 年底发布，当前最新 0.13.3。本项目已升级到 0.13.3。主要变更：

| 变更 | 影响 | 说明 |
|------|------|------|
| **rustls 成为默认 TLS** | 🔴 Breaking | `rustls-tls` feature 改名为 `rustls`；本项目统一使用 reqwest 0.13 默认的 `aws_lc_rs` provider |
| **内置 retry** | 🟢 新功能 | `ClientBuilder::retry(policy)` + `reqwest::retry::Builder` |
| **hyper-util 可组合池** | 🟢 新功能 | 连接池拆分为可组合的 tower layer |
| **native-tls ALPN** | 🟡 变更 | 默认启用，可 `native-tls-no-alpn` 禁用 |
| **query/form 独立 feature** | 🔴 Breaking | 需显式启用 `query` / `form` feature |
| **TLS 方法重命名** | 🟡 Soft deprecate | `use_rustls_tls()` → `tls_backend_rustls()`（旧名仍可用） |
| **移除长期废弃项** | 🔴 Breaking | `trust-dns` → 已移除（用 `hickory-dns`） |

#### 2.1.2 对 catcher 的影响

**Cargo.toml 改动**：

```toml
# 当前
reqwest = { version = "0.12", default-features = false, features = [
    "json", "stream", "socks", "rustls-tls",
    "hickory-dns", "multipart",
] }

# 升级后
reqwest = { version = "0.13", default-features = false, features = [
    "json", "stream", "socks", "rustls",
    "hickory-dns", "multipart",
    "query", "form",          # 新增：0.13 从默认中拆出
] }
```

**代码改动**：

| 文件 | 改动 | 复杂度 |
|------|------|:------:|
| `tls.rs` | `use_rustls_tls()` → `tls_backend_rustls()` 或保持旧名（soft deprecate） | 低 |
| `Cargo.toml` | feature 名更新、添加 query/form | 低 |
| `resilience/retry.ts` | 评估是否用 reqwest 内置 retry 替代 reqwest-retry | 中 |
| `http_client.rs` | 连接池 API 可能微调 | 低 |

#### 2.1.3 reqwest-retry / reqwest-middleware 兼容性

**风险**：reqwest-middleware 0.4 和 reqwest-retry 0.7 针对 reqwest 0.12。reqwest 0.13 的兼容版本尚未确认。

| 方案 | 说明 |
|------|------|
| **A. 等上游跟进** | 等 reqwest-middleware 发布支持 0.13 的版本 |
| **B. 用 reqwest 内置 retry** | 0.13 自带 `ClientBuilder::retry()`，可能替代 reqwest-retry |
| **C. 自实现中间件** | 用 tower service 层自实现 retry，绕过 reqwest-middleware |

**建议**：先检查 reqwest-middleware 0.5+ 是否已支持 reqwest 0.13。如果已支持，直接升级；否则采用方案 B（用 reqwest 内置 retry 替代）。

#### 2.1.4 收益评估

| 收益 | 说明 | 优先级 |
|------|------|:------:|
| 内置 retry | 移除 reqwest-retry / reqwest-middleware 依赖链 | 中 |
| hyper-util 可组合池 | **解决 G-01/G-02 的路径** | 高 |
| rustls 默认 | 减少配置复杂度 | 低 |
| HTTP/3 改进 | QUIC 连接池修复 | 低 |
| 活跃维护 | 0.12 分支已进入维护模式 | 中 |

#### 2.1.5 风险评估

| 风险 | 概率 | 影响 | 缓解 |
|------|:----:|:----:|------|
| reqwest-middleware 不兼容 0.13 | 中 | 高 | 用内置 retry 替代 |
| TLS 行为变化 | 低 | 中 | 回归测试覆盖 |
| 连接池行为变化 | 低 | 中 | E2E 弱网测试验证 |
| Cargo.lock 冲突 | 低 | 低 | cargo update |

---

### 2.2 tokio-tungstenite 0.24 → 0.29 — ✅ 已实施

#### 2.2.1 上游变更

本项目已升级到 tokio-tungstenite/tungstenite 0.29.0。主要适配点：

| 变更 | 影响 | 说明 |
|------|------|------|
| `Message` payload 类型变化 | 🔴 Breaking | `Text(String)` → `Text(Utf8Bytes)`，`Binary/Ping/Pong(Vec<u8>)` → `Bytes` |
| `CloseFrame.reason` 类型变化 | 🟡 变更 | `Cow<str>` → `Utf8Bytes` |
| `WebSocketConfig` 标记 `#[non_exhaustive]` | 🔴 Breaking | 不能用 struct literal，需用 builder 方法 |
| **permessage-deflate** | ❌ 仍未支持 | issue #2 / PR #426 仍未合并 |

#### 2.2.2 对 catcher 的影响

已完成适配：

| 改动区域 | 说明 | 状态 |
|---------|------|:------:|
| `Message::Text` / `Binary` / `Ping` | 显式 `.into()` 转为 `Utf8Bytes` / `Bytes` | ✅ |
| `Message::Binary` 接收 | `Bytes` 转 `Vec<u8>` | ✅ |
| `CloseFrame.reason` | 使用 `.into()` / `.to_string()` | ✅ |
| `WebSocketConfig` | 改用 `WebSocketConfig::default().max_message_size(...).max_frame_size(...)` | ✅ |

**关键结论**：升级到 0.29 仍不会解决 permessage-deflate 问题，仅完成上游 API 跟进。

#### 2.2.3 收益评估

| 收益 | 说明 | 优先级 |
|------|------|:------:|
| 跟进上游 | 0.24 → 0.29，避免长期停留旧 API | 中 |
| bug 修复 | 获取 0.25~0.29 期间的协议/性能修复 | 中 |
| permessage-deflate | ❌ 仍然不支持 | — |

---

### 2.3 yawc 替换 tungstenite

#### 2.3.1 yawc 概况

| 属性 | 值 |
|------|---|
| 版本 | 0.3.x（最新） |
| 维护者 | `infinitefield` 社区项目（无明确 Vector/Datadog 背书） |
| RFC 6455 合规 | ✅ Autobahn 测试通过 |
| permessage-deflate | ✅ RFC 7692 完整支持 |
| HTTP/1.1 升级 | ✅ 内置（不依赖外部 HTTP 客户端） |
| reqwest 集成 | ✅ 通过 `reqwest` feature |
| WASM 支持 | ✅ |
| 零拷贝 | ✅ |
| 异步 | ✅ tokio |

#### 2.3.2 API 对比

| 功能 | tungstenite 0.29 | yawc 0.3 |
|------|------------------|----------|
| 连接建立 | `connect_async(url)` | `Client::connect(url, options)` |
| TLS | `connect_async_tls_with_config()` | 内置（通过 reqwest 或 rustls） |
| 发送消息 | `sink.send(Message::Text(s))` | `conn.send(Message::text(s))` |
| 接收消息 | `stream.next()` (Stream) | `conn.recv()` 或 Stream trait |
| Close | `Message::Close(Some(CloseFrame))` | `conn.close(code, reason)` |
| Ping/Pong | `Message::Ping/Pong` | 内置自动响应 |
| 压缩配置 | ❌ 不支持 | `Options::compression()` |
| 最大消息大小 | `WebSocketConfig::max_message_size` | `Options::max_message_size()` |
| 自定义 headers | `IntoClientRequest::into_client_request()` + headers | `Options::headers()` |

#### 2.3.3 迁移工作量

| 文件 | 当前（tungstenite） | 改动量 |
|------|-------------------|:------:|
| `Cargo.toml` | tokio-tungstenite = "0.29" | 改为 yawc |
| `ws_client.rs` | ~200 行 WS 连接/消息/生命周期 | **重写** — API 完全不同 |
| `compression.rs` | 空实现 ~30 行 | 删除或改为 yawc Options 配置 |
| `transport/mod.rs` | re-export 类型 | 更新类型 |
| FFI 层 | 直接使用 WS 类型 | 适配新消息类型 |

**估计**：约 300-400 行改动，主要是 `ws_client.rs` 重写。

#### 2.3.4 风险

| 风险 | 概率 | 影响 | 缓解 |
|------|:----:|:----:|------|
| yawc API 不稳定 | 中 | 高 | 0.3 版本暗示可能有 breaking changes |
| 社区验证不足 | 中 | 高 | GitHub Stars 约 97，使用面远小于 tungstenite |
| 维护者背书不明确 | 中 | 中 | 不能视为 Vector/Datadog 官方背书 |
| Autobahn 兼容性边界 | 低 | 中 | 自跑 Autobahn 测试套件 |
| 与 reqwest 0.13 冲突 | 中 | 高 | 需确认 yawc 支持的 reqwest 版本 |

---

## 三、G-01/G-02 根治路径：hyper-util 可组合池

### 3.1 问题回顾

G-01（Retry 复用坏连接）和 G-02（keepAlive 无健康检查）的根因是 reqwest 的 `Client` 不暴露连接池驱逐 API。缓解措施（idle_timeout 90→30s, keepalive 60→20s）降低了问题概率但未根治。

### 3.2 hyper-util 可组合池方案

reqwest 作者 seanmonstar 于 2025-12 发布 `hyper-util::client::pool` 模块，将连接池拆分为可组合的 tower layer：

| 池类型 | 功能 |
|--------|------|
| `pool::cache` | 连接缓存池，支持 idle timeout、最大连接数、连接 racing |
| `pool::singleton` | 单连接池（HTTP/2 场景） |
| `pool::negotiate` | ALPN 协议协商（HTTP/1.1 ↔ HTTP/2 自动切换） |
| `pool::map` | 按目标地址路由连接 |
| `pool::expire`（即将发布） | 连接过期层：idle time / max lifetime / poisoning |

**根治路径**：

```
reqwest 0.13（内置 hyper-util 池）
  → 自定义 pool::expire layer（连接过期 + poisoning）
  → retry 前检查连接健康状态
  → G-01/G-02 根治
```

### 3.3 前置条件

1. reqwest 0.13 升级完成
2. `pool::expire` layer 发布（或自实现类似逻辑）
3. E2E 弱网测试环境可验证

### 3.4 实施难度

| 难度因素 | 评估 |
|---------|------|
| API 理解 | 中 — tower layer 组合需要理解 Service trait |
| 代码改动 | 中 — 主要在 http_client.rs 的 Client 构建逻辑 |
| 测试验证 | 高 — 需 E2E 弱网测试验证连接驱逐效果 |
| 回归风险 | 中 — 连接池行为改变影响所有请求 |

---

## 四、PR 分解建议

### PR-1: reqwest 0.12 → 0.13 升级 — ✅ 已完成

**已完成范围**：
- 更新 `catcher-http/Cargo.toml` 依赖版本和 feature flags
- `reqwest-middleware` 升级到 0.5，`reqwest-retry` 升级到 0.9，`retry-policies` 升级到 0.5
- `rustls-tls` feature 映射到 `reqwest/rustls`，项目直接 rustls provider 也统一到 `aws_lc_rs`，避免 aws-lc-rs/ring provider 冲突
- 显式启用 `query` / `form` feature
- 新增调用 `tcp_keepalive_interval()` / `tcp_keepalive_retries()`，进一步缓解 G-02
- `cargo check -p catcher-http -p catcher-ws` 通过

**风险结论**：reqwest-middleware 0.5 已兼容 reqwest 0.13，本项目无需在本 PR 中移除 reqwest-retry。

---

### PR-2: 移除 reqwest-retry，使用 reqwest 内置 retry（暂缓）

**范围**：
- 移除 `reqwest-middleware` / `reqwest-retry` / `retry-policies` 依赖
- 重写 `resilience/retry.rs` 使用 reqwest 0.13 的 `ClientBuilder::retry()`
- 确保现有 retry 行为不变（attempts, backoff, onRetry callback）
- 验证 retry 测试全部通过

**预估改动**：~150 行代码

**依赖**：PR-1

**风险**：中等（需确保 retry 行为完全兼容）

---

### PR-3: tungstenite 0.24 → 0.29 升级 — ✅ 已完成

**已完成范围**：
- 更新 `catcher-ws/Cargo.toml`: `tokio-tungstenite = "0.29"`
- 适配 `Message` payload 类型变化（`Utf8Bytes` / `Bytes`）
- 适配 `CloseFrame.reason` 类型变化
- `WebSocketConfig` 改用 builder 方法，避免 `#[non_exhaustive]` struct literal
- `cargo check -p catcher-http -p catcher-ws` 通过

**限制**：tungstenite 0.29 仍未合入 permessage-deflate。A-02 仍需 upstream PR / Signal fork / experimental feature 路线。

---

### PR-4: yawc 替换 tungstenite（高工作量，观望）

**范围**：
- 移除 `tokio-tungstenite` 依赖
- 引入 `yawc`
- 重写 `ws_client.rs` 使用 yawc API
- 删除 `compression.rs` 空实现，改用 yawc Options 配置
- 适配 FFI 层消息类型
- 新增 permessage-deflate 配置选项和测试

**预估改动**：~300-400 行代码

**依赖**：PR-3（如果不做 PR-3 则独立进行）

**风险**：中高（yawc 成熟度、API 稳定性）

**建议**：仅在明确需要 WS 压缩功能时才做此 PR。

---

### PR-5: hyper-util 可组合池 — G-01/G-02 根治（高工作量）

**范围**：
- 设计自定义 pool layer 组合（cache + expire + map）
- 实现连接健康检查（pre-request liveness probe）
- 实现连接 poisoning（请求失败标记连接为不健康）
- 替换 reqwest 默认池为自定义池
- E2E 弱网测试验证

**预估改动**：~200-300 行代码

**依赖**：PR-1（reqwest 0.13）

**风险**：高（hyper-util pool API 不稳定、连接池行为变化影响全局）

---

## 五、优先级与时间线建议

```
PR-1 (reqwest 0.13 升级)           ✅ 已完成
  │
  ├── PR-2 (内置 retry 替代)       ← 暂缓：reqwest-middleware 0.5 已兼容 0.13
  │
  ├── PR-5 (可组合池 G-01/G-02)    ← PR-1 后择机进行，收益最大
  │
PR-3 (tungstenite 0.29 升级)       ✅ 已完成
  │
PR-4 (yawc 替换)                   ← 观望；yawc 社区验证不足，优先考虑 Signal fork / upstream PR
```

| PR | 优先级 | 收益 | 风险 | 建议时机 |
|----|:------:|------|:----:|---------|
| PR-1 | ✅ 完成 | 解锁后续 + 维护性 | 中 | 已实施 |
| PR-2 | 中 | 减少依赖链 | 中 | 暂缓；现有 reqwest-retry 0.9 可用 |
| PR-3 | ✅ 完成 | 跟进上游 | 低 | 已实施 |
| PR-4 | 低 | WS 压缩 | 中高 | 观望；优先看 Signal fork / upstream PR |
| PR-5 | **高** | 根治 G-01/G-02 | 高 | PR-1 后评估 |

---

## 六、决策矩阵

| 问题 | 不做 | 升级 | 替换 |
|------|------|------|------|
| **reqwest 0.13** | 已过时：0.12 会逐渐脱节 | ✅ 已升级到 0.13.3 | N/A（reqwest 无替换必要） |
| **tungstenite 0.29** | 已过时：0.24 会逐渐脱节 | ✅ 已升级到 0.29.0，但不解决压缩 | N/A |
| **WS 压缩 (A-02)** | ❌ 仍不支持 | N/A（upstream PR 未合并） | yawc 可行但社区验证不足；更推荐 Signal fork / upstream PR experimental 路线 |
| **连接池 (G-01/G-02)** | 🟡 缓解状态持续 | reqwest 0.13 + hyper-util 可组合池 | N/A |

---

## 附录 A：reqwest-middleware 兼容性检查

已确认：

- `reqwest-middleware 0.5.1` 依赖 `reqwest 0.13.1`，兼容 reqwest 0.13
- `reqwest-retry 0.9.1` 与 `reqwest-middleware 0.5.1` 同仓维护，当前编译通过
- 项目直接 `rustls` provider 与 reqwest 0.13 默认 provider 统一为 `aws_lc_rs`，避免同时启用 `aws-lc-rs` 与 `ring` 导致 rustls 无法自动选择 provider

因此 PR-2 不再是 reqwest 0.13 升级的阻塞项，可作为后续依赖精简单独评估。

## 附录 B：yawc 成熟度评估清单

升级前需验证：

- [ ] yawc Autobahn 测试报告完整且通过
- [ ] yawc 最近 6 个月有 release
- [ ] yawc issue tracker 无未解决的 crash/data loss
- [ ] yawc 与 reqwest 0.13 兼容（或确认支持的 reqwest 版本）
- [ ] yawc 在 Linux/macOS/Windows 三平台可编译

## 附录 C：连接池配置现状

`catcher-http/src/transport/http_client.rs` 中 reqwest Client 构建：

```rust
// 当前连接池配置（G-01/G-02 缓解措施）
let mut builder = Client::builder()
    .pool_idle_timeout(Duration::from_secs(pool_idle_timeout_secs))  // 30s
    .pool_max_idle_per_host(max_idle)
    .tcp_keepalive(Some(Duration::from_secs(20)))
    .tcp_keepalive_interval(Some(Duration::from_secs(20)))
    .tcp_keepalive_retries(Some(3))
    .connect_timeout(Duration::from_millis(connect_timeout_ms))
    .timeout(Duration::from_millis(response_timeout_ms));
```

升级到 reqwest 0.13 + hyper-util 可组合池后，这些配置将分散到各 pool layer 中：

```rust
// 未来形态（PR-5 预览）
let pool = pool::cache(exec)
    .with_idle_timeout(Duration::from_secs(30))
    .with_max_idle_per_host(max_idle)
    .with_health_check(|conn| check_liveness(conn));  // 自定义健康检查
```
