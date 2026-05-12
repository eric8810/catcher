# 07 — TS e2e 测试复用方案

> 目标：用现有 TypeScript e2e 测试基础设施验证 Rust 核心的正确性和性能
> 位置：`packages/catcher-ts/test/`

---

## 1. 总览：复用 vs 新建

| 组件 | 文件 | 复用 | 说明 |
|------|------|------|------|
| NetworkProxy | `test/network/proxy.ts` | ✅ 直接复用 | 模拟延迟/丢包/限宽/断连 |
| 网络预设 | `test/network/presets.ts` | ✅ 直接复用 | 7 种环境 (good→metro) |
| HTTP 测试服务器 | `test/servers/http-server.ts` | ✅ 直接复用 | IM API Gateway 模拟 |
| WS 测试服务器 | `test/servers/ws-server.ts` | ✅ 直接复用 | Echo + heartbeat |
| Harness 框架 | `test/harness.ts` | ✅ 直接复用 | 并发对比 + 指标计算 |
| ComparisonReporter | `test/reporters/comparison-reporter.ts` | ✅ 直接复用 | 对比报告生成 |
| 8 个 E2E 场景 | `test/e2e/scenarios.test.ts` | 🔄 改写适配 | vanilla (axios) vs Rust (napi) |
| Chaos 测试 | `test/chaos/chaos.test.ts` | 🔄 改写适配 | 只测 Rust，不对比 |
| HTTP 集成测试 | `test/integration/http.test.ts` | 🔄 改写适配 | 对比模式不变 |
| WS 集成测试 | `test/integration/ws.test.ts` | 🔄 改写适配 | 对比模式不变 |

---

## 2. 适配方案

### 2.1 创建 Rust adapter layer

新建 `packages/catcher-ts/test/adapters/rust-adapter.ts`：

```typescript
/**
 * Rust adapter — 将 catcher-rs (napi-rs) 的 API 包装为
 * 与现有测试 harness 兼容的接口。
 *
 * 签名与现有 catcher TS 实现一致，可直接替换 import。
 */
import { HttpClient, WsClient, pack, unpack } from 'catcher-rs'
import type { IterationResult } from '../harness.js'

// ── HTTP ────────────────────────────────────────────

export interface RustHttpConfig {
  baseURL: string
  keepAlive: boolean
  dnsCacheTtl: number
  retry: { attempts: number; backoff: string; onRetry?: () => void }
  timeout: { response: number }
  concurrency?: number
}

export function createRustHttpClient(config: RustHttpConfig) {
  const inner = new HttpClient(JSON.stringify({
    base_url: config.baseURL,
    connect_timeout_ms: 5000,
    response_timeout_ms: config.timeout.response,
    keep_alive: config.keepAlive,
    keep_alive_interval_secs: 60,
    max_idle_per_host: 10,
    idle_timeout_secs: 90,
    retry: config.retry ? {
      max_attempts: config.retry.attempts,
      backoff: mapBackoff(config.retry.backoff),
      min_backoff_ms: 100,
      max_backoff_ms: 10000,
      jitter: true,
    } : null,
    circuit_breaker: null, // Phase 3+
    max_concurrency: config.concurrency ?? 50,
  }))

  return {
    async get(path: string): Promise<any> {
      const resp = await inner.get(path)
      if (resp.status >= 400) throw new Error(`HTTP ${resp.status}`)
      return JSON.parse(Buffer.from(resp.body).toString('utf-8'))
    },
    async post(path: string, body: unknown): Promise<any> {
      const json = JSON.stringify(body)
      const resp = await inner.post(path, Buffer.from(json), 'application/json')
      if (resp.status >= 400) throw new Error(`HTTP ${resp.status}`)
      return JSON.parse(Buffer.from(resp.body).toString('utf-8'))
    },
  }
}

// ── WebSocket ────────────────────────────────────────

export function createRustWsClient(config: {
  url: string
  perMessageDeflate?: boolean
  handshakeTimeout?: number
  reconnect?: { maxAttempts?: number }
}) {
  const ws = new WsClient(JSON.stringify({
    urls: [config.url],
    per_message_deflate: config.perMessageDeflate ?? false,
    handshake_timeout_ms: config.handshakeTimeout ?? 15000,
    reconnect: config.reconnect ? {
      max_attempts: config.reconnect.maxAttempts ?? 0,
      initial_delay_ms: 500,
      max_delay_ms: 15000,
      backoff_multiplier: 2,
    } : null,
    race_count: 1,
  }))

  return {
    addEventListener(event: string, handler: (...args: any[]) => void) {
      ws.on(event, handler)
    },
    send(data: any) {
      ws.send(data)
    },
    close() {
      ws.close()
    },
  }
}

function mapBackoff(b: string): string {
  switch (b) {
    case 'fixed': return 'Fixed'
    case 'exponential': return 'Exponential'
    default: return 'DecorrelatedJitter'
  }
}
```

