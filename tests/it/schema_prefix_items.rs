use jsonb_schema::schema::{decode, encode, InstanceType, Schema, SingleOrVec};
use jsonb_schema::Number;
use jsonb_schema::Value;
use std::borrow::Cow;

#[test]
fn test_prefix_items_optimization() {
    // Schema: {
    //   "type": "array",
    //   "prefixItems": [
    //     {"type": "integer"},
    //     {"type": "string"}
    //   ]
    // }
    let prefix_items = vec![
        Schema {
            instance_type: Some(SingleOrVec::Single(InstanceType::Integer)),
            properties: None,
            required: None,
            minimum: None,
            maximum: None,
            multiple_of: None,
            prefix_items: None,
            items: None,
            enum_values: None,
            const_value: None,
            format: None,
        },
        Schema {
            instance_type: Some(SingleOrVec::Single(InstanceType::String)),
            properties: None,
            required: None,
            minimum: None,
            maximum: None,
            multiple_of: None,
            prefix_items: None,
            items: None,
            enum_values: None,
            const_value: None,
            format: None,
        },
    ];

    let schema = Schema {
        instance_type: Some(SingleOrVec::Single(InstanceType::Array)),
        properties: None,
        required: None,
        minimum: None,
        maximum: None,
        multiple_of: None,
        prefix_items: Some(prefix_items),
        items: None,
        enum_values: None,
        const_value: None,
        format: None,
    };

    // Value: [10, "hello", true]
    // 10 -> matches first prefixItem (Integer), no tag.
    // "hello" -> matches second prefixItem (String), no tag.
    // true -> no prefixItem, no items schema -> fallback to untyped (should have tag).
    let val_arr = vec![
        Value::Number(Number::Int64(10)),
        Value::String(Cow::Borrowed("hello")),
        Value::Bool(true),
    ];
    let value = Value::Array(val_arr);

    let mut buf = Vec::new();
    encode(&value, &schema, &mut buf);

    // Encoding Analysis:
    // Array Length: uvarint(3) -> 1 byte (0x03)
    // Item 0 (10): Integer typed.
    //   compact_encode(10) -> NUMBER_INT(0x40) + i8(10) = 2 bytes.
    //   Typed Number writes: length(uvarint) + bytes.
    //   Wait, Number::compact_encode writes the tag internal to the number format?
    //   No, Number::compact_encode writes internal representation.
    //   Typed Number encoding:
    //     write_uvarint(buf, temp.len());
    //     buf.extend(temp);
    //   So for 10: 2 bytes temp. len=2.
    //   Total: 1 byte (len) + 2 bytes (data) = 3 bytes.
    //   Standard Untyped would be: TAG_NUMBER + 1 byte (len) + 2 bytes (data) = 4 bytes.
    //   Optimization: saves 1 byte (TAG).

    // Item 1 ("hello"): String typed.
    //   len=5.
    //   Typed: uvarint(5) + "hello" = 1 + 5 = 6 bytes.
    //   Untyped: TAG_STRING + uvarint(5) + "hello" = 1 + 1 + 5 = 7 bytes.
    //   Optimization: saves 1 byte (TAG).

    // Item 2 (true): Untyped (no schema).
    //   Untyped Bool: TAG_BOOL_TRUE (1 byte).

    // Total Expected:
    // Array Len (1) + Item 0 (3) + Item 1 (6) + Item 2 (1) = 11 bytes.

    // If untyped:
    // TAG_ARRAY + len + Item0(4) + Item1(7) + Item2(1) = 1 + 1 + 4 + 7 + 1 = 14 bytes.

    // Since we are encoding a Typed Array (schema has type: array), we start inside encode_typed_value.
    // It writes array length (1 byte).
    // Then items...

    assert_eq!(buf.len(), 11);

    let decoded = decode(&buf, &schema);
    assert_eq!(value, decoded);
}

#[test]
fn test_items_optimization() {
    // Schema: {
    //   "type": "array",
    //   "items": {"type": "integer"}
    // }
    let items_schema = Schema {
        instance_type: Some(SingleOrVec::Single(InstanceType::Integer)),
        properties: None,
        required: None,
        minimum: None,
        maximum: None,
        multiple_of: None,
        prefix_items: None,
        items: None,
        enum_values: None,
        const_value: None,
        format: None,
    };

    let schema = Schema {
        instance_type: Some(SingleOrVec::Single(InstanceType::Array)),
        properties: None,
        required: None,
        minimum: None,
        maximum: None,
        multiple_of: None,
        prefix_items: None,
        items: Some(Box::new(items_schema)),
        enum_values: None,
        const_value: None,
        format: None,
    };

    // Value: [10, 20]
    let val_arr = vec![
        Value::Number(Number::Int64(10)),
        Value::Number(Number::Int64(20)),
    ];
    let value = Value::Array(val_arr);

    let mut buf = Vec::new();
    encode(&value, &schema, &mut buf);

    // Item 0 (10): Typed Integer. 3 bytes.
    // Item 1 (20): Typed Integer. 3 bytes.
    // Array Len: 1 byte.
    // Total: 7 bytes.
    assert_eq!(buf.len(), 7);

    let decoded = decode(&buf, &schema);
    assert_eq!(value, decoded);
}

#[test]
fn test_prefix_items_and_items_optimization() {
    // Schema: {
    //   "type": "array",
    //   "prefixItems": [{"type": "string"}],
    //   "items": {"type": "integer"}
    // }
    let prefix_items = vec![Schema {
        instance_type: Some(SingleOrVec::Single(InstanceType::String)),
        properties: None,
        required: None,
        minimum: None,
        maximum: None,
        multiple_of: None,
        prefix_items: None,
        items: None,
        enum_values: None,
        const_value: None,
        format: None,
    }];

    let items_schema = Schema {
        instance_type: Some(SingleOrVec::Single(InstanceType::Integer)),
        properties: None,
        required: None,
        minimum: None,
        maximum: None,
        multiple_of: None,
        prefix_items: None,
        items: None,
        enum_values: None,
        const_value: None,
        format: None,
    };

    let schema = Schema {
        instance_type: Some(SingleOrVec::Single(InstanceType::Array)),
        properties: None,
        required: None,
        minimum: None,
        maximum: None,
        multiple_of: None,
        prefix_items: Some(prefix_items),
        items: Some(Box::new(items_schema)),
        enum_values: None,
        const_value: None,
        format: None,
    };

    // Value: ["start", 10, 20]
    let val_arr = vec![
        Value::String(Cow::Borrowed("start")),
        Value::Number(Number::Int64(10)),
        Value::Number(Number::Int64(20)),
    ];
    let value = Value::Array(val_arr);

    let mut buf = Vec::new();
    encode(&value, &schema, &mut buf);

    // Item 0 ("start"): Typed String. len=5. uvarint(5) + "start" = 6 bytes.
    // Item 1 (10): Typed Integer. 3 bytes.
    // Item 2 (20): Typed Integer. 3 bytes.
    // Array Len: 1 byte.
    // Total: 13 bytes.
    assert_eq!(buf.len(), 13);

    let decoded = decode(&buf, &schema);
    assert_eq!(value, decoded);
}
