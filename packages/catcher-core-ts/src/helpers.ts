import type { CatcherErrorType } from './types.js'

// ── Cockatiel executor (shared by all packages) ──────────────────

/**
 * 创建与 Cockatiel 兼容的最小 ExecuteWrapper。
 * ExecuteWrapper 未从 'cockatiel' 包中直接导出，因此需要手动构造。
 */
export function createExecutor(): any {
  const self: any = {
    onSuccess: {
      addListener: () => {},
      removeListener: () => {},
      get size() { return 0 },
    },
    onFailure: {
      addListener: () => {},
      removeListener: () => {},
      get size() { return 0 },
    },
    clone() {
      return createExecutor()
    },
    async invoke(fn: (...args: any[]) => any, ...args: any[]) {
      try {
        const value = await fn(...args)
        return { success: value }
      } catch (error) {
        return { error }
      }
    },
  }
  return self
}

// ── 异步工具 ──────────────────────────────────────────────────────

/**
 * 休眠指定的毫秒数。
 */
export function sleep(ms: number): Promise<void> {
  return new Promise(resolve => setTimeout(resolve, ms))
}

/**
 * 使用指数退避和抖动计算重试延迟。
 */
export function calculateDelay(attempt: number, initialDelay: number, maxDelay: number, multiplier: number): number {
  const base = initialDelay * Math.pow(multiplier, attempt - 1)
  const capped = Math.min(base, maxDelay)
  const jitter = capped * 0.25 * (Math.random() * 2 - 1)
  return Math.max(0, Math.round(capped + jitter))
}

// ── 错误处理 ──────────────────────────────────────────────────────

/** 需要脱敏的敏感请求头集合。 */
export const SENSITIVE_HEADERS = new Set(['authorization', 'cookie', 'set-cookie', 'proxy-authorization'])

/**
 * 将浏览器 fetch 错误分类为 CatcherErrorType。
 */
export function classifyFetchError(error: any): CatcherErrorType {
  if (error.name === 'AbortError' || error.code === 'ECANCELED') return 'cancelled'
  if (error.name === 'TypeError' && error.message?.includes('Failed to fetch')) return 'connection'
  if (error.code === 'HTTP_5XX') return 'http'
  if (error.response) return 'http'
  return 'unknown'
}

/**
 * 对请求头进行脱敏处理，便于安全序列化。
 */
export function redactHeaders(headers: Record<string, string>): Record<string, string> {
  const safe: Record<string, string> = {}
  for (const [key, value] of Object.entries(headers)) {
    safe[key] = SENSITIVE_HEADERS.has(key.toLowerCase()) ? '[REDACTED]' : value
  }
  return safe
}

// ── SSE 流核心 ──

import type { SSEStreamOptions, SSEStream, SSETimeoutError } from './types.js'
import { routeLine } from './sse-router.js'

export class SSETimeoutErrorImpl extends Error implements SSETimeoutError {
  readonly type = 'SSE_TIMEOUT' as const
  constructor(timeout: number) {
    super(`SSE timeout after ${timeout}ms`)
    this.name = 'SSETimeoutError'
  }
}

type ReadChunkFn = (
  reader: ReadableStreamDefaultReader<Uint8Array>,
  timeoutMs: number,
  signal?: AbortSignal,
) => Promise<{ done: boolean; value?: Uint8Array }>

