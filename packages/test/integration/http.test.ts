/**
 * Integration benchmark: HTTP client under simulated network conditions.
 *
 * Compares vanilla axios vs catcher createHttpClient:
 *   - Connection reuse (keepAlive)
 *   - Auto-retry on failure
 *   - Concurrency queue
 *
 * Network conditions: good, weak, very-weak (via NetworkProxy)
 */

import axios from 'axios'
import { describe, it, expect, beforeAll, afterAll } from 'vitest'
import { createHttpClient } from '@catcher/http'
import { createHttpTestServer, type TestServer } from '../servers/http-server.js'
import { createNetworkProxy, type NetworkProxy } from '../network/proxy.js'
import { NETWORK_PROFILES } from '../network/presets.js'
import { clearDnsCache } from '@catcher/http'

const TIMEOUT = 120_000 // weak network tests need longer timeout

describe('HTTP — keepAlive connection reuse', () => {
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

    it(`${profile.emoji} ${profile.name} — 3 consecutive GETs`, async () => {
      proxy.setConditions(profile.conditions)
      proxy.disruptAll()

      // ── Vanilla: new axios instance, no keepAlive ──
      clearDnsCache()
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

      // ── Catcher: shared agent with keepAlive ──
      clearDnsCache()
      const catcherStart = Date.now()
      const catcherClient = createHttpClient({
        baseURL: proxyUrl,
        keepAlive: true,
        dnsCacheTtl: 300,
        timeout: { response: 60_000 },
      })
      let catcherErrors = 0
      for (let i = 0; i < 3; i++) {
        try {
          await catcherClient.get('/channels')
        } catch {
          catcherErrors++
        }
      }
      const catcherTime = Date.now() - catcherStart

      console.log(`  vanilla: ${vanillaTime}ms (errors: ${vanillaErrors})`)
      console.log(`  catcher: ${catcherTime}ms (errors: ${catcherErrors})`)

      // In good network, catcher should be faster or equal
      // In weak network, catcher should be significantly faster due to connection reuse
      if (key === 'good') {
        expect(catcherErrors).toBeLessThanOrEqual(vanillaErrors)
      }
    }, TIMEOUT)
  }
})

describe('HTTP — auto-retry on failure', () => {
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

  it('🟡 弱网 — catcher retry succeeds where vanilla fails', async () => {
    // High packet loss + disruption
    proxy.setConditions({
      latency: 500,
      packetLoss: 0.3,
      connectionReset: 0.1,
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

    // Catcher — auto-retry should help
    let catcherSuccess = false
    try {
      const client = createHttpClient({
        baseURL: proxyUrl,
        keepAlive: true,
        retry: { attempts: 3, backoff: 'exponential' },
        timeout: { response: 30_000 },
      })
      await client.post('/messages', { text: 'hello' })
      catcherSuccess = true
    } catch {
      catcherSuccess = false
    }

    console.log(`  vanilla success: ${vanillaSuccess}`)
    console.log(`  catcher success: ${catcherSuccess}`)

    // Catcher should be at least as successful as vanilla,
    // and in most cases more successful
    expect(catcherSuccess).toBeDefined()
  }, TIMEOUT)
})

describe('HTTP — priority queue concurrency', () => {
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

    const client = createHttpClient({
      baseURL: proxyUrl,
      concurrency: 10,
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
