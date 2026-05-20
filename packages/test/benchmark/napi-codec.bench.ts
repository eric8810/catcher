/**
 * Micro-benchmark: Rust rmp-serde (via NAPI) vs JS msgpackr vs JSON.
 *
 * Mirrors codec.bench.ts but adds Rust NAPI codec for three-way comparison:
 *   - JSON.stringify / JSON.parse  (baseline)
 *   - msgpackr pack / unpack      (JS, from @eric8810/catcher-ws)
 *   - Rust pack / unpack           (NAPI, from @eric8810/catcher-napi-ws/codec)
 *
 * Test data sizes: small (300B IM message), medium (20KB message list), large (500KB)
 */

import { bench, describe } from 'vitest'
import { pack as jsPack, unpack as jsUnpack } from '@eric8810/catcher-ws'
import { pack as rustPack, unpack as rustUnpack } from '@eric8810/catcher-napi-ws/codec'

// ── Test data ──────────────────────────────────────────────

const smallMsg = {
  event: 'message',
  id: 'msg_abc123',
  from: 'user_001',
  to: 'channel_general',
  text: 'Hello world! '.repeat(10),
  ts: Date.now(),
  metadata: { platform: 'desktop', version: '2.1.0' },
}

const mediumList = {
  messages: Array.from({ length: 50 }, (_, i) => ({
    id: 'msg_' + i,
    from: 'user_' + (i % 10),
    text: `Message number ${i}: ` + 'lorem ipsum dolor sit amet '.repeat(10),
    ts: Date.now() - i * 60000,
    status: i % 3,
  })),
}

const largePayload = {
  type: 'sync',
  channels: Array.from({ length: 30 }, (_, ci) => ({
    id: 'ch_' + ci,
    name: 'Channel ' + ci,
    messages: Array.from({ length: 100 }, (_, mi) => ({
      id: `msg_${ci}_${mi}`,
      from: 'user_' + (mi % 20),
      text: 'The quick brown fox jumps over the lazy dog. '.repeat(5),
      ts: Date.now() - mi * 30000,
    })),
  })),
}

// ── Encode ─────────────────────────────────────────────────

describe('napi codec — encode 300B', () => {
  bench('JSON.stringify', () => { JSON.stringify(smallMsg) })
  bench('JS msgpackr', () => { jsPack(smallMsg) })
  bench('Rust rmp-serde (NAPI)', () => { rustPack(smallMsg) })
})

describe('napi codec — encode 20KB', () => {
  bench('JSON.stringify', () => { JSON.stringify(mediumList) })
  bench('JS msgpackr', () => { jsPack(mediumList) })
  bench('Rust rmp-serde (NAPI)', () => { rustPack(mediumList) })
})

describe('napi codec — encode 500KB', () => {
  bench('JSON.stringify', () => { JSON.stringify(largePayload) })
  bench('JS msgpackr', () => { jsPack(largePayload) })
  bench('Rust rmp-serde (NAPI)', () => { rustPack(largePayload) })
})

// ── Decode ─────────────────────────────────────────────────

describe('napi codec — decode 300B', () => {
  const jsonBuf = JSON.stringify(smallMsg)
  const jsBin = jsPack(smallMsg)
  const rustBin = rustPack(smallMsg)

  bench('JSON.parse', () => { JSON.parse(jsonBuf) })
  bench('JS msgpackr', () => { jsUnpack(jsBin) })
  bench('Rust rmp-serde (NAPI)', () => { rustUnpack(rustBin) })
})

describe('napi codec — decode 20KB', () => {
  const jsonBuf = JSON.stringify(mediumList)
  const jsBin = jsPack(mediumList)
  const rustBin = rustPack(mediumList)

  bench('JSON.parse', () => { JSON.parse(jsonBuf) })
  bench('JS msgpackr', () => { jsUnpack(jsBin) })
  bench('Rust rmp-serde (NAPI)', () => { rustUnpack(rustBin) })
})

describe('napi codec — decode 500KB', () => {
  const jsonBuf = JSON.stringify(largePayload)
  const jsBin = jsPack(largePayload)
  const rustBin = rustPack(largePayload)

  bench('JSON.parse', () => { JSON.parse(jsonBuf) })
  bench('JS msgpackr', () => { jsUnpack(jsBin) })
  bench('Rust rmp-serde (NAPI)', () => { rustUnpack(rustBin) })
})

// ── Size comparison ────────────────────────────────────────

describe('napi codec — size 300B', () => {
  bench('JSON size', () => { return Buffer.byteLength(JSON.stringify(smallMsg)) }, { time: 0 })
  bench('JS msgpackr size', () => { return jsPack(smallMsg).length }, { time: 0 })
  bench('Rust rmp-serde size', () => { return rustPack(smallMsg).length }, { time: 0 })
})
