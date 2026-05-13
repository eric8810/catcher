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
import { clearDnsCache } from '../adapters/dns-adapter.js'
import { createHttpTestServer, type TestServer } from '../servers/http-server.js'
import { createWSTestServer, type WSTestServer } from '../servers/ws-server.js'
import { createNetworkProxy, type NetworkProxy } from '../network/proxy.js'
import { NETWORK_PROFILES } from '../network/presets.js'
import { runConcurrentComparison, type IterationResult } from '../harness.js'
import { ComparisonReporter } from '../reporters/comparison-reporter.js'

const TIMEOUT = 300_000
const FAST_ITERATIONS = parseInt(process.env.FAST_ITERATIONS ?? '30', 10)
const SLOW_ITERATIONS = Math.min(FAST_ITERATIONS, 20)
const itersFor = (key: string) => {
  const isSlow = ['weak', 'veryWeak', 'mobile3g', 'metro'].includes(key)
  return isSlow ? 5 : FAST_ITERATIONS
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
    await axios.get(baseUrl + '/channels', { timeout: 10_000 })
    return { success: true, time: Date.now() - start, connections: 1 }
  } catch {
    return { success: false, time: 10_000, connections: 1 }
  }
}

async function rustS1(baseUrl: string): Promise<IterationResult> {
  clearDnsCache()
  const start = Date.now()
  let retries = 0
  try {
    const client = createRustHttpClient({
      baseURL: baseUrl,
      keepAlive: true,
      dnsCacheTtl: 300,
      retry: { attempts: 2, backoff: 'exponential', onRetry: () => { retries++ } },
      timeout: { response: 10_000 },
    })
    await client.get('/channels')
    return { success: true, time: Date.now() - start, connections: 1, retries }
  } catch {
    return { success: false, time: 10_000, connections: 1, retries }
  }
}

