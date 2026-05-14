# 01 — 网络损伤代理

> 代码位置：`packages/test/network/proxy.ts`
> 参考：[../research/test-strategy-gaps.md](../research/test-strategy-gaps.md)

---

## 架构

```
客户端 ←→ NetworkProxy ←→ 目标服务
           │
    每 chunk 经过以下损伤管道：
    
    data chunk
      │
      ├── [blackhole]  静默丢弃？→ return
      ├── [packetLoss] 独立随机丢包？
      ├── [burstLoss]  Gilbert-Elliott 突发丢包？
      ├── [corrupt]    包损坏？
      ├── [reorder]    乱序重排？
      ├── [duplicate]  包重复？
      ├── [latency]    延迟 + jitter
      ├── [bandwidth]  带宽限速
      └── write to dest
```

---

## 损伤模型

### 路由黑洞 (blackhole)

全部数据包静默丢弃，不发 RST。模拟上游路由器/光猫故障。

```
配置：
  blackhole.enabled          — 是否启用
  blackhole.duration         — 持续时间 ms（0 = 手动关闭）
  blackhole.startAfter       — 延迟启动 ms
  blackhole.destroyOnRecover — 恢复后是否销毁僵尸连接

实现（在 data handler 最前面）：
  if (conditions.blackhole?.enabled) return
```

### 延迟抖动 (jitter)

在基础延迟上叠加随机波动。

```
配置：
  latency: 1000        — 基础一程延迟 ms
  jitter: 250          — ±250ms 均匀分布
  jitterDistribution: 'uniform' | 'normal'

计算：
  actualDelay = latency + (Math.random() * 2 - 1) * jitter
  clamp(actualDelay, 0, +∞)
```

### 延迟尖刺 (spike)

在正常延迟基础上，以低概率触发极端高延迟尖刺。模拟偶发网络拥塞或路由抖动。

```
配置：
  spikeLatency: 2000       — 尖刺延迟 ms
  spikeProbability: 0.01   — 每 chunk 触发尖刺的概率（0-1）

计算：
  if (Math.random() < spikeProbability) {
    actualDelay = spikeLatency
  } else {
    actualDelay = latency + jitter  // 正常延迟
  }
```

### 突发丢包 (burstLoss)

Gilbert-Elliott 两状态马尔可夫模型。网络在"好状态"和"坏状态"之间切换。

```
状态转移：
       p_gg              p_bb
    ┌────────┐         ┌────────┐
    │  GOOD  │ ──p_gb→ │  BAD   │
    │ 低丢包  │ ←─p_bg─ │ 高丢包  │
    └────────┘         └────────┘

配置：
  burstLoss.p_good_to_bad   — GOOD → BAD 转移概率（典型 0.01-0.05）
  burstLoss.p_bad_to_good   — BAD → GOOD 转移概率（典型 0.1-0.3）
  burstLoss.loss_good       — GOOD 状态丢包率（典型 0-0.02）
  burstLoss.loss_bad        — BAD 状态丢包率（典型 0.3-0.8）
  burstLoss.minBadDuration  — 坏状态最短持续 ms（0 = 无限制）

每 chunk 时：
  1. 先根据转移概率判断是否切换状态（坏状态未满 minBadDuration 时不允许切回好状态）
  2. 再根据当前状态的丢包率判断是否丢弃
```

### 上下行不对称 (upload / download)

不同方向使用不同的损伤参数。

```
配置（向下兼容对称参数）：
  conditions.latency = 100           ← 对称（upload/download 都未设时用）
  conditions.upload.latency = 500    ← 上行专用
  conditions.download.latency = 50   ← 下行专用

createThrottledPipe 根据方向选择参数：
  clientSocket → targetSocket  使用 upload 参数
  targetSocket → clientSocket  使用 download 参数
```

### 带宽波动 (bandwidth)

周期性随机调整带宽上限。

```
配置：
  bandwidth: 100_000            — 基础带宽 bytes/s
  bandwidthFluctuation: 0.5     — 波动幅度（0-1）

每 1-3 秒：
  currentBw = bandwidth * (1 + (Math.random() * 2 - 1) * bandwidthFluctuation)
  clamp(currentBw, bandwidth * 0.1, bandwidth * 2)
```

### 包损坏 / 乱序 / 重复 (corrupt / reorder / duplicate)

对标 Linux tc netem 的完整能力。

```
corrupt:
  每 chunk 以 P% 概率随机修改其中 1 个字节

reorder:
  每 chunk 以 P% 概率延迟 N ms（排在后面的包之前发送）

duplicate:
  每 chunk 以 P% 概率发送两次
```

---

## 完整 NetworkConditions 接口

```typescript
interface NetworkConditions {
  // ── 对称损伤（向下兼容）──
  latency?: number
  jitter?: number
  jitterDistribution?: 'uniform' | 'normal'
  spikeLatency?: number                // 尖刺延迟 ms
  spikeProbability?: number            // 尖刺触发概率 0-1
  packetLoss?: number
  bandwidth?: number
  bandwidthFluctuation?: number     // 0-1
  connectionReset?: number
  corrupt?: number                  // 0-1
  reorder?: { probability: number; delay: number }
  duplicate?: number                // 0-1

  // ── 突发丢包（与 packetLoss 互斥或叠加）──
  burstLoss?: {
    p_good_to_bad: number
    p_bad_to_good: number
    loss_good: number
    loss_bad: number
    minBadDuration?: number          // 坏状态最短持续 ms，0 = 无限制
  }

  // ── 路由黑洞 ──
  blackhole?: {
    enabled: boolean
    duration?: number               // 0 = 手动关闭
    startAfter?: number
    destroyOnRecover?: boolean
  }

  // ── 上下行不对称（设了则覆盖对称参数）──
  upload?: DirectionConditions
  download?: DirectionConditions
}

interface DirectionConditions {
  latency?: number
  jitter?: number
  jitterDistribution?: 'uniform' | 'normal'
  spikeLatency?: number
  spikeProbability?: number
  packetLoss?: number
  bandwidth?: number
  bandwidthFluctuation?: number
  burstLoss?: BurstLossConfig
}
```

---

## 实现注意事项

1. **动态读取** — 所有条件在每 chunk 时从 `conditions` 对象动态读取，`setConditions()` 对活跃连接立即生效
2. **节流** — 带宽限制使用 100ms 滑动窗口，避免微突发
3. **随机种子** — `Math.random()` 即可，测试不需要可重现的随机（如果需要，用 `seedrandom`）
4. **连接追踪** — `activeSockets` Set 追踪所有活跃连接，`disruptAll()` 和 `stop()` 负责清理
5. **zero-copy 优先** — 不复制 chunk，直接转发（添加延迟除外）
