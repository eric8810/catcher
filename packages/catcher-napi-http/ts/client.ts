import type {
  HttpClientConfig,
  RequestOptions,
  HttpResponse,
  Metrics,
  StreamEvent,
} from './types'
import { loadNativeAddon } from './native'

const { JsHttpClient } = loadNativeAddon('catcher-napi-http')

const NATIVE_ERROR_PREFIX = 'CATCHER_ERROR:'
const NATIVE_HTTP_ERROR_PATTERN = /^HTTP error: status (\d{3}), body: ([\s\S]*)$/

export type CatcherErrorCode =
  | 'CONNECTION_TIMEOUT'
  | 'REQUEST_TIMEOUT'
  | 'TLS_ERROR'
  | 'DNS_ERROR'
  | 'CONNECTION_ERROR'
  | 'TRANSPORT_ERROR'
  | 'HTTP_ERROR'
  | 'WS_HANDSHAKE_TIMEOUT'
  | 'WS_DISCONNECTED'
  | 'WS_ALL_ENDPOINTS_FAILED'
  | 'RETRY_EXHAUSTED'
  | 'CIRCUIT_BREAKER_OPEN'
  | 'QUEUE_TIMEOUT'
  | 'ENCODE_ERROR'
  | 'DECODE_ERROR'
  | 'INVALID_CONFIG'
  | 'SSE_TIMEOUT'
  | 'INTERNAL_ERROR'

export type CatcherErrorPhase =
  | 'config'
  | 'dns'
  | 'connect'
  | 'tls'
  | 'queue'
  | 'request'
  | 'response'
  | 'encode'
  | 'decode'
  | 'internal'

export interface CatcherErrorSnapshot {
  code: CatcherErrorCode
  phase: CatcherErrorPhase
  retryable: boolean
  message: string
  details: CatcherErrorDetails
}

export interface CatcherErrorDetails {
  status?: number
  body?: string
  timeoutMs?: number
  host?: string
  reason?: string
  attempts?: number
  lastError?: CatcherErrorSnapshot
}

type NativeErrorPayload = CatcherErrorSnapshot

/** Catcher 原生层的结构化错误。 */
export class CatcherError extends Error {
  readonly code: CatcherErrorCode
  readonly phase: CatcherErrorPhase
  readonly retryable: boolean
  readonly details: CatcherErrorDetails
  readonly cause: unknown

  constructor(payload: NativeErrorPayload, cause?: unknown) {
    super(payload.message)
    this.name = 'CatcherError'
    this.code = payload.code
    this.phase = payload.phase
    this.retryable = payload.retryable
    this.details = payload.details
    this.cause = cause
  }

  toJSON(): Record<string, unknown> {
    return {
      name: this.name,
      code: this.code,
      phase: this.phase,
      retryable: this.retryable,
      message: this.message,
      details: this.details,
    }
  }
}

/** Catcher HTTP 状态错误。 */
export class HttpError extends CatcherError {
  readonly status: number
  readonly body: string

  constructor(status: number, body: string, cause?: unknown) {
    super({
      code: 'HTTP_ERROR',
      phase: 'response',
      retryable: status >= 500,
      message: `HTTP error: status ${status}, body: ${body}`,
      details: { status, body },
    }, cause)
    this.name = 'HttpError'
    this.status = status
    this.body = body
  }
}

function parseNativeErrorPayload(message: string): NativeErrorPayload | undefined {
  if (!message.startsWith(NATIVE_ERROR_PREFIX)) return undefined
  try {
    const payload = JSON.parse(message.slice(NATIVE_ERROR_PREFIX.length)) as NativeErrorPayload
    if (
      typeof payload.code !== 'string' ||
      typeof payload.phase !== 'string' ||
      typeof payload.retryable !== 'boolean' ||
      typeof payload.message !== 'string' ||
      typeof payload.details !== 'object' ||
      payload.details === null
    ) {
      return undefined
    }
    return payload
  } catch {
    return undefined
  }
}

function normalizeNativeError(error: unknown): Error {
  if (error instanceof CatcherError) return error
  const message = error instanceof Error ? error.message : String(error)
  const payload = parseNativeErrorPayload(message)
  if (payload) {
    if (
      payload.code === 'HTTP_ERROR' &&
      typeof payload.details.status === 'number' &&
      typeof payload.details.body === 'string'
    ) {
      return new HttpError(payload.details.status, payload.details.body, error)
    }
    return new CatcherError(payload, error)
  }
  const match = NATIVE_HTTP_ERROR_PATTERN.exec(message)
  if (!match) return error instanceof Error ? error : new Error(message)
  return new HttpError(Number(match[1]), match[2], error)
}

