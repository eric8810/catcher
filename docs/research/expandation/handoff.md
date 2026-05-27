# Handoff: 网络场景扩展调研

> 创建日期：2025-07-18
> 状态：第一阶段完成，可继续扩展
> 给你的继任者：这份文档帮你 10 分钟理解我们在做什么、怎么做、做到哪了、下一步去哪。

---

## 一、为什么要做这件事

### 出发点

catcher 是一个跨平台网络韧性库（HTTP + WebSocket + SSE），目标是在弱网、断连、高延迟等恶劣网络条件下保持高成功率。当前已有的设计文档和测试用例覆盖了基础功能路径和 8 个 E2E 场景，但我们想知道：

> **还有多少网络通讯场景、硬件、软件、环境、用户交互是我们没有考虑到的？**

这些遗漏的场景直接关系到 catcher 在真实生产环境中的可靠性——用户遇到问题不会说"你的设计文档很完整"，只会说"你的库在我这不行"。

### 目标

系统性地调研 catcher **当前未覆盖**的场景，输出可执行的测试用例建议和代码变更项。

---

## 二、研究方式

### 2.1 信息源

| 来源 | 比重 | 具体方法 |
|------|:----:|---------|
| 内部设计文档 | 40% | 精读全部 `docs/arch-rs/`、`docs/arch-ts/`、`docs/plan/`、`docs/issues/`，画出能力边界 |
| Web 搜索 | 35% | 搜索 RFC、竞品 issue tracker（reqwest#2283, tokio-tungstenite#35, ws#1617）、生产事故 postmortem（Mike Talbot SSE）、社区讨论（Stack Overflow, HN） |
| 竞品分析 | 15% | reqwest / axios / OkHttp / ws 库的 issue tracker 和测试套件 |
| 协议规范 | 10% | RFC 7231 (HTTP/1.1), RFC 7540 (HTTP/2), RFC 6455 (WebSocket), RFC 8305 (Happy Eyeballs), W3C SSE, OWASP CRLF Injection |

> 所有外部引用来源的完整索引详见 `00-summary.md` 第七节。

### 2.2 分析方法

对每个发现的场景，按以下矩阵评估：

```
场景描述
  → catcher 当前覆盖状态：✅ 已覆盖 / ⚠️ 部分覆盖 / ❌ 未覆盖
  → 严重度：🔴 高（影响正确性/安全性） / 🟡 中（影响体验/可维护性）
  → 验证方式：代码审查 / 社区报告 / 协议规范 / 生产事故
```

### 2.3 分类体系

6 个维度，42+ 个细分领域：

| 维度 | 关注点 | 示例场景 |
|------|--------|---------|
| A. 协议场景 | HTTP/WS/SSE 协议边界 | HTTP 408 应重试、WS send backpressure |
| B. 网络环境 | 代理/NAT/LB/CDN/云原生 | CGNAT 空闲超时、Happy Eyeballs |
| C. 硬件设备 | 移动/嵌入式/架构 | iOS 后台网络限制、ARM64 CI |
| D. 软件环境 | OS/浏览器/容器 | musl DNS 差异、WKWebView cookie |
| E. 用户交互 | 生命周期/并发/竞态 | 双重 destroy、429 限流风暴 |
| F. 安全攻击 | TLS/HTTP/WS/codec/FFI | CRLF 注入、msgpack 深度限制 |

---

## 三、当前进度与产出

### 3.1 已完成的文件

```
docs/
├── plan/
│   └── 05-expansion-research.md          # 调研计划（框架 + 方法论）
├── research/
│   └── expandation/
│       ├── 00-summary.md                 # 汇总报告（核心交付物）
│       ├── 01-protocols.md               # 阶段一：协议场景
│       ├── 02-network-env.md             # 阶段二：网络环境
│       ├── 03-hardware.md                # 阶段三：硬件设备
│       ├── 04-software-env.md            # 阶段四：软件环境
│       ├── 05-user-interaction.md        # 阶段五：用户交互
│       └── 06-security.md                # 阶段六：安全场景
```

### 3.2 核心数字

| 指标 | 数值 |
|------|:----:|
| 已读内部文档 | 80+ 篇 |
| Web 搜索次数 | 20+ |
| 识别高优先级缺失场景 | 25 个 |
| 识别中优先级缺失场景 | 16 个 |
| 推荐新增测试用例 | 30+ 个 |
| 建议代码变更 | 8 项 |

### 3.3 最重要的 5 个发现（TL;DR）

1. **HTTP 408 应重试** — `ErrorCategory` 将所有 4xx 判为 NonRetryable，但 408 是 keepalive race 的正常信号（已验证：`error.rs:82-88`）
2. **msgpack 无输入限制** — 恶意数据包可 OOM 或栈溢出，需加 `max_unpack_size` 和 `max_depth`（已验证：`codec.rs` 无任何限制）
3. **header value 无 CRLF 过滤** — 可能被注入构造 HTTP Response Splitting（已验证：`http_client.rs` 无校验）
4. **WS send 无背压** — 依赖 tokio-tungstenite 的已知缺陷，发送队列无限增长
5. **CGNAT 空闲超时** — 默认 keepAlive 30s 在某些 ISP 下可能不够，需文档说明

> **2025-07-21 验证更新**：
> - SSE 跨 chunk UTF-8 在 TS 侧已有测试（S8），但 Rust `SseStream` 仍用 `String::from_utf8_lossy` 有 Bug。
> - IPv6 host_mapping 代码已支持（`IpAddr::parse` 天然处理 IPv6），仅缺测试。
> - msgpack codec 位于 `packages/catcher-ws/src/codec.rs`，非 `catcher-core`。

---

## 四、如何继续推进

### 4.1 如果你要做"更多调研"

