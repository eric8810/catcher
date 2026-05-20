#!/usr/bin/env bash
set -euo pipefail

PACKAGE_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
REPO_ROOT="$(cd "$PACKAGE_DIR/../.." && pwd)"
PACKAGES_DIR="$REPO_ROOT/packages"

ANDROID_API="${ANDROID_API:-24}"
ANDROID_NDK_HOME="${ANDROID_NDK_HOME:-${ANDROID_NDK_ROOT:-${ANDROID_NDK:-}}}"
IPHONEOS_DEPLOYMENT_TARGET="${IPHONEOS_DEPLOYMENT_TARGET:-13.0}"
MACOSX_DEPLOYMENT_TARGET="${MACOSX_DEPLOYMENT_TARGET:-10.15}"

build_catcher_ffi() {
  local target="$1"
  cargo build --release -p catcher-ffi --target "$target" --manifest-path "$PACKAGES_DIR/Cargo.toml"
}

require_command() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "Missing required command: $1" >&2
    exit 1
  }
}

copy_file() {
  local source="$1"
  local dest="$2"
  test -f "$source" || {
    echo "Missing expected build artifact: $source" >&2
    exit 1
  }
  mkdir -p "$(dirname "$dest")"
  cp "$source" "$dest"
}

create_framework() {
  local source="$1"
  local framework_dir="$2"
  local bundle_id="$3"

  rm -rf "$framework_dir"
  mkdir -p "$framework_dir"
  cp "$source" "$framework_dir/catcher_ffi"
  install_name_tool -id "@rpath/catcher_ffi.framework/catcher_ffi" "$framework_dir/catcher_ffi"
  cat > "$framework_dir/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleDevelopmentRegion</key>
  <string>en</string>
  <key>CFBundleExecutable</key>
  <string>catcher_ffi</string>
  <key>CFBundleIdentifier</key>
  <string>${bundle_id}</string>
  <key>CFBundleInfoDictionaryVersion</key>
  <string>6.0</string>
  <key>CFBundleName</key>
  <string>catcher_ffi</string>
  <key>CFBundlePackageType</key>
  <string>FMWK</string>
  <key>CFBundleShortVersionString</key>
  <string>1.0</string>
  <key>CFBundleVersion</key>
  <string>1</string>
</dict>
</plist>
PLIST
}

build_android() {
  test -n "$ANDROID_NDK_HOME" || {
    echo "Set ANDROID_NDK_HOME or ANDROID_NDK_ROOT to build Android binaries." >&2
    exit 1
  }

  local host_tag
  case "$(uname -s)" in
    Darwin) host_tag="darwin-x86_64" ;;
    Linux) host_tag="linux-x86_64" ;;
    *) echo "Unsupported Android build host: $(uname -s)" >&2; exit 1 ;;
  esac

  local toolchain="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/$host_tag/bin"
  export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$toolchain/aarch64-linux-android${ANDROID_API}-clang"
  export CARGO_TARGET_ARMV7_LINUX_ANDROIDEABI_LINKER="$toolchain/armv7a-linux-androideabi${ANDROID_API}-clang"
  export CARGO_TARGET_I686_LINUX_ANDROID_LINKER="$toolchain/i686-linux-android${ANDROID_API}-clang"
  export CARGO_TARGET_X86_64_LINUX_ANDROID_LINKER="$toolchain/x86_64-linux-android${ANDROID_API}-clang"
  export CC_aarch64_linux_android="$toolchain/aarch64-linux-android${ANDROID_API}-clang"
  export CC_armv7_linux_androideabi="$toolchain/armv7a-linux-androideabi${ANDROID_API}-clang"
  export CC_i686_linux_android="$toolchain/i686-linux-android${ANDROID_API}-clang"
  export CC_x86_64_linux_android="$toolchain/x86_64-linux-android${ANDROID_API}-clang"
  export AR_aarch64_linux_android="$toolchain/llvm-ar"
  export AR_armv7_linux_androideabi="$toolchain/llvm-ar"
  export AR_i686_linux_android="$toolchain/llvm-ar"
  export AR_x86_64_linux_android="$toolchain/llvm-ar"

  rustup target add aarch64-linux-android armv7-linux-androideabi i686-linux-android x86_64-linux-android
  build_catcher_ffi aarch64-linux-android
  build_catcher_ffi armv7-linux-androideabi
  build_catcher_ffi i686-linux-android
  build_catcher_ffi x86_64-linux-android

  copy_file "$PACKAGES_DIR/target/aarch64-linux-android/release/libcatcher_ffi.so" "$PACKAGE_DIR/android/src/main/jniLibs/arm64-v8a/libcatcher_ffi.so"
  copy_file "$PACKAGES_DIR/target/armv7-linux-androideabi/release/libcatcher_ffi.so" "$PACKAGE_DIR/android/src/main/jniLibs/armeabi-v7a/libcatcher_ffi.so"
  copy_file "$PACKAGES_DIR/target/i686-linux-android/release/libcatcher_ffi.so" "$PACKAGE_DIR/android/src/main/jniLibs/x86/libcatcher_ffi.so"
  copy_file "$PACKAGES_DIR/target/x86_64-linux-android/release/libcatcher_ffi.so" "$PACKAGE_DIR/android/src/main/jniLibs/x86_64/libcatcher_ffi.so"
}

