# 网络模拟与测试策略补充

> 基于对 Chrome DevTools、tc netem、Gilbert-Elliott 模型、Chaos Engineering 实践的调研
> 配套：现有 `presets.ts` 中的 7 个 profile、`proxy.ts` 的 4 种损伤

---

## 一、当前覆盖 vs 缺失

### 1.1 当前 NetworkProxy 能模拟的

| 损伤类型 | 实现方式 | 真实度 |
|---------|---------|--------|
| 固定延迟 | 每 chunk `await delay(N)` | 🟡 缺抖动 |
| 独立随机丢包 | 每 chunk `Math.random() < P` | 🔴 不真实 |
| 固定带宽上限 | 100ms 滑动窗口 | 🟡 缺波动 |
| 连接重置 | 随机 `socket.destroy()` | 🟢 OK |

### 1.2 需要补充的损伤模型

| 损伤 | 真实表现 | 对 catcher 的影响 |
|------|---------|------------------|
| **延迟抖动 (jitter)** | RTT 不是恒定值，波动 ±20-50% | 自适应超时边界、退避窗口 |
| **突发丢包 (burst loss)** | 丢包集中爆发，非独立事件 | 多请求同时失败 → 熔断器 |
| **上下行不对称** | 移动端下载好、上传差 | keepAlive 连接复用有方向差异 |
| **带宽波动** | 不是恒定天花板，每秒都在变 | 大 payload 吞吐预测 |
| **数据包损坏 (corrupt)** | 包到达但内容错 | 类似丢包但浪费了 RTT + 带宽 |
| **数据包重复 (duplicate)** | 同一包到达多次 | TCP 栈可以处理，应用层无感 |
| **数据包乱序 (reorder)** | 后发包先到 | 触发 TCP 重传，增加延迟 |

### 1.3 未覆盖的测试场景

| 场景 | 描述 | 优先级 |
|------|------|--------|
| **DNS 故障** | DNS 超时、NXDOMAIN、慢解析 | P1 |
| **TLS 握手失败** | 证书错误、握手超时、协议不匹配 | P2 |
| **连接池耗尽** | 所有 socket 被占满 | P1 |
| **服务端限流 (429)** | 触发 retry-after 逻辑 | P2 |
| **级联故障 (502/503/504)** | 上游服务不可用 | P1 |
| **网络突然中断** | 飞行模式/隧道进入/离开 | P1 |
| **网络类型切换** | 4G→3G→WiFi 的过渡期 | P2 |
| **慢响应（非网络）** | 服务端处理慢（非丢包/延迟）| P2 |

---

## 二、标准网络 Profile（对标 Chrome DevTools）

