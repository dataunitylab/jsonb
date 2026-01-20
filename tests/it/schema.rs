use std::collections::{BTreeMap, BTreeSet};
use jsonb::schema::{Schema, InstanceType, SingleOrVec, encode, decode};
use jsonb::Value;
use jsonb::Number;
use std::borrow::Cow;

#[test]
fn test_schema_serialization() {
    let mut properties = BTreeMap::new();
    properties.insert("name".to_string(), Schema {
        instance_type: Some(SingleOrVec::Single(InstanceType::String)),
        properties: None,
        required: None,
    });
    
    let mut required = BTreeSet::new();
    required.insert("name".to_string());

    let schema = Schema {
        instance_type: Some(SingleOrVec::Single(InstanceType::Object)),
        properties: Some(properties),
        required: Some(required),
    };

    let json = serde_json::to_string(&schema).unwrap();
    // The order of map keys in JSON output depends on the map implementation. BTreeMap preserves order, so it should be deterministic.
    assert_eq!(json, r#"{"type":"object","properties":{"name":{"type":"string"}},"required":["name"]}"#);
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
        },
        _ => panic!("Expected Vec of types"),
    }
    assert!(schema.required.unwrap().is_empty());
}

#[test]
fn test_schema_encoding_decoding() {
    // Schema: {"type": "object", "properties": {"a": {"type": "integer"}, "b": {"type": "string"}}, "required": ["a", "b"]}
    let mut properties = BTreeMap::new();
    properties.insert("a".to_string(), Schema {
        instance_type: Some(SingleOrVec::Single(InstanceType::Integer)),
        properties: None,
        required: None,
    });
    properties.insert("b".to_string(), Schema {
        instance_type: Some(SingleOrVec::Single(InstanceType::String)),
        properties: None,
        required: None,
    });
    
    let mut required = BTreeSet::new();
    required.insert("a".to_string());
    required.insert("b".to_string());

    let schema = Schema {
        instance_type: Some(SingleOrVec::Single(InstanceType::Object)),
        properties: Some(properties),
        required: Some(required),
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
    properties.insert("a".to_string(), Schema {
        instance_type: Some(SingleOrVec::Single(InstanceType::Integer)),
        properties: None,
        required: None,
    });
    
    let mut required = BTreeSet::new();
    required.insert("a".to_string());

    let schema = Schema {
        instance_type: Some(SingleOrVec::Single(InstanceType::Object)),
        properties: Some(properties),
        required: Some(required),
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