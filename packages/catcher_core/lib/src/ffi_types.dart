import 'dart:ffi';

/// Mirrors Rust `#[repr(C)] FfiResult`
final class FfiResult extends Struct {
  @Int32()
  external int errorCode;

  external Pointer<Char> errorMessage;

  external Pointer<Void> data;

  @Size()
  external int dataLen;
}

/// Mirrors Rust `#[repr(C)] FfiString`
final class FfiString extends Struct {
  external Pointer<Char> data;

  @Size()
  external int len;
}

/// Mirrors Rust `#[repr(C)] FfiBytes`
final class FfiBytes extends Struct {
  external Pointer<Uint8> data;

  @Size()
  external int len;

  external Pointer<NativeFunction<Void Function(Pointer<Void>)>> freeFn;

  external Pointer<Void> freeCtx;
}

/// Rust `EventCallback` function pointer
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

/// Check FfiResult for success
bool ffiResultIsOk(FfiResult result) => result.errorCode == 0;

/// Extract error message from FfiResult
String ffiResultError(FfiResult result) {
  if (result.errorMessage == nullptr) return 'unknown error';
  return result.errorMessage.cast<Utf8>().toDartString();
}

/// Free an FfiResult (call after consuming data)
void ffiResultFree(FfiResult result) {
  // The Rust Drop impl handles error_message CString cleanup.
  // Dart side: if data was allocated via calloc, caller must free it.
}
