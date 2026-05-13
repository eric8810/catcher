import 'dart:async';
import 'dart:convert';
import 'dart:ffi';
import 'dart:isolate';

import 'package:ffi/ffi.dart';

import 'ffi_bindings.dart';
import 'native_loader.dart';

/// Dart wrapper around the Rust catcher HTTP client via C ABI.
///
/// Usage:
/// ```dart
/// final client = CatcherHttpClient(HttpClientConfig(
///   baseUrl: 'https://api.example.com',
///   retry: RetryConfig(maxAttempts: 3),
/// ));
/// final resp = await client.get('/channels');
/// print('Status: ${resp.status}, Body: ${resp.bodyAsString}');
/// client.dispose();
/// ```
class CatcherHttpClient {
  Pointer<Void>? _handle;
  late final DynamicLibrary _lib;
  late final CatcherHttpClientCreateDart _create;
  late final CatcherHttpClientDestroyDart _destroy;
  late final CatcherHttpExecuteDart _executeFn;
  late final CatcherFreeEventDataDart _freeEventDataFn;

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

    final configJson = jsonEncode(config.toJson()).toNativeUtf8();
    _handle = _create(configJson.cast<Char>());
    malloc.free(configJson);

    if (_handle == nullptr) {
      throw StateError('Failed to create HTTP client — invalid config or Rust init error');
    }
  }

  /// GET request
  Future<HttpResponse> get(String path) async {
    return _execute('GET', path, null, null);
  }

  /// POST request
  Future<HttpResponse> post(String path,
      {Map<String, dynamic>? body, String contentType = 'application/json'}) async {
    final bodyBytes = body != null ? utf8.encode(jsonEncode(body)) : null;
    return _execute('POST', path, bodyBytes, contentType);
  }

  /// PUT request
  Future<HttpResponse> put(String path,
      {Map<String, dynamic>? body, String contentType = 'application/json'}) async {
    final bodyBytes = body != null ? utf8.encode(jsonEncode(body)) : null;
    return _execute('PUT', path, bodyBytes, contentType);
  }

  /// DELETE request
  Future<HttpResponse> delete(String path) async {
    return _execute('DELETE', path, null, null);
  }

  /// PATCH request
  Future<HttpResponse> patch(String path,
      {Map<String, dynamic>? body, String contentType = 'application/json'}) async {
    final bodyBytes = body != null ? utf8.encode(jsonEncode(body)) : null;
    return _execute('PATCH', path, bodyBytes, contentType);
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

  /// Build a FfiStringNative on the heap. Caller must call [freeFfiString] when done.
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
  /// the HTTP method as a parameter, supporting GET/POST/PUT/DELETE/PATCH.
  Future<HttpResponse> _execute(
    String method,
    String path,
    List<int>? body,
    String? contentType,
  ) async {
    _ensureHandle();
    final receivePort = ReceivePort();
    final completer = Completer<HttpResponse>();
    bool cleanedUp = false;

    final nativeCallback = NativeCallable<EventCallbackNative>.listener(
      (Pointer<Char> eventType, Pointer<Uint8> eventData, int eventDataLen,
          Pointer<Void> userData) {
        // Copy data immediately — pointers will be freed below
        final jsonBytes = eventData.asTypedList(eventDataLen);
        final jsonStr = utf8.decode(jsonBytes, allowMalformed: true);

        // Free the CStrings that Rust leaked via CString::into_raw()
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

    // Prepare FFI strings for method, URL, and content type
    final methodFfi = _allocFfiString(method);
    final urlFfi = _allocFfiString(path);
    final ctFfi = contentType != null
        ? _allocFfiString(contentType)
        : _allocFfiString('');

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
      if (body != null && body.isNotEmpty) malloc.free(bodyPtr);
      rethrow;
    }

    _freeFfiString(methodFfi);
    _freeFfiString(urlFfi);
    _freeFfiString(ctFfi);
    if (body != null && body.isNotEmpty) malloc.free(bodyPtr);

    return completer.future.timeout(
      const Duration(seconds: 30),
      onTimeout: () {
        // Complete with error but do NOT close nativeCallback here —
        // Rust might still invoke it, and closing causes UB.
        // Safety-net: schedule a forced cleanup after 60s
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

class HttpClientConfig {
  final String baseUrl;
  final int connectTimeoutMs;
  final int responseTimeoutMs;
  final PoolConfig pool;
  final RetryConfig? retry;
  final CircuitBreakerConfig? circuitBreaker;
  final int maxConcurrency;

  const HttpClientConfig({
    required this.baseUrl,
    this.connectTimeoutMs = 10000,
    this.responseTimeoutMs = 30000,
    this.pool = const PoolConfig(),
    this.retry,
    this.circuitBreaker,
    this.maxConcurrency = 50,
  });

  Map<String, dynamic> toJson() => {
        'base_url': baseUrl,
        'connect_timeout_ms': connectTimeoutMs,
        'response_timeout_ms': responseTimeoutMs,
        'pool': pool.toJson(),
        if (retry != null) 'retry': retry!.toJson(),
        if (circuitBreaker != null)
          'circuit_breaker': circuitBreaker!.toJson(),
        'max_concurrency': maxConcurrency,
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
    final rawBody = json['body'];
    List<int> bodyBytes;
    if (rawBody is List) {
      // Rust sends Vec<u8> which serde_json may serialize as base64 or array
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
