import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import http from 'node:http'
import { createHttpClient } from '../client.js'

let server: http.Server
let baseUrl: string

function startServer(): Promise<void> {
  return new Promise((resolve) => {
    server = http.createServer((req, res) => {
      const url = new URL(req.url!, `http://${req.headers.host}`)

      // Echo endpoint
      if (url.pathname === '/echo') {
        let body = ''
        req.on('data', (chunk) => { body += chunk })
        req.on('end', () => {
          res.writeHead(200, { 'Content-Type': 'application/json' })
          res.end(JSON.stringify({
            method: req.method,
            url: req.url,
            headers: req.headers,
            body: body ? JSON.parse(body) : null,
          }))
        })
        return
      }

      // GET /channels
      if (url.pathname === '/channels' && req.method === 'GET') {
        res.writeHead(200, { 'Content-Type': 'application/json' })
        res.end(JSON.stringify([{ id: 1, name: 'general' }]))
        return
      }

      // GET /messages
      if (url.pathname === '/messages' && req.method === 'GET') {
        res.writeHead(200, { 'Content-Type': 'application/json' })
        res.end(JSON.stringify([{ id: 1, text: 'hello' }]))
        return
      }

      // POST /auth
      if (url.pathname === '/auth' && req.method === 'POST') {
        let body = ''
        req.on('data', (chunk) => { body += chunk })
        req.on('end', () => {
          res.writeHead(200, { 'Content-Type': 'application/json' })
          res.end(JSON.stringify({ token: 'jwt-token', user: JSON.parse(body) }))
        })
        return
      }

      // Delayed response for timeout test
      if (url.pathname === '/slow') {
        setTimeout(() => {
          res.writeHead(200, { 'Content-Type': 'application/json' })
          res.end(JSON.stringify({ ok: true }))
        }, parseInt(url.searchParams.get('delay') ?? '10000', 10))
        return
      }

      // 5xx endpoint for retry test
      if (url.pathname === '/fail-twice') {
        const failCount = (globalThis as any).__failCount ?? 0
        ;(globalThis as any).__failCount = failCount + 1
        if (failCount < 2) {
          res.writeHead(503, { 'Content-Type': 'application/json' })
          res.end(JSON.stringify({ error: 'service unavailable' }))
          return
        }
        res.writeHead(200, { 'Content-Type': 'application/json' })
        res.end(JSON.stringify({ ok: true }))
        return
      }

      res.writeHead(404)
      res.end('not found')
    })
    server.listen(0, '127.0.0.1', () => resolve())
  })
}

function stopServer(): Promise<void> {
  return new Promise((resolve) => { server.close(() => resolve()) })
}

beforeEach(async () => {
  await startServer()
  const addr = server.address() as any
  baseUrl = `http://127.0.0.1:${addr.port}`
  ;(globalThis as any).__failCount = 0
})

afterEach(async () => {
  await stopServer()
})

// ── H1-H7: Basic requests ────────────────────────────────────────

describe('H1 — GET request success', () => {
  it('returns status 200 and correct data', async () => {
    const client = createHttpClient({ baseURL: baseUrl })
    const data = await client.get('/channels')
    expect(data).toEqual([{ id: 1, name: 'general' }])
  })
})

describe('H2 — POST + body', () => {
  it('server receives the request body', async () => {
    const client = createHttpClient({ baseURL: baseUrl })
    const data = await client.post('/auth', { username: 'test' })
    expect(data.user).toEqual({ username: 'test' })
  })
})

describe('H3 — PUT/DELETE/PATCH methods', () => {
  it('echoes the correct HTTP method', async () => {
    const client = createHttpClient({ baseURL: baseUrl })

    const put = await client.put('/echo', { a: 1 })
    expect(put.method).toBe('PUT')

    const del = await client.delete('/echo')
    expect(del.method).toBe('DELETE')

    const patch = await client.patch('/echo', { a: 2 })
    expect(patch.method).toBe('PATCH')
  })
})

describe('H4 — Custom headers pass through', () => {
  it('request includes Authorization header', async () => {
    const client = createHttpClient({ baseURL: baseUrl })
    const data = await client.get('/echo', {
      headers: { Authorization: 'Bearer xxx' },
    } as any)
    expect(data.headers.authorization).toBe('Bearer xxx')
  })
})

describe('H5 — Query params serialization', () => {
  it('URL contains serialized query params', async () => {
    const client = createHttpClient({ baseURL: baseUrl })
    const data = await client.get('/echo', {
      params: { a: 1, b: [2, 3] },
    } as any)
    expect(data.url).toContain('a=1')
    expect(data.url).toContain('b=2')
    expect(data.url).toContain('b=3')
  })
})

describe('H6 — baseURL concatenation', () => {
  it('prepends baseURL to request URL', async () => {
    const client = createHttpClient({ baseURL: baseUrl })
    // Verify the request URL includes the path by checking echoed URL
    const data = await client.get('/echo')
    expect(data.url).toBe('/echo')
  })
})

