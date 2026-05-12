import 'dart:ffi';

import 'ffi_types.dart';
import 'native_loader.dart';

// ── C function typedefs ──────────────────────────────────────

// HTTP
typedef CatcherHttpClientCreateNative = Pointer<Void> Function(
  Pointer<Char> configJson,
);
typedef CatcherHttpClientDestroyNative = Void Function(
  Pointer<Void> handle,
);
typedef CatcherHttpGetNative = Void Function(
  Pointer<Void> handle,
  Pointer<Char> url,
  Pointer<NativeFunction<EventCallbackNative>> callback,
  Pointer<Void> userData,
);
typedef CatcherHttpPostNative = Void Function(
  Pointer<Void> handle,
  Pointer<Char> url,
  Pointer<Uint8> body,
  Size bodyLen,
  Pointer<Char> contentType,
  Pointer<NativeFunction<EventCallbackNative>> callback,
  Pointer<Void> userData,
);

// WS
typedef CatcherWsCreateNative = Pointer<Void> Function(
  Pointer<Char> configJson,
  Pointer<NativeFunction<EventCallbackNative>> eventCallback,
  Pointer<Void> userData,
);
typedef CatcherWsSendTextNative = FfiResult Function(
  Pointer<Void> handle,
  Pointer<Char> message,
);
typedef CatcherWsSendBinaryNative = FfiResult Function(
  Pointer<Void> handle,
  Pointer<Uint8> data,
  Size len,
);
typedef CatcherWsCloseNative = Void Function(
  Pointer<Void> handle,
  Uint16 code,
  Pointer<Char> reason,
);
typedef CatcherWsDestroyNative = Void Function(
  Pointer<Void> handle,
);

// Codec
typedef CatcherPackNative = FfiResult Function(
  Pointer<Char> jsonInput,
);
typedef CatcherUnpackNative = FfiResult Function(
  Pointer<Uint8> data,
  Size len,
);

// Quality
typedef CatcherEvaluateQualityNative = Void Function(
  Pointer<Char> host,
  Pointer<NativeFunction<EventCallbackNative>> callback,
  Pointer<Void> userData,
);

// ── Bound functions ───────────────────────────────────────────

final _lib = loadCatcherLibrary();

final catcherHttpClientCreate = _lib
    .lookup<NativeFunction<CatcherHttpClientCreateNative>>(
      'catcher_http_client_create',
    )
    .asFunction();

final catcherHttpClientDestroy = _lib
    .lookup<NativeFunction<CatcherHttpClientDestroyNative>>(
      'catcher_http_client_destroy',
    )
    .asFunction();

final catcherHttpGet = _lib
    .lookup<NativeFunction<CatcherHttpGetNative>>('catcher_http_get')
    .asFunction();

final catcherHttpPost = _lib
    .lookup<NativeFunction<CatcherHttpPostNative>>('catcher_http_post')
    .asFunction();

final catcherWsCreate = _lib
    .lookup<NativeFunction<CatcherWsCreateNative>>('catcher_ws_create')
    .asFunction();

final catcherWsSendText = _lib
    .lookup<NativeFunction<CatcherWsSendTextNative>>('catcher_ws_send_text')
    .asFunction();

final catcherWsSendBinary = _lib
    .lookup<
      NativeFunction<CatcherWsSendBinaryNative>>('catcher_ws_send_binary')
    .asFunction();

final catcherWsClose = _lib
    .lookup<NativeFunction<CatcherWsCloseNative>>('catcher_ws_close')
    .asFunction();

final catcherWsDestroy = _lib
    .lookup<NativeFunction<CatcherWsDestroyNative>>('catcher_ws_destroy')
    .asFunction();

final catcherPack = _lib
    .lookup<NativeFunction<CatcherPackNative>>('catcher_pack')
    .asFunction();

final catcherUnpack = _lib
    .lookup<NativeFunction<CatcherUnpackNative>>('catcher_unpack')
    .asFunction();

final catcherEvaluateQuality = _lib
    .lookup<NativeFunction<CatcherEvaluateQualityNative>>(
      'catcher_evaluate_quality',
    )
    .asFunction();