// ── 选项归一化 ──
// NAPI-RS 自动把 Rust snake_case 转成 JS camelCase。
// 旧代码可能仍传 { content_type, timeout_ms }，映射到正确属性名。
// 显式构建干净对象，避免遗留 snake_case 属性传给 native addon。
function normalizeOptions(options?: RequestOptions): RequestOptions | undefined {
  if (!options) return undefined
  const raw = options as Record<string, unknown>
  return {
    headers: (raw.headers ?? raw.headers) as Record<string, string> | undefined,
    timeoutMs: (raw.timeoutMs ?? raw.timeout_ms) as number | undefined,
    contentType: (raw.contentType ?? raw.content_type) as string | undefined,
  }
}

/**
 * 类型安全的 HTTP 客户端
 *
 * ```ts
 * const client = new HttpClient({ base_url: 'https://api.example.com' })
 * const resp = await client.get('/users/1')
 * ```
 */
export class HttpClient {
  private _raw: any  // napi 原生 JsHttpClient 实例

  constructor(config: HttpClientConfig | string) {
    const json = typeof config === 'string' ? config : JSON.stringify(config)
    try {
      this._raw = new JsHttpClient(json)
    } catch (error) {
      throw normalizeNativeError(error)
    }
  }

  private async _execute<T>(operation: () => Promise<T>): Promise<T> {
    try {
      return await operation()
    } catch (error) {
      throw normalizeNativeError(error)
    }
  }

  async get(url: string, options?: RequestOptions): Promise<HttpResponse> {
    return this._execute(() => this._raw.get(url, normalizeOptions(options)))
  }

  async post(url: string, body?: Buffer, options?: RequestOptions): Promise<HttpResponse> {
    return this._execute(() =>
      this._raw.post(url, body ?? undefined, normalizeOptions(options)),
    )
  }

  async put(url: string, body?: Buffer, options?: RequestOptions): Promise<HttpResponse> {
    if (!this._raw.put) {
      throw new Error('put() requires rebuilt native addon (cargo build)')
    }
    return this._execute(() =>
      this._raw.put(url, body ?? undefined, normalizeOptions(options)),
    )
  }

  async delete(url: string, options?: RequestOptions): Promise<HttpResponse> {
    if (!this._raw.delete) {
      throw new Error('delete() requires rebuilt native addon (cargo build)')
    }
    return this._execute(() => this._raw.delete(url, normalizeOptions(options)))
  }

  async patch(url: string, body?: Buffer, options?: RequestOptions): Promise<HttpResponse> {
    if (!this._raw.patch) {
      throw new Error('patch() requires rebuilt native addon (cargo build)')
    }
    return this._execute(() =>
      this._raw.patch(url, body ?? undefined, normalizeOptions(options)),
    )
  }

  circuitBreakerState(): 'closed' | 'open' | 'half-open' {
    return this._raw.circuitBreakerState()
  }

  metrics(): Metrics {
    return this._raw.metrics()
  }

  setAdaptiveTimeout(
    minTimeoutMs: number,
    maxTimeoutMs: number,
    multiplier: number,
    windowSize: number,
  ): void {
    this._raw.setAdaptiveTimeout(minTimeoutMs, maxTimeoutMs, multiplier, windowSize)
  }

  disableAdaptiveTimeout(): void {
    this._raw.disableAdaptiveTimeout()
  }

  /**
   * 通知客户端网络环境已变化（WiFi 切换 / VPN 换节点 / 蜂窝切换等）。
   *
   * 清空 DNS 缓存、重建连接池（丢弃可能半开的 keep-alive 连接）、重置
   * 熔断器 — 新请求立即走新网络上的全新连接。飞行中的请求不受影响。
   */
  networkChanged(): void {
    if (!this._raw.networkChanged) {
      throw new Error('networkChanged() requires rebuilt native addon (cargo build)')
    }
    try {
      this._raw.networkChanged()
    } catch (error) {
      throw normalizeNativeError(error)
    }
  }

  cancelAll(): void {
    this._raw.cancelAll()
  }

  cancelRequest(requestId: number): boolean {
    return this._raw.cancelRequest(requestId)
  }

  nextRequestId(): number {
    return this._raw.nextRequestId()
  }

  /**
   * 流式下载 — 回调直接收到解析后的强类型事件对象
   */
  executeStream(
    method: string,
    url: string,
    body?: Buffer,
    options?: RequestOptions,
    onChunk?: (event: StreamEvent) => void,
  ): void {
    const wrapped = typeof onChunk === 'function'
      ? (eventJson: string) => {
          try {
            onChunk(JSON.parse(eventJson))
          } catch {
            onChunk({ type: 'Error', message: eventJson })
          }
        }
      : undefined

    this._raw.executeStream(method, url, body ?? undefined, normalizeOptions(options), wrapped)
  }
}
