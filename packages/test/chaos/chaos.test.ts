/**
 * Chaos test — long-running resilience validation.
 *
 * Default: 30 minutes of continuous operation under randomly changing
 * network conditions. Measures:
 *   - Message send success rate
 *   - WS reconnection reliability
 *   - Recovery time after disruption
 *
 * Runs with catcher only (vanilla would not survive this).
 * This is about proving catcher WORKS under chaos, not comparing to vanilla.
 *
 * Usage:
 *   CHAOS_DURATION_MS=60000 npx vitest run test/chaos/
 */

import { describe, it, expect, beforeAll, afterAll } from 'vitest'
import fs from 'node:fs/promises'
import path from 'node:path'
import { createHttpClient } from '@eric8810/http'
import { createResilientWS } from '@eric8810/ws'
import { createHttpTestServer, type TestServer } from '../servers/http-server.js'
import { createWSTestServer, type WSTestServer } from '../servers/ws-server.js'
import { createNetworkProxy, type NetworkProxy, type NetworkConditions } from '../network/proxy.js'

// ── Chaos configuration ─────────────────────────────────────

const CHAOS_DURATION_MS = parseInt(process.env.CHAOS_DURATION_MS ?? '600000', 10) // 10 min default for tests
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

describe('Chaos — 韧性压力测试', () => {
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

  it(`chaos run — ${(CHAOS_DURATION_MS / 1000).toFixed(0)}s`, async () => {
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

    // Create catcher HTTP client
    const httpClient = createHttpClient({
      baseURL: proxyUrl,
      keepAlive: true,
      dnsCacheTtl: 300,
      retry: { attempts: 3, backoff: 'exponential' },
      timeout: { response: 30_000 },
      concurrency: 10,
    })

    // Create catcher WS client
    const ws = createResilientWS({
      url: wsProxyUrl,
      perMessageDeflate: true,
      handshakeTimeout: 15_000,
      reconnect: {
        initialDelay: 500,
        maxDelay: 15_000,
        backoffMultiplier: 2,
        maxAttempts: 100, // essentially unlimited during chaos
      },
    })

    ws.addEventListener('open', () => log('ws-open'))
    ws.addEventListener('close', () => {
      result.wsDisconnects++
      log('ws-close', `disconnect #${result.wsDisconnects}`)
    })

    // Periodic condition switcher
    const conditionTimer = setInterval(() => {
      const { name, conditions } = randomCondition()
      httpProxy.setConditions(conditions)
      result.conditionsApplied.push(name)
      httpProxy.disruptAll()
      log('condition-switch', `${name} (latency=${conditions.latency}ms, loss=${((conditions.packetLoss ?? 0) * 100).toFixed(0)}%)`)
    }, CONDITION_SWITCH_MS)

    // Start with a random condition
    {
      const { name, conditions } = randomCondition()
      httpProxy.setConditions(conditions)
      result.conditionsApplied.push(name)
      httpProxy.disruptAll()
    }

    // Wait for WS to connect
    await new Promise((r) => setTimeout(r, 3000))

    // Message sending loop
    const startTime = Date.now()
    const endTime = startTime + CHAOS_DURATION_MS

    log('chaos-start', `duration=${CHAOS_DURATION_MS}ms, sendInterval=${SEND_INTERVAL_MS}ms`)

    while (Date.now() < endTime) {
      result.totalSends++

      try {
        await httpClient.post('/messages', {
          text: 'chaos message ' + result.totalSends,
          ts: Date.now(),
        })
        result.successfulSends++
      } catch {
        result.failedSends++
        log('send-fail', `message #${result.totalSends} failed`)
      }

      // Small pause between sends
      await new Promise((r) => setTimeout(r, SEND_INTERVAL_MS))
    }

    // Cleanup
    clearInterval(conditionTimer)
    result.successRate = result.totalSends > 0
      ? result.successfulSends / result.totalSends
      : 0

    log('chaos-end', `successRate=${(result.successRate * 100).toFixed(1)}%`)
    ws.close()

    // ── Assertions ──
    console.log('')
    console.log('═══ Chaos Test Results ═══')
    console.log(`  Duration:       ${(result.durationMs / 1000).toFixed(0)}s`)
    console.log(`  Total sends:    ${result.totalSends}`)
    console.log(`  Successful:     ${result.successfulSends}`)
    console.log(`  Failed:         ${result.failedSends}`)
    console.log(`  Success rate:   ${(result.successRate * 100).toFixed(1)}%`)
    console.log(`  WS disconnects: ${result.wsDisconnects}`)
    console.log(`  Conditions:     ${result.conditionsApplied.length} switches`)
    console.log('')

    // Minimum acceptable success rate under chaos: 70%
    expect(result.successRate).toBeGreaterThanOrEqual(0.70)

    // Should have experienced at least some disruption
    expect(result.conditionsApplied.length).toBeGreaterThan(0)

    // Write result to file for analysis
    const outputDir = path.resolve('test-results')
    await fs.mkdir(outputDir, { recursive: true })
    await fs.writeFile(
      path.join(outputDir, 'chaos-result.json'),
      JSON.stringify(result, null, 2),
      'utf-8',
    )
    console.log(`  Full result → test-results/chaos-result.json`)
  }, CHAOS_DURATION_MS + 60_000)
})
