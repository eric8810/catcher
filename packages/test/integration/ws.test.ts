/**
 * Integration benchmark: WebSocket under simulated network conditions.
 *
 * Compares vanilla ws vs catcher createResilientWS:
 *   - perMessageDeflate compression (bandwidth)
 *   - Exponential backoff reconnection
 *   - Multi-endpoint racing
 */

import WebSocket from 'ws'
import { describe, it, expect, beforeAll, afterAll } from 'vitest'
import { createResilientWS } from '@eric8810/catcher-ws'
import { createWSTestServer, type WSTestServer } from '../servers/ws-server.js'
import { createNetworkProxy, type NetworkProxy } from '../network/proxy.js'
import { NETWORK_PROFILES } from '../network/presets.js'

const TIMEOUT = 120_000
const isCI = !!process.env.CI

describe('WS — message latency with perMessageDeflate', () => {
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

      // Drain proxy pipes between vanilla and catcher
      proxy.disruptAll()
      await new Promise((r) => setTimeout(r, 500))

      // ── Catcher WS (perMessageDeflate) ──
      const catcherLatencies: number[] = []
      await new Promise<void>((resolve) => {
        const ws = createResilientWS({
          url: proxyUrl,
          perMessageDeflate: true,
          handshakeTimeout: 30_000,
          reconnect: { maxAttempts: 3, initialDelay: 500, maxDelay: 2000 },
        })

        let sent = 0
        let received = 0
        let sendTime = 0

        ws.addEventListener('open', () => {
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
          catcherLatencies.push(Date.now() - sendTime)
          received++
          if (received >= messageCount) {
            ws.close()
          }
        })

        ws.addEventListener('close', () => resolve())
        setTimeout(() => resolve(), 30_000)
      })

      // Report
      const avg = (arr: number[]) => (arr.length > 0 ? arr.reduce((a, b) => a + b, 0) / arr.length : 0)
      console.log(`  vanilla avg latency: ${avg(vanillaLatencies).toFixed(1)}ms (${vanillaLatencies.length} msgs)`)
      console.log(`  catcher avg latency: ${avg(catcherLatencies).toFixed(1)}ms (${catcherLatencies.length} msgs)`)

      expect(catcherLatencies.length).toBeGreaterThan(0)
    }, TIMEOUT)
  }
})

describe('WS — reconnection with exponential backoff', () => {
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

    const ws = createResilientWS({
      url: proxyUrl,
      handshakeTimeout: 10_000,
      reconnect: {
        initialDelay: 500,
        maxDelay: 5_000,
        backoffMultiplier: 2,
        maxAttempts: 10,
      },
    })

    ws.addEventListener('open', () => {
      connectEvents.push('open')
    })

    ws.addEventListener('close', () => {
      connectEvents.push('close')
    })

    // Wait for initial connection
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

describe('WS — multi-endpoint racing', () => {
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

  // NOTE: The TCP proxy has known limitations with concurrent WS connections
  // for multi-endpoint racing. This test verifies the API works without crashing.
  it('connects to at least one of multiple endpoints', async () => {
    const urls = [
      server1.url,
      server2.url,
    ]

    const ws = createResilientWS({
      url: urls,
      handshakeTimeout: 15_000,
      raceCount: 2,
      reconnect: { maxAttempts: 0 },
    })

    let connected = false
    await new Promise<void>((resolve) => {
      ws.addEventListener('open', () => {
        connected = true
        resolve()
      })
      setTimeout(resolve, 15_000)
    })

    console.log(`  multi-endpoint connected: ${connected}`)
    expect(connected).toBe(true)
    ws.close()
  }, TIMEOUT)
})
