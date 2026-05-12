/// WebSocket 客户端配置
class WsClientConfig {
  final List<String> urls;
  final bool perMessageDeflate;
  final int handshakeTimeoutMs;
  final ReconnectConfig? reconnect;
  final int raceCount;

  const WsClientConfig({
    this.urls = const [],
    this.perMessageDeflate = false,
    this.handshakeTimeoutMs = 15000,
    this.reconnect,
    this.raceCount = 1,
  });

  Map<String, dynamic> toJson() => {
    'urls': urls,
    'per_message_deflate': perMessageDeflate,
    'handshake_timeout_ms': handshakeTimeoutMs,
    if (reconnect != null) 'reconnect': reconnect!.toJson(),
    'race_count': raceCount,
  };
}

class ReconnectConfig {
  final int initialDelayMs;
  final int maxDelayMs;
  final double backoffMultiplier;
  final int maxAttempts;

  const ReconnectConfig({
    this.initialDelayMs = 500,
    this.maxDelayMs = 30000,
    this.backoffMultiplier = 2.0,
    this.maxAttempts = 20,
  });

  Map<String, dynamic> toJson() => {
    'initial_delay_ms': initialDelayMs,
    'max_delay_ms': maxDelayMs,
    'backoff_multiplier': backoffMultiplier,
    'max_attempts': maxAttempts,
  };
}
