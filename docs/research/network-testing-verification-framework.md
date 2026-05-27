# 网络条件标准、模拟方案与测试机制 — 完整调研框架

> 创建日期：2026-07
> 核心命题：catcher 的目标是为**绝大多数网络场景**提供最佳的韧性配置和托底方案。
> 要达成这个目标，必须先回答：**业界到底有多少种网络条件？它们的权威标准是什么？业界用什么方式模拟和验证？**

---

## 〇、核心逻辑链

```
网络条件分类学              权威标准                  模拟方案                  测试机制
(Condition Taxonomy)  →   (Industry Standards)  →  (Simulation Methods)  →  (Verification)

  「真实世界有什么」        「谁定义的、参数从哪来」    「怎么在实验室复现」      「怎么证明复现有效」
```

catcher 的每个 profile、每个损伤参数、每个测试场景，都应该能沿着这条链追溯到权威来源。
当前状态：

| 环节 | 现状 | 差距 |
|------|------|------|
| 条件分类学 | 分散在 6 份扩研报告中，未统一 | 缺系统性的分类体系 |
| 权威标准 | 部分引用（Chrome DevTools / WebPageTest） | 大量参数无标准溯源，RTT/带宽/丢包取值靠"合理估计" |
| 模拟方案 | proxy.ts 实现 12 种损伤 | 未对标行业工具，未做真实度校验 |
| 测试机制 | S1-S16 + harness 对比 | 无法回答"模拟和真实差多少" |

---

## 一、网络条件分类学（Condition Taxonomy）

### 1.1 顶层分类框架

```
网络条件
├── L1: 物理链路层
│   ├── 有线接入 (DSL / Cable / Fiber / Ethernet / Powerline)
│   ├── 无线蜂窝 (2G / 2.5G / 3G / 4G / 5G NSA / 5G SA)
│   ├── 无线局域网 (WiFi a/b/g/n/ac/ax/be)
│   ├── 卫星通信 (GEO / MEO / LEO)
│   ├── LPWAN / IoT (LoRa / NB-IoT / LTE-M / Sigfox)
│   └── 特殊链路 (蓝牙 PAN / 无人机图传 / 水下声学)
│
├── L2: 网络拓扑层
│   ├── 局域网 (LAN / VLAN / SD-LAN)
│   ├── 广域网 (WAN / SD-WAN / MPLS)
│   ├── 隧道 (VPN / IPSec / WireGuard / SSH Tunnel)
│   ├── 覆盖网络 (CDN / Edge / Mesh / P2P)
│   └── 虚拟网络 (容器网络 / Service Mesh / VPC / Overlay)
│
├── L3: 中间件层
│   ├── 代理 (Forward / Reverse / Transparent / SOCKS / PAC / WPAD)
│   ├── 负载均衡 (L4 / L7 / DNS-based)
│   ├── NAT (Full Cone / Restricted / Symmetric / CGNAT)
│   ├── 防火墙 (Stateful / DPI / WAF / 协议白名单)
│   └── 网关 (API Gateway / 协议转换 / TLS Termination)
│
├── L4: 协议行为层
│   ├── TCP (拥塞控制变体 / KeepAlive / Selective ACK / ECN)
│   ├── TLS (版本协商 / 证书链 / OCSP / 0-RTT)
│   ├── DNS (递归/迭代 / EDNS / DNSSEC / DNS-over-TLS / DNS-over-HTTPS)
│   ├── HTTP (1.0 / 1.1 / 2 / 3 / 版本协商 / GOAWAY)
│   └── 应用协议 (WebSocket / SSE / gRPC / WebTransport / MQTT)
│
└── L5: 运行时环境层
    ├── OS 网络栈 (Linux TCP stack / macOS Network.framework / Windows Winsock)
    ├── 移动 OS (iOS 后台限制 / Android Doze / Data Saver)
    ├── 浏览器引擎 (V8 fetch / WKWebView / Web Worker)
    ├── 容器与编排 (Docker DNS 127.0.0.11 / K8s CNI / Service Mesh sidecar)
    └── Serverless (Lambda / Workers / 连接池无意义)
```

