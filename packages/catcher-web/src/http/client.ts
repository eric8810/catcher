/**
 * @catcher/web — Resilient HTTP client for browsers.
 *
 * Replaces axios with fetch(). Reuses the same resilience stack
 * (p-retry + cockatiel + p-queue) from @catcher/http.
 *
 * API is intentionally identical to @catcher/http.
 */

import { CircuitBreakerPolicy, ConsecutiveBreaker } from 'cockatiel'
import type { ExecuteWrapper } from 'cockatiel/dist/common/Executor.js'
import pRetry, { AbortError } from 'p-retry'
import PQueue from 'p-queue'
import type {
  HttpClientConfig,
  IHttpClient,
  RequestConfig,
  HttpResponse,
  RetryOptions,
} from '@catcher/core'
import { createInterceptorManager } from './interceptors.js'

// ── Helpers ───────────────────────────────────────────────────

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

/** Serialize query params. */
function serializeParams(params: Record<string, any>): string {
  const parts: string[] = []
  for (const [key, val] of Object.entries(params)) {
    if (Array.isArray(val)) {
      for (const item of val) parts.push(`${encodeURIComponent(key)}=${encodeURIComponent(String(item))}`)
    } else {
      parts.push(`${encodeURIComponent(key)}=${encodeURIComponent(String(val))}`)
    }
  }
  return parts.join('&')
}

function appendParams(url: string, query: string): string {
  if (!query) return url
  return url + (url.includes('?') ? '&' : '?') + query
}

/** Create a combined AbortSignal that fires on either user cancel or timeout. */
function createTimeoutSignal(signal?: AbortSignal, timeoutMs?: number): { signal: AbortSignal; clear: () => void } {
  if (!timeoutMs && !signal) return { signal: undefined as any, clear: () => {} }
  const controller = new AbortController()
  let timeoutId: ReturnType<typeof setTimeout> | null = null

  const onUserAbort = () => controller.abort()
  signal?.addEventListener('abort', onUserAbort, { once: true })

  if (timeoutMs && timeoutMs > 0) {
    timeoutId = setTimeout(() => controller.abort(), timeoutMs)
  }

  return {
    signal: controller.signal,
    clear() {
      if (timeoutId !== null) clearTimeout(timeoutId)
      signal?.removeEventListener('abort', onUserAbort)
    },
  }
}

/** Parse response body according to responseType. */
async function parseBody(resp: Response, responseType?: 'json' | 'text' | 'bytes'): Promise<any> {
  switch (responseType) {
    case 'text': return resp.text()
    case 'bytes': return new Uint8Array(await resp.arrayBuffer())
    case 'json':
    default:
      return tryParseJSON(await resp.text())
  }
}

function tryParseJSON(text: string): any {
  try { return JSON.parse(text) } catch { return text }
}

// ── Main ──────────────────────────────────────────────────────

/**
 * Create a resilient HTTP client for browsers.
 *
 * Resilience layers (inside → out):
 *   fetch → retry → circuit breaker → concurrency queue
 */
