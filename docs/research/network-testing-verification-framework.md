# 网络条件标准、模拟方案与测试机制 — 完整调研框架

> 版本：v3 — 方法论重构（7 轮迭代闭环完成）
> 创建日期：2025-07 | 重构日期：2026-05 | 最终闭环：2026-05
> 
> v3 重构要点：循环模型、故障本质分类、边界定义、Phase 0 发现机制
>
> 7 轮迭代产出：
> - 49 项故障模式（Q0-Q2 全部可追溯，Q3 6/7 有方案，Q4 待代码落地）
> - 7 个 P0 缺口全部完成标准溯源 + 实测数据对标
> - 4 个被推翻的设计假设
> - 竞品 9 Bug 模式 + 7 Postmortem 案例 + 7 极端场景推导

---

## 〇、核心逻辑链

### 0.1 前置条件：定义物理边界

在探索"网络世界有什么"之前，必须先回答一个更根本的问题：

> **Catcher 作为应用层韧性库，物理上能感知什么、能改变什么？**

只有在这个边界内的条件才是调研对象。边界外的条件虽然真实存在，但 Catcher 无法干预——调研它们只能帮助理解"为什么做不到"，不能转化为产品能力。

这个边界决定了整个调研的范围和有效性。没有边界定义的调研会无休止地膨胀，最终失去聚焦。

### 0.2 调研循环（非线性的、持续的）

```
                    ┌────────────────────────────────────────────┐
                    │     ←── 验证暴露新盲区，触发新一轮发现 ──←    │
                    ▼                                            │
  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐
  │ ①发现    │  │ ②分类    │  │ ③溯源    │  │ ④模拟    │  │ ⑤验证    │
  │          │  │          │  │          │  │          │  │          │
  │ 未知的   │→│ 故障本质  │→│ 标准+实测 │→│ 可复现的  │→│ 可证明的  │
  │ 未知     │  │ 正交矩阵  │  │ 数据交叉  │  │ 损伤注入  │  │ 效果度量  │
  └──────────┘  └──────────┘  └──────────┘  └──────────┘  └──────────┘
       ↑                                                             │
       │         ┌──────────────────────────────────────────┐        │
       └─────────│  ←── 验证结果推翻假设，修正分类体系 ──←  │←───────┘
                 └──────────────────────────────────────────┘
```

五个环节，各自有独立的**核心问题**和**方法论**：

| 环节 | 核心问题 | 方法论 | 当前状态 |
|------|---------|--------|---------|
| **①发现** | 我们不知道什么？ | 跨行业方法论深层提取、postmortem 挖掘、竞品 bug 数据库分析、真实测量数据收集 | 仅 1 次探索性调研，无系统化机制 |
| **②分类** | 如何组织已知的故障？ | 按故障本质（时间/完整度/可达性/身份/策略）为经，网络位置为纬，建立正交矩阵 | 当前按 OSI 层次分类，需翻转为故障优先 |
| **③溯源** | 参数凭什么取这个值？ | 标准文档 + 真实测量数据的交叉验证，区分"设计目标"与"统计现实" | 有标准溯源，缺实测数据对标 |
| **④模拟** | 如何在实验室可信复现？ | 工具功能对标 + 保真度校验（与真实链路的统计对比） | 有功能列表对标，缺保真度校验 |
| **⑤验证** | 如何证明 Catcher 有效？ | 不变量验证 + 统计假设检验 + 因果模型预测 vs 实测 | 有基础测试，缺属性验证和因果模型 |

### 0.3 关键区别

这不是一次性的线性流程，而是一个**持续循环**。每轮验证都会暴露新的盲区，触发下一轮发现——分类随之修正，溯源随之更新。当前框架最大的问题是把它当成了线性流程：先分类、再溯源、再模拟、再验证。但正确的顺序是：**先发现（看看我们不知道什么），再分类（组织已知），然后溯源→模拟→验证，验证结果回到发现**。

---

## 一、条件分类学（Condition Taxonomy）

### 1.1 前置：定义 Catcher 的物理边界

在分类网络条件之前，必须先回答：**Catcher 能感知什么、能改变什么？**

```
Catcher 的管辖范围：应用层能感知、能介入、能改变结果的

  能做 ✅：
    - 应用层超时检测与重试（TCP 连接建立之后的一切）
    - 连接断开后重连（包括 IP 地址变更后的重建）
    - 多端点竞速 / 故障转移
    - Circuit breaker 熔断保护
    - 退避策略控制（间隔、抖动、上限）

  不能做 ❌：
    - 改善 TCP 拥塞控制行为（内核态，Catcher 不可达）
    - 加快 TCP 重传检测速度（同上）
    - 修复 OS 网络栈 bug（超出范围）
    - 绕过运营商限速 / 封锁（不可抗力）
    - 在 iOS 后台维持长连接（物理约束）

  模糊地带 ⚠️（需要逐个决策）：
    - DNS 解析失败 → 应接管（已有）
    - TLS 握手失败 → 应分类并选择性重试
    - 代理认证失败 (407) → 应正确分类但不重试
    - 省电模式导致断连 → 应快速检测并重连，但不保证不断
```

