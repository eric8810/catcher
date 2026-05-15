import 'dart:async';
import 'dart:convert';
import 'dart:ffi';

import 'package:ffi/ffi.dart';

import 'ffi_bindings.dart';
import 'native_loader.dart';

// ═══════════════════════════════════════════════════════════════
// SSE event types
// ═══════════════════════════════════════════════════════════════

/// Base class for SSE events received from the Rust SSE client.
sealed class SseEvent {}

/// The SSE connection has been established (or re-established after reconnect).
class SseOpenEvent extends SseEvent {
  @override
  String toString() => 'SseOpenEvent';
}

/// A data line was received from the SSE stream.
class SseDataEvent extends SseEvent {
  final String data;
  final String? event;
  final String? id;
  final int? retryMs;

  SseDataEvent({
    required this.data,
    this.event,
    this.id,
    this.retryMs,
  });

  factory SseDataEvent.fromJson(Map<String, dynamic> json) => SseDataEvent(
        data: json['data'] as String? ?? '',
        event: json['event'] as String?,
        id: json['id'] as String?,
        retryMs: json['retry_ms'] as int?,
      );

  @override
  String toString() => 'SseDataEvent(data: $data, id: $id)';
}

/// An error occurred on the SSE connection.
class SseErrorEvent extends SseEvent {
  final String message;

  SseErrorEvent(this.message);

  factory SseErrorEvent.fromJson(Map<String, dynamic> json) =>
      SseErrorEvent(json['data'] as String? ??
          json['message'] as String? ??
          'Unknown SSE error');

  @override
  String toString() => 'SseErrorEvent($message)';
}

/// The SSE connection has been closed.
class SseCloseEvent extends SseEvent {
  @override
  String toString() => 'SseCloseEvent';
}

// ═══════════════════════════════════════════════════════════════
// SSE ready state
// ═══════════════════════════════════════════════════════════════

enum SseReadyState { connecting, open, closed, invalid }

// ═══════════════════════════════════════════════════════════════
// SSE client config
// ═══════════════════════════════════════════════════════════════

class SseReconnectConfig {
  final bool enabled;
  final int maxAttempts;
  final String backoffKind; // "fixed" | "exponential" | "decorrelated"
  final int initialBackoffMs;
  final int maxBackoffMs;

  const SseReconnectConfig({
    this.enabled = true,
    this.maxAttempts = 5,
    this.backoffKind = 'exponential',
    this.initialBackoffMs = 1000,
    this.maxBackoffMs = 30000,
  });

  Map<String, dynamic> toJson() => {
        'enabled': enabled,
        'max_attempts': maxAttempts,
        'backoff_kind': backoffKind,
        'initial_backoff_ms': initialBackoffMs,
        'max_backoff_ms': maxBackoffMs,
      };
}

class SseClientConfig {
  final String url;
  final String method;
  final Map<String, String>? headers;
  final String? body;
  final SseReconnectConfig? reconnect;
  final int timeoutMs;

  const SseClientConfig({
    required this.url,
    this.method = 'GET',
    this.headers,
    this.body,
    this.reconnect,
    this.timeoutMs = 30000,
  });

  Map<String, dynamic> toJson() => {
        'url': url,
        'method': method,
        if (headers != null && headers!.isNotEmpty) 'headers': headers,
        if (body != null) 'body': body,
        if (reconnect != null) 'reconnect': reconnect!.toJson(),
        'timeout_ms': timeoutMs,
      };
}

// ═══════════════════════════════════════════════════════════════
// CatcherSseClient — persistent SSE with auto-reconnect
// ═══════════════════════════════════════════════════════════════

/// Dart wrapper around the Rust SSE client via C ABI.
///
/// Supports persistent SSE connections with auto-reconnect.
/// For one-shot POST SSE streams (e.g. AI streaming APIs), use
/// [CatcherHttpClient.sseStream] instead.
///
/// Usage:
/// ```dart
/// final sse = CatcherSseClient(SseClientConfig(
///   url: 'https://api.example.com/v1/events',
///   headers: {'Authorization': 'Bearer xxx'},
/// ));
/// sse.events.listen((event) {
///   if (event is SseDataEvent) print('Data: ${event.data}');
///   if (event is SseCloseEvent) sse.dispose();
/// });
/// ```
class CatcherSseClient {
  Pointer<Void>? _handle;
  late final DynamicLibrary _lib;
  late final CatcherSseConnectDart _connectFn;
  late final CatcherSseReadyStateDart _readyStateFn;
  late final CatcherSseLastEventIdDart _lastEventIdFn;
  late final CatcherSseCloseDart _closeFn;
  late final CatcherSseDestroyDart _destroyFn;
  late final CatcherFreeEventDataDart _freeEventDataFn;
  late final CatcherFreeDataDart _freeDataFn;

