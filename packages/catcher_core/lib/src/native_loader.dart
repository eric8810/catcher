import 'dart:ffi';
import 'dart:io';

/// Load the catcher native library.
///
/// - Android: libcatcher_core.so (packaged in APK)
/// - iOS: statically linked via native_assets
/// - macOS: libcatcher_core.dylib
/// - Linux: libcatcher_core.so
/// - Windows: catcher_core.dll
DynamicLibrary loadCatcherLibrary() {
  if (Platform.isAndroid) {
    return DynamicLibrary.open('libcatcher_core.so');
  } else if (Platform.isIOS) {
    return DynamicLibrary.process();
  } else if (Platform.isMacOS) {
    return DynamicLibrary.open('libcatcher_core.dylib');
  } else if (Platform.isWindows) {
    return DynamicLibrary.open('catcher_core.dll');
  } else if (Platform.isLinux) {
    return DynamicLibrary.open('libcatcher_core.so');
  }
  throw UnsupportedError('Unsupported platform: ${Platform.operatingSystem}');
}
