import 'dart:async';
import 'dart:convert';
import 'dart:ffi';
import 'dart:isolate';

import 'package:ffi/ffi.dart';

import 'ffi_bindings.dart';
import 'native_loader.dart';
import 'sse_client.dart';

/// Dart wrapper around the Rust catcher HTTP client via C ABI.
///
/// Usage:
/// ```dart
/// final client = CatcherHttpClient(HttpClientConfig(
///   baseUrl: 'https://api.example.com',
///   retry: RetryConfig(maxAttempts: 3),
/// ));
/// final resp = await client.get('/channels',
///   headers: {'Authorization': 'Bearer xxx'},
/// );
/// print('Status: ${resp.status}, Body: ${resp.bodyAsString}');
/// print('CB state: ${client.circuitBreakerState}');
/// client.dispose();
/// ```
class CatcherHttpClient {
  Pointer<Void>? _handle;
  late final DynamicLibrary _lib;
  late final CatcherHttpClientCreateDart _create;
  late final CatcherHttpClientDestroyDart _destroy;
  late final CatcherHttpExecuteDart _executeFn;
  late final CatcherFreeEventDataDart _freeEventDataFn;
  late final CatcherFreeDataDart _freeDataFn;
  CatcherHttpClientCancelAllDart? _cancelAllFn;
  CatcherHttpCircuitBreakerStateDart? _circuitBreakerStateFn;
  CatcherHttpMetricsDart? _metricsFn;
  CatcherHttpAdaptiveTimeoutConfigDart? _adaptiveTimeoutFn;
  CatcherHttpExecuteStreamDart? _executeStreamFn;
  CatcherHttpExecuteWithIdDart? _executeWithIdFn;
  CatcherHttpCancelRequestDart? _cancelRequestFn;

  CatcherHttpClient(HttpClientConfig config) {
    _lib = loadCatcherLibrary();

    _create = _lib
        .lookupFunction<CatcherHttpClientCreateNative,
            CatcherHttpClientCreateDart>('catcher_http_client_create');

    _destroy = _lib
        .lookupFunction<CatcherHttpClientDestroyNative,
            CatcherHttpClientDestroyDart>('catcher_http_client_destroy');

    _executeFn = _lib.lookupFunction<CatcherHttpExecuteNative,
        CatcherHttpExecuteDart>('catcher_http_execute');

    _freeEventDataFn = _lib.lookupFunction<CatcherFreeEventDataNative,
        CatcherFreeEventDataDart>('catcher_free_event_data');

    _freeDataFn = _lib.lookupFunction<CatcherFreeDataNative,
        CatcherFreeDataDart>('catcher_free_data');

    // Optional symbols — may not exist if Rust was compiled without these
    try {
      _cancelAllFn = _lib.lookupFunction<CatcherHttpClientCancelAllNative,
          CatcherHttpClientCancelAllDart>('catcher_http_client_cancel_all');
    } catch (_) {
      _cancelAllFn = null;
    }

    try {
      _circuitBreakerStateFn =
          _lib.lookupFunction<CatcherHttpCircuitBreakerStateNative,
              CatcherHttpCircuitBreakerStateDart>(
                  'catcher_http_circuit_breaker_state');
    } catch (_) {
      _circuitBreakerStateFn = null;
    }

    try {
      _metricsFn = _lib.lookupFunction<CatcherHttpMetricsNative,
          CatcherHttpMetricsDart>('catcher_http_metrics');
    } catch (_) {
      _metricsFn = null;
    }

    try {
      _adaptiveTimeoutFn = _lib.lookupFunction<
          CatcherHttpAdaptiveTimeoutConfigNative,
          CatcherHttpAdaptiveTimeoutConfigDart>(
              'catcher_http_adaptive_timeout_config');
    } catch (_) {
      _adaptiveTimeoutFn = null;
    }

    try {
      _executeStreamFn = _lib.lookupFunction<CatcherHttpExecuteStreamNative,
          CatcherHttpExecuteStreamDart>('catcher_http_execute_stream');
    } catch (_) {
      _executeStreamFn = null;
    }

    try {
      _executeWithIdFn = _lib.lookupFunction<CatcherHttpExecuteWithIdNative,
          CatcherHttpExecuteWithIdDart>('catcher_http_execute_with_id');
    } catch (_) {
      _executeWithIdFn = null;
    }

    try {
      _cancelRequestFn = _lib.lookupFunction<CatcherHttpCancelRequestNative,
          CatcherHttpCancelRequestDart>('catcher_http_cancel_request');
    } catch (_) {
      _cancelRequestFn = null;
    }

    final configJson = jsonEncode(config.toJson()).toNativeUtf8();
    _handle = _create(configJson.cast<Char>());
    malloc.free(configJson);

    if (_handle == nullptr) {
      throw StateError('Failed to create HTTP client — invalid config or Rust init error');
    }
  }

