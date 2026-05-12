import 'dart:async';
import 'dart:convert';
import 'dart:ffi';
import 'dart:isolate';

import '../ffi_bindings.dart' as bindings;
import '../ffi_types.dart';

/// Network quality evaluation result.
class NetworkQualityResult {
  final String level;
  final int avgRttMs;
  final int jitterMs;
  final double packetLossRate;
  final String connectionType;

  const NetworkQualityResult({
    required this.level,
    this.avgRttMs = 0,
    this.jitterMs = 0,
    this.packetLossRate = 0.0,
    this.connectionType = 'Unknown',
  });

  factory NetworkQualityResult.fromJson(Map<String, dynamic> json) =>
      NetworkQualityResult(
        level: json['level'] as String? ?? 'Bad',
        avgRttMs: json['avg_rtt_ms'] as int? ?? 0,
        jitterMs: json['jitter_ms'] as int? ?? 0,
        packetLossRate: (json['packet_loss_rate'] as num?)?.toDouble() ?? 0.0,
        connectionType: json['connection_type'] as String? ?? 'Unknown',
      );
}

void _onQualityCallback(
  Pointer<Char> eventType,
  Pointer<Uint8> eventData,
  int eventDataLen,
  Pointer<Void> userData,
) {
  final port = ReceivePort.fromRawReceivePort(userData.address);
  if (eventData != nullptr && eventDataLen > 0) {
    final json = eventData.cast<Utf8>().toDartString(length: eventDataLen);
    port.send(json);
  } else {
    port.send('{}');
  }
}

/// Evaluate network quality to the given host.
Future<NetworkQualityResult> evaluateQuality(String host) async {
  final receivePort = ReceivePort();
  final hostNative = host.toNativeUtf8();

  bindings.catcherEvaluateQuality(
    hostNative.cast<Char>(),
    Pointer.fromFunction<EventCallbackNative>(_onQualityCallback),
    Pointer.fromAddress(receivePort.sendPort.nativePort),
  );

  final resultJson = await receivePort.first as String;
  receivePort.close();
  calloc.free(hostNative);

  final parsed = jsonDecode(resultJson) as Map<String, dynamic>;
  return NetworkQualityResult.fromJson(parsed);
}
