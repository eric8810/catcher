# 探索性调研报告 — 业界网络测试与韧性方法论

> **这不是一份"定义好的框架"。这是探索过程的记录——我们去看了谁在做什么，发现了什么，哪些出乎意料。**
> 调研日期：2026-07
> 方法：WebSearch + WebFetch 跨行业探索，不预设维度

---

## 一、探索路径

```
学术界 survey → 游戏行业 (引擎内置 + 独立方法论) → 混沌工程 (Netflix) → 
电信设备厂商 (Keysight/Spirent) → 大型互联网 (Google SRE, Cloudflare) → 
其他高要求行业 (金融/IoT) → 综合对比
```

---

## 二、出乎意料的发现

### 发现 1：游戏引擎已经内置了网络测试标准

**这是最大的意外。** 我们以为需要自己去定义"测试哪些网络条件"，但 Unreal Engine 和 Unity 已经做了：

#### Unreal Engine 的 Network Emulation 设置（直接对标 Catcher proxy.ts）

| Unreal 设置 | 含义 | Catcher proxy.ts 对应 |
|------------|------|---------------------|
| `PktLoss` | 发出包丢包率 % | `packetLoss` ✅ |
| `PktIncomingLoss` | 接收包丢包率 % | 🔴 缺失（upload/download 未分 loss） |
| `PktLag` | 发出延迟 ms | `latency` ✅ |
| `PktLagVariance` | 延迟变化 (±ms) | `jitter` ✅ |
| `PktLagMin` / `PktLagMax` | 延迟范围 [min, max] | 🔴 Catcher 仅支持 ±jitter 对称 |
| `PktIncomingLagMin/Max` | 接收方向延迟范围 | 🔴 缺失（对称假设） |
| `PktJitter` | 抖动叠加 | `jitter` ✅ |
| `PktOrder` | 随机乱序 | `reorder` ✅ |
| `PktDup` | 包重复 | `duplicate` ✅ |
| `PktEmulationProfile` | 预设：**Average / Bad** | `presets.ts` ✅ 但 Unreal 只有 2 个预设（够用主义） |

**关键洞察**：Unreal 的预设只有 **2 个**（Average + Bad）。而 Catcher 有 14 个。游戏行业的哲学是**少而精**——不需要 14 个 Profile，2 个极端的就够了。

#### Unity Netcode for GameObjects 的测试指南

| 平台 | 推荐测试延迟 | 推荐丢包率 |
|------|:--------:|:--------:|
| **桌面** | 100-150ms | 5-10% |
| **移动端** | 200-300ms | 5-10% |

**关键洞察**：Unity 按**平台**而非按**网络技术**来分类测试条件。这和我们按 3GPP/WiFi 分类完全不同。

#### Epic Games 的极端测试建议

> "Test at 500ms round trip ping, 10% packet loss or higher."
> — Epic Games, Unreal Engine Documentation

**500ms RTT + 10%+ 丢包**是 Epic 的"你应该测试的最坏条件"。这比 Catcher 的 `veryWeak` (4000ms RTT) 要克制得多——游戏行业知道超过 500ms 就没意义了，玩家已经退出了。

---

### 发现 2：不同游戏类型的延迟容忍度差异极大

这不是我们已知的——我们知道了具体的学术测量数值：

| 游戏类型 | 延迟容忍上限 | 来源 |
|---------|:--------:|------|
| 竞技 FPS (CS:GO, Valorant) | **100-125ms** | ACM CHI 2021 研究 (43 人实验) |
| FPS (Quake 3) | **150-180ms** | Armitage 等 |
| 格斗游戏 | **~66ms** (4 帧 @ 60fps) | 社区共识 |
| MOBA (LoL/Dota 2) | **~80ms** (Riot 目标: >80% 玩家 <80ms) | Riot Direct |
| RTS | **最高 1000ms** | ResearchGate 研究 |
| 休闲/回合制 | 数秒可接受 | — |

**这对 Catcher 的意义**：不同应用类型的"网络韧性"定义完全不同。一个 FPS 游戏的 retry 策略和一个 REST API 的 retry 策略应该完全不同。Catcher 作为通用库，需要提供**分类配置**。

---

### 发现 3：混沌工程有清晰的方法论，但应用层注入严重不足

来自 arXiv 2025 论文 *"Chaos Engineering in the Wild: Findings from GitHub"*（971 个 repo 分析）：

| 故障注入类型 | 占比 |
|------------|:---:|
| **网络故障** | **40.9%** |
| 实例终止 | 32.7% |
| 资源压力 | 23.4% |
| **应用层故障** | **3.0%** |

**关键洞察**：在混沌工程实践中，**网络是最主要的故障注入目标**（40.9%），但**应用层故障注入严重不足**（仅 3%）。这意味着——Catcher 作为应用层的韧性库，填补的正是这个 3% 的空白。

最常用的混沌工程工具：**Toxiproxy** (Shopify) 和 **Chaos Mesh** (CNCF)。

---

### 发现 4：Google SRE 有一套完善的测试分级

来自 *Google SRE Workbook Chapter 17 — Testing for Reliability*：

