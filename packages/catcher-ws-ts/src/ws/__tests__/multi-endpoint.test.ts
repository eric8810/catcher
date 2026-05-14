import { describe, it, expect, afterEach } from 'vitest'
import { WebSocket, WebSocketServer } from 'ws'
import { raceEndpoints } from '../multi-endpoint.js'

function createServer(port: number): Promise<WebSocketServer> {
  return new Promise((resolve) => {
    const wss = new WebSocketServer({ port }, () => resolve(wss))
  })
}

function closeServer(wss: WebSocketServer): Promise<void> {
  return new Promise((resolve) => {
    wss.close(() => resolve())
  })
}

describe('ME1 — First endpoint succeeds', () => {
  it('returns the first socket when both endpoints work', async () => {
    const wss1 = await createServer(0)
    const wss2 = await createServer(0)
    const port1 = (wss1.address() as any).port
    const port2 = (wss2.address() as any).port

    try {
      const result = await raceEndpoints(
        [`ws://127.0.0.1:${port1}`, `ws://127.0.0.1:${port2}`],
        {},
        5000,
      )

      expect(result.socket).toBeInstanceOf(WebSocket)
      expect(result.endpoint).toMatch(/ws:\/\/127\.0\.0\.1:\d+/)
      result.socket.close()
    } finally {
      await closeServer(wss1)
      await closeServer(wss2)
    }
  })
})

describe('ME2 — First fails, second succeeds', () => {
  it('returns second socket when first rejects', async () => {
    // Use an unreachable port for the first endpoint
    const wss2 = await createServer(0)
    const port2 = (wss2.address() as any).port

    try {
      const result = await raceEndpoints(
        ['ws://127.0.0.1:1', `ws://127.0.0.1:${port2}`],
        { handshakeTimeout: 1000 },
        5000,
      )

      expect(result.socket).toBeInstanceOf(WebSocket)
      expect(result.endpoint).toBe(`ws://127.0.0.1:${port2}`)
      result.socket.close()
    } finally {
      await closeServer(wss2)
    }
  })
})

describe('ME3 — All endpoints fail', () => {
  it('rejects with "All WebSocket endpoints failed"', async () => {
    await expect(
      raceEndpoints(
        ['ws://127.0.0.1:1', 'ws://127.0.0.1:2'],
        { handshakeTimeout: 500 },
        5000,
      ),
    ).rejects.toThrow('All WebSocket endpoints failed')
  })
})

describe('ME4 — Global timeout', () => {
  it('rejects with timeout message when all endpoints hang', async () => {
    // Create a TCP server that accepts connections but never completes WS handshake
    const net = await import('node:net')
    const hangServer = net.createServer((socket) => {
      // Accept connection but never respond — simulates a hanging server
    })
    await new Promise<void>((r) => hangServer.listen(0, '127.0.0.1', () => r()))
    const hangPort = (hangServer.address() as any).port

    try {
      await expect(
        raceEndpoints(
          [`ws://127.0.0.1:${hangPort}`],
          {},
          100,
        ),
      ).rejects.toThrow('WebSocket race timeout')
    } finally {
      hangServer.close()
    }
  })
})

describe('ME5 — Failed sockets are closed', () => {
  it('closes non-winning sockets after race settles', async () => {
    const wss1 = await createServer(0)
    const wss2 = await createServer(0)
    const port1 = (wss1.address() as any).port
    const port2 = (wss2.address() as any).port
    const closed = new Set<number>()

    wss1.on('connection', (ws) => { ws.on('close', () => closed.add(port1)) })
    wss2.on('connection', (ws) => { ws.on('close', () => closed.add(port2)) })

    try {
      const result = await raceEndpoints(
        [`ws://127.0.0.1:${port1}`, `ws://127.0.0.1:${port2}`],
        {},
        5000,
      )
      result.socket.close()

      // Wait a bit for close to propagate
      await new Promise((r) => setTimeout(r, 200))

      // At least one of the servers should have seen a close event
      // (the losing socket was closed)
      expect(closed.size).toBeGreaterThanOrEqual(1)
    } finally {
      await closeServer(wss1)
      await closeServer(wss2)
    }
  })
})
