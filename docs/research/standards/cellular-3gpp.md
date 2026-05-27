# 蜂窝网络标准溯源 — 3GPP 2G→5G

> 所有 Catcher Profile 的蜂窝网络参数须追溯至 3GPP TS 规范。本文建立完整参数链。

---

## 一、3GPP 规范索引

| 代 | 技术 | 3GPP 核心规范 | UE 射频 | UE 一致性测试 |
|:--|------|-------------|---------|:-----------:|
| 2G | GSM | TS 45.001 (物理层) | TS 45.005 | TS 51.010 |
| 2.5G | GPRS | TS 43.064 (GPRS 整体) | TS 45.005 | TS 51.010 |
| 2.75G | EDGE | TS 45.001 (8-PSK 调制) | TS 45.005 | TS 51.010 |
| 3G | UMTS (WCDMA) | TS 25.101 (UE 射频) | TS 25.101 | TS 34.121 |
| 3.5G | HSPA | TS 25.306 (UE 能力) | TS 25.101 | TS 34.121 |
| 3.75G | HSPA+ | TS 25.306 (64QAM/MIMO) | TS 25.101 | TS 34.121 |
| 4G | LTE | TS 36.101 (UE 射频) | TS 36.101 | **TS 36.521** |
| 4.5G | LTE-Advanced | TS 36.101 (CA/MIMO) | TS 36.101 | TS 36.521 |
| 5G NSA | NR (非独立) | TS 38.101-1 (FR1) | TS 38.101-1 | TS 38.521-1 |
| 5G SA | NR (独立) | TS 38.101-2 (FR2) | TS 38.101-2 | **TS 38.521-2** |

---

## 二、各代核心参数

### 2.1 延迟 (Latency)

| 代 | 3GPP 用户面目标 (单向) | 实际测量典型值 (单向) | 来源 |
|:--|:---------------------:|:------------------:|------|
| GSM (2G) | 未定义（电路交换） | 150-400ms | 测量研究 |
| GPRS (2.5G) | 未定义 | 300-600ms | 信道分配 + 编码 |
| EDGE (2.75G) | 未定义 | 150-300ms | 更快的编码 |
| UMTS (3G) | ~50-100ms | 70-200ms | TS 25.101 |
| HSPA (3.5G) | ~30-80ms | 50-120ms | TS 25.306 |
| HSPA+ (3.75G) | ~20-60ms | 40-80ms | MIMO 改善 |
| LTE (4G) | **10ms** (TS 36.913) | 15-50ms | 空闲→激活 50ms |
| LTE-A (4.5G) | **<5ms** (目标) | 10-30ms | CA + 高阶 MIMO |
| 5G NR NSA | **<4ms** (eMBB) | 5-20ms | 依赖 LTE 核心 |
| 5G NR SA | **<1ms** (URLLC) | 1-10ms | 独立核心网 + 边缘计算 |

### 2.2 带宽 (Throughput)

| 代 | 理论下行 (DL) | 理论上行 (UL) | 典型实测 DL | 典型实测 UL | 3GPP 定义 |
|:--|:-----------:|:-----------:|:---------:|:---------:|---------|
| GPRS | 85.6 kbps (4×CS-4) | 85.6 kbps | 30-50 kbps | 20-30 kbps | TS 43.064 |
| EDGE | 236.8 kbps (4×MCS-9) | 236.8 kbps | 100-180 kbps | 60-100 kbps | TS 45.001 |
| UMTS | 2 Mbps | 384 kbps | 300-800 kbps | 100-250 kbps | TS 25.101 |
| HSPA | 14.4 Mbps | 5.76 Mbps | 2-6 Mbps | 1-3 Mbps | TS 25.306 Cat 10 |
| HSPA+ | 42 Mbps (DC) | 11.5 Mbps | 8-20 Mbps | 3-6 Mbps | TS 25.306 Cat 20 |
| LTE | 150 Mbps (Cat 4) | 50 Mbps | 10-50 Mbps | 5-20 Mbps | TS 36.101 Cat 4 |
| LTE-A | 300-2000 Mbps (Cat 6+) | 50-300 Mbps | 30-150 Mbps | 10-50 Mbps | TS 36.101 Cat 6-16 |
| 5G NR (FR1) | 2.5 Gbps | 900 Mbps | 100-800 Mbps | 30-200 Mbps | TS 38.101-1 |
| 5G NR (FR2 mmWave) | 7 Gbps+ | 3 Gbps+ | 500-2000 Mbps | 100-500 Mbps | TS 38.101-2 |

### 2.3 丢包率 (BLER / Packet Loss)

| 代 | 3GPP BLER 目标 | 实际测量丢包率 | 丢包模式 |
|:--|:------------:|:-----------:|---------|
| GSM/GPRS/EDGE | 10% (BLER target) | 2-15% | 突发 (衰落相关) |
| UMTS | 10% (初始传输) | 1-8% | 突发 + HARQ 平滑 |
| HSPA | 10% (初传) / <1% (HARQ 后) | 0.5-5% | HARQ 掩蔽后低 |
| LTE | 10% (初传 BLER) | 0.1-3% | HARQ + ARQ 掩蔽 |
| 5G NR | 1-10% (eMBB) / 0.001% (URLLC) | 0.01-1% | 极低（URLLC 模式） |

**关键理解**：3GPP 将 BLER 目标设为 10% 以最大化频谱效率。这意味着在"正常"信号条件下，**10% 的数据块在首次传输时出错**，依赖 HARQ 重传纠正。这对 Catcher 的意义是——即使"正常"网络也存在大量瞬时错误，retry 机制必须在毫秒级而非秒级生效。

