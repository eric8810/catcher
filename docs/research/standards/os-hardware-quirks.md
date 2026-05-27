# 操作系统底层 / 移动端 / 硬件 — 网络栈特性与陷阱

> **catcher 的托底价值在 OS/硬件异动时最为凸显**——协议标准没问题，但实现有 bug、配置有反直觉默认值、移动 OS 有激进省电策略。

---

## 一、移动 OS 网络限制

### 1.1 Android Doze 模式

| 行为 | 参数 | Catcher 影响 |
|------|------|-------------|
| 进入条件 | 屏幕关闭 + 未充电 + 静止 ≥ 一段时间 | — |
| **网络访问** | **完全暂停** | 所有连接静默断开 |
| 维护窗口 | **每 15 分钟 → 30 分钟 → 1 小时 → 2 小时**（逐渐稀疏） | 窗口外一切连接失效 |
| Wake Lock | 被忽略 | `setAndAllowWhileIdle()` 才可用 |
| WiFi 扫描 | 暂停 | — |
| 高优先级 FCM | **可穿透 Doze** | 推送通道是唯一可靠方式 |
| 闹钟 | `setAlarmClock()` 可唤醒，普通 `set()` 被推迟 | — |

**对 Catcher 的关键影响**：
- 后台 SSE/WS 长连接在 Doze 下**不可能维持**
- App 唤醒后，所有连接需要**重新建立**
- reconnect 的退避策略**不应继承 Doze 期间的状态**（飞行模式同理）

### 1.2 Android App Standby

| 行为 | 条件 |
|------|------|
| 网络访问 | **每天 1 次**维护窗口 |
| 触发条件 | App 多天未被用户主动使用 |
| FCM | 降级为普通优先级 |

### 1.3 Android Data Saver

| 行为 | Catcher 影响 |
|------|-------------|
| 前台 App: **不受限制** | — |
| 后台 App: 网络访问受限 | 后台连接断开 |
| `ConnectivityManager.isActiveNetworkMetered()` | Catcher 可感知计费网络 |

### 1.4 iOS 后台网络

| 行为 | 参数 | Catcher 影响 |
|------|------|-------------|
| 后台执行时间 | **iOS 13+: 30 秒** | SSE/WS 在 30s 后被挂起 |
| `beginBackgroundTask` | 可延长到 **~3 分钟**（实际取决于系统负载） | 不够支撑长连接 |
| URLSession 后台模式 | **系统接管传输**，进程可能被 kill | Catcher 无法控制 |
| VoIP Push | 特殊 entitlement，可唤醒 | 普通 App 拿不到 |
| BGTaskScheduler | 周期性后台刷新，~15 分钟一次 | 频繁度不够 |
| **iOS Watchdog** | 主线程阻塞 **~10s → 0x8badf00d crash** | **同步网络请求 = 必崩** |

### 1.5 iOS 网络类型切换

| 事件 | 行为 | Catcher 影响 |
|------|------|-------------|
| WiFi → Cellular | WiFi Assist 或手动关闭 WiFi | **IP 地址变更**，所有 TCP 连接 RST |
| Cellular → WiFi | 自动切换 | **IP 地址变更** |
| VPN 连接/断开 | 路由表变化 | 可能由不同接口发出 |
| 飞行模式 | 所有无线关闭 | 类似 Doze |
| Low Data Mode (iOS 13+) | App 应减少网络使用 | 系统不强制 |

---

## 二、桌面 OS 网络栈

### 2.1 Linux TCP 默认参数

```bash
# 这些默认值直接影响 Catcher 的 keepAlive 和超时策略
/proc/sys/net/ipv4/tcp_keepalive_time   = 7200   # 2h 后才发第一个 keepalive 探测
/proc/sys/net/ipv4/tcp_keepalive_intvl  = 75     # 探测间隔
/proc/sys/net/ipv4/tcp_keepalive_probes = 9      # 探测次数
/proc/sys/net/ipv4/tcp_retries2         = 15     # ≈ 13-30min 用户超时
/proc/sys/net/ipv4/tcp_syn_retries      = 6      # SYN 重试 ≈ 127s
/proc/sys/net/ipv4/tcp_fin_timeout      = 60     # FIN-WAIT-2 60s
```

