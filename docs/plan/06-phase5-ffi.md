# 06 — Phase 5: FFI Bindings

> 对应 arch 文档：`09-ffi.md`
> 工期：9 人天
> 目标：C ABI 导出 + napi-rs (Node.js) 绑定 + flutter_rust_bridge (Dart) 绑定 + TS e2e 验证

---

## 1. 模块概览

```
src/ffi/
├── mod.rs              # re-export
├── types_ffi.rs        # FfiResult, FfiString, FfiBytes, EventCallback
├── http_ffi.rs         # HTTP C ABI: create / get / post / destroy
├── ws_ffi.rs           # WebSocket C ABI: create / send / close / destroy
├── codec_ffi.rs        # Codec C ABI: pack / unpack
└── quality_ffi.rs      # Network Quality C ABI: evaluate_quality

catcher-rs-napi/         # npm 包 (napi-rs binding)
├── package.json
├── Cargo.toml
├── build.rs
├── src/
│   ├── lib.rs
│   ├── http.rs
│   ├── ws.rs
│   └── codec.rs
└── index.js

catcher_core/             # pub.dev 包 (flutter_rust_bridge)
├── pubspec.yaml
├── rust/
│   ├── Cargo.toml
│   └── src/
│       ├── api/
│       └── frb_generated.rs
└── lib/
    └── catcher_core.dart
```

---

## 2. C ABI 层实现

### Step 5.1 — `src/ffi/types_ffi.rs`

**参考**：`arch-rs/09-ffi.md`

```rust
use std::ffi::{c_char, c_void};
use std::fmt;

#[repr(C)]
pub struct FfiResult {
    pub error_code: i32,          // 0 = 成功, 非 0 = 错误
    pub error_message: *mut c_char,
    pub data: *mut c_void,
    pub data_len: usize,
}

#[repr(C)]
pub struct FfiString {
    pub data: *const c_char,
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

// FFI 安全的错误结果
impl FfiResult {
    pub fn ok(data: *mut c_void, len: usize) -> Self {
        Self { error_code: 0, error_message: std::ptr::null_mut(), data, data_len: len }
    }

    pub fn error(code: i32, msg: &str) -> Self {
        let c_msg = std::ffi::CString::new(msg).unwrap();
        Self {
            error_code: code,
            error_message: c_msg.into_raw(),
            data: std::ptr::null_mut(),
            data_len: 0,
        }
    }
}

impl Drop for FfiResult {
    fn drop(&mut self) {
        if !self.error_message.is_null() {
            unsafe { let _ = std::ffi::CString::from_raw(self.error_message); }
        }
    }
}
```

### Step 5.2 — `src/ffi/http_ffi.rs`

```rust
use crate::ffi::types_ffi::*;
use crate::transport::http_client::HttpTransport;
use crate::types::http::HttpClientConfig;
use std::ffi::CStr;
use std::sync::Mutex;
use std::collections::HashMap;

static HANDLES: Mutex<HashMap<usize, HttpTransport>> = Mutex::new(HashMap::new());
static NEXT_ID: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(1);

#[no_mangle]
pub extern "C" fn catcher_http_client_create(
    config_json: *const c_char,
) -> *mut c_void {
    let json = unsafe {
        if config_json.is_null() { return std::ptr::null_mut(); }
        CStr::from_ptr(config_json)
    };

    let config: HttpClientConfig = match serde_json::from_str(json.to_str().unwrap_or("")) {
        Ok(c) => c,
        Err(_) => return std::ptr::null_mut(),
    };

    let transport = match HttpTransport::new(config) {
        Ok(t) => t,
        Err(_) => return std::ptr::null_mut(),
    };

    let id = NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    HANDLES.lock().unwrap().insert(id, transport);

    Box::into_raw(Box::new(id)) as *mut c_void
}

#[no_mangle]
pub extern "C" fn catcher_http_client_destroy(handle: *mut c_void) {
    if handle.is_null() { return; }
    let id = unsafe { *(handle as *const usize) };
    HANDLES.lock().unwrap().remove(&id);
    unsafe { drop(Box::from_raw(handle as *mut usize)); }
}
```

