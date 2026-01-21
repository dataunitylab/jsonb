use jsonb_schema::schema::{decode, encode, InstanceType, Schema, SingleOrVec};
use jsonb_schema::Value;
use std::borrow::Cow;

#[test]
fn test_format_time_compression_z() {
    let schema = Schema {
        instance_type: Some(SingleOrVec::Single(InstanceType::String)),
        format: Some("time".to_string()),
        ..Schema::default()
    };

    let time_str = "20:20:39Z";
    let value = Value::String(Cow::Borrowed(time_str));

    let mut buf = Vec::new();
    encode(&value, &schema, &mut buf);

    // TAG_TIME_COMPRESSED (0x02) + H(1) + M(1) + S(1) + Nanos(4) + Sign(1) + OffH(1) + OffM(1) = 11 bytes.
    assert_eq!(buf.len(), 11);
    assert_eq!(buf[0], 0x02);

    // Check H, M, S
    assert_eq!(buf[1], 20);
    assert_eq!(buf[2], 20);
    assert_eq!(buf[3], 39);
    // Nanos 0
    assert_eq!(buf[4], 0);
    assert_eq!(buf[5], 0);
    assert_eq!(buf[6], 0);
    assert_eq!(buf[7], 0);
    // Sign 0 (Z)
    assert_eq!(buf[8], 0);
    // Offset 0
    assert_eq!(buf[9], 0);
    assert_eq!(buf[10], 0);

    let decoded = decode(&buf, &schema);
    assert_eq!(decoded, value);
}

#[test]
fn test_format_time_compression_offset() {
    let schema = Schema {
        instance_type: Some(SingleOrVec::Single(InstanceType::String)),
        format: Some("time".to_string()),
        ..Schema::default()
    };

    let time_str = "20:20:39.4+03:30";
    let value = Value::String(Cow::Borrowed(time_str));

    let mut buf = Vec::new();
    encode(&value, &schema, &mut buf);

    assert_eq!(buf.len(), 11);
    assert_eq!(buf[0], 0x02);

    // Nanos .4 -> 400,000,000 (0x17D78400)
    assert_eq!(buf[4], 0x17);
    assert_eq!(buf[5], 0xD7);
    assert_eq!(buf[6], 0x84);
    assert_eq!(buf[7], 0x00);

    // Sign 1 (+)
    assert_eq!(buf[8], 1);
    // Offset 03:30
    assert_eq!(buf[9], 3);
    assert_eq!(buf[10], 30);

    let decoded = decode(&buf, &schema);
    assert_eq!(decoded, value);
}

#[test]
fn test_format_time_fallback() {
    let schema = Schema {
        instance_type: Some(SingleOrVec::Single(InstanceType::String)),
        format: Some("time".to_string()),
        ..Schema::default()
    };

    let invalid_time = "invalid-time";
    let value = Value::String(Cow::Borrowed(invalid_time));

    let mut buf = Vec::new();
    encode(&value, &schema, &mut buf);

    // TAG_STRING_UNCOMPRESSED (0x00) + uvarint(12) + bytes.
    // 1 + 1 + 12 = 14 bytes.
    assert_eq!(buf.len(), 14);
    assert_eq!(buf[0], 0x00);

    let decoded = decode(&buf, &schema);
    assert_eq!(decoded, value);
}
