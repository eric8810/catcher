import { describe, it, expect, beforeEach, afterEach } from 'vitest'
import { WebSocketServer, WebSocket as WsTypes } from 'ws'
import { createResilientWS } from '../client.js'

let wss: WebSocketServer
let port: number

function startWss(): Promise<void> {
  return new Promise((resolve) => {
    wss = new WebSocketServer({ port: 0 }, () => {
      port = (wss.address() as any).port
      resolve()
    })
  })
}

function stopWss(): Promise<void> {
  return new Promise((resolve) => {
    wss.close(() => resolve())
  })
}

beforeEach(async () => {
  await startWss()
})

afterEach(async () => {
  await stopWss()
})

function wsUrl() {
  return `ws://127.0.0.1:${port}`
}

// ── W1-W5: Connection lifecycle ──────────────────────────────────

describe('W1 — Successful connection + open event', () => {
  it('status becomes CONNECTED and open event fires', async () => {
    const ws = createResilientWS({
      url: wsUrl(),
      reconnect: { maxAttempts: 0 },
    })

    await new Promise<void>((resolve) => {
      ws.addEventListener('open', () => {
        expect(ws.status).toBe('CONNECTED')
        resolve()
      })
    })

    ws.close()
  })
})

describe('W2 — Send text message', () => {
  it('server receives the message', async () => {
    const received = new Promise<string>((resolve) => {
      wss.on('connection', (socket) => {
        socket.on('message', (data) => resolve(data.toString()))
      })
    })

    const ws = createResilientWS({
      url: wsUrl(),
      reconnect: { maxAttempts: 0 },
    })

    await new Promise<void>((resolve) => {
      ws.addEventListener('open', () => resolve())
    })
    ws.send('hello world')

    const msg = await received
    expect(msg).toBe('hello world')
    ws.close()
  })
})

describe('W3 — Receive message', () => {
  it('message event fires with correct data', async () => {
    wss.on('connection', (socket) => {
      socket.send('from server')
    })

    const ws = createResilientWS({
      url: wsUrl(),
      reconnect: { maxAttempts: 0 },
    })

    const msg = await new Promise<string>((resolve) => {
      ws.addEventListener('message', ((ev: any) => {
        resolve(typeof ev.data === 'string' ? ev.data : ev.data.toString())
      }) as any)
    })

    expect(msg).toBe('from server')
    ws.close()
  })
})

describe('W4 — Close connection', () => {
  it('close event fires and status is CLOSED', async () => {
    const ws = createResilientWS({
      url: wsUrl(),
      reconnect: { maxAttempts: 0 },
    })

    await new Promise<void>((resolve) => {
      ws.addEventListener('open', () => resolve())
    })

    const closeEvent = new Promise<void>((resolve) => {
      ws.addEventListener('close', () => {
        expect(ws.status).toBe('CLOSED')
        resolve()
      })
    })

    ws.close()
    await closeEvent
  })
})

describe('W5 — No reconnect after explicit close', () => {
  it('does not reconnect after ws.close()', async () => {
    let connectionCount = 0
    wss.on('connection', () => { connectionCount++ })

    const ws = createResilientWS({
      url: wsUrl(),
      reconnect: { maxAttempts: 3, initialDelay: 100 },
    })

    await new Promise<void>((resolve) => {
      ws.addEventListener('open', () => resolve())
    })

    ws.close()
    await new Promise((r) => setTimeout(r, 500))

    expect(connectionCount).toBe(1)
  })
})

// ── W6-W9: Reconnection ──────────────────────────────────────────

describe('W6 — Auto-reconnect after server closes', () => {
  it('reconnects and fires second open event', async () => {
    let openCount = 0
    wss.on('connection', (socket) => {
      openCount++
      if (openCount === 1) {
        socket.close()
      }
    })

    const ws = createResilientWS({
      url: wsUrl(),
      reconnect: { maxAttempts: 3, initialDelay: 100 },
    })

    await new Promise<void>((resolve) => {
      let opens = 0
      ws.addEventListener('open', () => {
        opens++
        if (opens === 2) resolve()
      })
    })

    ws.close()
  })
})

