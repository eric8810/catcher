import https from 'node:https'
import type { Socket } from 'node:net'
import CacheableLookup from 'cacheable-lookup'
import type { SharedAgentOptions } from '@catcher/core'

let _defaultDnsCache: CacheableLookup | null = null

function getDnsCache(ttl: number): CacheableLookup {
  if (!_defaultDnsCache) {
    _defaultDnsCache = new CacheableLookup({ maxTtl: ttl })
  }
  return _defaultDnsCache
}

/**
 * Create a shared https.Agent with keep-alive, DNS caching, and sensible defaults.
 * Reuse this agent across all HTTP clients to avoid repeated TCP+TLS handshakes.
 *
 * Health checks:
 * - freeSocketTimeout auto-evicts idle sockets (prevents stale connection reuse)
 * - socket error handler marks bad sockets so Node can remove them from the pool
 */
export function createSharedAgent(options: SharedAgentOptions = {}): https.Agent {
  const {
    keepAlive = true,
    keepAliveMsecs = 30_000,
    maxSockets = 25,
    maxFreeSockets = 10,
    timeout = 60_000,
    rejectUnauthorized = true,
    dnsCacheTtl = 300,
  } = options

  // freeSocketTimeout: destroy unused free sockets after this many ms.
  // Shorter TTL means stale/broken connections are evicted sooner.
  // Default: keepAliveMsecs + 5s grace period.
  const freeSocketTimeout = keepAlive ? keepAliveMsecs + 5_000 : 0

  const agentOpts: https.AgentOptions = {
    keepAlive,
    keepAliveMsecs,
    maxSockets,
    maxFreeSockets,
    timeout,
    rejectUnauthorized,
    // Schedule requests FIFO to avoid hoarding connections
    scheduling: 'fifo' as const,
  }

  if (dnsCacheTtl > 0) {
    ;(agentOpts as any).lookup = getDnsCache(dnsCacheTtl).lookup
  }

  const agent = new https.Agent(agentOpts)

  // Set freeSocketTimeout — Node destroys idle free sockets older than this
  if (keepAlive && freeSocketTimeout > 0) {
    ;(agent as any).freeSocketTimeout = freeSocketTimeout
  }

  // Auto-evict bad sockets from the keepAlive pool.
  // When a socket errors, Node normally removes it from freeSockets but
  // may keep it in CLOSE_WAIT. Explicitly destroy on error + close.
  agent.on('free', (_req: any, socket: Socket) => {
    const onError = () => {
      socket.destroy()
    }
    // If socket already has an error, destroy immediately
    if (socket.destroyed) return
    socket.once('error', onError)
    // Cleanup listener after close to avoid leaks
    socket.once('close', () => {
      socket.removeListener('error', onError)
    })
  })

  return agent
}

/** Reset the global DNS cache (useful for testing or network changes) */
export function clearDnsCache(): void {
  _defaultDnsCache = null
}
