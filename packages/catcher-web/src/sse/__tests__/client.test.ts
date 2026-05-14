/**
 * createSSEClient 集成测试 — catcher-web 浏览器版
 *
 * 验证浏览器端 SSE 长连接 + 自动重连行为。
 * Mock 方式：vi.spyOn(globalThis, 'fetch')，与 catcher-http-ts 版相同。
 *
 * 用例编号 C1-C11，与设计文档 docs/arch-ts/10-sse.md 一一对应。
 */
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { createSSEClient } from '../client.js'

// ── Mock 工具函数 ──────────────────────────────────────────

function mockResponse(stream: ReadableStream, status = 200) {
  return {
    ok: status >= 200 && status < 300,
    status,
    headers: new Headers({ 'Content-Type': 'text/event-stream' }),
    body: stream,
  } as Response
}

function mockSSEResponse(lines: string[], options?: { status?: number }) {
  const encoder = new TextEncoder()
  const stream = new ReadableStream({
    start(controller) {
      for (const line of lines) {
        controller.enqueue(encoder.encode(line + '\n'))
      }
      controller.close()
    },
  })
  vi.spyOn(globalThis, 'fetch').mockResolvedValueOnce(
    mockResponse(stream, options?.status),
  )
}

async function collectUntil(client: AsyncIterable<string>, count: number, timeoutMs = 5000): Promise<string[]> {
  const result: string[] = []
  const deadline = Date.now() + timeoutMs
  for await (const line of client) {
    result.push(line)
    if (result.length >= count) break
    if (Date.now() > deadline) break
  }
  return result
}

beforeEach(() => { vi.restoreAllMocks() })
afterEach(() => { vi.restoreAllMocks() })

