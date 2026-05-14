import axios, { type AxiosInstance, type AxiosRequestConfig } from 'axios'
import { CircuitBreakerPolicy, ConsecutiveBreaker } from 'cockatiel'
import type { ExecuteWrapper } from 'cockatiel/dist/common/Executor.js'
import { createSharedAgent } from '../agent/shared-agent.js'
import type {
  HttpClientConfig,
  IHttpClient,
  RequestConfig,
  HttpResponse,
  RetryOptions,
  ProxyConfig,
  ClientEvent,
} from '@eric8810/catcher-core'
import { createRetryWrapper } from './retry.js'
import { createPriorityQueue } from '../queue/priority-queue.js'
import { createInterceptorManager } from './interceptors.js'
import { classifyAxiosError, createCatcherError } from './error.js'

/**
 * Minimal ExecuteWrapper compatible with Cockatiel's internal one.
 * Needed because ExecuteWrapper is not exported from 'cockatiel' directly.
 */
function createExecutor(): ExecuteWrapper {
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

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/** Merge instance-level config with per-request overrides. */
function mergeConfig(
  instance: HttpClientConfig,
  req?: RequestConfig,
): RequestConfig {
  if (!req) return {}
  const merged: any = { ...req }

  // Merge headers: instance defaults → request overrides
  if (req.headers) {
    merged.headers = { ...req.headers }
  }

  return merged
}

/** Serialize query params into a URL query string. */
function serializeParams(
  params: Record<string, string | number | boolean | (string | number | boolean)[]>,
): string {
  const parts: string[] = []
  for (const [key, val] of Object.entries(params)) {
    if (Array.isArray(val)) {
      for (const item of val) {
        parts.push(`${encodeURIComponent(key)}=${encodeURIComponent(String(item))}`)
      }
    } else {
      parts.push(`${encodeURIComponent(key)}=${encodeURIComponent(String(val))}`)
    }
  }
  return parts.join('&')
}

/** Append query string to a URL. */
function appendParams(url: string, query: string): string {
  if (!query) return url
  const sep = url.includes('?') ? '&' : '?'
  return url + sep + query
}

/** Map our responseType to axios responseType. */
function toAxiosResponseType(
  rt?: 'json' | 'text' | 'bytes' | 'stream',
): AxiosRequestConfig['responseType'] {
  switch (rt) {
    case 'text': return 'text'
    case 'bytes': return 'arraybuffer'
    case 'stream': return 'stream'
    default: return 'json'
  }
}

/** Detect if body is a FormData instance (Node.js >= 18 or form-data npm package). */
function isFormDataBody(body: any): boolean {
  if (typeof FormData !== 'undefined' && body instanceof FormData) return true
  if (body?.constructor?.name === 'FormData') return true
  return false
}

/** Resolve proxy config from boolean/string/ProxyConfig. */
function resolveProxyConfig(proxy: boolean | string | ProxyConfig): ProxyConfig | null {
  if (proxy === false) return null
  if (proxy === true) {
    const url = process.env.HTTPS_PROXY || process.env.HTTP_PROXY || process.env.http_proxy || process.env.https_proxy
    if (!url) return null
    return { url, noProxy: process.env.NO_PROXY?.split(',') }
  }
  if (typeof proxy === 'string') return { url: proxy }
  return proxy
}

/**
 * G4: Create a proxy Agent for the given proxy config.
 * Dynamically imports https-proxy-agent or socks-proxy-agent (optional deps).
 */
function createProxyAgent(proxyConfig: ProxyConfig): any | null {
  try {
    const proxyUrl = proxyConfig.url
    if (proxyUrl.startsWith('socks')) {
      const { SocksProxyAgent } = require('socks-proxy-agent')
      return new SocksProxyAgent(proxyUrl)
    } else {
      const { HttpsProxyAgent } = require('https-proxy-agent')
      return new HttpsProxyAgent(proxyUrl)
    }
  } catch {
    // Proxy agent packages not installed — warn and continue without proxy
    console.warn('[catcher] Proxy agent not available. Install https-proxy-agent or socks-proxy-agent.')
    return null
  }
}

// ---------------------------------------------------------------------------
// createHttpClient
// ---------------------------------------------------------------------------

/**
 * Create a resilient HTTP client.
 *
 * Resilience layers (inside → out):
 *   axios → retry → circuit breaker → concurrency queue
 *
 * Interceptor chain wraps the entire request lifecycle:
 *   request interceptors (LIFO) → resilience → axios → response interceptors (FIFO)
 */
export function createHttpClient(config: HttpClientConfig): IHttpClient {
  const {
    baseURL,
    keepAlive = true,
    dnsCacheTtl = 300,
    rejectUnauthorized = true,
    timeout,
    retry,
    concurrency,
    circuitBreaker,
    interceptors: staticInterceptors,
  } = config

  // 1. Build shared Agent (connection pooling + DNS cache + health checks)
  const agentOptions = { keepAlive, dnsCacheTtl, rejectUnauthorized }
  // G7: custom DNS host mapping
  if (config.dns?.hostMapping) {
    ;(agentOptions as any).hostMapping = config.dns.hostMapping
  }
  const agent = createSharedAgent(agentOptions)

  // G4: Resolve proxy agent if proxy is configured
  const proxyConfig = config.proxy ? resolveProxyConfig(config.proxy) : null
  const proxyAgent = proxyConfig ? createProxyAgent(proxyConfig) : null

  // 2. Create underlying axios instance
  const axiosDefaults: AxiosRequestConfig = {
    baseURL,
    httpsAgent: proxyAgent ?? agent,
    httpAgent: proxyAgent ?? agent,
    timeout: typeof timeout === 'number' ? timeout : timeout?.response ?? 30_000,
    // G3: withCredentials
    withCredentials: config.withCredentials,
    // G6: redirect
    maxRedirects: config.redirect?.follow === false ? 0 : (config.redirect?.maxRedirects ?? 5),
  }
  const instance: AxiosInstance = axios.create(axiosDefaults)

  // G6: NOTE — beforeRedirect is NOT supported in catcher-http-ts.
  // Axios doesn't expose individual redirect events. Use redirect.follow=false
  // (manual mode) and inspect 3xx responses yourself if you need redirect control.

  // 3. Build dynamic interceptor managers
  const reqInterceptors = createInterceptorManager<RequestConfig>()
  const resInterceptors = createInterceptorManager<HttpResponse>()

  // Seed with static interceptors from config (backward compat)
  if (staticInterceptors?.request) {
    for (const fn of staticInterceptors.request) {
      reqInterceptors.use(fn as any)
    }
  }
  if (staticInterceptors?.response) {
    const [onFulfilled, onRejected] = staticInterceptors.response
    resInterceptors.use(onFulfilled as any, onRejected as any)
  }

  // G11: Event system
  const eventListeners = new Map<string, Set<(event: any) => void>>()
  const emit = (event: ClientEvent) => {
    eventListeners.get(event.type)?.forEach(fn => fn(event))
  }

  // G2: Track attempt count across retries via mutable context
  const retryContext = { lastAttempt: 0 }

  // G11: Mutable config for runtime hot-update
  const mutableConfig = { retry, timeout: config.timeout }

  // 4. Determine effective retry config for a request
  const effectiveRetry = (req?: RequestConfig): RetryOptions | null => {
    if (req?.retry === false) return null        // per-request disabled
    if (req?.retry) return req.retry              // per-request override
    return mutableConfig.retry ?? null            // instance default (mutable)
  }

  // 5. Build tracked retry wrapper (with attempt tracking + event emission)
  const baseDoRequest = retry
    ? (() => {
        const trackedRetry = {
          ...retry,
          onRetry: (attempt: number) => {
            retryContext.lastAttempt = attempt
            emit({ type: 'retry', attempt, error: new Error('retry'), url: '' })
            retry.onRetry?.(attempt)
          },
        }
        return createRetryWrapper(instance, trackedRetry)
      })()
    : (method: string, ...args: any[]) => (instance as any)[method](...args)

  // 6. Per-request retry dispatch — handles retry:false and per-request override
  const rawDoRequest = (method: string, ...args: any[]) => {
    const reqCfg: RequestConfig | undefined = args[args.length - 1]
    const er = effectiveRetry(reqCfg)
    if (!er) {
      // No retry — call axios directly
      return (instance as any)[method](...args)
    }
    // If using the original retry config, use pre-built tracked wrapper
    if (er === retry) {
      return baseDoRequest(method, ...args)
    }
    // Dynamic wrapper for per-request retry config (also tracks attempts)
    const dynamicWrapper = createRetryWrapper(instance, {
      ...er,
      onRetry: (attempt: number) => {
        retryContext.lastAttempt = attempt
        emit({ type: 'retry', attempt, error: new Error('retry'), url: '' })
        er.onRetry?.(attempt)
      },
    })
    return dynamicWrapper(method, ...args)
  }

  // 7. Optionally wrap with circuit breaker (tracks failures across requests)
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
    ? (method: string, ...args: any[]) =>
        breaker!.execute(() => rawDoRequest(method, ...args))
    : rawDoRequest

  // 8. Optionally wrap with concurrency queue (outermost layer)
  const queue = concurrency && concurrency > 0
    ? createPriorityQueue({ concurrency })
    : null

  const enqueue = (priority: number, fn: () => Promise<any>): Promise<any> => {
    if (queue) {
      return queue.add(fn, { priority })
    }
    return fn()
  }

  // 9. Execute a single request through the full pipeline
  const execute = async (
    method: string,
    url: string,
    body: any | undefined,
    defaultPriority: number,
    reqConfig?: RequestConfig,
  ): Promise<any> => {
    const startTime = Date.now()
    retryContext.lastAttempt = 0

    // 9a. Build initial request config (merge instance defaults + per-request)
    const merged = mergeConfig(config, reqConfig)

    // G12: Auth helpers — inject auth headers before interceptor chain
    const authHeaders: Record<string, string> = {}
    if (config.auth) {
      const encoded = Buffer.from(`${config.auth.username}:${config.auth.password}`).toString('base64')
      authHeaders['Authorization'] = `Basic ${encoded}`
    }
    if (config.bearerToken) {
      const resolveToken = typeof config.bearerToken === 'function'
        ? config.bearerToken
        : () => config.bearerToken as string
      const token = await resolveToken()
      if (token) authHeaders['Authorization'] = `Bearer ${token}`
    }
    if (Object.keys(authHeaders).length > 0) {
      merged.headers = { ...authHeaders, ...merged.headers }
    }

    // 9b. Run request interceptor chain (LIFO)
    let processedConfig = await (reqInterceptors as any)._runRequestChain(merged, merged)

    // 9c. Serialize query params
    let finalUrl = url
    if (processedConfig.params) {
      const serializer = processedConfig.paramsSerializer ?? serializeParams
      const qs = serializer(processedConfig.params)
      finalUrl = appendParams(finalUrl, qs)
    }

    // 9d. Check cancellation before sending
    if (processedConfig.signal?.aborted) {
      throw createCatcherError(
        new Error('Request cancelled'),
        'cancelled',
        method, finalUrl,
        processedConfig.headers ?? {},
        processedConfig,
        0,
        Date.now() - startTime,
      )
    }

    // 9e. Build axios config
    const axiosConfig: any = {}
    if (processedConfig.headers) axiosConfig.headers = processedConfig.headers
    if (processedConfig.timeout) axiosConfig.timeout = processedConfig.timeout
    if (processedConfig.signal) axiosConfig.signal = processedConfig.signal
    if (processedConfig.responseType) {
      axiosConfig.responseType = toAxiosResponseType(processedConfig.responseType)
    }
    if (processedConfig.validateStatus) {
      axiosConfig.validateStatus = processedConfig.validateStatus
    }
    if (processedConfig.onUploadProgress) {
      axiosConfig.onUploadProgress = processedConfig.onUploadProgress
    }
    if (processedConfig.onDownloadProgress) {
      axiosConfig.onDownloadProgress = processedConfig.onDownloadProgress
    }
    // Pass retry override through to rawDoRequest
    if (processedConfig.retry !== undefined) {
      axiosConfig.retry = processedConfig.retry
    }

    // G5: FormData — don't set Content-Type, let axios/form-data handle boundary
    if (body !== undefined && isFormDataBody(body)) {
      if (axiosConfig.headers) {
        delete axiosConfig.headers['Content-Type']
        delete axiosConfig.headers['content-type']
      }
    }

    // G3: per-request withCredentials
    if (processedConfig.credentials !== undefined || config.withCredentials) {
      axiosConfig.withCredentials = processedConfig.credentials === 'include' || config.withCredentials === true
    }

    const priority = processedConfig.priority ?? defaultPriority

    // 9f. Execute through resilience layers
    try {
      const rawResp = await enqueue(priority, () => {
        if (body !== undefined) {
          return doRequest(method, finalUrl, body, axiosConfig)
        }
        return doRequest(method, finalUrl, axiosConfig)
      })

      // G10: Stream response — skip interceptor chain and return raw stream
      if (processedConfig.responseType === 'stream') {
        const streamResponse: HttpResponse = {
          status: rawResp.status,
          headers: rawResp.headers as Record<string, string>,
          data: rawResp.data,
          config: processedConfig,
        }
        // G11: emit requestComplete
        emit({
          type: 'requestComplete',
          method, url: finalUrl,
          status: rawResp.status,
          durationMs: Date.now() - startTime,
        })
        return streamResponse
      }

      // Normalize response shape
      const response: HttpResponse = {
        status: rawResp.status,
        headers: rawResp.headers as Record<string, string>,
        data: rawResp.data ?? rawResp,
        config: processedConfig,
      }

      // 9g. Run response interceptor chain (FIFO), return data
      const finalResponse = await (resInterceptors as any)._runResponseChain(response)
      // If response interceptors returned the full HttpResponse, extract data;
      // if they returned just data (like axios does), pass it through.

      // G11: emit requestComplete
      emit({
        type: 'requestComplete',
        method, url: finalUrl,
        status: response.status,
        durationMs: Date.now() - startTime,
      })

      return finalResponse?.data !== undefined ? finalResponse.data : finalResponse
    } catch (error: any) {
      // G2: Wrap into CatcherHttpError
      const type = classifyAxiosError(error)

      // G6: Map axios too-many-redirects error
      if (
        error.code === 'ERR_FR_TOO_MANY_REDIRECTS' ||
        error.message?.includes('too many redirects')
      ) {
        const redirectErr = new Error(`Max redirects exceeded`)
        redirectErr.name = 'MaxRedirectError'
        const catcherErr = createCatcherError(
          redirectErr,
          'http',
          method, finalUrl,
          processedConfig.headers ?? {},
          processedConfig,
          retryContext.lastAttempt,
          Date.now() - startTime,
        )
        throw catcherErr
      }

      const catcherErr = createCatcherError(
        error,
        type,
        method, finalUrl,
        processedConfig.headers ?? {},
        processedConfig,
        retryContext.lastAttempt,
        Date.now() - startTime,
      )
      throw catcherErr
    }
  }

  // 10. Return stable IHttpClient
  return {
    get(url, reqConfig) {
      return execute('get', url, undefined, 3, reqConfig)
    },
    post(url, body, reqConfig) {
      return execute('post', url, body, 1, reqConfig)
    },
    put(url, body, reqConfig) {
      return execute('put', url, body, 2, reqConfig)
    },
    delete(url, reqConfig) {
      return execute('delete', url, undefined, 3, reqConfig)
    },
    patch(url, body, reqConfig) {
      return execute('patch', url, body, 2, reqConfig)
    },

    interceptors: {
      request: reqInterceptors,
      response: resInterceptors,
    },

    circuitBreakerState() {
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

    // G11: Runtime config hot-update (retry only)
    updateConfig(updates: Partial<Pick<HttpClientConfig, 'retry' | 'timeout'>>) {
      if (updates.retry) {
        mutableConfig.retry = updates.retry
      }
      if (updates.timeout) {
        mutableConfig.timeout = updates.timeout
        // Update axios instance timeout
        instance.defaults.timeout = typeof updates.timeout === 'number'
          ? updates.timeout
          : updates.timeout?.response ?? 30_000
      }
    },
  } as IHttpClient
}
