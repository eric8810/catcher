import { describe, it, expect } from 'vitest'
import https from 'node:https'
import { createSharedAgent, clearDnsCache } from '../shared-agent.js'

describe('A1 — Default config creates Agent with keepAlive', () => {
  it('returns https.Agent with keepAlive=true', () => {
    const agent = createSharedAgent() as any
    expect(agent).toBeInstanceOf(https.Agent)
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
    expect(agent).toBeInstanceOf(https.Agent)
  })
})

describe('A7 — clearDnsCache()', () => {
  it('allows creating a new agent with fresh DNS cache', () => {
    clearDnsCache()
    const agent1 = createSharedAgent({ dnsCacheTtl: 300 })
    clearDnsCache()
    const agent2 = createSharedAgent({ dnsCacheTtl: 300 })
    // Both should be valid agents (new cache created after clear)
    expect(agent1).toBeInstanceOf(https.Agent)
    expect(agent2).toBeInstanceOf(https.Agent)
  })
})
