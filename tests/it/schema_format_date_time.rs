use jsonb_schema::schema::{decode, encode, InstanceType, Schema, SingleOrVec};
use jsonb_schema::Value;
use std::borrow::Cow;

#[test]
fn test_format_date_time_compression_z() {
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
        format: Some("date-time".to_string()),
    };

    let dt_str = "2023-10-25T20:20:39Z";
    let value = Value::String(Cow::Borrowed(dt_str));

    let mut buf = Vec::new();
    encode(&value, &schema, &mut buf);

    // TAG_DATE_TIME_COMPRESSED (0x03) + Date(4) + Time(10) = 15 bytes.
    // Date: Y(2)+M(1)+D(1) = 4.
    // Time: H(1)+M(1)+S(1)+N(4)+Sign(1)+OH(1)+OM(1) = 10.
    // Total 15.
    assert_eq!(buf.len(), 15);
    assert_eq!(buf[0], 0x03);

    // Check Y, M, D
    // 2023 = 0x07E7
    assert_eq!(buf[1], 0x07);
    assert_eq!(buf[2], 0xE7);
    assert_eq!(buf[3], 10);
    assert_eq!(buf[4], 25);

    // Check Time H, M, S
    assert_eq!(buf[5], 20);
    assert_eq!(buf[6], 20);
    assert_eq!(buf[7], 39);

    // Sign Z
    assert_eq!(buf[12], 0);

    let decoded = decode(&buf, &schema);
    assert_eq!(decoded, value);
}

#[test]
fn test_format_date_time_compression_offset() {
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
        format: Some("date-time".to_string()),
    };

    let dt_str = "2023-10-25T20:20:39.4+03:30";
    let value = Value::String(Cow::Borrowed(dt_str));

    let mut buf = Vec::new();
    encode(&value, &schema, &mut buf);

    assert_eq!(buf.len(), 15);
    assert_eq!(buf[0], 0x03);

    // Check Y, M, D
    assert_eq!(buf[1], 0x07);
    assert_eq!(buf[2], 0xE7);
    assert_eq!(buf[3], 10);
    assert_eq!(buf[4], 25);

    // Check Time H, M, S
    assert_eq!(buf[5], 20);
    assert_eq!(buf[6], 20);
    assert_eq!(buf[7], 39);

    // Nanos .4 -> 400,000,000 (0x17D78400)
    assert_eq!(buf[8], 0x17);

    // Sign +
    assert_eq!(buf[12], 1);
    // Offset 03:30
    assert_eq!(buf[13], 3);
    assert_eq!(buf[14], 30);

    let decoded = decode(&buf, &schema);
    assert_eq!(decoded, value);
}

#[test]
fn test_format_date_time_fallback() {
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
        format: Some("date-time".to_string()),
    };

    let invalid_dt = "not-a-datetime";
    let value = Value::String(Cow::Borrowed(invalid_dt));

    let mut buf = Vec::new();
    encode(&value, &schema, &mut buf);

    // TAG_STRING_UNCOMPRESSED (0x00) + uvarint(len) + bytes.
    // Len 14. uvarint(14) is 0x0E.
    // 1 + 1 + 14 = 16 bytes.
    assert_eq!(buf.len(), 16);
    assert_eq!(buf[0], 0x00);

    let decoded = decode(&buf, &schema);
    assert_eq!(decoded, value);
}
