import 'dart:ffi';
import 'dart:io';

import 'ffi_types.dart';

/// Platform-aware dynamic library loader for catcher_core.
DynamicLibrary loadCatcherLibrary() {
  if (Platform.isAndroid) {
    return DynamicLibrary.open('libcatcher_core.so');
  } else if (Platform.isIOS) {
    // iOS: statically linked via Native Assets
    return DynamicLibrary.process();
  } else if (Platform.isMacOS) {
    return DynamicLibrary.open('libcatcher_core.dylib');
  } else if (Platform.isWindows) {
    return DynamicLibrary.open('catcher_core.dll');
  } else if (Platform.isLinux) {
    return DynamicLibrary.open('libcatcher_core.so');
  }
  throw UnsupportedError(
    'Unsupported platform: ${Platform.operatingSystem}',
  );
}
