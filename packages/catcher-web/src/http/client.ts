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

// ── Main ──────────────────────────────────────────────────────

/**
 * Create a resilient HTTP client for browsers.
 *
 * Resilience layers (inside → out):
 *   fetch → retry → circuit breaker → concurrency queue
 */
export function createWebClient(config: HttpClientConfig): IHttpClient {
  const { baseURL, retry, concurrency, circuitBreaker } = config
  const resolvedBase = baseURL.replace(/\/$/, '')

  // ── fetch executor ──
  const doFetch = async (method: string, url: string, body: any, reqConfig?: RequestConfig) => {
    const init: RequestInit = {
      method,
      headers: { 'Content-Type': 'application/json', ...reqConfig?.headers },
      signal: reqConfig?.signal,
    }
    if (body !== undefined) init.body = JSON.stringify(body)

    const fullUrl = resolvedBase + url
    const resp = await fetch(fullUrl, init)

    if (!resp.ok && resp.status >= 500) {
      const err: any = new Error(`HTTP ${resp.status}`)
      err.code = 'HTTP_5XX'
      err.response = { status: resp.status }
      throw err
    }

    const text = await resp.text()
    return {
      status: resp.status,
      headers: Object.fromEntries(resp.headers.entries()),
      data: tryParseJSON(text),
    }
  }

  // ── retry wrapper ──
  const rawDoRequest = retry
    ? (method: string, url: string, body: any, reqConfig?: RequestConfig) =>
        pRetry(
          async () => {
            try { return await doFetch(method, url, body, reqConfig) }
            catch (error: any) {
              const isRetryable =
                error.code === 'HTTP_5XX' ||
                error.name === 'TypeError' // network error in fetch
              if (isRetryable) throw error
              throw new AbortError(error)
            }
          },
          {
            retries: retry.attempts,
            factor: retry.backoff === 'exponential' ? 2 : 1,
            minTimeout: 500,
            maxTimeout: 30_000,
          },
        )
    : (method: string, url: string, body: any, reqConfig?: RequestConfig) =>
        doFetch(method, url, body, reqConfig)

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
      request: { use: () => 0, eject: () => {}, clear: () => {} },
      response: { use: () => 0, eject: () => {}, clear: () => {} },
    } as any,
    circuitBreakerState() {
      return breaker?.state ?? 'closed' as any
    },
    queueDepth() {
      return queue?.size ?? 0
    },
  }
}

function tryParseJSON(text: string): any {
  try { return JSON.parse(text) } catch { return text }
}

export type { IHttpClient, RequestConfig, HttpClientConfig, HttpResponse } from '@catcher/core'
