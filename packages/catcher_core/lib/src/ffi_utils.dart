import 'dart:async';
import 'dart:convert';
import 'dart:ffi';
import 'dart:isolate';
import 'dart:typed_data';

import 'package:ffi/ffi.dart';

import 'ffi_bindings.dart';

// ── FFI 字符串 ──

Pointer<FfiStringNative> allocFfiString(String dartString) {
  final encoded = utf8.encode(dartString);
  final native = malloc<Uint8>(encoded.length);
  for (var i = 0; i < encoded.length; i++) { native[i] = encoded[i]; }
  final ffiStr = calloc<FfiStringNative>();
  ffiStr.ref.data = native.cast<Char>();
  ffiStr.ref.len = encoded.length;
  return ffiStr;
}

void freeFfiString(Pointer<FfiStringNative> ffiStr) {
  malloc.free(ffiStr.ref.data);
  calloc.free(ffiStr);
}

// ── Body 字节 ──

Pointer<Uint8> copyBytesToMalloc(List<int>? bytes) {
  if (bytes == null || bytes.isEmpty) return Pointer<Uint8>.fromAddress(0);
  final ptr = malloc<Uint8>(bytes.length);
  for (var i = 0; i < bytes.length; i++) { ptr[i] = bytes[i]; }
  return ptr;
}

// ── Headers ──

Pointer<Char> allocHeaders(Map<String, String>? headers) {
  if (headers == null || headers.isEmpty) return nullptr.cast<Char>();
  return jsonEncode(headers).toNativeUtf8().cast<Char>();
}

// ── 事件回调解析 ──

Map<String, dynamic>? decodeEventCallback(
  Pointer<Char> eventType,
  Pointer<Uint8> eventData,
  int eventDataLen,
  void Function(Pointer<Char>, Pointer<Uint8>) freeEventData,
) {
  final jsonBytes = eventData.asTypedList(eventDataLen);
  final jsonStr = utf8.decode(jsonBytes, allowMalformed: true);
  freeEventData(eventType, eventData);
  try {
    return jsonDecode(jsonStr) as Map<String, dynamic>;
  } catch (_) {
    return null;
  }
}

// ── 统一错误类 ──

class CatcherError implements Exception {
  final String message;
  const CatcherError(this.message);
  @override
  String toString() => 'CatcherError: $message';
}

// ── 异步 FFI 桥接 ──

/// 封装 ReceivePort + NativeCallable + Completer 模式，
/// 消除 http_client / ws_client / quality 中的重复桥接样板。
class FfiAsyncBridge<T> {
  final ReceivePort _port = ReceivePort();
  final Completer<T> _completer = Completer<T>();
  bool _cleanedUp = false;
  NativeCallable<EventCallbackNative>? _callable;

  /// 创建 NativeCallable.listener，自动解码 JSON 事件并发送到内部端口。
  NativeCallable<EventCallbackNative> createCallback(
    void Function(Pointer<Char>, Pointer<Uint8>) freeEventData,
  ) {
    _callable = NativeCallable<EventCallbackNative>.listener(
      (Pointer<Char> eventType, Pointer<Uint8> eventData, int eventDataLen,
          Pointer<Void> userData) {
        final jsonBytes = eventData.asTypedList(eventDataLen);
        final jsonStr = utf8.decode(jsonBytes, allowMalformed: true);
        freeEventData(eventType, eventData);
        try {
          _port.sendPort.send(jsonDecode(jsonStr) as Map<String, dynamic>);
        } catch (_) {
          _port.sendPort.send({'error': jsonStr});
        }
      },
    );
    return _callable!;
  }

  /// 监听端口，收到结果后自动清理并完成 completer。
  /// [fromJson] 将成功结果转换为 T。
  void listen(T Function(Map<String, dynamic>) fromJson) {
    _port.listen((message) {
      if (_cleanedUp) return;
      _cleanup();
      if (!_completer.isCompleted) {
        if (message is Map && !message.containsKey('error')) {
          try {
            _completer.complete(fromJson(Map<String, dynamic>.from(message)));
          } catch (e) {
            _completer.completeError(e);
          }
        } else if (message is Map) {
          _completer.completeError(
            CatcherError(message['error']?.toString() ?? 'Unknown error'));
        } else {
          _completer.completeError(CatcherError(message.toString()));
        }
      }
    });
  }

  void _cleanup() {
    _cleanedUp = true;
    _callable?.close();
    _port.close();
  }

  /// 出错时安全清理（不重复触发 listen 回调）。
  void cleanupOnError() {
    if (!_cleanedUp) _cleanup();
  }

  /// 等待结果，带超时。
  Future<T> get future => _completer.future.timeout(
    const Duration(seconds: 30),
    onTimeout: () {
      Future.delayed(const Duration(seconds: 60), _cleanup);
      if (!_completer.isCompleted) {
        _completer.completeError(
          TimeoutException('Request timed out after 30s'));
      }
      throw TimeoutException('Request timed out after 30s');
    },
  );
}
