/**
 * Lightweight TCP network proxy with configurable damage models —
 * no external binary required.
 *
 * Damage models (applied per-chunk, read dynamically):
 *   blackhole — silent drop all packets (route black hole)
 *   jitter    — random latency fluctuation
 *   burstLoss — Gilbert-Elliott 2-state Markov model
 *   asymmetric — different conditions per direction (upload/download)
 *   bwFluc   — periodic bandwidth fluctuation
 *   corrupt   — random byte corruption
 *   reorder   — packet reordering
 *   duplicate — packet duplication
 *
 * Usage:
 *   const proxy = createNetworkProxy(3000)
 *   await proxy.start()
 *   proxy.setConditions({ latency: 2000, jitter: 500, packetLoss: 0.1 })
 *   // ... run tests ...
 *   await proxy.stop()
 */

import net from 'node:net'

// ── Types ─────────────────────────────────────────────────────

export interface BurstLossConfig {
  /** GOOD → BAD transition probability per chunk */
  p_good_to_bad: number
  /** BAD → GOOD transition probability per chunk */
  p_bad_to_good: number
  /** Packet loss rate in GOOD state (0-1) */
  loss_good: number
  /** Packet loss rate in BAD state (0-1) */
  loss_bad: number
}

export interface DirectionConditions {
  latency?: number
  jitter?: number
  jitterDistribution?: 'uniform' | 'normal'
  packetLoss?: number
  bandwidth?: number
  bandwidthFluctuation?: number
  burstLoss?: BurstLossConfig
  corrupt?: number
  reorder?: { probability: number; delayMs: number }
  duplicate?: number
  connectionReset?: number
}

export interface BlackholeConfig {
  enabled: boolean
  /** Duration in ms. 0 = until manually disabled */
  duration?: number
  /** Delay before blackhole starts (ms) */
  startAfter?: number
  /** Destroy all zombie connections when blackhole ends */
  destroyOnRecover?: boolean
}

export interface NetworkConditions {
  // ── Symmetric damage (backward compatible) ──
  latency?: number
  jitter?: number
  jitterDistribution?: 'uniform' | 'normal'
  packetLoss?: number
  bandwidth?: number
  bandwidthFluctuation?: number
  connectionReset?: number
  corrupt?: number
  reorder?: { probability: number; delayMs: number }
  duplicate?: number

  // ── Burst loss (overrides packetLoss when set) ──
  burstLoss?: BurstLossConfig

  // ── Route black hole ──
  blackhole?: BlackholeConfig

