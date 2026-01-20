use crate::schema::{Schema, InstanceType, SingleOrVec};
use crate::Value;
use std::collections::{BTreeMap, BTreeSet};

// Define tags for untyped values
const TAG_NULL: u8 = 0x00;
const TAG_BOOL_FALSE: u8 = 0x01;
const TAG_BOOL_TRUE: u8 = 0x02;
const TAG_NUMBER: u8 = 0x03;
const TAG_STRING: u8 = 0x04;
const TAG_ARRAY: u8 = 0x05;
const TAG_OBJECT: u8 = 0x06;

pub fn encode(value: &Value, schema: &Schema, buf: &mut Vec<u8>) {
    encode_value(value, Some(schema), buf);
}

fn encode_value(value: &Value, schema: Option<&Schema>, buf: &mut Vec<u8>) {
    if let Some(schema) = schema {
        match &schema.instance_type {
            Some(SingleOrVec::Single(instance_type)) => {
                encode_typed_value(value, instance_type, schema, buf);
                return;
            }
            Some(SingleOrVec::Vec(types)) => {
                // Check if we can use delta encoding for numbers in a union
                if let Value::Number(n) = value {
                    if types.contains(&InstanceType::Integer) || types.contains(&InstanceType::Number) {
                        if let Some(min) = schema.minimum {
                            if let Some(val) = n.as_i128() {
                                if val >= min {
                                    let delta = (val - min) as u128;
                                    buf.push(TAG_NUMBER);
                                    write_uvarint128(buf, delta);
                                    return;
                                }
                            }
                        }
                    }
                }
            }
            None => {}
        }
    }
    // Fallback or untyped
    encode_untyped_value(value, buf);
}

fn encode_typed_value(value: &Value, instance_type: &InstanceType, schema: &Schema, buf: &mut Vec<u8>) {
    match (instance_type, value) {
        (InstanceType::Null, _) => {
            // Null is 0 bytes if typed
        },
        (InstanceType::Boolean, Value::Bool(b)) => {
            buf.push(if *b { 1 } else { 0 });
        },
        (InstanceType::Number, Value::Number(n)) | (InstanceType::Integer, Value::Number(n)) => {
            if let Some(min) = schema.minimum {
                if let Some(val) = n.as_i128() {
                    if val >= min {
                        let delta = (val - min) as u128;
                        write_uvarint128(buf, delta);
                        return;
                    }
                }
            }
            let mut temp = Vec::new();
            let _ = n.compact_encode(&mut temp).unwrap();
            write_uvarint(buf, temp.len() as u64);
            buf.extend_from_slice(&temp);
        },
        (InstanceType::String, Value::String(s)) => {
             write_uvarint(buf, s.len() as u64);
             buf.extend_from_slice(s.as_bytes());
        },
        (InstanceType::Object, Value::Object(obj)) => {
            let required_default = BTreeSet::new();
            let required = schema.required.as_ref().unwrap_or(&required_default);
            let properties_default = BTreeMap::new();
            let properties = schema.properties.as_ref().unwrap_or(&properties_default);

            // 1. Required keys
            for key in required {
                 let val = obj.get(key).unwrap_or(&Value::Null); 
                 let sub_schema = properties.get(key);
                 encode_value(val, sub_schema, buf);
            }
            
            // 2. Extra keys
            let mut extras = Vec::new();
            for (k, v) in obj {
                if !required.contains(k) {
                    extras.push((k, v));
                }
            }
            
            write_uvarint(buf, extras.len() as u64);
            for (k, v) in extras {
                write_uvarint(buf, k.len() as u64);
                buf.extend_from_slice(k.as_bytes());
                encode_untyped_value(v, buf);
            }
        },
         (InstanceType::Array, Value::Array(arr)) => {
             write_uvarint(buf, arr.len() as u64);
             for v in arr {
                 encode_untyped_value(v, buf);
             }
         },
         _ => {
             encode_untyped_value(value, buf);
         }
    }
}

fn encode_untyped_value(value: &Value, buf: &mut Vec<u8>) {
    match value {
        Value::Null => buf.push(TAG_NULL),
        Value::Bool(b) => buf.push(if *b { TAG_BOOL_TRUE } else { TAG_BOOL_FALSE }),
        Value::Number(n) => {
            buf.push(TAG_NUMBER);
            let mut temp = Vec::new();
            let _ = n.compact_encode(&mut temp).unwrap();
            write_uvarint(buf, temp.len() as u64);
            buf.extend_from_slice(&temp);
        },
        Value::String(s) => {
            buf.push(TAG_STRING);
            write_uvarint(buf, s.len() as u64);
            buf.extend_from_slice(s.as_bytes());
        },
        Value::Array(arr) => {
             buf.push(TAG_ARRAY);
             write_uvarint(buf, arr.len() as u64);
             for v in arr {
                 encode_untyped_value(v, buf);
             }
        },
        Value::Object(obj) => {
            buf.push(TAG_OBJECT);
            write_uvarint(buf, obj.len() as u64);
             for (k, v) in obj {
                write_uvarint(buf, k.len() as u64);
                buf.extend_from_slice(k.as_bytes());
                encode_untyped_value(v, buf);
            }
        },
        _ => {
             buf.push(TAG_NULL);
        }
    }
}

fn write_uvarint(buf: &mut Vec<u8>, mut n: u64) {
    while n >= 0x80 {
        buf.push((n as u8) | 0x80);
        n >>= 7;
    }
    buf.push(n as u8);
}

fn write_uvarint128(buf: &mut Vec<u8>, mut n: u128) {
    while n >= 0x80 {
        buf.push((n as u8) | 0x80);
        n >>= 7;
    }
    buf.push(n as u8);
}