**get / post / put / delete** 实现类似：从 HANDLES 取出对应 transport，执行业务逻辑，结果通过 `EventCallback` 回调或返回值。

### Step 5.3 — `src/ffi/ws_ffi.rs`

```rust
#[no_mangle]
pub extern "C" fn catcher_ws_create(
    config_json: *const c_char,
    event_callback: EventCallback,
    user_data: *mut c_void,
) -> *mut c_void { /* todo */ }

#[no_mangle]
pub extern "C" fn catcher_ws_send_text(
    handle: *mut c_void,
    message: FfiString,
) -> FfiResult { /* todo */ }

#[no_mangle]
pub extern "C" fn catcher_ws_send_binary(
    handle: *mut c_void,
    data: *const u8,
    len: usize,
) -> FfiResult { /* todo */ }

#[no_mangle]
pub extern "C" fn catcher_ws_close(
    handle: *mut c_void,
    code: u16,
    reason: FfiString,
) { /* todo */ }

#[no_mangle]
pub extern "C" fn catcher_ws_destroy(handle: *mut c_void) { /* todo */ }
```

### Step 5.4 — `src/ffi/codec_ffi.rs`

```rust
use crate::ffi::types_ffi::FfiResult;
use crate::codec::msgpack;

#[no_mangle]
pub extern "C" fn catcher_pack(
    json_input: *const c_char,
) -> FfiResult {
    let json = unsafe {
        if json_input.is_null() {
            return FfiResult::error(1, "null input");
        }
        std::ffi::CStr::from_ptr(json_input)
    };

    let value: serde_json::Value = match serde_json::from_str(
        json.to_str().unwrap_or("")
    ) {
        Ok(v) => v,
        Err(e) => return FfiResult::error(2, &e.to_string()),
    };

    match msgpack::pack(&value) {
        Ok(bytes) => {
            let len = bytes.len();
            let ptr = bytes.as_ptr();
            std::mem::forget(bytes); // 调用方负责释放
            FfiResult::ok(ptr as *mut std::ffi::c_void, len)
        }
        Err(e) => FfiResult::error(3, &e.to_string()),
    }
}

#[no_mangle]
pub extern "C" fn catcher_unpack(
    data: *const u8,
    len: usize,
) -> FfiResult {
    if data.is_null() {
        return FfiResult::error(1, "null data");
    }
    let slice = unsafe { std::slice::from_raw_parts(data, len) };
    match msgpack::unpack_value(slice) {
        Ok(value) => {
            let json = value.to_string();
            let c_str = std::ffi::CString::new(json).unwrap();
            let ptr = c_str.into_raw();
            FfiResult::ok(ptr as *mut std::ffi::c_void, 0)
        }
        Err(e) => FfiResult::error(3, &e.to_string()),
    }
}
```

### Step 5.5 — `src/ffi/quality_ffi.rs`

```rust
#[no_mangle]
pub extern "C" fn catcher_evaluate_quality(
    host: FfiString,
    callback: EventCallback,
    user_data: *mut c_void,
) {
    let host_str = unsafe {
        std::str::from_utf8(
            std::slice::from_raw_parts(host.data as *const u8, host.len)
        ).unwrap_or("https://www.example.com")
    };

    tokio::spawn(async move {
        let mut evaluator = crate::observability::network_quality::NetworkQualityEvaluator::new(20);
        let result = evaluator.evaluate();
        let json = serde_json::to_string(&result).unwrap();
        let c_event = std::ffi::CString::new("quality_result").unwrap();
        callback(
            c_event.as_ptr(),
            json.as_ptr(),
            json.len(),
            user_data,
        );
    });
}
```

