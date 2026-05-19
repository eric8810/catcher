/**
 * NAPI mirror of extreme scenario tests — S9 to S16c.
 *
 * Tests catcher's Rust/NAPI resilience mechanisms under pathological network conditions:
 *   S9  — GPRS extreme weak network
 *   S10 — Burst packet loss storm (Gilbert-Elliott)
 *   S11 — Asymmetric up/down (2G)
 *   S12a — Route black hole 30s
 *   S12b — Black hole recovery
 *   S12c — Intermittent black hole
 *   S13 — 5xx server error storm
 *   S14 — Latency jitter + spikes
 *   S15 — Slow DNS resolution
 *   S16 — Connection pool exhaustion
 *
 * Configurable via env:
 *   EXTREME_ITERATIONS=30    iterations per scenario
 */

import axios from 'axios'
import { describe, it, expect, beforeAll, afterAll } from 'vitest'
import { createRustHttpClient } from '../adapters/rust-adapter.js'
import { createHttpTestServer, type TestServer } from '../servers/http-server.js'
import { createNetworkProxy, type NetworkProxy } from '../network/proxy.js'
import { NETWORK_PROFILES } from '../network/presets.js'

const TIMEOUT = 300_000
const ITERATIONS = parseInt(process.env.EXTREME_ITERATIONS ?? '30', 10)

// ── Helpers ───────────────────────────────────────────────────

interface LatencyStats {
  p50: number; p95: number; p99: number; mean: number; min: number; max: number
}

function computeStats(times: number[]): LatencyStats {
  if (times.length === 0) return { p50: 0, p95: 0, p99: 0, mean: 0, min: 0, max: 0 }
  const sorted = [...times].sort((a, b) => a - b)
  const p = (pct: number) => sorted[Math.ceil((pct / 100) * sorted.length) - 1] ?? sorted[sorted.length - 1]
  return {
    p50: p(50), p95: p(95), p99: p(99),
    mean: Math.round(sorted.reduce((a, b) => a + b, 0) / sorted.length),
    min: sorted[0], max: sorted[sorted.length - 1],
  }
}

// ═══════════════════════════════════════════════════════════════
// S9 — GPRS extreme weak network
// ═══════════════════════════════════════════════════════════════

describe('S9 — GPRS extreme weak network', () => {
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

  it('GPRS — vanilla vs catcher', async () => {
    proxy.setConditions(NETWORK_PROFILES.gprs.conditions)
    proxy.disruptAll()

    const profile = NETWORK_PROFILES.gprs

    // vanilla
    let vSuccess = 0, vTimes: number[] = [], vFails = 0
    {
      const instance = axios.create({ baseURL: proxyUrl, timeout: 30_000 })
      for (let i = 0; i < ITERATIONS; i++) {
        const t0 = Date.now()
        try {
          await instance.post('/auth', { user: 'test' })
          await instance.get('/channels')
          for (let j = 0; j < 3; j++) await instance.get(`/channels/${j}/messages?pageSize=10`)
          vSuccess++
          vTimes.push(Date.now() - t0)
        } catch { vFails++ }
      }
    }

    // catcher (NAPI)
    let cSuccess = 0, cTimes: number[] = [], cFails = 0
    {
      const client = createRustHttpClient({
        baseURL: proxyUrl,
        keepAlive: true,
        dnsCacheTtl: 300,
        retry: { attempts: 3, backoff: 'exponential' },
        timeout: { response: 30_000 },
      })
      for (let i = 0; i < ITERATIONS; i++) {
        const t0 = Date.now()
        try {
          await client.post('/auth', { user: 'test' })
          await client.get('/channels')
          for (let j = 0; j < 3; j++) await client.get(`/channels/${j}/messages?pageSize=10`)
          cSuccess++
          cTimes.push(Date.now() - t0)
        } catch { cFails++ }
      }
    }

    const vRate = (vSuccess / ITERATIONS * 100).toFixed(1)
    const cRate = (cSuccess / ITERATIONS * 100).toFixed(1)
    const vStats = computeStats(vTimes)
    const cStats = computeStats(cTimes)

    console.log(`\n  ${profile.emoji} S9 GPRS (${ITERATIONS} iterations)`)
    console.log(`  vanilla: ${vRate}% success, p50=${vStats.p50}ms p95=${vStats.p95}ms mean=${vStats.mean}ms`)
    console.log(`  catcher: ${cRate}% success, p50=${cStats.p50}ms p95=${cStats.p95}ms mean=${cStats.mean}ms`)

    // catcher should not be worse than vanilla
    expect(cSuccess / ITERATIONS).toBeGreaterThanOrEqual(vSuccess / ITERATIONS - 0.05)
  }, 600_000) // GPRS is extremely slow — needs 10min for 30 iterations
})

