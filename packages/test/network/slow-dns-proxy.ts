/**
 * Slow DNS proxy — forwards DNS queries to a real resolver
 * with an artificial delay (default 200ms) per query.
 *
 * Used by S8 (DNS cache test) to make DNS resolution expensive,
 * so that caching provides a measurable benefit.
 *
 * Usage:
 *   const proxy = createSlowDnsProxy(200)  // 200ms delay
 *   proxy.start()  // returns { port } once listening
 *   // Configure catcher: dns: { nameservers: [`127.0.0.1:${port}`], cache_ttl_secs: 300 }
 *   proxy.stop()
 */
import dgram from 'node:dgram'
import dns from 'node:dns/promises'

export interface SlowDnsProxy {
  readonly port: number
  start(): Promise<void>
  stop(): Promise<void>
}

export function createSlowDnsProxy(delayMs: number = 200): SlowDnsProxy {
  const server = dgram.createSocket('udp4')
  let _port = 0

  server.on('message', async (msg: Buffer, rinfo: dgram.RemoteInfo) => {
    // Artificial delay before resolving
    await new Promise((r) => setTimeout(r, delayMs))

    try {
      // Parse the DNS query to extract the domain name
      const domain = parseDnsQueryName(msg)
      if (!domain) {
        // Can't parse — skip
        return
      }

      // Resolve using system DNS (native, not the proxy itself)
      const addresses = await dns.resolve4(domain).catch(() => [])

      if (addresses.length === 0) return

      // Build a DNS response
      const response = buildDnsResponse(msg, addresses)
      server.send(response, rinfo.port, rinfo.address)
    } catch {
      // Silently ignore resolution failures
    }
  })

  return {
    get port() { return _port },
    start(): Promise<void> {
      return new Promise((resolve, reject) => {
        server.bind(0, '127.0.0.1', () => {
          const addr = server.address()
          if (typeof addr === 'object') _port = addr.port
          resolve()
        })
        server.on('error', reject)
      })
    },
    stop(): Promise<void> {
      return new Promise((resolve) => {
        server.close(() => resolve())
      })
    },
  }
}

/**
 * Parse the QNAME from a raw DNS query packet.
 * DNS header is 12 bytes, then the question section starts.
 */
function parseDnsQueryName(msg: Buffer): string | null {
  if (msg.length < 12) return null
  let offset = 12
  const labels: string[] = []
  while (offset < msg.length) {
    const len = msg[offset]
    if (len === 0) break
    offset++
    if (offset + len > msg.length) return null
    labels.push(msg.subarray(offset, offset + len).toString('ascii'))
    offset += len
  }
  return labels.length > 0 ? labels.join('.') : null
}

/**
 * Build a DNS response for the given query + resolved addresses.
 */
function buildDnsResponse(query: Buffer, addresses: string[]): Buffer {
  // Copy the query as the basis for the response
  const header = Buffer.from(query.subarray(0, 12))

  // Set QR bit (response) = 1, RCODE = 0 (no error)
  header[2] = (header[2] & 0x7f) | 0x80 // QR=1
  // Set ANCOUNT = number of answers (same byte offset as QDCOUNT in query)
  header[6] = 0
  header[7] = addresses.length

  // Find the end of the question section
  let qEnd = 12
  while (qEnd < query.length) {
    const len = query[qEnd]
    if (len === 0) { qEnd++; break }
    qEnd += len + 1
  }
  // Skip QTYPE (2 bytes) + QCLASS (2 bytes)
  qEnd += 4

  // Build answer records
  const answers: Buffer[] = []
  for (const addr of addresses) {
    // Name pointer to the question name (offset 12)
    const namePtr = Buffer.from([0xc0, 0x0c])
    // TYPE=A (1), CLASS=IN (1), TTL=60s, RDLENGTH=4
    const rrMeta = Buffer.alloc(10)
    rrMeta.writeUInt16BE(1, 0)   // TYPE = A
    rrMeta.writeUInt16BE(1, 2)   // CLASS = IN
    rrMeta.writeUInt32BE(60, 4)  // TTL = 60s
    rrMeta.writeUInt16BE(4, 8)   // RDLENGTH = 4
    const ipParts = addr.split('.').map(Number)
    const ipBuf = Buffer.from(ipParts)
    answers.push(Buffer.concat([namePtr, rrMeta, ipBuf]))
  }

  return Buffer.concat([
    header,
    query.subarray(12, qEnd),  // question section
    ...answers,
  ])
}
