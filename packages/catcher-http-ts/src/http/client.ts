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
} from '@catcher/core'
import { createRetryWrapper } from './retry.js'
import { createPriorityQueue } from '../queue/priority-queue.js'
import { createInterceptorManager } from './interceptors.js'

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
  rt?: 'json' | 'text' | 'bytes',
): AxiosRequestConfig['responseType'] {
  switch (rt) {
    case 'text': return 'text'
    case 'bytes': return 'arraybuffer'
    default: return 'json'
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
    rejectUnauthorized = false,
    timeout,
    retry,
    concurrency,
    circuitBreaker,
    interceptors: staticInterceptors,
  } = config

  // 1. Build shared Agent (connection pooling + DNS cache + health checks)
  const agent = createSharedAgent({ keepAlive, dnsCacheTtl, rejectUnauthorized })

  // 2. Create underlying axios instance
  const instance: AxiosInstance = axios.create({
    baseURL,
    httpsAgent: agent,
    timeout: typeof timeout === 'number' ? timeout : timeout?.response ?? 30_000,
  })

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

  // 4. Determine effective retry config for a request
  const effectiveRetry = (req?: RequestConfig): RetryOptions | null => {
    if (req?.retry === false) return null        // per-request disabled
    if (req?.retry) return req.retry              // per-request override
    return retry ?? null                          // instance default
  }

  // 5. Optionally wrap with retry (innermost resilience layer)
  const baseDoRequest = retry
    ? createRetryWrapper(instance, retry)
    : (method: string, ...args: any[]) => (instance as any)[method](...args)

  // Per-request retry wrapper (handles retry: false override)
  const rawDoRequest = (method: string, ...args: any[]) => {
    const reqCfg: RequestConfig | undefined = args[args.length - 1]
    const er = effectiveRetry(reqCfg)
    if (!er) {
      // No retry — call axios directly
      return (instance as any)[method](...args)
    }
    // Dynamic wrapper (respects per-request retry config)
    if (er !== retry) {
      const dynamicWrapper = createRetryWrapper(instance, er)
      return dynamicWrapper(method, ...args)
    }
    return baseDoRequest(method, ...args)
  }

  // 6. Optionally wrap with circuit breaker (tracks failures across requests)
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

  // 7. Optionally wrap with concurrency queue (outermost layer)
  const queue = concurrency && concurrency > 0
    ? createPriorityQueue({ concurrency })
    : null

  const enqueue = (priority: number, fn: () => Promise<any>): Promise<any> => {
    if (queue) {
      return queue.add(fn, { priority })
    }
    return fn()
  }

  // 8. Execute a single request through the full pipeline
  const execute = async (
    method: string,
    url: string,
    body: any | undefined,
    defaultPriority: number,
    reqConfig?: RequestConfig,
  ): Promise<any> => {
    // 8a. Build initial request config (merge instance defaults + per-request)
    const merged = mergeConfig(config, reqConfig)

    // 8b. Run request interceptor chain (LIFO)
    let processedConfig = await (reqInterceptors as any)._runRequestChain(merged, merged)

    // 8c. Serialize query params
    let finalUrl = url
    if (processedConfig.params) {
      const serializer = processedConfig.paramsSerializer ?? serializeParams
      const qs = serializer(processedConfig.params)
      finalUrl = appendParams(finalUrl, qs)
    }

    // 8d. Check cancellation before sending
    if (processedConfig.signal?.aborted) {
      const err = new Error('Request cancelled') as Error & { code: string }
      err.code = 'ECANCELED'
      throw err
    }

    // 8e. Build axios config
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

    const priority = processedConfig.priority ?? defaultPriority

    // 8f. Execute through resilience layers
    const rawResp = await enqueue(priority, () => {
      if (body !== undefined) {
        return doRequest(method, finalUrl, body, axiosConfig)
      }
      return doRequest(method, finalUrl, axiosConfig)
    })

    // Normalize response shape
    const response: HttpResponse = {
      status: rawResp.status,
      headers: rawResp.headers as Record<string, string>,
      data: rawResp.data ?? rawResp,
      config: processedConfig,
    }

    // 8g. Run response interceptor chain (FIFO), return data
    const finalResponse = await (resInterceptors as any)._runResponseChain(response)
    // If response interceptors returned the full HttpResponse, extract data;
    // if they returned just data (like axios does), pass it through.
    return finalResponse?.data !== undefined ? finalResponse.data : finalResponse
  }

  // 9. Return stable IHttpClient
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
  }
}
