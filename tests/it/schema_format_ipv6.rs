use jsonb_schema::schema::{decode, encode, InstanceType, Schema, SingleOrVec};
use jsonb_schema::Value;
use std::borrow::Cow;

#[test]
fn test_format_ipv6_compression() {
    let schema = Schema {
        instance_type: Some(SingleOrVec::Single(InstanceType::String)),
        properties: None,
        required: None,
        minimum: None,
        maximum: None,
        multiple_of: None,
        prefix_items: None,
        items: None,
        enum_values: None,
        const_value: None,
        format: Some("ipv6".to_string()),
        pattern: None,
        pattern_prefix: None,
        pattern_suffix: None,
    };

    let ip_str = "2001:db8::1";
    let value = Value::String(Cow::Borrowed(ip_str));

    let mut buf = Vec::new();
    encode(&value, &schema, &mut buf);

    // TAG_IPV6_COMPRESSED (0x05) + 16 bytes = 17 bytes.
    assert_eq!(buf.len(), 17);
    assert_eq!(buf[0], 0x05);

    // 2001:db8::1 -> 2001, 0db8, 0000, 0000, 0000, 0000, 0000, 0001
    // [32, 1, 13, 184, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1]
    assert_eq!(buf[1], 0x20);
    assert_eq!(buf[2], 0x01);
    assert_eq!(buf[3], 0x0D);
    assert_eq!(buf[4], 0xB8);
    // ... zeros ...
    assert_eq!(buf[16], 0x01);

    let decoded = decode(&buf, &schema);
    assert_eq!(decoded, value);
}

#[test]
fn test_format_ipv6_fallback() {
    let schema = Schema {
        instance_type: Some(SingleOrVec::Single(InstanceType::String)),
        properties: None,
        required: None,
        minimum: None,
        maximum: None,
        multiple_of: None,
        prefix_items: None,
        items: None,
        enum_values: None,
        const_value: None,
        format: Some("ipv6".to_string()),
        pattern: None,
        pattern_prefix: None,
        pattern_suffix: None,
    };

    let invalid_ip = "not-an-ipv6-address";
    let value = Value::String(Cow::Borrowed(invalid_ip));

    let mut buf = Vec::new();
    encode(&value, &schema, &mut buf);

    // TAG_STRING_UNCOMPRESSED (0x00) + uvarint(len) + bytes.
    assert_eq!(buf[0], 0x00);

    let decoded = decode(&buf, &schema);
    assert_eq!(decoded, value);
}
