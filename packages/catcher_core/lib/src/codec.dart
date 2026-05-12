import 'dart:convert';
import 'dart:ffi';
import 'dart:typed_data';

import '../ffi_bindings.dart' as bindings;
import '../ffi_types.dart';

/// Pack a Dart value into msgpack binary.
Uint8List pack(dynamic value) {
  final json = jsonEncode(value).toNativeUtf8();
  final result = bindings.catcherPack(json.cast<Char>());
  calloc.free(json);

  if (!ffiResultIsOk(result)) {
    throw Exception('pack failed: ${ffiResultError(result)}');
  }

  final data = Uint8List(result.dataLen);
  if (result.data != nullptr && result.dataLen > 0) {
    final src = result.data.cast<Uint8>().asTypedList(result.dataLen);
    data.setAll(0, src);
  }
  // Data allocated by Rust (via std::mem::forget), caller must free
  calloc.free(result.data);
  return data;
}

/// Unpack msgpack binary into a Dart value (parsed from JSON).
dynamic unpack(Uint8List data) {
  final dataPtr = calloc<Uint8>(data.length);
  for (var i = 0; i < data.length; i++) {
    dataPtr[i] = data[i];
  }

  final result = bindings.catcherUnpack(dataPtr, data.length);
  calloc.free(dataPtr);

  if (!ffiResultIsOk(result)) {
    throw Exception('unpack failed: ${ffiResultError(result)}');
  }

  // result.data is a null-terminated JSON string
  final json = result.data.cast<Utf8>().toDartString();
  // result.data was allocated via CString::into_raw(), free it
  calloc.free(result.data);

  return jsonDecode(json);
}
