import 'dart:ffi';
import 'dart:io';

/// Load the catcher native library.
///
/// Resolution order:
/// 1. Android: libcatcher_ffi.so (packaged in APK)
/// 2. iOS: statically linked via native_assets
/// 3. macOS: libcatcher_ffi.dylib
/// 4. Windows: catcher_ffi.dll
/// 5. Linux: libcatcher_ffi.so
///
/// For local development on Linux/macOS, you can also set the
/// `CATCHER_FFI_PATH` environment variable to the absolute path
/// of the built library.
DynamicLibrary loadCatcherLibrary() {
  // Allow overriding the library path for local testing
  final envPath = Platform.environment['CATCHER_FFI_PATH'];
  if (envPath != null && envPath.isNotEmpty) {
    return DynamicLibrary.open(envPath);
  }

  if (Platform.isAndroid) {
    return DynamicLibrary.open('libcatcher_ffi.so');
  } else if (Platform.isIOS) {
    return DynamicLibrary.process();
  } else if (Platform.isMacOS) {
    return DynamicLibrary.open('libcatcher_ffi.dylib');
  } else if (Platform.isWindows) {
    return DynamicLibrary.open('catcher_ffi.dll');
  } else if (Platform.isLinux) {
    return DynamicLibrary.open('libcatcher_ffi.so');
  }
  throw UnsupportedError('Unsupported platform: ${Platform.operatingSystem}');
}