### 2.2 场景改写模式

以 S2 (发送文本消息) 为例，展示改写前后对比：

**改写前**（使用 TS catcher）：
```typescript
import { createHttpClient } from '../../src/http/client.js'

async function catcherS2(baseUrl: string): Promise<IterationResult> {
  clearDnsCache()
  const start = Date.now()
  let retries = 0
  try {
    const client = createHttpClient({
      baseURL: baseUrl, keepAlive: true,
      retry: { attempts: 3, backoff: 'exponential', onRetry: () => { retries++ } },
      timeout: { response: 30_000 },
    })
    await client.post('/messages', { text: 'Hello '.repeat(30) })
    return { success: true, time: Date.now() - start, retries }
  } catch { return { success: false, time: 30_000, retries } }
}
```

**改写后**（使用 Rust via napi-rs）：
```typescript
import { createRustHttpClient } from '../adapters/rust-adapter.js'
import { clearDnsCache } from '../adapters/dns-adapter.js'

async function rustS2(baseUrl: string): Promise<IterationResult> {
  clearDnsCache()
  const start = Date.now()
  let retries = 0
  try {
    const client = createRustHttpClient({
      baseURL: baseUrl, keepAlive: true,
      retry: { attempts: 3, backoff: 'exponential', onRetry: () => { retries++ } },
      timeout: { response: 30_000 },
    })
    await client.post('/messages', { text: 'Hello '.repeat(30) })
    return { success: true, time: Date.now() - start, retries }
  } catch { return { success: false, time: 30_000, retries } }
}
```

**变化**：只替换了 `import` 和工厂函数名。其余逻辑完全不变。

### 2.3 对比模式

**验证 Rust 对标 TS 实现**（Phase 5 后）：

```typescript
// 新文件：test/e2e/rust-vs-vanilla.test.ts
describe('S2: 发送文本消息 (Rust)', () => {
  for (const [key, profile] of Object.entries(NETWORK_PROFILES)) {
    if (!['weak', 'veryWeak', 'mobile3g'].includes(key)) continue
    it(`${profile.emoji} ${profile.name}`, async () => {
      httpProxy.setConditions(profile.conditions)
      httpProxy.disruptAll()
      const r = await runConcurrentComparison(
        { name: 'S2: 发送文本消息 (Rust)', iterations: itersFor(key), iterationTimeout: 35_000 },
        profile.conditions, `${profile.emoji} ${profile.name}`,
        vanillaS2,      // vanilla: axios
        rustS2,          // Rust via napi-rs
        httpUrl,
      )
      reporter.addResult(r)
      expect(r.catcher.successRate).toBeGreaterThanOrEqual(r.vanilla.successRate - 0.3)
    }, TIMEOUT)
  }
})
```

**也可以对比 Rust vs TS catcher**（性能对比）：

```typescript
// 新文件：test/e2e/rust-vs-ts.test.ts
describe('S2: Rust vs TS catcher', () => {
  // ...
  const r = await runConcurrentComparison(
    config, conditions, name,
    tsCatcherS2,     // TS 实现
    rustS2,          // Rust 实现
    httpUrl,
  )
  // 验证 Rust 不低于 TS 的成功率
})
```

---

## 3. 8 个场景的 Rust 适配要点

| 场景 | 关键功能 | 适配注意事项 |
|------|---------|-------------|
| S1 — 冷启动 | keepAlive + DNS cache | Rust 的 DnsConfig 需暴露 TTL 参数 |
| S2 — 发送文本 | retry on weak network | Rust retry 通过 napi 暴露 retry count |
| S3 — 加载消息 | keepAlive + retry | 复用 S1/S2 adapter |
| S4 — 跨地域 | 高 RTT 连接复用 | 无特殊适配 |
| S5 — 大体积 | msgpack vs JSON 压缩率 | Rust pack() 返回 Buffer，对比字节数 |
| S6 — WS 高频 | perMessageDeflate + msgpack | Rust WsClient 通过 napi 暴露 |
| S7 — 优先级队列 | 高优消息优先于低优请求 | Rust concurrency 限制 + 优先级参数 |
| S8 — DNS 缓存 | 首次 vs 后续请求延迟 | Rust DNS cache TTL 配置 |

---

## 4. Chaos 测试适配

