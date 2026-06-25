/**
 * @eric8810/catcher-web — Resilient HTTP client for browsers.
 *
 * Replaces axios with fetch(). Reuses the same resilience stack
 * (p-retry + cockatiel + p-queue) from @eric8810/catcher-http.
 *
 * API is intentionally identical to @eric8810/catcher-http.
 */

import { CircuitBreakerPolicy, ConsecutiveBreaker } from 'cockatiel'
import pRetry, { AbortError } from 'p-retry'
import PQueue from 'p-queue'
import type {
  HttpClientConfig,
  IHttpClient,
  RequestConfig,
  HttpResponse,
  RetryOptions,
  CatcherErrorType,
  CatcherHttpError,
  ClientEvent,
} from '@eric8810/catcher-core'
import { createInterceptorManager } from './interceptors.js'
import { createExecutor, classifyFetchError, redactHeaders } from '@eric8810/catcher-core'

// ── Error helpers (G2) ──────────────────────────────────────────

function createCatcherError(
  error: any,
  type: CatcherErrorType,
  method: string,
  url: string,
  headers: Record<string, string>,
  config: RequestConfig,
  attempt: number,
  elapsedMs: number,
): CatcherHttpError {
  const err = new Error(error.message ?? String(error)) as Error & CatcherHttpError
  err.name = 'CatcherHttpError'
  ;(err as any).type = type
  ;(err as any).request = { method, url, headers, config }
  if (error.response) {
    ;(err as any).response = {
      status: error.response.status,
      headers: error.response.headers ?? {},
      data: error.response.data,
      rawData: error.response.rawData,
    }
  }
  ;(err as any).attempt = attempt
  ;(err as any).elapsedMs = elapsedMs
  ;(err as any).toJSON = () => ({
    type, message: err.message,
    request: { method, url, headers: redactHeaders(headers) },
    response: (err as any).response ? { status: (err as any).response.status, data: (err as any).response.data } : undefined,
    attempt, elapsedMs,
  })
  if (error.stack) err.stack = error.stack
  return err as unknown as CatcherHttpError
}

// ── Helpers ───────────────────────────────────────────────────

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
async function parseBody(resp: Response, responseType?: 'json' | 'text' | 'bytes' | 'stream'): Promise<any> {
  switch (responseType) {
    case 'text': return resp.text()
    case 'bytes': return new Uint8Array(await resp.arrayBuffer())
    case 'stream': return resp.body  // G10: Web ReadableStream<Uint8Array>
    case 'json':
    default:
      return tryParseJSON(await resp.text())
  }
}

/**
 * NEW-2: Stream the response body and call onDownloadProgress as bytes arrive.
 * Falls back to simple arrayBuffer() when no progress callback is provided.
 */
async function readBodyWithProgress(
  resp: Response,
  onProgress?: (event: { loaded: number; total?: number }) => void,
  responseType?: 'json' | 'text' | 'bytes' | 'stream',
): Promise<{ data: any; raw: Uint8Array }> {
  const contentLength = parseInt(resp.headers.get('content-length') ?? '', 10)
  const total = isNaN(contentLength) ? undefined : contentLength

  if (!onProgress || !resp.body) {
    // No progress callback — simple read
    const raw = new Uint8Array(await resp.arrayBuffer())
    return { data: parseBodyFromRaw(raw, responseType), raw }
  }

  // Stream with progress tracking
  const reader = resp.body.getReader()
  const chunks: Uint8Array[] = []
  let loaded = 0

  for (;;) {
    const { done, value } = await reader.read()
    if (done) break
    chunks.push(value)
    loaded += value.length
    onProgress({ loaded, total })
  }

  // Merge chunks into single Uint8Array
  const raw = new Uint8Array(loaded)
  let offset = 0
  for (const chunk of chunks) {
    raw.set(chunk, offset)
    offset += chunk.length
  }

  return { data: parseBodyFromRaw(raw, responseType), raw }
}

