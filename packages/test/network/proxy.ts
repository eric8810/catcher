/**
 * Lightweight TCP network proxy with configurable latency, packet loss,
 * and connection disruption — no external binary required.
 *
 * Usage:
 *   const proxy = createNetworkProxy({ targetPort: 3000 })
 *   await proxy.start()
 *   proxy.setConditions({ latency: 2000, packetLoss: 0.1 })
 *   // ... run tests ...
 *   await proxy.stop()
 *
 * IMPORTANT: latency, packetLoss, bandwidth and connectionReset are read
 * dynamically from `conditions` on every chunk. setConditions() affects
 * all active connections immediately. Call disruptAll() between test
 * cases to ensure keepAlive connections don't leak state across scenarios.
 */
import net from 'node:net'

export interface NetworkConditions {
  /** One-way latency in ms. Default: 0 */
  latency?: number
  /** Packet loss probability 0-1. Default: 0 */
  packetLoss?: number
  /** Max bandwidth in bytes/sec. 0 = unlimited */
  bandwidth?: number
  /** Probability of connection being reset mid-flight 0-1 */
  connectionReset?: number
}

export interface NetworkProxy {
  port: number
  setConditions: (c: NetworkConditions) => void
  getConditions: () => NetworkConditions
  /** Temporarily disrupt all active connections */
  disruptAll: () => void
  start: () => Promise<void>
  stop: () => Promise<void>
}

export function createNetworkProxy(targetPort: number): NetworkProxy {
  let conditions: NetworkConditions = {}
  let server: net.Server | null = null
  const activeSockets = new Set<net.Socket>()

  function delay(ms: number): Promise<void> {
    return new Promise((r) => setTimeout(r, ms))
  }

  function shouldDrop(): boolean {
    return Math.random() < (conditions.packetLoss ?? 0)
  }

  function shouldReset(): boolean {
    return Math.random() < (conditions.connectionReset ?? 0)
  }

  /**
   * Pipe data from source to dest with conditions read DYNAMICALLY
   * from the closure's `conditions` on every chunk.
   *
   * Fix: previously latency/bw were captured at connect time (#6).
   * Now setConditions() affects existing connections immediately.
   */
  function createThrottledPipe(
    source: net.Socket,
    dest: net.Socket,
  ): void {
    let buffer: Buffer[] = []
    let draining = false
    let bytesInWindow = 0
    const windowMs = 100 // sliding window for bandwidth limiting

    const flushWindow = () => {
      bytesInWindow = 0
      if (buffer.length > 0) {
        const chunk = Buffer.concat(buffer)
        buffer = []
        bytesInWindow += chunk.length
        if (!dest.write(chunk)) {
          draining = true
        }
      }
    }

    const windowTimer = setInterval(flushWindow, windowMs)

    source.on('data', async (chunk: Buffer) => {
      if (shouldDrop()) return // simulate packet loss

      const latencyMs = conditions.latency ?? 0
      if (latencyMs > 0) {
        await delay(latencyMs)
      }

      const bytesPerSec = conditions.bandwidth ?? 0
      if (bytesPerSec > 0) {
        const maxPerWindow = Math.max(1, bytesPerSec / (1000 / windowMs))
        if (bytesInWindow + chunk.length > maxPerWindow) {
          buffer.push(chunk)
          return
        }
        bytesInWindow += chunk.length
      }

      if (!dest.write(chunk)) {
        draining = true
      }
    })

    source.on('close', () => {
      clearInterval(windowTimer)
      try { dest.end() } catch {}
    })

    source.on('error', () => {
      clearInterval(windowTimer)
      try { dest.destroy() } catch {}
    })

    dest.on('drain', () => {
      draining = false
    })
  }

  const proxy: NetworkProxy = {
    port: 0,

    setConditions(c: NetworkConditions) {
      conditions = { ...c }
    },

    getConditions() {
      return { ...conditions }
    },

    disruptAll() {
      for (const sock of activeSockets) {
        try { sock.destroy() } catch {}
      }
      activeSockets.clear()
    },

    start(): Promise<void> {
      return new Promise((resolve, reject) => {
        server = net.createServer((clientSocket) => {
          const targetSocket = new net.Socket()

          targetSocket.connect(targetPort, '127.0.0.1', () => {
            activeSockets.add(clientSocket)
            activeSockets.add(targetSocket)

            // Client ↔ Target (latency/bw read dynamically per-chunk)
            createThrottledPipe(clientSocket, targetSocket)
            createThrottledPipe(targetSocket, clientSocket)

            // Random connection reset
            if (shouldReset()) {
              setTimeout(() => {
                try { clientSocket.destroy() } catch {}
              }, Math.random() * 5000)
            }
          })

          targetSocket.on('error', () => {
            try { clientSocket.destroy() } catch {}
          })

          clientSocket.on('error', () => {
            try { targetSocket.destroy() } catch {}
          })

          const cleanup = () => {
            activeSockets.delete(clientSocket)
            activeSockets.delete(targetSocket)
          }

          clientSocket.on('close', cleanup)
          targetSocket.on('close', cleanup)
        })

        server.on('error', reject)
        server.listen(0, '127.0.0.1', () => {
          proxy.port = (server!.address() as net.AddressInfo).port
          resolve()
        })
      })
    },

    stop(): Promise<void> {
      return new Promise((resolve) => {
        for (const sock of activeSockets) {
          try { sock.destroy() } catch {}
        }
        activeSockets.clear()
        if (server) {
          server.close(() => resolve())
        } else {
          resolve()
        }
      })
    },
  }

  return proxy
}