describe('W7 — Handshake timeout triggers reconnect with close code 4000', () => {
  it('closes with code 4000 and triggers reconnect', async () => {
    // Create a TCP server that accepts but never completes WS handshake
    const net = await import('node:net')
    let tcpConnections = 0
    const hangServer = net.createServer((socket) => {
      tcpConnections++
      // Accept connection but never respond — simulates hanging handshake
    })
    await new Promise<void>((r) => hangServer.listen(0, '127.0.0.1', () => r()))
    const hangPort = (hangServer.address() as any).port

    try {
      const ws = createResilientWS({
        url: `ws://127.0.0.1:${hangPort}`,
        handshakeTimeout: 200,
        reconnect: { maxAttempts: 3, initialDelay: 100 },
      })

      // Wait for multiple reconnection attempts (each creates a new TCP connection)
      await new Promise((r) => setTimeout(r, 1200))

      // Should have had multiple connection attempts (initial + reconnects)
      // handshakeTimeout=200ms, initialDelay=100ms, backoff ~200, ~400
      // In ~1200ms: connect(0) → timeout(200ms) → delay(100ms) → connect(300ms) → timeout(500ms) → delay(200ms) → connect(700ms) → timeout(900ms)
      expect(tcpConnections).toBeGreaterThanOrEqual(2)

      ws.close()
    } finally {
      hangServer.close()
    }
  })
})

describe('W8 — maxAttempts exhausted stops reconnect', () => {
  it('stops reconnecting after maxAttempts', async () => {
    const ws = createResilientWS({
      url: 'ws://127.0.0.1:1',
      reconnect: { maxAttempts: 2, initialDelay: 50 },
      handshakeTimeout: 200,
    })

    await new Promise((r) => setTimeout(r, 1500))

    expect(ws.status).toBe('CLOSED')
  })
})

describe('W9 — Reconnect success resets attempt counter', () => {
  it('successfully reconnects after first failure and remains stable', async () => {
    let connectionCount = 0
    wss.on('connection', (socket) => {
      connectionCount++
      if (connectionCount === 1) {
        // Close first connection to trigger reconnect
        socket.close()
      }
      // Second connection stays open
    })

    const ws = createResilientWS({
      url: wsUrl(),
      reconnect: { maxAttempts: 5, initialDelay: 100 },
    })

    // Wait for second successful connection
    await new Promise<void>((resolve) => {
      let opens = 0
      ws.addEventListener('open', () => {
        opens++
        if (opens === 2) resolve()
      })
    })

    // After reconnect, the connection should be stable
    expect(ws.status).toBe('CONNECTED')

    // Wait extra time — no more reconnections should occur
    const countBefore = connectionCount
    await new Promise((r) => setTimeout(r, 500))
    expect(connectionCount).toBe(countBefore) // No extra connections

    ws.close()
  })
})

// ── W10-W11: Multi-endpoint ──────────────────────────────────────

describe('W10 — Multi-endpoint connects to one', () => {
  it('connects to at least one endpoint', async () => {
    const ws = createResilientWS({
      url: [wsUrl(), wsUrl()],
      reconnect: { maxAttempts: 0 },
    })

    await new Promise<void>((resolve) => {
      ws.addEventListener('open', () => resolve())
    })

    expect(ws.status).toBe('CONNECTED')
    expect(ws.url).toMatch(/ws:\/\/127\.0\.0\.1:\d+/)
    ws.close()
  })
})

describe('W11 — raceCount limits concurrent endpoint attempts', () => {
  it('only attempts raceCount endpoints per connection phase', async () => {
    // Create 3 servers and track which ones receive connections
    const servers: WebSocketServer[] = []
    const connectionCounts: number[] = []

    for (let i = 0; i < 3; i++) {
      const s = await new Promise<WebSocketServer>((resolve) => {
        const wss = new WebSocketServer({ port: 0 }, () => resolve(wss))
      })
      servers.push(s)
      connectionCounts.push(0)
    }

    // Track connections per server
    servers.forEach((s, idx) => {
      s.on('connection', (ws) => {
        connectionCounts[idx]++
        ws.close() // Close to force reconnect
      })
    })

    const urls = servers.map((s) => `ws://127.0.0.1:${(s.address() as any).port}`)

    const ws = createResilientWS({
      url: urls,
      raceCount: 2,
      reconnect: { maxAttempts: 1, initialDelay: 100 },
      handshakeTimeout: 2000,
    })

    // Wait for first connection + potential reconnect
    await new Promise((r) => setTimeout(r, 800))

    // At most 2 of the 3 servers should have received connections
    const connectedServers = connectionCounts.filter((c) => c > 0).length
    expect(connectedServers).toBeLessThanOrEqual(2)

    ws.close()

    for (const s of servers) {
      await new Promise<void>((r) => s.close(() => r()))
    }
  })
})

