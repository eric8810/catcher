import { CircuitBreakerPolicy, ConsecutiveBreaker } from 'cockatiel'
import type { ExecuteWrapper } from 'cockatiel/dist/common/Executor.js'
import type { SSEClientOptions, SSEClient } from '@eric8810/catcher-core'
import { routeLine } from './router.js'

// ── Cockatiel executor (same pattern as http/client.ts) ────────

function createExecutor(): ExecuteWrapper {
  const self: any = {
    onSuccess: { addListener: () => {}, removeListener: () => {}, get size() { return 0 } },
    onFailure: { addListener: () => {}, removeListener: () => {}, get size() { return 0 } },
    clone() { return createExecutor() },
    async invoke(fn: (...args: any[]) => any, ...args: any[]) {
      try { return { success: await fn(...args) } }
      catch (error) { return { error } }
    },
  }
  return self
}

// ── Push queue: background task pushes lines, consumer iterates ──

class PushQueue<T> {
  private items: T[] = []
  private waiting: Array<(result: IteratorResult<T>) => void> = []
  private _done = false
  private _error: any = null

  push(item: T) {
    if (this._done) return
    if (this.waiting.length > 0) {
      this.waiting.shift()!({ value: item, done: false })
    } else {
      this.items.push(item)
    }
  }

  finish() {
    this._done = true
    for (const resolve of this.waiting) resolve({ value: undefined, done: true })
    this.waiting = []
  }

  fail(error: any) {
    this._error = error
    this.finish()
  }

  get isDone() { return this._done }

  [Symbol.asyncIterator](): AsyncIterator<T> & { return(): Promise<IteratorResult<T>> } {
    return {
      next: (): Promise<IteratorResult<T>> => {
        if (this.items.length > 0) {
          return Promise.resolve({ value: this.items.shift()!, done: false })
        }
        if (this._done) {
          return this._error
            ? Promise.reject(this._error)
            : Promise.resolve({ value: undefined, done: true })
        }
        return new Promise<IteratorResult<T>>(resolve => { this.waiting.push(resolve) })
      },
      return: (): Promise<IteratorResult<T>> => {
        return Promise.resolve({ value: undefined, done: true })
      },
    }
  }
}

// ── Helpers ────────────────────────────────────────────────────

function sleep(ms: number): Promise<void> {
  return new Promise(resolve => setTimeout(resolve, ms))
}

function calculateDelay(attempt: number, initialDelay: number, maxDelay: number, multiplier: number): number {
  const base = initialDelay * Math.pow(multiplier, attempt - 1)
  const capped = Math.min(base, maxDelay)
  const jitter = capped * 0.25 * (Math.random() * 2 - 1)
  return Math.max(0, Math.round(capped + jitter))
}

type ReadyState = 'CONNECTING' | 'OPEN' | 'CLOSED'

// ── createSSEClient ────────────────────────────────────────────

/**
 * Create a long-lived SSE client with auto-reconnect.
 *
 * Connects via fetch, yields content lines, and automatically
 * reconnects with exponential backoff + Last-Event-ID on disconnect.
 */
