use jsonb_schema::schema::{decode, encode, InstanceType, Schema, SingleOrVec};
use jsonb_schema::Value;
use std::borrow::Cow;

#[test]
fn test_format_date_compression() {
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
        format: Some("date".to_string()),
        pattern: None,
        pattern_prefix: None,
        pattern_suffix: None,
    };

    let date_str = "2023-10-25";
    let value = Value::String(Cow::Borrowed(date_str));

    let mut buf = Vec::new();
    encode(&value, &schema, &mut buf);

    // Expected: TAG_DATE_COMPRESSED (0x01) + Year(2) + Month(1) + Day(1) = 5 bytes.
    assert_eq!(buf.len(), 5);
    assert_eq!(buf[0], 0x01); // TAG_DATE_COMPRESSED

    // Year 2023 = 0x07E7
    assert_eq!(buf[1], 0x07);
    assert_eq!(buf[2], 0xE7);
    assert_eq!(buf[3], 10); // Month
    assert_eq!(buf[4], 25); // Day

    let decoded = decode(&buf, &schema);
    assert_eq!(decoded, value);
}

#[test]
fn test_format_date_fallback() {
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
        format: Some("date".to_string()),
        pattern: None,
        pattern_prefix: None,
        pattern_suffix: None,
    };

    let invalid_date = "not-a-date";
    let value = Value::String(Cow::Borrowed(invalid_date));

    let mut buf = Vec::new();
    encode(&value, &schema, &mut buf);

    // Expected: TAG_STRING_UNCOMPRESSED (0x00) + uvarint(len) + bytes.
    // "not-a-date" len is 10. uvarint(10) is 0x0A.
    // Total len: 1 + 1 + 10 = 12 bytes.
    assert_eq!(buf.len(), 12);
    assert_eq!(buf[0], 0x00); // TAG_STRING_UNCOMPRESSED
    assert_eq!(buf[1], 10); // Length

    let decoded = decode(&buf, &schema);
    assert_eq!(decoded, value);
}

#[test]
fn test_format_date_empty() {
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
        format: Some("date".to_string()),
        pattern: None,
        pattern_prefix: None,
        pattern_suffix: None,
    };

    let value = Value::String(Cow::Borrowed(""));

    let mut buf = Vec::new();
    encode(&value, &schema, &mut buf);

    // Expected: TAG_STRING_UNCOMPRESSED (0x00) + uvarint(0) + bytes(0).
    // Total len: 1 + 1 = 2 bytes.
    assert_eq!(buf.len(), 2);
    assert_eq!(buf[0], 0x00);
    assert_eq!(buf[1], 0x00);

    let decoded = decode(&buf, &schema);
    assert_eq!(decoded, value);
}
