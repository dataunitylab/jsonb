use crate::schema::{InstanceType, Schema, SingleOrVec};
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
const TAG_OPTIMIZED_NUMBER: u8 = 0xFF;
const TAG_DATE_COMPRESSED: u8 = 0x01;
const TAG_TIME_COMPRESSED: u8 = 0x02;
const TAG_DATE_TIME_COMPRESSED: u8 = 0x03;
const TAG_IPV4_COMPRESSED: u8 = 0x04;
const TAG_IPV6_COMPRESSED: u8 = 0x05;
const TAG_UUID_COMPRESSED: u8 = 0x06;
const TAG_PATTERN_COMPRESSED: u8 = 0x07;
const TAG_STRING_UNCOMPRESSED: u8 = 0x00;

use crate::from_raw_jsonb;
use jiff::civil::{Date, Time}; // Ensure jiff is available

pub fn encode(value: &Value, schema: &Schema, buf: &mut Vec<u8>) {
    encode_value(value, Some(schema), buf);
}

fn encode_value(value: &Value, schema: Option<&Schema>, buf: &mut Vec<u8>) {
    if let Some(schema) = schema {
        if schema.const_value.is_some() {
            return;
        }
        if let Some(enums) = &schema.enum_values {
            let serde_val = to_serde_value(value);
            if let Some(idx) = enums.iter().position(|v| v == &serde_val) {
                write_uvarint(buf, idx as u64);
                return;
            }
        }

        match &schema.instance_type {
            Some(SingleOrVec::Single(instance_type)) => {
                encode_typed_value(value, instance_type, schema, buf);
                return;
            }
            Some(SingleOrVec::Vec(types)) => {
                // Check if we can use delta encoding for numbers in a union
                if let Value::Number(n) = value {
                    if types.contains(&InstanceType::Integer)
                        || types.contains(&InstanceType::Number)
                    {
                        if let Some(val) = n.as_i128() {
                            if let Some(mul) = schema.multiple_of {
                                if let Some(min) = schema.minimum {
                                    if min % mul == 0 {
                                        if val >= min {
                                            let delta = (val - min) / mul;
                                            buf.push(TAG_NUMBER);
                                            buf.push(TAG_OPTIMIZED_NUMBER);
                                            write_uvarint128(buf, delta as u128);
                                            return;
                                        }
                                    } else {
                                        let res = val / mul;
                                        buf.push(TAG_NUMBER);
                                        buf.push(TAG_OPTIMIZED_NUMBER);
                                        write_uvarint128(buf, res as u128);
                                        return;
                                    }
                                } else {
                                    let res = val / mul;
                                    buf.push(TAG_NUMBER);
                                    buf.push(TAG_OPTIMIZED_NUMBER);
                                    write_uvarint128(buf, res as u128);
                                    return;
                                }
                            } else if let Some(min) = schema.minimum {
                                if val >= min {
                                    let delta = (val - min) as u128;
                                    buf.push(TAG_NUMBER);
                                    buf.push(TAG_OPTIMIZED_NUMBER);
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

fn to_serde_value(value: &Value) -> serde_json::Value {
    // Convert jsonb::Value to RawJsonb then to serde_json::Value
    let vec = value.to_vec();
    let raw = crate::RawJsonb::new(&vec);
    from_raw_jsonb(&raw).unwrap()
}

fn encode_typed_value(
    value: &Value,
    instance_type: &InstanceType,
    schema: &Schema,
    buf: &mut Vec<u8>,
) {
    match (instance_type, value) {
        (InstanceType::Null, _) => {
            // Null is 0 bytes if typed
        }
        (InstanceType::Boolean, Value::Bool(b)) => {
            buf.push(if *b { 1 } else { 0 });
        }
        (InstanceType::Number, Value::Number(n)) | (InstanceType::Integer, Value::Number(n)) => {
            if let Some(mul) = schema.multiple_of {
                if let Some(val) = n.as_i128() {
                    if let Some(min) = schema.minimum {
                        if min % mul == 0 {
                            if val >= min {
                                let delta = (val - min) / mul;
                                buf.push(TAG_OPTIMIZED_NUMBER);
                                write_uvarint128(buf, delta as u128);
                                return;
                            }
                        } else {
                            let res = val / mul;
                            buf.push(TAG_OPTIMIZED_NUMBER);
                            write_uvarint128(buf, res as u128);
                            return;
                        }
                    } else {
                        let res = val / mul;
                        buf.push(TAG_OPTIMIZED_NUMBER);
                        write_uvarint128(buf, res as u128);
                        return;
                    }
                } else {
                    // Try float optimization if it's perfectly divisible
                    let f_val = n.as_f64();
                    let f_mul = mul as f64;
                    if (f_val % f_mul) == 0.0 {
                        let res = (f_val / f_mul) as i128;
                        buf.push(TAG_OPTIMIZED_NUMBER);
                        write_uvarint128(buf, res as u128);
                        return;
                    }
                }
            } else if let Some(min) = schema.minimum {
                if let Some(val) = n.as_i128() {
                    if val >= min {
                        let delta = (val - min) as u128;
                        buf.push(TAG_OPTIMIZED_NUMBER);
                        write_uvarint128(buf, delta);
                        return;
                    }
                }
            }
            let mut temp = Vec::new();
            let _ = n.compact_encode(&mut temp).unwrap();
            write_uvarint(buf, temp.len() as u64);
            buf.extend_from_slice(&temp);
        }
        (InstanceType::String, Value::String(s)) => {
            if let Some(format) = &schema.format {
                if encode_string_with_format(format, s, buf) {
                    return;
                }
            } else if schema.pattern_prefix.is_some() || schema.pattern_suffix.is_some() {
                if encode_string_with_prefix_or_suffix(
                    &schema.pattern_prefix,
                    &schema.pattern_suffix,
                    s,
                    buf,
                ) {
                    return;
                }
            }

            write_uvarint(buf, s.len() as u64);
            buf.extend_from_slice(s.as_bytes());
        }
        (InstanceType::Object, Value::Object(obj)) => {
            let required_default = BTreeSet::new();
            let required = schema.required.as_ref().unwrap_or(&required_default);
            let properties_default = BTreeMap::new();
            let properties = schema.properties.as_ref().unwrap_or(&properties_default);

            encode_object(required, properties, obj, buf);
        }
        (InstanceType::Array, Value::Array(arr)) => {
            encode_array(&schema.prefix_items, &schema.items, arr, buf);
        }
        _ => {
            encode_untyped_value(value, buf);
        }
    }
}

fn encode_object(
    required: &BTreeSet<String>,
    properties: &BTreeMap<String, Schema>,
    obj: &BTreeMap<String, Value>,
    buf: &mut Vec<u8>,
) {
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
        if let Some(sub_schema) = properties.get(k) {
            encode_value(v, Some(sub_schema), buf);
        } else {
            encode_untyped_value(v, buf);
        }
    }
}

fn encode_array(
    prefix_items: &Option<Vec<Schema>>,
    items: &Option<Box<Schema>>,
    arr: &[Value],
    buf: &mut Vec<u8>,
) {
    write_uvarint(buf, arr.len() as u64);
    for (i, v) in arr.iter().enumerate() {
        if let Some(prefix_items) = prefix_items {
            if i < prefix_items.len() {
                encode_value(v, Some(&prefix_items[i]), buf);
                continue;
            }
        }
        if let Some(items) = &items {
            encode_value(v, Some(items), buf);
            continue;
        }
        encode_untyped_value(v, buf);
    }
}

fn encode_string_with_format(format: &str, s: &str, buf: &mut Vec<u8>) -> bool {
    if format == "date" {
        if let Ok(date) = s.parse::<Date>() {
            let y = date.year();
            let m = date.month();
            let d = date.day();
            if y >= 0 && y <= 9999 {
                buf.push(TAG_DATE_COMPRESSED);
                buf.extend_from_slice(&(y as u16).to_be_bytes());
                buf.push(m as u8);
                buf.push(d as u8);
                return true;
            }
        }
        buf.push(TAG_STRING_UNCOMPRESSED);
    } else if format == "time" {
        let mut time_part: &str = &s;
        let mut sign = 0; // 0=Z, 1=+, 2=-
        let mut off_h = 0;
        let mut off_m = 0;
        let mut valid = false;

        if s.ends_with('Z') {
            time_part = &s[..s.len() - 1];
            sign = 0;
            valid = true;
        } else if s.len() >= 6 {
            let sign_char = s.as_bytes()[s.len() - 6];
            let colon = s.as_bytes()[s.len() - 3];
            if colon == b':' && (sign_char == b'+' || sign_char == b'-') {
                time_part = &s[..s.len() - 6];
                if sign_char == b'+' {
                    sign = 1;
                } else {
                    sign = 2;
                }
                if let (Ok(h), Ok(m)) = (
                    s[s.len() - 5..s.len() - 3].parse::<u8>(),
                    s[s.len() - 2..].parse::<u8>(),
                ) {
                    off_h = h;
                    off_m = m;
                    valid = true;
                }
            }
        }

        if valid {
            if let Ok(t) = time_part.parse::<Time>() {
                if off_h <= 23 && off_m <= 60 {
                    buf.push(TAG_TIME_COMPRESSED);
                    buf.push(t.hour() as u8);
                    buf.push(t.minute() as u8);
                    buf.push(t.second() as u8);
                    buf.extend_from_slice(&(t.subsec_nanosecond() as u32).to_be_bytes());
                    buf.push(sign);
                    buf.push(off_h);
                    buf.push(off_m);
                    return true;
                }
            }
        }
        buf.push(TAG_STRING_UNCOMPRESSED);
    } else if format == "date-time" {
        if let Some(t_idx) = s.find('T').or_else(|| s.find('t')) {
            let date_part = &s[..t_idx];
            let time_part_full = &s[t_idx + 1..];

            if let Ok(date) = date_part.parse::<Date>() {
                let mut time_str = time_part_full;
                let mut sign = 0;
                let mut off_h = 0;
                let mut off_m = 0;
                let mut valid_time = false;

                if time_str.ends_with('Z') {
                    time_str = &time_str[..time_str.len() - 1];
                    sign = 0;
                    valid_time = true;
                } else if time_str.len() >= 6 {
                    let sign_char = time_str.as_bytes()[time_str.len() - 6];
                    let colon = time_str.as_bytes()[time_str.len() - 3];
                    if colon == b':' && (sign_char == b'+' || sign_char == b'-') {
                        if sign_char == b'+' {
                            sign = 1;
                        } else {
                            sign = 2;
                        }
                        if let (Ok(h), Ok(m)) = (
                            time_str[time_str.len() - 5..time_str.len() - 3].parse::<u8>(),
                            time_str[time_str.len() - 2..].parse::<u8>(),
                        ) {
                            off_h = h;
                            off_m = m;
                            time_str = &time_str[..time_str.len() - 6];
                            valid_time = true;
                        }
                    }
                }

                if valid_time {
                    if let Ok(t) = time_str.parse::<Time>() {
                        let y = date.year();
                        let m = date.month();
                        let d = date.day();

                        if y >= 0 && y <= 9999 && off_h <= 23 && off_m <= 60 {
                            buf.push(TAG_DATE_TIME_COMPRESSED);
                            // Date
                            buf.extend_from_slice(&(y as u16).to_be_bytes());
                            buf.push(m as u8);
                            buf.push(d as u8);
                            // Time
                            buf.push(t.hour() as u8);
                            buf.push(t.minute() as u8);
                            buf.push(t.second() as u8);
                            buf.extend_from_slice(&(t.subsec_nanosecond() as u32).to_be_bytes());
                            buf.push(sign);
                            buf.push(off_h);
                            buf.push(off_m);
                            return true;
                        }
                    }
                }
            }
        }
        buf.push(TAG_STRING_UNCOMPRESSED);
    } else if format == "ipv4" {
        if let Ok(addr) = s.parse::<std::net::Ipv4Addr>() {
            buf.push(TAG_IPV4_COMPRESSED);
            buf.extend_from_slice(&addr.octets());
            return true;
        }
        buf.push(TAG_STRING_UNCOMPRESSED);
    } else if format == "ipv6" {
        if let Ok(addr) = s.parse::<std::net::Ipv6Addr>() {
            buf.push(TAG_IPV6_COMPRESSED);
            buf.extend_from_slice(&addr.octets());
            return true;
        }
        buf.push(TAG_STRING_UNCOMPRESSED);
    } else if format == "uuid" {
        if let Ok(u) = s.parse::<uuid::Uuid>() {
            buf.push(TAG_UUID_COMPRESSED);
            buf.extend_from_slice(u.as_bytes());
            return true;
        }
        buf.push(TAG_STRING_UNCOMPRESSED);
    }

    false
}

fn encode_string_with_prefix_or_suffix(
    maybe_prefix: &Option<String>,
    maybe_suffix: &Option<String>,
    s: &str,
    buf: &mut Vec<u8>,
) -> bool {
    let mut s_slice: &str = &s;
    let mut matches = true;

    if let Some(prefix) = &maybe_prefix {
        if s_slice.starts_with(prefix) {
            s_slice = &s_slice[prefix.len()..];
        } else {
            matches = false;
        }
    }

    if matches {
        if let Some(suffix) = &maybe_suffix {
            if s_slice.ends_with(suffix) {
                s_slice = &s_slice[..s_slice.len() - suffix.len()];
            } else {
                matches = false;
            }
        }
    }

    if matches {
        buf.push(TAG_PATTERN_COMPRESSED);
        write_uvarint(buf, s_slice.len() as u64);
        buf.extend_from_slice(s_slice.as_bytes());
        return true;
    }

    buf.push(TAG_STRING_UNCOMPRESSED);
    false
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
        }
        Value::String(s) => {
            buf.push(TAG_STRING);
            write_uvarint(buf, s.len() as u64);
            buf.extend_from_slice(s.as_bytes());
        }
        Value::Array(arr) => {
            buf.push(TAG_ARRAY);
            write_uvarint(buf, arr.len() as u64);
            for v in arr {
                encode_untyped_value(v, buf);
            }
        }
        Value::Object(obj) => {
            buf.push(TAG_OBJECT);
            write_uvarint(buf, obj.len() as u64);
            for (k, v) in obj {
                write_uvarint(buf, k.len() as u64);
                buf.extend_from_slice(k.as_bytes());
                encode_untyped_value(v, buf);
            }
        }
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
