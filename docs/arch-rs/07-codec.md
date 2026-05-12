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
