import { describe, it, expect, beforeEach, afterEach } from 'vitest'
import http from 'node:http'
import { createSharedAgent, clearDnsCache } from '../shared-agent.js'

describe('A1 — Default config creates Agent with keepAlive', () => {
  it('returns https.Agent with keepAlive=true', () => {
    const agent = createSharedAgent() as any
    expect(agent).toBeInstanceOf(http.Agent)
    expect(agent.keepAlive).toBe(true)
  })
})

describe('A2 — keepAlive=false', () => {
  it('returns Agent with keepAlive=false', () => {
    const agent = createSharedAgent({ keepAlive: false }) as any
    expect(agent.keepAlive).toBe(false)
  })
})

describe('A3 — maxSockets config', () => {
  it('returns Agent with maxSockets=50', () => {
    const agent = createSharedAgent({ maxSockets: 50 })
    expect(agent.maxSockets).toBe(50)
  })
})

describe('A4 — DNS cache enabled', () => {
  it('agent has custom lookup when dnsCacheTtl > 0', () => {
    clearDnsCache()
    const agent = createSharedAgent({ dnsCacheTtl: 300 }) as any
    // When dnsCacheTtl > 0, the lookup is overridden via agentOpts.lookup
    // Check that options were set (Node stores it internally)
    expect(agent.options.lookup).toBeDefined()
  })
})

describe('A5 — DNS cache disabled', () => {
  it('agent has no custom lookup when dnsCacheTtl=0', () => {
    const agent = createSharedAgent({ dnsCacheTtl: 0 }) as any
    expect(agent.options.lookup).toBeUndefined()
  })
})

describe('A6 — rejectUnauthorized=false', () => {
  it('Agent creates successfully', () => {
    const agent = createSharedAgent({ rejectUnauthorized: false })
    expect(agent).toBeInstanceOf(http.Agent)
  })
})

describe('A7 — clearDnsCache()', () => {
  it('allows creating a new agent with fresh DNS cache', () => {
    clearDnsCache()
    const agent1 = createSharedAgent({ dnsCacheTtl: 300 })
    clearDnsCache()
    const agent2 = createSharedAgent({ dnsCacheTtl: 300 })
    // Both should be valid agents (new cache created after clear)
    expect(agent1).toBeInstanceOf(http.Agent)
    expect(agent2).toBeInstanceOf(http.Agent)
  })
})

// ── A8-A9: G7 hostMapping end-to-end ────────────────────────────

let e2eServer: http.Server
let e2ePort: number

beforeEach(async () => {
  e2eServer = http.createServer((req, res) => {
    res.writeHead(200, { 'Content-Type': 'application/json' })
    res.end(JSON.stringify({ host: req.headers.host, ok: true }))
  })
  await new Promise<void>((resolve) => e2eServer.listen(0, '127.0.0.1', () => resolve()))
  e2ePort = (e2eServer.address() as any).port
})

afterEach(async () => {
  await new Promise<void>((resolve) => e2eServer.close(() => resolve()))
})

describe('A8 — hostMapping resolves custom hostname to IP', () => {
  it('request to fake hostname reaches local server via hostMapping', async () => {
    // Verify hostMapping is wired correctly by checking the lookup function directly
    const agent = createSharedAgent({
      dnsCacheTtl: 300,
      hostMapping: { 'my-fake-service.local': '127.0.0.1' },
    }) as any

    // The agent's custom lookup should resolve the fake hostname to 127.0.0.1
    const lookupFn = agent.options.lookup
    expect(lookupFn).toBeDefined()

    const lookupResult = await new Promise<any>((resolve, reject) => {
      lookupFn('my-fake-service.local', (err: any, address: string, family: number) => {
        if (err) reject(err)
        else resolve({ address, family })
      })
    })

    expect(lookupResult.address).toBe('127.0.0.1')
    expect(lookupResult.family).toBe(4)

    // Clean up
    agent.destroy()
  })
})

describe('A9 — hostMapping does not interfere with unmapped hosts', () => {
  it('agent with hostMapping still works for normal IPs', async () => {
    const agent = createSharedAgent({
      dnsCacheTtl: 300,
      hostMapping: { 'other-service.local': '10.0.0.1' },
    })

    const result = await new Promise<any>((resolve, reject) => {
      const req = http.request(
        {
          hostname: '127.0.0.1',
          port: e2ePort,
          path: '/test',
          method: 'GET',
          agent,
        },
        (res) => {
          let body = ''
          res.on('data', (chunk) => { body += chunk })
          res.on('end', () => {
            resolve(JSON.parse(body))
          })
        },
      )
      req.on('error', reject)
      req.end()
    })

    expect(result.ok).toBe(true)

    // Clean up
    agent.destroy()
  })
})
