/// HTTP 客户端配置
class HttpClientConfig {
  final String baseUrl;
  final int connectTimeoutMs;
  final int responseTimeoutMs;
  final bool keepAlive;
  final int keepAliveIntervalSecs;
  final int maxIdlePerHost;
  final int idleTimeoutSecs;
  final RetryConfig? retry;
  final CircuitBreakerConfig? circuitBreaker;
  final int maxConcurrency;

  const HttpClientConfig({
    this.baseUrl = '',
    this.connectTimeoutMs = 10000,
    this.responseTimeoutMs = 30000,
    this.keepAlive = true,
    this.keepAliveIntervalSecs = 60,
    this.maxIdlePerHost = 10,
    this.idleTimeoutSecs = 90,
    this.retry,
    this.circuitBreaker,
    this.maxConcurrency = 50,
  });

  Map<String, dynamic> toJson() => {
    'base_url': baseUrl,
    'connect_timeout_ms': connectTimeoutMs,
    'response_timeout_ms': responseTimeoutMs,
    'pool': {
      'keep_alive': keepAlive,
      'keep_alive_interval_secs': keepAliveIntervalSecs,
      'max_idle_per_host': maxIdlePerHost,
      'idle_timeout_secs': idleTimeoutSecs,
    },
    if (retry != null) 'retry': retry!.toJson(),
    if (circuitBreaker != null) 'circuit_breaker': circuitBreaker!.toJson(),
    'max_concurrency': maxConcurrency,
  };
}

class RetryConfig {
  final int maxAttempts;
  final String backoff;
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
