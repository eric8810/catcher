/**
 * High-concurrency throughput benchmark — catcher vs vanilla axios.
 *
 * Three scenarios:
 *   A) Direct throughput (no proxy) — pure req/sec + latency distribution
 *   B) Weak network resilience (through proxy) — success rate + retry effectiveness
 *   C) Connection efficiency — TCP socket reuse rate
 *
 * Configurable via env:
 *   THROUGHPUT_REQUESTS=1000    total requests for scenario A
 *   THROUGHPUT_CONCURRENCY=100  parallel requests for scenario A
 *   WEAK_REQUESTS=100           total requests for scenario B
 *   WEAK_CONCURRENCY=20         parallel requests for scenario B
 *
 * Usage:
 *   THROUGHPUT_REQUESTS=500 THROUGHPUT_CONCURRENCY=50 pnpm vitest run test/benchmark/throughput.bench.ts
 */

import http from 'node:http'
import axios from 'axios'
import { describe, it, expect, beforeAll, afterAll } from 'vitest'
import { createHttpClient } from '@eric8810/catcher-http'
import { createHttpTestServer, type TestServer } from '../servers/http-server.js'
import { createNetworkProxy, type NetworkProxy } from '../network/proxy.js'
import { NETWORK_PROFILES } from '../network/presets.js'

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

const TIMEOUT = 300_000

const THROUGHPUT_REQUESTS = parseInt(process.env.THROUGHPUT_REQUESTS ?? '500', 10)
const THROUGHPUT_CONCURRENCY = parseInt(process.env.THROUGHPUT_CONCURRENCY ?? '50', 10)
// Standard benchmark: 500 requests. Reduce to 100 for CI via env var.
const WEAK_REQUESTS = parseInt(process.env.WEAK_REQUESTS ?? '500', 10)
const WEAK_CONCURRENCY = parseInt(process.env.WEAK_CONCURRENCY ?? '50', 10)

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

interface LatencyStats {
  p50: number
  p95: number
  p99: number
  mean: number
  min: number
  max: number
}

function computeStats(times: number[]): LatencyStats {
  if (times.length === 0) return { p50: 0, p95: 0, p99: 0, mean: 0, min: 0, max: 0 }
  const sorted = [...times].sort((a, b) => a - b)
  const p = (pct: number) => sorted[Math.ceil((pct / 100) * sorted.length) - 1] ?? sorted[sorted.length - 1]
  return {
    p50: p(50),
    p95: p(95),
    p99: p(99),
    mean: Math.round(times.reduce((a, b) => a + b, 0) / times.length),
    min: sorted[0],
    max: sorted[sorted.length - 1],
  }
}

/** Run a batch of requests with bounded concurrency */
async function runBatch(
  fn: (i: number) => Promise<{ time: number; success: boolean; error?: string }>,
  total: number,
  concurrency: number,
): Promise<{ times: number[]; failures: number; errors: string[] }> {
  const times: number[] = []
  const errors: string[] = []
  let failures = 0
  let idx = 0

  const worker = async () => {
    while (idx < total) {
      const i = idx++
      try {
        const result = await fn(i)
        if (result.success) {
          times.push(result.time)
        } else {
          failures++
          if (result.error) errors.push(result.error)
        }
      } catch (e: any) {
        failures++
        errors.push(e.message ?? String(e))
      }
    }
  }

  const workers = Array.from({ length: concurrency }, () => worker())
  await Promise.all(workers)

  return { times, failures, errors }
}

// ---------------------------------------------------------------------------
// Scenario A: Direct throughput (no proxy)
// ---------------------------------------------------------------------------

