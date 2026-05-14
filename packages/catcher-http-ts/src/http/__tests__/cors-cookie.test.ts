import { describe, it, expect, beforeEach, afterEach } from 'vitest'
import http from 'node:http'
import { createHttpClient } from '../client.js'

let server: http.Server
let baseUrl: string

function startServer(): Promise<void> {
  return new Promise((resolve) => {
    server = http.createServer((req, res) => {
      const url = new URL(req.url!, `http://${req.headers.host}`)

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

// ── C1-C4: CORS / Credentials (Node.js axios layer) ──────────────
//
// NOTE: withCredentials is a browser concept (controls cookie sending).
// In Node.js, axios sets the flag but there is no cookie jar, so the
// observable behavior is the same whether it's true or false.
//
// These tests verify:
// 1. The config is accepted without error (smoke test)
// 2. The code path is exercised (no crash)
//
// The real behavior (cookies, CORS) is properly tested in
// catcher-web (C5-C9) where globalThis.fetch is mocked and
// we verify the actual `credentials` / `mode` parameters.

describe('C1 — withCredentials: true config accepted', () => {
  it('creates client and completes request', async () => {
    const client = createHttpClient({ baseURL: baseUrl, withCredentials: true })
    const data = await client.get('/echo')
    expect(data.method).toBe('GET')
    expect(data.url).toBe('/echo')
  })
})

describe('C2 — Per-request credentials: "include" accepted', () => {
  it('completes request with credentials: "include"', async () => {
    const client = createHttpClient({ baseURL: baseUrl })
    const data = await client.get('/echo', { credentials: 'include' } as any)
    expect(data.method).toBe('GET')
    expect(data.url).toBe('/echo')
  })
})

describe('C3 — credentials: "omit" accepted', () => {
  it('completes request with credentials: "omit"', async () => {
    const client = createHttpClient({ baseURL: baseUrl })
    const data = await client.get('/echo', { credentials: 'omit' } as any)
    expect(data.method).toBe('GET')
    expect(data.url).toBe('/echo')
  })
})

describe('C4 — No credentials config (default)', () => {
  it('completes request without any credentials configuration', async () => {
    const client = createHttpClient({ baseURL: baseUrl })
    const data = await client.get('/echo')
    expect(data.method).toBe('GET')
    expect(data.url).toBe('/echo')
  })
})
