/**
 * NAPI Chaos test — long-running resilience validation using Rust NAPI bindings.
 *
 * Mirror of chaos.test.ts but exercising catcher-napi-http / catcher-napi-ws
 * through the rust-adapter layer instead of the pure-TS catcher packages.
 *
 * Default: 10 minutes of continuous operation under randomly changing
 * network conditions. Measures:
 *   - Message send success rate
 *   - WS reconnection reliability
 *   - Recovery time after disruption
 *
 * Usage:
 *   CHAOS_DURATION_MS=60000 npx vitest run test/chaos/napi-chaos
 */

import { describe, it, expect, beforeAll, afterAll } from 'vitest'
import fs from 'node:fs/promises'
import path from 'node:path'
import { createRustHttpClient, createRustWsClient } from '../adapters/rust-adapter.js'
import { createHttpTestServer, type TestServer } from '../servers/http-server.js'
import { createWSTestServer, type WSTestServer } from '../servers/ws-server.js'
import { createNetworkProxy, type NetworkProxy, type NetworkConditions } from '../network/proxy.js'

// ── Chaos configuration ─────────────────────────────────────

const CHAOS_DURATION_MS = parseInt(process.env.CHAOS_DURATION_MS ?? '60000', 10) // 1 min default, override with env
const SEND_INTERVAL_MS = 500  // send a message every 500ms
const CONDITION_SWITCH_MS = 30_000 // switch network conditions every 30s

interface ChaosResult {
  durationMs: number
  totalSends: number
  successfulSends: number
  failedSends: number
  successRate: number
  wsReconnects: number
  wsDisconnects: number
  totalBytesReceived: number
  conditionsApplied: string[]
  timeline: Array<{ ts: number; event: string; detail?: string }>
}

// ── Random network conditions ───────────────────────────────

function randomCondition(): { name: string; conditions: NetworkConditions } {
  const profiles = [
    { name: 'good', latency: 25, packetLoss: 0, bandwidth: 0, connectionReset: 0 },
    { name: 'weak', latency: 500, packetLoss: 0.05, bandwidth: 50_000, connectionReset: 0.02 },
    { name: 'very-weak', latency: 1500, packetLoss: 0.1, bandwidth: 10_000, connectionReset: 0.05 },
    { name: 'spike', latency: 3000, packetLoss: 0.3, bandwidth: 5_000, connectionReset: 0.1 },
    { name: 'packet-storm', latency: 200, packetLoss: 0.5, bandwidth: 0, connectionReset: 0.2 },
    { name: 'normal', latency: 100, packetLoss: 0.01, bandwidth: 0, connectionReset: 0 },
  ]

  const profile = profiles[Math.floor(Math.random() * profiles.length)]
  return {
    name: profile.name,
    conditions: {
      latency: profile.latency + Math.random() * 200 - 100,
      packetLoss: Math.max(0, profile.packetLoss + (Math.random() - 0.5) * 0.1),
      bandwidth: profile.bandwidth === 0 ? 0 : profile.bandwidth + Math.random() * 20_000,
      connectionReset: Math.max(0, profile.connectionReset + (Math.random() - 0.5) * 0.05),
    },
  }
}

// ── Test ────────────────────────────────────────────────────

