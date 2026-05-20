/**
 * End-to-end benchmark: built-in msgpack codec vs JSON passthrough.
 *
 * Measures the full HTTP roundtrip with msgpack: true vs msgpack: false
 * against a local echo server. The codec runs entirely inside Rust
 * transport — no NAPI boundary crossing for encode/decode.
 *
 * Compares:
 *   - Throughput (req/sec)
 *   - Wire size (request body bytes)
 *   - Latency distribution
 */

import http from 'node:http'
import { describe, it, expect, beforeAll, afterAll } from 'vitest'
import { HttpClient } from '@eric8810/catcher-napi-http'

const TIMEOUT = 120_000
const REQUESTS = parseInt(process.env.MSGPACK_BENCH_REQUESTS ?? '200', 10)

// ── Msgpack-aware echo server ──

interface EchoServer { port: number; close: () => Promise<void>; stats: { msgpackReqs: number; jsonReqs: number; totalBytes: number } }

function createEchoServer(): Promise<EchoServer> {
  const stats = { msgpackReqs: 0, jsonReqs: 0, totalBytes: 0 }
  return new Promise((resolve) => {
    const server = http.createServer((req, res) => {
      const chunks: Buffer[] = []
      req.on('data', (c: Buffer) => chunks.push(c))
      req.on('end', () => {
        const body = Buffer.concat(chunks)
        stats.totalBytes += body.length
        const ct = req.headers['content-type'] ?? ''
        if (ct.includes('msgpack')) {
          stats.msgpackReqs++
          res.writeHead(200, { 'Content-Type': 'application/msgpack' })
        } else {
          stats.jsonReqs++
          res.writeHead(200, { 'Content-Type': 'application/json' })
        }
        res.end(body)
      })
    })
    server.listen(0, '127.0.0.1', () => {
      const addr = server.address() as { port: number }
      resolve({ port: addr.port, close: () => new Promise(r => server.close(() => r())), stats })
    })
  })
}

// ── Helpers ──

function computeStats(times: number[]) {
  if (!times.length) return { p50: 0, p95: 0, mean: 0 }
  const sorted = [...times].sort((a, b) => a - b)
  return {
    p50: sorted[Math.ceil(0.5 * sorted.length) - 1],
    p95: sorted[Math.ceil(0.95 * sorted.length) - 1],
    mean: Math.round(sorted.reduce((a, b) => a + b, 0) / sorted.length),
  }
}

// ── Test data ──

const smallPayload = JSON.stringify({
  event: 'message', id: 'msg_001', from: 'user_001',
  text: 'Hello world! '.repeat(10), ts: Date.now(),
})

const mediumPayload = JSON.stringify({
  messages: Array.from({ length: 50 }, (_, i) => ({
    id: `msg_${i}`, from: `user_${i % 10}`,
    text: 'The quick brown fox. '.repeat(10), ts: Date.now() - i * 60000,
  })),
})

// ── Tests ──

describe('Msgpack codec E2E — small payload (300B)', () => {
  let server: EchoServer

  beforeAll(async () => { server = await createEchoServer() }, 30_000)
  afterAll(async () => { await server.close() })

  it(`JSON passthrough — ${REQUESTS} requests`, async () => {
    const client = new HttpClient(JSON.stringify({
      base_url: `http://127.0.0.1:${server.port}`,
      connect_timeout_ms: 5000, response_timeout_ms: 10_000,
    }))
    const times: number[] = []
    const start = Date.now()
    for (let i = 0; i < REQUESTS; i++) {
      const t0 = Date.now()
      await client.post('/', Buffer.from(smallPayload), { content_type: 'application/json' })
      times.push(Date.now() - t0)
    }
    const elapsed = Date.now() - start
    const stats = computeStats(times)
    console.log(`\n  ┌─ JSON (${REQUESTS} × 300B) ─────────`)
    console.log(`  │ total:   ${elapsed}ms, req/s: ${(REQUESTS / (elapsed / 1000)).toFixed(0)}`)
    console.log(`  │ p50:     ${stats.p50}ms, p95: ${stats.p95}ms, mean: ${stats.mean}ms`)
    console.log(`  │ server:  ${server.stats.jsonReqs} json reqs`)
    console.log(`  └────────────────────────────────\n`)
    expect(times.length).toBe(REQUESTS)
  }, TIMEOUT)

  it(`msgpack: true — ${REQUESTS} requests`, async () => {
    server.stats.msgpackReqs = 0
    server.stats.totalBytes = 0
    const client = new HttpClient(JSON.stringify({
      base_url: `http://127.0.0.1:${server.port}`,
      connect_timeout_ms: 5000, response_timeout_ms: 10_000,
      msgpack: true,
    }))
    const times: number[] = []
    const start = Date.now()
    for (let i = 0; i < REQUESTS; i++) {
      const t0 = Date.now()
      await client.post('/', Buffer.from(smallPayload))
      times.push(Date.now() - t0)
    }
    const elapsed = Date.now() - start
    const stats = computeStats(times)
    const avgWireBytes = Math.round(server.stats.totalBytes / REQUESTS)
    console.log(`\n  ┌─ msgpack (${REQUESTS} × 300B) ──────`)
    console.log(`  │ total:   ${elapsed}ms, req/s: ${(REQUESTS / (elapsed / 1000)).toFixed(0)}`)
    console.log(`  │ p50:     ${stats.p50}ms, p95: ${stats.p95}ms, mean: ${stats.mean}ms`)
    console.log(`  │ server:  ${server.stats.msgpackReqs} msgpack reqs, avg wire: ${avgWireBytes}B`)
    console.log(`  │ vs JSON: ${smallPayload.length}B → ${avgWireBytes}B (${((1 - avgWireBytes / smallPayload.length) * 100).toFixed(0)}% smaller)`)
    console.log(`  └────────────────────────────────\n`)
    expect(server.stats.msgpackReqs).toBe(REQUESTS)
  }, TIMEOUT)
})

