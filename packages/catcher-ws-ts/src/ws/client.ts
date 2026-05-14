import WebSocket from 'ws'
import type { ResilientWSOptions, ResilientWS, ProxyConfig } from '@eric8810/catcher-core'
import { createReconnectStrategy } from './reconnect.js'
import { raceEndpoints } from './multi-endpoint.js'

/**
 * Resolve proxy config from boolean/string/ProxyConfig.
 */
function resolveProxyConfig(proxy: boolean | string | ProxyConfig): ProxyConfig | null {
  if (proxy === false) return null
  if (proxy === true) {
    const url = process.env.HTTPS_PROXY || process.env.HTTP_PROXY || process.env.http_proxy || process.env.https_proxy
    if (!url) return null
    return { url, noProxy: process.env.NO_PROXY?.split(',') }
  }
  if (typeof proxy === 'string') return { url: proxy }
  return proxy
}

/**
 * Create a resilient WebSocket client with:
 * - perMessageDeflate compression
 * - configurable handshake timeout
 * - exponential backoff reconnection
 * - optional multi-endpoint racing
 * - cookie header support (G3)
 * - proxy support (G4)
 */
export function createResilientWS(options: ResilientWSOptions): ResilientWS {
  const {
    url,
    protocol,
    perMessageDeflate = true,
    handshakeTimeout = 10_000,
    maxPayload = 1024 * 1024,
    reconnect: reconnectOpts,
    raceCount = 3,
    headers,
    rejectUnauthorized = true,
    cookie,
    proxy,
  } = options

  const urls = Array.isArray(url) ? url : [url]
  const reconnect = createReconnectStrategy(reconnectOpts)

  // Build WebSocket constructor options
  const wsOptions: WebSocket.ClientOptions = {
    handshakeTimeout,
    maxPayload,
    rejectUnauthorized,
    headers: {
      ...headers,
      ...(cookie ? { Cookie: cookie } : {}),
    },
  }

  // G4: Proxy support
  const proxyConfig = proxy ? resolveProxyConfig(proxy) : null
  if (proxyConfig) {
    try {
      // Dynamic import for proxy agents — these are optional dependencies
      const proxyUrl = proxyConfig.url
      if (proxyUrl.startsWith('socks')) {
        // SOCKS5 proxy
        const { SocksProxyAgent } = require('socks-proxy-agent')
        ;(wsOptions as any).agent = new SocksProxyAgent(proxyUrl)
      } else {
        // HTTP/HTTPS proxy
        const { HttpsProxyAgent } = require('https-proxy-agent')
        ;(wsOptions as any).agent = new HttpsProxyAgent(proxyUrl)
      }
    } catch {
      // Proxy agent not available — continue without proxy
      console.warn('[catcher] Proxy agent not available. Install https-proxy-agent or socks-proxy-agent.')
    }
  }

  if (protocol) {
    // ws v8 ClientOptions.protocol accepts string | string[]
    ;(wsOptions as any).protocol = protocol
  }

  if (perMessageDeflate) {
    wsOptions.perMessageDeflate = typeof perMessageDeflate === 'object'
      ? perMessageDeflate
      : {
          zlibDeflateOptions: { level: 6, memLevel: 7 },
          threshold: 1024,
        }
  }

  let activeSocket: WebSocket | null = null
  let activeUrl = ''
  let currentStatus: ResilientWS['status'] = 'CLOSED'
  let closedByUser = false
  const listeners = new Map<string, Set<EventListener>>()

  function emit(type: string, event?: Event) {
    listeners.get(type)?.forEach((fn) => fn(event ?? new Event(type)))
  }

  function connect(): void {
    if (closedByUser) return
    currentStatus = 'CONNECTING'
    emit('statuschange')

    if (urls.length > 1) {
      // Multi-endpoint racing
      raceEndpoints(urls.slice(0, raceCount), wsOptions).then(({ socket, endpoint }) => {
        activeSocket = socket
        activeUrl = endpoint
        setupSocket(socket)
      }).catch(() => {
        // All failed — schedule reconnect
        currentStatus = 'CLOSED'
        emit('statuschange')
        if (!closedByUser) {
          const delay = reconnect.nextDelay()
          if (delay !== -1) setTimeout(connect, delay)
        }
      })
    } else {
      // Single endpoint
      const socket = new WebSocket(urls[0], wsOptions)
      activeSocket = socket
      activeUrl = urls[0]
      setupSocket(socket)
    }
  }

  function setupSocket(socket: WebSocket): void {
    const connectTimer = setTimeout(() => {
      socket.close(4000, 'Handshake timeout')
    }, handshakeTimeout)

    // If socket is already open (from raceEndpoints), fire open immediately
    if (socket.readyState === WebSocket.OPEN) {
      clearTimeout(connectTimer)
      reconnect.reset()
      currentStatus = 'CONNECTED'
      emit('open')
      emit('statuschange')
    } else {
      socket.on('open', () => {
        clearTimeout(connectTimer)
        reconnect.reset()
        currentStatus = 'CONNECTED'
        emit('open')
        emit('statuschange')
      })
    }

    socket.on('message', (data) => {
      emit('message', new MessageEvent('message', { data }))
    })

    socket.on('close', (code, reason) => {
      clearTimeout(connectTimer)
      currentStatus = 'CLOSED'
      if (activeSocket === socket) {
        // Node.js does not have CloseEvent — use a custom event with code + reason
        const event = new Event('close') as Event & { code: number; reason: string }
        ;(event as any).code = code
        ;(event as any).reason = reason.toString()
        emit('close', event)
        emit('statuschange')
        if (!closedByUser) {
          const delay = reconnect.nextDelay()
          if (delay === -1) return // maxAttempts exceeded
          setTimeout(connect, delay)
        }
      }
    })

    socket.on('error', (err) => {
      clearTimeout(connectTimer)
      // ErrorEvent is a browser API not available in Node.js — use a custom event
      const event = new Event('error') as Event & { error: Error }
      ;(event as any).error = err
      emit('error', event)
    })
  }

  // Start first connection
  connect()

  return {
    send(data) {
      if (activeSocket?.readyState === WebSocket.OPEN) {
        activeSocket.send(data)
      }
    },
    close(code, reason) {
      closedByUser = true
      reconnect.reset()
      activeSocket?.close(code, reason)
    },
    get readyState() { return activeSocket?.readyState ?? WebSocket.CLOSED },
    get url() { return activeUrl },
    get status() { return currentStatus },
    addEventListener(type, listener) {
      if (!listeners.has(type)) listeners.set(type, new Set())
      listeners.get(type)!.add(listener)
    },
    removeEventListener(type, listener) {
      listeners.get(type)?.delete(listener)
    },
    dispatchEvent(event: Event): boolean {
      emit(event.type, event)
      return true
    },
  }
}
