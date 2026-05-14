# catcher-ffi

[![crates.io](https://img.shields.io/crates/v/catcher-ffi.svg)](https://crates.io/crates/catcher-ffi)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

Unified C ABI library for the [catcher](https://github.com/eric8810/catcher) toolkit — links `catcher-http`, `catcher-ws`, and `catcher-core` into a single shared library (`libcatcher_ffi.so` / `catcher_ffi.dylib` / `catcher_ffi.dll`).

All `#[no_mangle] pub extern "C"` functions from the dependency FFI modules are automatically exported. This crate is the bridge used by **dart:ffi** (Flutter) bindings.

## Exported Symbols

### HTTP Client
| Symbol | Description |
|--------|-------------|
| `catcher_http_client_create` | Create an HTTP client from JSON config |
| `catcher_http_client_destroy` | Free the HTTP client |
| `catcher_http_execute` | Execute an HTTP request (async, callback-based) |

### WebSocket Client
| Symbol | Description |
|--------|-------------|
| `catcher_ws_create` | Create a WebSocket client from JSON config |
| `catcher_ws_send_text` | Send a text message |
| `catcher_ws_send_binary` | Send a binary message |
| `catcher_ws_close` | Close the connection |
| `catcher_ws_destroy` | Free the WebSocket client |

### Codec
| Symbol | Description |
|--------|-------------|
| `catcher_pack` | Pack JSON → msgpack binary |
| `catcher_unpack` | Unpack msgpack binary → JSON string |
| `catcher_free_data` | Free memory allocated by pack/unpack |

### Memory
| Symbol | Description |
|--------|-------------|
| `catcher_free_result` | Free an `FfiResult` struct |
| `catcher_free_event_data` | Free callback event data |

## Build

```bash
cargo build --release
# Output: target/release/libcatcher_ffi.so (Linux)
#         target/release/catcher_ffi.dylib (macOS)
#         target/release/catcher_ffi.dll (Windows)
```

## Usage from C

```c
#include <stdint.h>

typedef struct { void* data; uintptr_t data_len; uint32_t error_code; const char* error_message; } FfiResult;

extern FfiResult catcher_pack(const char* json_input);
extern void catcher_free_data(void* data, uintptr_t len);
```

## Usage from Dart (Flutter)

This library is loaded by the `catcher_core` Flutter package via `dart:ffi`. See [`catcher_core`](https://pub.dev/packages/catcher_core) for the Dart API.

## License

MIT