### 2.4 切换中断时间 (Handover Interruption Time)

这是 Catcher **最关键且当前完全未覆盖**的维度：

| 切换类型 | 3GPP 目标中断 | 实际测量 | Catcher 影响 |
|---------|:----------:|:------:|-------------|
| LTE 同频切换 | **~30-50ms** (TS 36.133) | 40-80ms | TCP 可无感恢复 |
| LTE 异频切换 | ~50-100ms | 80-150ms | 可能触发 1-2 次 TCP 重传 |
| LTE → 3G (IRAT) | **~500ms - 2s** | 1-3s | **TCP 用户超时可能性高** |
| LTE → 2G (IRAT) | ~1-5s | 3-10s | **几乎一定触发连接断开** |
| 5G NR 同频 | **0ms (make-before-break)** | 0ms | 无损 |
| 5G NR → LTE | ~30-60ms | 50-100ms | 正常 |
| WiFi → Cellular | **无标准，取决于 OS** | 1-5s | **IP 地址变更，连接全部断开** |

**对 Catcher 的建议**：
- 短暂中断 (< 200ms)：TCP 重传可恢复，应用层应等待不触发 reconnect
- 中长期中断 (200ms - 3s)：可能需要应用层 retry，不应触发 CB
- 长中断 (> 3s)：连接断开，应触发 reconnect + CB 半开探测

---

## 三、RRC 状态与连接建立

### 3.1 LTE RRC 状态转换时间

```
RRC_IDLE ──→ RRC_CONNECTED:  ~50ms (控制面建立)
RRC_CONNECTED (DRX) → 激活:  ~10-20ms (调度延迟)
RRC_CONNECTED → RRC_IDLE:    ~10-30s (不活动定时器，eNB 配置)
```

### 3.2 5G NR RRC 新增状态

```
RRC_INACTIVE ──→ RRC_CONNECTED:  ~10-20ms (比 LTE IDLE 快 3-5 倍)
RRC_INACTIVE:  保持核心网连接，释放无线资源
```

### 3.3 对 Catcher 的测试建议

| 场景 | 模拟方式 | 验证点 |
|------|---------|--------|
| LTE IDLE→CONNECTED (50ms 延迟突增) | proxy.ts: `spikeLatency` 在首次请求后触发 50ms | keepAlive 是否掩盖此延迟 |
| RRC CONNECTED→IDLE (10s 空闲断开) | proxy.ts: 10s `blackhole` → 恢复 | keepAlive 间隔是否低于 10s |
| 5G INACTIVE→CONNECTED (10ms) | 短暂 `blackhole` 10-20ms | 不应触发 retry |

---

## 四、3GPP UE 一致性测试标准

### 4.1 TS 36.521 (LTE 一致性测试) 关键测试条件

| 测试条件 | 参数 | 测试目的 |
|---------|------|---------|
| 正常条件 | -75 dBm (RSRP), SNR 15dB | 基准性能 |
| 弱信号 | -105 dBm, SNR -3dB | 边缘覆盖 |
| 极弱信号 | -115 dBm, SNR -8dB | 极限覆盖 |
| 高干扰 | -75 dBm + AWGN 干扰 | 干扰环境 |
| 衰落信道 | EPA5 / EVA70 / ETU300 | 多径效应 |
| 高速移动 | 300 km/h (多普勒 750Hz @ 2.6GHz) | 高铁场景 |

### 4.2 TS 38.521 (5G NR 一致性测试) 新增条件

| 条件 | 参数 | 测试目的 |
|------|------|---------|
| FR2 (mmWave) 波束切换 | 波束丢失 + 重选 | 高频特有 |
| 多 TRP (Multi-TRP) | 2+ 传输接收点 | 分布式 MIMO |
| URLLC 低延迟 | 0.5ms 单向目标 | 工业控制 |

---

## 五、测试案例汇总

### 5.1 已有 Profile 可覆盖的

| Catcher Profile | 对应 3GPP 场景 | 是否正确？ |
|:---|------|:---:|
| `gprs` (500ms RTT, 6.25KB/s, 2% loss) | GPRS CS-2 典型实测 | ⚠️ RTT 偏大，带宽偏小 |
| `2g_regular` (300ms RTT, 31.25KB/s, 1% loss) | EDGE MCS-5 正常条件 | 🟢 合理 |
| `3g_slow` (200ms RTT, 97.5KB/s, 0.5% loss) | UMTS/HSPA 边缘覆盖 | 🟢 合理 |
| `4g_lte` (20ms RTT, 500KB/s, 0% loss) | LTE 近基站、信号好 | 🟢 合理 |
| `mobile3g` (2000ms RTT) | — | 🔴 任何蜂窝不会达到 2000ms 单向 |

### 5.2 应新增的 Profile

| 新 Profile | 对应场景 | 参数 |
|-----------|---------|------|
| `5g_sa` | 5G NR SA (eMBB) | RTT=5ms, DL=200MB/s, loss=0.01% |
| `5g_urllc` | 5G NR SA (URLLC) | RTT=1ms, DL=50MB/s, loss=0.001% |
| `lte_weak` | LTE 小区边缘 (-115dBm) | RTT=50ms, DL=2MB/s, loss=5%, jitter=20ms |
| `lte_highspeed` | LTE 高铁场景 (300km/h) | RTT=50ms, 周期性衰落 loss=0→15%→0 |
| `irat_4g_to_3g` | LTE→3G IRAT 切换 | 2s blackhole → 恢复 (200ms RTT) |
| `irat_4g_to_2g` | LTE→2G IRAT 切换 | 5s blackhole → 恢复 (500ms RTT) |