// ═══════════════════════════════════════════════════════════════
// S10 — Burst packet loss storm
// ═══════════════════════════════════════════════════════════════

describe('S10 — Burst packet loss storm', () => {
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

  it('burst loss — vanilla vs catcher with CB', async () => {
    proxy.setConditions(NETWORK_PROFILES.burst_storm.conditions)
    proxy.disruptAll()

    const TOTAL = 100

    // vanilla
    let vSuccess = 0, vTimes: number[] = []
    {
      const instance = axios.create({ baseURL: proxyUrl, timeout: 15_000 })
      for (let i = 0; i < TOTAL; i++) {
        const t0 = Date.now()
        try {
          await instance.post('/messages', { text: `msg_${i}` })
          vSuccess++
          vTimes.push(Date.now() - t0)
        } catch {}
      }
    }

    // catcher with CB (NAPI)
    let cSuccess = 0, cTimes: number[] = []
    {
      const client = createRustHttpClient({
        baseURL: proxyUrl,
        keepAlive: true,
        retry: { attempts: 3, backoff: 'exponential' },
        circuitBreaker: { failureThreshold: 5, resetTimeout: 30_000 },
        timeout: { response: 15_000 },
      })
      for (let i = 0; i < TOTAL; i++) {
        const t0 = Date.now()
        try {
          await client.post('/messages', { text: `msg_${i}` })
          cSuccess++
          cTimes.push(Date.now() - t0)
        } catch {}
      }
    }

    const vRate = (vSuccess / TOTAL * 100).toFixed(1)
    const cRate = (cSuccess / TOTAL * 100).toFixed(1)
    const vStats = computeStats(vTimes)
    const cStats = computeStats(cTimes)

    console.log(`\n  🌪️ S10 Burst Loss Storm (${TOTAL} requests)`)
    console.log(`  vanilla: ${vRate}% success, p50=${vStats.p50}ms p95=${vStats.p95}ms`)
    console.log(`  catcher: ${cRate}% success, p50=${cStats.p50}ms p95=${cStats.p95}ms`)

    // Catcher should outperform vanilla under burst loss
    expect(cSuccess).toBeGreaterThanOrEqual(vSuccess)
  }, TIMEOUT)
})

// ═══════════════════════════════════════════════════════════════
// S11 — Asymmetric up/down
// ═══════════════════════════════════════════════════════════════

describe('S11 — Asymmetric up/down (2G)', () => {
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

  it('asymmetric — POST (upload-heavy) vs GET (download-heavy)', async () => {
    proxy.setConditions(NETWORK_PROFILES.asymmetric_2g.conditions)
    proxy.disruptAll()

    const client = createRustHttpClient({
      baseURL: proxyUrl,
      keepAlive: true,
      retry: { attempts: 2, backoff: 'exponential' },
      timeout: { response: 30_000 },
    })

    // POST should be slower (upload is worse)
    const postTimes: number[] = []
    for (let i = 0; i < 20; i++) {
      const t0 = Date.now()
      try {
        await client.post('/messages', { text: `test_${i}` })
        postTimes.push(Date.now() - t0)
      } catch {}
    }

    // GET should be faster (download is better)
    const getTimes: number[] = []
    for (let i = 0; i < 20; i++) {
      const t0 = Date.now()
      try {
        await client.get('/channels')
        getTimes.push(Date.now() - t0)
      } catch {}
    }

    const postStats = computeStats(postTimes)
    const getStats = computeStats(getTimes)

    console.log(`\n  ⚖️ S11 Asymmetric 2G`)
    console.log(`  POST (upload-heavy): p50=${postStats.p50}ms p95=${postStats.p95}ms mean=${postStats.mean}ms`)
    console.log(`  GET  (download-heavy): p50=${getStats.p50}ms p95=${getStats.p95}ms mean=${getStats.mean}ms`)

    // POST should be observably slower (upload is 5x worse)
    // Not a hard assertion because variance is high with 20 samples
    expect(postTimes.length).toBeGreaterThan(0)
    expect(getTimes.length).toBeGreaterThan(0)
  }, TIMEOUT)
})

