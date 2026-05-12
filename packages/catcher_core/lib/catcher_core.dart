/// catcher_core — Resilient HTTP/WebSocket client backed by Rust for Flutter.
///
/// ```dart
/// import 'package:catcher_core/catcher_core.dart';
///
/// final client = CatcherHttpClient(HttpClientConfig(
///   baseUrl: 'https://api.example.com',
///   keepAlive: true,
///   retry: RetryConfig(maxAttempts: 3),
/// ));
///
/// final resp = await client.get('/channels');
/// print('${resp.status}: ${resp.bodyAsString}');
/// ```

export 'src/http_client.dart';
export 'src/ws_client.dart';
export 'src/codec.dart';
export 'src/quality.dart';

export 'src/models/http_config.dart'
    show HttpClientConfig, RetryConfig, CircuitBreakerConfig;
export 'src/models/http_response.dart' show HttpResponse;
export 'src/models/ws_config.dart' show WsClientConfig, ReconnectConfig;
export 'src/models/ws_event.dart' show WsEvent;
