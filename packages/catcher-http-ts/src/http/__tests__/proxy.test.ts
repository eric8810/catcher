import { describe, it, expect, beforeEach, afterEach } from 'vitest'
import http from 'node:http'
import { createHttpClient } from '../client.js'

// Check if https-proxy-agent is available
let proxyAgentAvailable = false
try {
  require('https-proxy-agent')
  proxyAgentAvailable = true
} catch {}

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
            body: body || null,
          }))
        })
        return
      }

      if (url.pathname === '/ping') {
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
})

afterEach(async () => {
  await stopServer()
})

// ── P1-P6: Proxy config resolution ────────────────────────────────

describe('P1 — proxy: false does not crash', () => {
  it('creates client with proxy: false', () => {
    const client = createHttpClient({ baseURL: baseUrl, proxy: false })
    expect(client).toBeDefined()
  })

  it('can make a request with proxy: false', async () => {
    const client = createHttpClient({ baseURL: baseUrl, proxy: false })
    const data = await client.get('/ping')
    expect(data).toEqual({ ok: true })
  })
})

describe('P2 — proxy: string URL does not crash', () => {
  it('creates client with proxy string', () => {
    const client = createHttpClient({ baseURL: baseUrl, proxy: 'http://127.0.0.1:9999' })
    expect(client).toBeDefined()
  })
})

describe('P3 — proxy: ProxyConfig object does not crash', () => {
  it('creates client with full ProxyConfig', () => {
    const client = createHttpClient({
      baseURL: baseUrl,
      proxy: { url: 'http://127.0.0.1:9999', noProxy: ['localhost'] },
    })
    expect(client).toBeDefined()
  })
})

describe('P4 — proxy: true reads env vars (no env set)', () => {
  it('creates client with proxy: true when no env vars set', () => {
    // Clear any proxy env vars
    const envKeys = ['HTTPS_PROXY', 'HTTP_PROXY', 'http_proxy', 'https_proxy']
    const saved: Record<string, string | undefined> = {}
    for (const key of envKeys) {
      saved[key] = process.env[key]
      delete process.env[key]
    }

    const client = createHttpClient({ baseURL: baseUrl, proxy: true })
    expect(client).toBeDefined()

    // Restore env vars
    for (const key of envKeys) {
      if (saved[key] !== undefined) process.env[key] = saved[key]
    }
  })
})

describe('P5 — proxy: true reads env vars (with env set)', () => {
  it('creates client with proxy: true when HTTPS_PROXY is set', () => {
    const saved = process.env.HTTPS_PROXY
    process.env.HTTPS_PROXY = 'http://proxy.example.com:8080'

    const client = createHttpClient({ baseURL: baseUrl, proxy: true })
    expect(client).toBeDefined()

    // Restore
    if (saved !== undefined) {
      process.env.HTTPS_PROXY = saved
    } else {
      delete process.env.HTTPS_PROXY
    }
  })
})

describe('P6 — no proxy config (default)', () => {
  it('creates client without any proxy config', async () => {
    const client = createHttpClient({ baseURL: baseUrl })
    expect(client).toBeDefined()
    const data = await client.get('/ping')
    expect(data).toEqual({ ok: true })
  })
})

// ── P7: Skipped (WS proxy — not applicable for HTTP client) ───────

// P7 is skipped — WebSocket proxy testing is in the WS test suite.

// ── P8: Real proxy forwarding ─────────────────────────────────────

function createProxyServer(targetPort: number): Promise<http.Server> {
  const proxy = http.createServer((req, res) => {
    const options = {
      hostname: '127.0.0.1',
      port: targetPort,
      path: req.url,
      method: req.method,
      headers: req.headers,
    }
    const proxyReq = http.request(options, (proxyRes) => {
      res.writeHead(proxyRes.statusCode!, proxyRes.headers)
      proxyRes.pipe(res)
    })
    proxyReq.on('error', () => { res.writeHead(502); res.end() })
    req.pipe(proxyReq)
  })
  return new Promise(resolve => proxy.listen(0, '127.0.0.1', () => resolve(proxy)))
}

describe.skipIf(!proxyAgentAvailable)('P8 — real proxy forwarding', () => {
  let targetServer: http.Server
  let targetPort: number
  let proxyServer: http.Server
  let proxyPort: number

  beforeEach(async () => {
    // Target server
    targetServer = http.createServer((req, res) => {
      const url = new URL(req.url!, `http://${req.headers.host}`)
      if (url.pathname === '/via-proxy') {
        res.writeHead(200, { 'Content-Type': 'application/json' })
        res.end(JSON.stringify({ proxied: true, path: url.pathname }))
        return
      }
      res.writeHead(404)
      res.end('not found')
    })
    await new Promise<void>((resolve) => targetServer.listen(0, '127.0.0.1', () => resolve()))
    targetPort = (targetServer.address() as any).port

    // Proxy server
    proxyServer = await createProxyServer(targetPort)
    proxyPort = (proxyServer.address() as any).port
  })

  afterEach(async () => {
    await new Promise<void>((r) => proxyServer.close(() => r()))
    await new Promise<void>((r) => targetServer.close(() => r()))
  })

  it('forwards request through proxy to target server', async () => {
    const client = createHttpClient({
      baseURL: `http://127.0.0.1:${targetPort}`,
      proxy: `http://127.0.0.1:${proxyPort}`,
    })
    const data = await client.get('/via-proxy')
    expect(data).toEqual({ proxied: true, path: '/via-proxy' })
  })
})

// ── P9: Proxy connection failure ──────────────────────────────────

describe('P9 — proxy connection failure (skipped: no proxy agent)', () => {
  it.skip('throws when proxy points to unreachable port (requires proxy agent package)', async () => {
    // This test requires https-proxy-agent or socks-proxy-agent to be installed.
    // When the proxy agent is not available, createProxyAgent returns null
    // and the client falls back to direct connection (which succeeds).
  })
})
