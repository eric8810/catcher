import 'dart:async';
import 'dart:convert';
import 'dart:ffi';

import 'package:ffi/ffi.dart';

import 'ffi_bindings.dart';
import 'native_loader.dart';

/// Dart wrapper around the Rust catcher WebSocket client via C ABI.
///
/// Usage:
/// ```dart
/// final ws = CatcherWsClient(WsClientConfig(
///   urls: ['wss://echo.example.com'],
///   reconnect: WsReconnectConfig(initialDelayMs: 1000),
/// ));
/// ws.events.listen((e) {
///   if (e is WsMessageEvent) print('Message: ${e.text}');
/// });
/// await ws.sendText('hello');
/// await ws.close();
/// ```
class CatcherWsClient {
  Pointer<Void>? _handle;
  DynamicLibrary? _lib;
  late final CatcherWsCreateDart _create;
  late final CatcherWsSendTextDart _sendText;
  late final CatcherWsSendBinaryDart _sendBinary;
  late final CatcherWsCloseDart _close;
  late final CatcherWsDestroyDart _destroy;

  final StreamController<WsEvent> _eventController =
      StreamController<WsEvent>.broadcast();

  NativeCallable<EventCallbackDart>? _nativeCallback;

  CatcherWsClient(WsClientConfig config) {
    _lib = loadCatcherLibrary();

    _create = _lib.lookupFunction<CatcherWsCreateNative, CatcherWsCreateDart>(
        'catcher_ws_create');
    _sendText = _lib.lookupFunction<CatcherWsSendTextNative,
        CatcherWsSendTextDart>('catcher_ws_send_text');
    _sendBinary = _lib.lookupFunction<CatcherWsSendBinaryNative,
        CatcherWsSendBinaryDart>('catcher_ws_send_binary');
    _close =
        _lib.lookupFunction<CatcherWsCloseNative, CatcherWsCloseDart>(
            'catcher_ws_close');
    _destroy = _lib.lookupFunction<CatcherWsDestroyNative,
        CatcherWsDestroyDart>('catcher_ws_destroy');

    // Register native callback
    _nativeCallback = NativeCallable<EventCallbackDart>.listener(
      (Pointer<Char> eventType, Pointer<Uint8> eventData, int eventDataLen,
          Pointer<Void> userData) {
        final typeStr = eventType.toDartString();
        final jsonStr = utf8.decode(eventData.asTypedList(eventDataLen));
        final Map<String, dynamic> json;
        try {
          json = jsonDecode(jsonStr) as Map<String, dynamic>;
        } catch (_) {
          _eventController.add(WsErrorEvent('Failed to parse event: $jsonStr'));
          return;
        }

        if (typeStr == 'ws_error' || json.containsKey('error')) {
          _eventController
              .add(WsErrorEvent(json['error']?.toString() ?? jsonStr));
          return;
        }

        _eventController.add(_parseWsEvent(json));
      },
    );

    final configJson =
        jsonEncode(config.toJson()).toNativeUtf8();
    _handle = _create(
      configJson.cast<Char>(),
      _nativeCallback!.nativeFunction,
      nullptr,
    );
    malloc.free(configJson);

    if (_handle == nullptr || _handle == null) {
      throw StateError('Failed to create WebSocket client');
    }
  }

  /// Stream of WebSocket events
  Stream<WsEvent> get events => _eventController.stream;

  /// Send a text message
  void sendText(String text) {
    _ensureHandle();
    final ffiStr = _allocFfiString(text);
    final result = _sendText(_handle!, ffiStr.ref);
    _freeFfiString(ffiStr);
    _checkResult(result);
  }

  /// Send a binary message
  void sendBinary(List<int> data) {
    _ensureHandle();
    final ptr = malloc<Uint8>(data.length);
    for (var i = 0; i < data.length; i++) {
      ptr[i] = data[i];
    }
    final result = _sendBinary(_handle!, ptr, data.length);
    malloc.free(ptr);
    _checkResult(result);
  }

  /// Close the WebSocket connection
  void close({int code = 1000, String reason = 'normal'}) {
    if (_handle == null || _handle == nullptr) return;
    final reasonFfi = _allocFfiString(reason);
    _close(_handle!, code, reasonFfi.ref);
    _freeFfiString(reasonFfi);
  }

  /// Release all resources
  void dispose() {
    if (_handle != null && _handle != nullptr) {
      _destroy(_handle!);
      _handle = null;
    }
    _nativeCallback?.close();
    _nativeCallback = null;
    if (!_eventController.isClosed) {
      _eventController.close();
    }
  }

  // ── Internal ──

  void _ensureHandle() {
    if (_handle == null || _handle == nullptr) {
      throw StateError('WebSocket client has been disposed');
    }
  }

  Pointer<FfiStringNative> _allocFfiString(String dartString) {
    final encoded = utf8.encode(dartString);
    final native = malloc<Uint8>(encoded.length);
    for (var i = 0; i < encoded.length; i++) {
      native[i] = encoded[i];
    }
    final ffiStr = calloc<FfiStringNative>();
    ffiStr.ref.data = native.cast<Char>();
    ffiStr.ref.len = encoded.length;
    return ffiStr;
  }

  void _freeFfiString(Pointer<FfiStringNative> ffiStr) {
    malloc.free(ffiStr.ref.data);
    calloc.free(ffiStr);
  }

  void _checkResult(FfiResultNative result) {
    if (result.errorCode != 0) {
      final msg = result.errorMessage != nullptr
          ? result.errorMessage.toDartString()
          : 'Unknown error';
      // Free the error_message CString allocated by Rust FfiResult::error()
      _freeResult(result);
      throw CatcherWsError(msg);
    }
  }

