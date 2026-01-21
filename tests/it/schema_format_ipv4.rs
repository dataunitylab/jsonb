use jsonb_schema::schema::{decode, encode, InstanceType, Schema, SingleOrVec};
use jsonb_schema::Value;
use std::borrow::Cow;

#[test]
fn test_format_ipv4_compression() {
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
        format: Some("ipv4".to_string()),
        pattern: None,
        pattern_prefix: None,
        pattern_suffix: None,
    };

    let ip_str = "192.168.1.1";
    let value = Value::String(Cow::Borrowed(ip_str));

    let mut buf = Vec::new();
    encode(&value, &schema, &mut buf);

    // TAG_IPV4_COMPRESSED (0x04) + 4 octets = 5 bytes.
    assert_eq!(buf.len(), 5);
    assert_eq!(buf[0], 0x04);
    assert_eq!(buf[1], 192);
    assert_eq!(buf[2], 168);
    assert_eq!(buf[3], 1);
    assert_eq!(buf[4], 1);

    let decoded = decode(&buf, &schema);
    assert_eq!(decoded, value);
}

#[test]
fn test_format_ipv4_fallback() {
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
        format: Some("ipv4".to_string()),
        pattern: None,
        pattern_prefix: None,
        pattern_suffix: None,
    };

    let invalid_ip = "999.999.999.999";
    let value = Value::String(Cow::Borrowed(invalid_ip));

    let mut buf = Vec::new();
    encode(&value, &schema, &mut buf);

    // TAG_STRING_UNCOMPRESSED (0x00) + uvarint(len) + bytes.
    assert_eq!(buf[0], 0x00);

    let decoded = decode(&buf, &schema);
    assert_eq!(decoded, value);
}
