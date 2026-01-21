use jsonb_schema::schema::{decode, encode, InstanceType, Schema, SingleOrVec};
use jsonb_schema::Number;
use jsonb_schema::Value;
use std::borrow::Cow;

#[test]
fn test_schema_multi_type_serialization() {
    let json = r#"{"type":["string", "integer", "null"]}"#;
    let schema: Schema = serde_json::from_str(json).unwrap();
    match schema.instance_type {
        Some(SingleOrVec::Vec(types)) => {
            assert_eq!(types.len(), 3);
            assert!(types.contains(&InstanceType::String));
            assert!(types.contains(&InstanceType::Integer));
            assert!(types.contains(&InstanceType::Null));
        }
        _ => panic!("Expected Vec"),
    }
}

#[test]
fn test_multi_type_encoding_decoding() {
    // Schema: {"type": ["integer", "string"], "minimum": 1000}
    // "minimum" applies if it's an integer.
    let schema = Schema {
        instance_type: Some(SingleOrVec::Vec(vec![
            InstanceType::Integer,
            InstanceType::String,
        ])),
        properties: None,
        required: None,
        minimum: Some(1000),
        maximum: None,
        multiple_of: None,
        prefix_items: None,
        items: None,
        enum_values: None,
        const_value: None,
        format: None,
    };

    // Case 1: Integer (should be delta encoded if we implement it, or at least round-trip)
    let val_int = Value::Number(Number::Int64(1005));
    let mut buf = Vec::new();
    encode(&val_int, &schema, &mut buf);
    let decoded_int = decode(&buf, &schema);
    assert_eq!(val_int, decoded_int);

    // Case 2: String (standard encoding)
    let val_str = Value::String(Cow::Borrowed("hello"));
    buf.clear();
    encode(&val_str, &schema, &mut buf);
    let decoded_str = decode(&buf, &schema);
    assert_eq!(val_str, decoded_str);
}