**调研的边界规则**：一个条件是否纳入调研范围，取决于它是否落在"能做 ✅"或"模糊地带 ⚠️"内。落在"不能做 ❌"内的，不纳入调研——调研它们不能转化为产品能力。

### 1.2 第一性分类：故障的本质维度（主分类轴）

从"一个数据包从 A 到 B 到底有几种失败方式"出发，而非从"网络有几层"出发：

```
故障的根本维度（正交于任何网络层次）：

1. 时间故障（Timing）
   ├── 绝对延迟过高（GEO 卫星 600ms RTT）
   ├── 延迟突变（WiFi 漫游中断 50-100ms）
   ├── 延迟抖动（队列波动、无线衰落）
   └── 延迟漂移（网络条件缓慢劣化）

2. 完整度故障（Integrity）
   ├── 丢失 — 独立丢包 / 突发丢包 / 周期性丢包
   ├── 损坏 — bit flip、校验和错误
   ├── 重复 — 中间设备重放
   └── 乱序 — 多路径传输、队列重排

3. 可达性故障（Reachability）
   ├── 永远不可达 — DNS NXDOMAIN、IP 不可路由、端口未监听
   ├── 暂时不可达 — 路由黑洞、conntrack 表满、中间设备静默丢弃
   ├── 部分可达 — 非对称路由（去程通回程不通）、gray failure
   └── 可达但不可用 — TCP 握手成功但应用层无响应

4. 身份故障（Identity）
   ├── 端点身份变更 — WiFi↔Cellular 切换 IP 变、DHCP 续租
   ├── 凭证失效 — TLS 证书过期、Token 过期、Session 超时
   └── 中间人介入 — 代理劫持、TLS MITM、 captive portal

5. 策略故障（Policy）
   ├── 速率限制 — HTTP 429、代理限流、运营商 QoS
   ├── 访问控制 — HTTP 403/407、防火墙 ACL
   ├── 资源配额 — 连接数上限、fd 耗尽、内存不足
   └── 省电策略 — Android Doze、iOS 后台挂起、Data Saver
```

### 1.3 交叉定位：故障模式 × 网络位置（正交矩阵）

以上五种故障模式是**经线**。网络位置是**纬线**（即原有 L1-L5 的层次结构，作为辅助定位）。调研产出应是一个**正交矩阵**而非一棵树：

```
              时间故障    完整度故障   可达性故障   身份故障    策略故障
  L1 物理       ✓           ✓           ✓          —          —
  L2 拓扑       ✓           —           ✓          —          —
  L3 中间件     ✓           ✓           ✓          ✓          ✓
  L4 协议       ✓           ✓           ✓          ✓          ✓
  L5 运行环境   ✓           —           ✓          ✓          ✓
```

每个交叉格定义：该故障在该网络位置的典型表现、Catcher 能做什么、哪些有物理限制无法干预。

### 1.4 各网络位置的关键退化模式（辅助参考）

以下按网络位置（L1-L5）列出典型退化——**不作为主分类，仅作为交叉矩阵中"网络位置"轴的展开**：

| 网络位置 | 典型退化 | 故障本质 | 对 Catcher 的影响 |
|---------|---------|---------|------------------|
| L1 物理 | 带宽 50bps→20Gbps、延迟 0.1ms→1200ms、丢包 0.001%→30% | 时间 + 完整度 | Catcher 只能在上层应对，无法改善物理层 |
| L2 拓扑 | 路由黑洞（无 RST hang 到超时）、非对称路由、间歇连通 | 可达性 | CB 状态机压力；需区分"短暂中断"和"永久断开" |
| L3 中间件 | NAT/CGNAT 空闲超时、代理缓冲 chunked、LB 主动断开 | 可达性 + 策略 | keepAlive 策略；SSE 企业兼容性 |
| L4 协议 | HTTP/2 GOAWAY、TLS 降级、DNS SERVFAIL、keepAlive race | 身份 + 策略 | 错误分类和重试策略 |
| L5 环境 | Android Doze、iOS 30s 后台、conntrack 满、CPU throttling | 策略 + 可达性 | 重连后状态恢复；不继承休眠期间的退避状态 |

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