**关键影响**：
- `tcp_keepalive_time=7200` 意味着 Catcher 的 keepAlive 默认为 30s 是**绝对必要**的
- `tcp_retries2=15` 意味着路由黑洞下 TCP 会等 **13+ 分钟**——Catcher 的应用层超时 (30s) 远早于此
- `tcp_syn_retries=6` 意味着 connect() 可能阻塞 **2 分钟**

### 2.2 Linux conntrack 表溢出

```
症状：新连接无法建立，现有连接正常
原因：nf_conntrack: table full, dropping packet
解决方案：增加 conntrack_max 或降低超时，或对 Catcher 来说——正确报告给用户
```

### 2.3 musl vs glibc DNS

| 行为 | glibc | musl (Alpine) |
|------|-------|---------------|
| `search domains` 自动补全 | ✅ | ❌ (需 `options ndots:n` 显式配置) |
| 并发 DNS 查询 | ✅ 有内部限流 | ❌ 可能全部同时发出 |
| `/etc/hosts` 优先 | ✅ | ✅ |
| 超时处理 | 有重试 | 行为不同 |
| Docker 中表现 | 正常 | **K3s#6132 已知问题**：DNS 解析失败 |

### 2.4 macOS App Sandbox

| entitlement | 说明 |
|-------------|------|
| `com.apple.security.network.client` | **必须启用**才能出站连接 |
| `com.apple.security.network.server` | 入站连接（极少场景） |

### 2.5 Windows TCP 默认值

| 参数 | 默认 | 与 Linux 的差异 |
|------|------|----------------|
| TCP KeepAlive 空闲 | 7200s (2h) | 同 |
| TCP KeepAlive 间隔 | **1s** (vs Linux 75s) | **差异显著** |
| TCP KeepAlive 探测次数 | 5-10 | 类似 |
| 初始 RTO | **3s** | Linux 1s |

---

## 三、NAT / 中间设备

### 3.1 NAT 超时表

| 设备/场景 | TCP 空闲超时 | UDP 超时 | Catcher 影响 |
|----------|:---------:|:------:|-------------|
| Linux conntrack | 5 天 (established) | 30-180s | — |
| 家用路由器 | 30min - 2h | 30-120s | keepAlive 30s 可保活 |
| CGNAT (运营商) | **60-120s** | 30-60s | ⚠️ **最激进**——keepAlive 必须 < 60s |
| AWS NAT Gateway | 350s | 120s | — |
| 企业防火墙 | 15min - 1h | 30-60s | 各不相同 |