---

## 3. napi-rs 绑定层（Node.js）

### Step 5.6 — `catcher-rs-napi/` npm 包

**目录结构**：

```
catcher-rs-napi/
├── package.json
├── Cargo.toml
├── build.rs
├── npm/                    # 平台预编译 .node 文件
│   ├── darwin-arm64/
│   ├── darwin-x64/
│   ├── linux-x64/
│   └── win32-x64/
├── src/
│   ├── lib.rs
│   ├── http.rs
│   ├── ws.rs
│   └── codec.rs
├── index.js                # JS 入口
└── index.d.ts              # TypeScript 类型声明
```

**`Cargo.toml`**：
```toml
[package]
name = "catcher-rs-napi"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib"]

[dependencies]
catcher-rs = { path = "../catcher-rs", features = ["napi"] }
napi = { version = "2", default-features = false, features = ["napi4", "tokio_rt", "serde-json"] }
napi-derive = "2"
serde_json = "1"
tokio = { version = "1", features = ["rt-multi-thread"] }

[build-dependencies]
napi-build = "2"
```

**`src/http.rs`**：
```rust
use napi::*;
use napi_derive::napi;
use catcher_rs::{HttpTransport, HttpClientConfig, HttpRequest, HttpMethod};
use std::sync::Arc;

#[napi(object)]
pub struct JsHttpResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Buffer,
    pub elapsed_ms: u32,
}

#[napi]
pub struct JsHttpClient {
    inner: Arc<HttpTransport>,
}

#[napi]
impl JsHttpClient {
    #[napi(constructor)]
    pub fn new(config_json: String) -> napi::Result<Self> {
        let config: HttpClientConfig = serde_json::from_str(&config_json)
            .map_err(|e| napi::Error::from_reason(e.to_string()))?;
        let transport = HttpTransport::new(config)
            .map_err(|e| napi::Error::from_reason(e.to_string()))?;
        Ok(Self { inner: Arc::new(transport) })
    }

    #[napi]
    pub async fn get(&self, url: String) -> napi::Result<JsHttpResponse> {
        let request = HttpRequest {
            method: HttpMethod::GET,
            url,
            headers: std::collections::HashMap::new(),
            body: None,
            content_type: None,
            timeout_ms: None,
        };
        let resp = self.inner.execute(request).await
            .map_err(|e| napi::Error::from_reason(e.to_string()))?;
        Ok(JsHttpResponse {
            status: resp.status,
            headers: resp.headers.into_iter().collect(),
            body: Buffer::from(resp.body),
            elapsed_ms: resp.elapsed_ms as u32,
        })
    }

    #[napi]
    pub async fn post(
        &self, url: String, body: Buffer, content_type: Option<String>,
    ) -> napi::Result<JsHttpResponse> {
        let request = HttpRequest {
            method: HttpMethod::POST,
            url,
            headers: std::collections::HashMap::new(),
            body: Some(body.into()),
            content_type: content_type.or_else(|| Some("application/octet-stream".into())),
            timeout_ms: None,
        };
        let resp = self.inner.execute(request).await
            .map_err(|e| napi::Error::from_reason(e.to_string()))?;
        Ok(JsHttpResponse {
            status: resp.status,
            headers: resp.headers.into_iter().collect(),
            body: Buffer::from(resp.body),
            elapsed_ms: resp.elapsed_ms as u32,
        })
    }
}
```

**`src/codec.rs`**：
```rust
use napi::*;
use napi_derive::napi;
use catcher_rs::codec::msgpack;

#[napi]
pub fn pack(obj: napi::JsUnknown, env: Env) -> napi::Result<Buffer> {
    let json_value: serde_json::Value = env.from_js_value(obj)?;
    let bytes = msgpack::pack(&json_value)
        .map_err(|e| napi::Error::from_reason(e.to_string()))?;
    Ok(Buffer::from(bytes))
}

#[napi]
pub fn unpack(buffer: Buffer) -> napi::Result<napi::JsUnknown> {
    let value = msgpack::unpack_value(&buffer)
        .map_err(|e| napi::Error::from_reason(e.to_string()))?;
    // 返回 serde_json::Value 给 JS
    Ok(napi::JsUnknown::default()) // 需要实际序列化
}
```

