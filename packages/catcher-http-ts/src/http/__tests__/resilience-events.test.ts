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
            body: body || null,
          }))
        })
        return
      }

      // Fail once then succeed (for retry tests)
      if (url.pathname === '/fail-once') {
        const count = (globalThis as any).__failOnceCount ?? 0
        ;(globalThis as any).__failOnceCount = count + 1
        if (count < 1) {
          res.writeHead(503, { 'Content-Type': 'application/json' })
          res.end(JSON.stringify({ error: 'temporarily unavailable' }))
          return
        }
        res.writeHead(200, { 'Content-Type': 'application/json' })
        res.end(JSON.stringify({ ok: true }))
        return
      }

      // Always fail (for retry exhaustion tests)
      if (url.pathname === '/always-fail') {
        res.writeHead(503, { 'Content-Type': 'application/json' })
        res.end(JSON.stringify({ error: 'service unavailable' }))
        return
      }

      // Slow endpoint
      if (url.pathname === '/slow') {
        setTimeout(() => {
          res.writeHead(200, { 'Content-Type': 'application/json' })
          res.end(JSON.stringify({ ok: true }))
        }, parseInt(url.searchParams.get('delay') ?? '5000', 10))
        return
      }

      // Fail always (for CB tests)
      if (url.pathname === '/cb-fail') {
        res.writeHead(500)
        res.end('error')
        return
      }

      // CB toggle: fail or succeed
      if (url.pathname === '/cb-toggle') {
        if ((globalThis as any).__cbShouldFail) {
          res.writeHead(500)
          res.end('error')
        } else {
          res.writeHead(200, { 'Content-Type': 'application/json' })
          res.end(JSON.stringify({ ok: true }))
        }
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
  ;(globalThis as any).__failOnceCount = 0
  ;(globalThis as any).__cbShouldFail = true
})

afterEach(async () => {
  await stopServer()
})

// ── RE1-RE9: Resilience Runtime Control (G11) ─────────────────────

describe('RE1 — on("retry") fires on retry', () => {
  it('retry event listener receives attempt and error', async () => {
    const client = createHttpClient({
      baseURL: baseUrl,
      retry: { attempts: 2, minTimeout: 10 },
    })

    const retryListener = vi.fn()
    ;(client as any).on('retry', retryListener)

    ;(globalThis as any).__failOnceCount = 0
    await client.get('/fail-once')

    expect(retryListener).toHaveBeenCalled()
    const event = retryListener.mock.calls[0][0]
    expect(event.type).toBe('retry')
    expect(event.attempt).toBeDefined()
  })
})

describe('RE2 — Circuit breaker state transitions', () => {
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

describe('RE3 — Circuit breaker recovery', () => {
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

    // Next request succeeds (half-open → closed)
    const result = await client.get('/test')
    expect(result).toEqual({ ok: true })
    expect(client.circuitBreakerState()).toBe('closed')

    await new Promise<void>((r) => cbServer.close(() => r()))
  })
})

describe('RE4 — on("requestComplete") fires on normal request', () => {
  it('event carries method, url, status, durationMs', async () => {
    const client = createHttpClient({ baseURL: baseUrl })

    const completeListener = vi.fn()
    ;(client as any).on('requestComplete', completeListener)

    await client.get('/echo')

    expect(completeListener).toHaveBeenCalledTimes(1)
    const event = completeListener.mock.calls[0][0]
    expect(event.type).toBe('requestComplete')
    expect(event.method.toLowerCase()).toBe('get')
    expect(event.url).toContain('/echo')
    expect(event.status).toBe(200)
    expect(typeof event.durationMs).toBe('number')
    expect(event.durationMs).toBeGreaterThanOrEqual(0)
  })
})

describe('RE5 — on() returns unsubscribe function', () => {
  it('unsubscribe stops events', async () => {
    const client = createHttpClient({ baseURL: baseUrl })

    const listener = vi.fn()
    const unsub = (client as any).on('requestComplete', listener)

    // First request — listener should fire
    await client.get('/echo')
    expect(listener).toHaveBeenCalledTimes(1)

    // Unsubscribe
    unsub()

    // Second request — listener should NOT fire
    await client.get('/echo')
    expect(listener).toHaveBeenCalledTimes(1) // still 1
  })
})

describe('RE6 — off() cancels subscription', () => {
  it('off removes the listener', async () => {
    const client = createHttpClient({ baseURL: baseUrl })

    const listener = vi.fn()
    ;(client as any).on('requestComplete', listener)

    await client.get('/echo')
    expect(listener).toHaveBeenCalledTimes(1)

    ;(client as any).off('requestComplete', listener)

    await client.get('/echo')
    expect(listener).toHaveBeenCalledTimes(1) // still 1
  })
})

describe('RE7 — updateConfig() modifies retry', () => {
  it('reduces retry attempts after updateConfig', async () => {
    const client = createHttpClient({
      baseURL: baseUrl,
      retry: { attempts: 5, minTimeout: 10 },
    })

    // Update to 0 retries
    ;(client as any).updateConfig({ retry: { attempts: 0, minTimeout: 10 } })

    // Server always returns 503 — should fail immediately (1 attempt, no retry)
    ;(globalThis as any).__failOnceCount = 0
    try {
      await client.get('/always-fail')
      expect.unreachable('should have thrown')
    } catch (error: any) {
      // With 0 retries, there should be only 1 total attempt
      expect(error.attempt).toBeLessThanOrEqual(1)
    }
  })
})

describe('RE8 — updateConfig() accepts retry and timeout', () => {
  it('does not crash when updating retry or timeout', () => {
    const client = createHttpClient({ baseURL: baseUrl })

    expect(() => {
      ;(client as any).updateConfig({ retry: { attempts: 2, minTimeout: 50 } })
    }).not.toThrow()

    expect(() => {
      ;(client as any).updateConfig({ timeout: 5000 })
    }).not.toThrow()
  })
})

describe('RE9 — updateConfig() only affects subsequent requests', () => {
  it('in-flight request keeps original timeout; new request uses updated timeout', async () => {
    // Create client with a generous timeout so the slow request succeeds
    const client = createHttpClient({ baseURL: baseUrl, timeout: 30_000 })

    // 1. Start a request that takes 200ms (well within the 30s timeout)
    const inflightPromise = client.get('/slow?delay=200')

    // Give the request a moment to be dispatched by axios (captures timeout)
    await new Promise((r) => setTimeout(r, 20))

    // 2. Now shorten the timeout to 50ms for future requests
    ;(client as any).updateConfig({ timeout: 50 })

    // 3. The in-flight request should complete successfully
    //    (it was dispatched with the original 30s timeout)
    const result = await inflightPromise
    expect(result).toEqual({ ok: true })

    // 4. A NEW slow request should now fail with the shortened 50ms timeout
    try {
      await client.get('/slow?delay=10000')
      expect.unreachable('should have timed out')
    } catch (error: any) {
      expect(error.type).toBe('timeout')
    }
  })
})