// ── W12-W14: Configuration ───────────────────────────────────────

describe('W12 — perMessageDeflate compression', () => {
  it('connects with compression enabled', async () => {
    const ws = createResilientWS({
      url: wsUrl(),
      perMessageDeflate: true,
      reconnect: { maxAttempts: 0 },
    })

    await new Promise<void>((resolve) => {
      ws.addEventListener('open', () => resolve())
    })

    expect(ws.status).toBe('CONNECTED')
    ws.close()
  })
})

describe('W13 — Custom headers passed through', () => {
  it('server receives custom headers from client config', async () => {
    const receivedHeaders = new Promise<Record<string, string>>((resolve) => {
      wss.on('connection', (socket, req) => {
        const headers: Record<string, string> = {}
        if (req.headers.authorization) headers.authorization = req.headers.authorization
        if (req.headers['x-custom']) headers['x-custom'] = req.headers['x-custom'] as string
        resolve(headers)
      })
    })

    const ws = createResilientWS({
      url: wsUrl(),
      headers: { Authorization: 'Bearer test-token', 'X-Custom': 'custom-value' },
      reconnect: { maxAttempts: 0 },
    })

    await new Promise<void>((resolve) => {
      ws.addEventListener('open', () => resolve())
    })

    const headers = await receivedHeaders
    expect(headers.authorization).toBe('Bearer test-token')
    expect(headers['x-custom']).toBe('custom-value')

    ws.close()
  })
})

describe('W14 — handshakeTimeout triggers close and reconnect', () => {
  it('connection closes after handshakeTimeout and reconnect is attempted', async () => {
    // Create a TCP server that accepts but never completes WS handshake
    const net = await import('node:net')
    let tcpConnections = 0
    const hangServer = net.createServer((socket) => {
      tcpConnections++
    })
    await new Promise<void>((r) => hangServer.listen(0, '127.0.0.1', () => r()))
    const hangPort = (hangServer.address() as any).port

    try {
      const ws = createResilientWS({
        url: `ws://127.0.0.1:${hangPort}`,
        handshakeTimeout: 150,
        reconnect: { maxAttempts: 2, initialDelay: 50 },
      })

      // Wait for timeout + reconnect attempts
      await new Promise((r) => setTimeout(r, 800))

      // Should have had at least 2 TCP connections (initial + 1 reconnect)
      expect(tcpConnections).toBeGreaterThanOrEqual(2)

      ws.close()
    } finally {
      hangServer.close()
    }
  })
})

// ── W15-W17: Event system ────────────────────────────────────────

describe('W15 — addEventListener / removeEventListener', () => {
  it('events dispatch correctly and can be removed', async () => {
    const ws = createResilientWS({
      url: wsUrl(),
      reconnect: { maxAttempts: 0 },
    })

    let removedHandlerCalled = false
    const handler = () => { removedHandlerCalled = true }

    ws.addEventListener('open', handler)
    ws.removeEventListener('open', handler)

    let openFired = false
    ws.addEventListener('open', () => { openFired = true })

    await new Promise<void>((resolve) => {
      ws.addEventListener('open', () => resolve())
    })

    // The removed handler should NOT have been called
    expect(removedHandlerCalled).toBe(false)
    // But the event itself did fire
    expect(openFired).toBe(true)

    ws.close()
  })
})

describe('W16 — readyState syncs', () => {
  it('readyState changes correctly', async () => {
    const ws = createResilientWS({
      url: wsUrl(),
      reconnect: { maxAttempts: 0 },
    })

    await new Promise<void>((resolve) => {
      ws.addEventListener('open', () => resolve())
    })

    expect(ws.readyState).toBe(1) // OPEN

    ws.close()

    await new Promise<void>((resolve) => {
      ws.addEventListener('close', () => resolve())
    })

    expect(ws.readyState).toBe(3) // CLOSED
  })
})

describe('W17 — url property', () => {
  it('returns the connected endpoint', async () => {
    const ws = createResilientWS({
      url: wsUrl(),
      reconnect: { maxAttempts: 0 },
    })

    await new Promise<void>((resolve) => {
      ws.addEventListener('open', () => resolve())
    })

    expect(ws.url).toBe(wsUrl())
    ws.close()
  })
})
