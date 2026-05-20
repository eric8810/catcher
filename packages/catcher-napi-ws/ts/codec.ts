import { loadNativeAddon } from './native'

const native = loadNativeAddon('catcher-napi-ws')

/** Encode a value to msgpack bytes using Rust rmp-serde. */
export function pack(value: unknown): Buffer {
  const jsonValue = typeof value === 'string' ? JSON.parse(value) : value
  return native.pack(jsonValue)
}

/** Decode msgpack bytes to a parsed value using Rust rmp-serde. */
export function unpack(data: Buffer | Uint8Array): any {
  const buf = Buffer.isBuffer(data) ? data : Buffer.from(data)
  return JSON.parse(native.unpack(buf))
}
