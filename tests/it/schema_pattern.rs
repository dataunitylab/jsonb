use jsonb_schema::schema::{decode, encode, from_serde_json, InstanceType, Schema, SingleOrVec};
use jsonb_schema::Value;
use serde_json::json;
use std::borrow::Cow;

#[test]
fn test_pattern_prefix() {
    let json = json!({
        "type": "string",
        "pattern": "^http://"
    });
    let schema = from_serde_json(&json).unwrap();
    assert_eq!(schema.pattern_prefix, Some("http://".to_string()));
    assert_eq!(schema.pattern_suffix, None);

    let val_str = "http://example.com";
    let value = Value::String(Cow::Borrowed(val_str));
    let mut buf = Vec::new();
    encode(&value, &schema, &mut buf);

    // TAG_PATTERN_COMPRESSED (0x07) + uvarint(11) + "example.com"
    // "example.com" len is 11.
    // 1 + 1 + 11 = 13 bytes.
    assert_eq!(buf.len(), 13);
    assert_eq!(buf[0], 0x07);

    let decoded = decode(&buf, &schema);
    assert_eq!(decoded, value);
}

#[test]
fn test_pattern_suffix() {
    let json = json!({
        "type": "string",
        "pattern": "\\.com$"
    });
    let schema = from_serde_json(&json).unwrap();
    assert_eq!(schema.pattern_prefix, None);
    assert_eq!(schema.pattern_suffix, Some(".com".to_string()));

    let val_str = "example.com";
    let value = Value::String(Cow::Borrowed(val_str));
    let mut buf = Vec::new();
    encode(&value, &schema, &mut buf);

    // TAG_PATTERN_COMPRESSED (0x07) + uvarint(7) + "example"
    // "example" len is 7.
    // 1 + 1 + 7 = 9 bytes.
    assert_eq!(buf.len(), 9);
    assert_eq!(buf[0], 0x07);

    let decoded = decode(&buf, &schema);
    assert_eq!(decoded, value);
}

#[test]
fn test_pattern_both() {
    let json = json!({
        "type": "string",
        "pattern": "^http://(.*)\\.com$"
    });
    let schema = from_serde_json(&json).unwrap();
    assert_eq!(schema.pattern_prefix, Some("http://".to_string()));
    assert_eq!(schema.pattern_suffix, Some(".com".to_string()));

    let val_str = "http://example.com";
    let value = Value::String(Cow::Borrowed(val_str));
    let mut buf = Vec::new();
    encode(&value, &schema, &mut buf);

    // TAG_PATTERN_COMPRESSED (0x07) + uvarint(7) + "example"
    // 1 + 1 + 7 = 9 bytes.
    assert_eq!(buf.len(), 9);
    assert_eq!(buf[0], 0x07);

    let decoded = decode(&buf, &schema);
    assert_eq!(decoded, value);
}

#[test]
fn test_pattern_mismatch() {
    let json = json!({
        "type": "string",
        "pattern": "^http://"
    });
    let schema = from_serde_json(&json).unwrap();

    let val_str = "https://example.com";
    let value = Value::String(Cow::Borrowed(val_str));
    let mut buf = Vec::new();
    encode(&value, &schema, &mut buf);

    // TAG_STRING_UNCOMPRESSED (0x00) + uvarint(19) + "https://example.com"
    // 1 + 1 + 19 = 21 bytes.
    assert_eq!(buf.len(), 21);
    assert_eq!(buf[0], 0x00);

    let decoded = decode(&buf, &schema);
    assert_eq!(decoded, value);
}

#[test]
fn test_pattern_none() {
    let json = json!({
        "type": "string",
        "pattern": ".*"
    });
    let schema = from_serde_json(&json).unwrap();
    assert_eq!(schema.pattern_prefix, None);
    assert_eq!(schema.pattern_suffix, None);
}

#[test]
fn test_pattern_complex_prefix() {
    let json = json!({
        "type": "string",
        "pattern": "^http://.*"
    });
    let schema = from_serde_json(&json).unwrap();
    assert_eq!(schema.pattern_prefix, Some("http://".to_string()));
    assert_eq!(schema.pattern_suffix, None);
}