  // ── Asymmetric (overrides symmetric params when set) ──
  upload?: DirectionConditions
  download?: DirectionConditions
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

// ── Helpers ───────────────────────────────────────────────────

function delay(ms: number): Promise<void> {
  return new Promise((r) => setTimeout(r, ms))
}

/** Resolve effective direction conditions (direction overrides symmetric) */
function resolveDir(
  conds: NetworkConditions,
  dir: 'upload' | 'download' | undefined,
): {
  latency: number
  jitter: number
  jitterDistribution: 'uniform' | 'normal'
  packetLoss: number
  bandwidth: number
  bandwidthFluctuation: number
  connectionReset: number
  burstLoss: BurstLossConfig | null
  corrupt: number
  reorderProb: number
  reorderDelayMs: number
  duplicate: number
} {
  const d = dir ? conds[dir] : undefined
  const burstLoss =
    d?.burstLoss ?? conds.burstLoss ?? null
  const reorder = d?.reorder ?? conds.reorder
  return {
    latency: d?.latency ?? conds.latency ?? 0,
    jitter: d?.jitter ?? conds.jitter ?? 0,
    jitterDistribution: d?.jitterDistribution ?? conds.jitterDistribution ?? 'uniform',
    packetLoss: d?.packetLoss ?? conds.packetLoss ?? 0,
    bandwidth: d?.bandwidth ?? conds.bandwidth ?? 0,
    bandwidthFluctuation: d?.bandwidthFluctuation ?? conds.bandwidthFluctuation ?? 0,
    connectionReset: d?.connectionReset ?? conds.connectionReset ?? 0,
    burstLoss,
    corrupt: d?.corrupt ?? conds.corrupt ?? 0,
    reorderProb: reorder?.probability ?? 0,
    reorderDelayMs: reorder?.delayMs ?? 0,
    duplicate: d?.duplicate ?? conds.duplicate ?? 0,
  }
}

/** Compute actual delay with jitter */
function actualDelay(latency: number, jitter: number, dist: 'uniform' | 'normal'): number {
  if (jitter <= 0) return latency
  if (dist === 'normal') {
    // Box-Muller — clamp to ±3σ
    let u = 0, v = 0
    while (u === 0) u = Math.random()
    while (v === 0) v = Math.random()
    const gauss = Math.sqrt(-2 * Math.log(u)) * Math.cos(2 * Math.PI * v)
    const dev = Math.max(-3, Math.min(3, gauss)) * jitter
    return Math.max(0, latency + Math.round(dev))
  }
  // uniform
  return Math.max(0, latency + Math.round((Math.random() * 2 - 1) * jitter))
}

// ── Gilbert-Elliott state ─────────────────────────────────────

function createBurstLossEngine(config: BurstLossConfig | null) {
  let inBadState = false

  function sample(): boolean {
    if (!config) return false
    // State transition
    if (inBadState) {
      if (Math.random() < config.p_bad_to_good) inBadState = false
    } else {
      if (Math.random() < config.p_good_to_bad) inBadState = true
    }
    // Drop decision
    const lossRate = inBadState ? config.loss_bad : config.loss_good
    return Math.random() < lossRate
  }

  return { sample, isBad: () => inBadState }
}

// ── Main ──────────────────────────────────────────────────────

export function createNetworkProxy(targetPort: number): NetworkProxy {
  let conditions: NetworkConditions = {}
  let server: net.Server | null = null
  const activeSockets = new Set<net.Socket>()
  let blackholeTimer: NodeJS.Timeout | null = null

  /**
   * Pipe data from source to dest.
   * @param direction 'upload' (client→server) or 'download' (server→client)
   */
  function createThrottledPipe(
    source: net.Socket,
    dest: net.Socket,
    direction: 'upload' | 'download',
  ): void {
    let buffer: Buffer[] = []
    let draining = false
    let bytesInWindow = 0
    const windowMs = 100

    // Per-direction burst loss engine
    const burstEngine = createBurstLossEngine(null)
    // Will be re-initialized lazily — but for simplicity, we'll sample from conditions each time

    // Bandwidth fluctuation
    let currentBw = 0
    let bwTimer: NodeJS.Timeout | null = null

    const updateBw = () => {
      const { bandwidth, bandwidthFluctuation } = resolveDir(conditions, direction)
      if (bandwidthFluctuation > 0) {
        const r = (Math.random() * 2 - 1) * bandwidthFluctuation
        currentBw = Math.max(bandwidth * 0.1, bandwidth * (1 + r))
      } else {
        currentBw = bandwidth
      }
    }

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
    if (resolveDir(conditions, direction).bandwidthFluctuation > 0) {
      updateBw()
      bwTimer = setInterval(updateBw, 1000 + Math.random() * 2000)
    } else {
      currentBw = resolveDir(conditions, direction).bandwidth
    }

    // Build per-direction burst engine (regenerated on setConditions)
    let burstEngineLocal = createBurstLossEngine(null)

    source.on('data', async (chunk: Buffer) => {
      // Refresh burst engine from current conditions
      const bcfg = resolveDir(conditions, direction).burstLoss
      if (bcfg !== burstEngineLocal['config']) {
        burstEngineLocal = createBurstLossEngine(bcfg)
        ;(burstEngineLocal as any)['config'] = bcfg
      }

      const dirConds = resolveDir(conditions, direction)

      // ── Blackhole check ──
      if (conditions.blackhole?.enabled) return

      // ── Packet loss (independent random) ──
      if (Math.random() < dirConds.packetLoss) return

      // ── Burst loss (Gilbert-Elliott) ──
      if (burstEngineLocal.sample()) return

      // ── Corrupt ──
      if (dirConds.corrupt > 0 && Math.random() < dirConds.corrupt) {
        if (chunk.length > 0) {
          chunk = Buffer.from(chunk)
          chunk[Math.floor(Math.random() * chunk.length)] ^= 0xFF
        }
      }

      // ── Reorder (delay this chunk) ──
      if (dirConds.reorderProb > 0 && Math.random() < dirConds.reorderProb) {
        delay(dirConds.reorderDelayMs).then(() => {
          if (!dest.destroyed) dest.write(chunk)
        })
        return
      }

      // ── Duplicate ──
      if (dirConds.duplicate > 0 && Math.random() < dirConds.duplicate) {
        if (!dest.write(chunk)) draining = true
      }

      // ── Latency + Jitter ──
      const lat = actualDelay(dirConds.latency, dirConds.jitter, dirConds.jitterDistribution)
      if (lat > 0) {
        await delay(lat)
      }

      // ── Bandwidth limit ──
      const bw = currentBw
      if (bw > 0) {
        const maxPerWindow = Math.max(1, bw / (1000 / windowMs))
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
      if (bwTimer) clearInterval(bwTimer)
      try { dest.end() } catch {}
    })

    source.on('error', () => {
      clearInterval(windowTimer)
      if (bwTimer) clearInterval(bwTimer)
      try { dest.destroy() } catch {}
    })

    dest.on('drain', () => {
      draining = false
    })
  }

  // ── Proxy API ──────────────────────────────────────────────

  const proxy: NetworkProxy = {
    port: 0,

    setConditions(c: NetworkConditions) {
      conditions = { ...c }

      // Handle blackhole lifecycle
      if (conditions.blackhole?.enabled) {
        const bh = conditions.blackhole
        // Schedule start
        if (bh.startAfter && bh.startAfter > 0) {
          setTimeout(() => {
            if (conditions.blackhole === bh) {
              // still same config
            }
          }, bh.startAfter)
        }
        // Schedule auto-off
        if (bh.duration && bh.duration > 0) {
          if (blackholeTimer) clearTimeout(blackholeTimer)
          blackholeTimer = setTimeout(() => {
            if (conditions.blackhole === bh) {
              const updated = { ...conditions, blackhole: { ...bh, enabled: false } }
              conditions = updated
              if (bh.destroyOnRecover) {
                for (const sock of activeSockets) {
                  try { sock.destroy() } catch {}
                }
              }
            }
          }, (bh.startAfter ?? 0) + bh.duration)
        }
      }
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

            // Upload: client → target
            createThrottledPipe(clientSocket, targetSocket, 'upload')
            // Download: target → client
            createThrottledPipe(targetSocket, clientSocket, 'download')

            // Random connection reset
            const resetProb = resolveDir(conditions, 'upload').connectionReset
            if (Math.random() < resetProb) {
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
        if (blackholeTimer) clearTimeout(blackholeTimer)
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
