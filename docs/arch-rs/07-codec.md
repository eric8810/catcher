# 07 — 编解码层

> 对应源文件：`src/codec/msgpack.rs`

---

```rust
use serde::{Serialize, de::DeserializeOwned};
use crate::error::CatcherError;

/// 编码为 msgpack 字节
pub fn pack<T: Serialize>(value: &T) -> Result<Vec<u8>, CatcherError> {
    rmp_serde::to_vec(value).map_err(|e| CatcherError::EncodeError(format!("{e}")))
}

/// 从 msgpack 字节解码
pub fn unpack<T: DeserializeOwned>(data: &[u8]) -> Result<T, CatcherError> {
    rmp_serde::from_slice(data).map_err(|e| CatcherError::DecodeError(format!("{e}")))
}

/// 解码为通用 serde_json::Value（兼容 JSON fallback）
pub fn unpack_value(data: &[u8]) -> Result<serde_json::Value, CatcherError> {
    let val: rmpv::Value = rmpv::decode::read_value(&mut &data[..])
        .map_err(|e| CatcherError::DecodeError(format!("{e}")))?;
    Ok(rmpv_to_json(val))
}

/// rmpv::Value → serde_json::Value 递归转换
fn rmpv_to_json(val: rmpv::Value) -> serde_json::Value {
    use rmpv::Value;
    match val {
        Value::Nil        => serde_json::Value::Null,
        Value::Boolean(b) => serde_json::Value::Bool(b),
        Value::Integer(i) => serde_json::Value::Number(i.into()),
        Value::F32(f)     => serde_json::json!(f),
        Value::F64(f)     => serde_json::json!(f),
        Value::String(s)  => {
            serde_json::Value::String(s.into_str().unwrap_or_default())
        }
        Value::Binary(b)  => serde_json::Value::Array(
            b.into_iter().map(|v| serde_json::Value::Number(v.into())).collect()
        ),
        Value::Array(arr) => serde_json::Value::Array(
            arr.into_iter().map(rmpv_to_json).collect()
        ),
        Value::Map(map)   => serde_json::Value::Object(
            map.into_iter().map(|(k, v)| {
                let key = match k {
                    Value::String(s) => s.into_str().unwrap_or_default(),
                    other => format!("{other:?}"),
                };
                (key, rmpv_to_json(v))
            }).collect()
        ),
        Value::Ext(_, data) => serde_json::Value::Array(
            data.into_iter().map(|v| serde_json::Value::Number(v.into())).collect()
        ),
    }
}
```

---

## Transport 层内置 Msgpack

> v0.3.8+ 新增

除了独立的 `pack` / `unpack` 函数外，catcher-http 和 catcher-ws 现在支持在 transport 层自动编解码 msgpack。

### 配置

```rust
pub struct HttpClientConfig {
    // ...
    pub msgpack: bool,  // default: false
}

pub struct WsClientConfig {
    // ...
    pub msgpack: bool,  // default: false
}
```

### HTTP 数据流（`msgpack: true`）

```
发送: JSON bytes → serde_json::from_slice → rmp_serde::to_vec → wire (Content-Type: application/msgpack)
接收: wire → rmp_serde::from_read (with cursor validation) → serde_json::to_vec → JSON bytes
```

响应解码只在 `Content-Type` 包含 `msgpack` 且 body 非空时触发。使用 `Cursor` 验证整个 body 被完整消费。

### WS 数据流（`msgpack: true`）

```
发送: JSON text → serde_json::from_str → rmp_serde::to_vec → Binary frame
接收: Binary frame → rmp_serde::from_slice → serde_json::to_string → Text event (is_binary: false)
```
