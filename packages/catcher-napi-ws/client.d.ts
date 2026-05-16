declare module '@eric8810/catcher-napi-ws' {
  interface WsClientConfig {
    urls: string[]
    per_message_deflate?: boolean
    handshake_timeout_ms?: number
    max_payload_bytes?: number
    reconnect?: {
      initial_delay_ms?: number
      max_delay_ms?: number
      backoff_multiplier?: number
      max_attempts?: number
    }
    headers?: Record<string, string>
    reject_unauthorized?: boolean
  }

  interface WsEvent {
    type: 'Connected' | 'Disconnected' | 'Message' | 'Error'
    url?: string
    latency_ms?: number
    code?: number
    reason?: string
    data?: string
    is_binary?: boolean
    message?: string
  }

  export class WsClient {
    constructor(config: string | WsClientConfig, onEvent?: (eventJson: string) => void)
    /** Send a text message */
    send(data: string): void
    /** Send a binary message (ArrayBuffer or Buffer) */
    sendBinary(data: Buffer | ArrayBuffer | Uint8Array): void
    close(code?: number, reason?: string): void
  }
}
