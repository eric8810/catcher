import 'dart:ffi';

// ═══════════════════════════════════════════════════════════════
// C ABI types — must match Rust #[repr(C)] structs exactly
// ═══════════════════════════════════════════════════════════════

/// FFI-safe string — matches Rust `FfiString { data: *const c_char, len: usize }`
final class FfiStringNative extends Struct {
  external Pointer<Char> data;

  @Size()
  external int len;
}

/// FFI-safe result — matches Rust `FfiResult { error_code, error_message, data, data_len }`
final class FfiResultNative extends Struct {
  @Int32()
  external int errorCode;

  external Pointer<Char> errorMessage;

  external Pointer<Void> data;

  @Size()
  external int dataLen;
}

// ═══════════════════════════════════════════════════════════════
// Event callback — matches Rust `EventCallback`
//   extern "C" fn(event_type: *const c_char, event_data: *const u8, len: usize, user_data: *mut c_void)
// ═══════════════════════════════════════════════════════════════

typedef EventCallbackNative = Void Function(
  Pointer<Char> eventType,
  Pointer<Uint8> eventData,
  Size eventDataLen,
  Pointer<Void> userData,
);

typedef EventCallbackDart = void Function(
  Pointer<Char> eventType,
  Pointer<Uint8> eventData,
  int eventDataLen,
  Pointer<Void> userData,
);

// ═══════════════════════════════════════════════════════════════
// HTTP client — create / destroy
// ═══════════════════════════════════════════════════════════════

typedef CatcherHttpClientCreateNative = Pointer<Void> Function(
  Pointer<Char> configJson,
);
typedef CatcherHttpClientCreateDart = Pointer<Void> Function(
  Pointer<Char> configJson,
);

typedef CatcherHttpClientDestroyNative = Void Function(
  Pointer<Void> handle,
);
typedef CatcherHttpClientDestroyDart = void Function(
  Pointer<Void> handle,
);

// ═══════════════════════════════════════════════════════════════
// HTTP request — async callback-based (matches Rust http_ffi.rs)
// ═══════════════════════════════════════════════════════════════

/// catcher_http_get(handle, url: FfiString, headers_json, timeout_ms, callback, user_data)
typedef CatcherHttpGetNative = Void Function(
  Pointer<Void> handle,
  FfiStringNative url,
  Pointer<Char> headersJson,
  Uint32 timeoutMs,
  Pointer<NativeFunction<EventCallbackNative>> callback,
  Pointer<Void> userData,
);
typedef CatcherHttpGetDart = void Function(
  Pointer<Void> handle,
  FfiStringNative url,
  Pointer<Char> headersJson,
  int timeoutMs,
  Pointer<NativeFunction<EventCallbackNative>> callback,
  Pointer<Void> userData,
);

/// catcher_http_post(handle, url: FfiString, body, body_len, content_type: FfiString, headers_json, timeout_ms, callback, user_data)
typedef CatcherHttpPostNative = Void Function(
  Pointer<Void> handle,
  FfiStringNative url,
  Pointer<Uint8> body,
  Size bodyLen,
  FfiStringNative contentType,
  Pointer<Char> headersJson,
  Uint32 timeoutMs,
  Pointer<NativeFunction<EventCallbackNative>> callback,
  Pointer<Void> userData,
);
typedef CatcherHttpPostDart = void Function(
  Pointer<Void> handle,
  FfiStringNative url,
  Pointer<Uint8> body,
  int bodyLen,
  FfiStringNative contentType,
  Pointer<Char> headersJson,
  int timeoutMs,
  Pointer<NativeFunction<EventCallbackNative>> callback,
  Pointer<Void> userData,
);

// ═══════════════════════════════════════════════════════════════
// Generic HTTP execute — method as FfiString (supports GET/POST/PUT/DELETE/PATCH)
// ═══════════════════════════════════════════════════════════════

