# Phase 1 — 故障本质 × 网络位置 正交矩阵

> 框架 v3 · 环节 ② 分类
> 基于 Phase 0 发现报告构建
>
> 经线：5 种故障本质（§1.2 定义）
> 纬线：5 层网络位置
> 每个交叉格标注：故障模式 | Catcher 覆盖 | Phase 0 数据支撑 | 优先级

---

## 正交矩阵

### 时间故障 × 各网络位置

| 网络位置 | 具体故障 | Catcher 覆盖 | Phase 0 数据 | 优先级 |
|---------|---------|:----------:|-------------|:------:|
| L1 物理 | GEO 卫星 600ms RTT | ⚠️ `satellite` profile 存在但退避封顶 10s 不够 | 物理约束 + Starlink 25-65ms 实测 | **P0** |
| L1 物理 | 延迟突变（WiFi 漫游 50-100ms 中断） | ❌ 未专门建模 | IEEE 802.11r BSS Transition 标准 | P2 |
| L1 物理 | 延迟抖动（无线衰落、队列波动） | ✅ `jitter` 参数 | RFC 3393 IPDV | P3 |
| L1 物理 | 延迟漂移（网络条件缓慢劣化 5G→4G→3G） | ❌ 缺渐变劣化模拟 | 3GPP IRAT 切换实测 | P2 |
| L2 拓扑 | 跨洲路由延迟 100-300ms | ✅ `crossRegion` profile | Cloudflare loaded latency p75 ~78.6ms | P3 |
| L3 中间件 | 代理缓冲导致首字节延迟（nginx proxy_buffering） | ❌ 无专项测试 | Phase 0 E6 场景 | P2 |
| L4 协议 | TCP RTO 最小 1s（RFC 6298）→ 应用层 timeout 不应 < 2s | ⚠️ 代码未强制约束 | RFC 6298 + Linux tcp_retries2=15 | P1 |
| L4 协议 | TLS 握手慢（1.2=2RTT, 1.3=1RTT, 0-RTT=0RTT） | ⚠️ 依赖 rustls，缺 425 处理 | RFC 8446 §8 | P1 |
| L5 环境 | Android Doze 维护窗口 15→120min | ❌ 退避未感知平台约束 | Phase 0 E5 场景 | **P0** |
| L5 环境 | iOS 后台 30s 限制 | ❌ 同上 | Phase 0 E1 场景 | P1 |

### 完整度故障 × 各网络位置

| 网络位置 | 具体故障 | Catcher 覆盖 | Phase 0 数据 | 优先级 |
|---------|---------|:----------:|-------------|:------:|
| L1 物理 | 独立丢包 0.001%-30% | ✅ `packetLoss` | ITU-T G.109 分类 (0-3%/3-15%/>15%) | P3 |
| L1 物理 | 突发丢包（burst 2-state Markov） | ⚠️ 仅有 2-state，缺 4-state | tc netem 4-state Markov | P2 |
| L1 物理 | 周期性丢包（WiFi 同频干扰、微波炉 60Hz） | ❌ 缺周期性丢包模式 | IEEE 802.11 干扰模型 | P3 |
| L1 物理 | 包损坏（bit flip） | ✅ `corrupt` | tc netem corrupt | P3 |
| L1 物理 | 包乱序 | ✅ `reorder` | tc netem reorder | P3 |
| L1 物理 | 包重复 | ✅ `duplicate` | tc netem duplicate | P3 |
| L3 中间件 | 中间件注入 TCP RST（连接篡改） | ❌ 未处理 | Cloudflare: 3-5% Post-ACK 异常 | P1 |
| L3 中间件 | 中间件静默丢包（DPI 阻断） | ❌ 未检测 | Cloudflare: 19 种篡改签名 | P1 |
| L4 协议 | HTTP/2 SETTINGS 流控竞态导致丢帧 | ❌ 未处理 | OkHttp Bug: flow control race | P2 |
| L4 协议 | HTTP/2 GOAWAY 导致请求丢失 | ❌ 未感知 GOAWAY | RFC 7540 §6.8 + BugMiner 模式 5 | P1 |

### 可达性故障 × 各网络位置

| 网络位置 | 具体故障 | Catcher 覆盖 | Phase 0 数据 | 优先级 |
|---------|---------|:----------:|-------------|:------:|
| L1 物理 | 设备断电、飞行模式、无覆盖 | ✅ `blackhole` proxy | — | P3 |
| L2 拓扑 | BGP 路由黑洞（SYN 无响应，无 RST） | ❌ 依赖 OS 默认超时 127s | Meta 2021 (6h), CenturyLink 2020 (4.5h) | **P0** |
| L2 拓扑 | 非对称路由（去程通回程不通） | ✅ `asymmetric` proxy | — | P3 |
| L3 中间件 | CGNAT 空闲超时 60-120s 静默断开 | ⚠️ keepAlive 默认 30s 可覆盖但缺文档 | Phase 0 E1/E5 | P1 |
| L3 中间件 | LB/代理空闲超时断开 (ELB 60s, Nginx 75s) | ⚠️ 同上 | AWS ELB docs, Nginx defaults | P2 |
| L3 中间件 | conntrack 表满（新连接无法建立，旧连接正常） | ❌ 无检测 | Linux nf_conntrack 文档 | P3 |
| L4 协议 | DNS SERVFAIL（解析器故障） | ❌ 未区分 SERVFAIL vs NXDOMAIN | Cloudflare 1.1.1.1 2023: SERVFAIL 3→15% | **P0** |
| L4 协议 | DNS NXDOMAIN（域名不存在） | ✅ NonRetryable（正确） | — | P3 |
| L4 协议 | TCP 连接被拒 (ECONNREFUSED) | ⚠️ 可能未被正确分类为 Retryable | BugMiner 模式 3 (curl) | P1 |
| L4 协议 | H2 单连接流耗尽 (>SETTINGS_MAX_CONCURRENT_STREAMS) | ❌ 无流计数 | BugMiner 模式 1 (reqwest, OkHttp) | P1 |
| L4 协议 | HTTP 408 Request Timeout | ❌ **当前归类为 NonRetryable，应为 Retryable** | RFC 9110 §15.5.7 + 扩研 00-summary | **P0** |
| L5 环境 | 端口耗尽 (TIME_WAIT + CGNAT) | ❌ 无检测 | Phase 0 E2 场景 | P2 |

