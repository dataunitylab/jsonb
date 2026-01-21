use jsonb_schema::schema::from_serde_json;
use serde_json::json;

#[test]
fn test_valid_dollar_schema() {
    let json = json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "type": "string"
    });
    let result = from_serde_json(&json);
    assert!(result.is_ok());
}

#[test]
fn test_invalid_dollar_schema_value() {
    let json = json!({
        "$schema": "http://json-schema.org/draft-07/schema",
        "type": "string"
    });
    let result = from_serde_json(&json);
    assert!(result.is_err());
    assert_eq!(
        result.unwrap_err().to_string(),
        "Unsupported $schema: http://json-schema.org/draft-07/schema"
    );
}

#[test]
fn test_invalid_dollar_schema_type() {
    let json = json!({
        "$schema": 123,
        "type": "string"
    });
    let result = from_serde_json(&json);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().to_string(), "$schema must be a string");
}
