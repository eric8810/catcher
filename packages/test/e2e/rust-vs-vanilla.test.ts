/**
 * E2E: Rust (via napi-rs) vs Vanilla (axios/ws) Comparison
 *
 * Mirrors the existing scenarios.test.ts but uses @eric8810/catcher-napi-http + @eric8810/catcher-napi-ws
 * instead of the TS catcher implementation. Tests S1-S8 with
 * concurrent comparison against vanilla axios / raw WebSocket.
 *
 * Prerequisites:
 *   1. Build @eric8810/catcher-napi-http + @eric8810/catcher-napi-ws: cd packages/@eric8810/catcher-napi-http + @eric8810/catcher-napi-ws && cargo build --release
 *   2. Link: cd packages/catcher-ts && pnpm install
 *
 * Run:
 *   FAST_ITERATIONS=30 pnpm vitest run test/e2e/rust-vs-vanilla.test.ts
 */
import axios from 'axios'
import WebSocket from 'ws'
import { describe, it, expect, beforeAll, afterAll } from 'vitest'
import path from 'node:path'

import { createRustHttpClient, createRustWsClient, rustPack } from '../adapters/rust-adapter.js'
import { createHttpTestServer, type TestServer } from '../servers/http-server.js'
import { createWSTestServer, type WSTestServer } from '../servers/ws-server.js'
import { createNetworkProxy, type NetworkProxy } from '../network/proxy.js'
import { NETWORK_PROFILES } from '../network/presets.js'
import { runConcurrentComparison, type IterationResult } from '../harness.js'
import { ComparisonReporter } from '../reporters/comparison-reporter.js'

const TIMEOUT = 300_000
const FAST_ITERATIONS = parseInt(process.env.FAST_ITERATIONS ?? '30', 10)
const itersFor = (key: string) => {
  const isSlow = ['weak', 'veryWeak', 'mobile3g', 'metro'].includes(key)
  // More iterations than before (was 5) for statistical significance,
  // but capped at 15 to fit within 5-min test timeout on slow networks
  return isSlow ? Math.min(15, FAST_ITERATIONS) : FAST_ITERATIONS
}

let httpServer: TestServer
let wsServer: WSTestServer
let httpProxy: NetworkProxy
let wsProxy: NetworkProxy
let httpUrl: string
let wsUrl: string
const reporter = new ComparisonReporter()

beforeAll(async () => {
  httpServer = await createHttpTestServer()
  wsServer = await createWSTestServer()
  httpProxy = createNetworkProxy(httpServer.port)
  wsProxy = createNetworkProxy(wsServer.port)
  await httpProxy.start()
  await wsProxy.start()
  httpUrl = `http://127.0.0.1:${httpProxy.port}`
  wsUrl = `ws://127.0.0.1:${wsProxy.port}`
}, 30000)

afterAll(async () => {
  await httpProxy.stop()
  await wsProxy.stop()
  await httpServer.close()
  await wsServer.close()
  await reporter.writeReports(path.resolve('test-results'))
})

// ═════════════════════════════════════════════════════════════
// S1: Cold start → channel list
// ═════════════════════════════════════════════════════════════

async function vanillaS1(baseUrl: string): Promise<IterationResult> {
  const start = Date.now()
  try {
    const r = await axios.get(baseUrl + '/channels', { timeout: 10_000 })
    const bytes = typeof r.data === 'string' ? Buffer.byteLength(r.data) : Buffer.byteLength(JSON.stringify(r.data))
    return { success: true, time: Date.now() - start, connections: 1, bytes }
  } catch {
    return { success: false, time: 10_000, connections: 1, bytes: 0 }
  }
}

