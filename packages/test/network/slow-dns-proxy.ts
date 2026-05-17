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
 * Create a Node.js `lookup` function that resolves hostnames
 * via the slow DNS proxy (UDP). Each call sends a real DNS query
 * through the proxy, incurring the proxy's configured delay.
 *
 * Usage:
 *   const lookup = createDnsLookupViaProxy(proxyPort)
 *   const agent = new http.Agent({ lookup })
 *   axios.get('http://example.com', { httpAgent: agent })
 */
export function createDnsLookupViaProxy(proxyPort: number): (hostname: string, options: dns.LookupOptions, callback: (err: NodeJS.ErrnoException | null, address: string, family: number) => void) => void {
  return (hostname, options, callback) => {
    dnsLookupViaProxy(proxyPort, hostname)
      .then(({ address, family }) => callback(null, address, family))
      .catch((err) => callback(err as NodeJS.ErrnoException, '', 4))
  }
}

async function dnsLookupViaProxy(proxyPort: number, hostname: string): Promise<{ address: string; family: number }> {
  const query = buildDnsQuery(hostname)
  const socket = dgram.createSocket('udp4')

  try {
    const response = await new Promise<Buffer>((resolve, reject) => {
      const timeout = setTimeout(() => {
        reject(new Error(`DNS lookup timeout for ${hostname}`))
        socket.close()
      }, 5000)

      socket.on('message', (msg: Buffer) => {
        clearTimeout(timeout)
        resolve(msg)
      })

      socket.on('error', (err) => {
        clearTimeout(timeout)
        reject(err)
      })

      socket.send(query, proxyPort, '127.0.0.1', (err) => {
        if (err) {
          clearTimeout(timeout)
          reject(err)
        }
      })
    })

    // Parse the first A record from the answer section
    const ip = parseDnsResponseA(response)
    if (!ip) throw new Error(`No A record in DNS response for ${hostname}`)
    return { address: ip, family: 4 }
  } finally {
    try { socket.close() } catch { /* ignore */ }
  }
}

/**
 * Build a minimal DNS query packet for an A record lookup.
 */
function buildDnsQuery(hostname: string): Buffer {
  // Header: ID(2) + Flags(2) + QDCOUNT(2) + ANCOUNT(2) + NSCOUNT(2) + ARCOUNT(2)
  const header = Buffer.alloc(12)
  header.writeUInt16BE(0x1234, 0)   // Transaction ID
  header.writeUInt16BE(0x0100, 2)   // Flags: standard query, recursion desired
  header.writeUInt16BE(1, 4)        // QDCOUNT = 1
  // ANCOUNT, NSCOUNT, ARCOUNT = 0 (already zeroed)

  // Question section: QNAME + QTYPE(A=1) + QCLASS(IN=1)
  const labels = hostname.split('.')
  const qnameParts: Buffer[] = []
  for (const label of labels) {
    qnameParts.push(Buffer.from([label.length]))
    qnameParts.push(Buffer.from(label, 'ascii'))
  }
  qnameParts.push(Buffer.from([0])) // Root label

  const qtype = Buffer.alloc(4)
  qtype.writeUInt16BE(1, 0)  // TYPE = A
  qtype.writeUInt16BE(1, 2)  // CLASS = IN

  return Buffer.concat([header, ...qnameParts, qtype])
}

/**
 * Parse the first A record IP address from a DNS response.
 */
function parseDnsResponseA(msg: Buffer): string | null {
  if (msg.length < 12) return null

  const ancount = msg.readUInt16BE(6)
  if (ancount === 0) return null

  // Skip header (12 bytes) + question section
  let offset = 12
  // Skip QNAME
  while (offset < msg.length) {
    const len = msg[offset]
    if (len === 0) { offset++; break }
    offset += len + 1
  }
  offset += 4 // Skip QTYPE + QCLASS

  // Parse answer records
  for (let i = 0; i < ancount && offset < msg.length; i++) {
    // Name (could be pointer or labels)
    if ((msg[offset] & 0xc0) === 0xc0) {
      offset += 2 // Compressed name pointer
    } else {
      while (offset < msg.length) {
        const len = msg[offset]
        if (len === 0) { offset++; break }
        offset += len + 1
      }
    }

    if (offset + 10 > msg.length) return null
    const rtype = msg.readUInt16BE(offset)
    // const rclass = msg.readUInt16BE(offset + 2)
    // const ttl = msg.readUInt32BE(offset + 4)
    const rdlength = msg.readUInt16BE(offset + 8)
    offset += 10

    if (rtype === 1 && rdlength === 4 && offset + 4 <= msg.length) {
      // A record
      return `${msg[offset]}.${msg[offset + 1]}.${msg[offset + 2]}.${msg[offset + 3]}`
    }
    offset += rdlength
  }

  return null
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
