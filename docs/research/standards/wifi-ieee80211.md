# WiFi 标准溯源 — IEEE 802.11 a/b/g/n/ac/ax/be

> WiFi 的损伤模式与蜂窝网络有本质不同：共享介质、CSMA/CA 竞争、AP 漫游、DFS 等是 Catcher 当前完全未覆盖的维度。

---

## 一、标准演进速览

| 代 | IEEE 修正案 | 年份 | 频段 | 最大 PHY 速率 | 调制 | 典型 RTT |
|:--|-----------|------|------|:----------:|------|:------:|
| — | 802.11b | 1999 | 2.4GHz | 11 Mbps | DSSS/CCK | 5-15ms |
| — | 802.11a | 1999 | 5GHz | 54 Mbps | OFDM | 3-8ms |
| — | 802.11g | 2003 | 2.4GHz | 54 Mbps | OFDM/DSSS | 3-10ms |
| WiFi 4 | 802.11n | 2009 | 2.4/5GHz | 600 Mbps | OFDM + MIMO | 2-5ms |
| WiFi 5 | 802.11ac | 2013 | 5GHz | 6.9 Gbps | OFDM + MU-MIMO | 1-3ms |
| WiFi 6 | 802.11ax | 2021 | 2.4/5/6GHz | 9.6 Gbps | OFDMA + MU-MIMO | <1-2ms |
| WiFi 7 | 802.11be | 2024 | 2.4/5/6GHz | 46 Gbps | OFDMA + MLO | <0.5ms |

---

## 二、WiFi 特有的损伤模式

### 2.1 RSSI → MCS → 吞吐映射

WiFi 的自适应速率控制 (RA) 根据信号强度选择调制编码策略：

| RSSI (dBm) | 802.11ac MCS Index | 理论速率 (1×1, 80MHz) | TCP 实测 | 丢包率 |
|:---------:|:-----------------:|:-------------------:|:------:|:-----:|
| > -50 | MCS 9 (256-QAM 5/6) | 433 Mbps | 180-250 Mbps | <0.01% |
| -50 ~ -60 | MCS 7 (64-QAM 5/6) | 325 Mbps | 120-180 Mbps | <0.1% |
| -60 ~ -70 | MCS 5 (64-QAM 2/3) | 260 Mbps | 60-120 Mbps | 0.1-1% |
| -70 ~ -75 | MCS 3 (16-QAM 1/2) | 130 Mbps | 20-60 Mbps | 1-5% |
| -75 ~ -80 | MCS 1 (QPSK 1/2) | 65 Mbps | 5-20 Mbps | 5-15% |
| < -80 | MCS 0 (BPSK 1/2) | 29 Mbps | 1-10 Mbps | 15-40% |

### 2.2 AP 漫游 (BSS Transition) ⚠️ Catcher 完全未覆盖

```
       旧 AP                           新 AP
  ┌──────────────┐               ┌──────────────┐
  │ 信号减弱...   │  ──漫游决策──→ │ 信号增强...   │
  │              │   802.11r FT   │              │
  │ 1. 断开旧 AP │               │ 2. 认证新 AP  │
  │ 3. 重关联    │               │ 4. 密钥协商   │
  └──────────────┘               └──────────────┘
```

| 漫游方式 | 中断时间 | 标准 | Catcher 影响 |
|---------|:------:|------|-------------|
| 完整 802.1X 认证 (WPA2-Enterprise) | **500ms-3s** | 802.11r 之前 | **TCP 连接可能断开** |
| 802.11r FT (Fast BSS Transition) | **10-50ms** | 802.11r | TCP 重传可恢复 |
| 802.11k (邻居报告) + 802.11v (BSS 转换管理) | 配合 11r，减少扫描时间 | 802.11k/v | 减少切换延迟 |
| PSK 个人网络切换 | 50-200ms | — | 取决于实现 |
| Mesh WiFi 节点切换 | 0-50ms | 厂商专有 | 较低影响 |

### 2.3 DFS (动态频率选择) ⚠️ Catcher 完全未覆盖

5GHz WiFi 在检测到雷达信号时必须**立即静默并切换信道**。

