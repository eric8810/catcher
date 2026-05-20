/**
 * Local HTTP test server — simulates a typical IM API Gateway.
 *
 * Endpoints:
 *   POST /auth              → login, returns token
 *   GET  /users/:id         → user profile
 *   GET  /channels          → channel list
 *   GET  /channels/:id/messages → paginated messages
 *   POST /messages          → send a message
 *
 * All endpoints accept a ?delay=ms query param to simulate server processing time.
 */

import http from 'node:http'

export interface TestServer {
  url: string
  port: number
  close: () => Promise<void>
}

export function createHttpTestServer(): Promise<TestServer> {
  return new Promise((resolve) => {
    const connections = new Set<import('net').Socket>()
    const server = http.createServer((req, res) => {
      const url = new URL(req.url ?? '/', `http://${req.headers.host}`)
      const delay = parseInt(url.searchParams.get('delay') ?? '0', 10)

      const respond = (status: number, body: unknown) => {
        const payload = JSON.stringify(body)
        res.writeHead(status, {
          'Content-Type': 'application/json',
          'Content-Length': Buffer.byteLength(payload),
        })
        res.end(payload)
      }

      const handle = () => {
        // Simulate varying response sizes
        if (url.pathname === '/auth' && req.method === 'POST') {
          respond(200, { token: 'tok_' + Math.random().toString(36).slice(2), expiresIn: 3600 })
        } else if (url.pathname.startsWith('/users/') && req.method === 'GET') {
          const userId = url.pathname.split('/')[2]
          respond(200, { id: userId, name: 'User ' + userId, avatar: 'https://cdn.example.com/avatar/' + userId })
        } else if (url.pathname === '/channels' && req.method === 'GET') {
          const channels = Array.from({ length: 20 }, (_, i) => ({
            id: 'ch_' + i,
            name: 'Channel ' + i,
            unread: Math.floor(Math.random() * 99),
          }))
          respond(200, channels)
        } else if (url.pathname.startsWith('/channels/') && url.pathname.endsWith('/messages') && req.method === 'GET') {
          const limit = parseInt(url.searchParams.get('pageSize') ?? '50', 10)
          const messages = Array.from({ length: limit }, (_, i) => ({
            id: 'msg_' + i,
            from: 'user_' + (i % 5),
            text: 'Hello world ' + i + '! '.repeat(10), // ~200 bytes per message
            ts: Date.now() - i * 60000,
          }))
          respond(200, { messages, hasMore: limit >= 50 })
        } else if (url.pathname === '/messages' && req.method === 'POST') {
          let body = ''
          req.on('data', (chunk: Buffer) => { body += chunk.toString() })
          req.on('end', () => {
            try {
              const parsed = JSON.parse(body || '{}')
              respond(200, { id: 'msg_' + Date.now(), text: parsed.text, ts: Date.now(), status: 1 })
            } catch {
              respond(400, { error: 'invalid json' })
            }
          })
          return // prevent double respond
        } else if (url.pathname === '/upload' && req.method === 'POST') {
          // Simulate 2MB upload
          let received = 0
          req.on('data', (chunk: Buffer) => { received += chunk.length })
          req.on('end', () => {
            respond(200, { url: 'https://cdn.example.com/img/' + Date.now(), size: received })
          })
          return
        } else if (url.pathname === '/large-messages' && req.method === 'GET') {
          // Large payload: 50 messages with metadata (~15KB JSON)
          const limit = parseInt(url.searchParams.get('count') ?? '50', 10)
          const messages = Array.from({ length: limit }, (_, i) => ({
            id: 'msg_' + String(i).padStart(8, '0'),
            from: 'user_' + (i % 20),
            to: 'channel_general',
            text: 'The quick brown fox jumps over the lazy dog. '.repeat(5), // ~220 bytes
            ts: Date.now() - i * 30000,
            status: i % 3,
            metadata: {
              platform: i % 2 === 0 ? 'desktop' : 'mobile',
              version: '2.' + (i % 5) + '.0',
              geo: { lat: 31.23 + i * 0.01, lng: 121.47 + i * 0.01 },
            },
          }))
          respond(200, { messages, hasMore: true, total: 999 })
        } else if (url.pathname === '/slow' && req.method === 'GET') {
          // Slow endpoint — simulates heavy backend processing
          const serverDelay = parseInt(url.searchParams.get('delay') ?? '500', 10)
          setTimeout(() => {
            respond(200, { ok: true, delay: serverDelay })
          }, serverDelay)
        } else if (url.pathname === '/avatar' && req.method === 'GET') {
          // Simulate avatar loading (low priority)
          const uid = url.searchParams.get('uid') ?? '0'
          respond(200, { uid, url: 'https://cdn.example.com/avatar/' + uid + '.png' })
        } else {
          respond(404, { error: 'not found' })
        }
      }

      if (delay > 0) {
        setTimeout(handle, delay)
      } else {
        handle()
      }
    })

    server.on('connection', (socket) => {
      connections.add(socket)
      socket.on('close', () => connections.delete(socket))
    })

    server.listen(0, '127.0.0.1', () => {
      const addr = server.address() as { port: number }
      resolve({
        url: `http://127.0.0.1:${addr.port}`,
        port: addr.port,
        close: () => new Promise<void>((res) => {
          for (const socket of connections) socket.destroy()
          server.close(() => res())
        }),
      })
    })
  })
}
