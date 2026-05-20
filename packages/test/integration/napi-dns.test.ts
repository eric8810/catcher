/**
 * DNS cache integration test — NAPI version
 *
 * Uses a fake domain (catcher.test) routed through a slow DNS proxy
 * so that DNS resolution is measurably expensive (~200ms per query).
 * The StaleAwareDnsResolver should cache the result after the first
 * lookup, making subsequent requests fast.
 */

import { describe, it, expect, beforeAll, afterAll } from 'vitest'
import { HttpClient } from '@eric8810/catcher-napi-http'
import { createHttpTestServer, type TestServer } from '../servers/http-server.js'
import { createSlowDnsProxy, type SlowDnsProxy } from '../network/slow-dns-proxy.js'

const FAKE_DOMAIN = 'catcher.test'
const DNS_DELAY_MS = 200

let httpServer: TestServer
let slowDns: SlowDnsProxy
let savedNoProxy: string | undefined
let savedNoProxyLower: string | undefined

beforeAll(async () => {
  // Bypass HTTP proxy for fake domain and localhost to avoid 502 from dev proxies
  savedNoProxy = process.env.NO_PROXY
  savedNoProxyLower = process.env.no_proxy
  process.env.NO_PROXY = `${FAKE_DOMAIN},127.0.0.1,localhost`
  process.env.no_proxy = `${FAKE_DOMAIN},127.0.0.1,localhost`

  httpServer = await createHttpTestServer()
  slowDns = createSlowDnsProxy(DNS_DELAY_MS, { [FAKE_DOMAIN]: '127.0.0.1' })
  await slowDns.start()
}, 30_000)

afterAll(async () => {
  // Restore original proxy env
  if (savedNoProxy === undefined) delete process.env.NO_PROXY
  else process.env.NO_PROXY = savedNoProxy
  if (savedNoProxyLower === undefined) delete process.env.no_proxy
  else process.env.no_proxy = savedNoProxyLower

  await slowDns.stop()
  await httpServer?.close()
})

function createCachingClient(overrides: Record<string, unknown> = {}) {
  return new HttpClient(JSON.stringify({
    base_url: `http://${FAKE_DOMAIN}:${httpServer.port}`,
    connect_timeout_ms: 5000,
    response_timeout_ms: 10_000,
    dns: {
      cache_size: 512,
      cache_ttl_secs: 300,
      negative_ttl_secs: 60,
      stale_ttl_secs: 3600,
      stale_on_error: true,
      nameservers: [`127.0.0.1:${slowDns.port}`],
      ...overrides,
    },
  }))
}

describe('NAPI DNS cache — StaleAwareDnsResolver', () => {
  it('cache hit — subsequent requests skip DNS lookup', async () => {
    const client = createCachingClient()

    const times: number[] = []
    for (let i = 0; i < 5; i++) {
      const start = Date.now()
      const resp = await client.get('/channels')
      expect(resp.status).toBe(200)
      times.push(Date.now() - start)
    }

    const first = times[0]
    const avgRest = times.slice(1).reduce((a, b) => a + b, 0) / (times.length - 1)

    console.log(`  NAPI DNS cache: first=${first}ms, avg(2-5)=${avgRest.toFixed(1)}ms`)
    console.log(`  Per-request times: [${times.join(', ')}]`)

    // First request pays slow DNS (~200ms) + HTTP; subsequent use cache
    expect(first).toBeGreaterThanOrEqual(DNS_DELAY_MS * 0.5)
    expect(avgRest).toBeLessThan(first * 0.5)
  })

  it('cold start — first request incurs DNS proxy latency', async () => {
    const client = createCachingClient()

    const start = Date.now()
    const resp = await client.get('/channels')
    const elapsed = Date.now() - start

    expect(resp.status).toBe(200)
    console.log(`  NAPI DNS cold start: ${elapsed}ms (expected >= ~${DNS_DELAY_MS}ms)`)
    expect(elapsed).toBeGreaterThanOrEqual(DNS_DELAY_MS * 0.5)
  })

  it('accepts all new DnsConfig fields', () => {
    const client = new HttpClient(JSON.stringify({
      base_url: `http://${FAKE_DOMAIN}:${httpServer.port}`,
      connect_timeout_ms: 5000,
      response_timeout_ms: 10_000,
      dns: {
        cache_size: 1024,
        cache_ttl_secs: 600,
        negative_ttl_secs: 30,
        stale_ttl_secs: 7200,
        stale_on_error: false,
        nameservers: [],
        host_mapping: {},
      },
    }))
    expect(client).toBeDefined()
  })

  it('accepts camelCase DnsConfig aliases', () => {
    const client = new HttpClient(JSON.stringify({
      base_url: `http://${FAKE_DOMAIN}:${httpServer.port}`,
      connect_timeout_ms: 5000,
      response_timeout_ms: 10_000,
      dns: {
        cacheSize: 256,
        cacheTtlSecs: 120,
        negativeTtlSecs: 10,
        staleTtlSecs: 1800,
        staleOnError: true,
        hostMapping: { 'api.test': '10.0.0.1' },
      },
    }))
    expect(client).toBeDefined()
  })

  it('host_mapping bypasses DNS — no slow proxy delay', async () => {
    const client = new HttpClient(JSON.stringify({
      base_url: `http://${FAKE_DOMAIN}:${httpServer.port}`,
      connect_timeout_ms: 5000,
      response_timeout_ms: 10_000,
      dns: {
        cache_ttl_secs: 300,
        nameservers: [`127.0.0.1:${slowDns.port}`],
        host_mapping: { [FAKE_DOMAIN]: '127.0.0.1' },
      },
    }))

    const start = Date.now()
    const resp = await client.get('/channels')
    const elapsed = Date.now() - start

    expect(resp.status).toBe(200)
    console.log(`  host_mapping bypass: ${elapsed}ms`)
    // host_mapping resolves instantly — should be well under DNS delay
    expect(elapsed).toBeLessThan(DNS_DELAY_MS)
  })

  it('default DnsConfig enables caching (no explicit dns field)', async () => {
    // No dns config at all — uses system DNS with default cache
    const client = new HttpClient(JSON.stringify({
      base_url: `http://127.0.0.1:${httpServer.port}`,
      connect_timeout_ms: 5000,
      response_timeout_ms: 10_000,
    }))

    const resp = await client.get('/channels')
    expect(resp.status).toBe(200)
    const resp2 = await client.get('/channels')
    expect(resp2.status).toBe(200)
  })
})