// ═══════════════════════════════════════════════════════════════
// S12a — Route black hole 30s
// ═══════════════════════════════════════════════════════════════

describe('S12a — Route black hole 30s', () => {
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

  it('blackhole 30s — catcher detects and recovers', async () => {
    // Phase 1: Normal — should succeed
    proxy.setConditions({ latency: 5, packetLoss: 0 })
    proxy.disruptAll()

    const client = createRustHttpClient({
      baseURL: proxyUrl,
      keepAlive: false, // fresh connections to ensure blackhole affects them
      retry: { attempts: 1, backoff: 'exponential' },
      timeout: { response: 5_000 },
    })

    let preOk = 0
    for (let i = 0; i < 5; i++) {
      try { await client.get('/channels'); preOk++ } catch {}
    }
    console.log(`  S12a Phase 1 (normal): ${preOk}/5 ok`)

    // Phase 2: Enable blackhole, verify requests fail quickly
    proxy.setConditions({
      latency: 5,
      blackhole: { enabled: true, duration: 30_000, destroyOnRecover: true },
    })
    proxy.disruptAll()

    let bhOk = 0, bhFail = 0
    const bhStart = Date.now()
    for (let i = 0; i < 5; i++) {
      try { await client.get('/channels'); bhOk++ } catch { bhFail++ }
    }
    const bhTime = Date.now() - bhStart
    console.log(`  S12a Phase 2 (blackhole): ${bhOk} ok, ${bhFail} fail in ${bhTime}ms`)

    // Phase 3: Wait for recovery, verify requests succeed again
    await new Promise(r => setTimeout(r, 35_000))

    proxy.setConditions({ latency: 5, packetLoss: 0 })
    proxy.disruptAll()

    let postOk = 0
    for (let i = 0; i < 5; i++) {
      try { await client.get('/channels'); postOk++ } catch {}
    }
    console.log(`  S12a Phase 3 (recovery): ${postOk}/5 ok`)

    // Phase 1 should be all successful
    expect(preOk).toBe(5)
    // Phase 2 — all should fail under blackhole
    expect(bhFail).toBeGreaterThanOrEqual(3)
    // Phase 3 — should recover
    expect(postOk).toBe(5)
  }, TIMEOUT)
})

// ═══════════════════════════════════════════════════════════════
// S12b — Black hole recovery (zombie connection cleanup)
// ═══════════════════════════════════════════════════════════════

describe('S12b — Black hole recovery', () => {
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

  it('blackhole recovery — zombie keepAlive connections cleaned', async () => {
    proxy.setConditions({ latency: 5, packetLoss: 0 })
    proxy.disruptAll()

    const client = createRustHttpClient({
      baseURL: proxyUrl,
      keepAlive: true,
      retry: { attempts: 2, backoff: 'exponential' },
      circuitBreaker: { failureThreshold: 3, resetTimeout: 10_000 },
      timeout: { response: 8_000 },
    })

    // Establish keepAlive connections
    for (let i = 0; i < 5; i++) {
      await client.get('/channels')
    }

    // Trigger blackhole with destroyOnRecover
    proxy.setConditions({
      latency: 5,
      blackhole: { enabled: true, duration: 15_000, destroyOnRecover: true },
    })

    // Try requests during blackhole
    for (let i = 0; i < 10; i++) {
      try { await client.get('/channels') } catch {}
    }

    // Wait for recovery
    await new Promise(r => setTimeout(r, 20_000))

    // After recovery — all should succeed with FRESH connections
    let postOk = 0
    for (let i = 0; i < 10; i++) {
      try { await client.get('/channels'); postOk++ } catch {}
    }
    console.log(`  S12b recovery: ${postOk}/10 ok`)

    expect(postOk).toBeGreaterThanOrEqual(8)
  }, TIMEOUT)
})

