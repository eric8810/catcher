/// [inline] Copy of catcher-codec source, using catcher_core for error types
use catcher_core::CatcherError;
use serde::{de::DeserializeOwned, Serialize};

/// 将任意可序列化值编码为 MsgPack 字节
pub fn pack<T: Serialize>(value: &T) -> Result<Vec<u8>, CatcherError> {
    rmp_serde::to_vec(value).map_err(|e| CatcherError::EncodeError(e.to_string()))
}

/// 从 MsgPack 字节解码为指定类型
pub fn unpack<T: DeserializeOwned>(data: &[u8]) -> Result<T, CatcherError> {
    rmp_serde::from_slice(data).map_err(|e| CatcherError::DecodeError(e.to_string()))
}

/// 解码为通用 serde_json::Value（兼容 JSON fallback）
pub fn unpack_value(data: &[u8]) -> Result<serde_json::Value, CatcherError> {
    let val =
        rmpv::decode::read_value(&mut &data[..]).map_err(|e| CatcherError::DecodeError(e.to_string()))?;
    Ok(rmpv_to_json(val))
}

/// rmpv::Value → serde_json::Value 递归转换
fn rmpv_to_json(val: rmpv::Value) -> serde_json::Value {
    use rmpv::Value;
    match val {
        Value::Nil => serde_json::Value::Null,
        Value::Boolean(b) => serde_json::Value::Bool(b),
        Value::Integer(i) => {
            if let Some(v) = i.as_i64() {
                serde_json::Value::Number(v.into())
            } else if let Some(v) = i.as_u64() {
                serde_json::Value::Number(v.into())
            } else {
                serde_json::Value::Null
            }
        }
        Value::F32(f) => serde_json::json!(f),
        Value::F64(f) => serde_json::json!(f),
        Value::String(s) => {
            if let Some(utf8) = s.into_str() {
                serde_json::Value::String(utf8)
            } else {
                serde_json::Value::String(String::new())
            }
        }
        Value::Binary(b) => serde_json::Value::Array(
            b.into_iter()
                .map(|v| serde_json::Value::Number(v.into()))
                .collect(),
        ),
        Value::Array(arr) => serde_json::Value::Array(arr.into_iter().map(rmpv_to_json).collect()),
        Value::Map(map) => {
            let obj = map
                .into_iter()
                .map(|(k, v)| {
                    let key = match k {
                        Value::String(s) => s.into_str().unwrap_or_default(),
                        other => format!("{other:?}"),
                    };
                    (key, rmpv_to_json(v))
                })
                .collect();
            serde_json::Value::Object(obj)
        }
        Value::Ext(_, data) => serde_json::Value::Array(
            data.into_iter()
                .map(|v| serde_json::Value::Number(v.into()))
                .collect(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    struct TestPayload {
        event: String,
        id: String,
        from: String,
        to: String,
        text: String,
        ts: u64,
    }

    #[test]
    fn pack_unpack_roundtrip_string() {
        let original = "hello world".to_string();
        let packed = pack(&original).unwrap();
        let unpacked: String = unpack(&packed).unwrap();
        assert_eq!(original, unpacked);
    }

    #[test]
    fn pack_unpack_roundtrip_number() {
        let original = 42u32;
        let packed = pack(&original).unwrap();
        let unpacked: u32 = unpack(&packed).unwrap();
        assert_eq!(original, unpacked);
    }

    #[test]
    fn pack_unpack_roundtrip_struct() {
        let payload = TestPayload {
            event: "message".into(),
            id: "msg_001".into(),
            from: "user_a".into(),
            to: "channel_1".into(),
            text: "Hello ! ".repeat(10),
            ts: 1700000000,
        };
        let packed = pack(&payload).unwrap();
        let unpacked: TestPayload = unpack(&packed).unwrap();
        assert_eq!(payload, unpacked);
    }

    #[test]
    fn pack_unpack_roundtrip_vec() {
        let original = vec![1i32, 2, 3, 4, 5];
        let packed = pack(&original).unwrap();
        let unpacked: Vec<i32> = unpack(&packed).unwrap();
        assert_eq!(original, unpacked);
    }

    #[test]
    fn pack_unpack_roundtrip_nested() {
        #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
        struct Inner {
            x: i32,
            y: String,
        }
        #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
        struct Outer {
            name: String,
            items: Vec<Inner>,
        }
        let original = Outer {
            name: "test".into(),
            items: vec![
                Inner { x: 1, y: "a".into() },
                Inner { x: 2, y: "b".into() },
            ],
        };
        let packed = pack(&original).unwrap();
        let unpacked: Outer = unpack(&packed).unwrap();
        assert_eq!(original, unpacked);
    }

    #[test]
    fn unpack_invalid_data_fails() {
        let result = unpack::<String>(&[0xff, 0xff, 0xff]);
        assert!(result.is_err());
        match result {
            Err(CatcherError::DecodeError(_)) => {}
            _ => panic!("expected DecodeError"),
        }
    }

    #[test]
    fn unpack_value_produces_array_for_struct() {
        let payload = TestPayload {
            event: "msg".into(),
            id: "x".into(),
            from: "a".into(),
            to: "b".into(),
            text: "hi".into(),
            ts: 1,
        };
        let packed = pack(&payload).unwrap();
        let value = unpack_value(&packed).unwrap();
        match &value {
            serde_json::Value::Array(arr) => {
                assert_eq!(arr.len(), 6);
                assert_eq!(arr[0], serde_json::Value::String("msg".into()));
                assert_eq!(arr[5], serde_json::json!(1));
            }
            _ => panic!("expected Array, got {value:?}"),
        }
    }

    #[test]
    fn pack_compare_size_vs_json() {
        let payload = TestPayload {
            event: "message".into(),
            id: "msg_001".into(),
            from: "user_001".into(),
            to: "channel_general".into(),
            text: "Hello ".repeat(30),
            ts: 1700000000,
        };
        let packed = pack(&payload).unwrap();
        let json = serde_json::to_vec(&payload).unwrap();
        assert!(
            packed.len() <= json.len(),
            "msgpack {} bytes vs json {} bytes",
            packed.len(),
            json.len()
        );
    }

    #[test]
    fn unpack_value_empty_slice_fails() {
        let result = unpack_value(&[]);
        assert!(result.is_err());
    }
}
