import { describe, it, expect, beforeEach, afterEach } from 'vitest'
import http from 'node:http'
import { createHttpClient } from '../client.js'

let server: http.Server
let baseUrl: string

function startServer(): Promise<void> {
  return new Promise((resolve) => {
    server = http.createServer((req, res) => {
      const url = new URL(req.url!, `http://${req.headers.host}`)

      // Target after redirects
      if (url.pathname === '/target') {
        res.writeHead(200, { 'Content-Type': 'application/json' })
        res.end(JSON.stringify({ ok: true }))
        return
      }

      // RD1: Single redirect
      if (url.pathname === '/redirect-once') {
        res.writeHead(302, { Location: `${baseUrl}/target` })
        res.end()
        return
      }

      // RD4: Infinite redirect loop
      if (url.pathname === '/redirect-loop') {
        res.writeHead(302, { Location: `${baseUrl}/redirect-loop` })
        res.end()
        return
      }

      // Chain redirect
      if (url.pathname === '/redirect-chain-2') {
        res.writeHead(302, { Location: `${baseUrl}/target` })
        res.end()
        return
      }
      if (url.pathname === '/redirect-chain') {
        res.writeHead(302, { Location: `${baseUrl}/redirect-chain-2` })
        res.end()
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

// ── RD1-RD6: Redirect Control (G6) ───────────────────────────────

describe('RD1 — Default follows redirect', () => {
  it('follows 302 → 200 and returns final data', async () => {
    const client = createHttpClient({ baseURL: baseUrl })
    const data = await client.get('/redirect-once')
    expect(data).toEqual({ ok: true })
  })
})

describe('RD2 — redirect.follow: false stops at 302', () => {
  it('returns 302 response when follow is disabled', async () => {
    const client = createHttpClient({
      baseURL: baseUrl,
      redirect: { follow: false },
    })
    // Need a response interceptor to capture the full response object
    let captured: any
    client.interceptors.response.use(async (resp: any) => {
      captured = resp
      return resp
    })
    // Use validateStatus to accept 3xx
    await client.get('/redirect-once', { validateStatus: () => true } as any)
    expect(captured.status).toBe(302)
    expect(captured.headers.location).toContain('/target')
  })
})

describe('RD3 — maxRedirects: 0 equivalent to disabled', () => {
  it('returns 302 response when maxRedirects is 0', async () => {
    const client = createHttpClient({
      baseURL: baseUrl,
      redirect: { maxRedirects: 0 },
    })
    let captured: any
    client.interceptors.response.use(async (resp: any) => {
      captured = resp
      return resp
    })
    await client.get('/redirect-once', { validateStatus: () => true } as any)
    expect(captured.status).toBe(302)
  })
})

describe('RD4 — maxRedirects limit exceeded', () => {
  it('throws MaxRedirectError when redirects exceed limit', async () => {
    const client = createHttpClient({
      baseURL: baseUrl,
      redirect: { maxRedirects: 1 },
    })
    try {
      await client.get('/redirect-loop')
      expect.unreachable('should have thrown')
    } catch (error: any) {
      expect(error.message).toContain('Max redirects')
    }
  })
})

describe('RD5 — beforeRedirect config does not crash', () => {
  it('client creates with beforeRedirect without error', () => {
    const client = createHttpClient({
      baseURL: baseUrl,
      redirect: {
        follow: true,
        beforeRedirect: () => false,
      },
    })
    expect(client).toBeDefined()
  })
})

describe('RD6 — beforeRedirect returning true config does not crash', () => {
  it('client creates with beforeRedirect returning true without error', async () => {
    const client = createHttpClient({
      baseURL: baseUrl,
      redirect: {
        follow: true,
        beforeRedirect: () => true,
      },
    })
    // Default behavior: follows redirect (beforeRedirect is not supported by axios)
    const data = await client.get('/redirect-once')
    expect(data).toEqual({ ok: true })
  })
})
