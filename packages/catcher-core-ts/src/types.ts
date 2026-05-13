// === Agent ===

export interface SharedAgentOptions {
  /** Enable TCP keep-alive. Default: true */
  keepAlive?: boolean
  /** Idle keep-alive duration in ms. Default: 30_000 */
  keepAliveMsecs?: number
  /** Max concurrent sockets per host. Default: 25 */
  maxSockets?: number
  /** Max idle sockets in pool. Default: 10 */
  maxFreeSockets?: number
  /** Socket timeout in ms. Default: 60_000 */
  timeout?: number
  /** Reject unauthorized TLS certificates. Default: true (secure). Set to false only for testing/development. */
  rejectUnauthorized?: boolean
  /** DNS cache TTL in seconds. Default: 300 */
  dnsCacheTtl?: number
}

// === HTTP ===

export interface HttpClientConfig {
  /** Base URL for all requests */
  baseURL: string
  /** Connection layer options */
  keepAlive?: boolean
  dnsCacheTtl?: number
  rejectUnauthorized?: boolean
  /** Timeout options in ms */
  timeout?: {
    connect?: number
    response?: number
  } | number
  /** Auto-retry on failure */
  retry?: {
    attempts: number
    backoff?: 'fixed' | 'exponential'
    retryIf?: (error: any) => boolean
    /** Called on each retry attempt (attempt number 1-based) */
    onRetry?: (attempt: number) => void
  }
  /** Max concurrent requests */
  concurrency?: number
  /** Circuit breaker */
  circuitBreaker?: {
    failureThreshold: number
    resetTimeout: number
  }
  /** Request/response interceptors (axios-compatible) */
  interceptors?: {
    request?: Array<(config: any) => any>
    response?: Array<(response: any) => any>
  }
}

// === Per-request Options ===

export interface RequestConfig {
  /** Per-request headers, merged with instance defaults */
  headers?: Record<string, string>
  /** Override timeout for this request (ms) */
  timeout?: number
  /** AbortController signal for cancellation */
  signal?: AbortSignal
  /** Override instance-level retry; false = disable retry for this request */
  retry?: RetryOptions | false
  /** Response body parsing mode */
  responseType?: 'json' | 'text' | 'bytes'
  /** Custom success status code predicate */
  validateStatus?: (status: number) => boolean
  /** Override priority (0 = highest) */
  priority?: number
  /** Opaque metadata, passed through to response interceptors */
  meta?: Record<string, unknown>
  /** Query parameters appended to URL */
  params?: Record<string, string | number | boolean | (string | number | boolean)[]>
  /** Custom params serializer */
  paramsSerializer?: (params: Record<string, any>) => string
  /** Upload progress callback */
  onUploadProgress?: (event: ProgressEvent) => void
  /** Download progress callback */
  onDownloadProgress?: (event: ProgressEvent) => void
}

export interface ProgressEvent {
  loaded: number
  total?: number
}

// === Interceptors ===

export interface InterceptorFulfilled<T> {
  (value: T): T | Promise<T>
}

export interface InterceptorRejected {
  (error: any): any
}

export interface InterceptorHandler<T> {
  onFulfilled: InterceptorFulfilled<T>
  onRejected?: InterceptorRejected
  runWhen?: (config: RequestConfig) => boolean
  synchronous?: boolean
}

export interface InterceptorManager<T> {
  use(
    onFulfilled: InterceptorFulfilled<T>,
    onRejected?: InterceptorRejected,
    options?: { runWhen?: (config: RequestConfig) => boolean; synchronous?: boolean },
  ): number
  eject(id: number): void
  clear(): void
}

// === HTTP Client ===

/** Response object passed through response interceptors */
export interface HttpResponse<T = any> {
  status: number
  headers: Record<string, string>
  data: T
  config: RequestConfig
}

export interface IHttpClient {
  get<T = any>(url: string, config?: RequestConfig): Promise<T>
  post<T = any>(url: string, body?: any, config?: RequestConfig): Promise<T>
  put<T = any>(url: string, body?: any, config?: RequestConfig): Promise<T>
  delete<T = any>(url: string, config?: RequestConfig): Promise<T>
  patch<T = any>(url: string, body?: any, config?: RequestConfig): Promise<T>

  /** Dynamic interceptor managers */
  interceptors: {
    request: InterceptorManager<RequestConfig>
    response: InterceptorManager<HttpResponse>
  }

  /** Current circuit breaker state */
  circuitBreakerState(): 'closed' | 'open' | 'half-open'

  /** Number of requests waiting in the concurrency queue */
  queueDepth(): number
}

// === WebSocket ===

export interface ResilientWSOptions {
  /** WebSocket server URL(s). Multiple = multi-endpoint racing */
  url: string | string[]
  /** Sub-protocols */
  protocol?: string | string[]
  /** Enable per-message deflate compression */
  perMessageDeflate?: boolean | { threshold?: number }
  /** Handshake timeout in ms. Default: 10_000 */
  handshakeTimeout?: number
  /** Max payload in bytes. Default: 1MB */
  maxPayload?: number
  /** Auto-reconnect strategy */
  reconnect?: {
    initialDelay?: number
    maxDelay?: number
    backoffMultiplier?: number
    maxAttempts?: number
  }
  /** Multi-endpoint: how many to race. Default: 3 */
  raceCount?: number
  /** Custom headers */
  headers?: Record<string, string>
  /** Skip TLS cert validation */
  rejectUnauthorized?: boolean
}

export interface ResilientWS extends EventTarget {
  send(data: string | Uint8Array): void
  close(code?: number, reason?: string): void
  readonly readyState: number
  readonly url: string
  readonly status: 'CONNECTING' | 'CONNECTED' | 'CLOSED'
  addEventListener(type: 'open' | 'close' | 'message' | 'error', listener: EventListener): void
  removeEventListener(type: string, listener: EventListener): void
}

// === Queue ===

export interface PriorityQueueOptions {
  /** Max concurrent tasks. Default: 10 */
  concurrency?: number
  /** Queue timeout in ms. Default: no timeout */
  timeout?: number
}

// === Retry ===

export interface RetryOptions {
  attempts: number
  backoff?: 'fixed' | 'exponential'
  minTimeout?: number
  maxTimeout?: number
  /** Called on each retry attempt. attemptNum: 1-based retry number (1 = first retry) */
  onRetry?: (attemptNum: number) => void
}
