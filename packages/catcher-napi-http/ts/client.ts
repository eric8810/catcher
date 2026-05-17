import type {
  HttpClientConfig,
  RequestOptions,
  HttpResponse,
  Metrics,
  StreamEvent,
} from './types'
import { loadNativeAddon } from './native'

const { JsHttpClient } = loadNativeAddon('catcher-napi-http')

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
    this._raw = new JsHttpClient(json)
  }

  async get(url: string, options?: RequestOptions): Promise<HttpResponse> {
    return this._raw.get(url, options ?? undefined)
  }

  async post(url: string, body?: Buffer, options?: RequestOptions): Promise<HttpResponse> {
    return this._raw.post(url, body ?? undefined, options ?? undefined)
  }

  async put(url: string, body?: Buffer, options?: RequestOptions): Promise<HttpResponse> {
    if (!this._raw.put) {
      throw new Error('put() requires rebuilt native addon (cargo build)')
    }
    return this._raw.put(url, body ?? undefined, options ?? undefined)
  }

  async delete(url: string, options?: RequestOptions): Promise<HttpResponse> {
    if (!this._raw.delete) {
      throw new Error('delete() requires rebuilt native addon (cargo build)')
    }
    return this._raw.delete(url, options ?? undefined)
  }

  async patch(url: string, body?: Buffer, options?: RequestOptions): Promise<HttpResponse> {
    if (!this._raw.patch) {
      throw new Error('patch() requires rebuilt native addon (cargo build)')
    }
    return this._raw.patch(url, body ?? undefined, options ?? undefined)
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

    this._raw.executeStream(method, url, body ?? undefined, options ?? undefined, wrapped)
  }
}