describe('NAPI Chaos — 韧性压力测试 (Rust)', () => {
  let httpServer: TestServer
  let wsServer: WSTestServer
  let httpProxy: NetworkProxy
  let wsProxy: NetworkProxy
  let proxyUrl: string
  let wsProxyUrl: string

  beforeAll(async () => {
    httpServer = await createHttpTestServer()
    wsServer = await createWSTestServer()
    httpProxy = createNetworkProxy(httpServer.port)
    wsProxy = createNetworkProxy(wsServer.port)
    await httpProxy.start()
    await wsProxy.start()
    proxyUrl = `http://127.0.0.1:${httpProxy.port}`
    wsProxyUrl = `ws://127.0.0.1:${wsProxy.port}`
  }, 30000)

  afterAll(async () => {
    await httpProxy.stop()
    await wsProxy.stop()
    await httpServer.close()
    await wsServer.close()
  })

  it(`napi chaos run — ${(CHAOS_DURATION_MS / 1000).toFixed(0)}s`, async () => {
    const result: ChaosResult = {
      durationMs: CHAOS_DURATION_MS,
      totalSends: 0,
      successfulSends: 0,
      failedSends: 0,
      successRate: 0,
      wsReconnects: 0,
      wsDisconnects: 0,
      totalBytesReceived: 0,
      conditionsApplied: [],
      timeline: [],
    }

    function log(event: string, detail?: string) {
      result.timeline.push({ ts: Date.now(), event, detail })
      if (detail) {
        console.log(`  [${new Date().toISOString()}] ${event}: ${detail}`)
      }
    }

    // Create Rust NAPI HTTP client
    const httpClient = createRustHttpClient({
      baseURL: proxyUrl,
      keepAlive: true,
      dnsCacheTtl: 300,
      retry: { attempts: 3, backoff: 'exponential' },
      timeout: { response: 30000 },
      concurrency: 10,
    })

    // Create Rust NAPI WS client
    const ws = createRustWsClient({
      url: wsProxyUrl,
      perMessageDeflate: true,
      handshakeTimeout: 15000,
      reconnect: { maxAttempts: 100 },
    })

    let wsConnected = false
    let hasConnectedOnce = false
    let wsMsgsSent = 0
    let wsMsgsReceived = 0

    ws.addEventListener('open', () => {
      if (hasConnectedOnce) {
        result.wsReconnects++
        log('ws-reconnect', `reconnect #${result.wsReconnects}`)
      } else {
        log('ws-open')
        hasConnectedOnce = true
      }
      wsConnected = true
    })
    ws.addEventListener('close', () => {
      wsConnected = false
      result.wsDisconnects++
      log('ws-close', `disconnect #${result.wsDisconnects}`)
    })
    ws.addEventListener('message', (msg: any) => {
      // msg.data is base64-encoded (from Rust WsEvent::to_ffi_json data_base64)
      // Decode and only count messages that contain our "chaos" marker (not server heartbeats)
      try {
        const decoded = Buffer.from(msg.data ?? '', 'base64').toString('utf-8')
        if (decoded.includes('chaos')) {
          wsMsgsReceived++
          result.totalBytesReceived += decoded.length
        }
      } catch { /* ignore decode errors */ }
    })
    ws.addEventListener('error', () => {
      log('ws-error')
    })

    // Periodic condition switcher
    const conditionTimer = setInterval(() => {
      const { name, conditions } = randomCondition()
      httpProxy.setConditions(conditions)
      wsProxy.setConditions(conditions)
      result.conditionsApplied.push(name)
      httpProxy.disruptAll()
      wsProxy.disruptAll()
      log('condition-switch', `${name} (latency=${conditions.latency}ms, loss=${((conditions.packetLoss ?? 0) * 100).toFixed(0)}%)`)
    }, CONDITION_SWITCH_MS)

    // Start with a random condition
    {
      const { name, conditions } = randomCondition()
      httpProxy.setConditions(conditions)
      wsProxy.setConditions(conditions)
      result.conditionsApplied.push(name)
      httpProxy.disruptAll()
      wsProxy.disruptAll()
    }

    // Wait for WS to connect via the ready promise
    await ws.ready

    // Message sending loop
    const startTime = Date.now()
    const endTime = startTime + CHAOS_DURATION_MS

    log('chaos-start', `duration=${CHAOS_DURATION_MS}ms, sendInterval=${SEND_INTERVAL_MS}ms`)

    while (Date.now() < endTime) {
      result.totalSends++

      // HTTP: send message via REST
      try {
        await httpClient.post('/messages', {
          text: 'chaos message ' + result.totalSends,
          ts: Date.now(),
        })
        result.successfulSends++
      } catch {
        result.failedSends++
        log('send-fail', `http #${result.totalSends} failed`)
      }

      // WS: send echo message if connected
      if (wsConnected) {
        try {
          ws.send(JSON.stringify({ type: 'chaos', seq: wsMsgsSent, ts: Date.now() }))
          wsMsgsSent++
        } catch {
          log('ws-send-fail', `ws send #${wsMsgsSent} failed`)
        }
      }

      await new Promise((r) => setTimeout(r, SEND_INTERVAL_MS))
    }

    // Cleanup
    clearInterval(conditionTimer)
    result.successRate = result.totalSends > 0
      ? result.successfulSends / result.totalSends
      : 0

    log('chaos-end', `successRate=${(result.successRate * 100).toFixed(1)}%`)
    ws.close()

    const wsEchoRate = wsMsgsSent > 0 ? wsMsgsReceived / wsMsgsSent : 0

    // ── Assertions ──
    console.log('')
    console.log('═══ NAPI Chaos Test Results ═══')
    console.log(`  Duration:       ${(result.durationMs / 1000).toFixed(0)}s`)
    console.log(`  HTTP sends:     ${result.totalSends} (ok=${result.successfulSends}, fail=${result.failedSends})`)
    console.log(`  HTTP rate:      ${(result.successRate * 100).toFixed(1)}%`)
    console.log(`  WS sent/recv:   ${wsMsgsSent}/${wsMsgsReceived} (echo rate ${(wsEchoRate * 100).toFixed(1)}%)`)
    console.log(`  WS disconnects: ${result.wsDisconnects}, reconnects: ${result.wsReconnects}`)
    console.log(`  WS bytes recv:  ${result.totalBytesReceived}`)
    console.log(`  Conditions:     ${result.conditionsApplied.length} switches`)
    console.log('')

    // HTTP: minimum acceptable success rate under chaos
    expect(result.successRate).toBeGreaterThanOrEqual(0.70)

    // WS: should have sent messages
    expect(wsMsgsSent).toBeGreaterThan(0)

    // Should have experienced at least some disruption
    expect(result.conditionsApplied.length).toBeGreaterThan(0)

    // Write result to file for analysis
    const outputDir = path.resolve('test-results')
    await fs.mkdir(outputDir, { recursive: true })
    await fs.writeFile(
      path.join(outputDir, 'napi-chaos-result.json'),
      JSON.stringify(result, null, 2),
      'utf-8',
    )
    console.log(`  Full result → test-results/napi-chaos-result.json`)
  }, CHAOS_DURATION_MS + 60_000)
})
