use jsonb_schema::schema::{decode, encode, InstanceType, Schema, SingleOrVec};
use jsonb_schema::Value;
use std::borrow::Cow;

#[test]
fn test_format_uuid_compression() {
    let schema = Schema {
        instance_type: Some(SingleOrVec::Single(InstanceType::String)),
        format: Some("uuid".to_string()),
        ..Schema::default()
    };

    let uuid_str = "123e4567-e89b-12d3-a456-426614174000";
    let value = Value::String(Cow::Borrowed(uuid_str));

    let mut buf = Vec::new();
    encode(&value, &schema, &mut buf);

    // TAG_UUID_COMPRESSED (0x06) + 16 bytes = 17 bytes.
    assert_eq!(buf.len(), 17);
    assert_eq!(buf[0], 0x06);

    // Check first few bytes. 123e4567 -> 0x12, 0x3e, 0x45, 0x67
    assert_eq!(buf[1], 0x12);
    assert_eq!(buf[2], 0x3e);
    assert_eq!(buf[3], 0x45);
    assert_eq!(buf[4], 0x67);

    let decoded = decode(&buf, &schema);
    assert_eq!(decoded, value);
}

#[test]
fn test_format_uuid_fallback() {
    let schema = Schema {
        instance_type: Some(SingleOrVec::Single(InstanceType::String)),
        format: Some("uuid".to_string()),
        ..Schema::default()
    };

    let invalid_uuid = "not-a-uuid";
    let value = Value::String(Cow::Borrowed(invalid_uuid));

    let mut buf = Vec::new();
    encode(&value, &schema, &mut buf);

    // TAG_STRING_UNCOMPRESSED (0x00) + uvarint(len) + bytes.
    assert_eq!(buf[0], 0x00);

    let decoded = decode(&buf, &schema);
    assert_eq!(decoded, value);
}
