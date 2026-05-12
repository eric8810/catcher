# 09 — FFI 接口契约

> 对应源文件：`src/ffi/`，以及 napi-rs / flutter_rust_bridge 绑定包

---

## FFI 设计原则

| 原则 | 说明 |
|------|------|
| **C ABI 唯一事实来源** | 所有导出用 `extern "C"` + `#[repr(C)]` |
| **字符串** | C 字符串指针 + 长度，避免 `CString` 跨边界 |
| **二进制** | `*const u8` + `len`，零拷贝 `Buffer` / `Uint8List` |
| **异步回调** | 函数指针 + `user_data` 观察者模式 |
| **错误** | 统一 `FfiResult` 结构体 |

---

## FFI 基础类型 (`src/ffi/types_ffi.rs`)

```rust
use std::ffi::{c_char, c_void};

/// FFI 安全的结果类型
#[repr(C)]
pub struct FfiResult {
    pub error_code: i32,         // 0 = 成功
    pub error_message: *mut c_char,
    pub data: *mut c_void,
    pub data_len: usize,
}

#[repr(C)]
pub struct FfiString {
    pub data: *mut c_char,
    pub len: usize,
}

#[repr(C)]
pub struct FfiBytes {
    pub data: *const u8,
    pub len: usize,
    pub free_fn: Option<extern "C" fn(*mut c_void)>,
    pub free_ctx: *mut c_void,
}

pub type EventCallback = extern "C" fn(
    event_type: *const c_char,
    event_data: *const u8,
    event_data_len: usize,
    user_data: *mut c_void,
);
```

---

## HTTP C ABI (`src/ffi/http_ffi.rs`)

```rust
#[no_mangle]
pub extern "C" fn catcher_http_client_create(
    config_json: *const c_char,
) -> *mut c_void { todo!() }

#[no_mangle]
pub extern "C" fn catcher_http_get(
    handle: *mut c_void,
    url: FfiString,
    callback: EventCallback,
    user_data: *mut c_void,
) { todo!() }

#[no_mangle]
pub extern "C" fn catcher_http_post(
    handle: *mut c_void,
    url: FfiString,
    body: FfiBytes,
    content_type: FfiString,
    callback: EventCallback,
    user_data: *mut c_void,
) { todo!() }

#[no_mangle]
pub extern "C" fn catcher_http_client_destroy(handle: *mut c_void) { todo!() }
```

---

## WebSocket C ABI (`src/ffi/ws_ffi.rs`)

```rust
#[no_mangle]
pub extern "C" fn catcher_ws_create(
    config_json: *const c_char,
    event_callback: EventCallback,
    user_data: *mut c_void,
) -> *mut c_void { todo!() }

#[no_mangle]
pub extern "C" fn catcher_ws_send_text(
    handle: *mut c_void,
    message: FfiString,
) -> FfiResult { todo!() }

#[no_mangle]
pub extern "C" fn catcher_ws_send_binary(
    handle: *mut c_void,
    data: *const u8,
    len: usize,
) -> FfiResult { todo!() }

#[no_mangle]
pub extern "C" fn catcher_ws_close(
    handle: *mut c_void,
    code: u16,
    reason: FfiString,
) { todo!() }

#[no_mangle]
pub extern "C" fn catcher_ws_destroy(handle: *mut c_void) { todo!() }
```

---

## Codec C ABI (`src/ffi/codec_ffi.rs`)

```rust
#[no_mangle]
pub extern "C" fn catcher_pack(
    json_input: *const c_char,
) -> FfiResult { todo!() }

#[no_mangle]
pub extern "C" fn catcher_unpack(
    data: *const u8, len: usize,
) -> FfiResult { todo!() }
```

---

## Network Quality C ABI (`src/ffi/quality_ffi.rs`)

```rust
#[no_mangle]
pub extern "C" fn catcher_evaluate_quality(
    host: FfiString,
    callback: EventCallback,
    user_data: *mut c_void,
) { todo!() }
```

---

## napi-rs 绑定层（Node.js）

```
catcher-rs-napi/          # npm 包
├── package.json
├── Cargo.toml              # [lib] crate-type = ["cdylib"]
├── build.rs
├── src/
│   ├── lib.rs
│   ├── http.rs
│   ├── ws.rs
│   └── codec.rs
└── index.js
```

```rust
use napi::*;
use napi_derive::napi;

#[napi(object)]
pub struct JsHttpResponse {
    pub status: u16,
    pub headers: HashMap<String, String>,
    pub body: Buffer,
    pub elapsed_ms: u32,
}

#[napi]
pub struct JsHttpClient { inner: Arc<catcher_core::HttpClient> }

#[napi]
impl JsHttpClient {
    #[napi(constructor)]
    pub fn new(config: String) -> napi::Result<Self> { todo!() }

    #[napi]
    pub async fn get(&self, url: String) -> napi::Result<JsHttpResponse> { todo!() }

    #[napi]
    pub async fn post(
        &self, url: String, body: Buffer, content_type: Option<String>,
    ) -> napi::Result<JsHttpResponse> { todo!() }
}

#[napi] pub fn pack(obj: napi::JsUnknown, env: Env) -> napi::Result<Buffer> { todo!() }
#[napi] pub fn unpack(buffer: Buffer) -> napi::Result<napi::JsUnknown> { todo!() }
```

---

## flutter_rust_bridge 绑定层（Dart）

```
catcher_core/               # pub.dev 包
├── pubspec.yaml
├── rust/
│   ├── Cargo.toml
│   └── src/
│       ├── api/
│       └── frb_generated.rs
└── lib/
    └── catcher_core.dart
```

```rust
// 函数签名被 flutter_rust_bridge codegen 解析，自动生成 Dart

pub struct HttpClientConfigDto {
    pub base_url: String,
    pub connect_timeout_ms: u64,
    pub retry_max_attempts: u32,
    // 所有字段平铺（flutter_rust_bridge 不支持 serde flatten）
}

pub struct HttpResponseDto {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    pub elapsed_ms: u64,
}

pub struct WsEventDto {
    pub event_type: String,
    pub data: Vec<u8>,
    pub metadata: Option<String>,
}

pub fn create_http_client(config: HttpClientConfigDto) -> HttpClientHandle { todo!() }
pub async fn http_get(handle: HttpClientHandle, url: String) -> Result<HttpResponseDto, String> { todo!() }

pub fn create_ws_client(config_json: String, event_sink: StreamSink<WsEventDto>) -> WsClientHandle { todo!() }

pub fn pack_msgpack(value_json: String) -> Vec<u8> { todo!() }
pub fn unpack_msgpack(data: Vec<u8>) -> String { todo!() }
```
