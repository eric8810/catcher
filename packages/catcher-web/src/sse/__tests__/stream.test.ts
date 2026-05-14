/**
 * createSSEStream 集成测试 — catcher-web 浏览器版
 *
 * 验证浏览器端 SSE 流消费行为（使用 ReadableStream.getReader()）。
 * Mock 方式：vi.spyOn(globalThis, 'fetch')，与 catcher-http-ts 版相同。
 *
 * 用例编号 S1-S23，与设计文档 docs/arch-ts/10-sse.md 一一对应。
 */
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { createSSEStream } from '../stream.js'

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

function mockSSEChunked(chunks: string[], options?: { status?: number }) {
  const encoder = new TextEncoder()
  let i = 0
  const stream = new ReadableStream({
    pull(controller) {
      if (i < chunks.length) {
        controller.enqueue(encoder.encode(chunks[i++]))
      } else {
        controller.close()
      }
    },
  })
  vi.spyOn(globalThis, 'fetch').mockResolvedValueOnce(
    mockResponse(stream, options?.status),
  )
}

/** 模拟空闲超时：发一行后永远不 pull */
function mockSSEIdleHang(options?: { status?: number }) {
  const encoder = new TextEncoder()
  let pulled = false
  const stream = new ReadableStream({
    pull(controller) {
      if (!pulled) {
        pulled = true
        controller.enqueue(encoder.encode('data: first\n'))
      }
    },
  })
  vi.spyOn(globalThis, 'fetch').mockResolvedValueOnce(
    mockResponse(stream, options?.status),
  )
}

function setupFetchSpy(lines: string[], options?: { status?: number }) {
  const encoder = new TextEncoder()
  const stream = new ReadableStream({
    start(controller) {
      for (const line of lines) {
        controller.enqueue(encoder.encode(line + '\n'))
      }
      controller.close()
    },
  })
  const fetchSpy = vi.spyOn(globalThis, 'fetch').mockResolvedValueOnce(
    mockResponse(stream, options?.status),
  )
  return { fetchSpy }
}

async function collectStream(stream: AsyncIterable<string>): Promise<string[]> {
  const result: string[] = []
  for await (const line of stream) {
    result.push(line)
  }
  return result
}

// ── Tests ──────────────────────────────────────────────────

beforeEach(() => { vi.restoreAllMocks() })
afterEach(() => { vi.restoreAllMocks() })