  /// GET request
  Future<HttpResponse> get(String path,
      {Map<String, String>? headers, int? timeoutMs}) async {
    return _execute('GET', path, null, null, headers: headers, timeoutMs: timeoutMs);
  }

  /// POST request
  Future<HttpResponse> post(String path,
      {Map<String, dynamic>? body,
      String contentType = 'application/json',
      Map<String, String>? headers,
      int? timeoutMs}) async {
    final bodyBytes = body != null ? utf8.encode(jsonEncode(body)) : null;
    return _execute('POST', path, bodyBytes, contentType,
        headers: headers, timeoutMs: timeoutMs);
  }

  /// PUT request
  Future<HttpResponse> put(String path,
      {Map<String, dynamic>? body,
      String contentType = 'application/json',
      Map<String, String>? headers,
      int? timeoutMs}) async {
    final bodyBytes = body != null ? utf8.encode(jsonEncode(body)) : null;
    return _execute('PUT', path, bodyBytes, contentType,
        headers: headers, timeoutMs: timeoutMs);
  }

  /// DELETE request
  Future<HttpResponse> delete(String path,
      {Map<String, String>? headers, int? timeoutMs}) async {
    return _execute('DELETE', path, null, null, headers: headers, timeoutMs: timeoutMs);
  }

  /// PATCH request
  Future<HttpResponse> patch(String path,
      {Map<String, dynamic>? body,
      String contentType = 'application/json',
      Map<String, String>? headers,
      int? timeoutMs}) async {
    final bodyBytes = body != null ? utf8.encode(jsonEncode(body)) : null;
    return _execute('PATCH', path, bodyBytes, contentType,
        headers: headers, timeoutMs: timeoutMs);
  }

  /// Cancel all in-flight requests on this client (page-exit scenario).
  void cancelAll() {
    final fn = _cancelAllFn;
    if (fn != null && _handle != null && _handle != nullptr) {
      fn(_handle!);
    }
  }

  /// Query the circuit breaker state.
  /// Returns a JSON string like `{"state":"closed","failure_count":0,...}`.
  /// Returns `{"state":"disabled"}` if no circuit breaker is configured.
  String? get circuitBreakerState {
    final fn = _circuitBreakerStateFn;
    if (fn == null || _handle == null || _handle == nullptr) return null;
    final ptr = fn(_handle!);
    if (ptr == nullptr) return null;
    final result = ptr.cast<Utf8>().toDartString();
    _freeDataFn(ptr.cast(), result.length + 1);
    return result;
  }

  /// Query runtime metrics.
  /// Returns a JSON string with http_requests, http_success_rate,
  /// http_avg_latency_us, etc.
  String? get metrics {
    final fn = _metricsFn;
    if (fn == null || _handle == null || _handle == nullptr) return null;
    final ptr = fn(_handle!);
    if (ptr == nullptr) return null;
    final result = ptr.cast<Utf8>().toDartString();
    _freeDataFn(ptr.cast(), result.length + 1);
    return result;
  }

  /// Configure adaptive timeout based on P90 RTT sliding window.
  /// [enabled] enables/disables; [minTimeoutMs]/[maxTimeoutMs] clamp;
  /// [multiplier] scales P90 RTT (e.g. 2.5 → timeout = P90_RTT * 2.5).
  void setAdaptiveTimeout({
    required bool enabled,
    int minTimeoutMs = 100,
    int maxTimeoutMs = 30000,
    double multiplier = 2.5,
    int windowSize = 20,
  }) {
    final fn = _adaptiveTimeoutFn;
    if (fn == null || _handle != null && _handle == nullptr) return;
    fn(
      _handle!,
      enabled ? 1 : 0,
      minTimeoutMs,
      maxTimeoutMs,
      (multiplier * 1000).round(),
      windowSize,
    );
  }

  /// Cancel a single in-flight request by [requestId].
  /// Returns `true` if the request was found and cancelled, `false` otherwise.
  bool cancelRequest(int requestId) {
    final fn = _cancelRequestFn;
    if (fn == null || _handle == null || _handle == nullptr) return false;
    return fn(_handle!, requestId) == 0;
  }

  /// Execute an HTTP request with per-request cancellation support.
  ///
  /// Returns a record `(requestId, response)`. Use [cancelRequest(requestId)]
  /// to cancel the in-flight request.
  Future<({int requestId, HttpResponse response})> executeWithCancel(
    String method,
    String path, {
    Map<String, dynamic>? body,
    String contentType = 'application/json',
    Map<String, String>? headers,
    int? timeoutMs,
  }) async {
    final fn = _executeWithIdFn;
    if (fn == null) {
      throw StateError('execute_with_id not available — upgrade catcher_ffi');
    }
    _ensureHandle();
    final bodyBytes = body != null ? utf8.encode(jsonEncode(body)) : null;
    return _executeWithCancel(
        fn, method, path, bodyBytes, contentType, headers, timeoutMs);
  }