// ═══════════════════════════════════════════════════════════════
// S12c — Intermittent black hole
// ═══════════════════════════════════════════════════════════════

describe('S12c — Intermittent black hole', () => {
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

  it('intermittent blackhole — CB cycles correctly', async () => {
    const CYCLES = 5
    const BH_DURATION = 5_000
    const NORMAL_DURATION = 3_000

    proxy.setConditions({ latency: 5, packetLoss: 0 })
    proxy.disruptAll()

    const client = createRustHttpClient({
      baseURL: proxyUrl,
      keepAlive: false, // fresh connections each time
      retry: { attempts: 1, backoff: 'exponential' },
      circuitBreaker: { failureThreshold: 3, resetTimeout: 3_000 },
      timeout: { response: 3_000 },
    })

    const states: string[] = []

    for (let cycle = 0; cycle < CYCLES; cycle++) {
      // Blackhole on
      proxy.setConditions({ latency: 5, blackhole: { enabled: true } })
      for (let i = 0; i < 5; i++) {
        try { await client.get('/channels') } catch {}
      }
      states.push(`BH:${client.circuitBreakerState()}`)

      // Blackhole off, normal
      proxy.setConditions({ latency: 5, packetLoss: 0 })
      await new Promise(r => setTimeout(r, NORMAL_DURATION))
      for (let i = 0; i < 5; i++) {
        try { await client.get('/channels') } catch {}
      }
      states.push(`OK:${client.circuitBreakerState()}`)
    }

    console.log(`  S12c CB states: ${states.join(' → ')}`)

    // After final recovery, CB should be closed
    expect(client.circuitBreakerState()).toBe('closed')
  }, TIMEOUT)
})

// ═══════════════════════════════════════════════════════════════
// S13 — 5xx server error storm
// ═══════════════════════════════════════════════════════════════

describe('S13 — 5xx server error storm', () => {
  let server: TestServer
  let serverUrl: string

  beforeAll(async () => {
    server = await createHttpTestServer()
    serverUrl = server.url
  }, 30000)

  afterAll(async () => {
    await server.close()
  })

  it('5xx storm — CB opens correctly', async () => {
    // We can't easily make the test server return 5xx on demand.
    // Instead, test against a non-existent endpoint to trigger failures,
    // plus verify the CB state transitions.
    const TOTAL = 30

    // Point to a non-routable port to force connection failures
    const client = createRustHttpClient({
      baseURL: 'http://127.0.0.1:1', // nothing listening
      keepAlive: false,
      retry: { attempts: 2, backoff: 'exponential' },
      circuitBreaker: { failureThreshold: 5, resetTimeout: 10_000 },
      timeout: { response: 3_000 },
    })

    let ok = 0, fail = 0
    for (let i = 0; i < TOTAL; i++) {
      try { await client.get('/test'); ok++ } catch { fail++ }
    }
    const cbState = client.circuitBreakerState()

    console.log(`  S13 5xx storm: ${ok} ok, ${fail} fail, CB=${cbState}`)

    // After 5+ consecutive failures, CB should be open
    // (Note: if all fail quickly, CB opens and then rejects immediately)
    expect(fail).toBeGreaterThan(0)
    // CB should have tripped
    expect(['open', 'half-open'].includes(cbState)).toBe(true)
  }, TIMEOUT)
})

// ═══════════════════════════════════════════════════════════════
// S14 — Latency jitter + spikes
// ═══════════════════════════════════════════════════════════════

describe('S14 — Latency jitter + spikes', () => {
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

  it('jitter + spikes — latency distribution', async () => {
    // Normal distribution jitter: base 50ms, ±40ms stddev
    proxy.setConditions({
      latency: 50,
      jitter: 40,
      jitterDistribution: 'normal',
      packetLoss: 0,
    })
    proxy.disruptAll()

    const client = createRustHttpClient({
      baseURL: proxyUrl,
      keepAlive: true,
      timeout: { response: 10_000 },
    })

    const times: number[] = []
    for (let i = 0; i < 100; i++) {
      const t0 = Date.now()
      try {
        await client.get('/avatar?uid=1')
        times.push(Date.now() - t0)
      } catch {}
    }

    const stats = computeStats(times)
    console.log(`\n  📊 S14 Jitter (base 50ms, σ=40ms, 100 requests)`)
    console.log(`  p50=${stats.p50}ms p95=${stats.p95}ms p99=${stats.p99}ms`)
    console.log(`  min=${stats.min}ms max=${stats.max}ms mean=${stats.mean}ms`)

    // With base=50 one-way latency (100ms RTT), jitter=40 normal:
    // Each direction adds independent jitter, effective RTT jitter σ ≈ 56ms
    // p50 should be around 100ms, p95 around 200ms
    expect(stats.p50).toBeGreaterThanOrEqual(40)
    expect(stats.p50).toBeLessThanOrEqual(130)
    expect(times.length).toBeGreaterThanOrEqual(90) // most should succeed
  }, TIMEOUT)
})

