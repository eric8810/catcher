import WebSocket from 'ws'
import type { ResilientWSOptions, ResilientWS } from '@catcher/core'
import { createReconnectStrategy } from './reconnect.js'
import { raceEndpoints } from './multi-endpoint.js'

/**
 * Create a resilient WebSocket client with:
 * - perMessageDeflate compression
 * - configurable handshake timeout
 * - exponential backoff reconnection
 * - optional multi-endpoint racing
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
    rejectUnauthorized = false,
  } = options

  const urls = Array.isArray(url) ? url : [url]
  const reconnect = createReconnectStrategy(reconnectOpts)

  // Build WebSocket constructor options
  const wsOptions: WebSocket.ClientOptions = {
    handshakeTimeout,
    maxPayload,
    rejectUnauthorized,
    headers,
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
  const listeners = new Map<string, Set<EventListener>>()

  function emit(type: string, event?: Event) {
    listeners.get(type)?.forEach((fn) => fn(event ?? new Event(type)))
  }

  function connect(): void {
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
        const delay = reconnect.nextDelay()
        setTimeout(connect, delay)
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
        emit('close', new CloseEvent('close', { code, reason: reason.toString() }))
        emit('statuschange')
        const delay = reconnect.nextDelay()
        setTimeout(connect, delay)
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
