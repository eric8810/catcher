// ── napi-http 配置 + 事件类型 ──
//
// #[napi(object)] 类型从自动生成的 index.d.ts re-export，不手写。
// JSON 配置类型仍需手写（serde 反序列化，NAPI-RS 不管）。

// ── NAPI-RS 自动生成的对象类型（re-export） ──
// index.d.ts 由 NAPI-RS 生成，字段名已是正确 camelCase。
export type {
  RequestOptions,
  JsHttpResponse as HttpResponse,
  JsMetrics as Metrics,
} from '../index.d'

// ── JSON 配置类型（通过 JSON.stringify 传给构造函数，serde 反序列化） ──

/** 连接池配置 — 对应 Rust PoolConfig */
export interface PoolConfig {
  /** 是否启用 TCP keepalive。默认: true */
  keep_alive?: boolean
  /** 每 host 最大空闲连接数。默认: 10 */
  max_idle_per_host?: number
  /** 空闲连接超时（秒）。默认: 30 */
  idle_timeout_secs?: number
  /** keepalive 探测间隔（秒）。默认: 20 */
  keep_alive_interval_secs?: number
}

/** TLS 版本 */
export type TlsVersion = 'Tls1_0' | 'Tls1_1' | 'Tls1_2' | 'Tls1_3'

/** TLS 配置 — 对应 Rust TlsConfig（完整 14 字段） */
export interface TlsConfig {
  /** 是否验证服务端证书。默认: true */
  reject_unauthorized?: boolean
  /** CA 证书 PEM 内容 */
  ca_cert_pem?: string
  /** CA 证书文件路径 */
  ca_cert_path?: string
  /** 客户端证书 PEM 内容 */
  client_cert_pem?: string
  /** 客户端证书文件路径 */
  client_cert_path?: string
  /** 客户端私钥 PEM 内容 */
  client_key_pem?: string
  /** 客户端私钥文件路径 */
  client_key_path?: string
  /** PFX/PKCS12 客户端身份（二进制） */
  client_identity_pfx?: Uint8Array
  /** PFX 身份密码 */
  client_identity_password?: string
  /** TLS SNI 覆写 */
  tls_sni_override?: string
  /** 最低 TLS 版本 */
  min_tls_version?: TlsVersion
  /** 最高 TLS 版本 */
  max_tls_version?: TlsVersion
  /** SHA-256 公钥指纹 pinning（deferred） */
  pin_sha256?: string[]
}

/** DNS 配置 — 对应 Rust DnsConfig */
export interface DnsConfig {
  /** DNS 模式。默认: 'catcher'。仅显式 native 时使用协议库原生解析。 */
  mode?: 'catcher' | 'native'
  /** 缓存条目数上限。默认: 512 */
  cache_size?: number
  /** DNS 缓存 TTL（秒）。默认: 300 */
  cache_ttl_secs?: number
  /** 否定缓存 TTL（秒）。默认: 60 */
  negative_ttl_secs?: number
  /** 过期后仍可用的宽限期（秒）— stale-while-revalidate。默认: 3600 */
  stale_ttl_secs?: number
  /** DNS 失败时是否用旧缓存兜底。默认: true */
  stale_on_error?: boolean
  /** 自定义 DNS 服务器 */
  nameservers?: string[]
  /** Hostname → IP 映射 */
  host_mapping?: Record<string, string>
  /** 读取系统 DNS 失败时是否退回默认 DNS。默认: false */
  fallback_to_default_nameservers?: boolean
}

/** 退避策略 */
export type BackoffStrategy = 'Fixed' | 'Exponential' | 'DecorrelatedJitter'

/** 重试配置 — 对应 Rust RetryConfig */
export interface RetryConfig {
  /** 最大重试次数。默认: 3 */
  max_attempts?: number
  /**
   * 退避策略。默认: 'Fixed'
   */
  backoff?: BackoffStrategy
  /** 最小退避延迟（ms）。默认: 100 */
  min_backoff_ms?: number
  /** 最大退避延迟（ms）。默认: 10000 */
  max_backoff_ms?: number
  /** 是否加入随机抖动。默认: true */
  jitter?: boolean
}