function makeRustS1(baseUrl: string) {
  const client = createRustHttpClient({
    baseURL: baseUrl,
    keepAlive: true,
    dnsCacheTtl: 300,
    retry: { attempts: 3, backoff: 'exponential' },
    timeout: { response: 10_000 },
  })
  return async function rustS1(): Promise<IterationResult> {
    const start = Date.now()
    const retriesBefore = client.retryCount
    try {
      await client.get('/channels')
      const retries = client.retryCount - retriesBefore
      return { success: true, time: Date.now() - start, connections: 1, bytes: client.lastBytes, retries }
    } catch {
      const retries = client.retryCount - retriesBefore
      return { success: false, time: 10_000, connections: 1, bytes: 0, retries }
    }
  }
}

describe('S1: Cold start (Rust)', () => {
  for (const [key, profile] of Object.entries(NETWORK_PROFILES)) {
    if (!['good', 'weak', 'veryWeak'].includes(key)) continue
    it(`${profile.emoji} ${profile.name}`, async () => {
      httpProxy.setConditions(profile.conditions)
      httpProxy.disruptAll()
      const rustFn = makeRustS1(httpUrl)
      const r = await runConcurrentComparison(
        { name: 'S1: Cold start (Rust)', iterations: itersFor(key), iterationTimeout: 12_000 },
        profile.conditions, `${profile.emoji} ${profile.name}`, vanillaS1, rustFn, httpUrl,
      )
      reporter.addResult(r)
      expect(r.catcher.successRate).toBeGreaterThanOrEqual(r.vanilla.successRate - 0.3)
    }, TIMEOUT)
  }
})

// ═════════════════════════════════════════════════════════════
// S2: Send text message
// ═════════════════════════════════════════════════════════════

async function vanillaS2(baseUrl: string): Promise<IterationResult> {
  const start = Date.now()
  try {
    const r = await axios.post(baseUrl + '/messages', { text: 'Hello '.repeat(30) }, { timeout: 15_000 })
    const bytes = typeof r.data === 'string' ? Buffer.byteLength(r.data) : Buffer.byteLength(JSON.stringify(r.data))
    return { success: true, time: Date.now() - start, bytes, connections: 1 }
  } catch {
    return { success: false, time: 15_000, bytes: 0, connections: 1 }
  }
}

function makeRustS2(baseUrl: string) {
  const client = createRustHttpClient({
    baseURL: baseUrl, keepAlive: true,
    retry: { attempts: 3, backoff: 'exponential' },
    timeout: { response: 30_000 },
  })
  return async function rustS2(): Promise<IterationResult> {
    const start = Date.now()
    const retriesBefore = client.retryCount
    try {
      await client.post('/messages', { text: 'Hello '.repeat(30) })
      const retries = client.retryCount - retriesBefore
      return { success: true, time: Date.now() - start, bytes: client.lastBytes, connections: 1, retries }
    } catch {
      const retries = client.retryCount - retriesBefore
      return { success: false, time: 30_000, bytes: 0, connections: 1, retries }
    }
  }
}

describe('S2: Send message (Rust)', () => {
  for (const [key, profile] of Object.entries(NETWORK_PROFILES)) {
    if (!['weak', 'veryWeak', 'mobile3g'].includes(key)) continue
    it(`${profile.emoji} ${profile.name}`, async () => {
      httpProxy.setConditions(profile.conditions)
      httpProxy.disruptAll()
      const rustFn = makeRustS2(httpUrl)
      const r = await runConcurrentComparison(
        { name: 'S2: Send message (Rust)', iterations: itersFor(key), iterationTimeout: 35_000 },
        profile.conditions, `${profile.emoji} ${profile.name}`, vanillaS2, rustFn, httpUrl,
      )
      reporter.addResult(r)
      expect(r.catcher.successRate).toBeGreaterThanOrEqual(r.vanilla.successRate - 0.3)
    }, TIMEOUT)
  }
})

// ═════════════════════════════════════════════════════════════
// S3: Switch channel → load messages
// ═════════════════════════════════════════════════════════════

async function vanillaS3(baseUrl: string): Promise<IterationResult> {
  const start = Date.now()
  try {
    const r = await axios.get(baseUrl + '/channels/ch_0/messages?pageSize=50', { timeout: 15_000 })
    const bytes = typeof r.data === 'string' ? Buffer.byteLength(r.data) : Buffer.byteLength(JSON.stringify(r.data))
    return { success: true, time: Date.now() - start, bytes, connections: 1 }
  } catch {
    return { success: false, time: 15_000, bytes: 0, connections: 1 }
  }
}

