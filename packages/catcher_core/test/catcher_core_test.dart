import 'package:flutter_test/flutter_test.dart';
import 'package:catcher_core/catcher_core.dart';

void main() {
  group('HttpClientConfig', () {
    test('default values', () {
      final config = HttpClientConfig();
      expect(config.connectTimeoutMs, 10000);
      expect(config.responseTimeoutMs, 30000);
      expect(config.keepAlive, true);
      expect(config.maxConcurrency, 50);
    });

    test('toJson produces valid JSON', () {
      final config = HttpClientConfig(
        baseUrl: 'https://api.example.com',
        retry: RetryConfig(maxAttempts: 3),
      );
      final json = config.toJson();
      expect(json['base_url'], 'https://api.example.com');
      expect(json['retry']['max_attempts'], 3);
    });

    test('custom retry config', () {
      final retry = RetryConfig(
        maxAttempts: 5,
        backoff: 'Fixed',
        minBackoffMs: 200,
      );
      expect(retry.maxAttempts, 5);
      expect(retry.backoff, 'Fixed');
      final json = retry.toJson();
      expect(json['max_attempts'], 5);
    });

    test('circuit breaker config', () {
      final cb = CircuitBreakerConfig(
        failureThreshold: 3,
        resetTimeoutMs: 60000,
      );
      expect(cb.failureThreshold, 3);
      final json = cb.toJson();
      expect(json['failure_threshold'], 3);
    });
  });

  group('HttpResponse', () {
    test('fromJson parses correctly', () {
      final resp = HttpResponse.fromJson({
        'status': 200,
        'headers': {},
        'body': [104, 101, 108, 108, 111],
        'elapsed_ms': 42,
      });
      expect(resp.status, 200);
      expect(resp.elapsedMs, 42);
      expect(resp.bodyAsString, 'hello');
    });
  });

  group('WsClientConfig', () {
    test('default values', () {
      final config = WsClientConfig();
      expect(config.urls, isEmpty);
      expect(config.perMessageDeflate, false);
      expect(config.handshakeTimeoutMs, 15000);
      expect(config.raceCount, 1);
    });

    test('toJson with reconnect', () {
      final config = WsClientConfig(
        urls: ['wss://example.com'],
        perMessageDeflate: true,
        reconnect: ReconnectConfig(maxAttempts: 3),
      );
      final json = config.toJson();
      expect(json['urls'], ['wss://example.com']);
      expect(json['per_message_deflate'], true);
      expect(json['reconnect']['max_attempts'], 3);
    });
  });

  group('WsEvent', () {
    test('fromJson parses Connected', () {
      final event = WsEvent.fromJson({
        'type': 'Connected',
        'url': 'wss://example.com',
        'latency_ms': 42,
      });
      expect(event.type, 'Connected');
      expect(event.url, 'wss://example.com');
      expect(event.latencyMs, 42);
    });

    test('fromJson parses Disconnected', () {
      final event = WsEvent.fromJson({
        'type': 'Disconnected',
        'code': 1006,
        'reason': 'abnormal',
      });
      expect(event.type, 'Disconnected');
      expect(event.code, 1006);
      expect(event.reason, 'abnormal');
    });

    test('fromJson parses Message', () {
      final event = WsEvent.fromJson({
        'type': 'Message',
        'data': [72, 101, 108, 108, 111],
        'is_binary': false,
      });
      expect(event.type, 'Message');
      expect(event.isBinary, false);
    });
  });

  group('NetworkQualityResult', () {
    test('fromJson parses correctly', () {
      final result = NetworkQualityResult.fromJson({
        'level': 'Good',
        'avg_rtt_ms': 50,
        'jitter_ms': 10,
        'packet_loss_rate': 0.01,
        'connection_type': 'Wifi',
      });
      expect(result.level, 'Good');
      expect(result.avgRttMs, 50);
      expect(result.packetLossRate, 0.01);
      expect(result.connectionType, 'Wifi');
    });
  });
}