export function createSSEClient(options: SSEClientOptions): SSEClient {
  let lastEventId = ''
  let readyState: ReadyState = 'CONNECTING'
  let closed = false
  let reconnectDelay = 0

  const {
    reconnect: reconnectConfig,
    circuitBreaker: cbConfig,
    ...streamOptions
  } = options

  const reconnectEnabled = reconnectConfig?.enabled !== false
  const maxRetries = reconnectConfig?.maxRetries ?? Infinity
  const initialDelay = reconnectConfig?.initialDelay ?? 1000
  const maxDelay = reconnectConfig?.maxDelay ?? 30_000
  const backoffMultiplier = reconnectConfig?.backoffMultiplier ?? 2

  // Circuit breaker
  let breaker: CircuitBreakerPolicy | null = null
  if (cbConfig) {
    breaker = new CircuitBreakerPolicy(
      {
        halfOpenAfter: cbConfig.resetTimeout,
        breaker: new ConsecutiveBreaker(cbConfig.failureThreshold),
      },
      createExecutor(),
    )
  }

  const queue = new PushQueue<string>()

  // ── Single connection attempt ──────────────────────────────

  async function connectOnce(): Promise<void> {
    const {
      url,
      method = 'GET',
      headers: baseHeaders = {},
      body,
      timeout = 30_000,
      signal,
    } = streamOptions

    const headers: Record<string, string> = { ...baseHeaders }
    if (lastEventId) headers['Last-Event-ID'] = lastEventId
    if (body !== undefined && !headers['Content-Type'] && !headers['content-type']) {
      headers['Content-Type'] = 'application/json'
    }

    const init: RequestInit = {
      method,
      headers,
      body: body !== undefined
        ? (typeof body === 'string' ? body : JSON.stringify(body))
        : undefined,
    }

    // Timeout for the initial connection
    const controller = new AbortController()
    const timeoutId = setTimeout(() => controller.abort(), timeout)
    const onUserAbort = () => controller.abort()
    signal?.addEventListener('abort', onUserAbort, { once: true })
    init.signal = controller.signal

    let response: Response
    try {
      response = await fetch(url, init)
    } finally {
      clearTimeout(timeoutId)
      signal?.removeEventListener('abort', onUserAbort)
    }

    // 204 = server says stop reconnecting (SSE spec)
    if (response.status === 204) {
      closed = true
      return
    }

    if (!response.ok) {
      throw new Error(`SSE connection failed: HTTP ${response.status}`)
    }
    if (!response.body) {
      throw new Error('SSE: response body is null')
    }

    readyState = 'OPEN'

    const reader = response.body.getReader()
    const decoder = new TextDecoder()
    let buffer = ''

    try {
      while (true) {
        const { done, value } = await reader.read()
        if (done) break
        if (closed) break

        buffer += decoder.decode(value, { stream: true })

        let newlineIdx: number
        while ((newlineIdx = buffer.indexOf('\n')) !== -1) {
          let line = buffer.slice(0, newlineIdx)
          buffer = buffer.slice(newlineIdx + 1)
          if (line.endsWith('\r')) line = line.slice(0, -1)

          const action = routeLine(line)
          switch (action.kind) {
            case 'yield': queue.push(action.line); break
            case 'setLastEventId': lastEventId = action.id; break
            case 'setRetry': reconnectDelay = action.ms; break
            case 'silent': break
          }
        }
      }

      // Process remaining buffer
      if (buffer.length > 0 && !closed) {
        let line = buffer
        if (line.endsWith('\r')) line = line.slice(0, -1)
        const action = routeLine(line)
        if (action.kind === 'yield') queue.push(action.line)
        else if (action.kind === 'setLastEventId') lastEventId = action.id
        else if (action.kind === 'setRetry') reconnectDelay = action.ms
      }
    } finally {
      reader.releaseLock()
      if (!closed) readyState = 'CONNECTING'
    }
  }

  // ── Reconnection loop ──────────────────────────────────────

  async function runLoop() {
    let attempt = 0

    while (!closed) {
      try {
        if (breaker) {
          await breaker.execute(() => connectOnce())
        } else {
          await connectOnce()
        }

        // Stream ended (not user-closed) → reconnect
        if (closed) break

        if (!reconnectEnabled) {
          queue.finish()
          break
        }

        attempt++
        if (attempt > maxRetries) {
          queue.finish()
          break
        }

        const delay = reconnectDelay > 0
          ? Math.round(reconnectDelay + reconnectDelay * 0.25 * (Math.random() * 2 - 1))
          : calculateDelay(attempt, initialDelay, maxDelay, backoffMultiplier)
        await sleep(delay)
      } catch (err) {
        if (closed) break

        attempt++
        if (attempt > maxRetries) {
          queue.fail(err)
          break
        }

        const delay = calculateDelay(attempt, initialDelay, maxDelay, backoffMultiplier)
        await sleep(delay)
      }
    }

    readyState = 'CLOSED'
    queue.finish()
  }

  // Start background reconnection loop
  runLoop().catch(err => queue.fail(err))

  // ── SSEClient interface ────────────────────────────────────

  const client: SSEClient = {
    get readyState() { return readyState },
    get lastEventId() { return lastEventId },
    close() {
      closed = true
      readyState = 'CLOSED'
      queue.finish()
    },
    [Symbol.asyncIterator]() {
      return queue[Symbol.asyncIterator]()
    },
  }

  return client
}
