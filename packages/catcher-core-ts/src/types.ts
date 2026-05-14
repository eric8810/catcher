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

// === Proxy ===

export interface ProxyConfig {
  /** "http://host:port" | "https://host:port" | "socks5://host:port" */
  url: string
  auth?: { username: string; password: string }
  noProxy?: string[]
}

// === DNS ===

export interface DnsConfig {
  /** Custom DNS nameservers (e.g. ["8.8.8.8"]) */
  nameservers?: string[]
  /** Hostname → IP mapping for custom DNS resolution */
  hostMapping?: Record<string, string>
}

// === TLS ===

export interface TlsConfig {
  rejectUnauthorized?: boolean
  /** Path to CA certificate PEM file */
  caCertPath?: string
  /** CA certificate PEM content */
  caCertPem?: string
  /** Path to client certificate PEM file */
  clientCertPath?: string
  /** Client certificate PEM content */
  clientCertPem?: string
  /** Path to client private key PEM file */
  clientKeyPath?: string
  /** Client private key PEM content */
  clientKeyPem?: string
  /** PFX/PKCS12 client identity (binary) */
  clientIdentityPfx?: Uint8Array
  /** Password for PFX identity */
  clientIdentityPassword?: string
  /** Override TLS SNI hostname */
  tlsSniOverride?: string
  /** Minimum TLS version */
  minTlsVersion?: '1.0' | '1.1' | '1.2' | '1.3'
  /** SHA-256 public key pins (deferred — requires custom cert verifier) */
  pinSha256?: string[]
}

// === Redirect ===

export interface RedirectInfo {
  url: string
  status: number
  headers: Record<string, string>
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
    /** Minimum retry delay in ms */
    minTimeout?: number
    /** Maximum retry delay in ms */
    maxTimeout?: number
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
  // --- G3: CORS / Credentials ---
  /** Browser fetch credentials policy */
  credentials?: 'include' | 'same-origin' | 'omit'
  /** Browser fetch mode */
  fetchMode?: 'cors' | 'no-cors' | 'same-origin' | 'navigate'
  /** Node.js axios withCredentials */
  withCredentials?: boolean
  // --- G4: Proxy ---
  /** HTTP/SOCKS5 proxy: true = auto-detect env, string = proxy URL, object = full config */
  proxy?: boolean | string | ProxyConfig
  // --- G6: Redirect ---
  redirect?: {
    follow?: boolean
    maxRedirects?: number
    beforeRedirect?: (info: RedirectInfo) => boolean
  }
  // --- G7: Custom DNS ---
  dns?: DnsConfig
  // --- G8: TLS ---
  tls?: TlsConfig
  // --- G9: Transport adapter (deferred — not yet consumed by client) ---
  /** @deprecated Not yet consumed by the client. Reserved for future use. */
  adapter?: TransportAdapter
  // --- G12: Auth helpers ---
  /** Basic authentication */
  auth?: { username: string; password: string }
  /** Bearer token — static string or async function for dynamic refresh */
  bearerToken?: string | (() => string | Promise<string>)
  /** XSRF/CSRF cookie name (browser only, reads from document.cookie) */
  xsrfCookieName?: string
  /** XSRF/CSRF header name. Default: "X-XSRF-TOKEN" */
  xsrfHeaderName?: string
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
  responseType?: 'json' | 'text' | 'bytes' | 'stream'
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
  // --- G3: per-request credentials override ---
  credentials?: 'include' | 'same-origin' | 'omit'
  // --- G4: per-request proxy override ---
  proxy?: boolean | string | ProxyConfig
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

  // --- G11: Event system ---
  /** Subscribe to client events. Returns unsubscribe function. */
  on?(event: ClientEvent['type'], listener: (event: ClientEvent) => void): () => void
  /** Unsubscribe from client events */
  off?(event: ClientEvent['type'], listener?: (event: ClientEvent) => void): void

  // --- G11: Runtime config update ---
  /** Hot-update retry configuration at runtime */
  updateConfig?(updates: Partial<Pick<HttpClientConfig, 'retry' | 'timeout'>>): void
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
  // --- G3: Cookie for WS handshake (Node.js ws library) ---
  cookie?: string
  // --- G4: Proxy for WS connections ---
  proxy?: boolean | string | ProxyConfig
}

