use jsonb_schema::schema::{from_serde_json, InstanceType, Schema, SingleOrVec};
use serde_json::json;

#[test]
fn test_from_json_basic() {
    let json = json!({
        "type": "string"
    });
    let schema = from_serde_json(&json).unwrap();
    assert_eq!(
        schema.instance_type,
        Some(SingleOrVec::Single(InstanceType::String))
    );
}

#[test]
fn test_from_json_boolean() {
    let true_schema = from_serde_json(&json!(true)).unwrap();
    assert_eq!(
        true_schema,
        Schema {
            instance_type: None,
            properties: None,
            required: None,
            minimum: None,
            maximum: None,
            multiple_of: None,
            prefix_items: None,
            items: None,
            enum_values: None,
            const_value: None,
        }
    );

    let false_schema = from_serde_json(&json!(false)).unwrap();
    assert!(false_schema.enum_values.is_some());
    assert!(false_schema.enum_values.as_ref().unwrap().is_empty());
}

#[test]
fn test_from_json_unsupported_keyword() {
    let json = json!({
        "type": "string",
        "description": "This is unsupported"
    });
    let result = from_serde_json(&json);
    assert!(result.is_err());
    assert_eq!(
        result.unwrap_err().to_string(),
        "Unsupported keyword: description"
    );
}

#[test]
fn test_from_json_nested() {
    let json = json!({
        "type": "object",
        "properties": {
            "foo": { "type": "integer" }
        },
        "required": ["foo"]
    });
    let schema = from_serde_json(&json).unwrap();

    match schema.instance_type {
        Some(SingleOrVec::Single(InstanceType::Object)) => {}
        _ => panic!("Expected object type"),
    }

    let props = schema.properties.unwrap();
    assert!(props.contains_key("foo"));
    let foo_schema = props.get("foo").unwrap();
    assert_eq!(
        foo_schema.instance_type,
        Some(SingleOrVec::Single(InstanceType::Integer))
    );

    let req = schema.required.unwrap();
    assert!(req.contains("foo"));
}

#[test]
fn test_from_json_arrays() {
    let json = json!({
        "type": "array",
        "prefixItems": [{"type": "string"}],
        "items": {"type": "integer"}
    });
    let schema = from_serde_json(&json).unwrap();

    assert!(schema.prefix_items.is_some());
    assert_eq!(schema.prefix_items.as_ref().unwrap().len(), 1);

    assert!(schema.items.is_some());
    assert_eq!(
        schema.items.as_ref().unwrap().instance_type,
        Some(SingleOrVec::Single(InstanceType::Integer))
    );
}

#[test]
fn test_from_json_numeric_constraints() {
    let json = json!({
        "type": "integer",
        "minimum": 10,
        "maximum": 100,
        "multipleOf": 5
    });
    let schema = from_serde_json(&json).unwrap();
    assert_eq!(schema.minimum, Some(10));
    assert_eq!(schema.maximum, Some(100));
    assert_eq!(schema.multiple_of, Some(5));
}

#[test]
fn test_from_json_numeric_float_error() {
    let json = json!({
        "type": "integer",
        "minimum": 10.5
    });
    let result = from_serde_json(&json);
    assert!(result.is_err());
    // Expect error about integer requirement
}