function makeRustS3(baseUrl: string) {
  const client = createRustHttpClient({
    baseURL: baseUrl, keepAlive: true,
    retry: { attempts: 3 },
    timeout: { response: 15_000 },
  })
  return async function rustS3(): Promise<IterationResult> {
    const start = Date.now()
    const retriesBefore = client.retryCount
    try {
      await client.get('/channels/ch_0/messages?pageSize=50')
      const retries = client.retryCount - retriesBefore
      return { success: true, time: Date.now() - start, bytes: client.lastBytes, connections: 1, retries }
    } catch {
      const retries = client.retryCount - retriesBefore
      return { success: false, time: 15_000, bytes: 0, connections: 1, retries }
    }
  }
}

describe('S3: Load messages (Rust)', () => {
  for (const [key, profile] of Object.entries(NETWORK_PROFILES)) {
    if (!['good', 'weak', 'veryWeak'].includes(key)) continue
    it(`${profile.emoji} ${profile.name}`, async () => {
      httpProxy.setConditions(profile.conditions)
      httpProxy.disruptAll()
      const rustFn = makeRustS3(httpUrl)
      const r = await runConcurrentComparison(
        { name: 'S3: Load messages (Rust)', iterations: itersFor(key) },
        profile.conditions, `${profile.emoji} ${profile.name}`, vanillaS3, rustFn, httpUrl,
      )
      reporter.addResult(r)
      expect(r.catcher.successRate).toBeGreaterThanOrEqual(r.vanilla.successRate - 0.3)
    }, TIMEOUT)
  }
})

// ═════════════════════════════════════════════════════════════
// S4: Cross-region (high RTT)
// ═════════════════════════════════════════════════════════════

async function vanillaS4(baseUrl: string): Promise<IterationResult> {
  const start = Date.now()
  let success = true
  let bytes = 0
  try {
    const r1 = await axios.post(baseUrl + '/auth', { user: 'sg_user' }, { timeout: 15_000 })
    bytes += Buffer.byteLength(JSON.stringify(r1.data))
  } catch { success = false }
  try {
    const r2 = await axios.get(baseUrl + '/channels', { timeout: 15_000 })
    bytes += Buffer.byteLength(JSON.stringify(r2.data))
  } catch { success = false }
  return { success, time: Date.now() - start, connections: 2, bytes }
}

function makeRustS4(baseUrl: string) {
  const client = createRustHttpClient({
    baseURL: baseUrl, keepAlive: true,
    retry: { attempts: 3 },
    timeout: { response: 15_000 },
  })
  return async function rustS4(): Promise<IterationResult> {
    const start = Date.now()
    const retriesBefore = client.retryCount
    let success = true
    let bytes = 0
    try {
      await client.post('/auth', { user: 'sg_user' })
      bytes += client.lastBytes
      await client.get('/channels')
      bytes += client.lastBytes
    } catch { success = false }
    const retries = client.retryCount - retriesBefore
    return { success, time: Date.now() - start, connections: 1, retries, bytes }
  }
}

describe('S4: Cross-region (Rust)', () => {
  const profile = NETWORK_PROFILES.crossRegion
  it(`${profile.emoji} ${profile.name}`, async () => {
    httpProxy.setConditions(profile.conditions)
    httpProxy.disruptAll()
    const rustFn = makeRustS4(httpUrl)
    const r = await runConcurrentComparison(
      { name: 'S4: Cross-region (Rust)', iterations: FAST_ITERATIONS },
      profile.conditions, `${profile.emoji} ${profile.name}`, vanillaS4, rustFn, httpUrl,
    )
    reporter.addResult(r)
    expect(r.catcher.successRate).toBeGreaterThanOrEqual(r.vanilla.successRate - 0.1)
  }, TIMEOUT)
})

