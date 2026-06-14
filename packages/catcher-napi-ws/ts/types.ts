// ── napi-ws 配置 + 事件类型 ──

/** 重连配置 — 对应 Rust ReconnectConfig */
export interface ReconnectConfig {
  /** 初始退避延迟（ms）。默认: 500 */
  initial_delay_ms?: number
  /** 最大退避延迟（ms）。默认: 30000 */
  max_delay_ms?: number
  /** 退避乘数。默认: 2.0 */
  backoff_multiplier?: number
  /** 最大重试次数。默认: 20 */
  max_attempts?: number
}

/** 心跳配置 — 对应 Rust HeartbeatConfig */
export interface HeartbeatConfig {
  /** 心跳间隔（ms）。默认: 30000 */
  interval_ms?: number
  /** 是否根据 RTT 自适应调整。默认: true */
  adaptive?: boolean
  /** pong 超时（ms）。默认: 10000 */
  pong_timeout_ms?: number
  /** 连续丢失多少个 pong 判定断线。默认: 3 */
  max_missed_pongs?: number
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
  /** 过期后仍可用的宽限期（秒）。默认: 3600 */
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

/** TLS 版本 */
export type TlsVersion = 'Tls1_0' | 'Tls1_1' | 'Tls1_2' | 'Tls1_3'

/** TLS 配置 — 对应 Rust TlsConfig */
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
  /** TLS SNI 覆写 */
  tls_sni_override?: string
  /** 最低 TLS 版本 */
  min_tls_version?: TlsVersion
  /** 最高 TLS 版本 */
  max_tls_version?: TlsVersion
  /** SHA-256 公钥指纹 pinning */
  pin_sha256?: string[]
}

/** 代理认证 */
export interface ProxyAuth {
  username: string
  password: string
}

/**
 * 代理配置 — 对应 Rust ProxyConfig（判别联合，按 `mode` 区分）。
 *
 * - Manual（默认，省略 `mode` 或 `mode: 'manual'`）：必须提供 `url`。
 * - System（`mode: 'system'`）：自动从 OS 读取系统代理（需构建时启用 `system-proxy`
 *   feature），`url` 可省略。注意：System 模式仅在调用 `networkChanged()` 后才会
 *   探测并应用系统代理，首次构建时走直连。
 */
export type ProxyConfig = ManualProxyConfig | SystemProxyConfig

/** 手动代理（默认）。`url` 必填。 */
export interface ManualProxyConfig {
  mode?: 'manual'
  /** 代理 URL。Catcher 会把 socks5:// 按 socks5h:// 处理，避免代理场景提前本地解析域名。 */
  url: string
  auth?: ProxyAuth
  /** 不走代理的 hostname 列表 */
  no_proxy?: string[]
}

/** 系统代理：自动从 OS 检测（需构建时启用 `system-proxy` feature）。 */
export interface SystemProxyConfig {
  mode: 'system'
  /** 忽略；系统代理由 `detect_system_proxy()` 在 `networkChanged()` 时解析。 */
  url?: string
  auth?: ProxyAuth
  /** 不走代理的 hostname 列表 */
  no_proxy?: string[]
}

/**
 * WebSocket 客户端配置 — 对应 Rust WsClientConfig
 */
export interface WsClientConfig {
  /** 端点 URL 列表（多端点竞速） */
  urls: string[]
  /** 子协议 */
  protocols?: string[]
  /** 自定义 headers */
  headers?: Record<string, string>
  /** 启用 perMessageDeflate 压缩。默认: true */
  per_message_deflate?: boolean
  /** 压缩阈值（字节）。默认: 1024 */
  deflate_threshold_bytes?: number
  /** 握手超时（ms）。默认: 15000 */
  handshake_timeout_ms?: number
  /** 单帧发送超时（ms，0 = 不限制）。半开连接上的发送超时后判定断线并重连。默认: 10000 */
  send_timeout_ms?: number
  /** 最大 payload（字节）。默认: 64MB */
  max_payload_bytes?: number
  /** 重连配置 */
  reconnect?: ReconnectConfig
  /** 心跳配置 */
  heartbeat?: HeartbeatConfig
  /** TLS 配置 */
  tls?: TlsConfig
  /** 代理配置 */
  proxy?: ProxyConfig
  /** 同时竞速端点数。默认: 1 */
  race_count?: number
  /** DNS 配置 */
  dns?: DnsConfig
  /** 启用 msgpack 编解码 — send 自动 JSON→msgpack, receive 自动 msgpack→JSON. 默认 false */
  msgpack?: boolean
}

/** WebSocket 事件 — 所有回调参数的联合类型 */
export type WsEvent =
  | { type: 'Connected'; url: string; latency_ms: number }
  | { type: 'Disconnected'; code: number; reason: string }
  // data 为 base64 编码，字段名与 Rust WsEvent::to_ffi_json() 一致
  | { type: 'Message'; data_base64: string; is_binary: boolean }
  | { type: 'Error'; message: string }
  | { type: 'Reconnecting'; attempt: number; delay_ms: number }
  | { type: 'HeartbeatRtt'; rtt_ms: number }
