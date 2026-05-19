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
  /** 最大 payload（字节）。默认: 64MB */
  max_payload_bytes?: number
  /** 重连配置 */
  reconnect?: ReconnectConfig
  /** 心跳配置 */
  heartbeat?: HeartbeatConfig
  /** 同时竞速端点数。默认: 1 */
  race_count?: number
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
