import { pack as _pack, unpack as _unpack } from 'msgpackr'

/**
 * Encode a value to msgpack binary.
 * Falls back to JSON.stringify if msgpackr is unavailable.
 */
export function pack(value: any): Buffer {
  return Buffer.from(_pack(value))
}

/**
 * Decode msgpack binary to a value.
 * Accepts both Buffer and Uint8Array.
 */
export function unpack(buffer: Buffer | Uint8Array): any {
  return _unpack(buffer)
}

/**
 * Check if a WebSocket data frame is binary (msgpack) or text (JSON fallback).
 */
export function isBinary(data: any): data is Buffer {
  return Buffer.isBuffer(data) || data instanceof ArrayBuffer ||
    data instanceof Uint8Array
}

/**
 * Decode a WebSocket message frame, auto-detecting binary vs text.
 */
export function decodeWSMessage(data: any): any {
  if (isBinary(data)) {
    return unpack(data instanceof Buffer ? data : Buffer.from(data))
  }
  // Fallback: JSON text
  try {
    return JSON.parse(typeof data === 'string' ? data : data.toString())
  } catch {
    return data
  }
}
