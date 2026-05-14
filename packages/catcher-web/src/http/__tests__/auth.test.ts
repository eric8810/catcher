import { describe, it, expect, vi } from 'vitest'
import { createWebClient } from '../client.js'

/**
 * Browser auth tests (AU5-AU7).
 * Mock globalThis.fetch and document.cookie since we're in Node.js.
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

describe('AU5 — XSRF cookie → header injection', () => {
  it('reads XSRF cookie and sends as header', async () => {
    const fetchSpy = vi.fn().mockResolvedValue(mockResponse())
    const originalFetch = globalThis.fetch
    globalThis.fetch = fetchSpy

    // Mock document.cookie
    const originalDocument = globalThis.document
    Object.defineProperty(globalThis, 'document', {
      value: { cookie: 'XSRF-TOKEN=test-xsrf-value' },
      writable: true,
      configurable: true,
    })

    try {
      const client = createWebClient({
        baseURL: 'http://localhost',
        xsrfCookieName: 'XSRF-TOKEN',
        xsrfHeaderName: 'X-XSRF-TOKEN',
      })
      await client.get('/test')

      expect(fetchSpy).toHaveBeenCalled()
      const init = fetchSpy.mock.calls[0][1] as RequestInit
      const headers = init.headers as Record<string, string>
      expect(headers['X-XSRF-TOKEN']).toBe('test-xsrf-value')
    } finally {
      globalThis.fetch = originalFetch
      if (originalDocument) {
        Object.defineProperty(globalThis, 'document', { value: originalDocument, writable: true, configurable: true })
      } else {
        delete (globalThis as any).document
      }
    }
  })
})

describe('AU6 — XSRF cookie not present → no header', () => {
  it('does not inject XSRF header when cookie is absent', async () => {
    const fetchSpy = vi.fn().mockResolvedValue(mockResponse())
    const originalFetch = globalThis.fetch
    globalThis.fetch = fetchSpy

    // Mock document.cookie without XSRF token
    const originalDocument = globalThis.document
    Object.defineProperty(globalThis, 'document', {
      value: { cookie: '' },
      writable: true,
      configurable: true,
    })

    try {
      const client = createWebClient({
        baseURL: 'http://localhost',
        xsrfCookieName: 'XSRF-TOKEN',
        xsrfHeaderName: 'X-XSRF-TOKEN',
      })
      await client.get('/test')

      expect(fetchSpy).toHaveBeenCalled()
      const init = fetchSpy.mock.calls[0][1] as RequestInit
      const headers = init.headers as Record<string, string>
      expect(headers['X-XSRF-TOKEN']).toBeUndefined()
    } finally {
      globalThis.fetch = originalFetch
      if (originalDocument) {
        Object.defineProperty(globalThis, 'document', { value: originalDocument, writable: true, configurable: true })
      } else {
        delete (globalThis as any).document
      }
    }
  })
})

describe('AU7 — Bearer Token auto-injection (browser)', () => {
  it('sends Authorization: Bearer header', async () => {
    const fetchSpy = vi.fn().mockResolvedValue(mockResponse())
    const originalFetch = globalThis.fetch
    globalThis.fetch = fetchSpy

    try {
      const client = createWebClient({
        baseURL: 'http://localhost',
        bearerToken: 'browser-token',
      })
      await client.get('/test')

      expect(fetchSpy).toHaveBeenCalled()
      const init = fetchSpy.mock.calls[0][1] as RequestInit
      const headers = init.headers as Record<string, string>
      expect(headers['Authorization']).toBe('Bearer browser-token')
    } finally {
      globalThis.fetch = originalFetch
    }
  })
})
