/**
 * Integration benchmark: NAPI Rust HTTP client under simulated network conditions.
 *
 * Mirrors http.test.ts but uses the native Rust adapter (createRustHttpClient)
 * instead of the TS createHttpClient. Per-client DNS cache means no clearDnsCache()
 * calls are needed.
 *
 * Compares vanilla axios vs NAPI Rust client:
 *   - Connection reuse (keepAlive)
 *   - Auto-retry on failure
 *   - Concurrency queue
 *
 * Network conditions: good, weak, very-weak (via NetworkProxy)
 */

import axios from 'axios'
import { describe, it, expect, beforeAll, afterAll } from 'vitest'
import { createRustHttpClient } from '../adapters/rust-adapter.js'
import { createHttpTestServer, type TestServer } from '../servers/http-server.js'
import { createNetworkProxy, type NetworkProxy } from '../network/proxy.js'
import { NETWORK_PROFILES } from '../network/presets.js'

const TIMEOUT = 120_000 // weak network tests need longer timeout
const isCI = !!process.env.CI

describe('NAPI HTTP — keepAlive connection reuse', () => {
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

  for (const [key, profile] of Object.entries(NETWORK_PROFILES)) {
    // Only test good/weak/veryWeak for integration
    if (!['good', 'weak', 'veryWeak'].includes(key)) continue

    // Skip weak network tests in CI — flaky due to unstable network simulation
    const skip = isCI && key !== 'good'

    it.skipIf(skip)(`${profile.emoji} ${profile.name} — 3 consecutive GETs`, async () => {
      proxy.setConditions(profile.conditions)
      proxy.disruptAll()

      // ── Vanilla: new axios instance, no keepAlive ──
      const vanillaStart = Date.now()
      const vanillaAxios = axios.create({
        baseURL: proxyUrl,
        timeout: 60_000,
        httpsAgent: false as any,
      })
      let vanillaErrors = 0
      for (let i = 0; i < 3; i++) {
        try {
          await vanillaAxios.get('/channels')
        } catch {
          vanillaErrors++
        }
      }
      const vanillaTime = Date.now() - vanillaStart

      // ── NAPI Rust: shared agent with keepAlive ──
      const napiStart = Date.now()
      const napiClient = createRustHttpClient({
        baseURL: proxyUrl,
        keepAlive: true,
        dnsCacheTtl: 300,
        timeout: { response: 60_000 },
      })
      let napiErrors = 0
      for (let i = 0; i < 3; i++) {
        try {
          await napiClient.get('/channels')
        } catch {
          napiErrors++
        }
      }
      const napiTime = Date.now() - napiStart

      console.log(`  vanilla: ${vanillaTime}ms (errors: ${vanillaErrors})`)
      console.log(`  napi:    ${napiTime}ms (errors: ${napiErrors})`)

      // In good network, napi should be faster or equal
      // In weak network, napi should be significantly faster due to connection reuse
      if (key === 'good') {
        expect(napiErrors).toBeLessThanOrEqual(vanillaErrors)
      }
    }, TIMEOUT)
  }
})

describe('NAPI HTTP — auto-retry on failure', () => {
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

  it.skipIf(isCI)('🟡 弱网 — napi retry survives packet loss', async () => {
    // Pure packet loss (no connection reset): vanilla may fail on individual
    // packets, but napi's 5 retries should eventually get through
    proxy.setConditions({
      latency: 200,
      packetLoss: 0.2,
      connectionReset: 0,
    })
    proxy.disruptAll()

    // Vanilla — likely to fail
    let vanillaSuccess = false
    try {
      await axios.post(proxyUrl + '/messages', { text: 'hello' }, { timeout: 10_000 })
      vanillaSuccess = true
    } catch {
      vanillaSuccess = false
    }

    // NAPI Rust — auto-retry should recover
    let napiSuccess = false
    try {
      const client = createRustHttpClient({
        baseURL: proxyUrl,
        keepAlive: true,
        retry: { attempts: 5, backoff: 'exponential' },
        timeout: { response: 30_000 },
      })
      await client.post('/messages', { text: 'hello' })
      napiSuccess = true
    } catch {
      napiSuccess = false
    }

    console.log(`  vanilla success: ${vanillaSuccess}`)
    console.log(`  napi success:    ${napiSuccess}`)

    // NAPI retry should achieve success even under packet loss
    expect(napiSuccess).toBe(true)
  }, TIMEOUT)
})

describe('NAPI HTTP — priority queue concurrency', () => {
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

  it('concurrency=10 — all requests complete', async () => {
    proxy.setConditions(NETWORK_PROFILES.good.conditions)
    proxy.disruptAll()

    const client = createRustHttpClient({
      baseURL: proxyUrl,
      keepAlive: true,
      concurrency: 10,
      timeout: { response: 60_000 },
    })

    const start = Date.now()
    const results = await Promise.allSettled(
      Array.from({ length: 50 }, (_, i) =>
        client.get(`/channels/${i % 20}/messages?pageSize=10`),
      ),
    )
    const elapsed = Date.now() - start

    const succeeded = results.filter((r) => r.status === 'fulfilled').length
    const failed = results.filter((r) => r.status === 'rejected').length

    console.log(`  50 requests: ${succeeded} ok, ${failed} failed in ${elapsed}ms`)
    expect(succeeded).toBeGreaterThanOrEqual(45) // at most 5 failures
  })
})