```
单元测试 → 集成测试 → 系统测试
                          ├─ Smoke tests (最简单但关键)
                          ├─ Performance tests (性能不退化)
                          ├─ Regression tests (旧 bug 不回归)
                          └─ (更多变体)

生产测试 ← 在 live 系统上验证
```

**关键概念**：**Zero MTTR bugs** ——在 push 阶段就被测试拦截的 bug，修复成本为零。

---

### 发现 5：不同行业的"韧性"定义是冲突的

| 行业 | 核心关切 | 延迟阈值 | 丢包容忍 | Catcher Profile 对标 |
|------|---------|:------:|:------:|---------------------|
| **竞技 FPS 游戏** | 延迟（宁可丢包） | <100ms | 可容忍 (UDP unreliable) | 无对应（Catcher 为 TCP 设计） |
| **Web API** | 可靠性（宁可慢） | <30s | 不可容忍 (retry) | `good`/`weak` 系列 |
| **金融交易** | 延迟 + 可靠性 | **<1μs** (HFT) | 不可容忍 (TCP) | 完全超出范围 |
| **IoT/LPWAN** | 省电 + 远距 | 数秒到数分钟 | 极高 (30%+) | 无对应 |
| **流媒体** | 带宽 + 抖动 | 数秒缓冲 | 可容忍 (buffer) | 无对应 |
| **XR/元宇宙** | 延迟 + 带宽 + 抖动 | **<20ms** (ITU-T P.1321) | 不可容忍 | 无对应 |
| **卫星互联网** | 高延迟 + 抖动 | 25-65ms (LEO) | 0.5-5% | `satellite` ✅ |

**核心洞察**：Catcher 不能用一个 Profile 体系服务所有行业。需要按应用类型分类。

---

### 发现 6：Unreal + Unity 的做法颠覆了我们的 Profile 思路

我们的 14 个 Profile 按**网络技术**分类（gprs, 4g_lte, wifi_weak...）。

游戏行业的做法是按**使用场景**分类：

```
Unity 的分类法：
  桌面 (100-150ms, 5-10% loss)
  移动端 (200-300ms, 5-10% loss)

Unreal 的分类法：
  Average (典型互联网条件)
  Bad (极端条件：500ms RTT, 10% loss)

Godot/Little Brats 的测试法：
  不预设 profile——用 tc 命令动态调节，在游戏过程中改变条件
```

**我们应该同时支持两种分类法**：技术型（给懂网络的开发者）和场景型（给"我就想测弱网"的开发者）。

---

## 三、各行业方法论对比

### 3.1 游戏行业

| 特征 | 说明 |
|------|------|
| **测试哲学** | 极端条件 + 动态变化（游戏过程中变条件） |
| **工具** | 引擎内置 (Unreal Network Emulation) + OS 工具 (tc/Clumsy/dummynet) |
| **指标** | 延迟容忍度（按游戏类型）、包乱序影响、预测/回滚正确性 |
| **独特之处** | **不只关心"请求成功与否"，还关心"状态同步是否正确"** |
| **Catcher 可借鉴** | 引擎内置测试功能、PktIncomingLoss 分离、min/max 延迟范围、预设少而精 |

### 3.2 混沌工程

| 特征 | 说明 |
|------|------|
| **测试哲学** | 生产环境注入故障，验证系统不崩溃 |
| **工具** | Toxiproxy (40.9% 网络故障), Chaos Mesh (K8s), LitmusChaos |
| **指标** | MTTR, MTBF, 错误预算消耗 |
| **独特之处** | **在生产环境做**（不是 lab） |
| **Catcher 可借鉴** | 故障注入分类学（网络/实例/资源/应用四类）、渐进式混沌实验设计 |

### 3.3 Google SRE

| 特征 | 说明 |
|------|------|
| **测试哲学** | "如果你没测试过，它就是坏的。" |
| **工具** | 内部系统（超越单元/集成/系统测试，有生产测试） |
| **指标** | Zero MTTR bugs, 测试覆盖率 → 可靠性预测 |
| **独特之处** | **把测试和 MTBF 建立了数学关系** |
| **Catcher 可借鉴** | 测试分级、SLO 定义方法 |

### 3.4 电信设备测试 (Keysight/Spirent)

| 特征 | 说明 |
|------|------|
| **测试哲学** | 确定性、可重复、全参数控制 |
| **工具** | 硬件网络仿真器（$10k-$500k 级别） |
| **指标** | RFC 2544 (吞吐/延迟/丢帧/背靠背/恢复) |
| **独特之处** | **确定性损伤注入**（时钟精度、可重现性） |
| **Catcher 可借鉴** | 损伤参数完整度对标、确定性模式（seed-based reproducibility） |

### 3.5 ITU-T / ETSI

| 特征 | 说明 |
|------|------|
| **测试哲学** | 标准化 + 互操作性 |
| **关键标准** | G.114 (延迟阈值), G.109 (丢包分类), P.1321 (XR 交互测试, 2025 年新), TR 103 702 (QoS) |
| **独特之处** | 有统一框架定义什么算"好"什么算"坏" |
| **Catcher 可借鉴** | G.114/G.109 直接映射到 Profile |

