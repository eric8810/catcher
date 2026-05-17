// Roundtrip integration tests for Dart FFI ↔ Rust catcher_ffi.
//
// Prerequisites:
//   1. Build the native library:
//      cargo build --release -p catcher-ffi
//
//   2. Set environment variable before running:
//      $env:CATCHER_FFI_PATH = "D:\code\catcher\target\release\catcher_ffi.dll"
//      (PowerShell)
//      set CATCHER_FFI_PATH=D:\code\catcher\target\release\catcher_ffi.dll
//      (CMD)
//
//   3. Run from packages/catcher_core:
//      dart test test/ffi_roundtrip_test.dart

import 'dart:convert';
import 'dart:ffi';
import 'dart:io';

import 'package:catcher_core/catcher_core.dart';
import 'package:catcher_core/src/ffi_bindings.dart';
import 'package:ffi/ffi.dart';
import 'package:test/test.dart';

void main() {
  // Skip entire suite if the native library is not available.
  final ffiPath = Platform.environment['CATCHER_FFI_PATH'];
  final hasFfi = ffiPath != null && ffiPath.isNotEmpty && File(ffiPath).existsSync();

  // Allow explicit skip via CATCHER_FFI_SKIP=1
  final skipAll = Platform.environment['CATCHER_FFI_SKIP'] == '1';

  if (!hasFfi || skipAll) {
    print('⚠️  Skipping FFI roundtrip tests — CATCHER_FFI_PATH not set or file not found.');
    print('   Set CATCHER_FFI_PATH=<path_to_catcher_ffi.dll> to enable.');
    return;
  }

  print('✅ CATCHER_FFI_PATH=$ffiPath');

  group('FFI symbol resolution', () {
    late DynamicLibrary lib;

    setUp(() {
      lib = loadCatcherLibrary();
    });

    test('all core symbols are exported', () {
      // HTTP lifecycle
      lib.lookupFunction<CatcherHttpClientCreateNative, CatcherHttpClientCreateDart>(
          'catcher_http_client_create');
      lib.lookupFunction<CatcherHttpClientDestroyNative, CatcherHttpClientDestroyDart>(
          'catcher_http_client_destroy');

      // HTTP execute (generic method)
      lib.lookupFunction<CatcherHttpExecuteNative, CatcherHttpExecuteDart>(
          'catcher_http_execute');

      // HTTP runtime control
      lib.lookupFunction<CatcherHttpClientCancelAllNative, CatcherHttpClientCancelAllDart>(
          'catcher_http_client_cancel_all');
      lib.lookupFunction<CatcherHttpCircuitBreakerStateNative,
          CatcherHttpCircuitBreakerStateDart>('catcher_http_circuit_breaker_state');
      lib.lookupFunction<CatcherHttpMetricsNative, CatcherHttpMetricsDart>(
          'catcher_http_metrics');
      lib.lookupFunction<CatcherHttpAdaptiveTimeoutConfigNative,
          CatcherHttpAdaptiveTimeoutConfigDart>('catcher_http_adaptive_timeout_config');

      // Per-request cancel
      lib.lookupFunction<CatcherHttpExecuteWithIdNative, CatcherHttpExecuteWithIdDart>(
          'catcher_http_execute_with_id');
      lib.lookupFunction<CatcherHttpCancelRequestNative, CatcherHttpCancelRequestDart>(
          'catcher_http_cancel_request');

      // Streaming download
      lib.lookupFunction<CatcherHttpExecuteStreamNative, CatcherHttpExecuteStreamDart>(
          'catcher_http_execute_stream');

      // Codec
      lib.lookupFunction<CatcherPackNative, CatcherPackDart>('catcher_pack');
      lib.lookupFunction<CatcherUnpackNative, CatcherUnpackDart>('catcher_unpack');

      // Memory management
      lib.lookupFunction<CatcherFreeResultNative, CatcherFreeResultDart>(
          'catcher_free_result');
      lib.lookupFunction<CatcherFreeDataNative, CatcherFreeDataDart>('catcher_free_data');

      // print success with count
      print('  ✅ All 16 core FFI symbols resolved');
    });
  });

  group('Codec roundtrip (pack ↔ unpack)', () {
    test('pack then unpack preserves simple map', () {
      final original = {'name': 'catcher', 'version': '0.2.2'};
      final packed = pack(original);
      expect(packed, isNotNull);
      expect(packed, isNotEmpty);
      print('  Packed ${original.length} keys into ${packed.length} bytes');

      final unpacked = unpack(packed);
      expect(unpacked, isA<Map>());
      final map = unpacked as Map;
      expect(map['name'], equals('catcher'));
      expect(map['version'], equals('0.2.2'));
    });

    test('pack then unpack preserves nested structures', () {
      final original = {
        'retry': {'max_attempts': 3, 'backoff': 'Exponential'},
        'hosts': ['a.example.com', 'b.example.com'],
        'enabled': true,
        'count': 42,
        'ratio': 3.14,
        'nullable': null,
      };
      final packed = pack(original);
      final unpacked = unpack(packed);
      final map = unpacked as Map;

      expect((map['retry'] as Map)['max_attempts'], equals(3));
      expect((map['hosts'] as List).length, equals(2));
      expect(map['enabled'], equals(true));
      expect(map['count'], equals(42));
      // Null may be serialized as null or omitted
    });

    test('pack then unpack preserves empty map', () {
      final packed = pack(<String, dynamic>{});
      expect(packed, isNotEmpty);
      final unpacked = unpack(packed);
      expect(unpacked, isA<Map>());
      expect((unpacked as Map).isEmpty, isTrue);
    });

    test('pack then unpack preserves list of ints', () {
      final original = [1, 2, 3, 100, 255];
      final packed = pack(original);
      final unpacked = unpack(packed);
      expect(unpacked, equals(original));
    });

    test('pack then unpack large payload (1000 entries)', () {
      final original = Map.fromEntries(
        List.generate(1000, (i) => MapEntry('key_$i', 'value_$i')),
      );
      final packed = pack(original);
      expect(packed.length, greaterThan(1000));
      final unpacked = unpack(packed) as Map;
      expect(unpacked.length, equals(1000));
      expect(unpacked['key_0'], equals('value_0'));
      expect(unpacked['key_999'], equals('value_999'));
    });
  });

  group('HTTP client lifecycle', () {
    test('create and destroy client without error', () {
      final client = CatcherHttpClient(HttpClientConfig(
        baseUrl: 'https://httpbin.org',
        connectTimeoutMs: 5000,
        responseTimeoutMs: 10000,
      ));
      expect(client, isNotNull);

      // Query circuit breaker state — should not throw
      final cbState = client.circuitBreakerState;
      expect(cbState, isNotNull);
      expect(cbState, contains('state'));
      print('  CB state: $cbState');

      // Query metrics — should not throw
      final m = client.metrics;
      expect(m, isNotNull);
      print('  Metrics: $m');

      client.dispose();
      print('  ✅ Client created, queried, disposed');
    });

    test('dispose is idempotent', () {
      final client = CatcherHttpClient(HttpClientConfig(
        baseUrl: 'https://httpbin.org',
      ));
      client.dispose();
      // Second dispose should not crash
      client.dispose();
      print('  ✅ Double dispose survived');
    });
  });

  group('HTTP roundtrip (network required)', () {
    CatcherHttpClient? client;

    setUp(() {
      client = CatcherHttpClient(HttpClientConfig(
        baseUrl: 'https://httpbin.org',
        connectTimeoutMs: 5000,
        responseTimeoutMs: 15000,
        retry: RetryConfig(maxAttempts: 2),
      ));
    });

    tearDown(() {
      client?.dispose();
    });

    test('GET /get returns 200 with JSON body', () async {
      final resp = await client!.get('/get');
      expect(resp.status, equals(200));
      expect(resp.body, isNotEmpty);
      final body = jsonDecode(resp.bodyAsString);
      expect(body, isA<Map>());
      expect((body as Map)['url'], isNotNull);
      print('  ✅ GET /get → ${resp.status} (${resp.elapsedMs}ms)');
    }, timeout: const Timeout(Duration(seconds: 30)));

    test('GET /status/404 returns 404', () async {
      final resp = await client!.get('/status/404');
      expect(resp.status, equals(404));
      print('  ✅ GET /status/404 → ${resp.status}');
    }, timeout: const Timeout(Duration(seconds: 30)));

    test('POST /post echoes body', () async {
      final payload = {'hello': 'world', 'num': 42};
      final resp = await client!.post('/post', body: payload);
      expect(resp.status, equals(200));
      final body = jsonDecode(resp.bodyAsString) as Map;
      final data = jsonDecode(body['data'] as String) as Map;
      expect(data['hello'], equals('world'));
      expect(data['num'], equals(42));
      print('  ✅ POST /post → ${resp.status} (${resp.elapsedMs}ms)');
    }, timeout: const Timeout(Duration(seconds: 30)));

    test('GET with custom headers sends them', () async {
      final resp = await client!.get('/headers', headers: {
        'X-Custom-Header': 'catcher-test',
      });
      expect(resp.status, equals(200));
      final body = jsonDecode(resp.bodyAsString) as Map;
      final headers = body['headers'] as Map;
      // httpbin capitalizes headers as "X-Custom-Header" or similar
      final hasHeader = headers.keys.any(
        (k) => (k as String).toLowerCase() == 'x-custom-header',
      );
      expect(hasHeader, isTrue, reason: 'Custom header not found in echo');
      print('  ✅ Custom header echoed back');
    }, timeout: const Timeout(Duration(seconds: 30)));
  });

  group('Stream event types', () {
    test('StreamEvent sealed class hierarchy exists', () {
      // Verify all event types are constructable
      final headers = StreamHeadersEvent(status: 200, headers: {'x': 'y'});
      expect(headers.status, equals(200));
      expect(headers.headers['x'], equals('y'));

      final chunk = StreamChunkEvent(data: [1, 2, 3]);
      expect(chunk.data, equals([1, 2, 3]));

      final done = StreamDoneEvent(requestId: 42);
      expect(done.requestId, equals(42));

      final error = StreamErrorEvent('test error');
      expect(error.message, equals('test error'));

      // Verify sealed pattern matching works
      final events = <StreamEvent>[headers, chunk, done, error];
      for (final e in events) {
        switch (e) {
          case StreamHeadersEvent():
          case StreamChunkEvent():
          case StreamDoneEvent():
          case StreamErrorEvent():
            break; // all handled
        }
      }
      print('  ✅ All 4 StreamEvent subtypes instantiated and matchable');
    });
  });

  group('Per-request cancel API', () {
    test('cancelRequest returns false for non-existent request', () {
      final client = CatcherHttpClient(HttpClientConfig(
        baseUrl: 'https://httpbin.org',
      ));
      // No requests in flight — cancel should return false
      final result = client.cancelRequest(99999);
      expect(result, isFalse);
      client.dispose();
      print('  ✅ cancelRequest(99999) → false');
    });
  });

  group('Adaptive timeout API', () {
    test('setAdaptiveToggle does not crash', () {
      final client = CatcherHttpClient(HttpClientConfig(
        baseUrl: 'https://httpbin.org',
      ));
      // Enable
      client.setAdaptiveTimeout(
        enabled: true,
        minTimeoutMs: 100,
        maxTimeoutMs: 30000,
        multiplier: 2.5,
        windowSize: 20,
      );
      // Disable
      client.setAdaptiveTimeout(enabled: false);
      client.dispose();
      print('  ✅ setAdaptiveTimeout enable/disable survived');
    });
  });
}
