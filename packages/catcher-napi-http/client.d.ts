declare module '@eric8810/catcher-napi-http' {
  interface HttpClientConfig {
    base_url?: string
    connect_timeout_ms?: number
    response_timeout_ms?: number
    pool?: {
      keep_alive?: boolean
      max_idle_per_host?: number
      idle_timeout_secs?: number
      keep_alive_interval_secs?: number
    }
    tls?: {
      reject_unauthorized?: boolean
      ca_cert_pem?: string
      client_cert_pem?: string
      client_key_pem?: string
    }
    dns?: {
      cache_ttl_secs?: number
      nameservers?: string[]
    }
    retry?: {
      max_attempts?: number
      backoff?: 'Fixed' | 'Exponential' | 'DecorrelatedJitter'
      min_backoff_ms?: number
      max_backoff_ms?: number
      jitter?: boolean
    }
    circuit_breaker?: {
      failure_threshold?: number
      success_threshold?: number
      reset_timeout_ms?: number
      half_open_max_requests?: number
    }
    max_concurrency?: number
    default_headers?: Record<string, string>
    hostname_override?: string
  }

  interface RequestOptions {
    headers?: Record<string, string>
    timeout_ms?: number
    content_type?: string
  }

  interface HttpResponse {
    status: number
    headers: Record<string, string>
    body: Buffer
    elapsed_ms: number
  }

  interface Metrics {
    http_requests: number
    http_success_rate: number
    http_avg_latency_us: number
    http_retries: number
    ws_connect_success_rate: number
    ws_disconnects: number
    ws_messages_sent: number
    ws_messages_received: number
    cb_open_count: number
    queue_timeouts: number
  }

  export class HttpClient {
    constructor(config: string | HttpClientConfig)
    get(url: string, options?: RequestOptions): Promise<HttpResponse>
    post(url: string, body?: Buffer, options?: RequestOptions): Promise<HttpResponse>
    put(url: string, body?: Buffer, options?: RequestOptions): Promise<HttpResponse>
    delete(url: string, options?: RequestOptions): Promise<HttpResponse>
    patch(url: string, body?: Buffer, options?: RequestOptions): Promise<HttpResponse>
    circuitBreakerState(): 'closed' | 'open' | 'half-open'
    metrics(): Metrics
    setAdaptiveTimeout(minTimeoutMs: number, maxTimeoutMs: number, multiplier: number, windowSize: number): void
    disableAdaptiveTimeout(): void
    cancelAll(): void
    cancelRequest(requestId: number): boolean
    nextRequestId(): number
    executeStream(
      method: string,
      url: string,
      body?: Buffer,
      options?: RequestOptions,
      onChunk?: (eventJson: string) => void,
    ): void
  }
}