> 路线图对应核心逻辑链的五个环节。Phase 0 是当前最大短板——没有系统化的"发现未知"机制，
> 后续 Phase 1-4 都只在填补已知维度内的缺口。

### Phase 0：发现未知（优先级 🔴 最高 — 方法论建设）

**目标**：建立系统化的"发现我们不知道的事"的机制，而非一次性探索。

| 任务 | 方法 | 产出 |
|------|------|------|
| **跨行业深层方法论提取** | 不只是对比"做了什么"，而是提取背后的**理论**——为什么游戏行业 2 个预设就够了？SRE 错误预算的数学基础是什么？ | 已产出 `exploratory/industry-methodology-survey.md`，需深化为理论提取 |
| **Postmortem 系统挖掘** | 搜索 GitHub Issues / Hacker News / 生产事故报告中与"网络导致的失败"相关的案例；分类、统计频率、提取模式 | Postmortem 模式库 |
| **竞品 Bug 数据库分析** | curl / chromium / OkHttp / reqwest 的 issue tracker 中标记为 network/timeout/retry/race 的 closed issues | 常见陷阱清单 |
| **真实测量数据收集** | Cloudflare Radar / Google CrUX / OpenSignal 等平台的全球 RTT、丢包、带宽分布 | 实测数据对标基准 |
| **物理约束推导** | 从第一性原理推导极限场景：WiFi + 蓝牙 + 微波炉干扰叠加的最坏情况？GEO 卫星 + CGNAT + iOS 后台的累积效应？ | 边界测试场景 |

**已有的探索成果**（见 `exploratory/industry-methodology-survey.md`）：

| 方向 | 关键发现 | 对 Catcher 的影响 |
|------|---------|-----------------|
| 🎮 游戏引擎 (Unreal/Unity/Godot) | 引擎内置 Network Emulation，2-3 个预设够用，按平台分类 | 双分类法 (技术+场景)，预设精简 |
| 🔥 混沌工程 (Netflix/Shopify) | 40.9% 实验是网络故障，应用层注入仅 3% | Catcher 填补的就是这 3% |
| 📐 Google SRE | 测试分级 + Zero MTTR bugs + 错误预算理论 | SLO 定义方法论 |
| 📡 电信设备 (Keysight/Spirent) | 确定性损伤 + RFC 2544 全参数 | proxy.ts 对标基准 |

**颠覆性发现**：
- Presets 应该少而精（游戏行业 2 个预设就够了）—— 挑战 Catcher 的 14 个 Profile 设计
- 按使用场景分类比按技术分类更重要 —— 挑战按 3GPP/WiFi 分类的根本假设
- 游戏行业不测带宽 —— 某些参数对特定场景是噪音
- 不同行业的"韧性"定义互相冲突 —— 通用 Profile 体系需要按应用类型分化

### Phase 1：边界定义与故障分类（优先级 🔴）

**目标**：明确 Catcher 负责什么、不负责什么，按故障本质（非网络层次）建立正交分类矩阵。

| 任务 | 产出 |
|------|------|
| **Catcher 物理边界文档化**：列出所有"能做 / 不能做 / 模糊地带"的决策 | 边界文档（见 §1.1 前置） |
| **故障模式 × 网络位置正交矩阵**：5 种故障本质 × 5 层网络位置 = 25 个交叉格 | 正交矩阵表格 |

### Phase 2：标准溯源 + 实测数据交叉验证（优先级 🔴）

**目标**：每个参数既有标准来源，又有实测数据验证——区分"设计目标"和"统计现实"。

| 任务 | 产出 | 已产出 |
|------|------|:------:|
| 蜂窝 3GPP 标准溯源 | `standards/cellular-3gpp.md` | ✅ |
| WiFi IEEE 802.11 标准溯源 | `standards/wifi-ieee80211.md` | ✅ |
| 协议行为 RFC 对照 | `standards/protocol-behaviors.md` | ✅ |
| OS/硬件陷阱 | `standards/os-hardware-quirks.md` | ✅ |
| 卫星 ITU-R 标准 | `standards/satellite-itu.md` | ❌ |
| 有线接入 ITU 标准 | `standards/wired-itu-ieee.md` | ❌ |
| **实测数据对标**：Cloudflare Radar / CrUX / OpenSignal 数据 vs Profile 参数 | 实测对标表 | ❌ |

### Phase 3：模拟对标 + 保真度校验（优先级 🟡）

**目标**：不仅知道 proxy.ts 缺什么功能，还要知道现有功能的模拟与真实有多大偏差。

