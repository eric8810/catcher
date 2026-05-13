import 'dart:typed_data';

/// Pack a Dart value into msgpack binary.
///
/// **Not yet implemented** — waiting for Rust `catcher_pack` FFI export.
/// Throws [UnsupportedError] when called.
Uint8List pack(dynamic value) {
  throw UnsupportedError(
    'pack() is not yet implemented — awaiting Rust catcher_pack FFI symbol',
  );
}

/// Unpack msgpack binary into a Dart value.
///
/// **Not yet implemented** — waiting for Rust `catcher_unpack` FFI export.
/// Throws [UnsupportedError] when called.
dynamic unpack(Uint8List data) {
  throw UnsupportedError(
    'unpack() is not yet implemented — awaiting Rust catcher_unpack FFI symbol',
  );
}
