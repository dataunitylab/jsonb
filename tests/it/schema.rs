use jsonb_schema::schema::{decode, encode, InstanceType, Schema, SingleOrVec};
use jsonb_schema::Number;
use jsonb_schema::Value;
use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};

#[test]
fn test_schema_serialization() {
    let mut properties = BTreeMap::new();
    properties.insert(
        "name".to_string(),
        Schema {
            instance_type: Some(SingleOrVec::Single(InstanceType::String)),
            ..Schema::default()
        },
    );

    let mut required = BTreeSet::new();
    required.insert("name".to_string());

    let schema = Schema {
        instance_type: Some(SingleOrVec::Single(InstanceType::Object)),
        properties: Some(properties),
        required: Some(required),
        ..Schema::default()
    };

    let json = serde_json::to_string(&schema).unwrap();
    // The order of map keys in JSON output depends on the map implementation. BTreeMap preserves order, so it should be deterministic.
    assert_eq!(
        json,
        r#"{"type":"object","properties":{"name":{"type":"string"}},"required":["name"]}"#
    );
}

#[test]
fn test_schema_deserialization() {
    let json = r#"{"type":["string", "null"],"required":[]}"#;
    let schema: Schema = serde_json::from_str(json).unwrap();

    match schema.instance_type {
        Some(SingleOrVec::Vec(types)) => {
            assert_eq!(types.len(), 2);
            assert!(types.contains(&InstanceType::String));
            assert!(types.contains(&InstanceType::Null));
        }
        _ => panic!("Expected Vec of types"),
    }
    assert!(schema.required.unwrap().is_empty());
}

#[test]
fn test_schema_encoding_decoding() {
    // Schema: {"type": "object", "properties": {"a": {"type": "integer"}, "b": {"type": "string"}}, "required": ["a", "b"]}
    let mut properties = BTreeMap::new();
    properties.insert(
        "a".to_string(),
        Schema {
            instance_type: Some(SingleOrVec::Single(InstanceType::Integer)),
            ..Schema::default()
        },
    );
    properties.insert(
        "b".to_string(),
        Schema {
            instance_type: Some(SingleOrVec::Single(InstanceType::String)),
            ..Schema::default()
        },
    );

    let mut required = BTreeSet::new();
    required.insert("a".to_string());
    required.insert("b".to_string());

    let schema = Schema {
        instance_type: Some(SingleOrVec::Single(InstanceType::Object)),
        properties: Some(properties),
        required: Some(required),
        ..Schema::default()
    };

    // Value: {"a": 10, "b": "hello"}
    let mut obj = BTreeMap::new();
    obj.insert("a".to_string(), Value::Number(Number::Int64(10)));
    obj.insert("b".to_string(), Value::String(Cow::Borrowed("hello")));
    let value = Value::Object(obj);

    let mut buf = Vec::new();
    encode(&value, &schema, &mut buf);

    let decoded = decode(&buf, &schema);
    assert_eq!(value, decoded);
}

#[test]
fn test_schema_encoding_extra_keys() {
    // Schema: {"type": "object", "properties": {"a": {"type": "integer"}}, "required": ["a"]}
    let mut properties = BTreeMap::new();
    properties.insert(
        "a".to_string(),
        Schema {
            instance_type: Some(SingleOrVec::Single(InstanceType::Integer)),
            ..Schema::default()
        },
    );

    let mut required = BTreeSet::new();
    required.insert("a".to_string());

    let schema = Schema {
        instance_type: Some(SingleOrVec::Single(InstanceType::Object)),
        properties: Some(properties),
        required: Some(required),
        ..Schema::default()
    };

    // Value: {"a": 10, "c": true}
    let mut obj = BTreeMap::new();
    obj.insert("a".to_string(), Value::Number(Number::Int64(10)));
    obj.insert("c".to_string(), Value::Bool(true));
    let value = Value::Object(obj);

    let mut buf = Vec::new();
    encode(&value, &schema, &mut buf);

    let decoded = decode(&buf, &schema);
    assert_eq!(value, decoded);
}

#[test]
fn test_delta_encoding_integers() {
    let min_val = 1000;
    // Schema: {"type": "integer", "minimum": 1000}
    let schema = Schema {
        instance_type: Some(SingleOrVec::Single(InstanceType::Integer)),
        properties: None,
        required: None,
        minimum: Some(min_val),
        ..Schema::default()
    };

    let val = 1005; // delta is 5
    let value = Value::Number(Number::Int64(val));

    let mut buf = Vec::new();
    encode(&value, &schema, &mut buf);

    // Delta 5 encodes as uvarint(5) -> 0x05.
    // Standard Int64 encoding for 1005 would be:
    // compact_encode: NUMBER_INT (0x40) + i16 (2 bytes) = 3 bytes total (approx) + length prefix uvarint.
    // 1005 fits in i16.
    // Old encoding: uvarint(len) + [tag, bytes...]
    // New encoding: TAG_OPTIMIZED_NUMBER (0xFF) + uvarint(delta) -> 2 bytes.

    assert_eq!(buf.len(), 2);
    assert_eq!(buf[0], 0xFF);
    assert_eq!(buf[1], 0x05);

    let decoded = decode(&buf, &schema);

    if let Value::Number(n) = decoded {
        assert_eq!(n.as_i64(), Some(val));
    } else {
        panic!("Expected Number");
    }
}
