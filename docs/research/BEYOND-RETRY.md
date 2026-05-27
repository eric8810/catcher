# 超越请求库 — 网络韧性的全局解法

> 框架 v3 · 扩展方向
> 日期：2026-05
>
> Catcher 解决的是"重试做得好不好"。但网络问题不止可以用重试来解决。
> 本报告探索请求库之外的策略空间。

---

## 一、DNS 层面：用 HTTP 绕过运营商 DNS

### 1.1 HTTPDNS 原理

传统 DNS：`App → ISP Local DNS (UDP 53) → 递归解析`

HTTPDNS：`App → HTTPS → 专用 HTTPDNS 服务器 → 权威DNS`

核心区别：绕过 ISP 的 Local DNS，直接通过 HTTPS 请求专用服务器。

### 1.2 解决的问题

| 问题 | 传统DNS | HTTPDNS |
|------|---------|---------|
| DNS 劫持（ISP 返回错误 IP） | 常见 | 防止（HTTPS + 直连） |
| DNS 污染（GFW 伪造响应） | 常见 | 防止 |
| 调度不准确（ISP DNS 返回远端节点 IP） | 常见 | 精确（服务端看到真实客户端 IP） |
| TTL 被 ISP 修改 | 常见 | 不受影响 |
| SERVFAIL 率 | ~1% | <0.01%（99.99% SLA） |

### 1.3 实测效果（Tencent HTTPDNS）

- 覆盖 **4 亿+** 用户
- 域名劫持导致的访问失败 **减少 60%+**
- 平均访问延迟 **降低 22%**
- 服务可用性 **99.99%**

### 1.4 DoH vs HTTPDNS

| 维度 | DoH (DNS-over-HTTPS) | HTTPDNS |
|------|---------------------|---------|
| 协议 | 标准 DNS wire format over HTTPS | 专用 HTTP API |
| 解析器 | 公共递归解析器（Cloudflare/Google） | 服务商自建 |
| 智能调度 | ❌ 递归解析器不感知业务 | ✅ 直接获取用户真实 IP，精确调度 |
| 加速权威 DNS | ❌ | ✅（如 DNSPod 联动，秒级生效） |
| 部署 | 浏览器/OS 级别 | App SDK 集成 |
| 中国以外使用 | DoH 为主 | 中国特有，海外用 DoH |

### 1.5 Catcher 可集成的策略

```
优先级:
  1. HTTPDNS (如果有 SDK) → 最快、最准确
  2. DoH (Cloudflare 1.1.1.1 / Google 8.8.8.8) → 防止劫持
  3. 传统 UDP DNS (8.8.8.8:53) → 最终 fallback
  4. 本地 /etc/hosts → 应急逃生通道
```

关键：**多解析器 fallback + 结果缓存 + 负缓存**。当前 Catcher 只有单一 DNS 路径。

---

## 二、连接层面：不只是"建连、失败、重试"

### 2.1 连接预热

Chrome 的做法：在用户输入 URL 时就已经开始 DNS 预解析 + TCP 预连接，用户按下回车时连接已经建立。Facebook/TikTok 在 App 启动时预热到核心 API 域名的连接。

**需要的数据**（agent 采集后更新）：预热命中率、带宽开销、延迟节省。

### 2.2 Happy Eyeballs v2（RFC 8305）

同时发起 IPv6 和 IPv4 连接尝试，250ms 后启动 IPv4 竞速。适用于双栈网络。

### 2.3 协议降级链

HTTP/3 → HTTP/2 → HTTP/1.1。每降一级增加 1-2 RTT 的握手延迟。浏览器会自动降级，但客户端库通常不会。

### 2.4 QUIC 连接迁移

WiFi→蜂窝切换时 IP 变更，QUIC 理论上可以无中断迁移。实测：IPv4 成功率 52%，IPv6 78%。**不可依赖**。

---

## 三、CDN/边缘层面：在离用户最近的地方处理故障

