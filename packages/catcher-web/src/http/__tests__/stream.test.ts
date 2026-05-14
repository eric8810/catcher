import { describe, it, expect, vi } from 'vitest'
import { createWebClient } from '../client.js'

/**
 * Browser stream tests (ST5-ST6).
 * Mock globalThis.fetch to return a Response with a ReadableStream body.
 */

function createMockReadableStream(chunks: string[]): ReadableStream<Uint8Array> {
  return new ReadableStream({
    start(controller) {
      for (const chunk of chunks) {
        controller.enqueue(new TextEncoder().encode(chunk))
      }
      controller.close()
    },
  })
}

describe('ST5 — responseType: "stream" returns ReadableStream', () => {
  it('returns an object with data being a ReadableStream', async () => {
    const stream = createMockReadableStream(['hello', ' world'])
    const mockResp = {
      status: 200,
      ok: true,
      headers: new Headers({ 'content-type': 'text/plain' }),
      body: stream,
      json: () => Promise.resolve({}),
      text: () => Promise.resolve(''),
      arrayBuffer: () => Promise.resolve(new ArrayBuffer(0)),
    } as unknown as Response

    const fetchSpy = vi.fn().mockResolvedValue(mockResp)
    const originalFetch = globalThis.fetch
    globalThis.fetch = fetchSpy

    try {
      const client = createWebClient({ baseURL: 'http://localhost' })
      const result = await client.get('/stream', { responseType: 'stream' } as any)

      expect(result.status).toBe(200)
      expect(result.data).toBeDefined()
      // Should be a ReadableStream (has getReader / locked)
      expect(result.data).toBe(stream)
    } finally {
      globalThis.fetch = originalFetch
    }
  })
})

describe('ST6 — Stream reads complete data', () => {
  it('reads all chunks from the ReadableStream', async () => {
    const stream = createMockReadableStream(['chunk1', 'chunk2', 'chunk3'])
    const mockResp = {
      status: 200,
      ok: true,
      headers: new Headers(),
      body: stream,
      json: () => Promise.resolve({}),
      text: () => Promise.resolve(''),
      arrayBuffer: () => Promise.resolve(new ArrayBuffer(0)),
    } as unknown as Response

    const fetchSpy = vi.fn().mockResolvedValue(mockResp)
    const originalFetch = globalThis.fetch
    globalThis.fetch = fetchSpy

    try {
      const client = createWebClient({ baseURL: 'http://localhost' })
      const result = await client.get('/stream', { responseType: 'stream' } as any)

      // Read all chunks from the ReadableStream
      const reader = (result.data as ReadableStream<Uint8Array>).getReader()
      const chunks: Uint8Array[] = []
      while (true) {
        const { done, value } = await reader.read()
        if (done) break
        chunks.push(value)
      }

      const fullText = chunks.map(c => new TextDecoder().decode(c)).join('')
      expect(fullText).toBe('chunk1chunk2chunk3')
    } finally {
      globalThis.fetch = originalFetch
    }
  })
})
