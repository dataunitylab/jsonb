use jsonb_schema::schema::{decode, encode, Schema};
use jsonb_schema::Number;
use jsonb_schema::Value;
use std::borrow::Cow;

#[test]
fn test_const_value() {
    let schema_json = r#"{
        "const": "constant_string"
    }"#;
    let schema: Schema = serde_json::from_str(schema_json).unwrap();

    let val = Value::String(Cow::Borrowed("constant_string"));
    let mut buf = Vec::new();

    // Encoder should write nothing
    encode(&val, &schema, &mut buf);
    assert!(buf.is_empty());

    // Decoder should reconstruct the value from schema
    let decoded = decode(&buf, &schema);
    assert_eq!(decoded, val);
}

#[test]
fn test_enum_values() {
    let schema_json = r#"{
        "enum": [1, "a", true]
    }"#;
    let schema: Schema = serde_json::from_str(schema_json).unwrap();

    // Test Integer (index 0)
    let val1 = Value::Number(Number::Int64(1));
    let mut buf = Vec::new();
    encode(&val1, &schema, &mut buf);
    // Should encode index 0 (1 byte: 0x00)
    assert_eq!(buf.len(), 1);
    assert_eq!(buf[0], 0x00);
    let decoded1 = decode(&buf, &schema);
    assert_eq!(decoded1, val1);

    // Test String (index 1)
    let val2 = Value::String(Cow::Borrowed("a"));
    buf.clear();
    encode(&val2, &schema, &mut buf);
    // Should encode index 1 (1 byte: 0x01)
    assert_eq!(buf.len(), 1);
    assert_eq!(buf[0], 0x01);
    let decoded2 = decode(&buf, &schema);
    assert_eq!(decoded2, val2);

    // Test Boolean (index 2)
    let val3 = Value::Bool(true);
    buf.clear();
    encode(&val3, &schema, &mut buf);
    // Should encode index 2 (1 byte: 0x02)
    assert_eq!(buf.len(), 1);
    assert_eq!(buf[0], 0x02);
    let decoded3 = decode(&buf, &schema);
    assert_eq!(decoded3, val3);
}
