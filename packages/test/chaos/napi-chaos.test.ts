/**
 * NAPI Chaos test — long-running resilience validation using Rust NAPI bindings.
 *
 * Default: 60 seconds. Override with CHAOS_DURATION_MS env var.
 *
 * Measures:
 *   - HTTP message send success rate under random network conditions
 *   - WS echo round-trip under the same disruption
 *   - WS reconnection after disconnects
 */

import { describe, it, expect, beforeAll, afterAll } from 'vitest'
import fs from 'node:fs/promises'
import path from 'node:path'
import { createRustHttpClient, createRustWsClient } from '../adapters/rust-adapter.js'
import { createHttpTestServer, type TestServer } from '../servers/http-server.js'
import { createWSTestServer, type WSTestServer } from '../servers/ws-server.js'
import { createNetworkProxy, type NetworkProxy, type NetworkConditions } from '../network/proxy.js'

const CHAOS_DURATION_MS = parseInt(process.env.CHAOS_DURATION_MS ?? '60000', 10)
const SEND_INTERVAL_MS = 500
const CONDITION_SWITCH_MS = 15_000

interface ChaosResult {
  durationMs: number
  totalHttpSends: number
  httpSuccesses: number
  httpFailures: number
  httpSuccessRate: number
  wsMsgsSent: number
  wsMsgsReceived: number
  wsEchoRate: number
  wsDisconnects: number
  wsReconnects: number
  conditionsApplied: string[]
  timeline: Array<{ ts: number; event: string; detail?: string }>
}

function randomCondition(): { name: string; conditions: NetworkConditions } {
  const profiles = [
    { name: 'good', latency: 25, packetLoss: 0, bandwidth: 0, connectionReset: 0 },
    { name: 'weak', latency: 500, packetLoss: 0.05, bandwidth: 50_000, connectionReset: 0.02 },
    { name: 'very-weak', latency: 1500, packetLoss: 0.1, bandwidth: 10_000, connectionReset: 0.05 },
    { name: 'spike', latency: 3000, packetLoss: 0.3, bandwidth: 5_000, connectionReset: 0.1 },
    { name: 'packet-storm', latency: 200, packetLoss: 0.5, bandwidth: 0, connectionReset: 0.2 },
    { name: 'normal', latency: 100, packetLoss: 0.01, bandwidth: 0, connectionReset: 0 },
  ]
  const p = profiles[Math.floor(Math.random() * profiles.length)]
  return {
    name: p.name,
    conditions: {
      latency: Math.max(0, p.latency + Math.random() * 200 - 100),
      packetLoss: Math.max(0, p.packetLoss + (Math.random() - 0.5) * 0.1),
      bandwidth: p.bandwidth === 0 ? 0 : p.bandwidth + Math.random() * 20_000,
      connectionReset: Math.max(0, p.connectionReset + (Math.random() - 0.5) * 0.05),
    },
  }
}