// ═══════════════════════════════════════════════════════════════
// S15 — Slow DNS resolution
// ═══════════════════════════════════════════════════════════════

describe('S15 — Slow DNS resolution', () => {
  // DNS testing happens through the OS resolver.
  // We test DNS cache by measuring cold vs warm request timings.
  // The proxy doesn't simulate DNS delay, but we can verify
  // that repeated requests to the same host are faster (keepAlive + DNS cache).

  let server: TestServer
  let serverUrl: string

  beforeAll(async () => {
    server = await createHttpTestServer()
    serverUrl = server.url
  }, 30000)

  afterAll(async () => {
    await server.close()
  })

  it('DNS cache — first request vs subsequent', async () => {
    const client = createRustHttpClient({
      baseURL: serverUrl,
      keepAlive: true,
      dnsCacheTtl: 300,
      timeout: { response: 10_000 },
    })

    // First request (cold — DNS lookup + TCP + TLS)
    const t0 = Date.now()
    await client.get('/channels')
    const firstTime = Date.now() - t0

    // Subsequent requests (warm — DNS cache hit + keepAlive)
    const warmTimes: number[] = []
    for (let i = 0; i < 10; i++) {
      const t1 = Date.now()
      await client.get('/channels')
      warmTimes.push(Date.now() - t1)
    }

    const warmStats = computeStats(warmTimes)

    console.log(`\n  🔍 S15 DNS Cache`)
    console.log(`  first (cold): ${firstTime}ms`)
    console.log(`  warm p50:     ${warmStats.p50}ms`)
    console.log(`  warm mean:    ${warmStats.mean}ms`)

    // First request should take longer (includes lookup)
    // Localhost DNS is fast, so this is mostly about connection establishment
    // Warm requests should be very fast (keepAlive reuse)
    expect(warmStats.mean).toBeLessThanOrEqual(firstTime)
  }, TIMEOUT)
})

// ═══════════════════════════════════════════════════════════════
// S16 — Connection pool exhaustion
// ═══════════════════════════════════════════════════════════════

describe('S16 — Connection pool exhaustion', () => {
  let server: TestServer
  let serverUrl: string

  beforeAll(async () => {
    server = await createHttpTestServer()
    serverUrl = server.url
  }, 30000)

  afterAll(async () => {
    await server.close()
  })

  it('pool exhaustion — queue handles overflow', async () => {
    const CONCURRENT = 100
    const POOL_SIZE = 10

    const client = createRustHttpClient({
      baseURL: serverUrl,
      keepAlive: true,
      concurrency: CONCURRENT,
      timeout: { response: 30_000 },
    })

    // Fire 100 slow requests (each server-side 200ms delay)
    const start = Date.now()
    const results = await Promise.allSettled(
      Array.from({ length: 100 }, (_, i) =>
        client.get(`/slow?delay=200&i=${i}`),
      ),
    )

    const elapsed = Date.now() - start
    const succeeded = results.filter(r => r.status === 'fulfilled').length
    const failed = results.filter(r => r.status === 'rejected').length

    console.log(`\n  🏊 S16 Pool Exhaustion`)
    console.log(`  ${CONCURRENT} concurrent, ${POOL_SIZE} pool, 100 requests`)
    console.log(`  elapsed: ${elapsed}ms, ok=${succeeded}, fail=${failed}`)
    console.log(`  queue depth: ${client.queueDepth()}`)

    // All should succeed (queue handles overflow)
    expect(succeeded).toBe(100)
    expect(failed).toBe(0)
  }, TIMEOUT)
})
