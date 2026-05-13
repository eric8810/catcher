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
///   final client = CatcherHttpClient(HttpClientConfig(
///     baseUrl: 'https://api.example.com',
///     retry: RetryConfig(maxAttempts: 3),
///   ));
///
///   final resp = await client.get('/channels');
///   print('Status: ${resp.status}');
///
///   client.dispose();
/// }
/// ```
library catcher_core;

export 'src/http_client.dart';
export 'src/native_loader.dart';
