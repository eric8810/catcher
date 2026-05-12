import 'dart:async';
import 'dart:convert';
import 'dart:ffi';
import 'dart:isolate';

import '../ffi_bindings.dart' as bindings;
import '../ffi_types.dart';
import '../models/http_config.dart';
import '../models/http_response.dart';

/// Raw event callback invoked by Rust C ABI on HTTP response.
/// Forwards the JSON result through a Dart ReceivePort.
void _onHttpResultCallback(
  Pointer<Char> eventType,
  Pointer<Uint8> eventData,
  int eventDataLen,
  Pointer<Void> userData,
) {
  final port = ReceivePort.fromRawReceivePort(userData.address);
  if (eventData != nullptr && eventDataLen > 0) {
    final json = eventData.cast<Utf8>().toDartString(length: eventDataLen);
    port.send(json);
  } else {
    port.send('{}');
  }
}

/// Dart-idiomatic HTTP client wrapping Rust HttpTransport via dart:ffi.
class CatcherHttpClient {
  late final Pointer<Void> _handle;
  static bool _callbackRegistered = false;

  CatcherHttpClient(HttpClientConfig config) {
    _registerCallbackOnce();
    final configJson = jsonEncode(config.toJson()).toNativeUtf8();
    _handle = bindings.catcherHttpClientCreate(configJson.cast<Char>());
    calloc.free(configJson);
    if (_handle == nullptr) {
      throw StateError('Failed to create CatcherHttpClient');
    }
  }

  /// Perform a GET request.
  Future<HttpResponse> get(String url) async {
    final receivePort = ReceivePort();
    final urlNative = url.toNativeUtf8();

    bindings.catcherHttpGet(
      _handle,
      urlNative.cast<Char>(),
      Pointer.fromFunction<EventCallbackNative>(_onHttpResultCallback),
      Pointer.fromAddress(receivePort.sendPort.nativePort),
    );

    final resultJson = await receivePort.first as String;
    receivePort.close();
    calloc.free(urlNative);

    final parsed = jsonDecode(resultJson) as Map<String, dynamic>?;
    if (parsed == null || parsed.containsKey('error')) {
      throw Exception(parsed?['error'] ?? 'HTTP request failed');
    }
    return HttpResponse.fromJson(parsed);
  }

  /// Perform a POST request.
  Future<HttpResponse> post(
    String url, {
    required List<int> body,
    String contentType = 'application/json',
  }) async {
    final receivePort = ReceivePort();
    final urlNative = url.toNativeUtf8();
    final contentTypeNative = contentType.toNativeUtf8();

    final bodyPtr = calloc<Uint8>(body.length);
    for (var i = 0; i < body.length; i++) {
      bodyPtr[i] = body[i];
    }

    bindings.catcherHttpPost(
      _handle,
      urlNative.cast<Char>(),
      bodyPtr,
      body.length,
      contentTypeNative.cast<Char>(),
      Pointer.fromFunction<EventCallbackNative>(_onHttpResultCallback),
      Pointer.fromAddress(receivePort.sendPort.nativePort),
    );

    final resultJson = await receivePort.first as String;
    receivePort.close();
    calloc.free(urlNative);
    calloc.free(contentTypeNative);
    calloc.free(bodyPtr);

    final parsed = jsonDecode(resultJson) as Map<String, dynamic>?;
    if (parsed == null || parsed.containsKey('error')) {
      throw Exception(parsed?['error'] ?? 'HTTP request failed');
    }
    return HttpResponse.fromJson(parsed);
  }

  /// Release the underlying Rust resources.
  void dispose() {
    bindings.catcherHttpClientDestroy(_handle);
  }

  static void _registerCallbackOnce() {
    if (_callbackRegistered) return;
    _callbackRegistered = true;
  }
}
