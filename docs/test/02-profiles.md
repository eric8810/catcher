# 02 — 网络 Profile 体系

> 代码位置：`packages/test/network/presets.ts`
> 参考：Chrome DevTools / WebPageTest 标准值

---

## 设计原则

1. **对标业界标准** — 参数取值参考 Chrome DevTools + WebPageTest connectivity profiles
2. **一程延迟** — `latency` 表示的是一程（one-way）延迟，RTT = latency × 2
3. **对称优先** — 默认使用对称参数。上下行不对称通过 `upload`/`download` 子配置覆盖
4. **可组合** — profile 可以叠加额外的损伤（如给 4G 加上 blackhole）

---

## 完整 Profile 列表

### 现有 Profile（v0.1）

| Key | 名称 | RTT | 带宽 (KB/s) | 丢包 | 重置 | 用途 |
|-----|------|-----|------------|------|------|------|
| `good` | 良好网络 | 50ms | ∞ | 0% | 0% | 基准 |
| `weak` | 弱网 | 2000ms | 25 | 5% | 2% | 3G Slow 退化 |
| `veryWeak` | 极弱网 | 4000ms | 6.25 | 10% | 5% | 2G 退化 |
| `satellite` | 卫星 WiFi | 800ms | 250 | 2% | 1% | 卫星 |
| `mobile3g` | 偏远 3G | 2000ms | 6.25 | 8% | 8% | 山区 |
| `crossRegion` | 跨地域 | 300ms | ∞ | 1% | 0% | SG→华东 |
| `metro` | 地铁通勤 | 100ms | ∞ | 3% | 10% | 频繁切换 |

### 新增 Profile（v0.2）

基于 [Chrome DevTools throttling profiles](https://gist.github.com/theodorosploumis/fd4086ee58369b68aea6b0782dc96a2e)：

| Key | 名称 | RTT | 下行 (KB/s) | 上行 (KB/s) | 丢包 | 抖动 |
|-----|------|-----|------------|------------|------|------|
| `gprs` | GPRS (2.5G) | 500ms | 6.25 | 2.5 | 2% | ±100ms |
| `2g_regular` | 2G Regular | 300ms | 31.25 | 6.25 | 1% | ±50ms |
| `2g_good` | 2G Good | 150ms | 56.25 | 18.75 | 0.5% | ±30ms |
| `3g_slow` | 3G Slow | 200ms | 97.5 | 41.25 | 0.5% | ±40ms |
| `3g_good` | 3G Good | 40ms | 187.5 | 93.75 | 0% | ±10ms |
| `4g_lte` | 4G/LTE | 20ms | 500 | 375 | 0% | ±5ms |
| `dsl` | DSL 宽带 | 6ms | 250 | 125 | 0% | ±2ms |

### 混沌 Profile

| Key | 名称 | 描述 |
|-----|------|------|
| `blackhole_30s` | 路由黑洞 30s | WiFi 正常 + 上游黑洞 30s |
| `blackhole_intermittent` | 间歇黑洞 | 10s 通 / 10s 断 × 5 轮 |
| `burst_storm` | 突发丢包风暴 | Gilbert-Elliott, 30s 坏状态 |
| `asymmetric_2g` | 2G 不对称 | 下行 250kbps / 上行 50kbps |

---

## 标准对标表

| catcher profile | Chrome DevTools | WebPageTest | 差异 |
|:--|:--|:--|:--|
| `good` | No throttling | Native | — |
| `4g_lte` | — | 4G | 新增 |
| `3g_good` | Fast 3G | 3G | 新增 |
| `3g_slow` | Slow 3G | 3G Slow | 新增 |
| `weak` | Slow 3G + 5% loss | — | catcher 特有 |
| `2g_good` | — | 2G | 新增 |
| `2g_regular` | — | 2G Regular | 新增 |
| `gprs` | — | GPRS | 新增 |
| `veryWeak` | — | — | catcher 特有（极弱）|
| `satellite` | — | — | catcher 特有 |
| `mobile3g` | — | — | catcher 特有 |
| `crossRegion` | — | — | catcher 特有 |
| `metro` | — | — | catcher 特有 |

---

## Profile 使用

```typescript
import { NETWORK_PROFILES } from '../network/presets.js'

// 直接使用
proxy.setConditions(NETWORK_PROFILES.gprs.conditions)

// 叠加额外损伤
proxy.setConditions({
  ...NETWORK_PROFILES['4g_lte'].conditions,
  blackhole: { enabled: true, duration: 30_000 },
})

// 上下行不对称
proxy.setConditions({
  ...NETWORK_PROFILES['3g_slow'].conditions,
  upload: { latency: 500, bandwidth: 6_250 },   // 上行极差
})
```
