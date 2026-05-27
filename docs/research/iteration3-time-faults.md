# Iteration 3 — 时间故障 × 移动端约束 深度挖掘

> 框架 v3 · 第 3 轮循环
> 聚焦：时间故障（延迟/抖动/漂移）× L1 物理 + L5 运行环境

---

## 一、延迟：实测数据（超越 3GPP 理论值）

### 1.1 Rohde & Schwarz 5G NSA 实测（2022，慕尼黑商用网络）

| 场景 | 单向延迟 (OWL) | 对应 RTT |
|------|:-----------:|:------:|
| 5G NR 最佳（DL, 100kbps） | **< 7ms** | ~14ms |
| 5G NR 最差（UL, 15Mbps） | **~18ms** | ~36ms |
| 移动测量 RTT 异常值 | > 100ms | — |
| 静止测量 | 稳定且一致 | — |

**关键洞察**：移动中（cell handover）RTT 可突增至 >100ms，同一位置静止测量则稳定。这意味着用户的物理运动状态直接影响延迟——Catcher 的超时策略不能假设 RTT 稳定。

### 1.2 Bufferbloat：空闲 vs 负载延迟的巨大差异

| 状态 | 典型 RTT | 来源 |
|------|:------:|------|
| 空闲 (idle) | 56ms | Cloudflare AIM 实测 |
| 负载 (loaded) | 174ms | 同上 |
| 差值 | **+118ms** | bufferbloat |

**影响**：在并发请求场景下（如页面加载、API 批量调用），每个请求的实际延迟远超空闲测量值。Catcher 的超时应基于 loaded latency 的 p95，而非 idle。

### 1.3 延迟突变

| 突变类型 | 中断时间 | 来源 |
|---------|:------:|------|
| LTE 同频切换 | 30-50ms | 3GPP TS 36.133 |
| LTE 异频切换 | 50-100ms | 3GPP |
| LTE→3G IRAT | 500ms-2s | 实测 |
| LTE→2G IRAT | 1-5s | 实测 |
| Wi-Fi BSS Transition (无 802.11r) | 500ms-3s | IEEE 802.11 |
| Wi-Fi BSS Transition (802.11r FT) | 10-50ms | IEEE 802.11r |
| DFS 信道切换 | **1-10s 完全静默** | IEEE 802.11h |
| 5G NR 同频 (make-before-break) | **0ms** | 3GPP TS 38. |

---

## 二、移动 OS 的硬性时间约束

### 2.1 Android Doze 维护窗口的展开算法

虽然 AOSP 源码不公开确切算法，但社区测试和文档揭示：

| 阶段 | 间隔 | 窗口持续 |
|------|:---:|:------:|
| 进入 Doze（屏幕关闭+静止+不充电） | 立刻 | — |
| 第一阶段窗口 | ~15min 间隔 | ~30s |
| 第二阶段窗口 | ~30min 间隔 | ~30s |
| 深度 Doze | **1-2h 间隔** | ~30s |

**对 Catcher 的致命影响**：WS/SSE 心跳在 Doze 下暂停 → CGNAT 60-120s 超时后映射被拆除 → 下一个维护窗口（2h 后）尝试重连，但 NAT 映射已不存在 → SYN 被黑洞或 RST → 重试在 30s 窗口内耗尽（5 次退避 ≈ 16s）→ 再等 2h。

**解决方案**：Android 平台需要 `ConnectivityManager.NetworkCallback` 监听网络恢复，而非依赖定时重试。

### 2.2 iOS 后台限制

| 机制 | 持续时间 | 限制 |
|------|:------:|------|
| `beginBackgroundTask` | **~30s**（iOS 13+） | 系统可能更早终止 |
| `BGTaskScheduler` | 系统决定 | 约 15min 间隔，不可靠 |
| URLSession 后台模式 | 系统接管 | 进程可能被 kill |
| iOS Watchdog | 主线程阻塞 ~10s → crash | 同步网络请求 = 必崩 |

**对 Catcher**：iOS 上无法维持长连接。Catcher 的 iOS FFI 应在进入后台时显式关闭连接，回到前台时全量重连，不继承后台期间的退避状态。

### 2.3 CGNAT 空闲超时（移动网络的隐藏杀手）

| 运营商/场景 | TCP 空闲超时 |
|-----------|:--------:|
| CGNAT（典型） | **60-120s** |
| AT&T 3G（报告值） | 30s UDP |
| 家用路由器 | 30min-2h |
| 企业防火墙 | 15min-1h |

**Catcher keepAlive 默认 30s** 可覆盖大部分 CGNAT，但需警告：若用户自定义为 > 60s，移动网络下连接将被静默断开。

---

## 三、对 Catcher 配置的具体影响

### 3.1 超时参数推荐值

| 参数 | 当前默认 | 推荐默认 | 依据 |
|------|:------:|:------:|------|
| `connect_timeout` | 未设/依赖 OS | **15s** | 对标 Cloudflare 19s，低于 Linux 127s |
| `response_timeout` | 30s | **30s**（保持） | 覆盖 loaded latency p95×4 |
| `keepAlive` interval | 30s | **30s**（保持） | 低于 CGNAT 60s 最坏情况 |
| `max_backoff` | 10,000ms | **min(RTT_p90×4, 30,000ms)** | RTT 感知联动 |
| `jitter` | 未设 | **±25%** | Google SRE + AWS 最佳实践 |

### 3.2 退避策略按 RTT 自适应

```
if RTT_p90 < 100ms:  max_backoff = 10,000ms  (当前默认)
if RTT_p90 100-500ms: max_backoff = 30,000ms  (GEO 卫星场景)
if RTT_p90 > 500ms:   max_backoff = 60,000ms  (极端高延迟)
```

### 3.3 移动平台特殊策略

| 平台 | 策略 |
|------|------|
| Android | 监听 `NetworkCallback.onAvailable()` 触发立即重连；Doze 期间不计数退避次数 |
| iOS | 进入后台时关闭长连接；回前台时全量重连，不继承后台退避状态 |
| 通用 | `keepAlive` < 45s（CGNAT 安全边际） |

---

## 四、本次迭代发现的 P0 缺口

| # | 缺口 | 影响 |
|---|------|------|
| T-1 | `max_backoff` 与 RTT 不联动 | GEO 卫星等高延迟场景过早放弃 |
| T-2 | 移动端平台退避不感知 Doze/iOS 后台 | 恢复时间可达 2h |
| T-3 | `connect_timeout` 依赖 OS 默认 | Linux 下 BGP 黑洞等待 127s |
| T-4 | 缺少 bufferbloat 感知（loaded vs idle latency） | 高并发下超时误触发 |
