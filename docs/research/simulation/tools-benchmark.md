# 业界网络模拟工具 — 损伤模型完整对标

> 目标：catcher 的 proxy.ts 每个损伤参数都能对标到至少一个业界工具的对应能力。

---

## 一、Linux tc netem — 损伤模型黄金标准

### 1.1 DELAY（延迟）

```
tc qdisc add dev eth0 root netem delay TIME [JITTER [CORRELATION]]

TIME:         固定延迟 ms
JITTER:       ± 随机延迟 ms
CORRELATION:  当前延迟与前一个的相关性 %（25% = 25% 基于前一个值）

distribution: uniform | normal | pareto | paretonormal
  uniform:      等概率分布
  normal:       高斯（正态）分布
  pareto:       长尾分布（模拟少见的极大延迟）
  paretonormal: Pareto + normal 混合
```

| 参数 | Catcher proxy.ts | 对标状态 |
|------|:--------------:|:------:|
| 固定延迟 | ✅ `latency` | 🟢 |
| uniform jitter | ✅ `jitter` + `jitterDistribution: 'uniform'` | 🟢 |
| normal jitter | ✅ `jitterDistribution: 'normal'` | 🟢 |
| pareto jitter | 🔴 缺失 | 长尾延迟场景无法模拟 |
| paretonormal jitter | 🔴 缺失 | — |
| **延迟相关性 (correlation)** | 🔴 缺失 | **关键差距**：真实网络延迟不是独立的 |

### 1.2 LOSS（丢包）

```
tc qdisc add dev eth0 root netem loss MODEL

MODEL:
  random PERCENT [CORRELATION]
    → 独立丢包 + 可加相关
    
  state P13 [P31 [P32 [P23 P14]]]
    → 4-state Markov 丢包模型
    State 0: 好接收（无丢包）
    State 1: 好接收 + 在突发内
    State 2: 突发丢包内
    State 3: 独立丢包
    
  gemodel PERCENT [R [1-H [1-K]]]
    → Gilbert-Elliott 模型
    PERCENT: 进入坏状态的概率
    R:       退出坏状态的概率
    1-H:     坏状态内的丢包概率
    1-K:     好状态内的丢包概率
    
  [ecn]: 用 ECN 标记代替丢弃
```

| 参数 | Catcher proxy.ts | 对标状态 |
|------|:--------------:|:------:|
| random % 丢包 | ✅ `packetLoss` | 🟢 |
| 丢包相关性 | 🔴 缺失 | 虽是 netem 的简化版，但有价值 |
| **4-state Markov** | 🔴 仅 2-state (Gilbert-Elliott) | 4-state 能区分"轻度突发"和"重度突发" |
| Gilbert-Elliott | ✅ `burstLoss` | 🟢 参数命名不同但语义对应 |
| ECN 标记 | 🔴 缺失 | 低优先级 |

### 1.3 CORRUPT（损坏）

```
tc qdisc add dev eth0 root netem corrupt PERCENT [CORRELATION]

→ 在随机位置修改 1 bit
```

| Catcher proxy.ts | 对标 |
|:--------------:|:--:|
| ✅ `corrupt` | 🟢 |

### 1.4 DUPLICATE（重复）

```
tc qdisc add dev eth0 root netem duplicate PERCENT [CORRELATION]
```

| Catcher proxy.ts | 对标 |
|:--------------:|:--:|
| ✅ `duplicate` | 🟢 |

### 1.5 REORDER（乱序）

```
tc qdisc add dev eth0 root netem reorder PERCENT [CORRELATION] [gap DISTANCE]

两种方式：
  gap DISTANCE: 每第 N 个包立即发出，前面的延迟
  PERCENT + CORRELATION: 概率性乱序
```

| Catcher proxy.ts | 对标 |
|:--------------:|:--:|
| ✅ `reorder` (probability + delay) | 🟡 仅概率方式，缺 gap 方式 |

### 1.6 RATE（带宽）

```
tc qdisc add dev eth0 root netem rate RATE [PACKETOVERHEAD [CELLSIZE [CELLOVERHEAD]]]

RATE:           比特率（如 1mbit、100kbit）
PACKETOVERHEAD: 每包额外字节（如 -14 模拟剥离以太网头）
CELLSIZE:       ATM 信元大小模拟
CELLOVERHEAD:   每信元额外开销
```

| Catcher proxy.ts | 对标 |
|:--------------:|:--:|
| ✅ `bandwidth` (bytes/s) | 🟡 缺 packetoverhead/cellsize |

### 1.7 SLOT（时隙）

```
tc qdisc add dev eth0 root netem slot MIN_DELAY [MAX_DELAY]
  [packets PACKETS] [bytes BYTES]

→ 模拟 TDMA / 时隙网络（DOCSIS, WiFi MAC, LTE）
```

| Catcher proxy.ts | 对标 |
|:--------------:|:--:|
| 🔴 缺失 | 可模拟 MAC 层时隙效应 |

### 1.8 关键限制

| 限制 | 说明 |
|------|------|
| 时钟粒度 | 受内核 HZ 影响（典型 250Hz=4ms），rate/delay 可能有抖动 |
| 单流限制 | 不会跨流重新排序 |
| TSQ 影响 | 对 TCP 测试要放在接收端 ingress |
| 与其他 qdisc 组合 | 可能不稳定 |

---

## 二、其他业界工具对标

### 2.1 ns-3 Network Simulator

