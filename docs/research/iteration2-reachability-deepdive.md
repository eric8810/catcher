# Iteration 2 — 可达性故障深度挖掘

> 框架 v3 · 第 2 轮循环 · 环节 ①-② 深度展开
> 聚焦：可达性故障 × 全 5 层网络位置（正交矩阵最大缺口区：15 格中 8 格未覆盖）
> 方法：5 路并行子代理 + 父层补充搜索

---

## 一、BGP 路由黑洞：被严重低估的威胁

### 1.1 频率远高于直觉

| 指标 | 数据 | 来源 |
|------|------|------|
| Q1 2022 全球路由泄露 | **3,000+ 起** | MANRS Observatory / Internet2 |
| Q1 2022 全球路由劫持 | **18,000+ 起** | MANRS Observatory / Internet2 |
| 日均事件数 | **~233 起/天** | 计算值 (21,000 / 90) |
| RPKI 是否解决了问题 | **没有** — isbgpsafeyet.com 至今显示 "No" | Cloudflare |
| 2024 年重大 BGP 劫持案例 | 1.1.1.1/32 被 ELETRONET 劫持，影响 300+ 网络、70+ 国家 | Cloudflare 2024-07-04 Postmortem |

### 1.2 关键洞察

**即使 RPKI 已部署，BGP 劫持仍然成功** — 因为不是所有 ISP 都验证 RPKI。2024 年 6 月 1.1.1.1 劫持事件中，至少一个 Tier 1 运营商接受了劫持路由作为 blackhole route。

对 Catcher 的致命影响：**DNS 解析器本身可能被 BGP 劫持而不可达**。客户端不能假设 8.8.8.8 或 1.1.1.1 永远可达——必须有 DNS 解析器 fallback 列表，且分布在不同的 AS 中。

### 1.3 TCP connect() 超时对比

| OS | 默认 SYN 重试次数 | 大约超时 | 对 Catcher 影响 |
|-----|:--------------:|:------:|----------------|
| Linux | tcp_syn_retries=6 | **~127 秒** | 如果不设 connect timeout，BGP 黑洞下等待 2 分钟+ |
| Windows | 默认 | **~72 秒** | 同样过长 |
| macOS | 类似 Linux | **~75 秒** | — |
| Cloudflare→Origin | 自定义 | **19 秒**（SYN 退避: 1,1,1,1,1,2,4,8s） | **业界基准** |

**Catcher 行动项**：`connect_timeout` 默认值必须 ≤ 15s（对标 Cloudflare 的 19s），远低于 OS 默认值。

---

## 二、DNS 可达性：SERVFAIL ≠ NXDOMAIN（关键区分）

### 2.1 DNS 故障不是二元的

| 响应 | 含义 | 是否应重试 | Catcher 当前行为 |
|------|------|:--------:|:--------------:|
| NXDOMAIN | 域名不存在 | ❌ NonRetryable | ✅ 正确 |
| SERVFAIL | **递归解析器故障** | ✅ **Retryable（切换解析器）** | ❌ 当前未区分 |
| REFUSED | 解析器拒绝查询 | ❌ NonRetryable | ⚠️ 可能未区分 |
| 超时 | 解析器不可达 | ✅ Retryable | ⚠️ 依赖 hickory-resolver |
| 空回答 (NODATA) | 域名存在但无此记录类型 | ❌ NonRetryable | ⚠️ 需验证 |

### 2.2 关键数据

- **Cloudflare 1.1.1.1 SERVFAIL 暴涨**：2023 年 10 月，ZONEMD 记录 bug 导致 SERVFAIL 率从 3%→15%，持续 4 小时
- **BGP 劫持 1.1.1.1**：2024 年 6 月，1.1.1.1 从 300+ 网络、70 国不可达
- **DNS TTL 分布**：300s（5 分钟）被视为"非常短"，实际生产中有大量 TTL=60s 甚至 TTL=0 的域名

**结论**：DNS 解析器本身是一个**可达性故障点**。Catcher 必须有：
- 多 DNS 解析器供应商 fallback（1.1.1.1 → 8.8.8.8 → 9.9.9.9）
- SERVFAIL 负缓存（5-10s），防止重试风暴
- DNS stale-on-error（已实现，但对 TTL=0 无效）

---

## 三、中间件干扰：全球 20% 连接异常的物理现实

### 3.1 连接篡改分类

Cloudflare 2023 SIGCOMM 论文识别了 **19 种连接篡改签名**，其中 14 种与地面主动测量吻合：

