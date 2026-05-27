# 阶段二：网络环境与拓扑调研

> 调研日期：2025-07-18
> 范围：网络架构、中间设备、链路特性、地域延迟、云原生环境
> 目标：发现 catcher 在各种网络拓扑下的未覆盖场景

---

## B1. 代理与中间件环境

### B1.1 代理类型全景

catcher 当前已支持 HTTP 代理和 SOCKS5 代理。但生产环境中代理类型远比这两类复杂：

| 代理类型 | 协议层面 | catcher 覆盖 | 风险点 |
|---------|---------|:----------:|--------|
| Forward HTTP Proxy | HTTP `CONNECT` / 直接转发 | ✅ | 代理认证失败重试 |
| Forward SOCKS5 Proxy | SOCKS5 协议 | ✅ | UDP associate 模式（WS 场景） |
| **Transparent Proxy** | 无客户端配置，网络层拦截 | ❌ | 客户端不知道代理存在，TLS MITM |
| **Reverse Proxy** | 对客户端透明，后端路由 | ✅（对客户端透明） | — |
| **PAC (Proxy Auto-Config)** | JS 脚本动态选代理 | ❌ | 需要解析 PAC 文件 |
| **WPAD (Web Proxy Auto-Discovery)** | DHCP/DNS 自动发现 | ❌ | 企业网络常见 |
| **SSH Tunnel (SOCKS5)** | `ssh -D 1080` | ✅ | 断线后需手动重建 |
| **HTTP/2 CONNECT (MASQUE)** | 基于 HTTP/2 的代理隧道 | ❌ | 新兴标准，未来需求 |

### B1.2 代理故障模式

| 场景 | 描述 | catcher 覆盖 | 建议测试 |
|------|------|:----------:|---------|
| 代理不可达 | 代理服务器宕机 | ✅ connect timeout | 验证 CB 是否应 trigger（应：代理故障 != 后端故障） |
| 代理间歇性失败 | 代理有概率返回 502 | ⚠️ | 验证 proxy error 的分类（Retryable?） |
| 代理认证失败 (407) | 代理要求认证 | ✅ | 407 非 retryable（除非 token 可刷新） |
| 代理连接超时 | 代理 connect 慢 | ⚠️ | connect_timeout 对代理+后端是否分段计时 |
| 代理劫持响应 | 代理返回 HTML 而非 JSON | ❌ | Content-Type 验证不通过时的错误信息 |
| 代理部分缓冲 | 代理缓冲 chunked 响应（类似 SSE 问题） | ❌ | 流式响应+cumulative buffer |
| 代理并发限制 | 代理有最大并发连接数 | ❌ | 连接排队时是否超时合理 |

### B1.3 企业代理环境

**关键发现**：企业环境（银行、政府、大型公司）常有：
- TLS 中间人代理（企业 CA 重签证书，catcher 需导入企业 CA）
- NTLM/Kerberos 认证代理（需要 3-way handshake，非标准 Basic Auth）
- 白名单代理（只允许特定域名出站，DNS 泄露风险）
- 代理链（多个代理串联）

| 场景 | catcher 覆盖 | 建议 |
|------|:----------:|------|
| TLS MITM 代理 + 企业 CA | ⚠️ `ca_cert_pem` | 验证自定义 CA 在 proxy 场景生效 |
| NTLM 代理认证 | ❌ | 超出范围（需外部库），但应文档说明 |
| 代理链 | ❌ | 调研：reqwest 是否支持多级代理 |

---

## B2. NAT 与地址转换

### B2.1 Carrier-Grade NAT (CGNAT)