describe('createSSEClient (catcher-web)', () => {
  // ── 3.1 基础连接和消费 ──────────────────────────────────

  describe('基础连接和消费', () => {
    it('C1 连接并消费内容行', async () => {
      mockSSEResponse(['data: Hello', '', 'data: World', ''])
      const client = createSSEClient({
        url: 'http://test/sse',
        reconnect: { enabled: false },
      })
      const lines = await collectUntil(client, 2)
      expect(lines).toEqual(['data: Hello', 'data: World'])
      client.close()
    })

    it('C2 readyState 变化', async () => {
      const encoder = new TextEncoder()
      let pulled = false
      const stream = new ReadableStream({
        pull(controller) {
          if (!pulled) {
            pulled = true
            controller.enqueue(encoder.encode('data: hi\n'))
          }
        },
      })
      vi.spyOn(globalThis, 'fetch').mockResolvedValueOnce(mockResponse(stream))

      const client = createSSEClient({
        url: 'http://test/sse',
        reconnect: { enabled: false },
      })

      expect(client.readyState).toBe('CONNECTING')

      const lines = await collectUntil(client, 1)
      expect(lines).toEqual(['data: hi'])
      expect(client.readyState).toBe('OPEN')

      client.close()
      expect(client.readyState).toBe('CLOSED')
    })

    it('C3 close() 停止迭代', async () => {
      const encoder = new TextEncoder()
      let pulled = false
      const stream = new ReadableStream({
        pull(controller) {
          if (!pulled) {
            pulled = true
            controller.enqueue(encoder.encode('data: ongoing\n'))
          }
        },
      })
      vi.spyOn(globalThis, 'fetch').mockResolvedValueOnce(mockResponse(stream))

      const client = createSSEClient({
        url: 'http://test/sse',
        reconnect: { enabled: false },
      })

      const lines = await collectUntil(client, 1)
      expect(lines).toEqual(['data: ongoing'])

      client.close()

      const more: string[] = []
      for await (const line of client) {
        more.push(line)
        break
      }
      expect(more).toEqual([])
    })

    it('C4 lastEventId 提取', async () => {
      mockSSEResponse(['id: abc123', 'data: payload', ''])
      const client = createSSEClient({
        url: 'http://test/sse',
        reconnect: { enabled: false },
      })
      await collectUntil(client, 1)
      expect(client.lastEventId).toBe('abc123')
      client.close()
    })
  })

  // ── 3.2 自动重连 ────────────────────────────────────────

  describe('自动重连', () => {
    it('C5 流结束后自动重连 — 两次内容都收到', async () => {
      const encoder = new TextEncoder()

      const stream1 = new ReadableStream({
        start(controller) {
          controller.enqueue(encoder.encode('data: first\n'))
          controller.close()
        },
      })
      const stream2 = new ReadableStream({
        start(controller) {
          controller.enqueue(encoder.encode('data: second\n'))
          controller.close()
        },
      })

      const fetchSpy = vi.spyOn(globalThis, 'fetch')
      fetchSpy.mockImplementationOnce(() => Promise.resolve(mockResponse(stream1)))

      const client = createSSEClient({
        url: 'http://test/sse',
        reconnect: { enabled: true, initialDelay: 10, maxDelay: 50, maxRetries: 1 },
      })

      const first = await collectUntil(client, 1, 2000)
      expect(first).toEqual(['data: first'])

      fetchSpy.mockImplementationOnce(() => Promise.resolve(mockResponse(stream2)))

      const iter = client[Symbol.asyncIterator]()
      const second = await iter.next()
      expect(second.value).toBe('data: second')
      client.close()
    })

    it('C6 重连携带 Last-Event-ID', async () => {
      const encoder = new TextEncoder()

      const stream1 = new ReadableStream({
        start(controller) {
          controller.enqueue(encoder.encode('id: reconnect-me\ndata: A\n'))
          controller.close()
        },
      })

      const fetchSpy = vi.spyOn(globalThis, 'fetch')
      fetchSpy.mockImplementationOnce(() => Promise.resolve(mockResponse(stream1)))

      const client = createSSEClient({
        url: 'http://test/sse',
        reconnect: { enabled: true, initialDelay: 10, maxDelay: 50, maxRetries: 1 },
      })

      const first = await collectUntil(client, 1, 2000)
      expect(first).toEqual(['data: A'])

      const stream2 = new ReadableStream({
        start(controller) {
          controller.enqueue(encoder.encode('data: B\n'))
          controller.close()
        },
      })
      fetchSpy.mockImplementationOnce(() => Promise.resolve(mockResponse(stream2)))

      const iter = client[Symbol.asyncIterator]()
      const second = await iter.next()
      expect(second.value).toBe('data: B')

      expect(fetchSpy).toHaveBeenCalledTimes(2)
      const secondCall = fetchSpy.mock.calls[1]
      const init = secondCall[1] as RequestInit
      const headers = init.headers as Record<string, string>
      expect(headers['Last-Event-ID']).toBe('reconnect-me')

      client.close()
    })

    it('C7 网络错误后重连', async () => {
      const fetchSpy = vi.spyOn(globalThis, 'fetch')
      fetchSpy.mockRejectedValueOnce(new Error('Network error'))

      const encoder = new TextEncoder()
      const stream2 = new ReadableStream({
        start(controller) {
          controller.enqueue(encoder.encode('data: recovered\n'))
          controller.close()
        },
      })

      const client = createSSEClient({
        url: 'http://test/sse',
        reconnect: { enabled: true, initialDelay: 10, maxDelay: 50, maxRetries: 1 },
      })

      fetchSpy.mockImplementationOnce(() => Promise.resolve(mockResponse(stream2)))

      const lines = await collectUntil(client, 1, 3000)
      expect(lines).toEqual(['data: recovered'])
      client.close()
    })

    it('C8 达到 maxRetries 停止', async () => {
      const fetchSpy = vi.spyOn(globalThis, 'fetch')
      fetchSpy.mockRejectedValue(new Error('Always fail'))

      const client = createSSEClient({
        url: 'http://test/sse',
        reconnect: { enabled: true, maxRetries: 1, initialDelay: 10, maxDelay: 50 },
      })

      const lines: string[] = []
      for await (const line of client) {
        lines.push(line)
      }
      expect(lines).toEqual([])
      expect(fetchSpy.mock.calls.length).toBeLessThanOrEqual(3)
    })

    it('C9 enabled: false 不重连', async () => {
      const encoder = new TextEncoder()
      const stream = new ReadableStream({
        start(controller) {
          controller.enqueue(encoder.encode('data: once\n'))
          controller.close()
        },
      })

      const fetchSpy = vi.spyOn(globalThis, 'fetch')
      fetchSpy.mockImplementationOnce(() => Promise.resolve(mockResponse(stream)))

      const client = createSSEClient({
        url: 'http://test/sse',
        reconnect: { enabled: false },
      })

      const lines = await collectUntil(client, 1, 2000)
      expect(lines).toEqual(['data: once'])

      await new Promise(r => setTimeout(r, 100))
      expect(fetchSpy).toHaveBeenCalledTimes(1)
      client.close()
    })
  })

  // ── 3.3 204 停止重连 ────────────────────────────────────

  describe('204 停止重连', () => {
    it('C10 204 停止重连', async () => {
      const fetchSpy = vi.spyOn(globalThis, 'fetch')

      const emptyStream = new ReadableStream({ start(c) { c.close() } })
      fetchSpy.mockImplementationOnce(() => Promise.resolve({
        ok: false,
        status: 204,
        headers: new Headers(),
        body: emptyStream,
      } as Response))

      const client = createSSEClient({
        url: 'http://test/sse',
        reconnect: { enabled: true, initialDelay: 10 },
      })

      const lines: string[] = []
      for await (const line of client) {
        lines.push(line)
      }
      expect(lines).toEqual([])

      await new Promise(r => setTimeout(r, 100))
      expect(fetchSpy).toHaveBeenCalledTimes(1)
    })
  })

  // ── 3.4 熔断器 ──────────────────────────────────────────

  describe('熔断器', () => {
    it('C11 circuitBreaker 集成 — 连续失败后停止', async () => {
      const fetchSpy = vi.spyOn(globalThis, 'fetch')
      fetchSpy.mockRejectedValue(new Error('Connection refused'))

      const client = createSSEClient({
        url: 'http://test/sse',
        reconnect: { enabled: true, maxRetries: 10, initialDelay: 10, maxDelay: 50 },
        circuitBreaker: { failureThreshold: 2, resetTimeout: 10_000 },
      })

      const lines: string[] = []
      for await (const line of client) {
        lines.push(line)
      }
      expect(lines).toEqual([])
      expect(fetchSpy.mock.calls.length).toBeLessThan(10)
    })
  })
})
