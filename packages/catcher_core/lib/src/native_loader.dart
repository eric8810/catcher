import 'dart:ffi';
import 'dart:io';

/// Load the catcher native library.
///
/// Resolution order:
/// 1. CATCHER_FFI_PATH environment variable (local dev/testing)
/// 2. iOS/macOS: catcher_ffi.framework/catcher_ffi from the bundled XCFramework
/// 3. Android/Linux: libcatcher_ffi.so from the app bundle
/// 4. Windows: catcher_ffi.dll from the app bundle
DynamicLibrary loadCatcherLibrary() {
  final envPath = Platform.environment['CATCHER_FFI_PATH'];
  if (envPath != null && envPath.isNotEmpty) {
    return DynamicLibrary.open(envPath);
  }

  if (Platform.isIOS || Platform.isMacOS) {
    return DynamicLibrary.open('catcher_ffi.framework/catcher_ffi');
  }

  if (Platform.isAndroid || Platform.isLinux) {
    return DynamicLibrary.open('libcatcher_ffi.so');
  }

  if (Platform.isWindows) {
    return DynamicLibrary.open('catcher_ffi.dll');
  }

  throw UnsupportedError('Unsupported platform: ${Platform.operatingSystem}');
}
