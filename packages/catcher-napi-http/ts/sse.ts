import type { SseClientConfig, SseEvent } from './types'
import { loadNativeAddon } from './native'

const native = loadNativeAddon('catcher-napi-http')

function wrapSseCallback(onEvent: (event: SseEvent) => void): (eventJson: string) => void {
  return (eventJson: string) => {
    try {
      onEvent(JSON.parse(eventJson))
    } catch {
      onEvent({ type: 'Error', message: eventJson })
    }
  }
}

/** 一次性 SSE 流（无自动重连） */
export class SseStream {
  private _handle: any

  constructor(config: SseClientConfig | string, onEvent: (event: SseEvent) => void) {
    const json = typeof config === 'string' ? config : JSON.stringify(config)
    this._handle = native.sseStream(json, wrapSseCallback(onEvent))
  }

  close(): void {
    this._handle.close()
  }
}

/** 长连接 SSE 客户端（自动重连） */
export class SseClient {
  private _handle: any

  constructor(config: SseClientConfig | string, onEvent: (event: SseEvent) => void) {
    const json = typeof config === 'string' ? config : JSON.stringify(config)
    this._handle = native.sseClient(json, wrapSseCallback(onEvent))
  }

  close(): void {
    this._handle.close()
  }
}