/// catcher_http_execute(handle, method: FfiString, url: FfiString, body, body_len, content_type: FfiString, headers_json, timeout_ms, callback, user_data)
typedef CatcherHttpExecuteNative = Void Function(
  Pointer<Void> handle,
  FfiStringNative method,
  FfiStringNative url,
  Pointer<Uint8> body,
  Size bodyLen,
  FfiStringNative contentType,
  Pointer<Char> headersJson,
  Uint32 timeoutMs,
  Pointer<NativeFunction<EventCallbackNative>> callback,
  Pointer<Void> userData,
);
typedef CatcherHttpExecuteDart = void Function(
  Pointer<Void> handle,
  FfiStringNative method,
  FfiStringNative url,
  Pointer<Uint8> body,
  int bodyLen,
  FfiStringNative contentType,
  Pointer<Char> headersJson,
  int timeoutMs,
  Pointer<NativeFunction<EventCallbackNative>> callback,
  Pointer<Void> userData,
);

// ═══════════════════════════════════════════════════════════════
// WebSocket — create / send / close / destroy
// ═══════════════════════════════════════════════════════════════

/// catcher_ws_create(config_json, event_callback, user_data) → handle
typedef CatcherWsCreateNative = Pointer<Void> Function(
  Pointer<Char> configJson,
  Pointer<NativeFunction<EventCallbackNative>> callback,
  Pointer<Void> userData,
);
typedef CatcherWsCreateDart = Pointer<Void> Function(
  Pointer<Char> configJson,
  Pointer<NativeFunction<EventCallbackNative>> callback,
  Pointer<Void> userData,
);

/// catcher_ws_send_text(handle, message: FfiString) → FfiResult
typedef CatcherWsSendTextNative = FfiResultNative Function(
  Pointer<Void> handle,
  FfiStringNative message,
);
typedef CatcherWsSendTextDart = FfiResultNative Function(
  Pointer<Void> handle,
  FfiStringNative message,
);

/// catcher_ws_send_binary(handle, data, len) → FfiResult
typedef CatcherWsSendBinaryNative = FfiResultNative Function(
  Pointer<Void> handle,
  Pointer<Uint8> data,
  Size len,
);
typedef CatcherWsSendBinaryDart = FfiResultNative Function(
  Pointer<Void> handle,
  Pointer<Uint8> data,
  int len,
);

/// catcher_ws_close(handle, code, reason: FfiString)
typedef CatcherWsCloseNative = Void Function(
  Pointer<Void> handle,
  Uint16 code,
  FfiStringNative reason,
);
typedef CatcherWsCloseDart = void Function(
  Pointer<Void> handle,
  int code,
  FfiStringNative reason,
);

/// catcher_ws_network_changed(handle) → FfiResult
/// 通知网络环境变化：立即断开重连，跳过退避延迟
typedef CatcherWsNetworkChangedNative = FfiResultNative Function(
  Pointer<Void> handle,
);
typedef CatcherWsNetworkChangedDart = FfiResultNative Function(
  Pointer<Void> handle,
);

/// catcher_ws_destroy(handle)
typedef CatcherWsDestroyNative = Void Function(Pointer<Void> handle);
typedef CatcherWsDestroyDart = void Function(Pointer<Void> handle);

/// catcher_free_result(result) — frees FfiResult.error_message CString
typedef CatcherFreeResultNative = Void Function(FfiResultNative result);
typedef CatcherFreeResultDart = void Function(FfiResultNative result);

/// catcher_free_event_data(event_type, event_data) — frees CStrings
/// allocated by Rust via CString::into_raw() for the async callback bridge.
/// Dart must call this after reading the callback data.
typedef CatcherFreeEventDataNative = Void Function(
  Pointer<Char> eventType,
  Pointer<Uint8> eventData,
);
typedef CatcherFreeEventDataDart = void Function(
  Pointer<Char> eventType,
  Pointer<Uint8> eventData,
);

// ═════════════════════════════════════════════════════════════════
// Codec — pack / unpack
// ═══════════════════════════════════════════════════════════════

typedef CatcherPackNative = FfiResultNative Function(Pointer<Char> jsonInput);
typedef CatcherPackDart = FfiResultNative Function(Pointer<Char> jsonInput);

typedef CatcherUnpackNative = FfiResultNative Function(
  Pointer<Uint8> data,
  Size len,
);
typedef CatcherUnpackDart = FfiResultNative Function(
  Pointer<Uint8> data,
  int len,
);

// ═══════════════════════════════════════════════════════════════════
// Data free — catcher_free_data
// ═══════════════════════════════════════════════════════════════════