export interface ResilientWS extends EventTarget {
  send(data: string | Uint8Array): void
  close(code?: number, reason?: string): void
  readonly readyState: number
  readonly url: string
  readonly status: 'CONNECTING' | 'CONNECTED' | 'CLOSED'
  addEventListener(type: 'open' | 'close' | 'message' | 'error' | 'statuschange', listener: EventListener): void
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

// === SSE ===

export interface SSEStreamOptions {
  /** SSE endpoint URL */
  url: string
  /** HTTP method, default 'GET'. AI scenarios typically use 'POST' */
  method?: 'GET' | 'POST'
  /** Request headers (e.g. Authorization) */
  headers?: Record<string, string>
  /** Request body (POST). Objects are auto JSON.stringify'd */
  body?: string | Record<string, unknown>
  /** Request timeout in ms, default 30_000 */
  timeout?: number
  /** Abort signal */
  signal?: AbortSignal
}

export interface SSEClientOptions extends SSEStreamOptions {
  /** Auto-reconnect configuration */
  reconnect?: {
    enabled?: boolean
    maxRetries?: number
    initialDelay?: number
    maxDelay?: number
    backoffMultiplier?: number
  }
  /** Circuit breaker configuration */
  circuitBreaker?: { failureThreshold: number; resetTimeout: number }
}

/**
 * SSE content line stream — yields content lines, silently filters control lines.
 *
 * Library silently handles:
 * - `id:` → records lastEventId for reconnect
 * - `retry:` → adjusts reconnect interval
 * - `: comment` → heartbeat, consumed
 * - empty line → event separator, consumed
 * - chunk buffering → guarantees complete lines per yield
 */
export interface SSEStream extends AsyncIterable<string> {
  /** Extracted from `id:` lines, used for reconnect Last-Event-ID */
  readonly lastEventId: string
}

export interface SSEClient extends AsyncIterable<string> {
  readonly readyState: 'CONNECTING' | 'OPEN' | 'CLOSED'
  readonly lastEventId: string
  /** Close the connection (only for createSSEClient) */
  close(): void
}

export interface SSETimeoutError extends Error {
  readonly type: 'SSE_TIMEOUT'
}

// === Error (G2) ===

export type CatcherErrorType =
  | 'timeout'
  | 'connection'
  | 'dns'
  | 'tls'
  | 'http'
  | 'cancelled'
  | 'unknown'

export interface CatcherHttpError extends Error {
  readonly type: CatcherErrorType
  readonly request: {
    method: string
    url: string
    headers: Record<string, string>
    config: RequestConfig
  }
  readonly response?: {
    status: number
    headers: Record<string, string>
    data: unknown
    rawData?: Uint8Array
  }
  readonly attempt: number
  readonly elapsedMs: number
  toJSON(): Record<string, unknown>
}

export function isCatcherError(err: unknown): err is CatcherHttpError {
  return (
    typeof err === 'object' &&
    err !== null &&
    'type' in err &&
    typeof (err as any).type === 'string' &&
    (err as any).type !== undefined &&
    'request' in err &&
    'attempt' in err &&
    'elapsedMs' in err
  )
}

// === Transport Adapter (G9) ===
//
// NOTE: TransportAdapter is typed here for forward-compatibility but is
// NOT yet consumed by createHttpClient(). The adapter config field exists
// on HttpClientConfig but is currently ignored. Implementation is deferred
// to a future release where it will allow swapping the underlying HTTP
// transport (e.g. for testing mocks, custom protocols, or FFI bridges).

/** @deprecated Not yet consumed by the client. Reserved for future use. */
export interface TransportAdapter {
  execute(config: RequestConfig & { method: string; url: string; body?: any }): Promise<HttpResponse>
}

// === Events (G11) ===
//
// NOTE: `circuitBreakerChange` and `networkQualityChange` are typed here
// for forward-compatibility but are NOT yet emitted by the current
// implementation. Only `retry` and `requestComplete` are emitted.
// Consumers can subscribe via client.on() without error — the events
// will simply not fire until a future release adds the emission logic.

export type ClientEvent =
  | { type: 'retry'; attempt: number; error: Error; url: string }
  | { type: 'requestComplete'; method: string; url: string; status: number; durationMs: number }
  /** @deprecated Not yet emitted. Reserved for future use. */
  | { type: 'circuitBreakerChange'; from: 'closed' | 'open' | 'half-open'; to: 'closed' | 'open' | 'half-open' }
  /** @deprecated Not yet emitted. Reserved for future use. */
  | { type: 'networkQualityChange'; from: string; to: string }
