import { describe, it, expect, beforeEach, afterEach } from 'vitest'
import http from 'node:http'
import { createHttpClient } from '../client.js'
import { isCatcherError } from '@eric8810/catcher-core'
import { classifyAxiosError } from '../error.js'

let server: http.Server
let baseUrl: string

function startServer(): Promise<void> {
  return new Promise((resolve) => {
    server = http.createServer((req, res) => {
      const url = new URL(req.url!, `http://${req.headers.host}`)

      // E2: 500 error endpoint
      if (url.pathname === '/error-500') {
        res.writeHead(500, { 'Content-Type': 'application/json' })
        res.end(JSON.stringify({ error: 'internal' }))
        return
      }

      // E3: Always fail with 503
      if (url.pathname === '/always-fail') {
        res.writeHead(503, { 'Content-Type': 'application/json' })
        res.end(JSON.stringify({ error: 'service unavailable' }))
        return
      }

      // E4/E5: Slow endpoint
      if (url.pathname === '/slow') {
        const delay = parseInt(url.searchParams.get('delay') ?? '10000', 10)
        setTimeout(() => {
          res.writeHead(200, { 'Content-Type': 'application/json' })
          res.end(JSON.stringify({ ok: true }))
        }, delay)
        return
      }

      // E7: Sensitive endpoint (500)
      if (url.pathname === '/sensitive') {
        res.writeHead(500, { 'Content-Type': 'application/json' })
        res.end(JSON.stringify({ error: 'sensitive' }))
        return
      }

      // E9: 403 forbidden
      if (url.pathname === '/forbidden') {
        res.writeHead(403, { 'Content-Type': 'application/json' })
        res.end(JSON.stringify({ denied: true }))
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
})

afterEach(async () => {
  await stopServer()
})

// ── E1-E9: Error Context Enrichment (G2) ─────────────────────────

describe('E1 — Network error carries request context', () => {
  it('includes url, method, type, attempt, elapsedMs on connection failure', async () => {
    const client = createHttpClient({ baseURL: 'http://127.0.0.1:1' })
    try {
      await client.get('/unreachable')
      expect.unreachable('should have thrown')
    } catch (error: any) {
      expect(error.request.url).toContain('/unreachable')
      // Method is lowercase internally ('get' not 'GET')
      expect(error.request.method.toLowerCase()).toBe('get')
      expect(error.type).toBeDefined()
      expect(error.attempt).toBeDefined()
      expect(error.elapsedMs).toBeDefined()
    }
  })
})

describe('E2 — HTTP error carries response info', () => {
  it('includes response.status and response.data on 500', async () => {
    const client = createHttpClient({ baseURL: baseUrl })
    try {
      await client.get('/error-500')
      expect.unreachable('should have thrown')
    } catch (error: any) {
      expect(error.response.status).toBe(500)
      expect(error.response.data).toBeDefined()
    }
  })
})

describe('E3 — Retry error carries attempt count', () => {
  it('records at least 2 retries on persistent 503', async () => {
    const client = createHttpClient({
      baseURL: baseUrl,
      retry: { attempts: 3, minTimeout: 10 },
    })
    try {
      await client.get('/always-fail')
      expect.unreachable('should have thrown')
    } catch (error: any) {
      expect(error.attempt).toBeGreaterThanOrEqual(2)
    }
  })
})

describe('E4 — Timeout error carries elapsedMs', () => {
  it('records elapsedMs and type=timeout on slow server', async () => {
    const client = createHttpClient({
      baseURL: baseUrl,
      timeout: 100,
    })
    try {
      await client.get('/slow?delay=10000')
      expect.unreachable('should have thrown')
    } catch (error: any) {
      expect(error.elapsedMs).toBeGreaterThanOrEqual(50)
      expect(error.type).toBe('timeout')
    }
  })
})

describe('E5 — Cancel error type correct', () => {
  it('sets type=cancelled when request is aborted', async () => {
    const client = createHttpClient({ baseURL: baseUrl })
    const ac = new AbortController()
    setTimeout(() => ac.abort(), 10)
    try {
      await client.get('/slow?delay=10000', { signal: ac.signal } as any)
      expect.unreachable('should have thrown')
    } catch (error: any) {
      expect(error.type).toBe('cancelled')
    }
  })
})

describe('E6 — Error classification correct', () => {
  it('classifies mock error codes correctly', () => {
    expect(classifyAxiosError({ code: 'ENOTFOUND' })).toBe('dns')
    expect(classifyAxiosError({ code: 'ECONNREFUSED' })).toBe('connection')
    expect(classifyAxiosError({ code: 'UNABLE_TO_VERIFY_LEAF_SIGNATURE' })).toBe('tls')
    expect(classifyAxiosError({ code: 'ERR_BAD_RESPONSE', response: { status: 500 } })).toBe('http')
  })
})

describe('E7 — toJSON() redacts sensitive headers', () => {
  it('does not expose Authorization value in serialized error', async () => {
    const client = createHttpClient({ baseURL: baseUrl })
    try {
      await client.get('/sensitive', {
        headers: { Authorization: 'Bearer secret-token-123' },
      } as any)
      expect.unreachable('should have thrown')
    } catch (error: any) {
      const serialized = JSON.stringify(error.toJSON())
      expect(serialized).not.toContain('secret-token-123')
    }
  })
})

describe('E8 — isCatcherError() detection', () => {
  it('returns true for CatcherHttpError and false for plain Error', async () => {
    // True case: real failed request
    const client = createHttpClient({ baseURL: 'http://127.0.0.1:1' })
    try {
      await client.get('/unreachable')
      expect.unreachable('should have thrown')
    } catch (error: any) {
      expect(isCatcherError(error)).toBe(true)
    }

    // False case: plain Error
    expect(isCatcherError(new Error('normal'))).toBe(false)
  })
})

describe('E9 — 4xx error also carries context', () => {
  it('includes request and response on 403', async () => {
    const client = createHttpClient({ baseURL: baseUrl })
    try {
      await client.get('/forbidden')
      expect.unreachable('should have thrown')
    } catch (error: any) {
      expect(error.request).toBeDefined()
      expect(error.response.status).toBe(403)
      expect(error.response.data).toBeDefined()
    }
  })
})