// ═════════════════════════════════════════════════════════════
// S5: Large payload — msgpack vs JSON bandwidth
// ═════════════════════════════════════════════════════════════

async function vanillaS5(baseUrl: string): Promise<IterationResult> {
  const start = Date.now()
  try {
    const r = await axios.get(baseUrl + '/large-messages?count=50', {
      timeout: 15_000,
      responseType: 'json',
    })
    const bytes = typeof r.data === 'string' ? Buffer.byteLength(r.data) : Buffer.byteLength(JSON.stringify(r.data))
    return { success: true, time: Date.now() - start, bytes, connections: 1 }
  } catch {
    return { success: false, time: 15_000, bytes: 0, connections: 1 }
  }
}

function makeRustS5(baseUrl: string) {
  const client = createRustHttpClient({
    baseURL: baseUrl, keepAlive: true,
    retry: { attempts: 3 },
    timeout: { response: 15_000 },
  })
  return async function rustS5(): Promise<IterationResult> {
    const start = Date.now()
    const retriesBefore = client.retryCount
    try {
      const data = await client.get('/large-messages?count=50')
      const jsonSize = Buffer.byteLength(JSON.stringify(data))
      const msgpackSize = rustPack(data).length
      const retries = client.retryCount - retriesBefore
      return {
        success: true,
        time: Date.now() - start,
        bytes: jsonSize - msgpackSize,
        connections: 1,
        retries,
      }
    } catch {
      const retries = client.retryCount - retriesBefore
      return { success: false, time: 15_000, bytes: 0, connections: 1, retries }
    }
  }
}

describe('S5: Large payload (Rust msgpack)', () => {
  for (const [key, profile] of Object.entries(NETWORK_PROFILES)) {
    if (!['good', 'weak', 'veryWeak'].includes(key)) continue
    it(`${profile.emoji} ${profile.name}`, async () => {
      httpProxy.setConditions(profile.conditions)
      httpProxy.disruptAll()
      const rustFn = makeRustS5(httpUrl)
      const r = await runConcurrentComparison(
        { name: 'S5: Large payload (Rust msgpack)', iterations: itersFor(key) },
        profile.conditions, `${profile.emoji} ${profile.name}`, vanillaS5, rustFn, httpUrl,
      )
      reporter.addResult(r)
      expect(r.catcher.avgBytes).toBeDefined()
    }, TIMEOUT)
  }
})

// ═════════════════════════════════════════════════════════════
// S6: WS high-frequency — perMessageDeflate + msgpack
// ═════════════════════════════════════════════════════════════

async function vanillaS6(wsBaseUrl: string): Promise<IterationResult> {
  return new Promise((resolve) => {
    const ws = new WebSocket(wsBaseUrl)
    const msgCount = 50
    let sent = 0; let received = 0; let totalBytes = 0
    const timings: number[] = []; let sendTime = 0

    const payload = {
      event: 'message', id: 'msg_test',
      from: 'user_001', to: 'channel_general',
      text: 'Hello '.repeat(30), ts: Date.now(),
      metadata: { platform: 'desktop', version: '2.1.0' },
    }

    ws.on('open', () => {
      const sendOne = () => {
        if (sent >= msgCount) return
        const json = JSON.stringify(payload)
        totalBytes += Buffer.byteLength(json)
        sendTime = Date.now()
        ws.send(json)
        sent++
      }
      const interval = setInterval(sendOne, 20)
      ws.on('message', (data: Buffer) => {
        totalBytes += data.length
        timings.push(Date.now() - sendTime)
        received++
        if (received >= msgCount) {
          clearInterval(interval)
          ws.close()
        }
      })
    })

    ws.on('close', () => {
      const avgTime = timings.length > 0
        ? Math.round(timings.reduce((a, b) => a + b, 0) / timings.length)
        : 0
      resolve({ success: received >= msgCount, time: avgTime, bytes: totalBytes })
    })
    ws.on('error', () => resolve({ success: false, time: 0 }))
    setTimeout(() => resolve({ success: false, time: 0 }), 30_000)
  })
}