  /// Stream download — receives headers, chunks, and completion events.
  ///
  /// Returns a [Stream] of [StreamEvent] objects:
  /// - [StreamHeadersEvent] with status and headers
  /// - [StreamChunkEvent] with binary data
  /// - [StreamDoneEvent] on completion
  /// - [StreamErrorEvent] on error
  Stream<StreamEvent> executeStream(
    String method,
    String path, {
    Map<String, dynamic>? body,
    String contentType = 'application/json',
    Map<String, String>? headers,
    int? timeoutMs,
  }) {
    final fn = _executeStreamFn;
    if (fn == null) {
      throw StateError('execute_stream not available — upgrade catcher_ffi');
    }
    _ensureHandle();
    final bodyBytes = body != null ? utf8.encode(jsonEncode(body)) : null;
    return _doExecuteStream(
        fn, method, path, bodyBytes, contentType, headers, timeoutMs);
  }

  /// One-shot SSE stream request (e.g. POST SSE for AI streaming APIs).
  ///
  /// Returns a stream of [SseEvent]s that completes when the server closes
  /// the connection or an error occurs. Does NOT auto-reconnect.
  ///
  /// ```dart
  /// final events = await client.sseStream(
  ///   method: 'POST',
  ///   url: '/v1/chat/completions',
  ///   body: jsonEncode({'model': 'gpt-4', 'stream': true}),
  ///   headers: {'Authorization': 'Bearer sk-xxx'},
  /// );
  /// for (final event in events) {
  ///   if (event is SseDataEvent) print(event.data);
  /// }
  /// ```
  Future<List<SseEvent>> sseStream({
    required String method,
    required String url,
    String? body,
    Map<String, String>? headers,
  }) async {
    _ensureHandle();

    final streamFn = _lib.lookupFunction<CatcherSseStreamNative,
        CatcherSseStreamDart>('catcher_sse_stream');

    final receivePort = ReceivePort();
    final completer = Completer<List<SseEvent>>();
    final events = <SseEvent>[];
    bool cleanedUp = false;

    final nativeCallback =
        NativeCallable<EventCallbackNative>.listener(
      (Pointer<Char> eventType, Pointer<Uint8> eventData, int eventDataLen,
          Pointer<Void> userData) {
        final jsonBytes = eventData.asTypedList(eventDataLen);
        final jsonStr = utf8.decode(jsonBytes, allowMalformed: true);

        _freeEventDataFn(eventType, eventData);

        final Map<String, dynamic> parsed;
        try {
          parsed = jsonDecode(jsonStr) as Map<String, dynamic>;
        } catch (_) {
          events.add(SseErrorEvent(jsonStr));
          receivePort.sendPort.send(null);
          return;
        }

        final type = parsed['type'] as String? ?? '';
        switch (type) {
          case 'open':
            events.add(SseOpenEvent());
            break;
          case 'data':
            events.add(SseDataEvent.fromJson(parsed));
            break;
          case 'error':
            events.add(SseErrorEvent.fromJson(parsed));
            break;
          case 'close':
            events.add(SseCloseEvent());
            break;
        }
        receivePort.sendPort.send(null);
      },
    );

    late StreamSubscription sub;
    sub = receivePort.listen((_) {
      final hasClose = events.any((e) => e is SseCloseEvent);
      final hasError = events.any((e) => e is SseErrorEvent);

      if ((hasClose || hasError) && !completer.isCompleted) {
        cleanedUp = true;
        sub.cancel();
        nativeCallback.close();
        receivePort.close();
        completer.complete(events);
      }
    });

    final methodFfi = _allocFfiString(method);
    final urlFfi = _allocFfiString(url);
    final headersJson = (headers != null && headers.isNotEmpty)
        ? jsonEncode(headers).toNativeUtf8().cast<Char>()
        : nullptr.cast<Char>();

    final bodyPtr = (body != null)
        ? malloc<Uint8>(body.length)
        : Pointer<Uint8>.fromAddress(0);
    if (body != null) {
      final bodyBytes = utf8.encode(body);
      for (var i = 0; i < bodyBytes.length; i++) {
        bodyPtr[i] = bodyBytes[i];
      }
    }

    try {
      final bodyBytes = body != null ? utf8.encode(body) : null;
      streamFn(
        _handle!,
        methodFfi.ref,
        urlFfi.ref,
        bodyPtr,
        bodyBytes?.length ?? 0,
        headersJson,
        nativeCallback.nativeFunction,
        nullptr,
      );
    } catch (e) {
      if (!cleanedUp) {
        cleanedUp = true;
        sub.cancel();
        nativeCallback.close();
        receivePort.close();
      }
      _freeFfiString(methodFfi);
      _freeFfiString(urlFfi);
      if (headersJson != nullptr) malloc.free(headersJson);
      if (body != null) malloc.free(bodyPtr);
      rethrow;
    }

    _freeFfiString(methodFfi);
    _freeFfiString(urlFfi);
    if (headersJson != nullptr) malloc.free(headersJson);
    if (body != null) malloc.free(bodyPtr);

    return completer.future.timeout(
      const Duration(seconds: 60),
      onTimeout: () {
        if (!cleanedUp) {
          cleanedUp = true;
          sub.cancel();
          nativeCallback.close();
          receivePort.close();
        }
        return events;
      },
    );
  }

