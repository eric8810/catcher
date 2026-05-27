# Phase 0 发现报告 — 网络韧性调研

> 框架 v3 · 环节 ① 发现
> 调研日期：2026-05
> 方法：Web 搜索 × WebFetch × 子代理并行挖掘
> 数据源：Cloudflare Radar 2024/2025、Cloudflare TCP Reset 研究、Google SRE Book、
>   AWS SDK Retry Behavior、AWS Builders Library、Starlink 实测、ACM SIGCOMM 连接篡改研究、
>   各公司公开 Postmortem、curl/reqwest/hyper/OkHttp 开源 Issue

---

## 一、核心发现：真实世界的网络故障远比想象中普遍

### 1.1 20% 的 TCP 连接在交换任何有用数据之前就终止了

这个数字来自 Cloudflare 的全球网络测量（2024/2025 年度报告），是全球平均值。
连接终止的分布因阶段而异：

| 终止阶段 | 典型比例 | 主要原因 |
|---------|:------:|---------|
| Post-SYN（握手期间） | ~5-10% | 扫描器、IP 欺骗、防火墙丢包 |
| Post-ACK（握手完成后立即） | ~3-5% | **中间件篡改**（防火墙注入 RST） |
| Post-PSH（发送第一个数据包后） | ~2-4% | TLS ClientHello 被拦截、DPI 阻断 |
| Later（发送多个数据包后） | ~3-6% | 浏览器标签关闭、应用层放弃 |

**对 Catcher 的影响**：如果 Catcher 的默认重试策略不考虑"握手后就断开"这个故障模式，那 20% 的连接失败中至少有 5-8% 可以通过**在同一连接上重试无法解决**——必须新建连接。

### 1.2 中间件连接篡改是真实存在的、可测量的

Cloudflare 的 SIGCOMM 2023 论文识别了 **19 种连接篡改签名**，其中 14 种与地面主动测量结果吻合。中间件（企业防火墙、ISP DPI 设备、国家级审查系统）会：
- 注入伪造的 TCP RST 包
- 在 TLS ClientHello 阶段丢弃连接
- 对特定 SNI 域名进行阻断

**对 Catcher 的影响**：这种故障不是"网络不好"，而是"网络故意阻断"。Catcher 的 retry 策略不应在这种场景下反复重试同一 endpoint——应该切换到备用 endpoint 或不同端口/协议。

---

## 二、延迟：从 0.25ms 到 1200ms——七个数量级的动态范围

### 2.1 实测延迟基准

| 场景 | 典型 RTT | 数据来源 |
|------|:------:|---------|
| AWS 同 AZ 内 | 0.25ms | cloudping.io 实测 |
| AWS 同 Region 跨 AZ | 0.5-3.4ms | cloudping.io 实测 |
| 同城光纤 | 1-5ms | 物理约束 (5μs/km) |
| 同国跨城 | 10-50ms | 物理约束 |
| 跨洲（美欧） | 60-120ms | 物理约束 + 路由 |
| Cloudflare 全球 loaded latency p75 | ~78.6ms | Cloudflare Radar 2024 |
| 4G LTE 实测 | 15-50ms | 3GPP TS 36.101 + 实测 |
| Starlink LEO 中位 | 25-50ms | Packetstorm 2026 分析 |
| Starlink LEO p99 | <65ms | Packetstorm 2026 分析 |
| 3G HSPA 实测 | 70-200ms | 3GPP TS 25.101 |
| GEO 卫星最低 | 480-600ms | 物理约束 (35,786km × 2/c) |
| GPRS 实测 | 300-600ms | 测量研究 |

**对 Catcher 的影响**：Catcher 的 `max_backoff_ms` 默认 10,000ms 在 600ms RTT 下只够 3-4 次重试就封顶，而 TCP 层在同等条件下需要数分钟才会放弃。**超时策略必须考虑 RTT 感知。**

---

## 三、移动端的物理约束是硬性的

### 3.1 关键约束

| 平台 | 约束 | Catcher 影响 |
|------|------|-------------|
| Android Doze | 网络挂起，维护窗口 15→30→60→120min 逐步稀疏 | WS/SSE 长连接在 Doze 下物理上不可维持 |
| iOS 后台 | 30s 后台执行限制（iOS 13+） | 所有连接在 30s 后被挂起 |
| CGNAT TCP 空闲 | 60-120s（部分运营商） | keepAlive 必须 < 60s |
| 移动设备流量占比 | 全球 41.3%（Cloudflare 2025） | 移动端不是边缘场景，是主场景 |

### 3.2 Doze + CGNAT + WS 的组合效应（极端场景 E5）

Android Doze 下 WS 心跳停止 → CGNAT 120s 超时后拆除 NAT mapping → 2h 后 Doze 窗口到来 → TCP SYN 到服务器但 NAT mapping 已不存在 → 所有重试在 30s 窗口内耗尽 → 又等 2h。