describe('Scenario A — direct throughput', () => {
  let server: TestServer
  let serverUrl: string

  beforeAll(async () => {
    server = await createHttpTestServer()
    serverUrl = server.url
  }, 30_000)

  afterAll(async () => {
    await server.close()
  })

  it(`vanilla — ${THROUGHPUT_REQUESTS} requests, concurrency=${THROUGHPUT_CONCURRENCY}`, async () => {
    const vanillaAxios = axios.create({
      baseURL: serverUrl,
      timeout: 10_000,
    })

    const start = Date.now()
    const { times, failures } = await runBatch(
      async (i) => {
        const t0 = Date.now()
        try {
          await vanillaAxios.get(`/channels/${i % 20}/messages?pageSize=10`)
          return { time: Date.now() - t0, success: true }
        } catch (e: any) {
          return { time: Date.now() - t0, success: false, error: e.message }
        }
      },
      THROUGHPUT_REQUESTS,
      THROUGHPUT_CONCURRENCY,
    )
    const elapsed = Date.now() - start

    const stats = computeStats(times)
    console.log(`\n  ┌─ vanilla ─────────────────────────────`)
    console.log(`  │ total:    ${elapsed}ms`)
    console.log(`  │ req/sec:  ${(THROUGHPUT_REQUESTS / (elapsed / 1000)).toFixed(1)}`)
    console.log(`  │ failures: ${failures}/${THROUGHPUT_REQUESTS}`)
    console.log(`  │ p50:      ${stats.p50}ms`)
    console.log(`  │ p95:      ${stats.p95}ms`)
    console.log(`  │ p99:      ${stats.p99}ms`)
    console.log(`  │ mean:     ${stats.mean}ms`)
    console.log(`  └──────────────────────────────────────\n`)

    expect(failures).toBe(0)
  }, TIMEOUT)

  it(`catcher — ${THROUGHPUT_REQUESTS} requests, concurrency=${THROUGHPUT_CONCURRENCY}`, async () => {
    const client = createHttpClient({
      baseURL: serverUrl,
      keepAlive: true,
      concurrency: THROUGHPUT_CONCURRENCY,
      timeout: 10_000,
    })

    const start = Date.now()
    const { times, failures } = await runBatch(
      async (i) => {
        const t0 = Date.now()
        try {
          await client.get(`/channels/${i % 20}/messages?pageSize=10`)
          return { time: Date.now() - t0, success: true }
        } catch (e: any) {
          return { time: Date.now() - t0, success: false, error: e.message }
        }
      },
      THROUGHPUT_REQUESTS,
      THROUGHPUT_CONCURRENCY,
    )
    const elapsed = Date.now() - start

    const stats = computeStats(times)
    console.log(`\n  ┌─ catcher ────────────────────────────`)
    console.log(`  │ total:    ${elapsed}ms`)
    console.log(`  │ req/sec:  ${(THROUGHPUT_REQUESTS / (elapsed / 1000)).toFixed(1)}`)
    console.log(`  │ failures: ${failures}/${THROUGHPUT_REQUESTS}`)
    console.log(`  │ queue:    ${client.queueDepth()}`)
    console.log(`  │ cb:       ${client.circuitBreakerState()}`)
    console.log(`  │ p50:      ${stats.p50}ms`)
    console.log(`  │ p95:      ${stats.p95}ms`)
    console.log(`  │ p99:      ${stats.p99}ms`)
    console.log(`  │ mean:     ${stats.mean}ms`)
    console.log(`  └──────────────────────────────────────\n`)

    expect(failures).toBe(0)
  }, TIMEOUT)
})

// ---------------------------------------------------------------------------
// Scenario B: Weak network resilience (through proxy)
// ---------------------------------------------------------------------------