  /// Release native resources
  void dispose() {
    if (_handle != null && _handle != nullptr) {
      _destroy(_handle!);
      _handle = null;
    }
  }

  // ── Internal ──

  void _ensureHandle() {
    if (_handle == null || _handle == nullptr) {
      throw StateError('HTTP client has been disposed');
    }
  }

  /// Execute with per-request cancellation support.
  Future<({int requestId, HttpResponse response})> _executeWithCancel(
    CatcherHttpExecuteWithIdDart fn,
    String method,
    String path,
    List<int>? body,
    String? contentType,
    Map<String, String>? headers,
    int? timeoutMs,
  ) async {
    _ensureHandle();
    final receivePort = ReceivePort();
    final completer = Completer<({int requestId, HttpResponse response})>();
    bool cleanedUp = false;

    final nativeCallback = NativeCallable<EventCallbackNative>.listener(
      (Pointer<Char> eventType, Pointer<Uint8> eventData, int eventDataLen,
          Pointer<Void> userData) {
        final jsonBytes = eventData.asTypedList(eventDataLen);
        final jsonStr = utf8.decode(jsonBytes, allowMalformed: true);

        _freeEventDataFn(eventType, eventData);

        final Map<String, dynamic> result;
        try {
          result = jsonDecode(jsonStr) as Map<String, dynamic>;
        } catch (_) {
          receivePort.sendPort.send({'error': jsonStr});
          return;
        }
        receivePort.sendPort.send(result);
      },
    );

    late StreamSubscription sub;
    sub = receivePort.listen((message) {
      sub.cancel();
      if (!cleanedUp) {
        cleanedUp = true;
        nativeCallback.close();
        receivePort.close();
      }
      if (!completer.isCompleted) {
        if (message is Map && message.containsKey('request_id')) {
          if (message.containsKey('error')) {
            completer.completeError(CatcherHttpError(
              message['error']?.toString() ?? 'Unknown error',
            ));
          } else if (message['type'] == 'cancelled') {
            completer.completeError(CatcherHttpError('Request cancelled'));
          } else {
            final requestId = message['request_id'] as int;
            final response = HttpResponse.fromJson(
                Map<String, dynamic>.from(message));
            completer.complete((requestId: requestId, response: response));
          }
        } else if (message is Map && message.containsKey('error')) {
          completer.completeError(CatcherHttpError(
            message['error']?.toString() ?? 'Unknown error',
          ));
        } else {
          completer.completeError(CatcherHttpError(message.toString()));
        }
      }
    });

    final methodFfi = _allocFfiString(method);
    final urlFfi = _allocFfiString(path);
    final ctFfi = contentType != null
        ? _allocFfiString(contentType)
        : _allocFfiString('');
    final headersJson = (headers != null && headers.isNotEmpty)
        ? jsonEncode(headers).toNativeUtf8().cast<Char>()
        : nullptr.cast<Char>();
    final bodyPtr = (body != null && body.isNotEmpty)
        ? malloc<Uint8>(body.length)
        : Pointer<Uint8>.fromAddress(0);
    if (body != null && body.isNotEmpty) {
      for (var i = 0; i < body.length; i++) {
        bodyPtr[i] = body[i];
      }
    }

    try {
      final requestId = fn(
        _handle!,
        methodFfi.ref,
        urlFfi.ref,
        bodyPtr,
        body?.length ?? 0,
        ctFfi.ref,
        headersJson,
        timeoutMs ?? 0,
        nativeCallback.nativeFunction,
        nullptr,
      );
      if (requestId == 0) {
        throw CatcherHttpError('Failed to start HTTP request');
      }
    } catch (e) {
      if (!cleanedUp) {
        cleanedUp = true;
        nativeCallback.close();
        receivePort.close();
      }
      _freeFfiString(methodFfi);
      _freeFfiString(urlFfi);
      _freeFfiString(ctFfi);
      if (headersJson != nullptr) malloc.free(headersJson);
      if (body != null && body.isNotEmpty) malloc.free(bodyPtr);
      rethrow;
    }

    _freeFfiString(methodFfi);
    _freeFfiString(urlFfi);
    _freeFfiString(ctFfi);
    if (headersJson != nullptr) malloc.free(headersJson);
    if (body != null && body.isNotEmpty) malloc.free(bodyPtr);

    return completer.future.timeout(
      const Duration(seconds: 30),
      onTimeout: () {
        if (!cleanedUp) {
          cleanedUp = true;
          nativeCallback.close();
          receivePort.close();
        }
        if (!completer.isCompleted) {
          completer.completeError(
            TimeoutException('HTTP request timed out after 30s'),
          );
        }
        throw TimeoutException('HTTP request timed out after 30s');
      },
    );
  }

