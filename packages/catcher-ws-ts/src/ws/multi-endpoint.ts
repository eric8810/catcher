import WebSocket from 'ws'

interface RaceResult {
  socket: WebSocket
  endpoint: string
}

/**
 * Connect to multiple WebSocket endpoints simultaneously,
 * return the first to succeed. Others are closed.
 */
export function raceEndpoints(
  urls: string[],
  options: WebSocket.ClientOptions,
  timeoutMs = 15_000,
): Promise<RaceResult> {
  return new Promise((resolve, reject) => {
    const sockets: WebSocket[] = []
    let settled = false

    const settle = (url: string, socket: WebSocket) => {
      if (settled) return
      settled = true
      // Close all other sockets
      for (const s of sockets) {
        if (s !== socket) {
          try { s.close() } catch {}
        }
      }
      resolve({ socket, endpoint: url })
    }

    const fail = () => {
      if (settled) return
      const allClosed = sockets.every(
        s => s.readyState === WebSocket.CLOSED || s.readyState === WebSocket.CLOSING
      )
      if (allClosed) {
        settled = true
        reject(new Error('All WebSocket endpoints failed'))
      }
    }

    for (const url of urls) {
      const ws = new WebSocket(url, options)
      sockets.push(ws)

      ws.on('open', () => settle(url, ws))
      ws.on('error', fail)
      ws.on('close', fail)
    }

    // Global timeout
    setTimeout(() => {
      if (!settled) {
        settled = true
        for (const s of sockets) {
          try { s.close() } catch {}
        }
        reject(new Error(`WebSocket race timeout after ${timeoutMs}ms`))
      }
    }, timeoutMs)
  })
}