**Catcher 需要**：平台感知的退避策略、网络恢复探测回调、无限重试模式（不靠固定次数）。

---

## 四、重试策略：从"好心"到"灾难"只有一线之隔

### 4.1 AWS 的教训（真实 Postmortem + SDK 演进）

| 来源 | 发现 |
|------|------|
| AWS us-east-1 2021 故障 | 客户端的 back-off 机制存在潜伏 bug，在生产环境运行多年才首次触发 → 6h 级联故障 |
| AWS SDK 2026 新策略 | 区分**瞬态错误**（连接重置、DNS 失败、5xx）→ **50ms base delay** vs **节流错误**（429）→ **1000ms base delay** |
| AWS 重试配额 | Token bucket 500 tokens，瞬态错误消耗 14 tokens，节流消耗 5 tokens |
| Google SRE | "Retries can amplify low error rates into higher levels of traffic" — 重试是**信号放大器** |
| Google SRE | "Every client that makes an RPC must implement exponential backoff (with jitter)" |
| Google SRE | "Limit retries per request. Consider having a server-wide retry budget." |

### 4.2 关键洞察：不同错误类型需要不同的重试策略

当前 Catcher 对所有 Retryable 错误使用统一的退避策略。但 AWS 的经验表明：

- **瞬态错误**（连接重置、DNS 瞬败、5xx）：应该**快速重试**（10-50ms base），因为这些错误通常瞬间消失
- **节流错误**（429、503 with Retry-After）：应该**慢速重试**（1s+ base），给服务端恢复时间
- **超时错误**：应该**谨慎重试**，因为可能只是服务端慢而非挂了——重试 = 双倍负载

---

## 五、竞品 Bug 模式：9 个 Catcher 必须避免的陷阱

从 curl、reqwest、hyper、OkHttp 的真实 Bug 中提取：

| # | 陷阱 | 后果 | Catcher 对策 |
|---|------|------|-------------|
| 1 | H2 单连接流耗尽 | 超过 SETTINGS_MAX_CONCURRENT_STREAMS 后请求静默失败 | 跟踪活跃 H2 stream 数，超限时开新连接 |
| 2 | 连接池返回已被服务端关闭的 socket | ~0.02% 静默失败率，极难复现 | 复用前 PING/read-zero-timeout 检测 |
| 3 | 重试逻辑遗漏某些瞬态错误码 | ETIMEDOUT/ECONNREFUSED 被当作永久失败 | 显式可审计的 Retryable 错误码白名单 |
| 4 | DNS TTL 过期但连接池保留旧 IP | 持续流量下连接永不轮到空闲超时，永久使用已废弃 IP | 连接最大生存时间 (TTL)，定期重解析 |
| 5 | H2 stream 超时后被误判为连接正常 | 超时后的连接被放回池中，后续请求全部失败 | 超时后 PING 检测；"降级连接"状态 |
| 6 | Handle 复用泄露上次请求的状态 | curl: speed-limit 定时器未初始化导致后续请求立即超时 | 每次请求后完整状态重置 |
| 7 | 超时未覆盖 DNS 解析阶段 | curl: 设置了 20s 超时但 DNS 解析阻塞 486s | 统一 deadline 计时器覆盖所有阶段 |
| 8 | H2 流量控制竞态 | OkHttp: DATA 与 RST_STREAM 并发导致流控死锁 | 原子化 SETTINGS 验证→确认→激活 |
| 9 | 错误类型擦除瞬态/永久区分 | reqwest: 超时错误被埋入 io::Error，用户靠字符串匹配判断 | 一级 `Error::category()` 方法 |

---

## 六、7 种真实生产故障模式 → Catcher 对策

| 故障模式 | 真实案例 | Catcher 客户端对策 |
|----------|---------|-------------------|
| DNS SERVFAIL | Cloudflare 1.1.1.1 2023 (SERVFAIL 3%→15%), Slack 2021 | 多 DNS fallback + SERVFAIL ≠ NXDOMAIN + 负缓存 |
| BGP 路由黑洞 | Meta 2021 (6h), CenturyLink 2020 (4.5h) | 多 endpoint 切换 + 连接超时 < OS 默认 127s |
| Retry Storm | AWS us-east-1 2021 (6h) | 指数退避+jitter + retry budget + CB |
| 瞬时网络分区 | GitHub 2018 (43s 分区→24h 数据修复) | 重连后幂等检查 + 序列号校验 |
| Gray Failure | Stripe 2019 (2.5h 超时) | 请求级超时 + 快速资源释放 |
| CDN 全局 503 | Fastly 2021 (49min) | 5xx 比例监控 + CDN→直连降级 |
| 依赖链级联 | AWS S3 2017 (4h) | 独立超时 + 非阻塞等待 |

---

## 七、极端场景：7 个从第一性原理推导的组合故障

