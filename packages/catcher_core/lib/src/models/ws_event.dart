/// WebSocket 事件
class WsEvent {
  final String type;
  final String? url;
  final int? latencyMs;
  final int? code;
  final String? reason;
  final int? attempt;
  final int? delayMs;
  final List<int>? data;
  final bool? isBinary;
  final String? message;
  final int? rttMs;

  const WsEvent({
    required this.type,
    this.url,
    this.latencyMs,
    this.code,
    this.reason,
    this.attempt,
    this.delayMs,
    this.data,
    this.isBinary,
    this.message,
    this.rttMs,
  });

  factory WsEvent.fromJson(Map<String, dynamic> json) {
    final type = json['type'] as String?;
    switch (type) {
      case 'Connected':
        return WsEvent(
          type: 'Connected',
          url: json['url'] as String?,
          latencyMs: json['latency_ms'] as int?,
        );
      case 'Disconnected':
        return WsEvent(
          type: 'Disconnected',
          code: json['code'] as int?,
          reason: json['reason'] as String?,
        );
      case 'Reconnecting':
        return WsEvent(
          type: 'Reconnecting',
          attempt: json['attempt'] as int?,
          delayMs: json['delay_ms'] as int?,
        );
      case 'Message':
        return WsEvent(
          type: 'Message',
          data: (json['data'] as List<dynamic>?)?.cast<int>(),
          isBinary: json['is_binary'] as bool?,
        );
      case 'Error':
        return WsEvent(
          type: 'Error',
          message: json['message'] as String?,
        );
      case 'HeartbeatRtt':
        return WsEvent(
          type: 'HeartbeatRtt',
          rttMs: json['rtt_ms'] as int?,
        );
      default:
        return WsEvent(type: type ?? 'Unknown');
    }
  }
}
