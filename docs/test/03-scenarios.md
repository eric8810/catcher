# 03 — 测试场景体系

> 代码位置：`packages/test/e2e/scenarios.test.ts` (S1-S8)、`packages/test/chaos/extreme-scenarios.test.ts` (S9-S16)
> 参考：[../research/test-strategy-gaps.md](../research/test-strategy-gaps.md)

---

## 场景索引

### 现有场景 (S1-S8)

| 编号 | 场景 | 损伤 | 验证特性 | 迭代 | 文件 |
|------|------|------|---------|------|------|
| S1 | 冷启动 → 登录 | keepAlive | Agent 连接复用 | 100 | scenarios.test.ts |
| S2 | 发送消息 | 丢包+重置 | retry | 100 | scenarios.test.ts |
| S3 | 频道切换 | 高 RTT | keepAlive + retry | 100 | scenarios.test.ts |
| S4 | 跨地域用户 | 中等 RTT | keepAlive | 100 | scenarios.test.ts |
| S5 | 大 payload | 低带宽 | msgpack vs JSON | 100 | scenarios.test.ts |
| S6 | WS 高频消息 | 丢包 | deflate + codec | 100 | scenarios.test.ts |
| S7 | 并发优先级 | 无损伤 | 优先级队列 | 100 | scenarios.test.ts |
| S8 | DNS 缓存 | 无损伤 | DNS cache | 100 | scenarios.test.ts |

### 新增极端场景 (S9-S16)

| 编号 | 场景 | 损伤 | 验证特性 | 迭代 | 文件 |
|------|------|------|---------|------|------|
| S9 | GPRS 极端弱网 | GPRS profile | retry 极限 | 30 | extreme-scenarios.test.ts |
| S10 | 突发丢包风暴 | Gilbert-Elliott 30s | CB + retry | 30 | extreme-scenarios.test.ts |
| S11 | 上下行不对称 | 2G asymmetrical | POST vs GET | 30 | extreme-scenarios.test.ts |
| S12a | 路由黑洞 30s | blackhole 30s | CB 检测速度 | 30 | extreme-scenarios.test.ts |
| S12b | 黑洞恢复 | blackhole 30s → 恢复 | 僵尸连接清理 | 30 | extreme-scenarios.test.ts |
| S12c | 间歇黑洞 | 10s on/off × 5 | CB 状态机 | 30 | extreme-scenarios.test.ts |
| S13 | 5xx 风暴 | 50% 502/503 | CB 熔断 | 50 | extreme-scenarios.test.ts |
| S14 | 延迟抖动尖刺 | 200ms ± 150ms, 2s spike | 自适应超时 | 50 | extreme-scenarios.test.ts |
| S15 | DNS 慢解析 | DNS 500-2000ms | DNS cache | 30 | extreme-scenarios.test.ts |
| S16 | 连接池耗尽 | 并发 100, 池 10 | 并发队列 | 30 | extreme-scenarios.test.ts |

---

## 场景详细设计

### S9 — GPRS 极端弱网

```yaml
profile: gprs (RTT 500ms, 下行 6.25KB/s, 上行 2.5KB/s)
damage: packetLoss 2%, jitter ±100ms
action: cold start → POST /auth → GET /channels → GET /messages × 3
iterations: 30
timeout: 60s per iteration
metrics: successRate, zeroRetryP50, avgRetries

关键假设：
  - vanilla 在这种条件下成功率预计 < 70%
  - catcher + retry(3) 预计 > 85%
  - DNS cache 避免了 DNS 查询的额外 RTT
```

### S10 — 突发丢包风暴

```yaml
profile: good (base) + burstLoss
damage:
  burstLoss:
    p_good_to_bad: 0.03     # 每 chunk 3% 概率进入坏状态
    p_bad_to_good: 0.15     # 坏状态下每 chunk 15% 概率恢复
    loss_good: 0.01          # 好状态 1% 丢包
    loss_bad: 0.6            # 坏状态 60% 丢包
  badStateMinDuration: 30000 # 坏状态至少持续 30s
action: 连续 POST /messages × 200（坏状态期间多个请求同时失败）
iterations: 30
timeout: 45s

关键假设：
  - 坏状态期间 > 50% 请求失败，vanilla 成功率 < 40%
  - catcher CB 应在 5 个连续失败后熔断，保护后续请求
  - 恢复后 CB 半开试探成功 → 关闭
```

### S12a — 路由黑洞 30s

```yaml
profile: good (base) + blackhole
damage:
  blackhole:
    enabled: true
    duration: 30_000
action: 
  1. 发 10 个请求（正常）
  2. 开启黑洞
  3. 发 50 个请求（全部超时）
  4. 等待恢复
  5. 发 10 个请求（验证恢复）
iterations: 10
timeout: 120s

关键假设：
  - 黑洞期间所有请求超时
  - vanilla 50 个请求全部 hang 到 timeout
  - catcher CB 应在足够早的时机熔断
  - 恢复后 keepAlive 僵尸连接需要正确清理
```

### S12b — 黑洞恢复

```yaml
profile: good (base) + blackhole
damage:
  blackhole:
    enabled: true
    duration: 30_000
    destroyOnRecover: true
action:
  1. 建立 keepAlive 连接
  2. 开启黑洞 30s
  3. 关闭黑洞
  4. 发 50 个请求（验证僵尸连接已清理）
iterations: 10
timeout: 120s

关键假设：
  - 恢复后 destroyOnRecover 应清理所有僵尸连接
  - 后续请求全部成功（新连接）
  - 如果僵尸连接未清理，前几个请求会复用僵尸连接继续超时
```

