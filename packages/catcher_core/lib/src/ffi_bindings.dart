import 'dart:ffi';
import 'dart:typed_data';

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

/// catcher_http_get(handle, url: FfiString, callback, user_data)
typedef CatcherHttpGetNative = Void Function(
  Pointer<Void> handle,
  FfiStringNative url,
  Pointer<NativeFunction<EventCallbackNative>> callback,
  Pointer<Void> userData,
);
typedef CatcherHttpGetDart = void Function(
  Pointer<Void> handle,
  FfiStringNative url,
  Pointer<NativeFunction<EventCallbackNative>> callback,
  Pointer<Void> userData,
);

/// catcher_http_post(handle, url: FfiString, body, body_len, content_type: FfiString, callback, user_data)
typedef CatcherHttpPostNative = Void Function(
  Pointer<Void> handle,
  FfiStringNative url,
  Pointer<Uint8> body,
  Size bodyLen,
  FfiStringNative contentType,
  Pointer<NativeFunction<EventCallbackNative>> callback,
  Pointer<Void> userData,
);
typedef CatcherHttpPostDart = void Function(
  Pointer<Void> handle,
  FfiStringNative url,
  Pointer<Uint8> body,
  int bodyLen,
  FfiStringNative contentType,
  Pointer<NativeFunction<EventCallbackNative>> callback,
  Pointer<Void> userData,
);

// ═══════════════════════════════════════════════════════════════
// Generic HTTP execute — method as FfiString (supports GET/POST/PUT/DELETE/PATCH)
// ═══════════════════════════════════════════════════════════════

/// catcher_http_execute(handle, method: FfiString, url: FfiString, body, body_len, content_type: FfiString, callback, user_data)
typedef CatcherHttpExecuteNative = Void Function(
  Pointer<Void> handle,
  FfiStringNative method,
  FfiStringNative url,
  Pointer<Uint8> body,
  Size bodyLen,
  FfiStringNative contentType,
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