  /// Streaming download implementation.
  Stream<StreamEvent> _doExecuteStream(
    CatcherHttpExecuteStreamDart fn,
    String method,
    String path,
    List<int>? body,
    String? contentType,
    Map<String, String>? headers,
    int? timeoutMs,
  ) {
    _ensureHandle();
    final controller = StreamController<StreamEvent>();
    bool cleanedUp = false;
    late final NativeCallable<EventCallbackNative> nativeCallback;
    nativeCallback = NativeCallable<EventCallbackNative>.listener(
      (Pointer<Char> eventType, Pointer<Uint8> eventData, int eventDataLen,
          Pointer<Void> userData) {
        final jsonBytes = eventData.asTypedList(eventDataLen);
        final jsonStr = utf8.decode(jsonBytes, allowMalformed: true);

        _freeEventDataFn(eventType, eventData);

        final typeStr = eventType.cast<Utf8>().toDartString();

        switch (typeStr) {
          case 'stream_headers':
            try {
              final parsed = jsonDecode(jsonStr) as Map<String, dynamic>;
              controller.add(StreamHeadersEvent(
                status: parsed['status'] as int,
                headers: Map<String, String>.from(parsed['headers'] ?? {}),
                requestId: parsed['request_id'] as int? ?? 0,
              ));
            } catch (_) {}
            break;
          case 'stream_chunk':
            try {
              final parsed = jsonDecode(jsonStr) as Map<String, dynamic>;
              final b64 = parsed['data_base64'] as String? ?? '';
              final requestId = parsed['request_id'] as int? ?? 0;
              controller.add(StreamChunkEvent(
                  data: base64Decode(b64), requestId: requestId));
            } catch (_) {
              controller.add(StreamChunkEvent(
                  data: List<int>.from(jsonBytes)));
            }
            break;
          case 'stream_done':
            try {
              final parsed = jsonDecode(jsonStr) as Map<String, dynamic>;
              controller.add(StreamDoneEvent(
                  requestId: parsed['request_id'] as int? ?? 0));
            } catch (_) {
              controller.add(const StreamDoneEvent(requestId: 0));
            }
            if (!cleanedUp) {
              cleanedUp = true;
              nativeCallback.close();
              controller.close();
            }
            break;
          case 'stream_error':
            try {
              final parsed = jsonDecode(jsonStr) as Map<String, dynamic>;
              controller.addError(CatcherHttpError(
                  parsed['error']?.toString() ?? 'Stream error'));
            } catch (_) {
              controller.addError(CatcherHttpError(jsonStr));
            }
            if (!cleanedUp) {
              cleanedUp = true;
              nativeCallback.close();
              controller.close();
            }
            break;
        }
      },
    );

    final methodFfi = _allocFfiString(method);
    final urlFfi = _allocFfiString(path);
    final ctFfi = contentType != null
        ? _allocFfiString(contentType)
        : _allocFfiString('');
    final headersJson = (headers != null && headers.isNotEmpty)
        ? jsonEncode(headers).toNativeUtf8().cast<Char>()
        : nullptr.cast<Char>();
    final bodyPtr = (body != null && body.isNotEmpty)
        ? malloc<Uint8>(body.length)
        : Pointer<Uint8>.fromAddress(0);
    if (body != null && body.isNotEmpty) {
      for (var i = 0; i < body.length; i++) {
        bodyPtr[i] = body[i];
      }
    }

    try {
      fn(
        _handle!,
        methodFfi.ref,
        urlFfi.ref,
        bodyPtr,
        body?.length ?? 0,
        ctFfi.ref,
        headersJson,
        timeoutMs ?? 0,
        nativeCallback.nativeFunction,
        nullptr,
      );
    } catch (e) {
      if (!cleanedUp) {
        cleanedUp = true;
        nativeCallback.close();
      }
      _freeFfiString(methodFfi);
      _freeFfiString(urlFfi);
      _freeFfiString(ctFfi);
      if (headersJson != nullptr) malloc.free(headersJson);
      if (body != null && body.isNotEmpty) malloc.free(bodyPtr);
      rethrow;
    }

    _freeFfiString(methodFfi);
    _freeFfiString(urlFfi);
    _freeFfiString(ctFfi);
    if (headersJson != nullptr) malloc.free(headersJson);
    if (body != null && body.isNotEmpty) malloc.free(bodyPtr);

    // Timeout cleanup
    Future.delayed(const Duration(minutes: 5), () {
      if (!cleanedUp) {
        cleanedUp = true;
        nativeCallback.close();
        if (!controller.isClosed) controller.close();
      }
    });

    return controller.stream;
  }