  final StreamController<SseEvent> _eventController =
      StreamController<SseEvent>.broadcast();

  NativeCallable<EventCallbackNative>? _nativeCallback;

  CatcherSseClient(SseClientConfig config) {
    _lib = loadCatcherLibrary();

    _connectFn = _lib.lookupFunction<CatcherSseConnectNative,
        CatcherSseConnectDart>('catcher_sse_connect');

    _readyStateFn = _lib.lookupFunction<CatcherSseReadyStateNative,
        CatcherSseReadyStateDart>('catcher_sse_ready_state');

    _lastEventIdFn = _lib.lookupFunction<CatcherSseLastEventIdNative,
        CatcherSseLastEventIdDart>('catcher_sse_last_event_id');

    _closeFn = _lib.lookupFunction<CatcherSseCloseNative, CatcherSseCloseDart>(
        'catcher_sse_close');

    _destroyFn = _lib.lookupFunction<CatcherSseDestroyNative,
        CatcherSseDestroyDart>('catcher_sse_destroy');

    _freeEventDataFn = _lib.lookupFunction<CatcherFreeEventDataNative,
        CatcherFreeEventDataDart>('catcher_free_event_data');

    _freeDataFn = _lib.lookupFunction<CatcherFreeDataNative,
        CatcherFreeDataDart>('catcher_free_data');

    // Set up the native callback that forwards events into the Dart stream
    _nativeCallback =
        NativeCallable<EventCallbackNative>.listener(_onNativeEvent);

    final configJson = jsonEncode(config.toJson()).toNativeUtf8();

    _handle = _connectFn(
      configJson.cast<Char>(),
      _nativeCallback!.nativeFunction,
      nullptr,
    );

    malloc.free(configJson);

    if (_handle == nullptr) {
      _nativeCallback?.close();
      _nativeCallback = null;
      throw StateError(
          'Failed to create SSE client — invalid config or connection error');
    }
  }

  /// Stream of SSE events (open, data, error, close).
  Stream<SseEvent> get events => _eventController.stream;

  /// Current ready state of the SSE connection.
  SseReadyState get readyState {
    if (_handle == null || _handle == nullptr) return SseReadyState.invalid;
    final value = _readyStateFn(_handle!);
    switch (value) {
      case 0:
        return SseReadyState.connecting;
      case 1:
        return SseReadyState.open;
      case 2:
        return SseReadyState.closed;
      default:
        return SseReadyState.invalid;
    }
  }

  /// The last event ID received from the server (for reconnection).
  String? get lastEventId {
    if (_handle == null || _handle == nullptr) return null;
    final ptr = _lastEventIdFn(_handle!);
    if (ptr == nullptr) return null;
    final result = ptr.cast<Utf8>().toDartString();
    _freeDataFn(ptr.cast(), result.length + 1);
    return result;
  }

  /// Close the SSE connection (stops reconnection).
  void close() {
    if (_handle != null && _handle != nullptr) {
      _closeFn(_handle!);
    }
  }

  /// Release native resources.
  void dispose() {
    if (_handle != null && _handle != nullptr) {
      _closeFn(_handle!);
      _destroyFn(_handle!);
      _handle = null;
    }
    _nativeCallback?.close();
    _nativeCallback = null;
    if (!_eventController.isClosed) {
      _eventController.close();
    }
  }

  // ── Internal ──

  void _onNativeEvent(
    Pointer<Char> eventType,
    Pointer<Uint8> eventData,
    int eventDataLen,
    Pointer<Void> userData,
  ) {
    final jsonBytes = eventData.asTypedList(eventDataLen);
    final jsonStr = utf8.decode(jsonBytes, allowMalformed: true);

    _freeEventDataFn(eventType, eventData);

    if (_eventController.isClosed) return;

    final Map<String, dynamic> parsed;
    try {
      parsed = jsonDecode(jsonStr) as Map<String, dynamic>;
    } catch (_) {
      _eventController.add(SseErrorEvent(jsonStr));
      return;
    }

    final type = parsed['type'] as String? ?? '';
    switch (type) {
      case 'open':
        _eventController.add(SseOpenEvent());
        break;
      case 'data':
        _eventController.add(SseDataEvent.fromJson(parsed));
        break;
      case 'error':
        _eventController.add(SseErrorEvent.fromJson(parsed));
        break;
      case 'close':
        _eventController.add(SseCloseEvent());
        break;
      default:
        _eventController.add(SseErrorEvent('Unknown SSE event type: $type'));
    }
  }
}
