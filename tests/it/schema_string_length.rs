use jsonb_schema::schema::{decode, encode, from_serde_json};
use jsonb_schema::Value;
use serde_json::json;

#[test]
fn test_string_min_length_optimization() {
    let schema_json = json!({
        "type": "string",
        "minLength": 5
    });
    let schema = from_serde_json(&schema_json).unwrap();
    assert_eq!(schema.min_length, Some(5));

    let val = Value::String("hello world".into()); // Length 11

    let mut buf = Vec::new();
    encode(&val, &schema, &mut buf);

    // Encoded length = 11 - 5 = 6
    // 6 in uvarint is 0x06.
    assert_eq!(buf[0], 0x06);

    // Check remaining bytes "hello world"
    let s = std::str::from_utf8(&buf[1..]).unwrap();
    assert_eq!(s, "hello world");

    let decoded = decode(&buf, &schema);
    assert_eq!(decoded, val);
}

#[test]
fn test_string_min_length_exact() {
    let schema_json = json!({
        "type": "string",
        "minLength": 5
    });
    let schema = from_serde_json(&schema_json).unwrap();

    let val = Value::String("hello".into()); // Length 5

    let mut buf = Vec::new();
    encode(&val, &schema, &mut buf);

    // Encoded length = 5 - 5 = 0
    assert_eq!(buf[0], 0x00);

    let s = std::str::from_utf8(&buf[1..]).unwrap();
    assert_eq!(s, "hello");

    let decoded = decode(&buf, &schema);
    assert_eq!(decoded, val);
}

#[test]
fn test_string_max_length_parsed() {
    let schema_json = json!({
        "type": "string",
        "maxLength": 10
    });
    let schema = from_serde_json(&schema_json).unwrap();
    assert_eq!(schema.max_length, Some(10));
}

#[test]
fn test_string_min_length_with_pattern_ignored() {
    // If pattern optimization applies, minLength subtraction should NOT apply because encoding path differs.
    // Wait, my implementation applies minLength ONLY if pattern optimization returns false.
    // So I need a case where pattern optimization works.
    let schema_json = json!({
        "type": "string",
        "pattern": "^prefix",
        "minLength": 6
    });
    let schema = from_serde_json(&schema_json).unwrap();

    // "prefixA" -> length 7. minLength 6.
    // If pattern optimization works, it writes TAG_PATTERN_COMPRESSED + len("A") + "A".
    // It does NOT write length 7-6=1.
    // len("A") is 1.
    // If it used minLength optimization on top of pattern? No, my code doesn't do that.

    let val = Value::String("prefixA".into());
    let mut buf = Vec::new();
    encode(&val, &schema, &mut buf);

    // Check tag
    assert_eq!(buf[0], 0x07); // TAG_PATTERN_COMPRESSED

    let decoded = decode(&buf, &schema);
    assert_eq!(decoded, val);
}
