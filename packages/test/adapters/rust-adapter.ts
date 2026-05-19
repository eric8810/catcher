/**
 * Rust adapter — wraps catcher napi-rs bindings to match
 * the existing test harness interface, enabling direct
 * vanilla (axios/ws) vs Rust comparison.
 *
 * Signatures match the existing TS catcher implementation,
 * so tests can swap imports with minimal changes.
 */
import { HttpClient } from '@eric8810/catcher-napi-http'
import { WsClient } from '@eric8810/catcher-napi-ws'
import { pack, unpack } from '@eric8810/catcher-ws'

// ── HTTP ────────────────────────────────────────────────────

export interface RustHttpConfig {
  baseURL: string
  keepAlive: boolean
  dnsCacheTtl?: number
  dnsNegativeTtl?: number
  dnsStaleTtl?: number
  dnsStaleOnError?: boolean
  dnsCacheSize?: number
  dnsNameservers?: string[]
  dnsHostMapping?: Record<string, string>
  retry?: { attempts: number; backoff?: string }
  timeout: { response: number }
  concurrency?: number
}

export function createRustHttpClient(config: RustHttpConfig) {
  const hasDns = config.dnsCacheTtl != null || config.dnsNegativeTtl != null
    || config.dnsStaleTtl != null || config.dnsStaleOnError != null
    || config.dnsCacheSize != null || config.dnsNameservers != null
    || config.dnsHostMapping != null
  const dnsConfig = hasDns
    ? {
        cache_size: config.dnsCacheSize ?? 512,
        cache_ttl_secs: config.dnsCacheTtl ?? 300,
        negative_ttl_secs: config.dnsNegativeTtl ?? 60,
        stale_ttl_secs: config.dnsStaleTtl ?? 3600,
        stale_on_error: config.dnsStaleOnError ?? true,
        nameservers: config.dnsNameservers ?? [],
        host_mapping: config.dnsHostMapping ?? {},
      }
    : undefined

  const inner = new HttpClient(JSON.stringify({
    base_url: config.baseURL,
    connect_timeout_ms: 5000,
    response_timeout_ms: config.timeout.response,
    pool: {
      keep_alive: config.keepAlive,
      keep_alive_interval_secs: 60,
      max_idle_per_host: 10,
      idle_timeout_secs: 90,
    },
    dns: dnsConfig ?? null,
    retry: config.retry
      ? {
          max_attempts: config.retry.attempts,
          backoff: mapBackoff(config.retry.backoff ?? 'exponential'),
          min_backoff_ms: 100,
          max_backoff_ms: 10_000,
          jitter: true,
        }
      : null,
    circuit_breaker: null,
    max_concurrency: config.concurrency ?? 50,
  }))

  const debugError = (method: string, path: string, error: unknown) => {
    if (process.env.DEBUG_RUST_E2E === '1') {
      const message = error instanceof Error ? error.message : String(error)
      console.error(`[rust-adapter] ${method} ${path} failed: ${message}`)
    }
  }

  // Per-request counters — reset before each request, readable after
  let lastBytes = 0
  // Cumulative retry count from the Rust MetricsCollector (monotonically increasing)
  let lastKnownRetries = 0

  return {
    /** Bytes received in the last request (response body length) */
    get lastBytes() { return lastBytes },
    /**
     * Cumulative number of retries since client creation.
     * Read from `MetricsCollector::http_retries` after each request.
     */
    get retryCount() {
      const snap = inner.metrics?.()
      if (snap) lastKnownRetries = snap.http_retries ?? lastKnownRetries
      return lastKnownRetries
    },
    async get(path: string): Promise<any> {
      try {
        const resp = await inner.get(path)
        lastBytes = resp.body.length
        // Sync retryCount from metrics
        const snap = inner.metrics?.()
        if (snap) lastKnownRetries = snap.http_retries ?? lastKnownRetries
        if (resp.status >= 400) throw new Error(`HTTP ${resp.status}`)
        const body = Buffer.from(resp.body).toString('utf-8')
        try { return JSON.parse(body) } catch { return body }
      } catch (e) {
        lastBytes = 0
        // Sync retryCount from metrics even on failure
        const snap = inner.metrics?.()
        if (snap) lastKnownRetries = snap.http_retries ?? lastKnownRetries
        debugError('GET', path, e)
        throw e
      }
    },
    async post(path: string, body: unknown): Promise<any> {
      try {
        const json = JSON.stringify(body)
        const reqBytes = Buffer.byteLength(json)
        const resp = await inner.post(path, Buffer.from(json), { content_type: 'application/json' })
        lastBytes = reqBytes + resp.body.length
        // Sync retryCount from metrics
        const snap = inner.metrics?.()
        if (snap) lastKnownRetries = snap.http_retries ?? lastKnownRetries
        if (resp.status >= 400) throw new Error(`HTTP ${resp.status}`)
        const raw = Buffer.from(resp.body).toString('utf-8')
        try { return JSON.parse(raw) } catch { return raw }
      } catch (e) {
        lastBytes = 0
        // Sync retryCount from metrics even on failure
        const snap = inner.metrics?.()
        if (snap) lastKnownRetries = snap.http_retries ?? lastKnownRetries
        debugError('POST', path, e)
        throw e
      }
    },
  }
}