  /// Call catcher_freeResult to release the error_message CString
  void _freeResult(FfiResultNative result) {
    final freeFn = _lib!.lookupFunction<CatcherFreeResultNative,
        CatcherFreeResultDart>('catcher_free_result');
    freeFn(result);
  }

  WsEvent _parseWsEvent(Map<String, dynamic> json) {
    final type = json['type'] as String? ?? '';
    switch (type) {
      case 'Connected':
        return WsConnectedEvent(
          url: json['url'] as String? ?? '',
          latencyMs: json['latency_ms'] as int? ?? 0,
        );
      case 'Disconnected':
        return WsDisconnectedEvent(
          code: json['code'] as int? ?? 1006,
          reason: json['reason'] as String? ?? '',
        );
      case 'Reconnecting':
        return WsReconnectingEvent(
          attempt: json['attempt'] as int? ?? 0,
          delayMs: json['delay_ms'] as int? ?? 0,
        );
      case 'Message':
        final data = json['data'];
        final isBinary = json['is_binary'] as bool? ?? false;
        if (data is String) {
          return WsMessageEvent(
            data: utf8.encode(data),
            isBinary: isBinary,
          );
        } else if (data is List) {
          return WsMessageEvent(
            data: data.cast<int>(),
            isBinary: isBinary,
          );
        }
        return WsMessageEvent(data: [], isBinary: false);
      case 'Error':
        return WsErrorEvent(json['message'] as String? ?? 'Unknown error');
      case 'HeartbeatRtt':
        return WsHeartbeatRttEvent(
          rttMs: json['rtt_ms'] as int? ?? 0,
        );
      default:
        return WsErrorEvent('Unknown event type: $type');
    }
  }
}

// ═══════════════════════════════════════════════════════════════
// WebSocket event types
// ═══════════════════════════════════════════════════════════════

abstract class WsEvent {}

class WsConnectedEvent extends WsEvent {
  final String url;
  final int latencyMs;
  WsConnectedEvent({required this.url, required this.latencyMs});
}

class WsDisconnectedEvent extends WsEvent {
  final int code;
  final String reason;
  WsDisconnectedEvent({required this.code, required this.reason});
}

class WsReconnectingEvent extends WsEvent {
  final int attempt;
  final int delayMs;
  WsReconnectingEvent({required this.attempt, required this.delayMs});
}

class WsMessageEvent extends WsEvent {
  final List<int> data;
  final bool isBinary;

  WsMessageEvent({required this.data, required this.isBinary});

  /// Decode data as UTF-8 text
  String get text => utf8.decode(data, allowMalformed: true);
}

class WsErrorEvent extends WsEvent {
  final String message;
  WsErrorEvent(this.message);
}

class WsHeartbeatRttEvent extends WsEvent {
  final int rttMs;
  WsHeartbeatRttEvent({required this.rttMs});
}

// ═══════════════════════════════════════════════════════════════
// WebSocket config types (match Rust WsClientConfig)
// ═══════════════════════════════════════════════════════════════

class WsReconnectConfig {
  final int initialDelayMs;
  final int maxDelayMs;
  final double backoffMultiplier;
  final int maxAttempts;

  const WsReconnectConfig({
    this.initialDelayMs = 500,
    this.maxDelayMs = 30000,
    this.backoffMultiplier = 2.0,
    this.maxAttempts = 20,
  });

  Map<String, dynamic> toJson() => {
        'initial_delay_ms': initialDelayMs,
        'max_delay_ms': maxDelayMs,
        'backoff_multiplier': backoffMultiplier,
        'max_attempts': maxAttempts,
      };
}

class WsHeartbeatConfig {
  final int intervalMs;
  final bool adaptive;
  final int pongTimeoutMs;
  final int maxMissedPongs;

  const WsHeartbeatConfig({
    this.intervalMs = 30000,
    this.adaptive = true,
    this.pongTimeoutMs = 10000,
    this.maxMissedPongs = 3,
  });

  Map<String, dynamic> toJson() => {
        'interval_ms': intervalMs,
        'adaptive': adaptive,
        'pong_timeout_ms': pongTimeoutMs,
        'max_missed_pongs': maxMissedPongs,
      };
}

class WsClientConfig {
  final List<String> urls;
  final bool perMessageDeflate;
  final int handshakeTimeoutMs;
  final int maxPayloadBytes;
  final WsReconnectConfig? reconnect;
  final WsHeartbeatConfig? heartbeat;
  final int raceCount;

  const WsClientConfig({
    required this.urls,
    this.perMessageDeflate = false,
    this.handshakeTimeoutMs = 15000,
    this.maxPayloadBytes = 67108864, // 64MB
    this.reconnect,
    this.heartbeat,
    this.raceCount = 1,
  });

  Map<String, dynamic> toJson() => {
        'urls': urls,
        'per_message_deflate': perMessageDeflate,
        'handshake_timeout_ms': handshakeTimeoutMs,
        'max_payload_bytes': maxPayloadBytes,
        if (reconnect != null) 'reconnect': reconnect!.toJson(),
        if (heartbeat != null) 'heartbeat': heartbeat!.toJson(),
        'race_count': raceCount,
      };
}

/// Error thrown when the Rust WS client returns an error
class CatcherWsError implements Exception {
  final String message;
  const CatcherWsError(this.message);

  @override
  String toString() => 'CatcherWsError: $message';
}
