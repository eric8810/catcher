/**
 * Metrics adapter for Rust napi bindings.
 *
 * When @catcher/napi-http exposes metrics via native functions,
 * they will be called here.
 */
export interface RustMetrics {
  httpRequests: number
  httpSuccessRate: number
  httpAvgLatencyUs: number
  httpRetries: number
  wsConnectSuccessRate: number
  wsDisconnects: number
  wsMessagesSent: number
  wsMessagesReceived: number
  cbOpenCount: number
  queueTimeouts: number
}

export function getRustMetrics(): RustMetrics | null {
  // TODO: when @catcher/napi-http exposes get_metrics_snapshot(),
  // call it here. For now, return null.
  return null
}
