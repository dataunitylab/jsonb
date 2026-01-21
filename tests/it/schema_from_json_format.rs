use jsonb_schema::schema::from_serde_json;
use serde_json::json;

#[test]
fn test_format_date() {
    let json = json!({
        "type": "string",
        "format": "date"
    });
    let schema = from_serde_json(&json).unwrap();
    assert_eq!(schema.format, Some("date".to_string()));
}

#[test]
fn test_invalid_format_type() {
    let json = json!({
        "type": "string",
        "format": 123
    });
    let result = from_serde_json(&json);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().to_string(), "format must be a string");
}
