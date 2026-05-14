/**
 * 单独验证 S1 Cold start 极弱网 — 20 iterations
 * 用法: npx tsx packages/test/e2e/verify-s1-extreme.ts
 */
import axios from 'axios'
import { createRustHttpClient } from '../adapters/rust-adapter.js'
import { clearDnsCache } from '../adapters/dns-adapter.js'
import { createHttpTestServer, type TestServer } from '../servers/http-server.js'
import { createNetworkProxy, type NetworkProxy } from '../network/proxy.js'
import { NETWORK_PROFILES } from '../network/presets.js'
import type { IterationResult } from '../harness.js'

const ITERATIONS = 20
const profile = NETWORK_PROFILES.veryWeak

async function vanillaS1(baseUrl: string): Promise<IterationResult> {
  const start = Date.now()
  try {
    await axios.get(baseUrl + '/channels', { timeout: 10_000 })
    return { success: true, time: Date.now() - start, connections: 1 }
  } catch {
    return { success: false, time: 10_000, connections: 1 }
  }
}

async function catcherS1(baseUrl: string): Promise<IterationResult> {
  clearDnsCache()
  const start = Date.now()
  let retries = 0
  try {
    const client = createRustHttpClient({
      baseURL: baseUrl,
      keepAlive: true,
      dnsCacheTtl: 300,
      retry: { attempts: 2, backoff: 'exponential', onRetry: () => { retries++ } },
      timeout: { response: 10_000 },
    })
    await client.get('/channels')
    return { success: true, time: Date.now() - start, connections: 1, retries }
  } catch {
    return { success: false, time: 10_000, connections: 1, retries }
  }
}

async function main() {
  const server: TestServer = await createHttpTestServer()
  const proxy: NetworkProxy = createNetworkProxy(server.port)
  await proxy.start()
  const url = `http://127.0.0.1:${proxy.port}`

  // Sanity check: good network first
  proxy.setConditions(NETWORK_PROFILES.good.conditions)
  proxy.disruptAll()
  console.log('Sanity check (good network)...')
  const [v0, c0] = await Promise.all([vanillaS1(url), catcherS1(url)])
  console.log(`  vanilla: ${v0.success ? '✓' : '✗'} ${v0.time}ms | catcher: ${c0.success ? '✓' : '✗'} ${c0.time}ms`)
  if (!v0.success || !c0.success) {
    console.error('Proxy broken even on good network! Aborting.')
    await proxy.stop(); await server.close(); process.exit(1)
  }
  console.log('Proxy OK.\n')

  let vanillaWins = 0
  let catcherWins = 0
  let vanillaOk = 0
  let catcherOk = 0

  console.log(`S1 Cold start 🔴 极弱网 — ${ITERATIONS} iterations`)
  console.log(`Network: ${profile.emoji} ${profile.name}`)
  console.log('---')

  proxy.setConditions(profile.conditions)
  proxy.disruptAll()

  for (let i = 0; i < ITERATIONS; i++) {
    // Run both concurrently (same as e2e test)
    const [v, c] = await Promise.all([
      vanillaS1(url),
      catcherS1(url),
    ])

    vanillaOk += v.success ? 1 : 0
    catcherOk += c.success ? 1 : 0
    if (v.success && !c.success) vanillaWins++
    if (c.success && !v.success) catcherWins++

    const vMark = v.success ? '✓' : '✗'
    const cMark = c.success ? '✓' : '✗'
    const retryInfo = c.retries ? ` (${c.retries} retries)` : ''
    console.log(`[${i + 1}/${ITERATIONS}] vanilla: ${vMark} ${v.time}ms | catcher: ${cMark} ${c.time}ms${retryInfo}`)
  }

  console.log('---')
  console.log(`Vanilla: ${vanillaOk}/${ITERATIONS} (${(vanillaOk / ITERATIONS * 100).toFixed(0)}%)`)
  console.log(`Catcher: ${catcherOk}/${ITERATIONS} (${(catcherOk / ITERATIONS * 100).toFixed(0)}%)`)
  console.log(`Catcher-only wins: ${catcherWins}, Vanilla-only wins: ${vanillaWins}`)

  await proxy.stop()
  await server.close()
}

main().catch(e => { console.error(e); process.exit(1) })