/** Parse body from raw bytes (avoids consuming the Response body). */
function parseBodyFromRaw(raw: Uint8Array, responseType?: string): any {
  const text = new TextDecoder().decode(raw)
  switch (responseType) {
    case 'text': return text
    case 'bytes': return raw
    case 'json':
    default:
      return tryParseJSON(text)
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
    return mutableConfig.retry ?? null
  }

  // G11: Event system
  const eventListeners = new Map<string, Set<(event: any) => void>>()

  // G2: Track attempt count across retries
  const retryContext = { lastAttempt: 0 }

  // G11: Mutable config for runtime hot-update
  const mutableConfig = { retry, timeout: config.timeout }

  // ── doFetch with interceptor chain ──
  const doFetch = async (method: string, url: string, body: any, reqConfig?: RequestConfig) => {
    const startTime = Date.now()
    retryContext.lastAttempt = 0

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
      throw createCatcherError(
        new Error('Request cancelled'), 'cancelled',
        method, finalUrl, merged.headers ?? {}, merged, 0, Date.now() - startTime,
      )
    }

    // Build timeout + cancellation signal
    const { signal, clear: clearTimeoutSignal } = createTimeoutSignal(
      merged.signal,
      merged.timeout,
    )

    const headers: Record<string, string> = { ...merged.headers }

    // G12: Auth helpers
    if (config.auth) {
      const encoded = btoa(`${config.auth.username}:${config.auth.password}`)
      headers['Authorization'] = `Basic ${encoded}`
    }
    if (config.bearerToken) {
      const resolveToken = typeof config.bearerToken === 'function'
        ? config.bearerToken
        : () => config.bearerToken as string
      const token = await resolveToken()
      if (token) headers['Authorization'] = `Bearer ${token}`
    }
    // G12: XSRF (browser only)
    if (config.xsrfCookieName && typeof document !== 'undefined') {
      const match = document.cookie.match(new RegExp(`(?:^|; )${config.xsrfCookieName}=([^;]*)`))
      if (match) {
        headers[config.xsrfHeaderName ?? 'X-XSRF-TOKEN'] = decodeURIComponent(match[1])
      }
    }

    // Only set default Content-Type when NOT sending FormData (G5)
    if (body !== undefined && !(body instanceof FormData) && !headers['Content-Type'] && !headers['content-type']) {
      headers['Content-Type'] = 'application/json'
    }

    const init: RequestInit = {
      method,
      headers,
      signal,
      // G3: CORS/Credentials
      credentials: merged.credentials ?? config.credentials ?? 'same-origin',
      mode: config.fetchMode ?? 'cors',
      // G6: Redirect
      redirect: config.redirect?.follow === false ? 'manual' : 'follow',
    }

    // Handle body: skip for GET/HEAD, allow custom Content-Type to override
    if (body !== undefined) {
      init.body = body instanceof FormData ? body : JSON.stringify(body)
    }

    try {
      const resp = await fetch(finalUrl, init)

      // NEW-2: Report upload progress — fetch doesn't support granular upload
      // progress, but once we have response headers, the upload is complete.
      if (merged.onUploadProgress && init.body) {
        const bodyLength = typeof init.body === 'string' ? new TextEncoder().encode(init.body).length
          : init.body instanceof ArrayBuffer ? init.body.byteLength
          : init.body instanceof Uint8Array ? init.body.length
          : undefined
        if (bodyLength !== undefined) {
          merged.onUploadProgress({ loaded: bodyLength, total: bodyLength })
        }
      }

      // validateStatus
      const isValid = merged.validateStatus
        ? merged.validateStatus(resp.status)
        : resp.ok

      if (!isValid && resp.status >= 500) {
        const { raw } = await readBodyWithProgress(resp, merged.onDownloadProgress)
        const err: any = new Error(`HTTP ${resp.status}`)
        err.code = 'HTTP_5XX'
        err.response = { status: resp.status, rawData: raw }
        throw err
      }

      // G10: Stream response — no progress tracking for raw streams
      if (merged.responseType === 'stream') {
        const streamResponse: HttpResponse = {
          status: resp.status,
          headers: Object.fromEntries(resp.headers.entries()),
          data: resp.body,
          config: merged,
        }
        // G11: emit requestComplete
        eventListeners.get('requestComplete')?.forEach(fn =>
          fn({ type: 'requestComplete', method, url: finalUrl, status: resp.status, durationMs: Date.now() - startTime }),
        )
        trackQuality(Date.now() - startTime)
        return streamResponse
      }

      // NEW-2: Read body with optional download progress
      const { data, raw: rawBody } = await readBodyWithProgress(resp, merged.onDownloadProgress, merged.responseType)

      if (!isValid) {
        // Non-5xx error (e.g. 4xx) — throw as CatcherHttpError
        const err: any = new Error(`HTTP ${resp.status}`)
        err.response = { status: resp.status, data, rawData: rawBody }
        throw err
      }

      const httpResp: HttpResponse = {
        status: resp.status,
        headers: Object.fromEntries(resp.headers.entries()),
        data,
        config: merged,
      }

      // Run response interceptor chain (FIFO)
      const final = await (resInterceptors as any)._runResponseChain(httpResp)

      // G11: emit requestComplete
      eventListeners.get('requestComplete')?.forEach(fn =>
        fn({ type: 'requestComplete', method, url: finalUrl, status: resp.status, durationMs: Date.now() - startTime }),
      )
      trackQuality(Date.now() - startTime)

      return final?.data !== undefined ? final.data : final
    } catch (error: any) {
      // G2: Wrap into CatcherHttpError
      const type = classifyFetchError(error)
      throw createCatcherError(
        error, type,
        method, finalUrl,
        headers, merged,
        retryContext.lastAttempt, Date.now() - startTime,
      )
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
            error.name === 'TypeError' // network error in fetch
          // AbortError (from AbortController) means intentional cancel — do NOT retry
          if (isRetryable) throw error
          throw new AbortError(error)
        }
      },
      {
        retries: er.attempts,
        factor: er.backoff === 'exponential' ? 2 : 1,
        minTimeout: er.minTimeout ?? 500,
        maxTimeout: er.maxTimeout ?? 30_000,
        onFailedAttempt: (err) => {
          retryContext.lastAttempt = err.attemptNumber
          er.onRetry?.(err.attemptNumber)
          // G11: emit retry event
          eventListeners.get('retry')?.forEach(fn =>
            fn({ type: 'retry', attempt: err.attemptNumber, error: err, url }),
          )
        },
      },
    )
  }

  // ── circuit breaker ──
  let breaker: CircuitBreakerPolicy | null = null
  let lastBreakerState: 'closed' | 'open' | 'half-open' = 'closed'
  if (circuitBreaker) {
    breaker = new CircuitBreakerPolicy(
      {
        halfOpenAfter: circuitBreaker.resetTimeout,
        breaker: new ConsecutiveBreaker(circuitBreaker.failureThreshold),
      },
      createExecutor(),
    )
  }

  const getBreakerState = (b: CircuitBreakerPolicy): 'closed' | 'open' | 'half-open' => {
    const s: number = (b as any).state
    if (s === 1) return 'open'
    if (s === 2) return 'half-open'
    return 'closed'
  }

  // ── network quality tracking ──
  type QualityLevel = 'excellent' | 'good' | 'fair' | 'poor' | 'bad'
  const rttWindow: number[] = []
  const RTT_WINDOW_SIZE = 20
  let lastQualityLevel: QualityLevel = 'good'

  const classifyQuality = (avgRtt: number): QualityLevel => {
    if (avgRtt < 80) return 'excellent'
    if (avgRtt < 200) return 'good'
    if (avgRtt < 500) return 'fair'
    if (avgRtt < 1000) return 'poor'
    return 'bad'
  }

  const trackQuality = (durationMs: number) => {
    rttWindow.push(durationMs)
    if (rttWindow.length > RTT_WINDOW_SIZE) rttWindow.shift()
    const avg = rttWindow.reduce((a, b) => a + b, 0) / rttWindow.length
    const newLevel = classifyQuality(avg)
    if (newLevel !== lastQualityLevel) {
      const from = lastQualityLevel
      lastQualityLevel = newLevel
      eventListeners.get('networkQualityChange')?.forEach(fn =>
        fn({ type: 'networkQualityChange', from, to: newLevel }),
      )
    }
  }

  const doRequest = breaker
    ? (method: string, url: string, body: any, reqConfig?: RequestConfig) => {
        const beforeState = lastBreakerState
        return breaker!.execute(() => rawDoRequest(method, url, body, reqConfig))
          .catch((e: any) => {
            const afterState = getBreakerState(breaker!)
            if (afterState !== lastBreakerState) {
              lastBreakerState = afterState
              eventListeners.get('circuitBreakerChange')?.forEach(fn =>
                fn({ type: 'circuitBreakerChange', from: beforeState, to: afterState }),
              )
            }
            throw e
          })
          .then((result: any) => {
            const afterState = getBreakerState(breaker!)
            if (afterState !== lastBreakerState) {
              lastBreakerState = afterState
              eventListeners.get('circuitBreakerChange')?.forEach(fn =>
                fn({ type: 'circuitBreakerChange', from: beforeState, to: afterState }),
              )
            }
            return result
          })
      }
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
      const s: number = (breaker as any).state
      if (s === 1) return 'open'
      if (s === 2) return 'half-open'
      return 'closed'
    },
    queueDepth() {
      return queue?.size ?? 0
    },

    // G11: Event subscription
    on(event: ClientEvent['type'], listener: (event: ClientEvent) => void): () => void {
      if (!eventListeners.has(event)) eventListeners.set(event, new Set())
      eventListeners.get(event)!.add(listener)
      return () => eventListeners.get(event)?.delete(listener)
    },

    off(event: ClientEvent['type'], listener?: (event: ClientEvent) => void): void {
      if (listener) {
        eventListeners.get(event)?.delete(listener)
      } else {
        eventListeners.delete(event)
      }
    },

    // G11: Runtime config hot-update
    updateConfig(updates: Partial<Pick<HttpClientConfig, 'retry' | 'timeout'>>) {
      if (updates.retry) {
        mutableConfig.retry = updates.retry
      }
      if (updates.timeout) {
        mutableConfig.timeout = updates.timeout
      }
    },
  }
}

export type { IHttpClient, RequestConfig, HttpClientConfig, HttpResponse, CatcherHttpError, ClientEvent } from '@eric8810/catcher-core'