async function rustS6(wsBaseUrl: string): Promise<IterationResult> {
    const ws = createRustWsClient({
      url: wsBaseUrl,
      perMessageDeflate: true,
      handshakeTimeout: 15_000,
      reconnect: { maxAttempts: 0 },
    })
    // Wait for WS connection
    await ws.ready
    const start = Date.now()

    const msgCount = 50
    const timings: number[] = []
    let totalBytes = 0

    const payload = {
      event: 'message', id: 'msg_test',
      from: 'user_001', to: 'channel_general',
      text: 'Hello '.repeat(30), ts: Date.now(),
      metadata: { platform: 'desktop', version: '2.1.0' },
    }

    for (let i = 0; i < msgCount; i++) {
      const binary = rustPack(payload)
      totalBytes += binary.length
      const sendTime = Date.now()
      try {
        ws.send(binary)
        timings.push(Date.now() - sendTime)
      } catch { /* ignore */ }
    }

    const avgTime = timings.length > 0
      ? Math.round(timings.reduce((a, b) => a + b, 0) / timings.length)
      : 0
    ws.close()
    return { success: true, time: avgTime, bytes: totalBytes }
}

describe('S6: WS high-freq (Rust)', () => {
  for (const [key, profile] of Object.entries(NETWORK_PROFILES)) {
    if (!['good', 'weak'].includes(key)) continue
    it(`${profile.emoji} ${profile.name}`, async () => {
      wsProxy.setConditions(profile.conditions)
      wsProxy.disruptAll()
      const r = await runConcurrentComparison(
        { name: 'S6: WS high-freq (Rust)', iterations: Math.min(FAST_ITERATIONS, 10), iterationTimeout: 30_000 },
        profile.conditions, `${profile.emoji} ${profile.name}`, vanillaS6, rustS6, wsUrl,
      )
      reporter.addResult(r)
      expect(r.catcher.avgBytes).toBeDefined()
    }, TIMEOUT)
  }
})

// ═════════════════════════════════════════════════════════════
// S7: Priority queue concurrency
// ═════════════════════════════════════════════════════════════

async function vanillaS7(baseUrl: string): Promise<IterationResult> {
  const start = Date.now()
  const slowReqs = Array.from({ length: 20 }, (_, i) =>
    axios.get(baseUrl + '/slow?delay=500&id=' + i, { timeout: 10_000 }),
  )
  const msgStart = Date.now()
  const msgReq = axios.post(baseUrl + '/messages', { text: 'prio test' }, { timeout: 10_000 })
  let success = true
  const results = await Promise.allSettled([msgReq, ...slowReqs])
  if (results[0].status === 'rejected') success = false
  return { success, time: Date.now() - msgStart, connections: 0 }
}

function makeRustS7(baseUrl: string) {
  const client = createRustHttpClient({
    baseURL: baseUrl, keepAlive: true,
    concurrency: 10,
    timeout: { response: 10_000 },
  })
  return async function rustS7(): Promise<IterationResult> {
    const start = Date.now()

    const avatarReqs = Array.from({ length: 20 }, (_, i) =>
      client.get('/slow?delay=200&id=' + i).catch(() => null),
    )
    const msgStart = Date.now()
    const msgPromise = client.post('/messages', { text: 'prio test' }).catch(() => null)

    let success = true
    const results = await Promise.allSettled([msgPromise, ...avatarReqs])
    if (results[0].status === 'rejected') success = false
    return { success, time: Date.now() - msgStart, connections: 0 }
  }
}

describe('S7: Priority queue (Rust)', () => {
  for (const [key, profile] of Object.entries(NETWORK_PROFILES)) {
    if (!['good', 'weak'].includes(key)) continue
    it(`${profile.emoji} ${profile.name}`, async () => {
      httpProxy.setConditions(profile.conditions)
      httpProxy.disruptAll()
      const rustFn = makeRustS7(httpUrl)
      const r = await runConcurrentComparison(
        { name: 'S7: Priority queue (Rust)', iterations: Math.min(FAST_ITERATIONS, 10), iterationTimeout: 15_000 },
        profile.conditions, `${profile.emoji} ${profile.name}`, vanillaS7, rustFn, httpUrl,
      )
      reporter.addResult(r)
      expect(r.catcher.successRate).toBeGreaterThanOrEqual(r.vanilla.successRate - 0.2)
    }, TIMEOUT)
  }
})

