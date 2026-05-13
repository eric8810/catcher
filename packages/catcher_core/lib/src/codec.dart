import 'dart:convert';
import 'dart:ffi';
import 'dart:typed_data';

import 'package:ffi/ffi.dart';

import 'ffi_bindings.dart';
import 'native_loader.dart';

// Lazy-resolved FFI function handles
DynamicLibrary? _lib;
CatcherPackDart? _packFn;
CatcherUnpackDart? _unpackFn;
CatcherFreeResultDart? _freeResultFn;
CatcherFreeDataDart? _freeDataFn;

DynamicLibrary _getLib() => _lib ??= loadCatcherLibrary();

CatcherPackDart _pack() =>
    _packFn ??= _getLib().lookupFunction<CatcherPackNative, CatcherPackDart>(
        'catcher_pack');

CatcherUnpackDart _unpack() =>
    _unpackFn ??= _getLib().lookupFunction<CatcherUnpackNative,
        CatcherUnpackDart>('catcher_unpack');

CatcherFreeResultDart _freeResult() =>
    _freeResultFn ??= _getLib().lookupFunction<CatcherFreeResultNative,
        CatcherFreeResultDart>('catcher_free_result');

CatcherFreeDataDart _freeData() =>
    _freeDataFn ??= _getLib().lookupFunction<CatcherFreeDataNative,
        CatcherFreeDataDart>('catcher_free_data');

/// Pack a Dart value into msgpack binary.
Uint8List pack(dynamic value) {
  final json = jsonEncode(value).toNativeUtf8();
  final result = _pack()(json.cast<Char>());
  malloc.free(json);

  if (result.errorCode != 0) {
    final msg = result.errorMessage != nullptr
        ? result.errorMessage.cast<Utf8>().toDartString()
        : 'Unknown error';
    _freeResult()(result);
    throw Exception('pack failed: $msg');
  }

  final data = Uint8List(result.dataLen);
  if (result.data != nullptr && result.dataLen > 0) {
    final src = result.data.cast<Uint8>().asTypedList(result.dataLen);
    data.setAll(0, src);
  }
  // Free the data allocated by Rust (Box<[u8]> via into_raw)
  if (result.data != nullptr) {
    _freeData()(result.data, result.dataLen);
  }
  _freeResult()(result);
  return data;
}

/// Unpack msgpack binary into a Dart value (parsed from JSON).
dynamic unpack(Uint8List data) {
  final dataPtr = malloc<Uint8>(data.length);
  for (var i = 0; i < data.length; i++) {
    dataPtr[i] = data[i];
  }

  final result = _unpack()(dataPtr, data.length);
  malloc.free(dataPtr);

  if (result.errorCode != 0) {
    final msg = result.errorMessage != nullptr
        ? result.errorMessage.cast<Utf8>().toDartString()
        : 'Unknown error';
    _freeResult()(result);
    throw Exception('unpack failed: $msg');
  }

  final json = result.data.cast<Utf8>().toDartString();
  // Free the CString allocated by Rust
  if (result.data != nullptr) {
    _freeData()(result.data, result.dataLen);
  }
  _freeResult()(result);

  return jsonDecode(json);
}