### S12c — 间歇黑洞

```yaml
profile: good (base) + blackhole
damage:
  循环 5 次: { blackhole 10s → 正常 5s }
action: 持续发 POST /messages × 100
iterations: 5
timeout: 120s

关键假设：
  - CB 在黑洞期间 OPEN，正常期间 HALF_OPEN → CLOSED
  - CB 状态转换次数 = 黑洞次数 × 2
  - 正常期间请求应该成功（CB 正确恢复）
```

### S13 — 5xx 风暴

```yaml
damage: HTTP 服务端每请求 50% 概率返回 502/503
action: 连续 POST /messages × 100
iterations: 50
timeout: 30s

关键假设：
  - CB 应正确熔断（5xx 属于可重试错误累加）
  - 熔断后不再发送请求
  - 半开后探测成功则恢复
```

### S14 — 延迟抖动尖刺

```yaml
profile: base latency 100ms, jitter ±75ms, 偶发 2000ms spike (1%)
damage:
  latency: 100
  jitter: 75
  jitterDistribution: uniform
  spikeLatency: 2000
  spikeProbability: 0.01
action: GET /channels × 50
iterations: 50
timeout: 15s

关键假设：
  - 99% 请求延迟在 25-175ms（100 ± 75）
  - 1% 请求遭遇 2000ms spike
  - spike 可能触发 timeout（如果 timeout < 2000ms）
  - catcher 自适应超时应该能区分 spike vs 真正超时
```

### S15 — DNS 慢解析

```yaml
damage: DNS lookup 每次 500-2000ms 随机
action:
  1. 清空 DNS cache
  2. POST /auth → GET /channels → GET /messages × 3  （慢，每次都 DNS 查询）
  3. 再跑一次相同请求（快，DNS cache 命中）
iterations: 30
timeout: 30s

指标: 首次请求延迟 vs 后续请求延迟的比值
关键假设：
  - 首次请求延迟 = 网络延迟 + DNS 延迟
  - catcher 后续请求延迟 = 网络延迟（DNS cache 命中）
  - vanilla 每次都要 DNS 查询
```

### S16 — 连接池耗尽

```yaml
profile: good (无网络损伤)
config: catcher maxSockets=10, concurrency=100
action: 并发 100 个 GET /slow?delay=500
iterations: 30
timeout: 30s

关键假设：
  - 只有 10 个真实 TCP 连接
  - 90 个请求在队列等待
  - 队列按优先级排序
  - 不会因连接池耗尽而超时
```

---

## 场景模板

新增场景时参考以下模板：

```typescript
import { describe, it, expect, beforeAll, afterAll } from 'vitest'
import axios from 'axios'
import { createHttpClient } from '@eric8810/http'
import { createHttpTestServer, type TestServer } from '../servers/http-server.js'
import { createNetworkProxy, type NetworkProxy } from '../network/proxy.js'
import { NETWORK_PROFILES } from '../network/presets.js'
import { runConcurrentComparison } from '../harness.js'

describe('SXX — 场景名称', () => {
  let server: TestServer
  let proxy: NetworkProxy
  let proxyUrl: string

  beforeAll(async () => {
    server = await createHttpTestServer()
    proxy = createNetworkProxy(server.port)
    await proxy.start()
    proxyUrl = `http://127.0.0.1:${proxy.port}`
  }, 30000)

  afterAll(async () => {
    await proxy.stop()
    await server.close()
  })

  const ITERATIONS = parseInt(process.env.EXTREME_ITERATIONS ?? '30', 10)

  // vanilla 实现
  async function vanillaFn(baseUrl: string) {
    const vanilla = axios.create({ baseURL: baseUrl, timeout: 30_000 })
    const t0 = Date.now()
    try {
      await vanilla.get('/channels')
      return { success: true, time: Date.now() - t0 }
    } catch {
      return { success: false, time: Date.now() - t0 }
    }
  }

  // catcher 实现
  async function catcherFn(baseUrl: string) {
    const client = createHttpClient({ baseURL: baseUrl, retry: { attempts: 3 } })
    const t0 = Date.now()
    try {
      await client.get('/channels')
      return { success: true, time: Date.now() - t0 }
    } catch {
      return { success: false, time: Date.now() - t0 }
    }
  }

  it('对比验证', async () => {
    proxy.setConditions(NETWORK_PROFILES.gprs.conditions)
    proxy.disruptAll()

    const result = await runConcurrentComparison(
      { name: 'SXX', iterations: ITERATIONS, iterationTimeout: 60_000 },
      NETWORK_PROFILES.gprs.conditions,
      'gprs',
      vanillaFn,
      catcherFn,
      proxyUrl,
    )

    console.log(`vanilla: ${(result.vanilla.successRate * 100).toFixed(1)}%`)
    console.log(`catcher: ${(result.catcher.successRate * 100).toFixed(1)}%`)
    console.log(`improvement: +${(result.improvements.successRate * 100).toFixed(1)}pp`)

    // catcher 应该优于 vanilla
    expect(result.catcher.successRate).toBeGreaterThanOrEqual(result.vanilla.successRate)
  }, 300_000)
})
```
