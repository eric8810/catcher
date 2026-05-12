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
  /** Skip TLS certificate validation. Default: false */
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

export interface IHttpClient {
  get<T = any>(url: string, config?: Record<string, any>): Promise<T>
  post<T = any>(url: string, body?: any, config?: Record<string, any>): Promise<T>
  put<T = any>(url: string, body?: any, config?: Record<string, any>): Promise<T>
  delete<T = any>(url: string, config?: Record<string, any>): Promise<T>
  patch<T = any>(url: string, body?: any, config?: Record<string, any>): Promise<T>
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
  send(data: string | Buffer): void
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
