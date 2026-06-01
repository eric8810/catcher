/// Catcher — Resilient HTTP/WebSocket client for Flutter
///
/// Backed by Rust core via dart:ffi.
///
/// ## Quick Start
///
/// ```dart
/// import 'package:catcher_core/catcher_core.dart';
///
/// void main() async {
///   // HTTP client
///   final client = CatcherHttpClient(HttpClientConfig(
///     baseUrl: 'https://api.example.com',
///     retry: RetryConfig(maxAttempts: 3),
///     pool: PoolConfig(keepAlive: true),
///   ));
///
///   final resp = await client.get('/channels');
///   print('Status: ${resp.status}, Body: ${resp.bodyAsString}');
///
///   client.dispose();
///
///   // WebSocket client
///   final ws = CatcherWsClient(WsClientConfig(
///     urls: ['wss://echo.example.com'],
///     reconnect: WsReconnectConfig(initialDelayMs: 1000),
///   ));
///
///   ws.events.listen((event) {
///     if (event is WsMessageEvent) {
///       print('Received: ${event.text}');
///     }
///   });
///
///   ws.sendText('hello');
///   await Future.delayed(Duration(seconds: 5));
///   ws.dispose();
/// }
/// ```
library catcher_core;

// HTTP client
export 'src/http_client.dart'
    show
        CatcherHttpClient,
        HttpClientConfig,
        RetryConfig,
        CircuitBreakerConfig,
        PoolConfig,
        TlsConfig,
        DnsConfig,
        ProxyConfig,
        ProxyAuth,
        RedirectConfig,
        HttpResponse,
        CatcherHttpError,
        StreamEvent,
        StreamHeadersEvent,
        StreamChunkEvent,
        StreamDoneEvent,
        StreamErrorEvent;

// WebSocket client
export 'src/ws_client.dart'
    show
        CatcherWsClient,
        WsClientConfig,
        WsApplicationCompressionAlgorithm,
        WsApplicationCompressionConfig,
        WsReconnectConfig,
        WsHeartbeatConfig,
        WsEvent,
        WsConnectedEvent,
        WsDisconnectedEvent,
        WsReconnectingEvent,
        WsMessageEvent,
        WsErrorEvent,
        WsHeartbeatRttEvent,
        CatcherWsError;

// SSE client
export 'src/sse_client.dart'
    show
        CatcherSseClient,
        SseClientConfig,
        SseReconnectConfig,
        SseEvent,
        SseOpenEvent,
        SseDataEvent,
        SseErrorEvent,
        SseCloseEvent,
        SseReadyState;

// Network quality
export 'src/quality.dart' show NetworkQualityResult, evaluateQuality, qualityHistory;

// Binary codec
export 'src/codec.dart' show pack, unpack;

// FFI loader
export 'src/native_loader.dart' show loadCatcherLibrary;