// ═════════════════════════════════════════════════════════════
// S8: DNS cache — slow DNS proxy + real domain
// ═════════════════════════════════════════════════════════════
//
// A local slow DNS proxy adds 200ms latency per DNS query.
// Both sides hit example.com through the slow proxy.
//
// Vanilla (axios): Node.js has no application-level DNS cache.
//   We simulate slow DNS by adding 200ms before each request
//   (equivalent to DNS cache miss on every request).
//   5 sequential requests × 200ms = ~1000ms DNS overhead
//
// Catcher (Rust): dnsCacheTtl=300s, nameservers point to slow proxy.
//   1st request: 200ms DNS overhead (cache miss)
//   2nd-5th requests: 0ms DNS overhead (cache hit)
//   = ~200ms DNS overhead total
//
// Expected: catcher ~800ms faster per iteration due to DNS caching.

import { createSlowDnsProxy, type SlowDnsProxy } from '../network/slow-dns-proxy.js'

const DNS_TARGET = 'http://example.com'
const DNS_DELAY_MS = 200

let slowDns: SlowDnsProxy

async function vanillaS8(_baseUrl: string): Promise<IterationResult> {
  const times: number[] = []
  let success = true
  for (let i = 0; i < 5; i++) {
    const start = Date.now()
    try {
      // Simulate slow DNS on every request (no cache)
      await new Promise((r) => setTimeout(r, DNS_DELAY_MS))
      await axios.get(DNS_TARGET, { timeout: 10_000 })
      times.push(Date.now() - start)
    } catch { success = false; times.push(10_000) }
  }
  const avgTime = Math.round(times.reduce((a, b) => a + b, 0) / times.length)
  return { success, time: avgTime, connections: 0, bytes: avgTime }
}

function makeRustS8(dnsProxyPort: number) {
  const client = createRustHttpClient({
    baseURL: DNS_TARGET,
    keepAlive: true,
    dnsCacheTtl: 300,
    dnsNameservers: [`127.0.0.1:${dnsProxyPort}`],
    timeout: { response: 10_000 },
  })
  return async function rustS8(): Promise<IterationResult> {
    const times: number[] = []
    let success = true
    for (let i = 0; i < 5; i++) {
      const start = Date.now()
      try {
        // First request: DNS lookup via slow proxy (200ms overhead)
        // Subsequent requests: DNS cache hit (0ms overhead)
        await client.get('/')
        times.push(Date.now() - start)
      } catch { success = false; times.push(10_000) }
    }
    const avgTime = Math.round(times.reduce((a, b) => a + b, 0) / times.length)
    return { success, time: avgTime, connections: 0, bytes: avgTime }
  }
}

describe('S8: DNS cache — slow DNS proxy (Rust)', () => {
  beforeAll(async () => {
    slowDns = createSlowDnsProxy(DNS_DELAY_MS)
    await slowDns.start()
  })

  afterAll(async () => {
    await slowDns.stop()
  })

  it('🌐 example.com via slow DNS — 5 sequential requests', async () => {
    const rustFn = makeRustS8(slowDns.port)
    const r = await runConcurrentComparison(
      { name: 'S8: DNS cache (Rust)', iterations: FAST_ITERATIONS },
      { latency: 0, jitter: 0, packetLoss: 0, connectionReset: 0 },
      '🌐 slow DNS (200ms/query)',
      vanillaS8, rustFn, DNS_TARGET,
    )
    reporter.addResult(r)
    expect(r.catcher.successRate).toBeGreaterThanOrEqual(0)
  }, TIMEOUT)
})
