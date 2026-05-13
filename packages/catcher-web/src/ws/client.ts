/**
 * WebSocket client for browsers — reconnect with exponential backoff.
 */

export interface WebSocketClientOptions {
  url: string | string[]
  protocols?: string | string[]
  reconnect?: {
    initialDelay?: number
    maxDelay?: number
    backoffMultiplier?: number
    maxAttempts?: number
  }
  /** Binary message codec (optional) */
  binaryType?: BinaryType
}

export type WsStatus = 'CONNECTING' | 'CONNECTED' | 'CLOSED'

export interface WebSocketClient {
  send(data: string | ArrayBuffer | Blob): void
  close(code?: number, reason?: string): void
  readonly status: WsStatus
  readonly url: string
  addEventListener(type: 'open' | 'close' | 'message' | 'error', listener: EventListener): void
  removeEventListener(type: string, listener: EventListener): void
}

export function createWebSocketClient(options: WebSocketClientOptions): WebSocketClient {
  const {
    url,
    protocols,
    binaryType = 'blob',
    reconnect: reconnectOpts,
  } = options

  const urls = Array.isArray(url) ? url : [url]
  const reconnect = createReconnectState(reconnectOpts)

  let activeSocket: WebSocket | null = null
  let activeUrl = ''
  let currentStatus: WsStatus = 'CLOSED'
  let closedByUser = false
  const listeners = new Map<string, Set<EventListener>>()

  function emit(type: string, event?: Event) {
    listeners.get(type)?.forEach(fn => fn(event ?? new Event(type)))
  }

  function connect() {
    if (closedByUser) return
    currentStatus = 'CONNECTING'

    // Try URLs in order (first to connect wins — browser WebSocket doesn't support racing)
    const targetUrl = urls[0]
    activeUrl = targetUrl

    try {
      const ws = new WebSocket(targetUrl, protocols)
      ws.binaryType = binaryType
      setupSocket(ws)
    } catch {
      // Invalid URL — schedule reconnect
      if (!closedByUser) {
        const delay = reconnect.nextDelay()
        if (delay >= 0) setTimeout(connect, delay)
      }
    }
  }

  function setupSocket(ws: WebSocket) {
    const connectTimer = setTimeout(() => {
      ws.close(4000, 'Handshake timeout')
    }, 10_000)

    ws.onopen = () => {
      clearTimeout(connectTimer)
      reconnect.reset()
      currentStatus = 'CONNECTED'
      activeSocket = ws
      emit('open')
    }

    ws.onmessage = (e) => {
      emit('message', e as Event)
    }

    ws.onclose = (e) => {
      clearTimeout(connectTimer)
      currentStatus = 'CLOSED'
      if (activeSocket === ws) {
        emit('close', new CloseEvent('close', { code: e.code, reason: e.reason }))
        activeSocket = null
        if (!closedByUser) {
          const delay = reconnect.nextDelay()
          if (delay >= 0) setTimeout(connect, delay)
        }
      }
    }

    ws.onerror = () => {
      clearTimeout(connectTimer)
      emit('error', new Event('error'))
    }
  }

  // Start first connection
  connect()

  return {
    send(data) {
      if (activeSocket?.readyState === WebSocket.OPEN) {
        activeSocket.send(data as any)
      }
    },
    close(code, reason) {
      closedByUser = true
      reconnect.reset()
      activeSocket?.close(code, reason)
    },
    get status() { return currentStatus },
    get url() { return activeUrl },
    addEventListener(type, listener) {
      if (!listeners.has(type)) listeners.set(type, new Set())
      listeners.get(type)!.add(listener)
    },
    removeEventListener(type, listener) {
      listeners.get(type)?.delete(listener)
    },
  }
}

// ── Reconnect strategy ──

interface ReconnectState {
  nextDelay: () => number
  reset: () => void
}

function createReconnectState(opts?: WebSocketClientOptions['reconnect']): ReconnectState {
  const {
    initialDelay = 1000,
    maxDelay = 30_000,
    backoffMultiplier = 2,
    maxAttempts = 20,
  } = opts ?? {}

  let attempt = 0

  return {
    nextDelay(): number {
      attempt++
      if (attempt > maxAttempts) return -1
      const exponential = initialDelay * Math.pow(backoffMultiplier, Math.max(0, attempt - 1))
      const delay = Math.min(exponential, maxDelay)
      const jitter = (Math.random() - 0.5) * 0.5 * delay
      return Math.round(delay + jitter)
    },
    reset(): void {
      attempt = 0
    },
  }
}