export function createWebClient(config: HttpClientConfig): IHttpClient {
  const {
    baseURL,
    retry,
    concurrency,
    circuitBreaker,
    interceptors: staticInterceptors,
  } = config
  const resolvedBase = baseURL.replace(/\/$/, '')

  // ── interceptors ──
  const reqInterceptors = createInterceptorManager<RequestConfig>()
  const resInterceptors = createInterceptorManager<HttpResponse>()

  // Seed with static interceptors from config
  if (staticInterceptors?.request) {
    for (const fn of staticInterceptors.request) {
      reqInterceptors.use(fn as any)
    }
  }
  if (staticInterceptors?.response) {
    const [onFulfilled, onRejected] = staticInterceptors.response
    resInterceptors.use(onFulfilled as any, onRejected as any)
  }

  // Determine effective retry config for a request
  const effectiveRetry = (req?: RequestConfig): RetryOptions | null => {
    if (req?.retry === false) return null
    if (req?.retry) return req.retry
    return retry ?? null
  }

  // ── doFetch with interceptor chain ──
  const doFetch = async (method: string, url: string, body: any, reqConfig?: RequestConfig) => {
    // Run request interceptor chain (LIFO)
    let merged = { ...reqConfig }
    merged = await (reqInterceptors as any)._runRequestChain(merged, merged)

    // Build query string from params
    let finalUrl = resolvedBase + url
    if (merged.params) {
      const serializer = merged.paramsSerializer ?? serializeParams
      const qs = serializer(merged.params)
      finalUrl = appendParams(finalUrl, qs)
    }

    // Check cancellation before sending
    if (merged.signal?.aborted) {
      const err = new Error('Request cancelled') as Error & { code: string }
      err.code = 'ECANCELED'
      throw err
    }

    // Build timeout + cancellation signal
    const { signal, clear: clearTimeoutSignal } = createTimeoutSignal(
      merged.signal,
      merged.timeout,
    )

    const headers: Record<string, string> = { ...merged.headers }
    // Only set default Content-Type when sending a body
    if (body !== undefined && !headers['Content-Type'] && !headers['content-type']) {
      headers['Content-Type'] = 'application/json'
    }

    const init: RequestInit = {
      method,
      headers,
      signal,
    }

    // Handle body: skip for GET/HEAD, allow custom Content-Type to override
    if (body !== undefined) {
      init.body = JSON.stringify(body)
    }

    try {
      const resp = await fetch(finalUrl, init)

      // validateStatus
      const isValid = merged.validateStatus
        ? merged.validateStatus(resp.status)
        : resp.ok

      if (!isValid && resp.status >= 500) {
        const err: any = new Error(`HTTP ${resp.status}`)
        err.code = 'HTTP_5XX'
        err.response = { status: resp.status }
        throw err
      }

      const data = await parseBody(resp, merged.responseType)
      const httpResp: HttpResponse = {
        status: resp.status,
        headers: Object.fromEntries(resp.headers.entries()),
        data,
        config: merged,
      }

      // Run response interceptor chain (FIFO)
      const final = await (resInterceptors as any)._runResponseChain(httpResp)
      return final?.data !== undefined ? final.data : final
    } finally {
      clearTimeoutSignal()
    }
  }

  // ── retry wrapper ──
  const rawDoRequest = (
    method: string,
    url: string,
    body: any,
    reqConfig?: RequestConfig,
  ) => {
    const er = effectiveRetry(reqConfig)
    if (!er) {
      return doFetch(method, url, body, reqConfig)
    }

    return pRetry(
      async () => {
        try { return await doFetch(method, url, body, reqConfig) }
        catch (error: any) {
          const isRetryable =
            error.code === 'HTTP_5XX' ||
            error.code === 'ECONNABORTED' ||
            error.name === 'TypeError' || // network error in fetch
            error.name === 'AbortError'
          if (isRetryable) throw error
          throw new AbortError(error)
        }
      },
      {
        retries: er.attempts,
        factor: er.backoff === 'exponential' ? 2 : 1,
        minTimeout: er.minTimeout ?? 500,
        maxTimeout: er.maxTimeout ?? 30_000,
        onFailedAttempt: er.onRetry
          ? (err) => er.onRetry!(err.attemptNumber)
          : undefined,
      },
    )
  }

  // ── circuit breaker ──
  let breaker: CircuitBreakerPolicy | null = null
  if (circuitBreaker) {
    breaker = new CircuitBreakerPolicy(
      {
        halfOpenAfter: circuitBreaker.resetTimeout,
        breaker: new ConsecutiveBreaker(circuitBreaker.failureThreshold),
      },
      createExecutor(),
    )
  }

  const doRequest = breaker
    ? (method: string, url: string, body: any, reqConfig?: RequestConfig) =>
        breaker!.execute(() => rawDoRequest(method, url, body, reqConfig))
    : rawDoRequest

  // ── concurrency queue ──
  const queue = concurrency && concurrency > 0
    ? new PQueue({ concurrency })
    : null

  const enqueue = (priority: number, fn: () => Promise<any>) =>
    queue ? queue.add(fn, { priority }) : fn()

  // ── return ──
  return {
    get(url, reqConfig) {
      return enqueue(3, () => doRequest('GET', url, undefined, reqConfig))
    },
    post(url, body, reqConfig) {
      return enqueue(1, () => doRequest('POST', url, body, reqConfig))
    },
    put(url, body, reqConfig) {
      return enqueue(2, () => doRequest('PUT', url, body, reqConfig))
    },
    delete(url, reqConfig) {
      return enqueue(3, () => doRequest('DELETE', url, undefined, reqConfig))
    },
    patch(url, body, reqConfig) {
      return enqueue(2, () => doRequest('PATCH', url, body, reqConfig))
    },
    interceptors: {
      request: reqInterceptors,
      response: resInterceptors,
    } as any,
    circuitBreakerState(): 'closed' | 'open' | 'half-open' {
      if (!breaker) return 'closed'
      // Cockatiel CircuitState enum: Closed=0, Open=1, HalfOpen=2
      const s: number = (breaker as any).state
      if (s === 1) return 'open'
      if (s === 2) return 'half-open'
      return 'closed'
    },
    queueDepth() {
      return queue?.size ?? 0
    },
  }
}

export type { IHttpClient, RequestConfig, HttpClientConfig, HttpResponse } from '@catcher/core'