### 1.2 各层级的关键退化模式

| 层级 | 退化类型 | 具体表现 | 对 catcher 的影响 |
|------|---------|---------|------------------|
| L1 物理 | 带宽受限 | 50bps (SMS) → 20Gbps (5G mmWave) | 1:400,000,000 的动态范围 |
| L1 物理 | 延迟变化 | 0.1ms (LAN) → 1200ms (GEO 卫星) | 1:12,000 的动态范围 |
| L1 物理 | 丢包模式 | 0.001% (光纤) → 30% (战场无线电) | 独立丢包 vs 突发丢包 vs 周期性丢包 |
| L2 拓扑 | 路由黑洞 | 上游设备静默丢弃 | 无 RST，hang 到超时 |
| L2 拓扑 | 非对称路由 | 去程/回程不同路径 | 一个方向丢包，另一个正常 |
| L2 拓扑 | 间歇连通 | 网络 flapping | CB 状态机压力 |
| L3 中间件 | 空闲超时 | LB/NAT/代理主动断开 | 僵尸连接 / keepAlive race |
| L3 中间件 | 缓冲/改写 | 代理缓冲 chunked 响应 | SSE 阻塞 / Content-Type 被改 |
| L4 协议 | 版本不匹配 | HTTP/2 协商失败 / TLS 降级 | 协议回退行为 |
| L4 协议 | 实现 Bug | 特定 OS/设备的 TCP 栈缺陷 | 非标准行为需容错 |
| L5 环境 | 资源限制 | CPU throttling / OOM / fd 耗尽 | 请求失败分类 |

---

## 二、权威标准溯源

每一个 catcher profile 参数都应该能追溯到权威标准。以下按层级列出标准来源：

### 2.1 蜂窝网络标准（3GPP）

| 技术 | 3GPP 规范 | 关键参数 | 与 catcher 的对应 |
|------|---------|---------|------------------|
| GPRS (2.5G) | TS 45.001 | CS-1~CS-4 编码方案，理论 9.05~21.4 kbps/slot | `gprs` profile (50kbps 下行) |
| EDGE (2.75G) | TS 45.001 | MCS-1~MCS-9，理论最大 59.2 kbps/slot | `2g_regular` profile (250kbps) |
| UMTS (3G) | TS 25.101 | QPSK/16QAM，RTT ~50-200ms | `3g_slow` / `3g_good` |
| HSPA+ (3.5G) | TS 25.306 | 64QAM + MIMO，理论 21-42 Mbps | `3g_good` profile (1.5Mbps 下行) |
| LTE (4G) | TS 36.101 | OFDMA，RTT ~10-50ms，理论 100-300 Mbps | `4g_lte` profile |
| 5G NR | TS 38.101 | mmWave + sub-6，RTT ~1-10ms | **无对应 profile** |
| 5G SA (URLLC) | TS 38.211 | 超低延迟模式 <1ms | **无对应 profile** |

**调研任务 C1**：每个 profile 的参数标注 3GPP 来源，标注"理论值 vs 实测典型值"的差异。

### 2.2 WiFi 标准（IEEE 802.11）

| 标准 | IEEE | 频段 | 理论速率 | 典型 RTT | 关键损伤 |
|------|------|------|---------|---------|---------|
| 802.11b | 1999 | 2.4GHz | 11 Mbps | 5-10ms | 干扰严重、隐藏节点 |
| 802.11g | 2003 | 2.4GHz | 54 Mbps | 3-5ms | 与 b 共存降速 |
| 802.11n (WiFi 4) | 2009 | 2.4/5GHz | 600 Mbps | 2-3ms | 40MHz 干扰、相邻信道 |
| 802.11ac (WiFi 5) | 2013 | 5GHz | 6.9 Gbps | 1-2ms | DFS 信道规避 |
| 802.11ax (WiFi 6) | 2021 | 2.4/5/6GHz | 9.6 Gbps | <1ms | OFDMA 调度延迟 |
| 802.11be (WiFi 7) | 2024 | 2.4/5/6GHz | 46 Gbps | <0.5ms | MLO 多链路聚合 |