| 能力 | 说明 |
|------|------|
| Error Model | RateErrorModel + BurstErrorModel (Markov) + ListErrorModel |
| WiFi 信道 | 完整 802.11a/b/g/n/ac/ax MAC + PHY，包括衰落、干扰 |
| LTE LENA | 完整 eNB + UE 协议栈，RRC/RLC/MAC |
| 5G NR | 3GPP TR 38.901 信道模型 (CDL/TDL) |
| 与 proxy.ts 对比 | ns-3 是全栈仿真，proxy.ts 是应用层——**不可直接对比但模型参数可参考** |

### 2.2 MahiMahi (MIT)

| 能力 | 说明 |
|------|------|
| RecordShell | 录制真实网络 trace（tcpdump + 时间戳） |
| ReplayShell | 精确回放 trace（packet-level timing） |
| DelayShell | 固定延迟管道（单向） |
| LinkShell | 可配置上行/下行链路 |
| LossShell | 独立丢包 |

**MahiMahi 的关键特性（proxy.ts 完全缺失）**：真实 trace 回放。录制 10 秒的真实 4G trace，就精确获得那 10 秒的延迟/丢包/带宽动态。

### 2.3 toxiproxy (Shopify)

| Toxic 类型 | 功能 | Catcher proxy.ts |
|-----------|------|:--------------:|
| `latency` | 添加延迟 + jitter | ✅ |
| `bandwidth` | 带宽限速 | ✅ |
| `timeout` | 静默断开连接 | ✅ (blackhole) |
| `slicer` | 将数据切片（模拟 MTU 或乱序） | 🔴 |
| `slow_close` | 延迟 TCP close | 🔴 |
| `reset_peer` | 发送 TCP RST | ⚠️ (destroy 近似) |
| API 驱动 | REST API 动态改变 toxic | ✅ (setConditions) |

### 2.4 Comcast (Go)

```
comcast --device=eth0 --latency=250 --target-bw=1000 --packet-loss=10%
```

简化的 tc netem wrapper：
- `--latency`: 延迟 ms（单向）
- `--target-bw`: 带宽 kbps
- `--packet-loss`: 丢包 %
- `--dry-run`: 仅打印命令

### 2.5 Clumsy (Windows)

基于 WinDivert 驱动拦截包：

| 功能 | 说明 |
|------|------|
| Lag | 延迟 |
| Drop | 随机丢包 |
| Throttle | 带宽限速 |
| Duplicate | 包重复 |
| Out of order | 乱序 |
| Tamper | 包篡改 |

---

## 三、ITU-T 损伤标准的 Catcher 映射

### 3.1 G.114 — 单向延迟阈值

| 范围 | 分类 | Catcher Profile 对应 |
|------|------|---------------------|
| **0-150ms** | 可接受（大多数应用） | `good`, `4g_lte`, `3g_good`, `dsl` |
| **150-400ms** | 用户可感知但可接受 | `3g_slow`, `crossRegion`, `satellite` |
| **>400ms** | 不可接受（交互式应用） | `gprs`, `2g_regular`, `veryWeak`, `mobile3g` |

### 3.2 ITU-T G.109 丢包分类

| 范围 | 分类 | Catcher Profile |
|------|------|:---|
| **0-3%** | 好 | `good`, `3g_good`, `4g_lte` |
| **3-15%** | 中等 | `weak`, `metro`, `mobile3g`, `2g_regular` |
| **>15%** | 差 | `veryWeak`, `burst_storm` |

### 3.3 RFC 3393 — IPDV (IP 包延迟变化 / Jitter)

```
IPDV = 第 i 个包的延迟 - 第 j 个包的延迟（选定的包对）

关键统计：
  - 百分位数 (p50, p95, p99)
  - 峰值 (peak-to-peak)
  - 平均 IPDV
```

**Catcher 对标**：`jitter` 设为 ±X ms (uniform)，未实现 IPDV 百分位数报告。

---

## 四、测试方法标准

### 4.1 RFC 2544 — 网络互联设备基准测试

| 测试项 | Catcher 对标 |
|--------|:-----------:|
| 吞吐量 | `benchmark/throughput.test.ts` |
| 延迟 | Harness 中的 p50/p95/p99 |
| 丢帧率 | — |
| 背靠背 | — |
| 系统恢复 | `extreme-scenarios.test.ts` S12 (黑洞恢复) |
| 重置 | — |

### 4.2 3GPP UE 一致性测试 — 衰落信道模型

| 模型 | 描述 | 对应场景 |
|------|------|---------|
| EPA 5Hz | 扩展步行 A，5Hz 多普勒 | 静止/步行 |
| EVA 70Hz | 扩展车辆 A，70Hz | 车辆 (~85km/h @ 2.6GHz) |
| ETU 300Hz | 扩展典型城市，300Hz | **高铁 (~360km/h)** |

---

## 五、proxy.ts 增强路线图

### 5.1 立即补强（对标 netem）

| 增强 | 对标 | 优先级 |
|------|------|:----:|
| `jitter` 加 `correlation` 参数 | netem delay CORRELATION | 🔴 |
| `jitterDistribution: 'pareto'` | netem distribution pareto | 🟡 |
| `burstLoss` 升级为 4-state | netem loss state | 🟡 |
| `slot` 模式 (TDMA 模拟) | netem slot | 🟢 |
| 丢包加 `correlation` 参数 | netem loss random CORRELATION | 🟡 |
| `slicer` (包切片/重组) | toxiproxy slicer | 🟢 |

### 5.2 中期补强（差异化能力）

| 增强 | 来源 | 优先级 |
|------|------|:----:|
| **真实 trace 回放** | MahiMahi | 🟡 |
| **周期性损伤模式** | ns-3 衰落模型 | 🟢 |
| **per-flow 独立损伤** | 组合测试 | 🟢 |
| **aggregate 带宽** | tc netem htb | 🟢 |