  /// Build a FfiStringNative on the heap. Caller must call [_freeFfiString] when done.
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

  /// Execute an HTTP request via Rust FFI with async callback bridging.
  ///
  /// Uses the generic `catcher_http_execute` Rust function which accepts
  /// the HTTP method, per-request headers, and per-request timeout.
  Future<HttpResponse> _execute(
    String method,
    String path,
    List<int>? body,
    String? contentType, {
    Map<String, String>? headers,
    int? timeoutMs,
  }) async {
    _ensureHandle();
    final receivePort = ReceivePort();
    final completer = Completer<HttpResponse>();
    bool cleanedUp = false;

    final nativeCallback = NativeCallable<EventCallbackNative>.listener(
      (Pointer<Char> eventType, Pointer<Uint8> eventData, int eventDataLen,
          Pointer<Void> userData) {
        final jsonBytes = eventData.asTypedList(eventDataLen);
        final jsonStr = utf8.decode(jsonBytes, allowMalformed: true);

        _freeEventDataFn(eventType, eventData);

        final Map<String, dynamic> result;
        try {
          result = jsonDecode(jsonStr) as Map<String, dynamic>;
        } catch (_) {
          receivePort.sendPort.send({'error': jsonStr});
          return;
        }
        receivePort.sendPort.send(result);
      },
    );

    late StreamSubscription sub;
    sub = receivePort.listen((message) {
      sub.cancel();
      if (!cleanedUp) {
        cleanedUp = true;
        nativeCallback.close();
        receivePort.close();
      }
      if (!completer.isCompleted) {
        if (message is Map && !message.containsKey('error')) {
          completer.complete(
              HttpResponse.fromJson(Map<String, dynamic>.from(message)));
        } else if (message is Map) {
          completer.completeError(CatcherHttpError(
            message['error']?.toString() ?? 'Unknown error',
          ));
        } else {
          completer.completeError(CatcherHttpError(message.toString()));
        }
      }
    });

    final methodFfi = _allocFfiString(method);
    final urlFfi = _allocFfiString(path);
    final ctFfi = contentType != null
        ? _allocFfiString(contentType)
        : _allocFfiString('');

    final headersJson = (headers != null && headers.isNotEmpty)
        ? jsonEncode(headers).toNativeUtf8().cast<Char>()
        : nullptr.cast<Char>();

    final bodyPtr = (body != null && body.isNotEmpty)
        ? malloc<Uint8>(body.length)
        : Pointer<Uint8>.fromAddress(0);
    if (body != null && body.isNotEmpty) {
      for (var i = 0; i < body.length; i++) {
        bodyPtr[i] = body[i];
      }
    }

    try {
      _executeFn(
        _handle!,
        methodFfi.ref,
        urlFfi.ref,
        bodyPtr,
        body?.length ?? 0,
        ctFfi.ref,
        headersJson,
        timeoutMs ?? 0,
        nativeCallback.nativeFunction,
        nullptr,
      );
    } catch (e) {
      if (!cleanedUp) {
        cleanedUp = true;
        nativeCallback.close();
        receivePort.close();
      }
      _freeFfiString(methodFfi);
      _freeFfiString(urlFfi);
      _freeFfiString(ctFfi);
      if (headersJson != nullptr) malloc.free(headersJson);
      if (body != null && body.isNotEmpty) malloc.free(bodyPtr);
      rethrow;
    }

    _freeFfiString(methodFfi);
    _freeFfiString(urlFfi);
    _freeFfiString(ctFfi);
    if (headersJson != nullptr) malloc.free(headersJson);
    if (body != null && body.isNotEmpty) malloc.free(bodyPtr);

    return completer.future.timeout(
      const Duration(seconds: 30),
      onTimeout: () {
        Future.delayed(const Duration(seconds: 60), () {
          if (!cleanedUp) {
            cleanedUp = true;
            nativeCallback.close();
            receivePort.close();
          }
        });
        if (!completer.isCompleted) {
          completer.completeError(
            TimeoutException('HTTP request timed out after 30s'),
          );
        }
        throw TimeoutException('HTTP request timed out after 30s');
      },
    );
  }
}

// ═══════════════════════════════════════════════════════════════
// Config types
// ═══════════════════════════════════════════════════════════════

class RetryConfig {
  final int maxAttempts;
  final String backoff; // Fixed, Exponential, DecorrelatedJitter
  final int minBackoffMs;
  final int maxBackoffMs;
  final bool jitter;

  const RetryConfig({
    this.maxAttempts = 3,
    this.backoff = 'Exponential',
    this.minBackoffMs = 100,
    this.maxBackoffMs = 10000,
    this.jitter = true,
  });