/** 创建平台无关的 SSE 流核心，readChunk 由平台提供（Node.js 用 Error，浏览器用 DOMException）。 */
export function createSSEStreamCore(options: SSEStreamOptions, readChunk: ReadChunkFn): SSEStream {
  let lastEventId = ''
  let reconnectDelay = 0
  let iterated = false

  const stream: SSEStream = {
    get lastEventId() { return lastEventId },

    [Symbol.asyncIterator]() {
      if (iterated) throw new Error('SSEStream can only be iterated once')
      iterated = true

      return (async function* () {
        const { url, method = 'GET', headers: baseHeaders = {}, body, timeout = 30_000, signal } = options
        const headers: Record<string, string> = { ...baseHeaders }
        if (body !== undefined && !headers['Content-Type'] && !headers['content-type']) {
          headers['Content-Type'] = 'application/json'
        }
        const init: RequestInit = {
          method, headers,
          body: body !== undefined ? (typeof body === 'string' ? body : JSON.stringify(body)) : undefined,
        }
        const controller = new AbortController()
        const timeoutId = setTimeout(() => controller.abort(), timeout)
        const onUserAbort = () => controller.abort()
        signal?.addEventListener('abort', onUserAbort, { once: true })
        init.signal = controller.signal

        let response: Response
        try {
          response = await fetch(url, init)
        } catch (err: any) {
          if (signal?.aborted) throw err
          if (err.name === 'AbortError') throw new SSETimeoutErrorImpl(timeout)
          throw err
        } finally {
          clearTimeout(timeoutId)
          signal?.removeEventListener('abort', onUserAbort)
        }
        if (!response.ok) throw new Error(`SSE connection failed: HTTP ${response.status}`)
        if (!response.body) throw new Error('SSE: response body is null')

        const reader = response.body.getReader()
        const decoder = new TextDecoder()
        let buffer = ''
        try {
          while (true) {
            const { done, value } = await readChunk(reader, timeout, signal)
            if (done) break
            buffer += decoder.decode(value, { stream: true })
            let newlineIdx: number
            while ((newlineIdx = buffer.indexOf('\n')) !== -1) {
              let line = buffer.slice(0, newlineIdx)
              buffer = buffer.slice(newlineIdx + 1)
              if (line.endsWith('\r')) line = line.slice(0, -1)
              const action = routeLine(line)
              switch (action.kind) {
                case 'yield': yield action.line; break
                case 'setLastEventId': lastEventId = action.id; break
                case 'setRetry': reconnectDelay = action.ms; break
                case 'silent': break
              }
            }
          }
          if (buffer.length > 0) {
            let line = buffer
            if (line.endsWith('\r')) line = line.slice(0, -1)
            const action = routeLine(line)
            if (action.kind === 'yield') yield action.line
            else if (action.kind === 'setLastEventId') lastEventId = action.id
            else if (action.kind === 'setRetry') reconnectDelay = action.ms
          }
        } finally {
          reader.releaseLock()
        }
      })()
    },
  }
  return stream
}

// ── SSE 客户端核心 ──

import type { SSEClientOptions, SSEClient } from './types.js'
import { PushQueue } from './sse-router.js'

type ReadyState = 'CONNECTING' | 'OPEN' | 'CLOSED'
export interface SseConnectOnceCtx {
  lastEventId: string; setLastEventId: (id: string) => void;
  setReconnectDelay: (ms: number) => void;
  closed: () => boolean; setReadyState: (s: ReadyState) => void;
  queue: PushQueue<string>;
}
export type ConnectOnceFn = (ctx: SseConnectOnceCtx) => Promise<void>

/** 创建平台无关的 SSE 客户端核心。connectOnce 由平台提供（含 readWithIdleTimeout 差异）。 */
export function createSSEClientCore(options: SSEClientOptions, connectOnce: ConnectOnceFn): SSEClient {
  let lastEventId = ''
  let readyState: ReadyState = 'CONNECTING'
  let closed = false
  let reconnectDelay = 0
  let retryCount = 0

  const { reconnect: reconnectConfig } = options
  const reconnectEnabled = reconnectConfig?.enabled !== false
  const maxRetries = reconnectConfig?.maxRetries ?? Infinity

  const queue = new PushQueue<string>()
  const ctx = {
    get lastEventId() { return lastEventId },
    setLastEventId: (id: string) => { lastEventId = id },
    setReconnectDelay: (ms: number) => { reconnectDelay = ms },
    closed: () => closed,
    setReadyState: (s: ReadyState) => { readyState = s },
    queue,
  }

  async function runLoop() {
    while (!closed) {
      try {
        await connectOnce(ctx)
        if (closed) break
        if (!reconnectEnabled) { queue.finish(); break }
        retryCount++
        if (retryCount > maxRetries) { queue.finish(); break }
        const delay = reconnectDelay > 0
          ? Math.round(reconnectDelay + reconnectDelay * 0.25 * (Math.random() * 2 - 1))
          : calculateDelay(retryCount, reconnectConfig?.initialDelay ?? 1000, reconnectConfig?.maxDelay ?? 30000, reconnectConfig?.backoffMultiplier ?? 2)
        await sleep(delay)
      } catch (err) {
        if (closed) break
        retryCount++
        if (retryCount > maxRetries) { queue.fail(err); break }
        const delay = calculateDelay(retryCount, reconnectConfig?.initialDelay ?? 1000, reconnectConfig?.maxDelay ?? 30000, reconnectConfig?.backoffMultiplier ?? 2)
        await sleep(delay)
      }
    }
    readyState = 'CLOSED'
    queue.finish()
  }

  runLoop().catch(err => queue.fail(err))

  return {
    get readyState() { return readyState },
    get lastEventId() { return lastEventId },
    close() { closed = true; readyState = 'CLOSED'; queue.finish() },
    [Symbol.asyncIterator]() { return queue[Symbol.asyncIterator]() },
  }
}
