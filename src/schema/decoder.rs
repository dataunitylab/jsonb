use crate::schema::{InstanceType, Schema, SingleOrVec};
use crate::{Number, Value};
use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};
use std::io::Cursor;

const TAG_NULL: u8 = 0x00;
const TAG_BOOL_FALSE: u8 = 0x01;
const TAG_BOOL_TRUE: u8 = 0x02;
const TAG_NUMBER: u8 = 0x03;
const TAG_STRING: u8 = 0x04;
const TAG_ARRAY: u8 = 0x05;
const TAG_OBJECT: u8 = 0x06;

pub fn decode(buf: &[u8], schema: &Schema) -> Value<'static> {
    let mut cursor = Cursor::new(buf);
    decode_value(&mut cursor, Some(schema))
}

fn decode_value(cursor: &mut Cursor<&[u8]>, schema: Option<&Schema>) -> Value<'static> {
    if let Some(schema) = schema {
        if let Some(c) = &schema.const_value {
            return serde_to_jsonb_value(c);
        }
        if let Some(enums) = &schema.enum_values {
            let idx = read_uvarint(cursor) as usize;
            if idx < enums.len() {
                return serde_to_jsonb_value(&enums[idx]);
            }
            // Fallback or error? Assuming valid encoding.
            return Value::Null;
        }

        match &schema.instance_type {
            Some(SingleOrVec::Single(instance_type)) => {
                return decode_typed_value(cursor, instance_type, schema);
            }
            Some(SingleOrVec::Vec(types)) => {
                // Check if it's a number tag and we have delta encoding
                let pos = cursor.position();
                let tag = read_byte(cursor);
                if tag == TAG_NUMBER {
                    if types.contains(&InstanceType::Integer)
                        || types.contains(&InstanceType::Number)
                    {
                        if let Some(min) = schema.minimum {
                            // Delta encoding
                            let delta = read_uvarint128(cursor);
                            let val = min + delta as i128;
                            if let Ok(v) = i64::try_from(val) {
                                return Value::Number(Number::Int64(v));
                            }
                            return Value::Number(Number::Decimal128(crate::Decimal128 {
                                scale: 0,
                                value: val,
                            }));
                        }
                    }
                }
                // Reset cursor
                cursor.set_position(pos);
            }
            None => {}
        }
    }
    decode_untyped_value(cursor)
}

fn serde_to_jsonb_value(v: &serde_json::Value) -> Value<'static> {
    match v {
        serde_json::Value::Null => Value::Null,
        serde_json::Value::Bool(b) => Value::Bool(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::Number(Number::Int64(i))
            } else if let Some(u) = n.as_u64() {
                Value::Number(Number::UInt64(u))
            } else if let Some(f) = n.as_f64() {
                Value::Number(Number::Float64(f))
            } else {
                // Arbitrary precision fallback via string?
                // jsonb::Number::decode handles it?
                // For now, float fallback.
                Value::Number(Number::Float64(n.as_f64().unwrap_or(0.0)))
            }
        }
        serde_json::Value::String(s) => Value::String(Cow::Owned(s.clone())),
        serde_json::Value::Array(arr) => {
            let mut res = Vec::with_capacity(arr.len());
            for item in arr {
                res.push(serde_to_jsonb_value(item));
            }
            Value::Array(res)
        }
        serde_json::Value::Object(obj) => {
            let mut res = BTreeMap::new();
            for (k, val) in obj {
                res.insert(k.clone(), serde_to_jsonb_value(val));
            }
            Value::Object(res)
        }
    }
}

