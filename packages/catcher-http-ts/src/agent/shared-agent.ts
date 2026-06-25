import http from 'node:http'
import https from 'node:https'
import { readFileSync } from 'node:fs'
import type { Socket } from 'node:net'
import CacheableLookup from 'cacheable-lookup'
import type { SharedAgentOptions, TlsConfig } from '@eric8810/catcher-core'

/**
 * Create a CacheableLookup instance for a specific agent.
 *
 * G7 fix: Each agent gets its own CacheableLookup instance so that
 * different clients with different hostMapping don't interfere.
 */
function createDnsLookup(ttl: number, hostMapping?: Record<string, string>, nameservers?: string[]): CacheableLookup {
  const cacheOpts: any = { maxTtl: ttl }
  if (nameservers && nameservers.length > 0) {
    cacheOpts.servers = nameservers
  }
  const cache = new CacheableLookup(cacheOpts)

  // G7: Inject custom host mapping into DNS lookup
  if (hostMapping && Object.keys(hostMapping).length > 0) {
    const originalLookup = cache.lookup.bind(cache)
    ;(cache as any).lookup = (
      hostname: string,
      options: any,
      callback: any,
    ) => {
      // Normalize: CacheableLookup.lookup(hostname, options, callback)
      // or CacheableLookup.lookup(hostname, callback) — handle both
      if (typeof options === 'function') {
        callback = options
        options = {}
      }

      const mappedIp = hostMapping[hostname]
      if (mappedIp) {
        // CacheableLookup returns lookup-shaped objects: { address, family }
        // When called with callback: callback(null, address, family)
        // When called without callback: returns Promise<{ address, family }>
        if (callback) {
          callback(null, mappedIp, 4)
          return
        }
        // Promise-based usage
        return Promise.resolve({ address: mappedIp, family: 4 })
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
export function createSharedAgent(options: SharedAgentOptions & { hostMapping?: Record<string, string>; tls?: TlsConfig; nameservers?: string[] } = {}): http.Agent | https.Agent {
  const {
    keepAlive = true,
    keepAliveMsecs = 30_000,
    maxSockets = 25,
    maxFreeSockets = 10,
    timeout = 60_000,
    rejectUnauthorized = true,
    dnsCacheTtl = 300,
    hostMapping,
    tls,
    nameservers,
  } = options

  // freeSocketTimeout: destroy unused free sockets after this many ms.
  // Shorter TTL means stale/broken connections are evicted sooner.
  // Default: keepAliveMsecs + 5s grace period.
  const freeSocketTimeout = keepAlive ? keepAliveMsecs + 5_000 : 0

  const agentOpts: http.AgentOptions = {
    keepAlive,
    keepAliveMsecs,
    maxSockets,
    maxFreeSockets,
    timeout,
    // Schedule requests FIFO to avoid hoarding connections
    scheduling: 'fifo' as const,
  }

  if (dnsCacheTtl > 0) {
    // Each agent gets its own DNS cache to ensure hostMapping isolation
    ;(agentOpts as any).lookup = createDnsLookup(dnsCacheTtl, hostMapping, nameservers).lookup
  }

  // Build TLS options for https.Agent
  const hasTlsConfig = tls && (
    tls.caCertPem || tls.caCertPath ||
    tls.clientCertPem || tls.clientCertPath ||
    tls.clientKeyPem || tls.clientKeyPath ||
    tls.clientIdentityPfx ||
    tls.minTlsVersion ||
    tls.tlsSniOverride ||
    tls.rejectUnauthorized === false
  )

  const tlsAgentOpts: https.AgentOptions = {
    ...agentOpts,
    rejectUnauthorized: tls?.rejectUnauthorized ?? rejectUnauthorized,
  }

  // G8: TLS — ca/cert/key from PEM content or file path
  if (tls?.caCertPem) {
    tlsAgentOpts.ca = tls.caCertPem
  } else if (tls?.caCertPath) {
    tlsAgentOpts.ca = readFileSync(tls.caCertPath, 'utf-8')
  }
  if (tls?.clientCertPem) {
    tlsAgentOpts.cert = tls.clientCertPem
  } else if (tls?.clientCertPath) {
    tlsAgentOpts.cert = readFileSync(tls.clientCertPath, 'utf-8')
  }
  if (tls?.clientKeyPem) {
    tlsAgentOpts.key = tls.clientKeyPem
  } else if (tls?.clientKeyPath) {
    tlsAgentOpts.key = readFileSync(tls.clientKeyPath, 'utf-8')
  }
  if (tls?.clientIdentityPfx) {
    tlsAgentOpts.pfx = Buffer.from(tls.clientIdentityPfx)
    if (tls.clientIdentityPassword) {
      tlsAgentOpts.passphrase = tls.clientIdentityPassword
    }
  }
  if (tls?.minTlsVersion) {
    const versionMap: Record<string, string> = {
      '1.0': 'TLSv1',
      '1.1': 'TLSv1_1',
      '1.2': 'TLSv1_2',
      '1.3': 'TLSv1_3',
    }
    ;(tlsAgentOpts as any).minVersion = versionMap[tls.minTlsVersion] ?? 'TLSv1_2'
  }
  if (tls?.tlsSniOverride) {
    tlsAgentOpts.servername = tls.tlsSniOverride
  }

  const agent = hasTlsConfig
    ? new https.Agent(tlsAgentOpts)
    : new http.Agent(agentOpts as http.AgentOptions)

  // Set freeSocketTimeout — Node destroys idle free sockets older than this
  if (keepAlive && freeSocketTimeout > 0) {
    ;(agent as any).freeSocketTimeout = freeSocketTimeout
  }

  // Auto-evict bad sockets from the keepAlive pool.
  // When a socket errors, Node normally removes it from freeSockets but
  // may keep it in CLOSE_WAIT. Explicitly destroy on error + close.
  agent.on('free', (_req: any, socket: any) => {
    // Guard: socket may not always be a proper Socket in edge cases
    if (!socket || typeof socket.once !== 'function') return
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

