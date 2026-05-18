# 05 — 网络场景扩展调研计划

> 目标：对 catcher 当前未覆盖的网络通讯场景、硬件、软件、环境、用户交互等进行系统调研，
> 产出可直接落地为测试用例的场景清单。
> 调研范围：catcher 作为跨平台网络韧性库，需覆盖其所有绑定平台 (Node.js / Browser / Rust / Dart / Swift / Kotlin)。

---

## 一、catcher 当前能力边界

基于对全部设计文档的精读，catcher 当前已覆盖：

| 能力维度 | 已覆盖 | 备注 |
|---------|:------:|------|
| HTTP/1.1 + HTTP/2 | ✅ | reqwest 底层支持 |
| WebSocket (ws:// wss://) | ✅ | tokio-tungstenite + ws (Node) |
| SSE (GET 长连接 + POST 流式) | ✅ | SseClient + SseStream |
| msgpack 编解码 | ✅ | rmp-serde |
| 指数退避 + Jitter | ✅ | backon / p-retry |
| 熔断器 (CLOSED/OPEN/HALF_OPEN) | ✅ | circuitbreaker-rs / cockatiel |
| 自适应超时 (P90 RTT) | ✅ | AdaptiveTimeout |
| 优先级队列 + 并发控制 | ✅ | Semaphore + mpsc channel |
| 连接池 (keepAlive) | ✅ | reqwest pool + Node https.Agent |
| DNS 缓存 + host_mapping | ✅ | hickory-dns + cacheable-lookup |
| TLS (rustls + native-tls) | ✅ | 含 mTLS / CA / pin |
| HTTP 代理 (HTTP/SOCKS5) | ✅ | 含认证 |
| 重定向控制 | ✅ | maxRedirects + beforeRedirect |
| 多端点竞速 (WS) | ✅ | raceEndpoints |
| 流式下载 (N-02) | ✅ | execute_stream |
| 请求取消 (全局 + 单请求) | ✅ | CancellationToken |
| 网络质量评估 | ✅ | NetworkQualityEvaluator |
| Basic Auth + Bearer Token | ✅ | 静态 + 动态刷新 |
| CORS / credentials | ✅ | TS/Web 层 |
| FormData / 文件上传 | ✅ | TS 层 (Rust P2) |

---

## 二、调研维度与分类体系

按以下 6 个维度展开调研，每个维度下细分具体场景：

### 维度 A：网络通讯协议场景
> 协议级别、传输层、应用层协议、编码格式、协议组合

### 维度 B：网络环境与拓扑
> 网络架构、中间设备、链路特性、地域延迟

### 维度 C：硬件与设备场景
> 终端类型、芯片架构、网络硬件、传感器网络

### 维度 D：软件运行环境
> 操作系统、运行时、容器、沙箱、浏览器引擎

### 维度 E：用户交互与极端场景
> 用户行为、生命周期、并发模式、资源约束

### 维度 F：安全与攻击场景
> TLS 攻击、协议降级、注入、重放、中间人

---

## 三、调研方法

每个维度采用以下调研路径：

1. **行业标准查阅** — RFC、W3C 规范、IANA 注册表
2. **竞品分析** — reqwest/axios/OkHttp/Retrofit 等成熟库的 issue tracker 和测试套件
3. **生产事故案例** — postmortem、CNCF 故障报告、云厂商最佳实践
4. **社区讨论** — Stack Overflow、Reddit、Hacker News 高频问题
5. **协议规范研读** — HTTP/2 RFC 7540、HTTP/3 RFC 9114、WebSocket RFC 6455、SSE W3C

---

## 四、分阶段执行计划

### 阶段一：网络通讯协议场景 (维度 A)
**调研时间**：阶段一

详细覆盖：
- A1: HTTP 协议变体与扩展
- A2: WebSocket 高级特性与边界
- A3: SSE 协议边界与互操作
- A4: 其他实时通讯协议 (gRPC, WebTransport, MQTT, QUIC)
- A5: 编码格式与序列化边界

### 阶段二：网络环境与拓扑 (维度 B)
**调研时间**：阶段二

详细覆盖：
- B1: 代理与中间件环境
- B2: 云原生与微服务网络
- B3: CDN 与边缘计算
- B4: 特殊网络拓扑 (IPv6, NAT, VPN, Air-Gapped)

### 阶段三：硬件与设备场景 (维度 C)
**调研时间**：阶段三

详细覆盖：
- C1: 移动设备网络特性
- C2: IoT / 嵌入式设备
- C3: 不同 CPU 架构与字节序

### 阶段四：软件运行环境 (维度 D)
**调研时间**：阶段四

详细覆盖：
- D1: 操作系统差异 (Linux/macOS/Windows)
- D2: 浏览器引擎差异
- D3: 容器与沙箱环境

### 阶段五：用户交互与极端场景 (维度 E)
**调研时间**：阶段五

详细覆盖：
- E1: 应用生命周期事件
- E2: 极端并发与资源耗尽
- E3: 弱网与间断性网络

### 阶段六：安全与攻击场景 (维度 F)
**调研时间**：阶段六

详细覆盖：
- F1: TLS 相关攻击
- F2: HTTP 协议攻击
- F3: WebSocket 攻击

---

## 五、产出物规划

| 产出物 | 路径 | 内容 |
|--------|------|------|
| 调研计划 | `docs/plan/05-expansion-research.md` | 本文档 |
| 阶段一报告 | `docs/research/expandation/01-protocols.md` | 协议场景调研 |
| 阶段二报告 | `docs/research/expandation/02-network-env.md` | 网络环境调研 |
| 阶段三报告 | `docs/research/expandation/03-hardware.md` | 硬件场景调研 |
| 阶段四报告 | `docs/research/expandation/04-software-env.md` | 软件环境调研 |
| 阶段五报告 | `docs/research/expandation/05-user-interaction.md` | 用户交互调研 |
| 阶段六报告 | `docs/research/expandation/06-security.md` | 安全场景调研 |
| 汇总报告 | `docs/research/expandation/00-summary.md` | 场景覆盖矩阵 + 测试用例推荐 |