### 3.1 多 CDN 策略

大型服务（Netflix、Amazon）使用多个 CDN。故障切换方式：
- **DNS 切换**：改 CNAME → 受 TTL 限制（分钟级）
- **HTTP 重定向**：302 到备用 CDN → 即时但需客户端支持
- **客户端多 endpoint**：Catcher 已支持

### 3.2 Cloudflare Argo / Orpheus

- Argo：实时监测 Internet 路径质量，自动选择最优路由。延迟降低 **~30%**
- Orpheus：检测 origin 不可达，自动绕行。免费向所有客户提供

### 3.3 边缘函数故障转移

Cloudflare Workers 在 origin 故障时：
- 返回 stale cache
- 返回静态 fallback 页面
- 重试到备用 origin

---

## 四、业务层面：网络不好时，优雅降级

### 4.1 Stale-while-revalidate

```
Cache-Control: max-age=3600, stale-while-revalidate=86400
```

CDN/浏览器在缓存过期后：先返回旧缓存，后台异步更新。用户看不到延迟。

### 4.2 对冲请求（Hedged Requests）

在 p95 延迟后，向不同服务器发送相同请求的副本。首个成功的响应被使用，其余取消。

Google 实测：P99 延迟降低 **40%**，额外请求量仅 **~5%**。

### 4.3 离线优先

- CRDT（无冲突数据类型）：Notion、Figma
- OT（操作变换）：Google Docs
- 本地优先 + 异步同步

### 4.4 乐观更新

先假设请求成功更新 UI，如果失败则回滚。适用于可逆操作。

---

## 五、运维层面：在生产中验证韧性

### 5.1 混沌工程

Netflix Chaos Monkey、AWS FIS、Gremlin、Chaos Mesh。

网络故障是混沌实验中最常见的注入类型（40.9%）。

### 5.2 SLO 告警

Google SRE 方法：burn rate × alert window = error budget consumed。

```
1h burn rate > 14.4× → critical alert (2% budget consumed)
6h burn rate > 6×   → warning alert (5% budget consumed)
```

### 5.3 金丝雀部署

DNS 变更、CDN 配置变更 → 先影响 1% 流量 → 观察错误率 → 逐步放量。

---

## 对 Catcher 的启示

| 层面 | Catcher 可以做什么 | 优先级 | 关键数据 |
|------|-------------------|:-----:|------|
| **DNS** | HTTPDNS/DoH + 多解析器 fallback + 缓存 + IP 优选 | 🔴 P0 | 4亿+用户,60%+劫持减少,22%延迟降低,99.99%SLA |
| **连接** | 预热(preconnect) + HappyEyeballs竞速 + 协议降级(H3→H2) | 🟡 P1 | 250ms CAD,40%连接可0-RTT,HEv3加入QUIC竞速 |
| **CDN/边缘** | 多endpoint(已有) + Argo/Orpheus对标 (路径绕行) | 🟡 P1 | Argo TTFB降33%,Orpheus救1320亿请求 |
| **业务层** | 对冲请求模式 + stale缓存返回 + 请求去重 | 🟢 P2 | Hedged:24×尾延迟降,2%开销; Mobile:29%会话信号差 |
| **运维** | 暴露retry/CB/DNS指标供SLO告警 + burn rate | 🟡 P1 | 14.4×critical,6×warning; GameDay降MTTR60%; 混沌ROI 245% |

### 最重要的设计原则

1. **竞速，不要等待**：QUIC/TCP、IPv6/IPv4、多endpoint — 并行启动，先用先得，缓存胜者
2. **缓存胜者和败者**：per-host, per-network 缓存 (RFC 8305: ~10min TTL)
3. **优雅降级，永不断路**：0-RTT被拒→1-RTT重试, QUIC被阻→TCP, 迁移失败→新连接
4. **可配置的激进程度**：移动弱网→激进竞速+0-RTT; 服务端API→简化顺序; IoT→最小开销