**调研任务 C2**：WiFi 特有的损伤——AP 漫游 (BSS Transition) 的 50-100ms 中断、同频干扰的周期性丢包、DFS 信道切换的 1-10s 断连——这些目前 catcher profile 完全未建模。

### 2.3 卫星通信标准

| 轨道 | 高度 | 单向延迟 | 典型丢包 | 带宽 | 标准来源 |
|------|------|---------|---------|------|---------|
| GEO | 35,786 km | 240-280ms | 0.1-2% | 1-100 Mbps | ITU-R S.1711 |
| MEO | 8,000-20,000 km | 50-140ms | 0.5-3% | 10-500 Mbps | ITU-R S.1712 |
| LEO (Starlink) | 340-1,200 km | 25-65ms | 0.5-5% | 50-500 Mbps | SpaceX 公开数据 |
| LEO (OneWeb) | 1,200 km | 30-80ms | 0.5-3% | 50-200 Mbps | — |

**调研任务 C3**：Starlink 的 15s 周期性抖动（卫星切换）、雨天衰减（Rain Fade）、Dishy 预热延迟——这些 LEO 特有的模式目前 catcher 未覆盖。

### 2.4 有线接入标准

| 技术 | 标准 | 下行/上行 | 典型 RTT | 丢包 |
|------|------|----------|---------|------|
| ADSL2+ | ITU G.992.5 | 24/1.4 Mbps | 10-30ms | 0.01-0.1% |
| VDSL2 | ITU G.993.2 | 100/50 Mbps | 5-15ms | 0.01-0.1% |
| Cable (DOCSIS 3.1) | ITU J.222 | 10/1 Gbps | 8-20ms | 0.01-0.5% |
| GPON (Fiber) | ITU G.984 | 2.5/1.25 Gbps | 1-4ms | <0.001% |
| XGS-PON | ITU G.9807 | 10/10 Gbps | 1-3ms | <0.001% |

### 2.5 网络损伤标准

| 损伤类型 | 标准/模型 | 定义 | catcher 对标 |
|---------|---------|------|-------------|
| 延迟 | ITU-T G.114 | 一程延迟 < 150ms (可接受), > 400ms (不可接受) | `latency` 参数 |
| 抖动 | RFC 3393 | IP 包延迟变化 (IPDV) | `jitter` 参数 |
| 丢包 | ITU-T G.109 | 0-3% (好), 3-15% (中等), >15% (差) | `packetLoss` 参数 |
| 突发丢包 | Gilbert-Elliott (1960/1963) | 两状态 Markov | `burstLoss` 配置 |
| 带宽 | RFC 3135 | 性能评估的基准 | `bandwidth` 参数 |
| 包乱序 | RFC 4737 | 包重排度量 | `reorder` 配置 |
| 包重复 | RFC 5560 | 包复制事件 | `duplicate` 配置 |