**改写前**（测试 TS catcher 韧性）：
```typescript
const httpClient = createHttpClient({ /* TS config */ })
const ws = createResilientWS({ /* TS config */ })
```

**改写后**（测试 Rust 韧性）：
```typescript
const httpClient = createRustHttpClient({ /* Rust config */ })
const ws = createRustWsClient({ /* Rust config */ })
```

其余 chaos 逻辑（随机网络条件切换、消息循环发送、成功率断言）完全不变：

```typescript
// 完全不变的部分
const conditionTimer = setInterval(() => {
  const { name, conditions } = randomCondition()
  httpProxy.setConditions(conditions)
  httpProxy.disruptAll()
  log('condition-switch', `${name}`)
}, CONDITION_SWITCH_MS)

while (Date.now() < endTime) {
  result.totalSends++
  try {
    await httpClient.post('/messages', { /* ... */ })
    result.successfulSends++
  } catch { result.failedSends++ }
  await new Promise((r) => setTimeout(r, SEND_INTERVAL_MS))
}

expect(result.successRate).toBeGreaterThanOrEqual(0.70)
```

---

## 5. 需要的额外 adapter

### 5.1 DNS 缓存 adapter

由于 Rust 的 DNS 缓存在进程内（hickory-resolver），`clearDnsCache()` 需要通过 napi 暴露：

```rust
// catcher-rs-napi/src/lib.rs 或 dns.rs
#[napi]
pub fn clear_dns_cache() {
    // 通知 Rust 侧清空 hickory-resolver 缓存
    catcher_rs::transport::dns::clear_cache();
}
```

```typescript
// test/adapters/dns-adapter.ts
import { clearDnsCache as rustClearDns } from 'catcher-rs'

export function clearDnsCache() {
  rustClearDns()
}
```

### 5.2 Metrics adapter

```typescript
// test/adapters/metrics-adapter.ts
import { getMetricsSnapshot } from 'catcher-rs'

export function getRustMetrics() {
  return getMetricsSnapshot()
}
```

---

## 6. 测试运行

### 6.1 运行 Rust e2e 测试

```bash
# 编译 Rust napi addon
cd packages/catcher-rs-napi/
pnpm build

# 运行 e2e 测试（使用 Rust adapter）
cd packages/catcher-ts/
FAST_ITERATIONS=100 pnpm vitest run test/e2e/rust-vs-vanilla.test.ts

# 运行 Rust vs TS 对比
pnpm vitest run test/e2e/rust-vs-ts.test.ts

# 运行 chaos 测试
CHAOS_DURATION_MS=60000 pnpm vitest run test/chaos/rust-chaos.test.ts
```

### 6.2 CI 集成

```yaml
# .github/workflows/e2e-rust.yml
name: E2E Rust Validation

jobs:
  e2e:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: pnpm/action-setup@v2
        with:
          version: 8
      - name: Build Rust addon
        working-directory: packages/catcher-rs-napi
        run: |
          cargo build --release
          pnpm build
      - name: Run E2E tests
        working-directory: packages/catcher-ts
        run: |
          pnpm install
          FAST_ITERATIONS=30 pnpm vitest run test/e2e/rust-vs-vanilla.test.ts
```

---

## 7. 对比指标体系

沿用 `test/harness.ts` 的 ScenarioResult 结构：

| 指标 | 说明 | 期望 |
|------|------|------|
| `successRate` | 成功率 0-1 | Rust >= 0.95 (good), >= 0.85 (weak), >= 0.70 (very-weak) |
| `zeroRetryP50` | 无重试成功延迟 P50 | Rust 应与 TS 实现持平或更优 |
| `zeroRetryP95` | 无重试成功延迟 P95 | Rust 应与 TS 实现持平或更优 |
| `avgRetries` | 平均重试次数 | Rust retry 行为与 TS 一致 |
| `avgBytes` | 平均传输字节 | Rust msgpack 字节 ≤ TS msgpackr |
| `avgConnections` | 平均连接数 | Rust keepAlive 连接数 ≤ vanilla |

**额外 Rust 特有指标**（通过 MetricsCollector 获取）：
- Circuit breaker open count
- Network quality level transitions
- Adaptive timeout P90 values

---

## 8. 回退计划

如果 napi-rs 绑定在某个平台不可用，Rust 核心仍可通过以下方式独立验证：

1. **纯 Rust 集成测试**：wiremock + tokio-tungstenite mock（Phase 1-4）
2. **C ABI 测试**：C test harness 验证 FFI 契约
3. **手动验证**：TS 测试回退到纯 TS catcher 实现

Rust 与 TS 对比测试是锦上添花，不是阻塞项。
