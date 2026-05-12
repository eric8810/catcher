/**
 * Micro-benchmark: msgpackr vs JSON serialization.
 *
 * Measures:
 *   - Encode time (serialize)
 *   - Decode time (deserialize)
 *   - Output size (bytes)
 *
 * Test data sizes: small (300B IM message), medium (20KB message list), large (500KB)
 */

import { bench, describe } from 'vitest'
import { pack, unpack } from '@catcher/ws'

// ── Test data ──────────────────────────────────────────────

const smallMsg = {
  event: 'message',
  id: 'msg_abc123',
  from: 'user_001',
  to: 'channel_general',
  text: 'Hello world! '.repeat(10), // ~150 bytes of text
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

// ── JSON baseline ──────────────────────────────────────────

describe('codec — encode', () => {
  bench('JSON.stringify — 300B msg', () => {
    JSON.stringify(smallMsg)
  })

  bench('msgpackr pack — 300B msg', () => {
    pack(smallMsg)
  })

  bench('JSON.stringify — 20KB list', () => {
    JSON.stringify(mediumList)
  })

  bench('msgpackr pack — 20KB list', () => {
    pack(mediumList)
  })

  bench('JSON.stringify — 500KB payload', () => {
    JSON.stringify(largePayload)
  })

  bench('msgpackr pack — 500KB payload', () => {
    pack(largePayload)
  })
})

describe('codec — decode', () => {
  const jsonSmall = JSON.stringify(smallMsg)
  const binSmall = pack(smallMsg)
  const jsonMedium = JSON.stringify(mediumList)
  const binMedium = pack(mediumList)
  const jsonLarge = JSON.stringify(largePayload)
  const binLarge = pack(largePayload)

  bench('JSON.parse — 300B msg', () => {
    JSON.parse(jsonSmall)
  })

  bench('msgpackr unpack — 300B msg', () => {
    unpack(binSmall)
  })

  bench('JSON.parse — 20KB list', () => {
    JSON.parse(jsonMedium)
  })

  bench('msgpackr unpack — 20KB list', () => {
    unpack(binMedium)
  })

  bench('JSON.parse — 500KB payload', () => {
    JSON.parse(jsonLarge)
  })

  bench('msgpackr unpack — 500KB payload', () => {
    unpack(binLarge)
  })
})

// ── Size comparison (one-shot, reported as console output) ─

describe('codec — size comparison', () => {
  bench('JSON size — 300B msg', () => {
    const s = JSON.stringify(smallMsg)
    return Buffer.byteLength(s)
  }, { time: 0 }) // single run

  bench('msgpackr size — 300B msg', () => {
    const b = pack(smallMsg)
    return b.length
  }, { time: 0 })
})