**关键**：[CGNAT 空闲超时可能只有 60-120s](https://anderstrier.dk/2021/01/11/my-isp-is-killing-my-idle-ssh-sessions-yours-might-be-too/)。Catcher keepAlive 默认 30s 可覆盖，但建议文档说明。

### 3.2 负载均衡器空闲超时

| LB | 空闲超时 | 说明 |
|----|:------:|------|
| AWS Classic ELB | 60s (可配置) | keepAlive 需 < 60s |
| AWS ALB | 60s (可配置到 6000s) | 有 HTTP/2 |
| AWS NLB | 350s | TCP 层 |
| Nginx | 75s (默认 `keepalive_timeout`) | 还可设 `keepalive_requests` |
| HAProxy | `timeout client` 50s (常见配置) | 取决于运维 |
| Envoy | 1h (默认) | Sidecar 模式 |

### 3.3 代理故障模式

| 故障 | 对 Catcher 的影响 | 测试建议 |
|------|-----------------|---------|
| 透明代理返回 HTML 而非 JSON | Content-Type 非预期 | 验证错误信息清晰 |
| 企业代理 TLS MITM | 需导入企业 CA (`ca_cert_pem`) | 验证 CA 在 proxy 场景生效 |
| HTTP 407 Proxy Auth | 代理要求认证 | 🔴 需分类为 NonRetryable |
| 代理连接超时 | connect_timeout 分段计时 | 验证超时错误包含代理信息 |

### 3.4 CDN / Edge

| 场景 | Catcher 影响 |
|------|-------------|
| CDN 回源失败 → 返回 5xx | CB 应作用在 CDN 节点级别 |
| Edge Function 超时 (Cloudflare 30s CPU) | SSE 流可能中断 |
| 多 CDN 故障切换 | 需 DNS 重解析（不同 CNAME） |

---

## 四、硬件级网络陷阱

### 4.1 WiFi 芯片固件 Bug

| 问题 | 表现 | Catcher 应对 |
|------|------|-------------|
| Intel AX200/AX210 随机断连 | 连接丢失 1-5s 后自动恢复 | Should NOT trigger reconnect; wait |
| Broadcom 固件 DHCP 更新卡死 | IP 丢失但仍然 "connected" | OS 级问题；apply-level timeout |
| Realtek USB WiFi 热插拔 | 突然消失 | 快速检测 socket 错误 |
| 路由器 ARP 表溢出 | 新连接无法建立但旧连接正常 | 与 conntrack 溢出类似 |

### 4.2 蜂窝基带问题

| 现象 | 描述 | Catcher 应对 |
|------|------|-------------|
| 基站拥塞 | RRC 连接成功但数据不通 | 类似 grey failure |
| 基站切换时短暂断流 | 50-100ms | TCP 重传可恢复 |
| iPhone 基带崩溃 | 搜索网络 → 恢复（~10s） | 飞行模式不继承状态 |
| Qualcomm X55/60 热节流 | 5G mmWave 过热→降级 4G | RTT/带宽突变 |

### 4.3 以太网/USB 网卡

| 场景 | 表现 |
|------|------|
| USB-C Dongle 拔插 | 接口消失 → MAC 地址变化 |
| 以太网唤醒 (WoL) | 从睡眠恢复后 ARP 缓存过期 |
| TCP Offload Bug | 校验和错误导致重传 |

---

## 五、测试案例汇总

### 5.1 应新增的 Catcher 测试场景

| 新增场景 | OS/硬件背景 | 测试设计 |
|---------|-----------|---------|
| **S17: Doze/后台模拟** | Android Doze 或 iOS 后台 | proxy.ts 模拟：15s 正常 → 120s blackhole → 恢复 |
| **S18: WiFi→Cellular 切换** | IP 变更 + 短暂中断 | proxy.ts: 50ms blackhole + 重连所有连接用新 localPort |
| **S19: CGNAT 60s 空闲超时** | NAT 静默清理映射 | proxy.ts: 60s idle → 静默丢弃所有包 |
| **S20: 代理劫持 (HTML 响应)** | 透明代理返回 HTML | wiremock 返回 Content-Type: text/html |
| **S21: conntrack 满** | 新连接无法建立 | proxy.ts 拒绝新连接但保持旧连接 |
| **S22: TCP Offload Bug** | 重传风暴 | proxy.ts: 随机 corrupt 1% 包 |
| **S23: DFS 信道切换** | WiFi 10s 静默 | proxy.ts: 10s blackhole → 恢复 (类似 S12a) |
| **S24: 基站拥塞 (Grey Failure)** | 部分请求超时 | proxy.ts: 30% 概率 10s 延迟 |
| **S25: 飞行模式 → 恢复** | 全连接断开 → 新 IP | 同 S18 但中断 2s |

### 5.2 新增 Profile 建议

| Profile | OS/硬件背景 | 参数 |
|---------|-----------|------|
| `cg_nat` | CGNAT 空闲超时 60s | 正常 RTT，60s idle 后 blackhole |
| `wifi_cellular_switch` | WiFi↔Cellular | 50ms blackhole + IP 变更 |
| `doze_recovery` | Android Doze 恢复 | 120s blackhole → 正常 |
| `grey_failure` | 基站拥塞 | 30% loss (burst), RTT 变为 200ms |
| `enterprise_proxy` | TLS MITM 代理 | 正常 + 需要 CA 证书 |