  Map<String, dynamic> toJson() => {
        'max_attempts': maxAttempts,
        'backoff': backoff,
        'min_backoff_ms': minBackoffMs,
        'max_backoff_ms': maxBackoffMs,
        'jitter': jitter,
      };
}

class CircuitBreakerConfig {
  final int failureThreshold;
  final int successThreshold;
  final int resetTimeoutMs;
  final int halfOpenMaxRequests;

  const CircuitBreakerConfig({
    this.failureThreshold = 5,
    this.successThreshold = 2,
    this.resetTimeoutMs = 30000,
    this.halfOpenMaxRequests = 5,
  });

  Map<String, dynamic> toJson() => {
        'failure_threshold': failureThreshold,
        'success_threshold': successThreshold,
        'reset_timeout_ms': resetTimeoutMs,
        'half_open_max_requests': halfOpenMaxRequests,
      };
}

/// Connection pool configuration (matches Rust PoolConfig)
class PoolConfig {
  final int maxIdlePerHost;
  final int idleTimeoutSecs;
  final bool keepAlive;
  final int keepAliveIntervalSecs;

  const PoolConfig({
    this.maxIdlePerHost = 10,
    this.idleTimeoutSecs = 90,
    this.keepAlive = true,
    this.keepAliveIntervalSecs = 60,
  });

  Map<String, dynamic> toJson() => {
        'max_idle_per_host': maxIdlePerHost,
        'idle_timeout_secs': idleTimeoutSecs,
        'keep_alive': keepAlive,
        'keep_alive_interval_secs': keepAliveIntervalSecs,
      };
}

/// TLS configuration
class TlsConfig {
  final bool rejectUnauthorized;
  final String? caCertPem;
  final String? caCertPath;
  final String? clientCertPem;
  final String? clientCertPath;
  final String? clientKeyPem;
  final String? clientKeyPath;
  final String? tlsSniOverride;
  final String? minTlsVersion;
  final List<String>? pinSha256;

  const TlsConfig({
    this.rejectUnauthorized = true,
    this.caCertPem,
    this.caCertPath,
    this.clientCertPem,
    this.clientCertPath,
    this.clientKeyPem,
    this.clientKeyPath,
    this.tlsSniOverride,
    this.minTlsVersion,
    this.pinSha256,
  });

  Map<String, dynamic> toJson() => {
        'reject_unauthorized': rejectUnauthorized,
        if (caCertPem != null) 'ca_cert_pem': caCertPem,
        if (caCertPath != null) 'ca_cert_path': caCertPath,
        if (clientCertPem != null) 'client_cert_pem': clientCertPem,
        if (clientCertPath != null) 'client_cert_path': clientCertPath,
        if (clientKeyPem != null) 'client_key_pem': clientKeyPem,
        if (clientKeyPath != null) 'client_key_path': clientKeyPath,
        if (tlsSniOverride != null) 'tls_sni_override': tlsSniOverride,
        if (minTlsVersion != null) 'min_tls_version': minTlsVersion,
        if (pinSha256 != null) 'pin_sha256': pinSha256,
      };
}

/// DNS configuration
class DnsConfig {
  final String mode;
  final int cacheSize;
  final int cacheTtlSecs;
  final int negativeTtlSecs;
  final int staleTtlSecs;
  final bool staleOnError;
  final List<String> nameservers;
  final Map<String, String> hostMapping;
  final bool fallbackToDefaultNameservers;

  const DnsConfig({
    this.mode = 'catcher',
    this.cacheSize = 512,
    this.cacheTtlSecs = 300,
    this.negativeTtlSecs = 60,
    this.staleTtlSecs = 3600,
    this.staleOnError = true,
    this.nameservers = const [],
    this.hostMapping = const {},
    this.fallbackToDefaultNameservers = false,
  });

  Map<String, dynamic> toJson() => {
        'mode': mode,
        'cache_size': cacheSize,
        'cache_ttl_secs': cacheTtlSecs,
        'negative_ttl_secs': negativeTtlSecs,
        'stale_ttl_secs': staleTtlSecs,
        'stale_on_error': staleOnError,
        'nameservers': nameservers,
        'host_mapping': hostMapping,
        'fallback_to_default_nameservers': fallbackToDefaultNameservers,
      };
}

/// Proxy authentication
class ProxyAuth {
  final String username;
  final String password;

  const ProxyAuth({required this.username, required this.password});

  Map<String, dynamic> toJson() => {
        'username': username,
        'password': password,
      };
}

/// Proxy configuration
class ProxyConfig {
  final String url;
  final ProxyAuth? auth;
  final List<String> noProxy;

  const ProxyConfig({
    required this.url,
    this.auth,
    this.noProxy = const [],
  });

  Map<String, dynamic> toJson() => {
        'url': url,
        if (auth != null) 'auth': auth!.toJson(),
        'no_proxy': noProxy,
      };
}

/// Redirect configuration
class RedirectConfig {
  final bool follow;
  final int maxRedirects;