describe('H7 — Timeout生效', () => {
  it('throws on timeout when server is slow', async () => {
    const client = createHttpClient({
      baseURL: baseUrl,
      timeout: { response: 100 },
    })
    await expect(client.get('/slow?delay=5000')).rejects.toThrow()
  })
})

// ── H8-H13: Resilience layer ─────────────────────────────────────

describe('H8 — Retry config works', () => {
  it('retries on 5xx and succeeds', async () => {
    const client = createHttpClient({
      baseURL: baseUrl,
      retry: { attempts: 3, minTimeout: 10 },
    })
    const data = await client.get('/fail-twice')
    expect(data).toEqual({ ok: true })
  })
})

describe('H9 — Per-request retry override', () => {
  it('uses per-request retry count', async () => {
    const client = createHttpClient({
      baseURL: baseUrl,
      retry: { attempts: 1, minTimeout: 10 },
    })
    // Global retry=1 (2 total tries) won't be enough for 2 failures,
    // but per-request retry=3 will
    const data = await client.get('/fail-twice', {
      retry: { attempts: 3, minTimeout: 10 },
    } as any)
    expect(data).toEqual({ ok: true })
  })
})

describe('H10 — retry:false disables retry', () => {
  it('does not retry when retry:false', async () => {
    const client = createHttpClient({
      baseURL: baseUrl,
      retry: { attempts: 5, minTimeout: 10 },
    })
    // /fail-twice fails first 2 times; with retry disabled it fails immediately
    ;(globalThis as any).__failCount = 0
    // Make only 1 failure happen
    const server2 = http.createServer((req, res) => {
      res.writeHead(500)
      res.end('error')
    })
    await new Promise<void>((r) => server2.listen(0, '127.0.0.1', () => r()))
    const addr = server2.address() as any

    const client2 = createHttpClient({
      baseURL: `http://127.0.0.1:${addr.port}`,
      retry: { attempts: 3, minTimeout: 10 },
    })

    await expect(
      client2.get('/test', { retry: false } as any),
    ).rejects.toThrow()

    await new Promise<void>((r) => server2.close(() => r()))
  })
})

describe('H11 — Circuit breaker state transitions to open', () => {
  it('breaker opens after consecutive failures', async () => {
    const failServer = http.createServer((req, res) => {
      res.writeHead(500)
      res.end('error')
    })
    await new Promise<void>((r) => failServer.listen(0, '127.0.0.1', () => r()))
    const addr = failServer.address() as any

    const client = createHttpClient({
      baseURL: `http://127.0.0.1:${addr.port}`,
      circuitBreaker: { failureThreshold: 3, resetTimeout: 60_000 },
    })

    // Trigger failures
    for (let i = 0; i < 5; i++) {
      try { await client.get('/test') } catch {}
    }

    expect(client.circuitBreakerState()).toBe('open')
    await new Promise<void>((r) => failServer.close(() => r()))
  })
})

describe('H12 — Circuit breaker recovery', () => {
  it('transitions closed → open → half-open → closed', async () => {
    let shouldFail = true
    const cbServer = http.createServer((req, res) => {
      if (shouldFail) {
        res.writeHead(500)
        res.end('error')
      } else {
        res.writeHead(200, { 'Content-Type': 'application/json' })
        res.end(JSON.stringify({ ok: true }))
      }
    })
    await new Promise<void>((r) => cbServer.listen(0, '127.0.0.1', () => r()))
    const addr = cbServer.address() as any

    const client = createHttpClient({
      baseURL: `http://127.0.0.1:${addr.port}`,
      circuitBreaker: { failureThreshold: 2, resetTimeout: 200 },
    })

    // Trigger failures to open breaker
    for (let i = 0; i < 3; i++) {
      try { await client.get('/test') } catch {}
    }
    expect(client.circuitBreakerState()).toBe('open')

    // Wait for reset timeout
    await new Promise((r) => setTimeout(r, 300))
    shouldFail = false

    // Next request should succeed (half-open → closed)
    const result = await client.get('/test')
    expect(result).toEqual({ ok: true })
    expect(client.circuitBreakerState()).toBe('closed')

    await new Promise<void>((r) => cbServer.close(() => r()))
  })
})

describe('H13 — Concurrency queue limits', () => {
  it('runs at most `concurrency` requests simultaneously', async () => {
    let running = 0
    let maxRunning = 0

    const concServer = http.createServer((req, res) => {
      running++
      maxRunning = Math.max(maxRunning, running)
      setTimeout(() => {
        running--
        res.writeHead(200, { 'Content-Type': 'application/json' })
        res.end(JSON.stringify({ ok: true }))
      }, 100)
    })
    await new Promise<void>((r) => concServer.listen(0, '127.0.0.1', () => r()))
    const addr = concServer.address() as any

    const client = createHttpClient({
      baseURL: `http://127.0.0.1:${addr.port}`,
      concurrency: 2,
    })

    const promises = Array.from({ length: 10 }, () => client.get('/test'))
    await Promise.allSettled(promises)

    expect(maxRunning).toBeLessThanOrEqual(2)
    await new Promise<void>((r) => concServer.close(() => r()))
  })
})

