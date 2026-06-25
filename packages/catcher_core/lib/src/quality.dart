import 'dart:async';
import 'dart:convert';
import 'dart:ffi';
import 'dart:isolate';

import 'package:ffi/ffi.dart';

import 'ffi_bindings.dart';
import 'ffi_utils.dart';
import 'native_loader.dart';

// Lazy-resolved FFI function handles
DynamicLibrary? _lib;
CatcherEvaluateQualityDart? _evalFn;
CatcherFreeEventDataDart? _freeEventDataFn;

DynamicLibrary _getLib() => _lib ??= loadCatcherLibrary();

CatcherEvaluateQualityDart _eval() =>
    _evalFn ??= _getLib().lookupFunction<CatcherEvaluateQualityNative,
        CatcherEvaluateQualityDart>('catcher_evaluate_quality');

CatcherFreeEventDataDart _freeEventData() =>
    _freeEventDataFn ??= _getLib().lookupFunction<CatcherFreeEventDataNative,
        CatcherFreeEventDataDart>('catcher_free_event_data');

CatcherFreeDataDart? _freeDataFn;
CatcherFreeDataDart _freeData() =>
    _freeDataFn ??= _getLib().lookupFunction<CatcherFreeDataNative,
        CatcherFreeDataDart>('catcher_free_data');

CatcherQualityHistoryDart? _qualityHistoryFn;
CatcherQualityHistoryDart _qualityHistoryFunc() =>
    _qualityHistoryFn ??= _getLib().lookupFunction<CatcherQualityHistoryNative,
        CatcherQualityHistoryDart>('catcher_quality_history');

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

/// Build a heap-allocated FfiStringNative. Caller must free.
// Use shared allocFfiString/freeFfiString from ffi_utils.dart

/// Evaluate network quality to the given host.
Future<NetworkQualityResult> evaluateQuality(String host) async {
  final receivePort = ReceivePort();
  final completer = Completer<String>();
  bool cleanedUp = false;

  final nativeCallback = NativeCallable<EventCallbackNative>.listener(
    (Pointer<Char> eventType, Pointer<Uint8> eventData, int eventDataLen,
        Pointer<Void> userData) {
      if (eventData != nullptr && eventDataLen > 0) {
        final jsonBytes = eventData.asTypedList(eventDataLen);
        final jsonStr = utf8.decode(jsonBytes, allowMalformed: true);
        _freeEventData()(eventType, eventData);
        receivePort.sendPort.send(jsonStr);
      } else {
        _freeEventData()(eventType, eventData);
        receivePort.sendPort.send('{}');
      }
    },
  );

  late StreamSubscription sub;
  sub = receivePort.listen((message) {
    sub.cancel();
    if (!cleanedUp) { cleanedUp = true; nativeCallback.close(); receivePort.close(); }
    if (!completer.isCompleted) completer.complete(message as String);
  });

  final hostFfi = allocFfiString(host);
  try {
    _eval()(hostFfi.ref, nativeCallback.nativeFunction, nullptr);
  } catch (e) {
    if (!cleanedUp) { cleanedUp = true; nativeCallback.close(); receivePort.close(); }
    freeFfiString(hostFfi);
    rethrow;
  }
  freeFfiString(hostFfi);

  final resultJson = await completer.future.timeout(
    const Duration(seconds: 30),
    onTimeout: () {
      Future.delayed(const Duration(seconds: 60), () {
        if (!cleanedUp) { cleanedUp = true; nativeCallback.close(); receivePort.close(); }
      });
      return '{}';
    },
  );

  final parsed = jsonDecode(resultJson) as Map<String, dynamic>;
  return NetworkQualityResult.fromJson(parsed);
}

/// Query the persistent quality sliding window history.
/// Returns a JSON string with rtt_samples and current_level.
String qualityHistory() {
  final ptr = _qualityHistoryFunc()();
  if (ptr == nullptr) return '{}';
  final len = ptr.cast<Utf8>().length;
  final result = ptr.cast<Utf8>().toDartString();
  _freeData()(ptr.cast(), len + 1);
  return result;
}