/** 熔断器配置 — 对应 Rust CircuitBreakerConfig */
export interface CircuitBreakerConfig {
  /** 连续失败多少次进入 OPEN。默认: 5 */
  failure_threshold?: number
  /** HALF_OPEN 连续成功多少次恢复 CLOSED。默认: 2 */
  success_threshold?: number
  /** OPEN → HALF_OPEN 等待时间（ms）。默认: 30000 */
  reset_timeout_ms?: number
  /** HALF_OPEN 状态最大放行请求数。默认: 5 */
  half_open_max_requests?: number
}

/** 代理认证 */
export interface ProxyAuth {
  username: string
  password: string
}

/** 代理配置 — 对应 Rust ProxyConfig */
export interface ProxyConfig {
  /** 代理 URL。Catcher 会把 socks5:// 按 socks5h:// 处理，避免代理场景提前本地解析域名。 */
  url: string
  auth?: ProxyAuth
  /** 不走代理的 hostname 列表 */
  no_proxy?: string[]
}

/** 重定向配置 — 对应 Rust RedirectConfig */
export interface RedirectConfig {
  /** 是否跟随重定向。默认: true */
  follow?: boolean
  /** 最大重定向次数。默认: 5 */
  max_redirects?: number
}

/**
 * HTTP 客户端配置 — 对应 Rust HttpClientConfig
 *
 * 所有可选字段有合理默认值，只需传入需要覆盖的字段。
 * 字段名使用 snake_case（与 Rust 一致，通过 serde alias 兼容 camelCase）。
 */
export interface HttpClientConfig {
  /** 基础 URL，会与请求路径拼接 */
  base_url?: string
  /** 连接超时（ms）。默认: 10000 */
  connect_timeout_ms?: number
  /** 响应超时（ms）。默认: 30000 */
  response_timeout_ms?: number
  /** 连接池配置 */
  pool?: PoolConfig
  /** TLS 配置 */
  tls?: TlsConfig
  /** DNS 配置 */
  dns?: DnsConfig
  /** 重试配置 */
  retry?: RetryConfig
  /** 熔断器配置 */
  circuit_breaker?: CircuitBreakerConfig
  /** 最大并发请求数。默认: 50 */
  max_concurrency?: number
  /** 默认请求头（每次请求自动携带） */
  default_headers?: Record<string, string>
  /** HTTP DNS 场景的 Hostname 覆写 */
  hostname_override?: string
  /** 代理配置 */
  proxy?: ProxyConfig
  /** 重定向配置 */
  redirect?: RedirectConfig
  /** Basic 认证 */
  auth?: ProxyAuth
  /** Bearer token */
  bearer_token?: string
  /** 启用 msgpack 编解码 — body 自动 JSON↔msgpack 转码. 默认 false */
  msgpack?: boolean
}

/** SSE 客户端配置 — 对应 Rust SseClientConfig */
export interface SseClientConfig {
  /** SSE 端点 URL */
  url: string
  /** HTTP 方法。默认: 'GET' */
  method?: 'GET' | 'POST'
  /** 请求头（如 Authorization） */
  headers?: Record<string, string>
  /** 请求体（POST 时使用） */
  body?: string
  /** 请求超时（ms）。默认: 30000 */
  timeout_ms?: number
  /** 自动重连配置 — 对应 Rust SseReconnectConfig */
  reconnect?: {
    /** 最大重试次数。默认: 10 */
    max_retries?: number
    /** 初始退避延迟（ms）。默认: 1000 */
    initial_delay_ms?: number
    /** 最大退避延迟（ms）。默认: 30000 */
    max_delay_ms?: number
    /** 退避乘数。默认: 2.0 */
    backoff_multiplier?: number
  }
  /** 熔断器配置 — 复用 CircuitBreakerConfig */
  circuit_breaker?: CircuitBreakerConfig
}

/** 流式下载事件 — executeStream 回调参数 */
export type StreamEvent =
  | { type: 'Headers'; status: number; headers: Record<string, string> }
  | { type: 'Chunk'; data: string }   // base64 编码
  | { type: 'Done' }
  | { type: 'Error'; message: string }

/** SSE 事件 */
export type SseEvent =
  | { type: 'Line'; data: string }
  | { type: 'Error'; message: string }
  | { type: 'End' }