describe('Msgpack codec E2E — medium payload (20KB)', () => {
  let server: EchoServer

  beforeAll(async () => { server = await createEchoServer() }, 30_000)
  afterAll(async () => { await server.close() })

  it(`JSON passthrough — ${REQUESTS} requests`, async () => {
    const client = new HttpClient(JSON.stringify({
      base_url: `http://127.0.0.1:${server.port}`,
      connect_timeout_ms: 5000, response_timeout_ms: 10_000,
    }))
    const times: number[] = []
    const start = Date.now()
    for (let i = 0; i < REQUESTS; i++) {
      const t0 = Date.now()
      await client.post('/', Buffer.from(mediumPayload), { content_type: 'application/json' })
      times.push(Date.now() - t0)
    }
    const elapsed = Date.now() - start
    const stats = computeStats(times)
    console.log(`\n  ┌─ JSON (${REQUESTS} × 20KB) ──────────`)
    console.log(`  │ total:   ${elapsed}ms, req/s: ${(REQUESTS / (elapsed / 1000)).toFixed(0)}`)
    console.log(`  │ p50:     ${stats.p50}ms, p95: ${stats.p95}ms, mean: ${stats.mean}ms`)
    console.log(`  └────────────────────────────────\n`)
    expect(times.length).toBe(REQUESTS)
  }, TIMEOUT)

  it(`msgpack: true — ${REQUESTS} requests`, async () => {
    server.stats.msgpackReqs = 0
    server.stats.totalBytes = 0
    const client = new HttpClient(JSON.stringify({
      base_url: `http://127.0.0.1:${server.port}`,
      connect_timeout_ms: 5000, response_timeout_ms: 10_000,
      msgpack: true,
    }))
    const times: number[] = []
    const start = Date.now()
    for (let i = 0; i < REQUESTS; i++) {
      const t0 = Date.now()
      await client.post('/', Buffer.from(mediumPayload))
      times.push(Date.now() - t0)
    }
    const elapsed = Date.now() - start
    const stats = computeStats(times)
    const avgWireBytes = Math.round(server.stats.totalBytes / REQUESTS)
    console.log(`\n  ┌─ msgpack (${REQUESTS} × 20KB) ───────`)
    console.log(`  │ total:   ${elapsed}ms, req/s: ${(REQUESTS / (elapsed / 1000)).toFixed(0)}`)
    console.log(`  │ p50:     ${stats.p50}ms, p95: ${stats.p95}ms, mean: ${stats.mean}ms`)
    console.log(`  │ server:  ${server.stats.msgpackReqs} msgpack reqs, avg wire: ${avgWireBytes}B`)
    console.log(`  │ vs JSON: ${mediumPayload.length}B → ${avgWireBytes}B (${((1 - avgWireBytes / mediumPayload.length) * 100).toFixed(0)}% smaller)`)
    console.log(`  └────────────────────────────────\n`)
    expect(server.stats.msgpackReqs).toBe(REQUESTS)
  }, TIMEOUT)
})