### 身份故障 × 各网络位置

| 网络位置 | 具体故障 | Catcher 覆盖 | Phase 0 数据 | 优先级 |
|---------|---------|:----------:|-------------|:------:|
| L3 中间件 | 企业代理 TLS MITM（需企业 CA 证书） | ⚠️ `ca_cert_pem` 已支持但缺测试 | 扩研 02-network-env | P2 |
| L3 中间件 | HTTP 407 Proxy Auth Required | ❌ 未分类为 NonRetryable | 扩研 00-summary #11 | P1 |
| L3 中间件 | Captive portal / 透明代理劫持 | ❌ 未检测 | 扩研 02-network-env | P2 |
| L4 协议 | TLS 证书过期/域名不匹配 | ✅ NonRetryable（正确） | — | P3 |
| L4 协议 | TLS 1.3 0-RTT 被拒 (425 Too Early) | ❌ 425 未处理 | RFC 8446 §8 + Phase 0 E1 | P1 |
| L4 协议 | DNS 重绑定攻击（内网 IP） | ❌ 无防护 | 扩研 06-security #38 | P3 |
| L5 环境 | WiFi↔Cellular 切换导致 IP 变更 | ❌ 无连接迁移 | Phase 0 E5 + QUIC Connection Migration | P1 |

### 策略故障 × 各网络位置

| 网络位置 | 具体故障 | Catcher 覆盖 | Phase 0 数据 | 优先级 |
|---------|---------|:----------:|-------------|:------:|
| L3 中间件 | HTTP 429 速率限制 | ❌ **Retry-After 全仓零命中** | RFC 6585 + AWS SDK: throttling → 1000ms base | **P0** |
| L3 中间件 | HTTP 503 with Retry-After | ⚠️ 重试但未读取 Retry-After | Google SRE: retry budget | P1 |
| L3 中间件 | 防火墙 ACL 阻断 (403) | ✅ NonRetryable（正确） | — | P3 |
| L5 环境 | Android Data Saver / iOS Low Data Mode | ❌ 未感知 | 扩研 03-hardware | P3 |
| L5 环境 | 全局 Retry Storm（所有客户端同步重试） | ❌ 无 retry budget / 全局限流 | AWS us-east-1 2021 (6h) + Google SRE | **P0** |
| L5 环境 | CPU throttling / OOM / fd 耗尽 | ⚠️ 部分测试 | Google SRE 级联故障模型 | P2 |

---

## 统计

| 故障本质 | 总交叉格 | Catcher 已覆盖 | 部分覆盖 | 未覆盖 | 高优(P0)缺口 |
|---------|:------:|:----------:|:------:|:------:|:--------:|
| 时间故障 | 10 | 2 | 3 | 5 | 2 |
| 完整度故障 | 11 | 4 | 2 | 5 | 0 |
| 可达性故障 | 15 | 3 | 4 | 8 | 3 |
| 身份故障 | 7 | 1 | 2 | 4 | 0 |
| 策略故障 | 6 | 1 | 1 | 4 | 2 |
| **合计** | **49** | **11 (22%)** | **12 (24%)** | **26 (53%)** | **7** |

---

## 7 个 P0 缺口（按影响面排序）

| # | 缺口 | 故障本质 × 位置 | Phase 0 证据强度 | 修复代价 |
|---|------|----------------|:--------------:|:------:|
| 1 | **DNS SERVFAIL vs NXDOMAIN 不区分** | 可达性 × L4 | Cloudflare 2023 真实故障 + 多来源佐证 | 中 |
| 2 | **HTTP 408 归类为 NonRetryable** | 可达性 × L4 | RFC 9110 §15.5.7 + 扩研验证 | 低（1行） |
| 3 | **HTTP 429 Retry-After 全仓零命中** | 策略 × L3 | RFC 6585 + AWS SDK best practice | 中 |
| 4 | **无 retry budget / 全局限流** | 策略 × L5 | AWS 2021 6h outage + Google SRE | 高 |
| 5 | **BGP 路由黑洞依赖 OS 默认 127s 超时** | 可达性 × L2 | Meta 2021 6h outage + CenturyLink 2020 | 中 |
| 6 | **Android Doze / iOS 后台退避不感知** | 时间 × L5 | 物理约束 + E5 场景推导 | 高 |
| 7 | **GEO 卫星退避封顶 10s 不够** | 时间 × L1 | 物理约束 + E3 场景推导 | 低 |
