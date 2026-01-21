use jsonb_schema::schema::{from_serde_json, InstanceType, SingleOrVec};
use serde_json::json;

#[test]
fn test_from_json_ignored_keywords() {
    let json = json!({
        "type": "string",
        "title": "A string",
        "description": "Just a string",
        "examples": ["example1", "example2"]
    });
    let schema = from_serde_json(&json).expect("Should not fail on ignored keywords");
    assert_eq!(
        schema.instance_type,
        Some(SingleOrVec::Single(InstanceType::String))
    );
}

#[test]
fn test_from_json_still_errors_on_unknown() {
    let json = json!({
        "type": "string",
        "unknownKeyword": "should fail"
    });
    let result = from_serde_json(&json);
    assert!(result.is_err());
    assert_eq!(
        result.unwrap_err().to_string(),
        "Unsupported keyword: unknownKeyword"
    );
}
