import { describe, it, expect } from 'vitest'
import { pack, unpack, isBinary, decodeWSMessage } from '../codec.js'

// ── C1-C6: pack / unpack ─────────────────────────────────────────

describe('C1 — pack object returns Buffer', () => {
  it('packs {type:"msg",text:"hello"} into Buffer', () => {
    const result = pack({ type: 'msg', text: 'hello' })
    expect(Buffer.isBuffer(result)).toBe(true)
    expect(result.length).toBeGreaterThan(0)
  })
})

describe('C2 — unpack round-trip', () => {
  it('pack → unpack returns original object', () => {
    const original = { type: 'msg', text: 'hello' }
    expect(unpack(pack(original))).toEqual(original)
  })
})

describe('C3 — pack array round-trip', () => {
  it('pack → unpack preserves array', () => {
    const original = [1, 2, 3]
    expect(unpack(pack(original))).toEqual(original)
  })
})

describe('C4 — pack nested object round-trip', () => {
  it('pack → unpack preserves deeply nested object', () => {
    const original = { a: { b: { c: 1 } } }
    expect(unpack(pack(original))).toEqual(original)
  })
})

describe('C5 — pack empty object round-trip', () => {
  it('pack → unpack handles empty object', () => {
    const original = {}
    expect(unpack(pack(original))).toEqual(original)
  })
})

describe('C6 — unpack accepts Uint8Array', () => {
  it('Uint8Array from pack result decodes correctly', () => {
    const original = { x: 1 }
    const buf = pack(original)
    const uint8 = new Uint8Array(buf)
    expect(unpack(uint8)).toEqual(original)
  })
})

// ── C7-C10: isBinary ─────────────────────────────────────────────

describe('C7 — Buffer → true', () => {
  it('Buffer.isBuffer data returns true', () => {
    expect(isBinary(Buffer.from('hi'))).toBe(true)
  })
})

describe('C8 — ArrayBuffer → true', () => {
  it('ArrayBuffer data returns true', () => {
    expect(isBinary(new ArrayBuffer(8))).toBe(true)
  })
})

describe('C9 — Uint8Array → true', () => {
  it('Uint8Array data returns true', () => {
    expect(isBinary(new Uint8Array(8))).toBe(true)
  })
})

describe('C10 — string → false', () => {
  it('string data returns false', () => {
    expect(isBinary('hello')).toBe(false)
  })
})

// ── C11-C14: decodeWSMessage ─────────────────────────────────────

describe('C11 — Binary msgpack decode', () => {
  it('auto-decodes msgpack binary', () => {
    const encoded = pack({ type: 'msg' })
    expect(decodeWSMessage(encoded)).toEqual({ type: 'msg' })
  })
})

describe('C12 — JSON string decode', () => {
  it('parses JSON string', () => {
    expect(decodeWSMessage('{"type":"msg"}')).toEqual({ type: 'msg' })
  })
})

describe('C13 — Non-JSON string returned as-is', () => {
  it('returns original string when not valid JSON', () => {
    expect(decodeWSMessage('not json')).toBe('not json')
  })
})

describe('C14 — Buffer input', () => {
  it('decodes Buffer via msgpack', () => {
    const encoded = pack({ x: 1 })
    expect(Buffer.isBuffer(encoded)).toBe(true)
    expect(decodeWSMessage(encoded)).toEqual({ x: 1 })
  })
})