  const RedirectConfig({
    this.follow = true,
    this.maxRedirects = 5,
  });

  Map<String, dynamic> toJson() => {
        'follow': follow,
        'max_redirects': maxRedirects,
      };
}

class HttpClientConfig {
  final String baseUrl;
  final int connectTimeoutMs;
  final int responseTimeoutMs;
  final PoolConfig pool;
  final TlsConfig tls;
  final DnsConfig? dns;
  final RetryConfig? retry;
  final CircuitBreakerConfig? circuitBreaker;
  final int maxConcurrency;
  final Map<String, String> defaultHeaders;
  final ProxyConfig? proxy;
  final RedirectConfig? redirect;
  final ProxyAuth? auth;
  final String? bearerToken;
  final bool msgpack;
  final String? networkPathId;

  const HttpClientConfig({
    required this.baseUrl,
    this.connectTimeoutMs = 10000,
    this.responseTimeoutMs = 30000,
    this.pool = const PoolConfig(),
    this.tls = const TlsConfig(),
    this.dns,
    this.retry,
    this.circuitBreaker,
    this.maxConcurrency = 50,
    this.defaultHeaders = const {},
    this.proxy,
    this.redirect,
    this.auth,
    this.bearerToken,
    this.msgpack = false,
    this.networkPathId,
  });

  Map<String, dynamic> toJson() => {
        'base_url': baseUrl,
        'connect_timeout_ms': connectTimeoutMs,
        'response_timeout_ms': responseTimeoutMs,
        'pool': pool.toJson(),
        'tls': tls.toJson(),
        if (dns != null) 'dns': dns!.toJson(),
        if (retry != null) 'retry': retry!.toJson(),
        if (circuitBreaker != null) 'circuit_breaker': circuitBreaker!.toJson(),
        'max_concurrency': maxConcurrency,
        'default_headers': defaultHeaders,
        if (proxy != null) 'proxy': proxy!.toJson(),
        if (redirect != null) 'redirect': redirect!.toJson(),
        if (auth != null) 'auth': auth!.toJson(),
        if (bearerToken != null) 'bearer_token': bearerToken,
        'msgpack': msgpack,
        if (networkPathId != null) 'network_path_id': networkPathId,
      };
}

class HttpResponse {
  final int status;
  final Map<String, String> headers;
  final List<int> body;
  final int elapsedMs;

  const HttpResponse({
    required this.status,
    this.headers = const {},
    this.body = const [],
    this.elapsedMs = 0,
  });

  factory HttpResponse.fromJson(Map<String, dynamic> json) {
    List<int> bodyBytes;
    // 优先读取 base64 编码的 body（Rust FFI 路径），回退兼容旧 JSON number array 格式
    final rawBodyBase64 = json['body_base64'];
    if (rawBodyBase64 is String && rawBodyBase64.isNotEmpty) {
      bodyBytes = base64.decode(rawBodyBase64);
    } else {
      final rawBody = json['body'];
      if (rawBody is List) {
        if (rawBody.isNotEmpty && rawBody.first is int) {
          bodyBytes = rawBody.cast<int>();
        } else {
          bodyBytes = [];
        }
      } else if (rawBody is String) {
        bodyBytes = base64.decode(rawBody);
      } else {
        bodyBytes = [];
      }
    }

    return HttpResponse(
      status: json['status'] as int,
      headers: Map<String, String>.from(json['headers'] ?? {}),
      body: bodyBytes,
      elapsedMs: json['elapsed_ms'] as int? ?? 0,
    );
  }

  /// Convenience: decode body bytes as UTF-8 string
  String get bodyAsString => utf8.decode(body, allowMalformed: true);
}

/// Error thrown when the Rust HTTP client returns an error
class CatcherHttpError implements Exception {
  final String message;
  const CatcherHttpError(this.message);

  @override
  String toString() => 'CatcherHttpError: $message';
}

// ═══════════════════════════════════════════════════════════════
// Stream download events (H-04)
// ═══════════════════════════════════════════════════════════════

/// Base class for streaming download events.
sealed class StreamEvent {
  const StreamEvent();
}

/// Received response headers.
class StreamHeadersEvent extends StreamEvent {
  final int status;
  final Map<String, String> headers;
  final int requestId;
  const StreamHeadersEvent({
    required this.status,
    this.headers = const {},
    this.requestId = 0,
  });
}

/// Received a data chunk (binary).
class StreamChunkEvent extends StreamEvent {
  final List<int> data;
  final int requestId;
  const StreamChunkEvent({this.data = const [], this.requestId = 0});
}

/// Stream completed successfully.
class StreamDoneEvent extends StreamEvent {
  final int requestId;
  const StreamDoneEvent({this.requestId = 0});
}

/// Stream encountered an error.
class StreamErrorEvent extends StreamEvent {
  final String message;
  const StreamErrorEvent(this.message);
}