build_apple() {
  require_command lipo
  require_command xcodebuild
  require_command install_name_tool

  export IPHONEOS_DEPLOYMENT_TARGET
  export MACOSX_DEPLOYMENT_TARGET

  rustup target add aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios
  rustup target add aarch64-apple-darwin x86_64-apple-darwin

  build_catcher_ffi aarch64-apple-ios
  build_catcher_ffi aarch64-apple-ios-sim
  build_catcher_ffi x86_64-apple-ios
  build_catcher_ffi aarch64-apple-darwin
  build_catcher_ffi x86_64-apple-darwin

  local tmp_dir
  tmp_dir="$(mktemp -d)"
  trap 'rm -rf "$tmp_dir"' RETURN

  lipo -create \
    "$PACKAGES_DIR/target/aarch64-apple-ios-sim/release/libcatcher_ffi.dylib" \
    "$PACKAGES_DIR/target/x86_64-apple-ios/release/libcatcher_ffi.dylib" \
    -output "$tmp_dir/libcatcher_ffi_ios_sim.dylib"

  create_framework \
    "$PACKAGES_DIR/target/aarch64-apple-ios/release/libcatcher_ffi.dylib" \
    "$tmp_dir/ios-device/catcher_ffi.framework" \
    "com.eric8810.catcher.ffi.ios"

  create_framework \
    "$tmp_dir/libcatcher_ffi_ios_sim.dylib" \
    "$tmp_dir/ios-simulator/catcher_ffi.framework" \
    "com.eric8810.catcher.ffi.ios-simulator"

  rm -rf "$PACKAGE_DIR/ios/Frameworks/catcher_ffi.xcframework"
  mkdir -p "$PACKAGE_DIR/ios/Frameworks"
  xcodebuild -create-xcframework \
    -framework "$tmp_dir/ios-device/catcher_ffi.framework" \
    -framework "$tmp_dir/ios-simulator/catcher_ffi.framework" \
    -output "$PACKAGE_DIR/ios/Frameworks/catcher_ffi.xcframework"

  lipo -create \
    "$PACKAGES_DIR/target/aarch64-apple-darwin/release/libcatcher_ffi.dylib" \
    "$PACKAGES_DIR/target/x86_64-apple-darwin/release/libcatcher_ffi.dylib" \
    -output "$tmp_dir/libcatcher_ffi_macos.dylib"

  create_framework \
    "$tmp_dir/libcatcher_ffi_macos.dylib" \
    "$tmp_dir/macos/catcher_ffi.framework" \
    "com.eric8810.catcher.ffi.macos"

  rm -rf "$PACKAGE_DIR/macos/Frameworks/catcher_ffi.xcframework"
  mkdir -p "$PACKAGE_DIR/macos/Frameworks"
  xcodebuild -create-xcframework \
    -framework "$tmp_dir/macos/catcher_ffi.framework" \
    -output "$PACKAGE_DIR/macos/Frameworks/catcher_ffi.xcframework"
}

build_desktop() {
  build_catcher_ffi x86_64-unknown-linux-gnu
  copy_file "$PACKAGES_DIR/target/x86_64-unknown-linux-gnu/release/libcatcher_ffi.so" "$PACKAGE_DIR/linux/lib/x64/libcatcher_ffi.so"

  if rustup target list --installed | grep -q '^aarch64-unknown-linux-gnu$'; then
    build_catcher_ffi aarch64-unknown-linux-gnu
    copy_file "$PACKAGES_DIR/target/aarch64-unknown-linux-gnu/release/libcatcher_ffi.so" "$PACKAGE_DIR/linux/lib/arm64/libcatcher_ffi.so"
  else
    echo "Skipping Linux arm64; install target and linker before running this script if needed."
  fi

  if rustup target list --installed | grep -q '^x86_64-pc-windows-msvc$'; then
    build_catcher_ffi x86_64-pc-windows-msvc
    copy_file "$PACKAGES_DIR/target/x86_64-pc-windows-msvc/release/catcher_ffi.dll" "$PACKAGE_DIR/windows/lib/x64/catcher_ffi.dll"
  else
    echo "Skipping Windows x64; install the MSVC target on a Windows host if needed."
  fi
}

case "${1:-all}" in
  android) build_android ;;
  apple) build_apple ;;
  desktop) build_desktop ;;
  all)
    build_android
    build_apple
    build_desktop
    ;;
  *)
    echo "Usage: $0 [android|apple|desktop|all]" >&2
    exit 1
    ;;
esac

echo "Native bundle files:"
for dir in \
  "$PACKAGE_DIR/android/src/main/jniLibs" \
  "$PACKAGE_DIR/ios/Frameworks" \
  "$PACKAGE_DIR/macos/Frameworks" \
  "$PACKAGE_DIR/linux/lib" \
  "$PACKAGE_DIR/windows/lib"; do
  if [ -d "$dir" ]; then
    find "$dir" -type f
  fi
done | sort