| 场景 | 关键约束叠加 | 最坏后果 | 优先级 |
|------|------------|---------|:----:|
| E1: GEO+CgNAT+TLS 0-RTT+iOS | 600ms RTT + NAT rebinding + 0-RTT rejection + 30s 后台 | 所有重试窗口被 CGNAT timeout 吞噬 | P1 |
| E2: TCP wrap+TIME_WAIT+H2 limit | 端口耗尽 + H2 流耗尽 + TIME_WAIT 4min | 新连接无法建立，已有连接流已满 | P2 |
| E3: Deep RTT + backoff mismatch | >1s RTT + max_backoff=10s + max_attempts=3 | ~22s 后放弃，TCP 层仍需数分钟 | **P0** |
| E4: DNS TTL=0 + transient storm | DNS 不缓存 + 503 + 每次重试重新解析 | DNS 重试风暴放大延迟 | P1 |
| E5: Doze+CGNAT+WS | 2h Doze + 120s NAT timeout + WS 心跳中断 | 恢复时间可达 2h | **P0** |
| E6: H2 HPACK + proxy buffering | HPACK 表污染 + chunked 延迟 + 代理缓冲 | GOAWAY + 连接级故障 | P2 |
| E7: All endpoints fail | BGP 全球中断 + DNS stale 过期 + 所有 CB OPEN | 全局不可达，无退路 | P3 |

---

## 八、Phase 0 结论：对后续调研的指引

### 8.1 推翻的假设

1. **"retry 应该统一使用指数退避"** → 错误。AWS 的经验表明瞬态错误应快速重试，节流错误应慢速重试。Catcher 需要**按错误类别区分退避策略**。
2. **"连接失败是罕见事件"** → 错误。全球 20% 的 TCP 连接在数据交换前就终止了。连接失败是常态，不是异常。
3. **"移动端是边缘场景"** → 错误。41.3% 的流量来自移动设备，且移动端有最极端的物理约束（Doze、iOS 后台、CGNAT）。
4. **"Profile 应该按网络技术分类"** → 需要重新审视。游戏行业按使用场景分类（桌面 vs 移动端），而非按 GPRS/4G/5G。

### 8.2 Phase 1 正交矩阵的输入

Phase 1 需要建立**故障本质 × 网络位置**的正交矩阵。Phase 0 发现为矩阵提供了：

- **故障本质维度**：时间故障 / 完整度故障 / 可达性故障 / 身份故障 / 策略故障（在总纲 §1.2 已定义）
- **关键填充数据**：20% TCP 失败率、连接篡改、Doze 约束、DNS SERVFAIL vs NXDOMAIN、中间件 RST 注入
- **新增交叉格**：策略故障 × L3 中间件（连接篡改）、时间故障 × L5 环境（Doze）、可达性故障 × L2 拓扑（BGP 黑洞）

### 8.3 Phase 2-4 缺口评估

| Phase | 最关键的缺口 |
|-------|------------|
| Phase 2 (溯源) | **实测数据对标**：当前所有 Profile 参数基于标准设计值，缺少 Cloudflare Radar / CrUX / OpenSignal 的统计分布对标 |
| Phase 3 (模拟) | **保真度校验**：proxy.ts 从未与真实网络 trace 做过统计对比，无法回答"模拟和真实差多少" |
| Phase 4 (合规) | **408/425/429 的 RFC 合规**：当前 Catcher 将 408 归为 NonRetryable，缺少 425 Too Early 和 429 Retry-After 处理 |

---

## 九、数据溯源索引

| 数据 | 来源 | URL |
|------|------|-----|
| 20% TCP 连接异常终止 | Cloudflare Radar 2024 | blog.cloudflare.com/radar-2024-year-in-review |
| 连接篡改 19 签名 | ACM SIGCOMM 2023 / Cloudflare | blog.cloudflare.com/connection-tampering |
| TCP 异常连接细分阶段 | Cloudflare Radar TCP Reset | blog.cloudflare.com/tcp-resets-timeouts |
| AWS SDK 重试策略 (50ms/1000ms) | AWS Developer Blog 2026 | aws.amazon.com/blogs/developer/announcing-updated-retry-behavior |
| SRE 级联故障 & 重试预算 | Google SRE Book Ch.21-22 | sre.google/sre-book |
| AWS 超时重试退避 jitter | AWS Builders Library | aws.amazon.com/builders-library/timeouts-retries-and-backoff-with-jitter |
| Starlink 延迟统计 | Packetstorm 2026 | packetstorm.com/starlink-satellite-internet-in-2026 |
| AWS 跨 AZ 延迟 | cloudping.io / xkyle.com | xkyle.com/Measuring-AWS-Region-and-AZ-Latency |
| 竞品 Bug (9 模式) | curl/reqwest/hyper/OkHttp GitHub Issues | 见 BugMiner agent 报告 |
| 生产 Postmortem (7 案例) | Meta/AWS/GitHub/Stripe/Fastly/Cloudflare/Slack | 见 Postmortem agent 报告 |
