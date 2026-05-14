import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { createWebClient } from '../client.js'

/**
 * Browser CORS/credentials tests (C5-C9).
 * Mock globalThis.fetch since we're running in Node.js.
 */

function mockResponse(status = 200, body: any = { ok: true }, headers: Record<string, string> = {}): Response {
  return {
    status,
    ok: status >= 200 && status < 300,
    headers: new Headers(headers),
    json: () => Promise.resolve(body),
    text: () => Promise.resolve(typeof body === 'string' ? body : JSON.stringify(body)),
    arrayBuffer: () => Promise.resolve(new ArrayBuffer(0)),
    body: null,
  } as unknown as Response
}

describe('C5 — credentials: "include" passed to fetch', () => {
  it('fetch receives credentials: "include"', async () => {
    const fetchSpy = vi.fn().mockResolvedValue(mockResponse())
    const originalFetch = globalThis.fetch
    globalThis.fetch = fetchSpy

    try {
      const client = createWebClient({
        baseURL: 'http://localhost',
        credentials: 'include',
      })
      await client.get('/test')

      expect(fetchSpy).toHaveBeenCalled()
      const init = fetchSpy.mock.calls[0][1] as RequestInit
      expect(init.credentials).toBe('include')
    } finally {
      globalThis.fetch = originalFetch
    }
  })
})

describe('C6 — fetchMode: "no-cors" passed to fetch', () => {
  it('fetch receives mode: "no-cors"', async () => {
    const fetchSpy = vi.fn().mockResolvedValue(mockResponse())
    const originalFetch = globalThis.fetch
    globalThis.fetch = fetchSpy

    try {
      const client = createWebClient({
        baseURL: 'http://localhost',
        fetchMode: 'no-cors',
      })
      await client.get('/test')

      expect(fetchSpy).toHaveBeenCalled()
      const init = fetchSpy.mock.calls[0][1] as RequestInit
      expect(init.mode).toBe('no-cors')
    } finally {
      globalThis.fetch = originalFetch
    }
  })
})

describe('C7 — Default credentials is "same-origin"', () => {
  it('fetch receives credentials: "same-origin" by default', async () => {
    const fetchSpy = vi.fn().mockResolvedValue(mockResponse())
    const originalFetch = globalThis.fetch
    globalThis.fetch = fetchSpy

    try {
      const client = createWebClient({ baseURL: 'http://localhost' })
      await client.get('/test')

      expect(fetchSpy).toHaveBeenCalled()
      const init = fetchSpy.mock.calls[0][1] as RequestInit
      expect(init.credentials).toBe('same-origin')
    } finally {
      globalThis.fetch = originalFetch
    }
  })
})

describe('C8 — Default mode is "cors"', () => {
  it('fetch receives mode: "cors" by default', async () => {
    const fetchSpy = vi.fn().mockResolvedValue(mockResponse())
    const originalFetch = globalThis.fetch
    globalThis.fetch = fetchSpy

    try {
      const client = createWebClient({ baseURL: 'http://localhost' })
      await client.get('/test')

      expect(fetchSpy).toHaveBeenCalled()
      const init = fetchSpy.mock.calls[0][1] as RequestInit
      expect(init.mode).toBe('cors')
    } finally {
      globalThis.fetch = originalFetch
    }
  })
})

describe('C9 — Per-request credentials override', () => {
  it('request-level credentials override instance default', async () => {
    const fetchSpy = vi.fn().mockResolvedValue(mockResponse())
    const originalFetch = globalThis.fetch
    globalThis.fetch = fetchSpy

    try {
      const client = createWebClient({
        baseURL: 'http://localhost',
        credentials: 'omit',
      })
      await client.get('/test', { credentials: 'include' } as any)

      expect(fetchSpy).toHaveBeenCalled()
      const init = fetchSpy.mock.calls[0][1] as RequestInit
      expect(init.credentials).toBe('include')
    } finally {
      globalThis.fetch = originalFetch
    }
  })
})