**调研任务 C4**：catcher 的损伤分类学是否完整——与 [tc netem](https://man7.org/linux/man-pages/man8/tc-netem.8.html)、[ns-3](https://www.nsnam.org/docs/models/html/index.html)、[MahiMahi](http://mahimahi.mit.edu/) 的损伤模型做完整对标。

### 2.6 协议行为标准

| 协议行为 | RFC / 标准 | catcher 相关 |
|---------|-----------|-------------|
| HTTP/1.1 keepAlive race | RFC 7231 §6.5.7 (408) | 错误分类 |
| HTTP/2 GOAWAY | RFC 7540 §6.8 | 重试策略 |
| HTTP/2 connection coalescing | RFC 7540 §9.1 | 连接池行为 |
| HTTP Retry-After | RFC 7231 §7.1.3 | retry 策略 |
| WebSocket Close | RFC 6455 §7 | 重连逻辑 |
| WebSocket Ping/Pong | RFC 6455 §5.5.2 | keepAlive |
| SSE Last-Event-ID | WHATWG HTML Standard §9.2 | 重连去重 |
| SSE retry field | WHATWG HTML Standard §9.2 | 重连间隔 |
| TCP KeepAlive | RFC 1122 §4.2.3.6 | 空闲连接检测 |
| TCP User Timeout | RFC 5482 | 数据重传超时 |
| DNS TTL | RFC 1035 §4.1.3 | 缓存策略 |
| Happy Eyeballs v2 | RFC 8305 | IPv4/IPv6 竞速 |
| TLS 1.3 0-RTT | RFC 8446 §8 | 重放安全性 |
| QUIC Connection Migration | RFC 9000 §9 | 网络切换 |

**调研任务 C5**：每个 RFC 中定义的"客户端应该做什么"与 catcher 的实际行为对照，产出协议合规矩阵。

---

## 三、业界模拟方案对标

### 3.1 模拟层级与工具全景

```
模拟层级                  代表性工具                    模拟真实度      catcher 可达性
─────────────────────────────────────────────────────────────────────────────────
L4: 物理设备仿真          Spirent / Ixia / Keysight    极高 ($$$$$)    不可达
L3: 全协议栈仿真          ns-3 / OMNeT++               高              可参考模型
L2: 内核级网络模拟        tc netem / iptables / Dummynet 高             可对标参数
L1: 应用层代理模拟        Comcast / toxiproxy / clumsy  中              proxy.ts 所在层
L0: 代码级 Mock           wiremock / nock / MSW        低              已有
```

catcher 的 proxy.ts 处于 L1 层——应用层代理模拟。这意味着：

1. **能做**：延迟、丢包、带宽、黑洞——TCP 层之上的一切
2. **不能做**：TCP 拥塞控制行为、SYN 重传、MTU 分片、ICMP 不可达——这些发生在内核层，proxy 无法模拟
3. **模糊地带**：TCP 连接断开（RST/FIN）proxy 可以模拟，但 TFO (TCP Fast Open)、TCP_USER_TIMEOUT 等内核参数无法模拟

### 3.2 各工具的损伤模型对比

此表是调研的核心产出——我们需要知道每个工具的损伤模型细节，才能判断 proxy.ts 的完整度：

| 损伤类型 | tc netem | ns-3 | Comcast | toxiproxy | MahiMahi | proxy.ts |
|---------|:------:|:----:|:------:|:--------:|:--------:|:--------:|
| 固定延迟 | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| 延迟分布 | normal/pareto/paretonormal | 多种分布 | ✅ | — | replay trace | uniform/normal |
| 延迟相关性 (jitter correlation) | ✅ (correlation %) | ✅ | — | — | ✅ (trace) | ❌ |
| 独立丢包 | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| 突发丢包 (N-state Markov) | ✅ (4-state) | ✅ (Gilbert-Elliott) | — | — | ✅ (trace) | ✅ (2-state) |
| 丢包相关性 | ✅ (gemodel) | ✅ | — | — | ✅ (trace) | ❌ |
| 带宽限速 | ✅ (rate/tbf) | ✅ | ✅ | ✅ | ✅ | ✅ |
| 带宽分层 (per-flow vs aggregate) | ✅ (htb/prio) | ✅ | — | — | — | ❌ |
| 包损坏 (corrupt) | ✅ | ✅ | — | — | — | ✅ |
| 包乱序 (reorder) | ✅ | ✅ | — | — | — | ✅ |
| 包重复 (duplicate) | ✅ | ✅ | — | — | — | ✅ |
| 路由黑洞 | ⚠️ (iptables DROP) | ✅ | — | ✅ (timeout) | — | ✅ |
| TCP RST 注入 | ✅ (iptables REJECT) | ✅ | — | ✅ | — | ⚠️ (destroy) |
| 上下行不对称 | ⚠️ (ifb + tc) | ✅ | ✅ | — | ✅ (trace) | ✅ |
| MTU 限制 | ✅ | ✅ | — | — | ✅ (trace) | ❌ |
| 真实 trace 回放 | ❌ | ✅ (trace sources) | — | — | ✅ (核心功能) | ❌ |
| 动态条件切换 | — | ✅ | — | ✅ | — | ✅ (setConditions) |

**关键差距**：
1. **延迟相关性**（`correlation`）：真实网络的延迟不是独立的——如果当前包延迟了 200ms，下一个包大概率也在 180-220ms。独立 jitter 会低估超时概率。
2. **4-state Markov 丢包**：Gilbert-Elliott 的 2-state 过于简化。4-state 可以区分"轻度拥塞"和"严重拥塞"。
3. **trace 回放**：这是 MahiMahi 的核心功能——录制真实网络 trace 并在实验室精确回放。proxy.ts 不支持。
4. **aggregate 带宽**：多个连接共享一个瓶颈带宽——proxy.ts 每个连接独立限速，无法模拟多连接争抢行为。

### 3.3 行业测试方法对标

| 行业/场景 | 测试方法 | 工具 | catcher 可借鉴 |
|----------|---------|------|---------------|
| **浏览器性能测试** | WebPageTest / Lighthouse / Chrome UX Report | WPT agents + throttling profiles | profile 参数对标 |
| **移动网络测试** | 3GPP conformance tests | Anritsu / Rohde & Schwarz 测试仪 | 参数溯源 |
| **CDN 测试** | RUM (Real User Monitoring) + Synthetic | Cloudflare / Akamai RUM | SLO 定义 |
| **云网络测试** | AWS FIS / GCP Network Chaos | 托管混沌工程 | 故障注入模式 |
| **游戏网络测试** | Network Emulation (GGPO/Rollback) | 自定义 netcode 测试 | 长时间运行测试 |
| **金融网络测试** | FIX protocol testing | 专有工具 | 协议合规性测试 |
| **IoT 网络测试** | LoRaWAN / NB-IoT 一致性测试 | 3GPP conformance | LPWAN profile |

---

## 四、测试验证机制分类学

### 4.1 验证层次

```
Layer 5: 生产验证 ── RUM / 金丝雀 / A/B / SLO 监控
Layer 4: 场景验证 ── E2E 场景 / 用户旅程 / 混沌实验
Layer 3: 属性验证 ── 不变量检查 / 统计对比 / 协议合规
Layer 2: 功能验证 ── 单元测试 / 集成测试 / FFI 边界
Layer 1: 静态验证 ── 类型检查 / clippy / 编译检查
```

catcher 当前覆盖：Layer 1-2 充分，Layer 3 缺失（无属性基测试），Layer 4 有基础但缺统计严谨性，Layer 5 完全缺失。

### 4.2 验证机制分类

| 机制 | 描述 | 适用场景 | catcher 状态 |
|------|------|---------|:----------:|
| **一致性验证** | 给定输入，输出是否与协议规范一致？ | 协议行为 | ✅ 单元测试 |
| **不变量验证** | 无论网络条件如何变化，某属性始终为真？ | 韧性保证 | ❌ 缺 PBT |
| **统计对比** | 在相同条件下，catcher 是否统计显著地优于 vanilla？ | 韧性效果 | ⚠️ 有 harness 但缺 power analysis |
| **属性验证** | 对任意损伤组合，retry 次数 ≤ max_attempts？ | 安全性 | ❌ |
| **协议合规** | 对 HTTP/1.1/2 协议的行为是否符合 RFC？ | 互操作性 | ❌ |
| **资源安全** | 长时间运行下 fd/内存/CPU 是否稳定？ | 可靠性 | ⚠️ 部分 |
| **安全验证** | CRLF 注入、DoS 攻击等是否被防御？ | 安全性 | ⚠️ 部分 |
| **生产验证** | 真实用户流量下 catcher 的表现？ | 最终验证 | ❌ |

### 4.3 业界验证工具

| 工具 | 类型 | 用途 | catcher 可借鉴 |
|------|------|------|---------------|
| [fast-check](https://fast-check.dev/) | PBT (JS) | 自动生成随机输入并检查不变量 | TS 层属性测试 |
| [proptest](https://docs.rs/proptest/latest/proptest/) | PBT (Rust) | Rust 属性测试 | Rust 层属性测试 |
| [GoReplay](https://goreplay.org/) | 流量回放 | 录制 HTTP 流量并重放 | 流量回放测试 |
| [mitmproxy](https://mitmproxy.org/) | 代理 | 可编程的中间人代理（Python） | 复杂代理行为模拟 |
| [Chaos Mesh](https://chaos-mesh.org/) | 混沌工程 | K8s 混沌注入 | 容器化混沌测试 |
| [toxiproxy](https://github.com/Shopify/toxiproxy) | 损伤代理 | TCP 层损伤注入 | 损伤模型对标 |
| [wrk2](https://github.com/giltene/wrk2) | 负载生成 | 恒定速率 HTTP 负载 | 吞吐基准 |
| [httpwg test suite](https://github.com/httpwg/http-test-suite) | 协议测试 | HTTP 协议合规性测试套件 | 协议合规 |

---

## 五、调研路线图

### Phase 1：标准溯源（优先级 🔴）

**目标**：catcher 的每个 profile 参数都有权威标准可追溯。

| 任务 | 产出 | 工作量 |
|------|------|:------:|
| **蜂窝网络标准溯源**：3GPP TS 45/25/36/38 系列中与 RTT、带宽、丢包相关的参数 | `docs/research/standards/cellular-3gpp.md` | 3 天 |
| **WiFi 标准 + 特有损伤建模**：802.11 系列 + BSS Transition / DFS / 干扰模式 | `docs/research/standards/wifi-ieee80211.md` | 2 天 |
| **卫星通信标准**：ITU-R + Starlink 实测数据 | `docs/research/standards/satellite-itu.md` | 1 天 |
| **Profile 参数溯源表**：所有 14 个 profile 的每个数值标注标准来源 | 更新 `docs/test/02-profiles.md` | 1 天 |

### Phase 2：模拟方案对标（优先级 🔴）

**目标**：知道 proxy.ts 和业界工具的差距，确定补强方向。

| 任务 | 产出 | 工作量 |
|------|------|:------:|
| **tc netem 完整能力对标**：逐项对比 proxy.ts 和 netem 的损伤模型 | `docs/research/simulation/tc-netem-benchmark.md` | 2 天 |
| **MahiMahi trace replay 调研**：评估引入真实网络 trace 回放的可行性 | `docs/research/simulation/trace-replay-feasibility.md` | 1 天 |
| **ns-3 模型参考**：从中提取比 Markov 2-state 更真实但又不至于过重的模型 | `docs/research/simulation/ns3-models-reference.md` | 2 天 |
| **Comcast / toxiproxy 对标**：看看 Go 生态的损伤代理有什么 proxy.ts 没做的 | `docs/research/simulation/industry-proxy-comparison.md` | 1 天 |

### Phase 3：协议合规矩阵（优先级 🟡）

**目标**：每个 RFC 中"客户端应该做什么"与 catcher 行为的对照。

| 任务 | 产出 | 工作量 |
|------|------|:------:|
| **HTTP/1.1 合规**：RFC 7230-7235 客户端行为检查 | `docs/research/compliance/http11-rfc.md` | 2 天 |
| **HTTP/2 合规**：RFC 7540 客户端行为检查 | `docs/research/compliance/http2-rfc.md` | 1 天 |
| **WebSocket 合规**：RFC 6455 客户端行为检查 | `docs/research/compliance/ws-rfc.md` | 1 天 |
| **SSE 合规**：WHATWG HTML Standard §9.2 | `docs/research/compliance/sse-whatwg.md` | 1 天 |

### Phase 4：验证机制补齐（优先级 🟡 → 🟢）

**目标**：引入属性基测试、统计验证、生产验证等新范式。

| 任务 | 产出 | 工作量 |
|------|------|:------:|
| **PBT 原型（TS 层）**：用 fast-check 验证 retry/CB 的不变量 | PoC 代码 | 2 天 |
| **统计功效分析**：确定 E2E 测试的最小迭代次数 | 分析报告 | 1 天 |
| **SLO 定义**：catcher 库的 SLO（成功率/延迟/CB 恢复时间） | SLO 文档 | 1 天 |
| **Flaky test 检测**：CI 中自动标记不稳定测试 | CI 脚本 | 1 天 |

### Phase 5：缺失条件补充（优先级 🟢 长期）

**目标**：补充“完全未覆盖”类型的网络条件。

目前已知的完全未覆盖：

| 缺失条件 | 所属层级 | 重要性 | 原因 |
|---------|:------:|:------:|------|
| TCP 拥塞控制交互 (CUBIC/BBR/Reno) | L4 协议 | 中 | proxy 无法模拟内核行为 |
| WiFi BSS Transition (漫游中断 50-100ms) | L1 物理 | 中 | 短暂丢包与独立丢包不同 |
| 5G NR / URLLC 超低延迟 | L1 物理 | 低 | 用户群小 |
| LPWAN (LoRa / NB-IoT) | L1 物理 | 低 | IoT 场景，catcher 可能不适合 |
| DNS over HTTPS / DNS over TLS | L4 协议 | 中 | 代理绕过场景 |
| MTU 变化 (PPPoE / VPN / IPv6-in-IPv4) | L1 物理 | 中 | 分片问题 |
| 容器 CNI 网络 (Calico/Cilium/Flannel) | L2 拓扑 | 低 | 对客户端透明 |
| 企业 NTLM/Kerberos 代理认证 | L3 中间件 | 低 | 超出范围 |

---

## 六、最终目标：完整的追溯链

调研完成后，catcher 的每个测试决策都应该能回答四个问题：

```
Q1: 为什么要测试这个条件？
A1: 因为它对应 3GPP TS 38.101 §7.2 定义的 4G LTE 延迟特性（标准溯源）

Q2: 这个模拟参数凭什么？
A2: 与 tc netem "delay 20ms 5ms distribution normal" 对标（模拟标标）

Q3: 怎么证明模拟是有效的？
A3: 对比 ns-3 LTE 模型仿真结果，proxy.ts 延迟分布在 p50/p95/p99 上偏差 < 15%（真实度校验）

Q4: 怎么验证 catcher 在这个条件下表现得当？
A4: PBT 验证 retry count ≤ max_attempts + S1-S16 场景对比统计显著（测试机制）
```

当前能完整回答这四个问题的 profile：0 个。这是差距，也是方向。

---

## 七、已产出文件

```
docs/research/
├── network-testing-verification-framework.md   ← 调研框架总纲
│
├── standards/                                   ← ✅ 已完成
│   ├── cellular-3gpp.md                         ← 蜂窝 2G→5G 标准溯源 + 切换 + 一致性测试
│   ├── wifi-ieee80211.md                        ← WiFi 损伤模式 (BSS/DFS/PS/MAC重试)
│   ├── protocol-behaviors.md                    ← TCP/TLS/DNS/HTTP/WS/SSE/QUIC RFC 行为对照
│   ├── os-hardware-quirks.md                    ← OS底层/移动端/硬件网络栈陷阱与测试案例
│   ├── satellite-itu.md                         ← (待补充)
│   └── wired-itu-ieee.md                        ← (待补充)
│
├── simulation/                                  ← ✅ 已完成
│   └── tools-benchmark.md                       ← tc netem/ns-3/MahiMahi/toxiproxy/Comcast 完整对标
│
├── (现有文件保持不变)
│   ├── test-strategy-gaps.md
│   ├── expandation/
│   │   ├── 00-summary.md
│   │   ├── 01-protocols.md
│   │   ├── 02-network-env.md
│   │   ├── 03-hardware.md
│   │   ├── 04-software-env.md
│   │   ├── 05-user-interaction.md
│   │   └── 06-security.md
│   └── ... (其他现有研究)
```

### 调研统计数据

| 维度 | 覆盖项 | 新增 Profile 建议 | 新增测试场景建议 | 🔴 发现 |
|------|:-----:|:-------------:|:-------------:|:-----:|
| 蜂窝 3GPP | 10 代 (2G→5G SA) | 6 个 (5g_sa, 5g_urllc, lte_weak, lte_highspeed, irat_4g_to_3g, irat_4g_to_2g) | 4 个 (切换中断/RRC状态) | 12 |
| WiFi 802.11 | 7 代 + 5 种特有损伤 | 5 个 (weak_signal, interference, bss_transition, dfs_switch, powersave) | 3 个 (DFS/BSS/干扰) | 6 |
| 协议行为 | 40+ RFC 章节对照 | — | — | 8 (408/425/429/SRVFAIL/GOAWAY/BOM/0-RTT/连接迁移) |
| OS/硬件 | 5 大类 (Android/iOS/Linux/macOS/Windows) | 5 个 (cg_nat, wifi_cellular_switch, doze_recovery, grey_failure, enterprise_proxy) | 9 个 (S17-S25) | 15 |
| 模拟工具 | 5 工具 × 8 损伤维度 | — | — | 5 (correlation/pareto/4-state/slot/slicer 缺失) |
| **合计** | **62+** | **16** | **16** | **46** |

---

## 八、参考资源索引

### 标准组织

| 组织 | 范围 | 关键文档 |
|------|------|---------|
| [3GPP](https://www.3gpp.org/specifications) | 蜂窝网络 | TS 45/25/36/38 系列 |
| [IEEE 802.11](https://www.ieee802.org/11/) | WiFi | 802.11a/b/g/n/ac/ax/be |
| [ITU-T](https://www.itu.int/itu-t/) | 通信标准 | G.114/G.109/G.984/G.992 |
| [ITU-R](https://www.itu.int/itu-r/) | 无线通信 | S.1711/S.1712 (卫星) |
| [IETF](https://www.ietf.org/standards/rfcs/) | 互联网协议 | RFC 7230-7235/7540/6455/8305/9000 |
| [WHATWG](https://html.spec.whatwg.org/) | 浏览器标准 | HTML Living Standard §9.2 (SSE) |
| [Broadband Forum](https://www.broadband-forum.org/) | 宽带接入 | TR-124/TR-181 |

### 模拟工具

| 工具 | 层级 | 语言 | 关键能力 |
|------|:----:|------|---------|
| [tc netem](https://man7.org/linux/man-pages/man8/tc-netem.8.html) | L2 内核 | C (Linux) | 最全面的损伤模型 |
| [ns-3](https://www.nsnam.org/) | L3 协议栈 | C++/Python | 全协议栈仿真，学术标准 |
| [MahiMahi](http://mahimahi.mit.edu/) | L1 应用 | C++/Shell | HTTP trace 录制与回放 |
| [Comcast](https://github.com/tylertreat/comcast) | L1 应用 | Go | 简化 tc netem 的工具 |
| [toxiproxy](https://github.com/Shopify/toxiproxy) | L1 应用 | Go | TCP 代理 + 故障注入 API |
| [Clumsy](https://github.com/jagt/clumsy) | L2 内核 | C | Windows 网络损伤（WinDivert） |
| [Gremlin](https://www.gremlin.com/) | 云平台 | SaaS | 托管混沌工程 |
| [Chaos Mesh](https://chaos-mesh.org/) | K8s | Go | CNCF 混沌工程平台 |

### 测试框架

| 框架 | 生态 | 范式 |
|------|------|------|
| [fast-check](https://fast-check.dev/) | JS/TS | 属性基测试 (PBT) |
| [proptest](https://docs.rs/proptest/) | Rust | 属性基测试 (PBT) |
| [GoReplay](https://goreplay.org/) | Go | 流量回放 |
| [wrk2](https://github.com/giltene/wrk2) | C/Lua | 恒定速率负载 |
| [httpwg test suite](https://github.com/httpwg/http-test-suite) | Python | HTTP 协议合规 |
| [Web Platform Tests](https://github.com/web-platform-tests/wpt) | JS | 浏览器 API 合规 |