fn decode_typed_value(
    cursor: &mut Cursor<&[u8]>,
    instance_type: &InstanceType,
    schema: &Schema,
) -> Value<'static> {
    match instance_type {
        InstanceType::Null => Value::Null,
        InstanceType::Boolean => {
            let b = read_byte(cursor) != 0;
            Value::Bool(b)
        }
        InstanceType::Number | InstanceType::Integer => {
            if let Some(min) = schema.minimum {
                let delta = read_uvarint128(cursor);
                // Convert u128 delta to i128 to add to min (i128)
                let val = min + delta as i128;
                // Try to fit in i64 if possible for cleaner Value
                if let Ok(v) = i64::try_from(val) {
                    return Value::Number(Number::Int64(v));
                }
                // Fallback to Decimal128 or just keep it as is if Number supported i128 directly
                // Number supports Decimal128 which holds i128
                return Value::Number(Number::Decimal128(crate::Decimal128 {
                    scale: 0,
                    value: val,
                }));
            }
            let len = read_uvarint(cursor) as usize;
            let bytes = read_bytes(cursor, len);
            let n = Number::decode(bytes).unwrap_or(Number::Int64(0));
            Value::Number(n)
        }
        InstanceType::String => {
            let len = read_uvarint(cursor) as usize;
            let s = read_bytes(cursor, len);
            let s_str = String::from_utf8_lossy(s).into_owned();
            Value::String(Cow::Owned(s_str))
        }
        InstanceType::Object => {
            let mut obj = BTreeMap::new();
            let required_default = BTreeSet::new();
            let required = schema.required.as_ref().unwrap_or(&required_default);
            let properties_default = BTreeMap::new();
            let properties = schema.properties.as_ref().unwrap_or(&properties_default);

            for key in required {
                let sub_schema = properties.get(key);
                let val = decode_value(cursor, sub_schema);
                obj.insert(key.clone(), val);
            }

            let count = read_uvarint(cursor);
            for _ in 0..count {
                let k_len = read_uvarint(cursor) as usize;
                let k_bytes = read_bytes(cursor, k_len);
                let k = String::from_utf8_lossy(k_bytes).into_owned();
                let v = decode_untyped_value(cursor);
                obj.insert(k, v);
            }
            Value::Object(obj)
        }
        InstanceType::Array => {
            let len = read_uvarint(cursor);
            let mut arr = Vec::with_capacity(len as usize);
            for _ in 0..len {
                arr.push(decode_untyped_value(cursor));
            }
            Value::Array(arr)
        }
    }
}

fn decode_untyped_value(cursor: &mut Cursor<&[u8]>) -> Value<'static> {
    let tag = read_byte(cursor);
    match tag {
        TAG_NULL => Value::Null,
        TAG_BOOL_FALSE => Value::Bool(false),
        TAG_BOOL_TRUE => Value::Bool(true),
        TAG_NUMBER => {
            let len = read_uvarint(cursor) as usize;
            let bytes = read_bytes(cursor, len);
            let n = Number::decode(bytes).unwrap_or(Number::Int64(0));
            Value::Number(n)
        }
        TAG_STRING => {
            let len = read_uvarint(cursor) as usize;
            let s = read_bytes(cursor, len);
            let s_str = String::from_utf8_lossy(s).into_owned();
            Value::String(Cow::Owned(s_str))
        }
        TAG_ARRAY => {
            let len = read_uvarint(cursor);
            let mut arr = Vec::with_capacity(len as usize);
            for _ in 0..len {
                arr.push(decode_untyped_value(cursor));
            }
            Value::Array(arr)
        }
        TAG_OBJECT => {
            let len = read_uvarint(cursor);
            let mut obj = BTreeMap::new();
            for _ in 0..len {
                let k_len = read_uvarint(cursor) as usize;
                let k_bytes = read_bytes(cursor, k_len);
                let k = String::from_utf8_lossy(k_bytes).into_owned();
                let v = decode_untyped_value(cursor);
                obj.insert(k, v);
            }
            Value::Object(obj)
        }
        _ => Value::Null,
    }
}

fn read_byte(cursor: &mut Cursor<&[u8]>) -> u8 {
    let pos = cursor.position() as usize;
    let buf = *cursor.get_ref();
    if pos < buf.len() {
        cursor.set_position((pos + 1) as u64);
        buf[pos]
    } else {
        0
    }
}

fn read_bytes<'a>(cursor: &mut Cursor<&'a [u8]>, len: usize) -> &'a [u8] {
    let pos = cursor.position() as usize;
    let buf = *cursor.get_ref();
    if pos + len <= buf.len() {
        cursor.set_position((pos + len) as u64);
        &buf[pos..pos + len]
    } else {
        &[]
    }
}

fn read_uvarint(cursor: &mut Cursor<&[u8]>) -> u64 {
    let mut n: u64 = 0;
    let mut shift = 0;
    loop {
        let b = read_byte(cursor);
        n |= ((b & 0x7F) as u64) << shift;
        if (b & 0x80) == 0 {
            break;
        }
        shift += 7;
    }
    n
}

fn read_uvarint128(cursor: &mut Cursor<&[u8]>) -> u128 {
    let mut n: u128 = 0;
    let mut shift = 0;
    loop {
        let b = read_byte(cursor);
        n |= ((b & 0x7F) as u128) << shift;
        if (b & 0x80) == 0 {
            break;
        }
        shift += 7;
    }
    n
}