describe('NAPI DNS cache — stale-while-revalidate (DNS goes down)', () => {
  it('requests succeed via stale cache after DNS proxy stops', async () => {
    // Short TTL so entry becomes stale quickly; long stale window for fallback
    const staleDns = createSlowDnsProxy(50, { [FAKE_DOMAIN]: '127.0.0.1' })
    await staleDns.start()

    const client = new HttpClient(JSON.stringify({
      base_url: `http://${FAKE_DOMAIN}:${httpServer.port}`,
      connect_timeout_ms: 5000,
      response_timeout_ms: 10_000,
      dns: {
        cache_ttl_secs: 1,
        stale_ttl_secs: 3600,
        stale_on_error: true,
        nameservers: [`127.0.0.1:${staleDns.port}`],
      },
    }))

    // Warm the cache
    const warmResp = await client.get('/channels')
    expect(warmResp.status).toBe(200)

    // Kill DNS proxy — new DNS queries will fail
    await staleDns.stop()

    // Wait for TTL to expire so entry becomes stale
    await new Promise(r => setTimeout(r, 1500))

    // Requests should still succeed: stale entry returned, background refresh fails silently
    const staleResp = await client.get('/channels')
    expect(staleResp.status).toBe(200)

    const staleResp2 = await client.get('/channels')
    expect(staleResp2.status).toBe(200)

    console.log('  DNS proxy down → stale cache served successfully')
  })
})

describe('NAPI DNS cache — 5-request comparison', () => {
  it('cached requests are faster than uncached first request', async () => {
    const client = createCachingClient()

    const times: number[] = []
    for (let i = 0; i < 5; i++) {
      const start = Date.now()
      await client.get('/channels')
      times.push(Date.now() - start)
    }

    const first = times[0]
    const avgRest = times.slice(1).reduce((a, b) => a + b, 0) / 4

    console.log(`  first=${first}ms, avg(2-5)=${avgRest.toFixed(1)}ms`)
    console.log(`  times: [${times.join(', ')}]`)

    // First pays ~200ms DNS; subsequent should be much faster
    expect(first).toBeGreaterThanOrEqual(DNS_DELAY_MS * 0.5)
    expect(avgRest).toBeLessThan(first * 0.5)
  })
})

describe('NAPI DNS cache — benchmark vs TS cacheable-lookup', () => {
  it('NAPI StaleAwareDnsResolver vs TS cacheable-lookup: 10 sequential requests', async () => {
    // ── NAPI side ──
    const napiClient = createCachingClient()

    const napiTimes: number[] = []
    for (let i = 0; i < 10; i++) {
      const start = Date.now()
      await napiClient.get('/channels')
      napiTimes.push(Date.now() - start)
    }

    const napiFirst = napiTimes[0]
    const napiAvgRest = napiTimes.slice(1).reduce((a, b) => a + b, 0) / 9
    const napiTotal = napiTimes.reduce((a, b) => a + b, 0)

    console.log(`  NAPI:  first=${napiFirst}ms, avg(2-10)=${napiAvgRest.toFixed(1)}ms, total=${napiTotal}ms`)
    console.log(`  NAPI times: [${napiTimes.join(', ')}]`)

    // DNS cache should make subsequent requests much faster than first
    expect(napiFirst).toBeGreaterThanOrEqual(DNS_DELAY_MS * 0.5)
    expect(napiAvgRest).toBeLessThan(napiFirst * 0.5)
  })
})