/// catcher_free_data(data, len) — frees data allocated by catcher_pack/unpack
typedef CatcherFreeDataNative = Void Function(Pointer<Void> data, Size len);
typedef CatcherFreeDataDart = void Function(Pointer<Void> data, int len);

// ═══════════════════════════════════════════════════════════════════
// Network quality — evaluate_quality
// ═══════════════════════════════════════════════════════════════════

/// catcher_evaluate_quality(host: FfiString, callback, user_data)
typedef CatcherEvaluateQualityNative = Void Function(
  FfiStringNative host,
  Pointer<NativeFunction<EventCallbackNative>> callback,
  Pointer<Void> userData,
);
typedef CatcherEvaluateQualityDart = void Function(
  FfiStringNative host,
  Pointer<NativeFunction<EventCallbackNative>> callback,
  Pointer<Void> userData,
);

/// catcher_quality_history() → *mut c_char (JSON, caller frees via catcher_free_data)
typedef CatcherQualityHistoryNative = Pointer<Char> Function();
typedef CatcherQualityHistoryDart = Pointer<Char> Function();

// ═══════════════════════════════════════════════════════════════════
// HTTP runtime control — cancel / circuit breaker state / metrics
// ═══════════════════════════════════════════════════════════════════

/// catcher_http_client_cancel_all(handle)
typedef CatcherHttpClientCancelAllNative = Void Function(Pointer<Void> handle);
typedef CatcherHttpClientCancelAllDart = void Function(Pointer<Void> handle);

/// catcher_http_network_changed(handle) → i32 (0=success, 1=invalid handle, 2=rebuild failed)
/// 通知网络环境变化：清 DNS 缓存、重建连接池、重置熔断器
typedef CatcherHttpNetworkChangedNative = Int32 Function(Pointer<Void> handle);
typedef CatcherHttpNetworkChangedDart = int Function(Pointer<Void> handle);

/// catcher_http_circuit_breaker_state(handle) → *mut c_char (JSON, caller frees via catcher_free_data)
typedef CatcherHttpCircuitBreakerStateNative = Pointer<Char> Function(
  Pointer<Void> handle,
);
typedef CatcherHttpCircuitBreakerStateDart = Pointer<Char> Function(
  Pointer<Void> handle,
);

/// catcher_http_metrics(handle) → *mut c_char (JSON, caller frees via catcher_free_data)
typedef CatcherHttpMetricsNative = Pointer<Char> Function(
  Pointer<Void> handle,
);
typedef CatcherHttpMetricsDart = Pointer<Char> Function(
  Pointer<Void> handle,
);

/// catcher_http_adaptive_timeout_config(handle, enabled, min_ms, max_ms, multiplier*1000, window_size)
typedef CatcherHttpAdaptiveTimeoutConfigNative = Void Function(
  Pointer<Void> handle,
  Int32 enabled,
  Uint32 minTimeoutMs,
  Uint32 maxTimeoutMs,
  Uint32 multiplierScaled,
  Uint32 windowSize,
);
typedef CatcherHttpAdaptiveTimeoutConfigDart = void Function(
  Pointer<Void> handle,
  int enabled,
  int minTimeoutMs,
  int maxTimeoutMs,
  int multiplierScaled,
  int windowSize,
);

// ═══════════════════════════════════════════════════════════════════
// SSE — persistent client + one-shot stream
// ═══════════════════════════════════════════════════════════════════

/// catcher_sse_connect(config_json, event_callback, user_data) → handle
typedef CatcherSseConnectNative = Pointer<Void> Function(
  Pointer<Char> configJson,
  Pointer<NativeFunction<EventCallbackNative>> callback,
  Pointer<Void> userData,
);
typedef CatcherSseConnectDart = Pointer<Void> Function(
  Pointer<Char> configJson,
  Pointer<NativeFunction<EventCallbackNative>> callback,
  Pointer<Void> userData,
);