for (const [key, profile] of Object.entries(NETWORK_PROFILES)) {
  if (!['weak', 'veryWeak'].includes(key)) continue

  describe(`Scenario B — ${profile.emoji} ${profile.name}`, () => {
    let server: TestServer
    let proxy: NetworkProxy
    let proxyUrl: string

    beforeAll(async () => {
      server = await createHttpTestServer()
      proxy = createNetworkProxy(server.port)
      await proxy.start()
      proxyUrl = `http://127.0.0.1:${proxy.port}`
    }, 30_000)

    afterAll(async () => {
      await proxy.stop()
      await server.close()
    })

    it(`vanilla — ${WEAK_REQUESTS} requests, concurrency=${WEAK_CONCURRENCY}`, async () => {
      proxy.setConditions(profile.conditions)
      proxy.disruptAll()

      const vanillaAxios = axios.create({
        baseURL: proxyUrl,
        timeout: 15_000,
      })

      const start = Date.now()
      const { times, failures, errors } = await runBatch(
        async (i) => {
          const t0 = Date.now()
          try {
            await vanillaAxios.post('/messages', { text: `msg_${i}` })
            return { time: Date.now() - t0, success: true }
          } catch (e: any) {
            return { time: Date.now() - t0, success: false, error: e.code ?? e.message }
          }
        },
        WEAK_REQUESTS,
        WEAK_CONCURRENCY,
      )
      const elapsed = Date.now() - start

      const stats = computeStats(times)
      const successRate = ((WEAK_REQUESTS - failures) / WEAK_REQUESTS * 100).toFixed(1)
      console.log(`\n  ┌─ vanilla (${profile.name}) ──────────`)
      console.log(`  │ total:       ${elapsed}ms`)
      console.log(`  │ successRate: ${successRate}%`)
      console.log(`  │ p50:         ${stats.p50}ms`)
      console.log(`  │ p95:         ${stats.p95}ms`)
      console.log(`  │ mean:        ${stats.mean}ms`)
      if (errors.length) console.log(`  │ top errors:  ${[...new Set(errors)].slice(0, 3).join(', ')}`)
      console.log(`  └──────────────────────────────────────\n`)
    }, TIMEOUT)

    it(`catcher with retry — ${WEAK_REQUESTS} requests, concurrency=${WEAK_CONCURRENCY}`, async () => {
      proxy.setConditions(profile.conditions)
      proxy.disruptAll()

      const client = createHttpClient({
        baseURL: proxyUrl,
        keepAlive: true,
        retry: { attempts: 3, backoff: 'exponential' },
        concurrency: WEAK_CONCURRENCY,
        timeout: 30_000,
      })

      const start = Date.now()
      const { times, failures, errors } = await runBatch(
        async (i) => {
          const t0 = Date.now()
          try {
            await client.post('/messages', { text: `msg_${i}` })
            return { time: Date.now() - t0, success: true }
          } catch (e: any) {
            return { time: Date.now() - t0, success: false, error: e.code ?? e.message }
          }
        },
        WEAK_REQUESTS,
        WEAK_CONCURRENCY,
      )
      const elapsed = Date.now() - start

      const stats = computeStats(times)
      const successRate = ((WEAK_REQUESTS - failures) / WEAK_REQUESTS * 100).toFixed(1)
      console.log(`\n  ┌─ catcher (${profile.name}) ──────────`)
      console.log(`  │ total:       ${elapsed}ms`)
      console.log(`  │ successRate: ${successRate}%`)
      console.log(`  │ queue:       ${client.queueDepth()}`)
      console.log(`  │ cb:          ${client.circuitBreakerState()}`)
      console.log(`  │ p50:         ${stats.p50}ms`)
      console.log(`  │ p95:         ${stats.p95}ms`)
      console.log(`  │ mean:        ${stats.mean}ms`)
      if (errors.length) console.log(`  │ top errors:  ${[...new Set(errors)].slice(0, 3).join(', ')}`)
      console.log(`  └──────────────────────────────────────\n`)
    }, TIMEOUT)
  })
}

// ---------------------------------------------------------------------------
// Scenario C: Connection efficiency (track socket count)
// ---------------------------------------------------------------------------

describe('Scenario C — connection efficiency', () => {
  let server: TestServer
  let serverUrl: string

  beforeAll(async () => {
    server = await createHttpTestServer()
    serverUrl = server.url
  }, 30_000)

  afterAll(async () => {
    await server.close()
  })

  it(`vanilla — 200 sequential, 1 at a time (no keepAlive)`, async () => {
    const vanillaAxios = axios.create({
      baseURL: serverUrl,
      timeout: 5_000,
      httpsAgent: false as any, // no keepAlive → new connection each time
    })

    const times: number[] = []
    let failures = 0

    const start = Date.now()
    for (let i = 0; i < 200; i++) {
      const t0 = Date.now()
      try {
        await vanillaAxios.get('/avatar?uid=' + (i % 50))
        times.push(Date.now() - t0)
      } catch {
        failures++
      }
    }
    const elapsed = Date.now() - start

    const stats = computeStats(times)
    console.log(`\n  ┌─ vanilla (sequential, no keepAlive) ─`)
    console.log(`  │ total:    ${elapsed}ms`)
    console.log(`  │ avg/req:  ${(elapsed / 200).toFixed(1)}ms`)
    console.log(`  │ failures: ${failures}/200`)
    console.log(`  │ p50:      ${stats.p50}ms`)
    console.log(`  │ mean:     ${stats.mean}ms`)
    console.log(`  │ expected: ~200 TCP connections`)
    console.log(`  └──────────────────────────────────────\n`)

    expect(failures).toBeLessThan(5)
  }, TIMEOUT)

  it(`catcher — 200 sequential, 1 at a time (keepAlive)`, async () => {
    const client = createHttpClient({
      baseURL: serverUrl,
      keepAlive: true,
      timeout: 5_000,
    })

    const times: number[] = []
    let failures = 0

    const start = Date.now()
    for (let i = 0; i < 200; i++) {
      const t0 = Date.now()
      try {
        await client.get('/avatar?uid=' + (i % 50))
        times.push(Date.now() - t0)
      } catch {
        failures++
      }
    }
    const elapsed = Date.now() - start

    const stats = computeStats(times)
    console.log(`\n  ┌─ catcher (sequential, keepAlive) ────`)
    console.log(`  │ total:    ${elapsed}ms`)
    console.log(`  │ avg/req:  ${(elapsed / 200).toFixed(1)}ms`)
    console.log(`  │ failures: ${failures}/200`)
    console.log(`  │ p50:      ${stats.p50}ms`)
    console.log(`  │ mean:     ${stats.mean}ms`)
    console.log(`  │ expected: ~1 TCP connection (reused)`)
    console.log(`  └──────────────────────────────────────\n`)

    expect(failures).toBeLessThan(5)
  }, TIMEOUT)
})