// ── WebSocket ────────────────────────────────────────────────

export function createRustWsClient(config: {
  url: string
  perMessageDeflate?: boolean
  handshakeTimeout?: number
  reconnect?: { maxAttempts?: number }
}) {
  const listeners: Record<string, Array<(...args: any[]) => void>> = {
    open: [],
    message: [],
    close: [],
    error: [],
  }

  // Promise that resolves when Connected event fires (or timeout)
  let onReady: (() => void) | null = null
  const ready = new Promise<void>((resolve) => { onReady = resolve })

  const onEvent = (eventJson: string) => {
    try {
      const event = JSON.parse(eventJson)
      switch (event.type) {
        case 'Connected':
          onReady?.()
          listeners.open.forEach((fn) => fn())
          break
        case 'Disconnected':
          listeners.close.forEach((fn) => fn({ code: event.code, reason: event.reason }))
          break
        case 'Message':
          listeners.message.forEach((fn) =>
            fn({ data: event.data ?? '' })
          )
          break
        case 'Error':
          listeners.error.forEach((fn) => fn({ message: event.message }))
          break
      }
    } catch { /* ignore parse errors */ }
  }

  const ws = new WsClient(JSON.stringify({
    urls: [config.url],
    per_message_deflate: config.perMessageDeflate ?? false,
    handshake_timeout_ms: config.handshakeTimeout ?? 15_000,
    reconnect: config.reconnect
      ? {
          max_attempts: config.reconnect.maxAttempts ?? 0,
          initial_delay_ms: 500,
          max_delay_ms: 15_000,
          backoff_multiplier: 2,
        }
      : null,
    race_count: 1,
  }), onEvent)

  // Timeout safeguard: resolve after handshakeTimeout + 2000ms even if not connected
  const timeout = setTimeout(() => onReady?.(), (config.handshakeTimeout ?? 15_000) + 2000)

  return {
    ready,
    addEventListener(
      event: string,
      handler: (...args: any[]) => void
    ) {
      if (listeners[event]) listeners[event].push(handler)
    },
    send(data: any) {
      const str = typeof data === 'string' ? data : JSON.stringify(data)
      ws.send(str)
    },
    close() {
      clearTimeout(timeout)
      ws.close()
    },
  }
}

// ── Codec ────────────────────────────────────────────────────

export function rustPack(value: unknown): Buffer {
  return pack(value)
}

export function rustUnpack(buffer: Buffer): any {
  return JSON.parse(unpack(buffer))
}

// ── Helpers ──────────────────────────────────────────────────

function mapBackoff(b: string): string {
  switch (b) {
    case 'fixed':
      return 'Fixed'
    case 'exponential':
      return 'Exponential'
    default:
      return 'DecorrelatedJitter'
  }
}
