import type { WsClientConfig, WsEvent } from './types'
import { loadNativeAddon } from './native'

const { JsWsClient } = loadNativeAddon('catcher-napi-ws')

/**
 * 类型安全的 WebSocket 客户端
 *
 * **注意**：不要在事件回调内同步调用 `send()`，否则可能因 napi 单线程
 * 限制导致死锁。如需在回调中发送消息，请使用 `setImmediate` 或
 * `process.nextTick` 延迟执行。
 *
 * ```ts
 * const ws = new WsClient(
 *   { urls: ['wss://echo.example.com'] },
 *   (event) => {
 *     if (event.type === 'Connected') console.log('Connected to', event.url)
 *   },
 * )
 * ws.send('hello')
 * ```
 */
export class WsClient {
  private _raw: any  // napi 原生 JsWsClient 实例

  constructor(config: WsClientConfig | string, onEvent?: (event: WsEvent) => void) {
    const json = typeof config === 'string' ? config : JSON.stringify(config)

    // 包装回调：自动 JSON.parse → 强类型
    const wrapped = typeof onEvent === 'function'
      ? (err: any, value: string) => {
          if (err) {
            onEvent({ type: 'Error', message: err.message ?? String(err) })
            return
          }
          if (typeof value === 'string') {
            try {
              onEvent(JSON.parse(value))
            } catch {
              onEvent({ type: 'Error', message: value })
            }
          }
        }
      : undefined

    this._raw = new JsWsClient(json, wrapped)
  }

  /** 发送文本消息 */
  send(data: string): void {
    this._raw.send(data)
  }

  /** 发送二进制消息 */
  sendBinary(data: Buffer | ArrayBuffer | Uint8Array): void {
    let buf: Buffer
    if (data instanceof ArrayBuffer) {
      buf = Buffer.from(data)
    } else if (data instanceof Uint8Array) {
      buf = Buffer.from(data.buffer, data.byteOffset, data.byteLength)
    } else {
      buf = data
    }
    this._raw.sendBinary(buf)
  }

  /** 关闭连接。默认 code=1000, reason='normal' */
  close(code?: number, reason?: string): void {
    this._raw.close(code ?? null, reason ?? null)
  }
}