// ---------------------------------------------------------------------------
// Scenario D: Mixed workload (IM simulation)
// ---------------------------------------------------------------------------

describe('Scenario D — mixed IM workload', () => {
  let server: TestServer
  let serverUrl: string

  beforeAll(async () => {
    server = await createHttpTestServer()
    serverUrl = server.url
  }, 30_000)

  afterAll(async () => {
    await server.close()
  })

  const MIXED_REQUESTS = parseInt(process.env.MIXED_REQUESTS ?? '300', 10)
  const MIXED_CONCURRENCY = parseInt(process.env.MIXED_CONCURRENCY ?? '30', 10)

  // Simulates: 40% GET channels, 30% GET messages, 20% POST messages, 10% GET users
  const workload = (i: number): { method: string; path: string; body?: any } => {
    const bucket = i % 10
    if (bucket < 4) return { method: 'GET', path: '/channels' }
    if (bucket < 7) return { method: 'GET', path: `/channels/${i % 20}/messages?pageSize=20` }
    if (bucket < 9) return { method: 'POST', path: '/messages', body: { text: `msg_${i}` } }
    return { method: 'GET', path: `/users/${i % 50}` }
  }

  it('vanilla mixed workload', async () => {
    const vanillaAxios = axios.create({
      baseURL: serverUrl,
      timeout: 10_000,
    })

    const start = Date.now()
    const { times, failures } = await runBatch(
      async (i) => {
        const { method, path, body } = workload(i)
        const t0 = Date.now()
        try {
          if (method === 'POST') {
            await vanillaAxios.post(path, body)
          } else {
            await vanillaAxios.get(path)
          }
          return { time: Date.now() - t0, success: true }
        } catch (e: any) {
          return { time: Date.now() - t0, success: false, error: e.message }
        }
      },
      MIXED_REQUESTS,
      MIXED_CONCURRENCY,
    )
    const elapsed = Date.now() - start

    const stats = computeStats(times)
    const reqPerSec = (MIXED_REQUESTS / (elapsed / 1000)).toFixed(1)
    console.log(`\n  ┌─ vanilla (mixed IM workload) ────────`)
    console.log(`  │ total:    ${elapsed}ms`)
    console.log(`  │ req/sec:  ${reqPerSec}`)
    console.log(`  │ failures: ${failures}/${MIXED_REQUESTS}`)
    console.log(`  │ p50:      ${stats.p50}ms`)
    console.log(`  │ p95:      ${stats.p95}ms`)
    console.log(`  │ p99:      ${stats.p99}ms`)
    console.log(`  └──────────────────────────────────────\n`)

    expect(failures).toBeLessThan(MIXED_REQUESTS * 0.05)
  }, TIMEOUT)

  it('catcher mixed workload (priority queue)', async () => {
    const client = createHttpClient({
      baseURL: serverUrl,
      keepAlive: true,
      concurrency: MIXED_CONCURRENCY,
      timeout: 10_000,
    })

    const start = Date.now()
    const { times, failures } = await runBatch(
      async (i) => {
        const { method, path, body } = workload(i)
        const t0 = Date.now()
        try {
          if (method === 'POST') {
            await client.post(path, body)
          } else {
            await client.get(path)
          }
          return { time: Date.now() - t0, success: true }
        } catch (e: any) {
          return { time: Date.now() - t0, success: false, error: e.message }
        }
      },
      MIXED_REQUESTS,
      MIXED_CONCURRENCY,
    )
    const elapsed = Date.now() - start

    const stats = computeStats(times)
    const reqPerSec = (MIXED_REQUESTS / (elapsed / 1000)).toFixed(1)
    console.log(`\n  ┌─ catcher (mixed IM workload) ────────`)
    console.log(`  │ total:    ${elapsed}ms`)
    console.log(`  │ req/sec:  ${reqPerSec}`)
    console.log(`  │ failures: ${failures}/${MIXED_REQUESTS}`)
    console.log(`  │ queue:    ${client.queueDepth()}`)
    console.log(`  │ cb:       ${client.circuitBreakerState()}`)
    console.log(`  │ p50:      ${stats.p50}ms`)
    console.log(`  │ p95:      ${stats.p95}ms`)
    console.log(`  │ p99:      ${stats.p99}ms`)
    console.log(`  └──────────────────────────────────────\n`)

    expect(failures).toBeLessThan(MIXED_REQUESTS * 0.05)
  }, TIMEOUT)
})