describe('NAPI Chaos — 韧性压力测试 (Rust)', () => {
  let httpServer: TestServer
  let wsServer: WSTestServer
  let httpProxy: NetworkProxy
  let wsProxy: NetworkProxy

  beforeAll(async () => {
    httpServer = await createHttpTestServer()
    wsServer = await createWSTestServer()
    httpProxy = createNetworkProxy(httpServer.port)
    wsProxy = createNetworkProxy(wsServer.port)
    await httpProxy.start()
    await wsProxy.start()
  }, 30_000)

  afterAll(async () => {
    await httpProxy.stop()
    await wsProxy.stop()
    await httpServer.close()
    await wsServer.close()
  })

  it(`napi chaos run — ${(CHAOS_DURATION_MS / 1000).toFixed(0)}s`, async () => {
    const proxyUrl = `http://127.0.0.1:${httpProxy.port}`
    const wsProxyUrl = `ws://127.0.0.1:${wsProxy.port}`

    const result: ChaosResult = {
      durationMs: CHAOS_DURATION_MS,
      totalHttpSends: 0, httpSuccesses: 0, httpFailures: 0, httpSuccessRate: 0,
      wsMsgsSent: 0, wsMsgsReceived: 0, wsEchoRate: 0,
      wsDisconnects: 0, wsReconnects: 0,
      conditionsApplied: [], timeline: [],
    }

    const log = (event: string, detail?: string) => {
      result.timeline.push({ ts: Date.now(), event, detail })
      if (detail) console.log(`  [${new Date().toISOString()}] ${event}: ${detail}`)
    }

    // ── HTTP client: short timeout for chaos ──
    const httpClient = createRustHttpClient({
      baseURL: proxyUrl,
      keepAlive: true,
      retry: { attempts: 2, backoff: 'exponential' },
      timeout: { response: 8_000 },
      concurrency: 10,
    })

    // ── WS client ──
    let wsConnected = false
    let hasConnectedOnce = false
    let wsHandle: ReturnType<typeof createRustWsClient> | null = null

    // Start WS connection under good conditions first
    httpProxy.setConditions({ latency: 5, packetLoss: 0 })
    wsProxy.setConditions({ latency: 5, packetLoss: 0 })

    wsHandle = createRustWsClient({
      url: wsProxyUrl,
      perMessageDeflate: false,
      handshakeTimeout: 10_000,
      reconnect: { maxAttempts: 20 },
    })

    wsHandle.addEventListener('open', () => {
      if (hasConnectedOnce) {
        result.wsReconnects++
        log('ws-reconnect', `reconnect #${result.wsReconnects}`)
      } else {
        log('ws-open')
        hasConnectedOnce = true
      }
      wsConnected = true
    })
    wsHandle.addEventListener('close', () => {
      wsConnected = false
      result.wsDisconnects++
      log('ws-close', `disconnect #${result.wsDisconnects}`)
    })
    wsHandle.addEventListener('message', (msg: any) => {
      try {
        const decoded = Buffer.from(msg.data ?? '', 'base64').toString('utf-8')
        if (decoded.includes('chaos')) {
          result.wsMsgsReceived++
        }
      } catch { /* ignore */ }
    })

    // Wait for initial WS connection — fixed short delay, not await ready
    await new Promise(r => setTimeout(r, 3_000))

    // ── Chaos loop ──
    const endTime = Date.now() + CHAOS_DURATION_MS
    log('chaos-start', `duration=${CHAOS_DURATION_MS}ms, interval=${SEND_INTERVAL_MS}ms`)

    // Apply random initial condition
    {
      const { name, conditions } = randomCondition()
      httpProxy.setConditions(conditions)
      wsProxy.setConditions(conditions)
      result.conditionsApplied.push(name)
      httpProxy.disruptAll()
      wsProxy.disruptAll()
      log('condition-switch', name)
    }

    const conditionTimer = setInterval(() => {
      const { name, conditions } = randomCondition()
      httpProxy.setConditions(conditions)
      wsProxy.setConditions(conditions)
      result.conditionsApplied.push(name)
      httpProxy.disruptAll()
      wsProxy.disruptAll()
      log('condition-switch', name)
    }, CONDITION_SWITCH_MS)

    while (Date.now() < endTime) {
      result.totalHttpSends++

      // HTTP
      try {
        await httpClient.post('/messages', {
          text: 'chaos message ' + result.totalHttpSends,
          ts: Date.now(),
        })
        result.httpSuccesses++
      } catch {
        result.httpFailures++
      }

      // WS
      if (wsConnected && wsHandle) {
        try {
          wsHandle.send(JSON.stringify({ type: 'chaos', seq: result.wsMsgsSent, ts: Date.now() }))
          result.wsMsgsSent++
        } catch { /* WS send failed — expected under disruption */ }
      }

      await new Promise(r => setTimeout(r, SEND_INTERVAL_MS))
    }

    // ── Cleanup ──
    clearInterval(conditionTimer)

    // Close WS gracefully
    try { wsHandle?.close() } catch { /* ignore */ }

    result.httpSuccessRate = result.totalHttpSends > 0
      ? result.httpSuccesses / result.totalHttpSends : 0
    result.wsEchoRate = result.wsMsgsSent > 0
      ? result.wsMsgsReceived / result.wsMsgsSent : 0

    log('chaos-end', `HTTP ${(result.httpSuccessRate * 100).toFixed(1)}%, WS echo ${(result.wsEchoRate * 100).toFixed(1)}%`)

    // ── Report ──
    console.log('')
    console.log('═══ NAPI Chaos Test Results ═══')
    console.log(`  Duration:       ${(result.durationMs / 1000).toFixed(0)}s`)
    console.log(`  HTTP:           ${result.totalHttpSends} sends, ${result.httpSuccesses} ok, ${result.httpFailures} fail (${(result.httpSuccessRate * 100).toFixed(1)}%)`)
    console.log(`  WS:             ${result.wsMsgsSent} sent, ${result.wsMsgsReceived} received (echo ${(result.wsEchoRate * 100).toFixed(1)}%)`)
    console.log(`  WS disconnects: ${result.wsDisconnects}, reconnects: ${result.wsReconnects}`)
    console.log(`  Conditions:     ${result.conditionsApplied.length} switches`)
    console.log('')

    // ── Assertions ──
    expect(result.totalHttpSends).toBeGreaterThan(0)
    expect(result.httpSuccessRate).toBeGreaterThanOrEqual(0.50)
    expect(result.conditionsApplied.length).toBeGreaterThan(0)

    // Write result
    const outputDir = path.resolve('test-results')
    await fs.mkdir(outputDir, { recursive: true })
    await fs.writeFile(
      path.join(outputDir, 'napi-chaos-result.json'),
      JSON.stringify(result, null, 2), 'utf-8',
    )
  }, CHAOS_DURATION_MS + 30_000)
})
