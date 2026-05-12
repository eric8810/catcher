import axios, { type AxiosInstance } from 'axios'
import { CircuitBreakerPolicy, ConsecutiveBreaker } from 'cockatiel'
import type { ExecuteWrapper } from 'cockatiel/dist/common/Executor.js'
import { createSharedAgent } from '../agent/shared-agent.js'
import type { HttpClientConfig, IHttpClient } from '@catcher/core'
import { createRetryWrapper } from './retry.js'
import { createPriorityQueue } from '../queue/priority-queue.js'

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

/**
 * Create a resilient HTTP client.
 *
 * Resilience layers (inside → out):
 *   axios → retry → circuit breaker → concurrency queue
 *
 * Under the hood it uses axios (today) but exposes a narrow, stable API
 * so we can swap to got/undici/fetch in the future without touching call sites.
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
    interceptors,
  } = config

  // 1. Build shared Agent (connection pooling + DNS cache + health checks)
  const agent = createSharedAgent({ keepAlive, dnsCacheTtl, rejectUnauthorized })

  // 2. Create underlying axios instance
  const instance: AxiosInstance = axios.create({
    baseURL,
    httpsAgent: agent,
    timeout: typeof timeout === 'number' ? timeout : timeout?.response ?? 30_000,
  })

  // 3. Attach interceptors
  if (interceptors?.request) {
    for (const fn of interceptors.request) {
      instance.interceptors.request.use(fn, undefined)
    }
  }
  if (interceptors?.response) {
    const [onFulfilled, onRejected] = interceptors.response
    instance.interceptors.response.use(onFulfilled, onRejected)
  }

  // 4. Optionally wrap with retry (innermost resilience layer)
  //    Fixes #1: retry destroys idle keepAlive sockets to force fresh connections
  //    Fixes #3: retry only on ECONNRESET/ENOTFOUND/ECONNREFUSED/5xx, not ETIMEDOUT
  const rawDoRequest = retry
    ? createRetryWrapper(instance, retry)
    : (method: string, ...args: any[]) => (instance as any)[method](...args)

  // 5. Optionally wrap with circuit breaker (tracks failures across requests)
  //    Fixes #4: circuit breaker now actually wired into request path
  //    Fixes #5: provides cross-request failure memory (CB is the standard pattern)
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

  // 6. Optionally wrap with concurrency queue (outermost layer)
  const queue = concurrency && concurrency > 0
    ? createPriorityQueue({ concurrency })
    : null

  const enqueue = (priority: number, fn: () => Promise<any>): Promise<any> => {
    if (queue) {
      return queue.add(fn, { priority })
    }
    return fn()
  }

  // 7. Return stable IHttpClient
  return {
    get(url, config) {
      return enqueue(3, () => doRequest('get', url, config).then((r: any) => r.data ?? r))
    },
    post(url, body, config) {
      return enqueue(1, () => doRequest('post', url, body, config).then((r: any) => r.data ?? r))
    },
    put(url, body, config) {
      return enqueue(2, () => doRequest('put', url, body, config).then((r: any) => r.data ?? r))
    },
    delete(url, config) {
      return enqueue(3, () => doRequest('delete', url, config).then((r: any) => r.data ?? r))
    },
    patch(url, body, config) {
      return enqueue(2, () => doRequest('patch', url, body, config).then((r: any) => r.data ?? r))
    },
  }
}
