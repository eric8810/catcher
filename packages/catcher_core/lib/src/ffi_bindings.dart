import 'dart:ffi';
import 'dart:typed_data';

// ═══════════════════════════════════════════════════════════════
// C ABI types (must match Rust #[repr(C)] FfiResult, etc.)
// ═══════════════════════════════════════════════════════════════

final class FfiResult extends Struct {
  @Int32()
  external int errorCode;

  external Pointer<Char> errorMessage;

  external Pointer<Void> data;

  @Size()
  external int dataLen;
}

typedef CatcherHttpClientCreateNative = Pointer<Void> Function(
  Pointer<Char> configJson,
);
typedef CatcherHttpClientDestroyNative = Void Function(
  Pointer<Void> handle,
);

// HTTP execute callback — Rust calls this with the result
typedef HttpEventCallbackNative = Void Function(
  Pointer<Char> eventType,
  Pointer<Uint8> eventData,
  Size eventDataLen,
  Pointer<Void> userData,
);
typedef HttpEventCallbackDart = void Function(
  Pointer<Char> eventType,
  Pointer<Uint8> eventData,
  int eventDataLen,
  Pointer<Void> userData,
);

typedef CatcherHttpGetNative = Void Function(
  Pointer<Void> handle,
  Pointer<Char> url,
  Pointer<NativeFunction<HttpEventCallbackNative>> callback,
  Pointer<Void> userData,
);
typedef CatcherHttpPostNative = Void Function(
  Pointer<Void> handle,
  Pointer<Char> url,
  Pointer<Uint8> body,
  Size bodyLen,
  Pointer<Char> contentType,
  Pointer<NativeFunction<HttpEventCallbackNative>> callback,
  Pointer<Void> userData,
);

// Codec
typedef CatcherPackNative = FfiResult Function(Pointer<Char> jsonInput);
typedef CatcherUnpackNative = FfiResult Function(
  Pointer<Uint8> data,
  Size len,
);
