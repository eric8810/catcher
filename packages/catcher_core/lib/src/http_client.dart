import 'dart:async';
import 'dart:convert';
import 'dart:ffi';
import 'dart:isolate';

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
/// client.dispose();
/// ```
class CatcherHttpClient {
  late final Pointer<Void> _handle;
  late final DynamicLibrary _lib;
  late final Pointer<Void> Function(Pointer<Char>) _create;
  late final void Function(Pointer<Void>) _destroy;

  CatcherHttpClient(HttpClientConfig config) {
    _lib = loadCatcherLibrary();

    _create = _lib
        .lookup<NativeFunction<CatcherHttpClientCreateNative>>(
          'catcher_http_client_create',
        )
        .asFunction();

    _destroy = _lib
        .lookup<NativeFunction<CatcherHttpClientDestroyNative>>(
          'catcher_http_client_destroy',
        )
        .asFunction();

    final configJson = jsonEncode(config.toJson()).toNativeUtf8();
    _handle = _create(configJson.cast<Char>());
    malloc.free(configJson);
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
    _destroy(_handle);
  }

  // ── Internal ──

  Future<HttpResponse> _execute(
    String method,
    String path,
    List<int>? body,
    String? contentType,
  ) async {
    final receivePort = ReceivePort();
    final completer = Completer<HttpResponse>();

    receivePort.listen((result) {
      if (result is Map) {
        completer.complete(HttpResponse.fromJson(result));
      } else {
        completer.completeError(result);
      }
      receivePort.close();
    });

    final request = jsonEncode({
      'method': method,
      'url': path,
      if (body != null) 'body': base64Encode(body),
      if (contentType != null) 'content_type': contentType,
    });

    // For now, use get/post lookup based on method
    if (method == 'GET') {
      final getFn = _lib
          .lookup<NativeFunction<CatcherHttpGetNative>>('catcher_http_get')
          .asFunction<void Function(Pointer<Void>, Pointer<Char>,
              Pointer<NativeFunction<HttpEventCallbackNative>>, Pointer<Void>)>();

      final urlNative = path.toNativeUtf8();
      // Note: real implementation needs a proper C callback trampoline
      // For now, this is a skeleton showing the interface
      malloc.free(urlNative);
    }

    return completer.future.timeout(
      const Duration(seconds: 30),
      onTimeout: () => throw TimeoutException('Request timed out'),
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

class HttpClientConfig {
  final String baseUrl;
  final int connectTimeoutMs;
  final int responseTimeoutMs;
  final RetryConfig? retry;
  final CircuitBreakerConfig? circuitBreaker;
  final int maxConcurrency;

  const HttpClientConfig({
    required this.baseUrl,
    this.connectTimeoutMs = 10000,
    this.responseTimeoutMs = 30000,
    this.retry,
    this.circuitBreaker,
    this.maxConcurrency = 50,
  });

  Map<String, dynamic> toJson() => {
        'base_url': baseUrl,
        'connect_timeout_ms': connectTimeoutMs,
        'response_timeout_ms': responseTimeoutMs,
        if (retry != null) 'retry': retry!.toJson(),
        if (circuitBreaker != null) 'circuit_breaker': circuitBreaker!.toJson(),
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

  factory HttpResponse.fromJson(Map<String, dynamic> json) => HttpResponse(
        status: json['status'] as int,
        headers: Map<String, String>.from(json['headers'] ?? {}),
        body: (json['body'] as List<dynamic>?)?.cast<int>() ?? [],
        elapsedMs: json['elapsed_ms'] as int? ?? 0,
      );
}
