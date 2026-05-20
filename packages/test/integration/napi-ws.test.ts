/**
 * Integration benchmark: WebSocket under simulated network conditions — NAPI version.
 *
 * Mirrors ws.test.ts but swaps TS createResilientWS for the Rust NAPI
 * createRustWsClient adapter. Vanilla ws remains as the comparison baseline.
 *
 * Tests:
 *   - perMessageDeflate compression (bandwidth)
 *   - Exponential backoff reconnection
 *   - Single-endpoint connection (multi-endpoint racing deferred to rust-vs-vanilla.test.ts)
 */

import WebSocket from 'ws'
import { describe, it, expect, beforeAll, afterAll } from 'vitest'
import { createRustWsClient } from '../adapters/rust-adapter.js'
import { createWSTestServer, type WSTestServer } from '../servers/ws-server.js'
import { createNetworkProxy, type NetworkProxy } from '../network/proxy.js'
import { NETWORK_PROFILES } from '../network/presets.js'

const TIMEOUT = 120_000
const isCI = !!process.env.CI

describe('NAPI WS — message latency with perMessageDeflate', () => {
  let server: WSTestServer
  let proxy: NetworkProxy
  let proxyUrl: string

  beforeAll(async () => {
    server = await createWSTestServer()
    proxy = createNetworkProxy(server.port)
    await proxy.start()
    proxyUrl = `ws://127.0.0.1:${proxy.port}`
  }, 30000)

  afterAll(async () => {
    await proxy.stop()
    await server.close()
  })

  for (const [key, profile] of Object.entries(NETWORK_PROFILES)) {
    if (!['good', 'weak'].includes(key)) continue

    // Skip weak network tests in CI — flaky due to unstable network simulation
    const skip = isCI && key !== 'good'

    it.skipIf(skip)(`${profile.emoji} ${profile.name} — 50 messages round-trip`, async () => {
      proxy.setConditions(profile.conditions)
      proxy.disruptAll()

      // Wait for proxy pipes to fully drain before creating new connections
      await new Promise((r) => setTimeout(r, 500))

      const messageCount = 50
      const payload = JSON.stringify({
        event: 'message',
        id: 'test',
        text: 'Hello '.repeat(30), // ~200 bytes
        ts: Date.now(),
      })

      // ── Vanilla WS (no compression) ──
      const vanillaLatencies: number[] = []
      await new Promise<void>((resolve, reject) => {
        const ws = new WebSocket(proxyUrl)
        let sent = 0
        let received = 0
        let sendTime = 0
        let settled = false

        const done = () => {
          if (settled) return
          settled = true
          resolve()
        }

        ws.on('open', () => {
          const interval = setInterval(() => {
            if (sent >= messageCount) {
              clearInterval(interval)
              return
            }
            sendTime = Date.now()
            ws.send(payload)
            sent++
          }, 50)
        })

        ws.on('message', () => {
          vanillaLatencies.push(Date.now() - sendTime)
          received++
          if (received >= messageCount) {
            ws.close()
          }
        })

        ws.on('close', () => done())
        ws.on('error', (err) => {
          // Weak network errors are expected — don't fail the whole test
          console.warn(`  vanilla ws error (expected in weak network): ${err.message}`)
          done()
        })
        setTimeout(() => done(), 30_000) // timeout safety
      })

      // Drain proxy pipes between vanilla and NAPI catcher
      proxy.disruptAll()
      await new Promise((r) => setTimeout(r, 500))

      // ── NAPI Catcher WS (perMessageDeflate) ──
      const napiLatencies: number[] = []
      await new Promise<void>((resolve) => {
        const ws = createRustWsClient({
          url: proxyUrl,
          perMessageDeflate: true,
          handshakeTimeout: 30_000,
          reconnect: { maxAttempts: 3 },
        })

        let sent = 0
        let received = 0
        let sendTime = 0
        let settled = false

        const done = () => {
          if (settled) return
          settled = true
          resolve()
        }

        ws.ready.then(() => {
          const interval = setInterval(() => {
            if (sent >= messageCount) {
              clearInterval(interval)
              return
            }
            sendTime = Date.now()
            ws.send(payload)
            sent++
          }, 50)
        })

        ws.addEventListener('message', () => {
          napiLatencies.push(Date.now() - sendTime)
          received++
          if (received >= messageCount) {
            ws.close()
          }
        })

        ws.addEventListener('close', () => done())
        ws.addEventListener('error', () => done())
        setTimeout(() => done(), 30_000) // timeout safety
      })

      // Report
      const avg = (arr: number[]) => (arr.length > 0 ? arr.reduce((a, b) => a + b, 0) / arr.length : 0)
      console.log(`  vanilla avg latency: ${avg(vanillaLatencies).toFixed(1)}ms (${vanillaLatencies.length} msgs)`)
      console.log(`  napi    avg latency: ${avg(napiLatencies).toFixed(1)}ms (${napiLatencies.length} msgs)`)

      expect(napiLatencies.length).toBeGreaterThan(0)
    }, TIMEOUT)
  }
})

describe('NAPI WS — reconnection with exponential backoff', () => {
  let server: WSTestServer
  let proxy: NetworkProxy

  beforeAll(async () => {
    server = await createWSTestServer()
    proxy = createNetworkProxy(server.port)
    await proxy.start()
  }, 30000)

  afterAll(async () => {
    await proxy.stop()
    await server.close()
  })

  // Skip in CI — reconnection test depends on proxy timing, flaky on CI runners
  it.skipIf(isCI)('reconnects after connection disruption', async () => {
    const proxyUrl = `ws://127.0.0.1:${proxy.port}`

    const connectEvents: string[] = []

    const ws = createRustWsClient({
      url: proxyUrl,
      handshakeTimeout: 10_000,
      reconnect: { maxAttempts: 10 },
    })

    ws.addEventListener('open', () => {
      connectEvents.push('open')
    })

    ws.addEventListener('close', () => {
      connectEvents.push('close')
    })

    // Wait for initial connection via ready promise
    await ws.ready

    // Extra settle time to ensure event listeners have fired
    await new Promise((r) => setTimeout(r, 2000))

    // Disrupt all connections
    proxy.disruptAll()

    // Wait for reconnect
    await new Promise((r) => setTimeout(r, 10_000))

    ws.close()

    console.log(`  connection events: ${connectEvents.join(' → ')}`)
    // Should have at least: open → close → open (reconnected)
    expect(connectEvents.filter((e) => e === 'open').length).toBeGreaterThanOrEqual(2)
  }, TIMEOUT)
})

describe('NAPI WS — single-endpoint connection', () => {
  let server1: WSTestServer
  let server2: WSTestServer

  beforeAll(async () => {
    server1 = await createWSTestServer()
    server2 = await createWSTestServer()
  }, 30000)

  afterAll(async () => {
    await server1.close()
    await server2.close()
  })

  // NOTE: createRustWsClient only accepts a single url string.
  // Multi-endpoint racing is tested in rust-vs-vanilla.test.ts via raw WsClient.
  // Here we verify basic single-endpoint connectivity against one of the servers.
  it('connects to a server endpoint', async () => {
    const ws = createRustWsClient({
      url: server1.url,
      handshakeTimeout: 15_000,
      reconnect: { maxAttempts: 0 },
    })

    let connected = false
    ws.addEventListener('open', () => {
      connected = true
    })

    await ws.ready
    // Give the open event listener time to fire
    await new Promise((r) => setTimeout(r, 200))

    console.log(`  single-endpoint connected: ${connected}`)
    expect(connected).toBe(true)
    ws.close()
  }, TIMEOUT)
})
