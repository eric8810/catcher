import { CircuitBreakerPolicy, ConsecutiveBreaker } from 'cockatiel'
import type { SSEClientOptions, SSEClient } from '@eric8810/catcher-core'
import type { SseConnectOnceCtx } from '@eric8810/catcher-core'
import { createExecutor, createSSEClientCore, SSETimeoutErrorImpl } from '@eric8810/catcher-core'
import { routeLine } from './router.js'

function readWithIdleTimeout(
  reader: ReadableStreamDefaultReader<Uint8Array>,
  timeoutMs: number,
  signal?: AbortSignal,
): Promise<{ done: boolean; value?: Uint8Array }> {
  return new Promise((resolve, reject) => {
    if (signal?.aborted) { reject(new Error('Aborted')); return }
    const timer = setTimeout(() => reject(new SSETimeoutErrorImpl(timeoutMs)), timeoutMs)
    const onAbort = () => { clearTimeout(timer); reject(new Error('Aborted')) }
    signal?.addEventListener('abort', onAbort, { once: true })
    reader.read().then(
      result => { clearTimeout(timer); signal?.removeEventListener('abort', onAbort); resolve(result) },
      error => { clearTimeout(timer); signal?.removeEventListener('abort', onAbort); reject(error) },
    )
  })
}

export function createSSEClient(options: SSEClientOptions): SSEClient {
  const { circuitBreaker: cbConfig, ...streamOptions } = options
  let breaker: CircuitBreakerPolicy | null = null
  if (cbConfig) {
    breaker = new CircuitBreakerPolicy(
      { halfOpenAfter: cbConfig.resetTimeout, breaker: new ConsecutiveBreaker(cbConfig.failureThreshold) },
      createExecutor(),
    )
  }

  async function connectOnce(ctx: SseConnectOnceCtx): Promise<void> {
    const { url, method = 'GET', headers: baseHeaders = {}, body, timeout = 30_000, signal } = streamOptions
    const headers: Record<string, string> = { ...baseHeaders }
    if (ctx.lastEventId) headers['Last-Event-ID'] = ctx.lastEventId
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
    try { response = await fetch(url, init) }
    finally { clearTimeout(timeoutId); signal?.removeEventListener('abort', onUserAbort) }

    if (response.status === 204) { ctx.setReadyState('CLOSED'); return }
    if (!response.ok) throw new Error(`SSE connection failed: HTTP ${response.status}`)
    if (!response.body) throw new Error('SSE: response body is null')

    ctx.setReadyState('OPEN')
    const reader = response.body.getReader()
    const decoder = new TextDecoder()
    let buffer = ''
    try {
      while (true) {
        if (ctx.closed()) break
        const { done, value } = await readWithIdleTimeout(reader, timeout, signal)
        if (done) break
        buffer += decoder.decode(value, { stream: true })
        let idx: number
        while ((idx = buffer.indexOf('\n')) !== -1) {
          let line = buffer.slice(0, idx); buffer = buffer.slice(idx + 1)
          if (line.endsWith('\r')) line = line.slice(0, -1)
          const action = routeLine(line)
          switch (action.kind) {
            case 'yield': ctx.queue.push(action.line); break
            case 'setLastEventId': ctx.setLastEventId(action.id); break
            case 'setRetry': ctx.setReconnectDelay(action.ms); break
          }
        }
      }
      if (buffer.length > 0 && !ctx.closed()) {
        let line = buffer; if (line.endsWith('\r')) line = line.slice(0, -1)
        const action = routeLine(line)
        if (action.kind === 'yield') ctx.queue.push(action.line)
        else if (action.kind === 'setLastEventId') ctx.setLastEventId(action.id)
        else if (action.kind === 'setRetry') ctx.setReconnectDelay(action.ms)
      }
    } finally { reader.releaseLock(); if (!ctx.closed()) ctx.setReadyState('CONNECTING') }
  }

  return createSSEClientCore(options, breaker ? (ctx: SseConnectOnceCtx) => breaker!.execute(() => connectOnce(ctx)) : connectOnce)
}