```
DFS 事件序列：
  检测到雷达 → 信道静默（停止发送）→ 选择新信道（1-10s）→ CAC 等待（60s）→ 恢复正常
  
  中断总时间：1-10s + 60s CAC = 61-70s（如果是初始信道选择）
  运行中切换：中断 1-10s（信道切换时间）
```

对 Catcher 的影响：**长达 10 秒的完全静默**（所有连接 hang），Catcher 的 CB 必须在这段时间内正确响应。

### 2.4 同频/邻频干扰

| 干扰类型 | 表现 | 与独立丢包的区别 |
|---------|------|-----------------|
| 同频干扰 | 2+ AP 在同一信道 | **CSMA/CA 退避导致延迟增加**，丢包为突发型 |
| 邻频干扰 | 相邻信道重叠 | 类似噪声增加，丢包与信号相关 |
| 隐藏节点 | 两站互相不可见但都能与 AP 通信 | **碰撞集中在特定时刻**，RTS/CTS 可缓解 |
| 非 WiFi 干扰 | 微波炉、蓝牙、无线摄像头 | 2.4GHz 周期性干扰（60Hz 电源周期） |

### 2.5 电源管理模式

| 模式 | 行为 | 延迟影响 |
|------|------|:------:|
| CAM (持续活跃) | 始终监听 | 0ms |
| PS (Power Save) - 传统 | 客户端定期醒来取缓存包 | +50-200ms (取决于 Listen Interval) |
| UAPSD (WMM Power Save) | VoIP 优化，触发式唤醒 | +10-30ms |
| WiFi 6 TWT (Target Wake Time) | 精确调度唤醒时间 | 可配置，典型 +1-10ms |

### 2.6 MAC 层重试

WiFi MAC 层有自己的重传机制，这对理解"为什么应用层 retry 不用过于激进"很关键：

| 参数 | 默认值 | 说明 |
|------|:----:|------|
| Short Retry Limit | 7 | RTS/CTS 失败后的重试次数 |
| Long Retry Limit | 4 | 数据帧无 ACK 后的重试次数 |
| MAC 重试退避 | 指数 (32→1024 slot) | 类似 TCP 退避但更快 |

这意味着：如果一个包在 MAC 层经过 4 次重试（每次退避 32→64→128→256 slots ≈ 共 ~3ms @ 2.4GHz），仍然失败才会被丢弃，然后 TCP 层再检测到丢包。**TCP 感知到的丢包延迟 = MAC 重试延迟 + 排队延迟**，通常为数毫秒到数十毫秒。

---

## 三、测试案例设计

### 3.1 应新增的 Catcher Profile

| Profile | 损伤组合 | 验证特性 |
|---------|---------|---------|
| `wifi_weak_signal` | 延迟 5ms, jitter 3ms, 丢包 5% 突发 (burstLoss), 带宽 30KB/s | retry 在 MAC 重传掩蔽后仍能成功 |
| `wifi_interference` | 延迟 5ms, 周期性 burstLoss (50ms on / 200ms off), jitter 10ms | CB 不因周期性干扰误触发 |
| `wifi_bss_transition` | 100ms blackhole + 短暂(50ms) delay spike → 恢复正常 | 短暂中断不触发重连 |
| `wifi_dfs_switch` | 10s blackhole → 恢复 | CB 检测速度（类似 S12 黑洞场景） |
| `wifi_powersave` | 正常连接，但在每次请求前有 50ms 额外延迟 | keepAlive 间隔需适当 |

### 3.2 与现有 Profile 的对标

| Catcher Profile | WiFi 场景对标 | 准确性 |
|:---|------|:---:|
| `good` (RTT 50ms) | WiFi 6, 近 AP, 无干扰 | 🟢 |
| `metro` (RTT 100ms, 3% loss, 10% reset) | — | 🔴 地铁通常是蜂窝信号差，非 WiFi 特性 |
| `crossRegion` (RTT 300ms) | — | 🔴 跨地域不是 WiFi 场景 |

---

## 四、参考来源

- IEEE 802.11-2020 (统一标准)
- IEEE 802.11r-2008 (Fast BSS Transition)
- IEEE 802.11k-2008 (Radio Resource Measurement)
- IEEE 802.11v-2011 (Wireless Network Management)
- 3GPP TS 37.340 (LTE-WLAN Aggregation / LWA)
- Passpoint / Hotspot 2.0 (WiFi Alliance)
