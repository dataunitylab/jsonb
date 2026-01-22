use jsonb_schema::schema::{decode, encode, from_serde_json};
use jsonb_schema::Value;
use serde_json::json;

#[test]
fn test_array_min_items_optimization() {
    let schema_json = json!({
        "type": "array",
        "minItems": 3,
        "items": { "type": "string" }
    });
    let schema = from_serde_json(&schema_json).unwrap();
    assert_eq!(schema.min_items, Some(3));

    let _json_val = json!(["a", "b", "c", "d", "e"]); // Length 5
                                                      // jsonb Value
    let val = Value::Array(vec![
        Value::String("a".into()),
        Value::String("b".into()),
        Value::String("c".into()),
        Value::String("d".into()),
        Value::String("e".into()),
    ]);

    let mut buf = Vec::new();
    encode(&val, &schema, &mut buf);

    // Verify encoded length is 5 - 3 = 2
    // Encoded format:
    // array tag (uvarint? No, this is typed array encoding)
    // Typed array encoding in `encode_typed_value`:
    // (InstanceType::Array...) -> encode_array -> write_uvarint(len)
    //
    // However, wait. `encode_typed_value` doesn't write a type tag if the schema says it's an array?
    // Let's check `encode_typed_value` implementation again.
    // It doesn't write a tag if it knows it is `InstanceType::Array`.
    // It just calls `encode_array`.
    // `encode_array` writes length.

    // So `buf` should start with uvarint(2).
    // 2 in uvarint is 0x02.
    assert_eq!(buf[0], 0x02);

    let decoded = decode(&buf, &schema);
    assert_eq!(decoded, val);
}

#[test]
fn test_array_min_items_exact_length() {
    let schema_json = json!({
        "type": "array",
        "minItems": 3,
        "items": { "type": "string" }
    });
    let schema = from_serde_json(&schema_json).unwrap();

    let val = Value::Array(vec![
        Value::String("a".into()),
        Value::String("b".into()),
        Value::String("c".into()),
    ]); // Length 3

    let mut buf = Vec::new();
    encode(&val, &schema, &mut buf);

    // Encoded length = 3 - 3 = 0
    assert_eq!(buf[0], 0x00);

    let decoded = decode(&buf, &schema);
    assert_eq!(decoded, val);
}

#[test]
fn test_array_min_items_less_than_min() {
    // This case technically violates schema, but let's see how encoding behaves.
    // Logic: if (len >= min) write len - min else write len.
    let schema_json = json!({
        "type": "array",
        "minItems": 5,
        "items": { "type": "string" }
    });
    let schema = from_serde_json(&schema_json).unwrap();

    let val = Value::Array(vec![
        Value::String("a".into()),
        Value::String("b".into()),
        Value::String("c".into()),
    ]); // Length 3

    let mut buf = Vec::new();
    encode(&val, &schema, &mut buf);

    // 3 < 5, so it writes 3.
    assert_eq!(buf[0], 0x03);

    // Decode: read 3, add 5 -> length 8?
    // This will result in reading past end of buffer or reading garbage if we try to read 8 items but only 3 are there.
    // But since the loop `for i in 0..len` reads items...
    // The decoder expects 8 items. It will try to read 8 items.
    // Since buffer only has 3 items, it will fail or panic or return garbage/error.
    // This confirms that "minItems" optimization requires valid data.
    // I won't test roundtrip here because it's undefined behavior for invalid data under this optimization.
}

#[test]
fn test_array_max_items_parsed() {
    let schema_json = json!({
        "type": "array",
        "maxItems": 10
    });
    let schema = from_serde_json(&schema_json).unwrap();
    assert_eq!(schema.max_items, Some(10));
}
