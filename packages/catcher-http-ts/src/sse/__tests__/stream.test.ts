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
        // 不 close，也不 enqueue 后续数据 → 模拟服务端停发
      }
    },
  })
  vi.spyOn(globalThis, 'fetch').mockResolvedValueOnce(
    mockResponse(stream, options?.status),
  )
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

describe('createSSEStream', () => {
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
  })

  // ── 2.3 行尾处理 ────────────────────────────────────────

  describe('行尾处理', () => {
    it('S8 \\r\\n 换行', async () => {
      mockSSEChunked(['data: A\r\n\r\n'])
      const stream = createSSEStream({ url: 'http://test/sse' })
      expect(await collectStream(stream)).toEqual(['data: A'])
    })

    it('S9 混合 \\n 和 \\r\\n', async () => {
      mockSSEChunked(['data: A\ndata: B\r\n'])
      const stream = createSSEStream({ url: 'http://test/sse' })
      expect(await collectStream(stream)).toEqual(['data: A', 'data: B'])
    })

    it('S10 最后一行无 \\n', async () => {
      mockSSEChunked(['data: end'])
      const stream = createSSEStream({ url: 'http://test/sse' })
      expect(await collectStream(stream)).toEqual(['data: end'])
    })
  })

  // ── 2.4 id: 和 retry: 提取 ──────────────────────────────

  describe('id: 提取', () => {
    it('S11 lastEventId 提取', async () => {
      mockSSEResponse(['id: msg_42', 'data: X'])
      const stream = createSSEStream({ url: 'http://test/sse' })
      await collectStream(stream)
      expect(stream.lastEventId).toBe('msg_42')
    })

    it('S12 多次 id 覆盖', async () => {
      mockSSEResponse(['id: first', 'data: A', '', 'id: second', 'data: B'])
      const stream = createSSEStream({ url: 'http://test/sse' })
      await collectStream(stream)
      expect(stream.lastEventId).toBe('second')
    })
  })

  // ── 2.5 错误处理 ────────────────────────────────────────

  describe('错误处理', () => {
    it('S13 HTTP 非 200 → throw', async () => {
      mockSSEResponse([], { status: 500 })
      const stream = createSSEStream({ url: 'http://test/sse' })
      await expect(collectStream(stream)).rejects.toThrow('HTTP 500')
    })

    it('S14 AbortSignal 中断 → 迭代器抛错', async () => {
      const encoder = new TextEncoder()
      let pulled = false
      const stream = new ReadableStream({
        pull(controller) {
          if (!pulled) {
            pulled = true
            controller.enqueue(encoder.encode('data: hello\n'))
          }
          // 不 close，模拟长连接
        },
      })
      vi.spyOn(globalThis, 'fetch').mockResolvedValueOnce(mockResponse(stream))

      const controller = new AbortController()
      const sseStream = createSSEStream({
        url: 'http://test/sse',
        signal: controller.signal,
      })

      const collected: string[] = []
      const iter = sseStream[Symbol.asyncIterator]()
      const first = await iter.next()
      expect(first.value).toBe('data: hello')

      // Abort after receiving first line
      controller.abort()

      // Next read should throw (Aborted propagates through readWithIdleTimeout)
      await expect(iter.next()).rejects.toThrow('Aborted')
    })
  })

  // ── 2.6 Idle Timeout ────────────────────────────────────

  describe('Idle Timeout', () => {
    it('S15 idle timeout 触发 → SSETimeoutError', async () => {
      mockSSEIdleHang()
      const stream = createSSEStream({ url: 'http://test/sse', timeout: 100 })
      await expect(collectStream(stream)).rejects.toThrow('SSE timeout after 100ms')
    })
  })

  // ── 2.7 event: 行原样通过 ───────────────────────────────

  describe('event: 行原样通过', () => {
    it('S16 event: 行原样输出', async () => {
      mockSSEResponse(['event: message_start', 'data: {"role":"assistant"}', ''])
      const stream = createSSEStream({ url: 'http://test/sse' })
      expect(await collectStream(stream)).toEqual([
        'event: message_start',
        'data: {"role":"assistant"}',
      ])
    })

    it('S17 多个 event: + data: 混合', async () => {
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

  // ── 2.8 只能迭代一次 ────────────────────────────────────

  describe('只能迭代一次', () => {
    it('S18 第二次迭代抛错', async () => {
      mockSSEResponse(['data: once'])
      const stream = createSSEStream({ url: 'http://test/sse' })
      await collectStream(stream)
      expect(() => stream[Symbol.asyncIterator]()).toThrow('can only be iterated once')
    })
  })
})
