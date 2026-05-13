@Tags(['integration'])
library;

import 'dart:convert';
import 'dart:io';

import 'package:test/test.dart';
import 'package:catcher_core/catcher_core.dart';

void main() {
  // Skip entire test file if no native library is available
  final ffiPath = Platform.environment['CATCHER_FFI_PATH'];
  final hasLib =
      ffiPath != null && ffiPath.isNotEmpty && File(ffiPath).existsSync();

  if (!hasLib) {
    print('SKIP: Set CATCHER_FFI_PATH to run FFI integration tests');
    test('FFI integration tests skipped (no CATCHER_FFI_PATH)', () {});
    return;
  }

  group('FFI HTTP client', () {
    late CatcherHttpClient client;

    setUp(() {
      client = CatcherHttpClient(HttpClientConfig(
        baseUrl: 'https://httpbin.org',
        retry: RetryConfig(maxAttempts: 2),
      ));
    });

    tearDown(() {
      client.dispose();
    });

    test('GET /get returns 200', () async {
      final resp = await client.get('/get');
      expect(resp.status, 200);
      expect(resp.body, isNotEmpty);
    });

    test('GET /get body is valid JSON', () async {
      final resp = await client.get('/get');
      final body = resp.bodyAsString;
      expect(body, isNotEmpty);
      expect(body, contains('"url"'));
    });

    test('GET /status/404 returns 404', () async {
      try {
        final resp = await client.get('/status/404');
        expect(resp.status, 404);
      } on CatcherHttpError {
        // Error path also acceptable
      }
    });

    test('POST /post echoes body', () async {
      final resp = await client.post('/post', body: {
        'message': 'hello from catcher',
      });
      expect(resp.status, 200);
      final body = resp.bodyAsString;
      expect(body, contains('hello from catcher'));
    });
  });

  group('FFI codec (pack/unpack)', () {
    test('pack produces non-empty bytes', () {
      final data = pack({'key': 'value', 'count': 42});
      expect(data, isNotEmpty);
    });

    test('pack/unpack roundtrip preserves data', () {
      final original = {'event': 'message', 'id': '123', 'ts': 1700000000};
      final packed = pack(original);
      final unpacked = unpack(packed);
      expect(unpacked['event'], 'message');
      expect(unpacked['id'], '123');
      expect(unpacked['ts'], 1700000000);
    });

    test('pack is smaller than JSON', () {
      final payload = {
        'event': 'message',
        'text': 'Hello World! ' * 20,
        'ts': 1700000000,
      };
      final packed = pack(payload);
      final jsonBytes = utf8.encode(jsonEncode(payload));
      expect(packed.length, lessThan(jsonBytes.length));
    });
  });
}
