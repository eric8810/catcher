import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { createWebClient } from '../client.js'

/**
 * Browser redirect tests (RD7-RD8).
 * Mock globalThis.fetch since we're running in Node.js.
 */

function mockResponse(status = 200, body: any = { ok: true }): Response {
  return {
    status,
    ok: status >= 200 && status < 300,
    headers: new Headers(),
    json: () => Promise.resolve(body),
    text: () => Promise.resolve(typeof body === 'string' ? body : JSON.stringify(body)),
    arrayBuffer: () => Promise.resolve(new ArrayBuffer(0)),
    body: null,
  } as unknown as Response
}

describe('RD7 — redirect: { follow: false } passes redirect: "manual"', () => {
  it('fetch receives redirect: "manual"', async () => {
    const fetchSpy = vi.fn().mockResolvedValue(mockResponse())
    const originalFetch = globalThis.fetch
    globalThis.fetch = fetchSpy

    try {
      const client = createWebClient({
        baseURL: 'http://localhost',
        redirect: { follow: false },
      })
      await client.get('/test', { validateStatus: () => true } as any)

      expect(fetchSpy).toHaveBeenCalled()
      const init = fetchSpy.mock.calls[0][1] as RequestInit
      expect(init.redirect).toBe('manual')
    } finally {
      globalThis.fetch = originalFetch
    }
  })
})

describe('RD8 — Default redirect is "follow"', () => {
  it('fetch receives redirect: "follow" by default', async () => {
    const fetchSpy = vi.fn().mockResolvedValue(mockResponse())
    const originalFetch = globalThis.fetch
    globalThis.fetch = fetchSpy

    try {
      const client = createWebClient({ baseURL: 'http://localhost' })
      await client.get('/test')

      expect(fetchSpy).toHaveBeenCalled()
      const init = fetchSpy.mock.calls[0][1] as RequestInit
      expect(init.redirect).toBe('follow')
    } finally {
      globalThis.fetch = originalFetch
    }
  })
})