| 任务 | 产出 | 已产出 |
|------|------|:------:|
| tc netem / ns-3 / MahiMahi / toxiproxy / Comcast 对标 | `simulation/tools-benchmark.md` | ✅ |
| **proxy.ts 保真度校验**：proxy.ts 模拟 5% 丢包时，TCP 行为与真实 5% 丢包链路的统计偏差 | 保真度校验报告 | ❌ |

### Phase 4：协议合规矩阵（优先级 🟡）

**目标**：每个 RFC 中"客户端应该做什么"与 Catcher 行为的对照。

| 任务 | 产出 | 已产出 |
|------|------|:------:|
| HTTP/1.1 合规 (RFC 9110) | `compliance/http11-rfc.md` | ❌ |
| HTTP/2 合规 (RFC 9113) | `compliance/http2-rfc.md` | ❌ |
| WebSocket 合规 (RFC 6455) | `compliance/ws-rfc.md` | ❌ |
| SSE 合规 (WHATWG HTML §9.2) | `compliance/sse-whatwg.md` | ❌ |

### Phase 5：验证机制补齐（优先级 🟢）

**目标**：引入不变量验证、统计假设检验、因果模型预测等新范式。

| 任务 | 产出 |
|------|------|
| PBT 原型 — 用 fast-check / proptest 验证 retry/CB 的不变量 | PoC 代码 |
| SLO 定义 — Catcher 库的成功率/延迟/CB 恢复时间 SLO | SLO 文档 |
| 因果模型 — 给定故障模式和 Catcher 配置，预测成功率期望值 | 因果模型 + 实测验证

---

## 六、最终目标：循环追溯链

> 调研不是一次性的——每轮验证都会暴露新盲区，触发新一轮追溯。

catcher 的每个测试决策都应该能回答五个问题（其中 Q0 是前置过滤，Q1-Q4 形成闭环）：

```
Q0: 这个条件在 Catcher 的物理边界内吗？
A0: 是 → 继续 Q1。否 → 不纳入调研（调研不能转化为产品能力）。

Q1: 为什么要测试这个条件？
A1: 因为它对应一种故障本质（时间/完整度/可达性/身份/策略）× 一个网络位置，
    有标准定义 + 实测数据证明其真实发生频率。

Q2: 这个模拟参数凭什么？
A2: 参数来自标准文档（设计值）与实测数据（统计值）的交叉验证，
    并与 tc netem / ns-3 等工具的对应损伤模型对标。

Q3: 怎么证明模拟是有效的？
A3: 对比真实网络 trace 与 proxy.ts 模拟输出，在关键统计量上偏差 < 可接受阈值。

Q4: 怎么验证 Catcher 在这个条件下表现得当？
A4: 不变量验证（PBT）+ 统计假设检验（harness 对比）+ 因果模型预测 vs 实测。

    └→ 验证结果反馈回 Q1：是否有新的故障模式暴露？分类是否需要修正？
```

当前能完整回答 Q0-Q4 的测试场景：**0 个**。这是差距，也是方向。

---

## 七、已产出文件

```
docs/research/
├── network-testing-verification-framework.md    ← 调研框架总纲 v3
├── phase0-discovery-report.md                   ← 🆕 环节① 发现 — 真实测量/Postmortem/Bug/方法论/极端场景
├── phase1-orthogonal-matrix.md                  ← 🆕 环节② 分类 — 5×5 正交矩阵 (49 项故障)
├── phase-final-synthesis.md                     ← 🆕 最终综合报告 — 全五环节闭环总结
│
├── exploratory/                                  ← 探索性调研
│   └── industry-methodology-survey.md            ← 跨行业方法论对比
│
├── standards/                                    ← 标准溯源
│   ├── cellular-3gpp.md                          ← 蜂窝 2G→5G
│   ├── wifi-ieee80211.md                         ← WiFi 损伤建模
│   ├── protocol-behaviors.md                     ← TCP/TLS/DNS/HTTP/WS/SSE/QUIC RFC 对照
│   └── os-hardware-quirks.md                     ← OS/移动端/硬件陷阱
│
├── simulation/                                   ← 模拟工具对标
│   └── tools-benchmark.md                        ← tc netem/ns-3/MahiMahi/toxiproxy/Comcast
│
└── expandation/                                  ← 早期扩研（6 维度 × 42+ 细分领域）
    ├── 00-summary.md
    ├── 01-protocols.md
    ├── 02-network-env.md
    ├── 03-hardware.md
    ├── 04-software-env.md
    ├── 05-user-interaction.md
    ├── 06-security.md
    └── handoff.md
```

> 具体统计数据（覆盖项、Profile 建议、测试场景建议、发现数）见各子文档，不在此框架总纲中重复。

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