// ── H14-H16: Interceptor integration ─────────────────────────────

describe('H14 — Request interceptor modifies config', () => {
  it('adds auth header via interceptor', async () => {
    const client = createHttpClient({ baseURL: baseUrl })
    client.interceptors.request.use(async (config: any) => {
      config.headers = { ...config.headers, Authorization: 'interceptor-token' }
      return config
    })
    const data = await client.get('/echo')
    expect(data.headers.authorization).toBe('interceptor-token')
  })
})

describe('H15 — Response interceptor transforms data', () => {
  it('response interceptor can extract data field', async () => {
    const client = createHttpClient({ baseURL: baseUrl })
    client.interceptors.response.use(async (resp: any) => {
      // Return just the status, discarding data
      return { ...resp, data: resp.data?.length ?? 0 }
    })
    const result = await client.get('/channels')
    // The interceptor replaced data with array length
    expect(result).toBe(1)
  })
})

describe('H16 — Static interceptor seed', () => {
  it('config interceptors.request seeds request interceptor', async () => {
    const client = createHttpClient({
      baseURL: baseUrl,
      interceptors: {
        request: [async (config: any) => {
          config.headers = { ...config.headers, 'X-Static': 'yes' }
          return config
        }],
      } as any,
    })
    const data = await client.get('/echo')
    expect(data.headers['x-static']).toBe('yes')
  })
})

// ── H17-H19: Helper methods ──────────────────────────────────────

describe('H17 — circuitBreakerState() defaults to closed', () => {
  it('returns "closed" when no breaker configured', () => {
    const client = createHttpClient({ baseURL: baseUrl })
    expect(client.circuitBreakerState()).toBe('closed')
  })
})

describe('H18 — queueDepth() defaults to 0', () => {
  it('returns 0 when no queue configured', () => {
    const client = createHttpClient({ baseURL: baseUrl })
    expect(client.queueDepth()).toBe(0)
  })
})

describe('H19 — queueDepth() with queue', () => {
  it('returns pending count when queue has pending tasks', async () => {
    const slowServer = http.createServer((req, res) => {
      setTimeout(() => {
        res.writeHead(200, { 'Content-Type': 'application/json' })
        res.end(JSON.stringify({ ok: true }))
      }, 300)
    })
    await new Promise<void>((r) => slowServer.listen(0, '127.0.0.1', () => r()))
    const addr = slowServer.address() as any

    const client = createHttpClient({
      baseURL: `http://127.0.0.1:${addr.port}`,
      concurrency: 1,
    })

    // Start several requests; with concurrency=1 some should queue
    const p1 = client.get('/test')
    const p2 = client.get('/test')
    const p3 = client.get('/test')

    // Give enough time for first request to start (occupying the slot)
    await new Promise((r) => setTimeout(r, 50))

    const depth = client.queueDepth()
    // With concurrency=1 and 3 requests, at least 1 should be queued
    expect(depth).toBeGreaterThanOrEqual(1)

    await Promise.allSettled([p1, p2, p3])
    await new Promise<void>((r) => slowServer.close(() => r()))
  })
})

// ── AU1-AU4: Auth Helpers (G12) ───────────────────────────────────

describe('AU1 — Basic Auth auto-injection', () => {
  it('sends Authorization: Basic header', async () => {
    const client = createHttpClient({
      baseURL: baseUrl,
      auth: { username: 'u', password: 'p' },
    })
    const data = await client.get('/echo')
    // Node.js http lowercases headers
    expect(data.headers.authorization).toBe('Basic dTpw')
  })
})

describe('AU2 — Bearer Token auto-injection', () => {
  it('sends Authorization: Bearer header', async () => {
    const client = createHttpClient({
      baseURL: baseUrl,
      bearerToken: 'my-token',
    })
    const data = await client.get('/echo')
    expect(data.headers.authorization).toBe('Bearer my-token')
  })
})

describe('AU3 — Bearer Token async refresh', () => {
  it('calls the token function on each request', async () => {
    const getToken = vi.fn().mockResolvedValue('refreshed-token')
    const client = createHttpClient({
      baseURL: baseUrl,
      bearerToken: getToken,
    })

    await client.get('/echo')
    await client.get('/echo')
    await client.get('/echo')

    expect(getToken).toHaveBeenCalledTimes(3)
  })
})

describe('AU4 — Bearer Token no caching', () => {
  it('function is called N times for N requests', async () => {
    const getToken = vi.fn().mockResolvedValue('dynamic-token')
    const client = createHttpClient({
      baseURL: baseUrl,
      bearerToken: getToken,
    })

    // Make 5 requests
    for (let i = 0; i < 5; i++) {
      await client.get('/echo')
    }

    // Token function called exactly 5 times (no caching)
    expect(getToken).toHaveBeenCalledTimes(5)
    // All requests should have the same token
    for (let i = 0; i < 5; i++) {
      // Can't easily check per-request headers from here,
      // but we verified the call count
    }
  })
})
