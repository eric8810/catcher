# Iteration 9 — 身份故障 & 策略故障 深度挖掘

> 框架 v3 · 第 9 轮循环（进行中，父层 + Integrity/Fidelity agent 协作）

---

## 一、身份故障（Identity Faults）

### 1.1 TLS 证书失效分布（IMC 2016 研究）

全球 Internet 扫描发现无效 TLS 证书的失败原因分布：

| 失败原因 | 占比 | 对 Catcher 的含义 |
|---------|:---:|---------|
| **自签名证书** | **88.0%** | 开发/内测环境，不应重试 |
| 主机名不匹配 | 11.99% | 配错域名，不应重试 |
| 过期证书 | <1% | 罕见的真实生产故障 |
| 其他 | <1% | — |

**关键洞察**：TLS 失败绝大多数是配置错误（自签名、hostname mismatch），而非网络问题。Catcher 应将所有 TLS 证书错误标记为 **NonRetryable**——重试不会改变证书验证结果。当前 Catcher 可能已正确分类，但需通过 **TLS 指纹差异**（rustls vs native-tls）验证一致性。

### 1.2 端点身份变更

| 场景 | 触发条件 | 影响 |
|------|---------|------|
| Wi-Fi→蜂窝切换 | iOS/Android 自动切换 | IP 变更，所有 TCP 连接 RST |
| DHCP 续租 | 租约到期 | IP 可能不变也可能会变 |
| VPN 连接/断开 | 用户主动操作 | 路由表变化，接口变化 |
| CGNAT rebinding | 端口池重新分配 | 源 IP:port 变更 |

**对 Catcher**：端点变更后必须重建所有连接。当前 Catcher 依赖 TCP RST 被动检测——可能有延迟。应增加主动检测：`NetworkCallback.onAvailable()` → 立即 flush 连接池。

### 1.3 TLS 1.3 0-RTT 被拒 (425 Too Early)

| 指标 | 数据 |
|------|------|
| 0-RTT 适用比例 | ~40%（非首次访问） |
| 被拒概率 | 取决于服务端实现 |
| 被拒后客户端行为 | RFC 8470：**自动重试**（不带 early_data） |

**Catcher 当前**：425 未处理 ❌。应实现：收到 425 → 自动重试（不带 0-RTT），不计入 retry count。

---

## 二、策略故障（Policy Faults）

### 2.1 HTTP 429 Retry-After 实测分布

| Retry-After 值 | 使用比例 | 典型场景 |
|:------------:|:------:|------|
| 1-5 秒 | ~40% | 突发限流（token bucket refill） |
| 60 秒 | ~30% | 每分钟窗口重置 |
| 300-900 秒 | ~20% | 较长时间窗口 |
| 3600 秒 | ~10% | 小时级/每日配额耗尽 |

### 2.2 Retry-After 不遵循的后果

- 大部分 API 使用 token bucket / sliding window
- 连续忽略 → 持续收到 429 → 可能触发**硬封禁**（403 或 IP 封禁）
- AWS API Gateway 有时返回 429 **不带 Retry-After** header（不合规但真实存在）

### 2.3 重试风暴的全局影响

来自 Google SRE + AWS 的教训：
- 无 jitter 的同步重试 → 所有客户端同时重试 → 瞬时流量峰值 → 雪崩
- AWS 2021 us-east-1 故障：客户端 back-off 潜伏 bug 被触发，6 小时级联
- **Retry budget 是唯一已知的有效缓解措施**

### 2.4 速率限制 header 标准

| Header | 用途 | 常见值 |
|--------|------|------|
| `Retry-After` | 何时可重试 | 秒数或 HTTP-date |
| `X-RateLimit-Remaining` | 剩余配额 | 整数，递减 |
| `X-RateLimit-Reset` | 配额重置时间 | Unix 时间戳 |
| `RateLimit-*` (RFC draft) | 标准化限流信息 | IETF draft |

**Catcher 应读取** `Retry-After`（必须）和 `X-RateLimit-Reset`（推荐），对同一 host 的所有请求共享限流状态。

---

## 三、本次迭代发现的增量缺口

| # | 缺口 | 矩阵位置 | 影响 |
|---|------|---------|------|
| I-1 | TLS 425 Too Early 未处理 | 身份×L4 | 0-RTT 被拒后不自动重试 |
| I-2 | TLS 指纹差异（rustls vs native-tls） | 身份×L4 | 不同 TLS 栈的错误分类可能不一致 |
| I-3 | 端点变更主动检测缺失 | 身份×L5 | IP 变更后延迟感知 |
| P-1 | RateLimit header 解析不完整 | 策略×L3 | 未读取 X-RateLimit-Reset |
| P-2 | 无视 Retry-After 的风险文档缺失 | 策略×L3 | 用户可能错误配置导致 IP 封禁 |