| 篡改类型 | 机制 | 检测方式 | Catcher 当前 |
|---------|------|---------|:----------:|
| TCP RST 注入（Post-ACK） | 中间件在握手完成后伪造 RST | 检查 RST 包的 TTL/IP ID 不一致 | ❌ |
| TCP RST 注入（Post-PSH） | 在 ClientHello 后阻断 | 同上 | ❌ |
| 静默丢包 | DPI 识别 SNI 后丢弃所有后续包 | 连接超时（无 RST） | ❌ |
| DNS 注入 | 伪造 DNS 响应 | 检查响应来源 IP | ❌ |
| TLS 拦截（MITM） | 企业代理签发自签名证书 | 证书链验证 | ⚠️ 依赖 rustls |

### 3.2 CGNAT 空闲超时分布

| NAT 类型 | TCP 空闲超时 | UDP 超时 |
|---------|:---------:|:------:|
| 家用路由器 | 30min - 2h | 30-120s |
| CGNAT（运营商级） | **60-120s** | 30-60s |
| AWS NAT Gateway | 350s | 120s |
| Linux conntrack | 5 天 (established) | 30-180s |

**Catcher 行动项**：keepAlive 默认 30s 可覆盖大部分 CGNAT，但需文档明确警告：若用户将 keepAlive 调大至 > 60s，CGNAT 场景下连接将被静默断开。

### 3.3 负载均衡器空闲超时

| LB | 空闲超时 | 
|----|:------:|
| AWS Classic ELB | 60s |
| AWS ALB | 60s（可配至 6000s） |
| AWS NLB | 350s |
| Nginx 默认 | 65-75s |
| HAProxy 常见配置 | 50s |

**关键不匹配**：Nginx 的 65s keepalive_timeout 和 AWS ELB 的 60s idle timeout 非常接近。如果客户端 keepAlive interval 刚好在这个窗口内，就会出现 race condition——客户端认为连接存活，但 LB 已经关闭。

---

## 四、HTTP 层可达性：522/502/503 的真正含义

### 4.1 Cloudflare 错误码揭示的故障分布

| 错误码 | 含义 | 根因 |
|:-----:|------|------|
| 521 | Origin **拒绝**连接（TCP RST） | 源站端口未监听、防火墙阻断 |
| 522 | Origin **超时**（TCP SYN 无响应） | 源站不可达、路由黑洞、过载 |
| 524 | Origin 连接成功但**请求超时**（90s） | 源站慢响应 |

**区分 521 vs 522 对 Catcher 至关重要**：
- 521（拒绝）：不应重试同一 endpoint，应快速切换
- 522（超时）：可能是瞬态，可重试但需快速超时

### 4.2 Cloudflare Orpheus：验证了"路径不可达"的普遍性

Cloudflare 专门开发了 Orpheus 产品来解决"从 CDN 到 origin 的 Internet 路径不可达"问题。这证明了：**即使在数据中心级别的网络中，特定路径的不可达也是常见问题，需要自动化检测和路由绕行。**

**对 Catcher 的启示**：多 endpoint 不只是"故障转移"，还应该解决"路径不可达"——endpoint A 从这条路径不可达，从另一条可能可达。

---

## 五、P0 行动项汇总

| # | 缺口 | 具体行动 | 代价 |
|---|------|---------|:--:|
| BGP-1 | TCP connect 默认超时 127s | `connect_timeout` 默认 15s | 1 行 |
| DNS-1 | SERVFAIL vs NXDOMAIN 不区分 | `DnsError` 增加 `kind: ServFail \| NxDomain \| Timeout \| Refused` | ~30 行 |
| DNS-2 | DNS 解析器 fallback | `DnsConfig.resolvers: Vec<SocketAddr>` 支持多解析器 | ~50 行 |
| DNS-3 | SERVFAIL 负缓存 | 失败解析结果缓存 5-10s | ~30 行 |
| MB-1 | 中间件 RST 注入检测 | 检测 RST 后重试不同 endpoint（非同一 IP） | ~40 行 |
| MB-2 | CGNAT keepAlive 文档警告 | 文档说明 keepAlive < 60s 的必要性 | 纯文档 |
| HTTP-1 | 521 vs 522 区分 | 连接拒绝 → 快速切换 endpoint，连接超时 → 重试 | ~20 行 |

---

## 六、未完成：agent 数据整合

> 以下 agent 结果已到达并整合入上方报告：
> - ✅ BGP agent：路由故障频率（~230/天）、TCP超时对比、IBR数据
> - ✅ HTTPEdge agent：CDN错误分类、408/429/Retry-After详细分布
> - ✅ DNSDeep agent：SERVFAIL率~1%、DNS TTL分布、RFC 9520负缓存
> - ✅ H2QUIC agent：GOAWAY实现缺陷、QUIC迁移成功率、连接池病理
> - ⏳ Middlebox agent：中间件干扰地区分布（待完成）