**`index.js`**：
```js
// 按平台加载预编译 .node addon
const os = require('os')
const arch = os.arch()
const platform = os.platform()
const addon = require(`./npm/${platform}-${arch}/catcher-rs-napi.node`)

module.exports = {
  HttpClient: addon.JsHttpClient,
  WsClient: addon.JsWsClient,
  pack: addon.pack,
  unpack: addon.unpack,
}
```

---

## 4. flutter_rust_bridge 绑定层

### Step 5.7 — `catcher_core/` pub.dev 包

使用 DTO (Data Transfer Object) 模式，字段平铺（flutter_rust_bridge 不支持 serde flatten）：

```rust
// catcher_core/rust/src/api/http.rs
pub struct HttpClientConfigDto {
    pub base_url: String,
    pub connect_timeout_ms: u64,
    pub response_timeout_ms: u64,
    pub keep_alive: bool,
    pub retry_max_attempts: u32,
    pub retry_min_backoff_ms: u64,
    pub retry_max_backoff_ms: u64,
    pub cb_failure_threshold: u32,
    pub cb_reset_timeout_ms: u64,
    pub max_concurrency: u32,
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

pub fn create_http_client(config: HttpClientConfigDto) -> Result<HttpClientHandle, String> { todo!() }
pub async fn http_get(handle: HttpClientHandle, url: String) -> Result<HttpResponseDto, String> { todo!() }
pub async fn http_post(handle: HttpClientHandle, url: String, body: Vec<u8>) -> Result<HttpResponseDto, String> { todo!() }

pub fn create_ws_client(
    config_json: String,
    event_sink: StreamSink<WsEventDto>,
) -> Result<WsClientHandle, String> { todo!() }
```

---

## 5. TS e2e 测试验证

Phase 5 完成后，在 TS 侧创建对比测试 adapter：

```typescript
// test/e2e/adapters/rust-adapter.ts
import { HttpClient, WsClient, pack } from 'catcher-rs'

export function createRustHttpClient(baseUrl: string) {
  return new HttpClient(JSON.stringify({
    baseUrl,
    keepAlive: true,
    dnsCacheTtlSecs: 300,
    connectTimeoutMs: 5000,
    responseTimeoutMs: 10000,
    retry: {
      maxAttempts: 3,
      backoff: 'exponential',
      minBackoffMs: 100,
      maxBackoffMs: 5000,
      jitter: true,
    },
    circuitBreaker: {
      failureThreshold: 5,
      successThreshold: 2,
      resetTimeoutMs: 30000,
      halfOpenMaxRequests: 5,
    },
  }))
}
```

**复用现有 S1-S8 场景**：替换 `catcherFn` 的实现为 Rust adapter，其余（vanilla、proxy、server、harness）保持不变。详细方案见 `07-test-reuse.md`。

---

## 6. Phase 5 完成标准

- [ ] C ABI 全部 6 个函数通过编译
- [ ] napi-rs 绑定 `JsHttpClient.get/post` + `pack/unpack` 可被 Node.js 调用
- [ ] `catcher-rs-napi` npm 包 `npm install` 可加载
- [ ] flutter_rust_bridge codegen 生成 Dart 绑定
- [ ] S1 (冷启动) 场景：vanilla vs Rust 成功率和延迟对比
- [ ] S5 (msgpack 压缩) 场景：pack/unpack 字节数对比
- [ ] S6 (WS 高频消息) 场景：perMessageDeflate 带宽对比
- [ ] `cargo clippy -- -D warnings` 零警告
