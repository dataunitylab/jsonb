use jsonb_schema::jsonpath::{parse_json_path, Selector};
use jsonb_schema::schema::{encode, from_serde_json};
use jsonb_schema::{Number, RawJsonb, Value};
use serde_json::json;

#[test]
fn test_selector_schema_string_min_length() {
    let schema_json = json!({
        "type": "string",
        "minLength": 5
    });
    let schema = from_serde_json(&schema_json).unwrap();
    let val = Value::String("hello world".into());
    let mut buf = Vec::new();
    encode(&val, &schema, &mut buf);
    let raw = RawJsonb::new(&buf);

    let mut selector = Selector::new(raw).with_schema(&schema);

    // Path: $
    let path = parse_json_path("$".as_bytes()).unwrap();
    let res = selector.select_value(&path).unwrap();
    assert_eq!(res.unwrap().to_string(), "\"hello world\"");

    // Predicate: $ == "hello world"
    let path = parse_json_path("$ == \"hello world\"".as_bytes()).unwrap();
    let res = selector.predicate_match(&path).unwrap();
    assert_eq!(res, Some(true));

    // Predicate: $ == "hello" (should be false)
    let path = parse_json_path("$ == \"hello\"".as_bytes()).unwrap();
    let res = selector.predicate_match(&path).unwrap();
    assert_eq!(res, Some(false));
}

#[test]
fn test_selector_schema_object_keys() {
    let schema_json = json!({
        "type": "object",
        "properties": {
            "a": { "type": "string" },
            "b": { "type": "number" }
        },
        "required": ["a"]
    });
    let schema = from_serde_json(&schema_json).unwrap();

    let val = Value::Object(
        vec![
            ("a".to_string(), Value::String("val_a".into())),
            ("b".to_string(), Value::Number(Number::Int64(10))),
        ]
        .into_iter()
        .collect(),
    );

    let mut buf = Vec::new();
    encode(&val, &schema, &mut buf);
    let raw = RawJsonb::new(&buf);

    let mut selector = Selector::new(raw).with_schema(&schema);

    // Select .a (required, key name NOT in encoding)
    let path = parse_json_path("$.a".as_bytes()).unwrap();
    let res = selector.select_value(&path).unwrap();
    assert_eq!(res.unwrap().to_string(), "\"val_a\"");

    // Select .b (not required, key name IN encoding)
    let path = parse_json_path("$.b".as_bytes()).unwrap();
    let res = selector.select_value(&path).unwrap();
    assert_eq!(res.unwrap().to_string(), "10");
}

#[test]
fn test_selector_schema_multiple_of_optimization() {
    let schema_json = json!({
        "type": "number",
        "multipleOf": 5
    });
    let schema = from_serde_json(&schema_json).unwrap();

    let val = Value::Number(Number::Int64(10)); // Encoded as 2
    let mut buf = Vec::new();
    encode(&val, &schema, &mut buf);
    let raw = RawJsonb::new(&buf);

    let mut selector = Selector::new(raw).with_schema(&schema);

    // Match 10
    let path = parse_json_path("$ == 10".as_bytes()).unwrap();
    let res = selector.predicate_match(&path).unwrap();
    assert_eq!(res, Some(true));

    // Match 12 (invalid multiple)
    // Should return false (checked via validity check)
    let path = parse_json_path("$ == 12".as_bytes()).unwrap();
    let res = selector.predicate_match(&path).unwrap();
    assert_eq!(res, Some(false));

    // Match 15 (valid multiple, but != 10)
    // Encoded 3. 2 != 3.
    let path = parse_json_path("$ == 15".as_bytes()).unwrap();
    let res = selector.predicate_match(&path).unwrap();
    assert_eq!(res, Some(false));
}

#[test]
fn test_selector_schema_array_indices() {
    let schema_json = json!({
        "type": "array",
        "items": { "type": "number" },
        "minItems": 3
    });
    let schema = from_serde_json(&schema_json).unwrap();

    let val = Value::Array(vec![
        Value::Number(Number::Int64(1)),
        Value::Number(Number::Int64(2)),
        Value::Number(Number::Int64(3)),
        Value::Number(Number::Int64(4)),
    ]);
    // Length 4. minItems 3. Encoded length 1.

    let mut buf = Vec::new();
    encode(&val, &schema, &mut buf);
    let raw = RawJsonb::new(&buf);

    let mut selector = Selector::new(raw).with_schema(&schema);

    // Select index 0
    let path = parse_json_path("$[0]".as_bytes()).unwrap();
    let res = selector.select_value(&path).unwrap();
    assert_eq!(res.unwrap().to_string(), "1");

    // Select index 3
    let path = parse_json_path("$[3]".as_bytes()).unwrap();
    let res = selector.select_value(&path).unwrap();
    assert_eq!(res.unwrap().to_string(), "4");

    // Select index 4 (out of bounds)
    let path = parse_json_path("$[4]".as_bytes()).unwrap();
    let res = selector.select_value(&path).unwrap();
    assert!(res.is_none());
}