以下是第一阶段没深入、但有价值的方向：

| 方向 | 为什么需要 | 建议方法 |
|------|-----------|---------|
| **gRPC-web 协议** | 微服务主流协议，catcher 目前不支持 | 研读 gRPC-web spec + grpc-rs 测试套件 |
| **WebTransport** | HTTP/3 时代的 WS 替代品 | 跟踪 Chrome WebTransport API 进展 |
| **QUIC/HTTP/3** | reqwest 未来支持路线 | 跟踪 hyper 1.x + quinn 集成 |
| **CoAP (IoT)** | 低功耗设备协议 | 调研 IoT 客户需求 |
| **真实生产事故挖掘** | 理论分析有盲区 | 搜索 GitHub issues / Hacker News / postmortem |
| **竞品全覆盖对比** | OkHttp/Ktor/Retrofit 的测试套件 | 下载这些库的 test/ 目录，逐条对照 |

### 4.2 如果你要做"落地实施"

按紧迫度排序的代码变更（详见 `00-summary.md` 第四节）：

1. `catcher-core/src/error.rs` — 408 → Retryable（1 行）
2. `packages/catcher-ws/src/codec.rs` — 增加 `max_unpack_size` / `max_depth`（~20 行）
3. `catcher-http/src/sse/stream.rs` — 字节级 buffering 替换 `String::from_utf8_lossy`（Rust 侧 Bug）
4. `catcher-http/src/transport/http_client.rs` — CRLF 过滤
5. `catcher-ws/src/transport/ws_client.rs` — send backpressure
6. `catcher-core/src/types/resilience.rs` — `RetryConfig.respect_retry_after`
7. ~~`catcher-http/src/transport/dns.rs` — IPv6 host_mapping~~（已支持，仅缺测试）
8. `catcher-http/src/resilience/circuit_breaker.rs` — `min_failure_window_ms`

### 4.3 如果你想写测试用例

参考 `00-summary.md` 第三节的测试清单，按以下顺序：

1. **安全优先** — `MP1→MP3`（msgpack 安全）、`H21→H22`（CRLF 注入）
2. **正确性** — `R14`（408 重试）、`R15`（429 Retry-After）
3. **FFI 边界** — `FFI1→FFI5`（null 安全、并发安全）
4. **稳定性** — `LR1→LR4`（长时运行、资源泄漏）
5. **网络拓扑** — `NET1→NET4`（利用现有 NetworkProxy 扩展）

---

## 五、文件与关键概念速查

### 5.1 如果你需要快速理解 catcher 架构

```
docs/arch-rs/14-workspace.md       ← 7 个 crate 的依赖关系
docs/arch-rs/02-module-tree.md     ← 每个 crate 的源码文件清单
docs/arch-rs/03-types.md           ← 核心类型定义（error/config/http/ws）
docs/arch-rs/04-transport.md       ← HTTP/WS 传输层
docs/arch-rs/05-resilience.md      ← retry/CB/backoff/timeout
docs/arch-rs/09-ffi.md             ← C ABI 25 个符号
docs/arch-ts/00-overview.md        ← TS 包依赖关系
```

### 5.2 如果你需要了解已知问题

```
docs/issues/README.md              ← 问题总索引（E2E 测试发现的 12 个问题）
docs/issues/arch-gap-audit-2026.md ← 架构差距审计
docs/issues/api-gap-features.md    ← API 功能缺口（G1→G12）
docs/issues/native-layer-capability-gaps.md ← 原生层能力缺口（N-01→N-04）
```

### 5.3 关键约束（来自 AGENTS.md 和 RUST_STYLE_GUIDE.md）

- 禁止 `use xxx::*;` 通配符导入
- 禁止在多个 crate 中重复定义相同函数（公共 helper 归入 catcher-core）
- Phase 追踪注释（`// Phase 3`）禁止提交
- 非测试代码禁止无注释的 `unwrap()`
- FFI 层 `CString::new()` 前必须 `replace('\0', "")`

---

## 六、未覆盖的盲区（已知的未知）

在第一阶段调研中有意跳过的领域，因为：
- 与 catcher 的客户端库定位距离较远
- 需要特定硬件/环境才能验证
- 属于 niche 场景，优先级低

| 盲区 | 原因 | 何时应该捡起来 |
|------|------|-------------|
| 服务端行为模拟的保真度 | catcher 是客户端库 | 如果在特定服务端（如 Nginx/Apache）前出现奇怪行为 |
| QUIC/HTTP3 实际测试 | 生态不成熟 | 当 reqwest 正式支持 QUIC |
| 真实卫星链路测试 | 需要硬件 | 如果有卫星通信客户 |
| Bluetooth/ NFC 等非 IP 网络 | 不在 TCP 范围 | 如果产品方向扩展到非 IP 协议 |
| Tor / I2P 匿名网络 | niche | 如果隐私成为核心卖点 |
| 离线优先/CRDT 同步 | 应用层逻辑 | 如果 catcher 扩展为数据同步库 |

---

## 七、给你的建议

1. **先读 `00-summary.md`** — 5 分钟了解全貌
2. **再读 `05-expansion-research.md`** — 理解调研框架
3. **对照 `docs/issues/README.md`** — 理解已知问题与调研发现的重叠和互补
4. **如果要做代码变更**：从 `🔴 高优先级` 的 8 项开始，每项改动都很小（通常 ≤10 行）
5. **如果要做更多调研**：重点投入"真实生产事故挖掘"和"竞品全覆盖对比"
6. **调研报告格式约定**：每个场景用 `| 场景 | 描述 | catcher 覆盖 | 建议 |` 四列表格，保持一致性

---

## 八、联系方式（留给你填写）

> 当前维护者签名：________
> 接手日期：________
