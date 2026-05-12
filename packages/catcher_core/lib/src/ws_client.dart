import 'dart:async';
import 'dart:convert';
import 'dart:ffi';
import 'dart:isolate';

import '../ffi_bindings.dart' as bindings;
import '../ffi_types.dart';
import '../models/ws_config.dart';
import '../models/ws_event.dart';

/// Raw event callback from Rust C ABI.
/// Forwards WS events into a Dart StreamController.
void _onWsEventCallback(
  Pointer<Char> eventType,
  Pointer<Uint8> eventData,
  int eventDataLen,
  Pointer<Void> userData,
) {
  final controller = _wsControllers[userData.address];
  if (controller == null) return;
  if (eventData != nullptr && eventDataLen > 0) {
    final json = eventData.cast<Utf8>().toDartString(length: eventDataLen);
    try {
      final parsed = jsonDecode(json) as Map<String, dynamic>;
      controller.add(WsEvent.fromJson(parsed));
    } catch (_) {}
  }
}

final _wsControllers = <int, StreamController<WsEvent>>{};

/// Dart-idiomatic WebSocket client wrapping Rust WsTransport via dart:ffi.
class CatcherWsClient {
  late final Pointer<Void> _handle;
  late final StreamController<WsEvent> _eventController;

  CatcherWsClient(WsClientConfig config) {
    _eventController = StreamController<WsEvent>.broadcast();

    final configJson = jsonEncode(config.toJson()).toNativeUtf8();

    _handle = bindings.catcherWsCreate(
      configJson.cast<Char>(),
      Pointer.fromFunction<EventCallbackNative>(_onWsEventCallback),
      Pointer.fromAddress(_eventController.hashCode),
    );

    calloc.free(configJson);
    _wsControllers[_eventController.hashCode] = _eventController;

    if (_handle == nullptr) {
      throw StateError('Failed to create CatcherWsClient');
    }
  }

  /// Stream of WebSocket events (Connected, Disconnected, Message, Error).
  Stream<WsEvent> get events => _eventController.stream;

  /// Send a text message.
  void sendText(String text) {
    final textNative = text.toNativeUtf8();
    final result = bindings.catcherWsSendText(_handle, textNative.cast<Char>());
    calloc.free(textNative);
    if (!ffiResultIsOk(result)) {
      throw Exception(ffiResultError(result));
    }
  }

  /// Send binary data.
  void sendBinary(Uint8List data) {
    final dataPtr = calloc<Uint8>(data.length);
    for (var i = 0; i < data.length; i++) {
      dataPtr[i] = data[i];
    }
    final result = bindings.catcherWsSendBinary(_handle, dataPtr, data.length);
    calloc.free(dataPtr);
    if (!ffiResultIsOk(result)) {
      throw Exception(ffiResultError(result));
    }
  }

  /// Close the WebSocket connection.
  void close({int code = 1000, String reason = 'normal'}) {
    final reasonNative = reason.toNativeUtf8();
    bindings.catcherWsClose(_handle, code, reasonNative.cast<Char>());
    calloc.free(reasonNative);
  }

  /// Release the underlying Rust resources.
  void dispose() {
    _wsControllers.remove(_eventController.hashCode);
    _eventController.close();
    bindings.catcherWsDestroy(_handle);
  }
}