**关键发现**：[CGNAT 的 TCP session timeout 可能极短](https://anderstrier.dk/2021/01/11/my-isp-is-killing-my-idle-ssh-sessions-yours-might-be-too/)，某些 ISP 在 60-120 秒无数据后就清除 NAT 映射。这意味着：

| 场景 | 描述 | catcher 覆盖 | 建议 |
|------|------|:----------:|------|
| CGNAT 空闲连接超时 | TCP 连接静默断开，无 RST/FIN | ⚠️ keepAlive 30s | 验证 keepAlive interval 是否低于 NAT timeout |
| NAT 端口耗尽 | 大量并发连接耗尽 NAT 端口 | ❌ | 连接失败时的错误分类 |
| NAT 类型差异 | Full Cone / Restricted / Port Restricted / Symmetric | ❌ | 影响 P2P/WebRTC 场景（non-goal） |

### B2.2 网络地址家族

| 场景 | catcher 覆盖 | 建议 |
|------|:----------:|------|
| **IPv6-only 环境** | ⚠️ 依赖 reqwest | 测试 IPv6-only 服务端、IPv6-only 客户端 |
| **IPv4-only → IPv6 服务** | ❌ | DNS64/NAT64 环境 |
| **Happy Eyeballs (RFC 8305)** | ⚠️ 依赖 reqwest | 验证 IPv6 不可达时快速回退 IPv4 |
| **IPv6 链路本地地址** | ❌ | `fe80::` 地址，需指定 scope_id (interface index) |
| **IPv6 地址中的 zone ID** | ❌ | `http://[fe80::1%eth0]:8080/` 格式解析 |

**关键发现**：reqwest 0.13 使用 `hickory-resolver` 时支持 Happy Eyeballs，但 catcher 的自定义 `host_mapping` 对 IPv6 的支持需要验证。

> **2025-07-21 验证**：`dns.rs:155` 使用 `IpAddr::parse(ip_str)` 解析映射值，`IpAddr` 枚举天然支持 IPv6（如 `::1`）。代码层面已就绪，但缺少 IPv6 host_mapping 的专项测试用例。

---

## B3. 负载均衡与服务网格

### B3.1 负载均衡器行为

| 负载均衡器 | 典型行为 | catcher 影响 |
|-----------|---------|-------------|
| AWS Classic ELB | 60s idle timeout，可配置 [1] | keepAlive 需 < 60s |
| AWS ALB | HTTP/2 aware，支持 gRPC [2] | ALB 可能在空闲 60s 后断开 |
| AWS NLB | TCP 层，无 HTTP 语义 [3] | 不感知 HTTP keepAlive，由后端决定 |
| Nginx | `keepalive_timeout` 默认 75s [4] | 可能与 client keepalive 不同步 |
| HAProxy | `timeout client` / `timeout server` [5] | 可能两端独立断开 |
| Envoy (Service Mesh) | Sidecar 模式，每个 Pod 一个代理 [6] | 连接池在 sidecar 层生效 |

| 场景 | catcher 覆盖 | 建议 |
|------|:----------:|------|
| LB 空闲超时 < 客户端 keepAlive | ⚠️ | 文档说明 keepAlive interval 需小于 LB idle timeout |
| LB 健康检查干扰 | ❌ | LB health check 请求不应触发 CB 和 metrics |
| 服务端滚动重启 | ❌ | 连接断开→retry→新实例→成功的时间窗口 |
| Sticky Session (Cookie-based) | ⚠️ | 验证 redirect 后 cookie 不变 |
| 连接迁移 (Connection Migration) | ❌ | HTTP/2 连接迁移到新实例导致协议错误 |

### B3.2 DNS 负载均衡

| 场景 | catcher 覆盖 | 建议 |
|------|:----------:|------|
| Round-robin DNS | ✅ (hickory-dns) | 验证 DNS cache TTL 到期后获取新 IP list |
| GeoDNS / Latency-based | ✅ (对客户端透明) | 验证连接失败后是否尝试其他解析 IP |
| DNS 返回多个 A 记录 | ⚠️ | 验证第一个 IP 失败后是否尝试其余 |

---

## B4. 云原生与边缘计算

### B4.1 Kubernetes / 容器环境

| 场景 | 描述 | catcher 覆盖 | 建议 |
|------|------|:----------:|------|
| Pod IP 变化 | Pod 重启后 IP 改变 | ❌ | DNS cache 需及时刷新 |
| 服务网格 sidecar | Envoy/Linkerd 拦截所有流量 | ❌ | 连接池在 sidecar 层双重叠加问题 |
| 容器 DNS 超时 | Alpine musl DNS 并发限制 [8] | ❌ | 验证 DNS 超时不阻塞请求 |
| 资源限制 (CPU throttling) | K8s CPU limit 导致超时 | ❌ | 超时类错误的 retry 策略验证 |
| OOM Kill | 被 K8s kill 时的 graceful shutdown | ❌ | 进行中请求的状态（由 OS 处理） |

### B4.2 CDN 与边缘

| 场景 | catcher 覆盖 | 建议 |
|------|:----------:|------|
| CDN 回源失败 | CDN 返回 5xx，源站正常 | ⚠️ | 验证 CB 作用域是 CDN 节点还是源站 |
| 多 CDN 故障切换 | 主 CDN 故障→备用 CDN | ❌ | 需要 DNS 重解析或多域名支持 |
| Edge Function 超时 | Cloudflare Workers 有 30s CPU 限制 [7] | ❌ | 超时后 SSE 流中断 |
| 边缘节点回源延迟 | 冷启动 + 回源慢 | ⚠️ | 首次连接 RTT 异常高 |

---

## B5. 特殊网络拓扑

### B5.1 VPN 环境

| 场景 | catcher 覆盖 | 建议 |
|------|:----------:|------|
| VPN 连接建立/断开 | 网络接口状态变化 | ❌ | 是否检测到网络变化并重建连接池 |
| Split Tunnel | 部分流量走 VPN，部分直连 | ❌ | DNS 解析走不同解析器 |
| VPN DNS 泄露 | DNS 不走 VPN 隧道 | ❌ | 需文档说明 DNS 配置的隐私影响 |
| MTU 变化 | VPN 降低有效 MTU (TLS overhead) | ❌ | 大 payload 可能被 fragment |

### B5.2 卫星与高延迟网络

| 场景 | 延迟 | catcher 覆盖 | 建议 |
|------|------|:----------:|------|
| GEO 卫星 | 500-600ms RTT | ⚠️ `response_timeout_ms` | 默认超时可能不够 |
| LEO 卫星 (Starlink) | 20-100ms RTT, 高抖动 [9] | ⚠️ | 验证 adaptive timeout 对高抖动的适应性 |
| 偏远 3G/2G | 200-2000ms RTT | ⚠️ (已有部分测试) | 验证 retry min_backoff 是否仍为 500ms |

### B5.3 单向/间歇网络

| 场景 | 描述 | catcher 覆盖 | 建议 |
|------|------|:----------:|------|
| Air-gapped（物理隔离） | 完全无网络 | ❌ | 连接超时等待完整 timeout 周期（验证不提前 panic） |
| 网络闪断 (Flapping) | 每 N 秒通断交替 | ❌ | CB 可能在 flapping 网络下错误触发 |
| 半双工链路 | 上传/下载不对称 | ❌ | 请求发出但收不到响应 (单向通信) |

### B5.4 延迟与丢包模式

catcher 的 E2E 测试已覆盖 7 种预设网络条件（good→metro），以下是未覆盖的真实世界网络退化模式：

| 模式 | 特征 | 测试建议 |
|------|------|---------|
| **Bufferbloat** | 延迟高但不丢包 | 验证 adaptive timeout 响应 |
| **Micro-outage** | 100-500ms 零连通 | 此类短断连不应触发重连 |
| **Gray failure** | 部分请求成功、部分失败 | CB threshold 的敏感性 |
| **Black hole** | 数据包被静默丢弃（无 ICMP） | 依赖 connect_timeout，非 retryable |
| **Asymmetric routing** | 去程和回程路径不同 | 可能出现一个方向丢包 |

---

## 阶段二总结：关键缺失场景

### 高优先级

1. **CGNAT 空闲超时** — keepAlive interval 应可配置并建议 < 60s
2. **HTTP 407 Proxy Auth Required** — 需明确定义为非 retryable
3. **IPv6-only 环境** — DNS host_mapping 代码已支持 IPv6（`IpAddr::parse`），需补充测试
4. **LB idle timeout < client keepAlive** — 文档警告
5. **代理返回非预期 Content-Type** — 不 panic，错误信息清晰
6. **网络闪断时 CB 误触发** — CB 阈值需考虑网络闪断模式
7. **Gray failure** — 部分成功/部分失败时 retry 策略
8. **VPN 网络变化检测** — 连接池是否感知网络接口变化

### 中优先级

9. **PAC/WPAD 代理自动发现** — 企业网需求，文档说明不支持
10. **NTLM 代理认证** — 企业需求，文档说明不支持
11. **容器 DNS 并发限制** — Alpine 环境下的 DNS 超时
12. **MTU 变化（VPN/PPPoE）** — 大 payload 分片问题
13. **LEO 卫星高抖动** — adaptive timeout 调优
14. **Black hole 路由** — 数据包静默丢弃的超时策略

---

## 引用来源

1. AWS, "Configure the idle connection timeout for your Classic Load Balancer," https://docs.aws.amazon.com/elasticloadbalancing/latest/classic/config-idle-timeout.html
2. AWS, "Application Load Balancer attributes," https://docs.aws.amazon.com/elasticloadbalancing/latest/application/edit-load-balancer-attributes.html
3. AWS, "Network Load Balancer target groups health checks," https://docs.aws.amazon.com/elasticloadbalancing/latest/network/target-group-health-checks.html
4. Nginx, "Module ngx_http_proxy_module — proxy_read_timeout," https://nginx.org/en/docs/http/ngx_http_proxy_module.html
5. HAProxy, "timeout client / timeout server documentation," https://docs.haproxy.org/
6. Envoy, "Service Mesh sidecar architecture," https://www.envoyproxy.io/docs/
7. Cloudflare, "Workers Limits — CPU time limit (default 30s)," https://developers.cloudflare.com/workers/platform/limits/
8. K3s issue #6132, "DNS resolution in alpine (musl) based containers fails," https://github.com/k3s-io/k3s/issues/6132 ; and "The ndots:5 Tax" (K8s DNS performance), https://loke.dev/blog/kubernetes-dns-ndots-performance
9. Packetstorm, "Starlink Satellite Internet in 2026: Bandwidth, Latency, and Packet Loss Analyzed," https://packetstorm.com/starlink-satellite-internet-in-2026-bandwidth-latency-and-packet-loss-analyzed/ ; and APNIC Labs, "Measuring Starlink Protocol Performance," https://labs.apnic.net/presentations/store/2025-05-07-starlink-lacnic.pdf
10. Anders Trier, "My ISP Is Killing My Idle SSH Sessions. Yours Might Be Too." (CGNAT session timeout), https://anderstrier.dk/2021/01/11/my-isp-is-killing-my-idle-ssh-sessions-yours-might-be-too/
