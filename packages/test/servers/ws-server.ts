/**
 * Local WebSocket test server — simulates IM real-time push.
 *
 * Features:
 *   - Echo mode: sends back whatever it receives
 *   - Broadcast mode: pushes periodic heartbeats + simulated messages
 *   - Configurable message size for bandwidth testing
 */

import { WebSocketServer, WebSocket } from 'ws'

export interface WSTestServer {
  url: string
  port: number
  broadcast: (data: string | Buffer) => void
  close: () => Promise<void>
}

export function createWSTestServer(): Promise<WSTestServer> {
  return new Promise((resolve) => {
    const wss = new WebSocketServer({ host: '127.0.0.1', port: 0 })
    const clients = new Set<WebSocket>()

    wss.on('connection', (ws) => {
      clients.add(ws)

      // Echo mode — send back whatever client sends
      ws.on('message', (data) => {
        ws.send(data)
      })

      // Start heartbeat
      const heartbeat = setInterval(() => {
        if (ws.readyState === WebSocket.OPEN) {
          ws.send(JSON.stringify({ type: 'ping', ts: Date.now() }))
        }
      }, 5000)

      ws.on('close', () => {
        clients.delete(ws)
        clearInterval(heartbeat)
      })

      ws.on('error', () => {
        clients.delete(ws)
        clearInterval(heartbeat)
      })
    })

    wss.on('listening', () => {
      const addr = wss.address() as { port: number }
      resolve({
        url: `ws://127.0.0.1:${addr.port}`,
        port: addr.port,
        broadcast(data: string | Buffer) {
          for (const client of clients) {
            if (client.readyState === WebSocket.OPEN) {
              client.send(data)
            }
          }
        },
        close: () =>
          new Promise((res) => {
            for (const client of clients) {
              client.close()
            }
            wss.close(() => res())
          }),
      })
    })
  })
}
