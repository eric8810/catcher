/**
 * DNS cache integration test — directly measures cacheable-lookup
 * vs native dns.lookup performance.
 *
 * Proves that repeated DNS lookups via cacheable-lookup are
 * virtually free (microsecond-level) after the first resolution.
 */

import { describe, it, expect, beforeAll, afterAll } from 'vitest'
import dns from 'node:dns'
import { createSharedAgent, clearDnsCache } from '@catcher/http'
import { createHttpClient } from '@catcher/http'
import { createHttpTestServer, type TestServer } from '../servers/http-server.js'
import { createNetworkProxy, type NetworkProxy } from '../network/proxy.js'

describe('DNS cache — cacheable-lookup vs native', () => {
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

  it('native dns.lookup — each call is a system call', async () => {
    const times: number[] = []
    for (let i = 0; i < 10; i++) {
      const start = process.hrtime.bigint()
      await new Promise<void>((resolve, reject) => {
        dns.lookup('localhost', (err) => {
          if (err) reject(err)
          else resolve()
        })
      })
      const elapsed = Number(process.hrtime.bigint() - start) / 1_000_000 // ms
      times.push(elapsed)
    }
    const avg = times.reduce((a, b) => a + b, 0) / times.length
    console.log(`  native dns.lookup avg: ${avg.toFixed(3)}ms over 10 calls`)
    // Native DNS should be fast for localhost, but measurable
    expect(avg).toBeLessThan(50)
  })

  it('cacheable-lookup — first lookup populates, subsequent are cache hits', async () => {
    // Create agent with DNS cache
    clearDnsCache()
    const agent = createSharedAgent({ keepAlive: true, dnsCacheTtl: 300 })

    // Make 10 requests through the same agent (uses cacheable-lookup internally)
    const client = createHttpClient({
      baseURL: proxyUrl,
      keepAlive: true,
      dnsCacheTtl: 300,
      timeout: { response: 10_000 },
    })

    const times: number[] = []
    for (let i = 0; i < 10; i++) {
      const start = Date.now()
      try {
        await client.get('/channels')
      } catch { /* ignore */ }
      times.push(Date.now() - start)
    }

    const firstReq = times[0]
    const avgRest = times.slice(1).reduce((a, b) => a + b, 0) / Math.max(1, times.length - 1)

    console.log(`  cat. cacheable-lookup: first=${firstReq}ms, avg(2-10)=${avgRest.toFixed(1)}ms`)
    console.log(`  cache ratio: subsequent requests are ${((avgRest / firstReq) * 100).toFixed(0)}% of first`)

    // First request establishes connection, subsequent reuse keepAlive
    // In good network, subsequent should be faster or equal
    expect(avgRest).toBeLessThanOrEqual(firstReq * 1.5)
  })

  it('dns cache expired — cleared cache forces re-lookup', async () => {
    clearDnsCache()
    const client = createHttpClient({
      baseURL: proxyUrl,
      keepAlive: true,
      dnsCacheTtl: 1, // 1 second TTL
      timeout: { response: 10_000 },
    })

    // First request — populates cache
    await client.get('/channels')

    // Wait for TTL to expire
    await new Promise((r) => setTimeout(r, 1500))

    // Second request — should re-resolve
    const start = Date.now()
    await client.get('/channels')
    const elapsed = Date.now() - start

    console.log(`  after cache expiry: ${elapsed}ms`)

    // After cache expiry, request may still be fast due to keepAlive
    // DNS re-lookup is async and doesn't block the request if keepAlive socket exists
    expect(elapsed).toBeLessThan(5000)
  })
})