/// catcher_sse_stream(handle, method: FfiString, url: FfiString, body, body_len, headers_json, callback, user_data)
typedef CatcherSseStreamNative = Void Function(
  Pointer<Void> handle,
  FfiStringNative method,
  FfiStringNative url,
  Pointer<Uint8> body,
  Size bodyLen,
  Pointer<Char> headersJson,
  Pointer<NativeFunction<EventCallbackNative>> callback,
  Pointer<Void> userData,
);
typedef CatcherSseStreamDart = void Function(
  Pointer<Void> handle,
  FfiStringNative method,
  FfiStringNative url,
  Pointer<Uint8> body,
  int bodyLen,
  Pointer<Char> headersJson,
  Pointer<NativeFunction<EventCallbackNative>> callback,
  Pointer<Void> userData,
);

/// catcher_sse_ready_state(sse_handle) → i32 (0=Connecting, 1=Open, 2=Closed, -1=Invalid)
typedef CatcherSseReadyStateNative = Int32 Function(Pointer<Void> handle);
typedef CatcherSseReadyStateDart = int Function(Pointer<Void> handle);

/// catcher_sse_last_event_id(sse_handle) → *mut c_char (caller frees via catcher_free_data)
typedef CatcherSseLastEventIdNative = Pointer<Char> Function(Pointer<Void> handle);
typedef CatcherSseLastEventIdDart = Pointer<Char> Function(Pointer<Void> handle);

/// catcher_sse_close(sse_handle)
typedef CatcherSseCloseNative = Void Function(Pointer<Void> handle);
typedef CatcherSseCloseDart = void Function(Pointer<Void> handle);

/// catcher_sse_destroy(sse_handle)
typedef CatcherSseDestroyNative = Void Function(Pointer<Void> handle);
typedef CatcherSseDestroyDart = void Function(Pointer<Void> handle);

// ═══════════════════════════════════════════════════════════════════
// Streaming download (N-02) — catcher_http_execute_stream
// ═══════════════════════════════════════════════════════════════════

/// catcher_http_execute_stream(handle, method, url, body, body_len, content_type, headers_json, timeout_ms, callback, user_data) → request_id (u64)
typedef CatcherHttpExecuteStreamNative = Uint64 Function(
  Pointer<Void> handle,
  FfiStringNative method,
  FfiStringNative url,
  Pointer<Uint8> body,
  Size bodyLen,
  FfiStringNative contentType,
  Pointer<Char> headersJson,
  Uint32 timeoutMs,
  Pointer<NativeFunction<EventCallbackNative>> callback,
  Pointer<Void> userData,
);
typedef CatcherHttpExecuteStreamDart = int Function(
  Pointer<Void> handle,
  FfiStringNative method,
  FfiStringNative url,
  Pointer<Uint8> body,
  int bodyLen,
  FfiStringNative contentType,
  Pointer<Char> headersJson,
  int timeoutMs,
  Pointer<NativeFunction<EventCallbackNative>> callback,
  Pointer<Void> userData,
);

// ═══════════════════════════════════════════════════════════════════
// Per-request cancel (N-03) — execute_with_id + cancel_request
// ═══════════════════════════════════════════════════════════════════

/// catcher_http_execute_with_id(handle, method, url, body, body_len, content_type, headers_json, timeout_ms, callback, user_data) → request_id (u64)
typedef CatcherHttpExecuteWithIdNative = Uint64 Function(
  Pointer<Void> handle,
  FfiStringNative method,
  FfiStringNative url,
  Pointer<Uint8> body,
  Size bodyLen,
  FfiStringNative contentType,
  Pointer<Char> headersJson,
  Uint32 timeoutMs,
  Pointer<NativeFunction<EventCallbackNative>> callback,
  Pointer<Void> userData,
);
typedef CatcherHttpExecuteWithIdDart = int Function(
  Pointer<Void> handle,
  FfiStringNative method,
  FfiStringNative url,
  Pointer<Uint8> body,
  int bodyLen,
  FfiStringNative contentType,
  Pointer<Char> headersJson,
  int timeoutMs,
  Pointer<NativeFunction<EventCallbackNative>> callback,
  Pointer<Void> userData,
);

/// catcher_http_cancel_request(handle, request_id) → i32 (0=success, -1=not found)
typedef CatcherHttpCancelRequestNative = Int32 Function(
  Pointer<Void> handle,
  Uint64 requestId,
);
typedef CatcherHttpCancelRequestDart = int Function(
  Pointer<Void> handle,
  int requestId,
);