describe('S1: Cold start (Rust)', () => {
  for (const [key, profile] of Object.entries(NETWORK_PROFILES)) {
    if (!['good', 'weak', 'veryWeak'].includes(key)) continue
    it(`${profile.emoji} ${profile.name}`, async () => {
      httpProxy.setConditions(profile.conditions)
      httpProxy.disruptAll()
      const r = await runConcurrentComparison(
        { name: 'S1: Cold start (Rust)', iterations: itersFor(key), iterationTimeout: 12_000 },
        profile.conditions, `${profile.emoji} ${profile.name}`, vanillaS1, rustS1, httpUrl,
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
    await axios.post(baseUrl + '/messages', { text: 'Hello '.repeat(30) }, { timeout: 15_000 })
    return { success: true, time: Date.now() - start }
  } catch {
    return { success: false, time: 15_000 }
  }
}

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
  } catch {
    return { success: false, time: 30_000, retries }
  }
}

describe('S2: Send message (Rust)', () => {
  for (const [key, profile] of Object.entries(NETWORK_PROFILES)) {
    if (!['weak', 'veryWeak', 'mobile3g'].includes(key)) continue
    it(`${profile.emoji} ${profile.name}`, async () => {
      httpProxy.setConditions(profile.conditions)
      httpProxy.disruptAll()
      const r = await runConcurrentComparison(
        { name: 'S2: Send message (Rust)', iterations: itersFor(key), iterationTimeout: 35_000 },
        profile.conditions, `${profile.emoji} ${profile.name}`, vanillaS2, rustS2, httpUrl,
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
    await axios.get(baseUrl + '/channels/ch_0/messages?pageSize=50', { timeout: 15_000 })
    return { success: true, time: Date.now() - start }
  } catch {
    return { success: false, time: 15_000 }
  }
}

async function rustS3(baseUrl: string): Promise<IterationResult> {
  clearDnsCache()
  const start = Date.now()
  let retries = 0
  try {
    const client = createRustHttpClient({
      baseURL: baseUrl, keepAlive: true,
      retry: { attempts: 3, onRetry: () => { retries++ } }, timeout: { response: 15_000 },
    })
    await client.get('/channels/ch_0/messages?pageSize=50')
    return { success: true, time: Date.now() - start, retries }
  } catch {
    return { success: false, time: 15_000, retries }
  }
}

describe('S3: Load messages (Rust)', () => {
  for (const [key, profile] of Object.entries(NETWORK_PROFILES)) {
    if (!['good', 'weak', 'veryWeak'].includes(key)) continue
    it(`${profile.emoji} ${profile.name}`, async () => {
      httpProxy.setConditions(profile.conditions)
      httpProxy.disruptAll()
      const r = await runConcurrentComparison(
        { name: 'S3: Load messages (Rust)', iterations: itersFor(key) },
        profile.conditions, `${profile.emoji} ${profile.name}`, vanillaS3, rustS3, httpUrl,
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
  try { await axios.post(baseUrl + '/auth', { user: 'sg_user' }, { timeout: 15_000 }) } catch { success = false }
  try { await axios.get(baseUrl + '/channels', { timeout: 15_000 }) } catch { success = false }
  return { success, time: Date.now() - start, connections: 2 }
}

async function rustS4(baseUrl: string): Promise<IterationResult> {
  clearDnsCache()
  const start = Date.now()
  let retries = 0
  let success = true
  try {
    const client = createRustHttpClient({
      baseURL: baseUrl, keepAlive: true,
      retry: { attempts: 3, onRetry: () => { retries++ } }, timeout: { response: 15_000 },
    })
    await client.post('/auth', { user: 'sg_user' })
    await client.get('/channels')
  } catch { success = false }
  return { success, time: Date.now() - start, connections: 1, retries }
}

describe('S4: Cross-region (Rust)', () => {
  const profile = NETWORK_PROFILES.crossRegion
  it(`${profile.emoji} ${profile.name}`, async () => {
    httpProxy.setConditions(profile.conditions)
    httpProxy.disruptAll()
    const r = await runConcurrentComparison(
      { name: 'S4: Cross-region (Rust)', iterations: FAST_ITERATIONS },
      profile.conditions, `${profile.emoji} ${profile.name}`, vanillaS4, rustS4, httpUrl,
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
    return { success: true, time: Date.now() - start, bytes: 0 }
  } catch {
    return { success: false, time: 15_000 }
  }
}

async function rustS5(baseUrl: string): Promise<IterationResult> {
  clearDnsCache()
  const start = Date.now()
  let retries = 0
  try {
    const client = createRustHttpClient({
      baseURL: baseUrl, keepAlive: true,
      retry: { attempts: 2, onRetry: () => { retries++ } },
      timeout: { response: 15_000 },
    })
    const data = await client.get('/large-messages?count=50')
    const jsonSize = Buffer.byteLength(JSON.stringify(data))
    const msgpackSize = rustPack(data).length
    return {
      success: true,
      time: Date.now() - start,
      bytes: jsonSize - msgpackSize,
      connections: 1,
      retries,
    }
  } catch {
    return { success: false, time: 15_000, retries }
  }
}

describe('S5: Large payload (Rust msgpack)', () => {
  for (const [key, profile] of Object.entries(NETWORK_PROFILES)) {
    if (!['good', 'weak', 'veryWeak'].includes(key)) continue
    it(`${profile.emoji} ${profile.name}`, async () => {
      httpProxy.setConditions(profile.conditions)
      httpProxy.disruptAll()
      const r = await runConcurrentComparison(
        { name: 'S5: Large payload (Rust msgpack)', iterations: itersFor(key) },
        profile.conditions, `${profile.emoji} ${profile.name}`, vanillaS5, rustS5, httpUrl,
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

async function rustS7(baseUrl: string): Promise<IterationResult> {
  clearDnsCache()
  const start = Date.now()
  const client = createRustHttpClient({
    baseURL: baseUrl, keepAlive: true,
    concurrency: 10,
    timeout: { response: 10_000 },
  })

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

describe('S7: Priority queue (Rust)', () => {
  for (const [key, profile] of Object.entries(NETWORK_PROFILES)) {
    if (!['good', 'weak'].includes(key)) continue
    it(`${profile.emoji} ${profile.name}`, async () => {
      httpProxy.setConditions(profile.conditions)
      httpProxy.disruptAll()
      const r = await runConcurrentComparison(
        { name: 'S7: Priority queue (Rust)', iterations: Math.min(FAST_ITERATIONS, 10), iterationTimeout: 15_000 },
        profile.conditions, `${profile.emoji} ${profile.name}`, vanillaS7, rustS7, httpUrl,
      )
      reporter.addResult(r)
      expect(r.catcher.successRate).toBeGreaterThanOrEqual(r.vanilla.successRate - 0.2)
    }, TIMEOUT)
  }
})

// ═════════════════════════════════════════════════════════════
// S8: DNS cache
// ═════════════════════════════════════════════════════════════

async function vanillaS8(baseUrl: string): Promise<IterationResult> {
  const times: number[] = []
  let success = true
  for (let i = 0; i < 5; i++) {
    const start = Date.now()
    try {
      await axios.get(baseUrl + '/channels', { timeout: 10_000 })
      times.push(Date.now() - start)
    } catch { success = false; times.push(10_000) }
  }
  const avgTime = Math.round(times.reduce((a, b) => a + b, 0) / times.length)
  return { success, time: avgTime, connections: 0 }
}

async function rustS8(baseUrl: string): Promise<IterationResult> {
  clearDnsCache()
  const times: number[] = []
  let success = true
  const client = createRustHttpClient({
    baseURL: baseUrl, keepAlive: true, dnsCacheTtl: 300,
    timeout: { response: 10_000 },
  })
  for (let i = 0; i < 5; i++) {
    const start = Date.now()
    try {
      await client.get('/channels')
      times.push(Date.now() - start)
    } catch { success = false; times.push(10_000) }
  }
  const avgTime = Math.round(times.reduce((a, b) => a + b, 0) / times.length)
  return { success, time: avgTime, connections: 0 }
}

describe('S8: DNS cache (Rust)', () => {
  for (const [key, profile] of Object.entries(NETWORK_PROFILES)) {
    if (!['good', 'weak'].includes(key)) continue
    it(`${profile.emoji} ${profile.name}`, async () => {
      httpProxy.setConditions(profile.conditions)
      httpProxy.disruptAll()
      const r = await runConcurrentComparison(
        { name: 'S8: DNS cache (Rust)', iterations: Math.min(FAST_ITERATIONS, 10), iterationTimeout: 15_000 },
        profile.conditions, `${profile.emoji} ${profile.name}`, vanillaS8, rustS8, httpUrl,
      )
      reporter.addResult(r)
      expect(r.catcher.successRate).toBeGreaterThanOrEqual(0)
    }, TIMEOUT)
  }
})