调研来源：[Chrome DevTools throttling profiles](https://gist.github.com/theodorosploumis/fd4086ee58369b68aea6b0782dc96a2e)、[WebPageTest connectivity](https://github.com/WPO-Foundation/webpagetest/blob/master/www/settings/connectivity.ini.sample)

| Profile | 下行 (kb/s) | 上行 (kb/s) | RTT (ms) | 一程延迟 (ms) |
|---------|------------|------------|----------|-------------|
| GPRS | 50 | 20 | 1000 | 500 |
| 2G Regular | 250 | 50 | 600 | 300 |
| 2G Good | 450 | 150 | 300 | 150 |
| 3G Slow | 780 | 330 | 400 | 200 |
| 3G Regular | 750 | 250 | 200 | 100 |
| 3G Good | 1500 | 750 | 80 | 40 |
| 4G/LTE | 4000 | 3000 | 40 | 20 |
| DSL | 2000 | 1000 | 10 | 5 |
| Wi-Fi | 30000 | 15000 | 4 | 2 |

### 与现有 preset 的映射

| 现有 preset | 对应标准 profile | 差距 |
|------------|-----------------|------|
| `good` (25ms, 无丢包) | Wi-Fi / 4G | ✅ 接近 |
| `weak` (1000ms, 5%丢包, 25KB/s) | 3G Slow + 丢包 | 🟡 上下行未分离 |
| `veryWeak` (2000ms, 10%丢包, 6.25KB/s) | 2G Regular + 高丢包 | 🟡 上下行未分离 |
| `satellite` (400ms, 2%丢包) | — | ✅ 卫星特有 |
| `mobile3g` (1000ms, 8%丢包, 6.25KB/s) | 3G Slow 下行 + GPRS 上行 | 🔴 需分上下行 |
| `crossRegion` (150ms, 1%丢包) | — | ✅ 跨地域特有 |
| `metro` (50ms, 3%丢包, 10%重置) | — | ✅ 地铁特有 |

**缺失的标准 profile**：GPRS、2G Regular、3G Good、DSL

---

## 三、损伤模型的改进方案

### 3.1 抖动 (Jitter)

**现状**：固定延迟 `latency: 2000` 意味着每个 chunk 都延迟精确 2000ms。

**真实情况**：2000ms RTT 的链路，实际延迟在 1500-2500ms 之间波动，偶尔出现 4000ms 的尖刺。

**实现方案**：

```typescript
interface NetworkConditions {
  latency?: number          // 基础延迟
  jitter?: number           // ±抖动范围 (uniform)，默认 latency * 0.25
  // 或者用对数正态分布：
  jitterDistribution?: 'uniform' | 'normal'
  jitterStdDev?: number     // 正态分布用标准差
}
```

延迟计算：`actualLatency = latency + random(-jitter, +jitter)`，clamp 到 0 以上。

**测试影响**：使自适应超时和退避策略的边界条件更真实。固定延迟下从不超时的请求，加抖动后可能偶尔触发超时。

### 3.2 突发丢包 (Burst Loss — Gilbert-Elliott 模型)

**现状**：独立随机丢包 `packetLoss: 0.05` 意味着每个 chunk 独立 5% 概率丢弃。

**真实情况**：网络在"好状态"和"坏状态"之间切换。在坏状态时，连续多个包全部丢弃。

**Gilbert-Elliott 两状态马尔可夫模型**：

```
       p(保持好)          p(保持坏)
    ┌──────────┐       ┌──────────┐
    │  GOOD    │ ←───→ │   BAD    │
    │ 丢包率低  │ p(g→b)│ 丢包率高  │
    └──────────┘ p(b→g)└──────────┘
```

**参数**：
- `p_gb`：从好状态转移到坏状态的概率（典型值 0.01-0.05）
- `p_bg`：从坏状态恢复到好状态的概率（典型值 0.1-0.3）
- `loss_good`：好状态下的丢包率（典型值 0-0.02）
- `loss_bad`：坏状态下的丢包率（典型值 0.3-0.8）

**实现方案**：

```typescript
interface NetworkConditions {
  // 保留原独立丢包（简单场景）
  packetLoss?: number
  
  // 新增突发丢包
  burstLoss?: {
    p_good_to_bad: number     // 好→坏 转移概率
    p_bad_to_good: number     // 坏→好 转移概率
    loss_good: number         // 好状态丢包率
    loss_bad: number          // 坏状态丢包率
  }
}
```

**测试影响**：burst loss 会让 catcher 在坏状态时面对连续的多个失败。retry + circuit breaker 的组合能否在这种情况下保持可用，会是一个关键的验证点。

### 3.3 上下行不对称

**现状**：同一个 `NetworkConditions` 同时作用于 client→server 和 server→client 两个方向的 pipe。

**真实情况**：移动网络上下行差距巨大。4G 下行可达 4000kb/s 但上行只有 3000kb/s；到了 2G，下行 250kb/s 但上行仅 50kb/s（5 倍差距）。

**实现方案**：

```typescript
interface NetworkConditions {
  // 对称参数（向下兼容）
  latency?: number
  packetLoss?: number
  
  // 上行（客户端→服务端）
  upload?: {
    latency?: number
    jitter?: number
    packetLoss?: number
    bandwidth?: number
    burstLoss?: BurstLossConfig
  }
  
  // 下行（服务端→客户端）
  download?: {
    latency?: number
    jitter?: number
    packetLoss?: number
    bandwidth?: number
    burstLoss?: BurstLossConfig
  }
}
```

**测试影响**：POST 请求（发送 body）受上行影响更大，GET 请求受下行影响更大。catcher 的 keepAlive 连接效率应该在不对称条件下仍有优势。

### 3.4 路由黑洞 (Blackhole)

**与随机丢包的本质区别**：

| | 随机丢包 | 连接 Reset | 路由黑洞 |
|--|---------|----------|---------|
| TCP 层面 | 部分包丢，重传可恢复 | RST 立刻通知 | **全部包静默丢弃** |
| 应用层感知 | 请求慢但可能成功 | 立刻 ECONNRESET | **hang 到超时** |
| retry | ✅ 有用 | ❌ 立即失败 | **❌ 全部超时** |
| CB | 不一定触发 | 会触发 | **必须触发** |
| keepAlive 连接 | 可能存活 | 被销毁 | **僵尸连接** |

**典型场景**：WiFi 信号满格，但上游光猫/路由器的 NAT 表溢出、运营商路由收敛、或者交换机端口 flapping。TCP 连接状态仍是 ESTABLISHED，但所有包被黑洞吞掉，没有 RST、没有 ICMP。

**对 catcher 的考验**：

```
请求1 → hang 30s → timeout
请求2 → hang 30s → timeout    ← retry 毫无意义
请求3 → hang 30s → timeout    ← CB 能否在足够早的时机熔断？
```

核心问题不是重试，而是**检测速度**：
- 如果 CB 等 5 个连续超时才熔断 → 150s 不可用窗口
- 如果超时设太长（60s）→ 一个请求就卡一分钟
- 恢复后 keepAlive 僵尸连接还在 → 后续请求继续失败

**实现方案**：

```typescript
interface NetworkConditions {
  // ... 现有 ...

  /** 路由黑洞：静默丢弃所有包，不发送 RST */
  blackhole?: {
    enabled: boolean
    /** 黑洞持续 ms。0 = 直到手动关闭 */
    duration?: number
    /** 黑洞开始前的延迟 ms */
    startAfter?: number
    /** 黑洞结束后是否自动销毁所有僵尸连接 */
    destroyConnectionsOnRecover?: boolean
  }
}
```

在 `createThrottledPipe` 的 data handler 最前面：

```typescript
source.on('data', async (chunk: Buffer) => {
  if (conditions.blackhole?.enabled) return  // 静默丢弃
  // ... existing logic ...
})
```

改动量：~15 行 proxy + ~30 行 test。

**对应测试场景**：

| 编号 | 场景 | 操作 | 验证点 |
|------|------|------|--------|
| S12a | 黑洞 30s | 开启黑洞 30s，期间发 200 请求 | catcher 是否比 vanilla 更早检测到不可用？ |
| S12b | 黑洞恢复 | 黑洞 30s → 恢复 → 再发 200 | 僵尸 keepAlive 连接是否被清理？ |
| S12c | 间歇黑洞 | 10s 黑洞 → 5s 正常 → 循环 5 次 | CB 开↔关状态转换是否正确？ |

### 3.5 带宽波动

**现状**：`bandwidth` 是固定的字节/秒上限。

**真实情况**：移动网络带宽波动很大。一秒 200KB/s，下一秒 2KB/s。

**简单实现**：不引入新参数，而是在现有 `bandwidth` 基础上周期性随机波动：

```typescript
// 每隔 1-3 秒，带宽在 [bandwidth * 0.3, bandwidth * 1.5] 之间随机变化
```

更复杂的方案可以引入正弦波模拟（模拟小区负载周期变化），但实际收益不大。

---

## 四、Chi 场景补充

### 4.1 场景全矩阵

| 编号 | 场景 | 损伤 | 验证特性 | 状态 |
|------|------|------|---------|------|
| S1 | 冷启动 → 登录 | keepAlive | Agent 连接复用 | ✅ |
| S2 | 发送消息 | 丢包+重置 | retry | ✅ |
| S3 | 频道切换 | 高 RTT | keepAlive + retry | ✅ |
| S4 | 跨地域用户 | 中等 RTT | keepAlive | ✅ |
| S5 | 大 payload | 低带宽 | msgpack vs JSON | ✅ |
| S6 | WS 高频消息 | 丢包 | deflate + codec | ✅ |
| S7 | 并发优先级 | 无损伤 | 优先级队列 | ✅ |
| S8 | DNS 缓存 | 无损伤 | DNS cache | ✅ |
| — | **以下为新增** | | | |
| S9 | **GPRS 极端弱网** | 500ms RTT, 50kbps 下行, 20kbps 上行 | retry 极限 | 🔴 |
| S10 | **突发丢包风暴** | Gilbert-Elliott, 30s 坏状态 | CB + retry | 🔴 |
| S11 | **上下行严重不对称** | 2G: 下行 250kbps, 上行 50kbps | POST vs GET 表现差异 | 🟡 |
| S12 | **路由黑洞** | 全部包静默丢弃，无 RST | CB 检测速度 + 僵尸连接清理 | 🔴 |
| S12a | ↳ 黑洞 30s | 开启黑洞 30s，期间发请求 | catcher 检测速度 vs vanilla | 🔴 |
| S12b | ↳ 黑洞恢复 | 黑洞 30s → 恢复 → 再发 | keepAlive 僵尸连接清理 | 🔴 |
| S12c | ↳ 间歇黑洞 | 10s 黑洞 → 5s 正常 × 5 轮 | CB 状态转换 | 🔴 |
| S13 | **服务端 5xx 风暴** | 50% 请求返回 502/503 | CB 正确熔断 | 🟡 |
| S14 | **延迟抖动尖刺** | 基础 200ms, ±150ms jitter, 偶尔 2000ms | 自适应超时 | 🟡 |
| S15 | **DNS 慢解析** | DNS 每次 500-2000ms 随机 | DNS cache 命中率 | 🟡 |
| S16 | **连接池耗尽** | 并发 100, 池大小 10 | 排队 + 超时表现 | 🟡 |

### 4.2 优先级排序

| 优先级 | 场景 | 理由 |
|--------|------|------|
| 🔴 P0 | S12 路由黑洞 | CB 检测速度 + 僵尸连接清理，最能暴露超时策略问题 |
| 🔴 P0 | S10 突发丢包风暴 | 最能暴露 retry+CB 的真实行为 |
| 🔴 P0 | S9 GPRS 极端弱网 | 测试 retry 极限条件下的表现 |
| 🟡 P1 | S13 5xx 风暴 | CB 功能验证 |
| 🟡 P1 | S14 延迟抖动 | 自适应超时的边界测试 |
| 🟡 P1 | S11 上下行不对称 | 补充 profile 真实性 |
| 🟡 P1 | S15 DNS 慢解析 | DNS cache 价值验证 |
| 🟢 P2 | S16 连接池耗尽 | 并发控制边界 |

---

## 五、Proxy 改进路线

| 阶段 | 改进 | 改动量 | 收益 |
|------|------|--------|------|
| 0 | **blackhole 模式** | ~15 行 | 模拟路由黑洞，CB 检测速度验证 |
| 1 | 加 jitter 参数 | ~10 行 | 延迟真实性 +30% |
| 2 | Gilbert-Elliott burst loss | ~60 行 | 丢包真实性 +50% |
| 3 | 上下行分离 (upload/download) | ~40 行 | 移动网络真实性 +40% |
| 4 | 带宽波动 | ~20 行 | 带宽真实性 +20% |
| 5 | packet corrupt + reorder | ~30 行 | 覆盖 tc netem 全能力 |

---

## 六、标准网络 Profile 补充（presets.ts 新增）

建议新增以下对标 Chrome DevTools 的 profile：

```typescript
// 新增（基于 Chrome DevTools + WebPageTest）
gprs: {
  name: 'GPRS (2.5G)',
  emoji: '📟',
  conditions: {
    latency: 250,         // 500ms RTT
    jitter: 100,
    bandwidth: 6_250,     // 50kbps → 6.25KB/s
    packetLoss: 0.02,
    connectionReset: 0.03,
  },
},
2g_regular: {
  name: '2G Regular',
  emoji: '📶',
  conditions: {
    latency: 150,         // 300ms RTT
    jitter: 50,
    bandwidth: 31_250,    // 250kbps → 31.25KB/s
    packetLoss: 0.01,
    connectionReset: 0.02,
  },
},
3g_slow: {
  name: '3G Slow',
  emoji: '📱',
  conditions: {
    latency: 100,         // 200ms RTT
    jitter: 40,
    bandwidth: 97_500,    // 780kbps → 97.5KB/s
    packetLoss: 0.005,
    connectionReset: 0.01,
  },
},
dsl: {
  name: 'DSL 宽带',
  emoji: '🏠',
  conditions: {
    latency: 3,           // 6ms RTT
    jitter: 2,
    bandwidth: 250_000,   // 2Mbps → 250KB/s
    packetLoss: 0,
    connectionReset: 0,
  },
},
```

---

## 七、参考

- [Chrome DevTools throttling profiles](https://github.com/mozilla/gecko-dev/blob/master/devtools/client/shared/components/throttling/profiles.js)
- [WebPageTest connectivity profiles](https://github.com/WPO-Foundation/webpagetest/blob/master/www/settings/connectivity.ini.sample)
- [Gilbert-Elliott model for packet loss](https://people.computing.clemson.edu/~jmarty/projects/lowLatencyNetworking/papers/APPFEC/GEModelForLossinTheRTInternet.pdf)
- [Linux tc netem manual](https://man7.org/linux/man-pages/man8/tc-netem.8.html)
- [Comcast — Go network emulator](https://github.com/tylertreat/comcast)
- [Network Emulation Conditions](https://github.com/addyosmani/network-emulation-conditions)
- [Chaos Engineering: types and experiments](https://steadybit.com/blog/chaos-experiments/)
