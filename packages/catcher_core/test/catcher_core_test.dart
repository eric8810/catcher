import 'dart:typed_data';

import 'package:test/test.dart';
import 'package:catcher_core/catcher_core.dart';

void main() {
  group('HttpClientConfig', () {
    test('requires baseUrl, has sensible defaults', () {
      final config = HttpClientConfig(baseUrl: 'https://api.example.com');
      expect(config.baseUrl, 'https://api.example.com');
      expect(config.connectTimeoutMs, 10000);
      expect(config.responseTimeoutMs, 30000);
      expect(config.pool.keepAlive, true);
      expect(config.maxConcurrency, 50);
    });

    test('toJson produces valid JSON', () {
      final config = HttpClientConfig(
        baseUrl: 'https://api.example.com',
        retry: RetryConfig(maxAttempts: 3),
        dns: DnsConfig(
          mode: 'catcher',
          cacheSize: 1024,
          cacheTtlSecs: 600,
          negativeTtlSecs: 30,
          staleTtlSecs: 1800,
          staleOnError: false,
          nameservers: ['1.1.1.1:53'],
          hostMapping: {'api.example.com': '127.0.0.1'},
        ),
        msgpack: true,
      );
      final json = config.toJson();
      expect(json['base_url'], 'https://api.example.com');
      expect(json['retry']['max_attempts'], 3);
      expect(json['dns']['mode'], 'catcher');
      expect(json['dns']['cache_size'], 1024);
      expect(json['dns']['cache_ttl_secs'], 600);
      expect(json['dns']['negative_ttl_secs'], 30);
      expect(json['dns']['stale_ttl_secs'], 1800);
      expect(json['dns']['stale_on_error'], false);
      expect(json['dns']['nameservers'], ['1.1.1.1:53']);
      expect(json['dns']['host_mapping']['api.example.com'], '127.0.0.1');
      expect(json['msgpack'], true);
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

    test('pool config', () {
      final pool = PoolConfig(
        maxIdlePerHost: 5,
        idleTimeoutSecs: 60,
        keepAlive: false,
      );
      expect(pool.keepAlive, false);
      expect(pool.maxIdlePerHost, 5);
      final json = pool.toJson();
      expect(json['keep_alive'], false);
      expect(json['max_idle_per_host'], 5);
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
    test('requires urls, has sensible defaults', () {
      final config = WsClientConfig(urls: ['wss://example.com']);
      expect(config.urls, ['wss://example.com']);
      expect(config.perMessageDeflate, true);
      expect(config.handshakeTimeoutMs, 15000);
      expect(config.raceCount, 1);
      expect(config.msgpack, false);
    });

    test('toJson with reconnect', () {
      final config = WsClientConfig(
        urls: ['wss://example.com'],
        perMessageDeflate: true,
        reconnect: WsReconnectConfig(maxAttempts: 3),
        dns: DnsConfig(
          mode: 'catcher',
          hostMapping: {'example.com': '127.0.0.1'},
        ),
        msgpack: true,
      );
      final json = config.toJson();
      expect(json['urls'], ['wss://example.com']);
      expect(json['per_message_deflate'], true);
      expect(json['reconnect']['max_attempts'], 3);
      expect(json['dns']['mode'], 'catcher');
      expect(json['dns']['host_mapping']['example.com'], '127.0.0.1');
      expect(json['msgpack'], true);
    });

    test('toJson with application compression', () {
      final config = WsClientConfig(
        urls: ['wss://example.com'],
        applicationCompression: WsApplicationCompressionConfig(
          algorithm: WsApplicationCompressionAlgorithm.zstd,
          thresholdBytes: 2048,
        ),
      );

      final json = config.toJson();
      expect(json['application_compression']['enabled'], true);
      expect(json['application_compression']['algorithm'], 'zstd');
      expect(json['application_compression']['threshold_bytes'], 2048);
    });

    test('WsReconnectConfig defaults', () {
      final rc = WsReconnectConfig();
      expect(rc.initialDelayMs, 500);
      expect(rc.maxDelayMs, 30000);
      expect(rc.backoffMultiplier, 2.0);
      expect(rc.maxAttempts, 20);
    });

    test('WsHeartbeatConfig defaults', () {
      final hb = WsHeartbeatConfig();
      expect(hb.intervalMs, 30000);
      expect(hb.adaptive, true);
      expect(hb.pongTimeoutMs, 10000);
      expect(hb.maxMissedPongs, 3);
    });
  });

  group('WsEvent types', () {
    test('WsMessageEvent.text decodes UTF-8', () {
      final event = WsMessageEvent(
        data: [72, 101, 108, 108, 111],
        isBinary: false,
      );
      expect(event.text, 'Hello');
      expect(event.isBinary, false);
    });

    test('WsConnectedEvent', () {
      final event = WsConnectedEvent(url: 'wss://example.com', latencyMs: 42);
      expect(event.url, 'wss://example.com');
      expect(event.latencyMs, 42);
    });

    test('WsDisconnectedEvent', () {
      final event = WsDisconnectedEvent(code: 1006, reason: 'abnormal');
      expect(event.code, 1006);
      expect(event.reason, 'abnormal');
    });

    test('WsReconnectingEvent', () {
      final event = WsReconnectingEvent(attempt: 2, delayMs: 1000);
      expect(event.attempt, 2);
      expect(event.delayMs, 1000);
    });

    test('WsErrorEvent', () {
      final event = WsErrorEvent('something failed');
      expect(event.message, 'something failed');
    });

    test('WsHeartbeatRttEvent', () {
      final event = WsHeartbeatRttEvent(rttMs: 15);
      expect(event.rttMs, 15);
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

    test('default values from partial JSON', () {
      final result = NetworkQualityResult.fromJson({});
      expect(result.level, 'Bad');
      expect(result.avgRttMs, 0);
      expect(result.jitterMs, 0);
      expect(result.packetLossRate, 0.0);
      expect(result.connectionType, 'Unknown');
    });
  });
}
