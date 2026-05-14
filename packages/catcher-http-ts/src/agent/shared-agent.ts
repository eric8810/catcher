import https from 'node:https'
import type { Socket } from 'node:net'
import CacheableLookup from 'cacheable-lookup'
import type { SharedAgentOptions } from '@eric8810/catcher-core'

/**
 * Create a CacheableLookup instance for a specific agent.
 *
 * G7 fix: Each agent gets its own CacheableLookup instance so that
 * different clients with different hostMapping don't interfere.
 */
function createDnsLookup(ttl: number, hostMapping?: Record<string, string>): CacheableLookup {
  const cache = new CacheableLookup({ maxTtl: ttl })

  // G7: Inject custom host mapping into DNS lookup
  if (hostMapping && Object.keys(hostMapping).length > 0) {
    const originalLookup = cache.lookup.bind(cache)
    ;(cache as any).lookup = (
      hostname: string,
      options: any,
      callback: any,
    ) => {
      if (hostMapping[hostname]) {
        // Return mapped IP directly
        if (typeof options === 'function') {
          callback = options
          options = {}
        }
        const ip = hostMapping[hostname]
        if (callback) {
          callback(null, ip, 4)
          return
        }
      }
      return originalLookup(hostname, options, callback)
    }
  }

  return cache
}

/**
 * Create a shared https.Agent with keep-alive, DNS caching, and sensible defaults.
 * Reuse this agent across all HTTP clients to avoid repeated TCP+TLS handshakes.
 *
 * Health checks:
 * - freeSocketTimeout auto-evicts idle sockets (prevents stale connection reuse)
 * - socket error handler marks bad sockets so Node can remove them from the pool
 *
 * Note: Each call creates a new Agent with its own DNS cache to ensure
 * hostMapping isolation between different client instances.
 */
export function createSharedAgent(options: SharedAgentOptions & { hostMapping?: Record<string, string> } = {}): https.Agent {
  const {
    keepAlive = true,
    keepAliveMsecs = 30_000,
    maxSockets = 25,
    maxFreeSockets = 10,
    timeout = 60_000,
    rejectUnauthorized = true,
    dnsCacheTtl = 300,
    hostMapping,
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
    // Each agent gets its own DNS cache to ensure hostMapping isolation
    ;(agentOpts as any).lookup = createDnsLookup(dnsCacheTtl, hostMapping).lookup
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

/**
 * Reset all DNS caches.
 * Since each agent now has its own cache, this is a no-op kept for API compat.
 */
export function clearDnsCache(): void {
  // No-op: DNS caches are now per-agent, not global.
  // Individual agents will naturally GC when dereferenced.
}