describe('createSSEStream (catcher-web)', () => {
  // ── 2.1 基础流式消费 ────────────────────────────────────

  describe('基础流式消费', () => {
    it('S1 完整 SSE 事件', async () => {
      mockSSEResponse(['data: Hello', '', 'data: World', ''])
      const stream = createSSEStream({ url: 'http://test/sse' })
      expect(await collectStream(stream)).toEqual(['data: Hello', 'data: World'])
    })

    it('S2 混合控制行和内容行', async () => {
      mockSSEResponse([': comment', 'data: A', 'id: 1', '', 'data: B'])
      const stream = createSSEStream({ url: 'http://test/sse' })
      expect(await collectStream(stream)).toEqual(['data: A', 'data: B'])
      expect(stream.lastEventId).toBe('1')
    })

    it('S3 心跳行被过滤', async () => {
      mockSSEResponse([': ping', ': pong', 'data: real', ''])
      const stream = createSSEStream({ url: 'http://test/sse' })
      expect(await collectStream(stream)).toEqual(['data: real'])
    })

    it('S4 空行被过滤', async () => {
      mockSSEResponse(['data: X', '', '', 'data: Y', ''])
      const stream = createSSEStream({ url: 'http://test/sse' })
      expect(await collectStream(stream)).toEqual(['data: X', 'data: Y'])
    })
  })

  // ── 2.2 Chunk 分片处理 ──────────────────────────────────

  describe('Chunk 分片处理', () => {
    it('S5 跨 chunk 的行 — 无半行', async () => {
      mockSSEChunked(['data: Hel', 'lo\n'])
      const stream = createSSEStream({ url: 'http://test/sse' })
      expect(await collectStream(stream)).toEqual(['data: Hello'])
    })

    it('S6 单 chunk 多行', async () => {
      mockSSEChunked(['data: A\ndata: B\n'])
      const stream = createSSEStream({ url: 'http://test/sse' })
      expect(await collectStream(stream)).toEqual(['data: A', 'data: B'])
    })

    it('S7 空 chunk + 数据 chunk', async () => {
      mockSSEChunked(['', 'data: X\n'])
      const stream = createSSEStream({ url: 'http://test/sse' })
      expect(await collectStream(stream)).toEqual(['data: X'])
    })

    it('S8 UTF-8 多字节跨 chunk — 无乱码', async () => {
      const prefix = new TextEncoder().encode('data: Héll')
      const chunk1 = new Uint8Array([...prefix, 0xc3])
      const chunk2 = new Uint8Array([0xa9, ...new TextEncoder().encode('\n')])

      const stream = new ReadableStream({
        start(controller) {
          controller.enqueue(chunk1)
          controller.enqueue(chunk2)
          controller.close()
        },
      })
      vi.spyOn(globalThis, 'fetch').mockResolvedValueOnce(mockResponse(stream))

      const sse = createSSEStream({ url: 'http://test/sse' })
      expect(await collectStream(sse)).toEqual(['data: Héllé'])
    })
  })

  // ── 2.3 行尾处理 ────────────────────────────────────────

  describe('行尾处理', () => {
    it('S9 \\r\\n 换行', async () => {
      mockSSEChunked(['data: A\r\n\r\n'])
      const stream = createSSEStream({ url: 'http://test/sse' })
      expect(await collectStream(stream)).toEqual(['data: A'])
    })

    it('S10 混合 \\n 和 \\r\\n', async () => {
      mockSSEChunked(['data: A\ndata: B\r\n'])
      const stream = createSSEStream({ url: 'http://test/sse' })
      expect(await collectStream(stream)).toEqual(['data: A', 'data: B'])
    })

    it('S11 最后一行无 \\n', async () => {
      mockSSEChunked(['data: end'])
      const stream = createSSEStream({ url: 'http://test/sse' })
      expect(await collectStream(stream)).toEqual(['data: end'])
    })
  })

  // ── 2.4 id: 和 retry: 提取 ──────────────────────────────

  describe('id: 提取', () => {
    it('S12 lastEventId 提取', async () => {
      mockSSEResponse(['id: msg_42', 'data: X'])
      const stream = createSSEStream({ url: 'http://test/sse' })
      await collectStream(stream)
      expect(stream.lastEventId).toBe('msg_42')
    })

    it('S13 多次 id 覆盖', async () => {
      mockSSEResponse(['id: first', 'data: A', '', 'id: second', 'data: B'])
      const stream = createSSEStream({ url: 'http://test/sse' })
      await collectStream(stream)
      expect(stream.lastEventId).toBe('second')
    })
  })

  // ── 2.5 错误处理 ────────────────────────────────────────

  describe('错误处理', () => {
    it('S14 HTTP 非 200 → throw', async () => {
      mockSSEResponse([], { status: 500 })
      const stream = createSSEStream({ url: 'http://test/sse' })
      await expect(collectStream(stream)).rejects.toThrow('HTTP 500')
    })

    it('S15 AbortSignal 中断 → 迭代器抛错', async () => {
      const encoder = new TextEncoder()
      let pulled = false
      const stream = new ReadableStream({
        pull(controller) {
          if (!pulled) {
            pulled = true
            controller.enqueue(encoder.encode('data: hello\n'))
          }
        },
      })
      vi.spyOn(globalThis, 'fetch').mockResolvedValueOnce(mockResponse(stream))

      const controller = new AbortController()
      const sseStream = createSSEStream({
        url: 'http://test/sse',
        signal: controller.signal,
      })

      const iter = sseStream[Symbol.asyncIterator]()
      const first = await iter.next()
      expect(first.value).toBe('data: hello')

      controller.abort()
      await expect(iter.next()).rejects.toThrow('Aborted')
    })
  })

  // ── 2.6 Idle Timeout ────────────────────────────────────

  describe('Idle Timeout', () => {
    it('S16 idle timeout 触发 → SSETimeoutError', async () => {
      mockSSEIdleHang()
      const stream = createSSEStream({ url: 'http://test/sse', timeout: 100 })
      await expect(collectStream(stream)).rejects.toThrow('SSE timeout after 100ms')
    })
  })

  // ── 2.7 event: 行原样通过 ───────────────────────────────

  describe('event: 行原样通过', () => {
    it('S17 event: 行原样输出', async () => {
      mockSSEResponse(['event: message_start', 'data: {"role":"assistant"}', ''])
      const stream = createSSEStream({ url: 'http://test/sse' })
      expect(await collectStream(stream)).toEqual([
        'event: message_start',
        'data: {"role":"assistant"}',
      ])
    })

    it('S18 多个 event: + data: 混合', async () => {
      mockSSEResponse([
        'event: ping', 'data: ok', '',
        'event: message', 'data: hi', '',
      ])
      const stream = createSSEStream({ url: 'http://test/sse' })
      expect(await collectStream(stream)).toEqual([
        'event: ping', 'data: ok',
        'event: message', 'data: hi',
      ])
    })
  })

  // ── 2.8 POST / headers 验证 ─────────────────────────────

  describe('POST / headers 验证', () => {
    it('S19 POST + JSON body — fetch 请求构造正确', async () => {
      const { fetchSpy } = setupFetchSpy(['data: ok', ''])

      const body = { model: 'gpt-4', messages: [{ role: 'user', content: 'hi' }] }
      const stream = createSSEStream({ url: 'http://test/sse', method: 'POST', body })
      await collectStream(stream)

      expect(fetchSpy).toHaveBeenCalledTimes(1)
      const [url, init] = fetchSpy.mock.calls[0]
      expect(url).toBe('http://test/sse')
      expect((init as RequestInit).method).toBe('POST')
      expect((init as RequestInit).body).toBe(JSON.stringify(body))
      const headers = (init as RequestInit).headers as Record<string, string>
      expect(headers['Content-Type']).toBe('application/json')
    })

    it('S20 自定义 headers 透传', async () => {
      const { fetchSpy } = setupFetchSpy(['data: ok', ''])

      const stream = createSSEStream({
        url: 'http://test/sse',
        headers: { Authorization: 'Bearer sk-xxx' },
      })
      await collectStream(stream)

      const init = fetchSpy.mock.calls[0][1] as RequestInit
      const headers = init.headers as Record<string, string>
      expect(headers['Authorization']).toBe('Bearer sk-xxx')
    })

    it('S21 POST + 已有 Content-Type 不覆盖', async () => {
      const { fetchSpy } = setupFetchSpy(['data: ok', ''])

      const stream = createSSEStream({
        url: 'http://test/sse',
        method: 'POST',
        body: 'raw text',
        headers: { 'Content-Type': 'text/plain' },
      })
      await collectStream(stream)

      const init = fetchSpy.mock.calls[0][1] as RequestInit
      const headers = init.headers as Record<string, string>
      expect(headers['Content-Type']).toBe('text/plain')
    })

    it('S22 string body 不 JSON.stringify', async () => {
      const { fetchSpy } = setupFetchSpy(['data: ok', ''])

      const stream = createSSEStream({
        url: 'http://test/sse',
        method: 'POST',
        body: 'raw string',
      })
      await collectStream(stream)

      const init = fetchSpy.mock.calls[0][1] as RequestInit
      expect(init.body).toBe('raw string')
    })
  })

  // ── 2.9 只能迭代一次 ────────────────────────────────────

  describe('只能迭代一次', () => {
    it('S23 第二次迭代抛错', async () => {
      mockSSEResponse(['data: once'])
      const stream = createSSEStream({ url: 'http://test/sse' })
      await collectStream(stream)
      expect(() => stream[Symbol.asyncIterator]()).toThrow('can only be iterated once')
    })
  })
})