---

## 四、对 Catcher 框架的关键修正

### 修正 1：从"按技术分类"到"技术 + 场景双分类"

```
当前：                       应改为：
gprs                        技术标签: gprs, 场景标签: mobile_extreme
4g_lte                      技术标签: 4g_lte, 场景标签: mobile_good
wifi_weak_signal            技术标签: wifi_ac, 场景标签: wifi_poor
```

### 修正 2：预设数量——"少而精"vs"分类细致"

- Unreal: **2 个预设** (Average, Bad)
- Unity: **2 类** (Desktop, Mobile)
- Catcher: **14 个预设**

**结论**：保留 14 个技术型 Profile（给高级用户），但**新增 ~4 个场景型预设**（给"就想测弱网"的用户）。

### 修正 3：增加动态条件变化

游戏行业和 Little Brats 都在测试过程中动态改变网络条件。Catcher 的 `setConditions()` 已经支持，但缺少对应的测试场景。应新增：

```
S26: 渐变劣化 — 每 5s 增加 50ms 延迟 + 1% 丢包
S27: 突变恢复 — 正常 → 500ms/10% loss (5s) → 恢复
S28: 周期性波动 — 正弦波延迟变化
```

### 修正 4：协议分层缺 PktIncomingLoss

Unreal 的 `PktIncomingLoss` 和 `PktIncomingLagMin/Max`（接收方向单独设置）是 Catcher 的 `upload`/`download` 子配置中缺失的。

### 修正 5：游戏行业不关心带宽

游戏行业的所有测试配置中**都不包含带宽限制**——游戏流量本身就小（kbps 级别），带宽不是瓶颈。Catcher 的 `bandwidth` 参数对游戏场景是无意义的。

---

## 五、探索出的关键数字基准

### 各行业延迟容忍度一览

```
0                         100ms                    500ms              1s              30s
|──────┬─────────────────────|───────────────────────|─────────────────|────────────────|
      HFT                FPS/格斗             MOBA/RTS         Web API           IoT/LPWAN
      <1μs                <100-180ms           <80-1000ms       <30s              数分钟
      (金融)              (竞技游戏)            (在线游戏)       (REST)            (传感器)
```

### 各行业丢包容忍度一览

```
0%          1%           5%           10%          15%          30%+
|───────────|────────────|────────────|────────────|────────────|─────
 TCP/HTTP   4G/LTE      3G/HSPA     WiFi边缘     GPRS边缘    LoRa/LPWAN
 (不丢包)    (好信号)     (中信号)    (弱信号)     (极弱)      (IoT容忍)
```

---

## 六、参考资料索引

| 来源 | 类型 | 关键内容 |
|------|------|---------|
| [Unreal Engine Network Emulation](https://dev.epicgames.com/documentation/unreal-engine/using-network-emulation-in-unreal-engine) | 引擎文档 | PktLoss/PktLag/PktJitter 参数体系, Average/Bad 预设, 500ms+10% 建议 |
| [Unity Netcode Testing Guide](https://docs.unity3d.com/Packages/com.unity.netcode.gameobjects@2.6/manual/tutorials/testing/testing_with_artificial_conditions.html) | 引擎文档 | 桌面 100-150ms vs 移动 200-300ms, Clumsy/dummynet 推荐 |
| [Google SRE Book Ch.17](https://sre.google/sre-book/testing-reliability/) | 书籍 | 测试分层, Zero MTTR bugs, 测试与 MTBF 关系 |
| [Chaos Engineering in the Wild (arXiv 2025)](https://arxiv.org/html/2505.13654v1) | 学术论文 | 971 repo 分析, 40.9% 网络故障注入, Toxiproxy/Chaos Mesh 最流行 |
| [ACM CHI 2021 — Latency in Competitive FPS](https://dl.acm.org/doi/fullHtml/10.1145/3411764.3445245) | 学术论文 | 43 人实验, 竞技 FPS <125ms 阈值 |
| [Riot Games — VALORANT Netcode](https://www.riotgames.com/en/news/peeking-valorants-netcode) | 行业博客 | 128-tick, 40ms baseline reduction |
| [Riot Direct](https://www.riotgames.com/en/news/leveling-networking-multi-game-future) | 行业博客 | 自建骨干网, >80% 玩家 <80ms |
| [Little Brats Blog — Netcode Testing](https://studios.ptilouk.net/little-brats/blog/2024-10-23_netcode.html) | 独立开发者 | tc 工具实测, 动态条件变化脚本, reliable vs unreliable 对比 |
| [ITU-T P.1321 (2025)](https://www.itu.int/rec/T-REC-P.1321-202510-P) | 标准 | XR 通信交互测试方法 |
| [Packetstorm — Starlink 2026 Analysis](https://packetstorm.com/starlink-satellite-internet-in-2026-bandwidth-latency-and-packet-loss-analyzed/) | 行业分析 | Starlink 25-50ms 中位延迟, 99th <65ms |
| Keysight Network Emulators Catalog | 产品目录 | 确定性损伤平台参数 |
