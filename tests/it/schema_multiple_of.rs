use jsonb_schema::schema::{decode, encode, InstanceType, Schema, SingleOrVec};
use jsonb_schema::Number;
use jsonb_schema::Value;

#[test]
fn test_multiple_of_optimization() {
    let multiple = 3;
    // Schema: {"type": "integer", "multipleOf": 3}
    let schema = Schema {
        instance_type: Some(SingleOrVec::Single(InstanceType::Integer)),
        properties: None,
        required: None,
        minimum: None,
        maximum: None,
        multiple_of: Some(multiple),
        ..Schema::default()
    };

    let val = 9;
    let value = Value::Number(Number::Int64(val));

    let mut buf = Vec::new();
    encode(&value, &schema, &mut buf);

    // 9 / 3 = 3. Encoded as TAG_OPTIMIZED_NUMBER (0xFF) + uvarint(3) -> 0xFF, 0x03.
    assert_eq!(buf.len(), 2);
    assert_eq!(buf[0], 0xFF);
    assert_eq!(buf[1], 0x03);

    let decoded = decode(&buf, &schema);
    if let Value::Number(n) = decoded {
        assert_eq!(n.as_i64(), Some(val));
    } else {
        panic!("Expected Number");
    }
}

#[test]
fn test_multiple_of_with_aligned_minimum() {
    let multiple = 5;
    let min = 10;
    // Schema: {"type": "integer", "multipleOf": 5, "minimum": 10}
    let schema = Schema {
        instance_type: Some(SingleOrVec::Single(InstanceType::Integer)),
        properties: None,
        required: None,
        minimum: Some(min),
        maximum: None,
        multiple_of: Some(multiple),
        ..Schema::default()
    };

    // val = 20.
    // min is multiple of 5 (10%5==0).
    // Optimization: (val - min) / mul = (20 - 10) / 5 = 2.
    let val = 20;
    let value = Value::Number(Number::Int64(val));

    let mut buf = Vec::new();
    encode(&value, &schema, &mut buf);

    // Encoded as TAG_OPTIMIZED_NUMBER (0xFF) + uvarint(2) -> 0xFF, 0x02.
    assert_eq!(buf.len(), 2);
    assert_eq!(buf[0], 0xFF);
    assert_eq!(buf[1], 0x02);

    let decoded = decode(&buf, &schema);
    if let Value::Number(n) = decoded {
        assert_eq!(n.as_i64(), Some(val));
    } else {
        panic!("Expected Number");
    }
}

#[test]
fn test_multiple_of_with_unaligned_minimum() {
    let multiple = 5;
    let min = 11; // Not multiple of 5
                  // Schema: {"type": "integer", "multipleOf": 5, "minimum": 11}
    let schema = Schema {
        instance_type: Some(SingleOrVec::Single(InstanceType::Integer)),
        properties: None,
        required: None,
        minimum: Some(min),
        maximum: None,
        multiple_of: Some(multiple),
        ..Schema::default()
    };

    // val = 20.
    // min is NOT multiple of 5.
    // Optimization: val / mul = 20 / 5 = 4.
    let val = 20;
    let value = Value::Number(Number::Int64(val));

    let mut buf = Vec::new();
    encode(&value, &schema, &mut buf);

    // Encoded as TAG_OPTIMIZED_NUMBER (0xFF) + uvarint(4) -> 0xFF, 0x04.
    assert_eq!(buf.len(), 2);
    assert_eq!(buf[0], 0xFF);
    assert_eq!(buf[1], 0x04);

    let decoded = decode(&buf, &schema);
    if let Value::Number(n) = decoded {
        assert_eq!(n.as_i64(), Some(val));
    } else {
        panic!("Expected Number");
    }
}

#[test]
fn test_multiple_of_union() {
    let multiple = 2;
    // Schema: {"type": ["integer", "string"], "multipleOf": 2}
    let schema = Schema {
        instance_type: Some(SingleOrVec::Vec(vec![
            InstanceType::Integer,
            InstanceType::String,
        ])),
        properties: None,
        required: None,
        minimum: None,
        maximum: None,
        multiple_of: Some(multiple),
        ..Schema::default()
    };

    // val = 8.
    // Optimization: 8 / 2 = 4.
    // Union encoding: TAG_NUMBER + TAG_OPTIMIZED_NUMBER + uvarint(4).
    let val = 8;
    let value = Value::Number(Number::Int64(val));

    let mut buf = Vec::new();
    encode(&value, &schema, &mut buf);

    // TAG_NUMBER = 0x03.
    // TAG_OPTIMIZED_NUMBER = 0xFF.
    // uvarint(4) = 0x04.
    assert_eq!(buf.len(), 3);
    assert_eq!(buf[0], 0x03);
    assert_eq!(buf[1], 0xFF);
    assert_eq!(buf[2], 0x04);

    let decoded = decode(&buf, &schema);
    if let Value::Number(n) = decoded {
        assert_eq!(n.as_i64(), Some(val));
    } else {
        panic!("Expected Number");
    }
}

#[test]
fn test_multiple_of_float() {
    let multiple = 2;
    // Schema: {"type": "number", "multipleOf": 2}
    let schema = Schema {
        instance_type: Some(SingleOrVec::Single(InstanceType::Number)),
        properties: None,
        required: None,
        minimum: None,
        maximum: None,
        multiple_of: Some(multiple),
        ..Schema::default()
    };

    let val = 4.0;
    let value = Value::Number(Number::Float64(val));

    let mut buf = Vec::new();
    encode(&value, &schema, &mut buf);

    // 4.0 / 2 = 2.0 (integer).
    // Encoded as TAG_OPTIMIZED_NUMBER (0xFF) + uvarint(2) -> 0xFF, 0x02.
    assert_eq!(buf.len(), 2);
    assert_eq!(buf[0], 0xFF);
    assert_eq!(buf[1], 0x02);

    let decoded = decode(&buf, &schema);
    if let Value::Number(n) = decoded {
        assert_eq!(n.as_f64(), val);
    } else {
        panic!("Expected Number");
    }
}

#[test]
fn test_multiple_of_float_unoptimized() {
    let multiple = 2;
    // Schema: {"type": "number", "multipleOf": 2}
    let schema = Schema {
        instance_type: Some(SingleOrVec::Single(InstanceType::Number)),
        properties: None,
        required: None,
        minimum: None,
        maximum: None,
        multiple_of: Some(multiple),
        ..Schema::default()
    };

    let val = 4.5;
    let value = Value::Number(Number::Float64(val));

    let mut buf = Vec::new();
    encode(&value, &schema, &mut buf);

    // 4.5 / 2 = 2.25 (NOT integer).
    // Should fallback to standard encoding.
    assert!(buf.len() > 2);
    assert_ne!(buf[0], 0xFF);

    let decoded = decode(&buf, &schema);
    if let Value::Number(n) = decoded {
        assert_eq!(n.as_f64(), val);
    } else {
        panic!("Expected Number");
    }
}

#[test]
fn test_multiple_of_decimal() {
    let multiple = 2;
    // Schema: {"type": "number", "multipleOf": 2}
    let schema = Schema {
        instance_type: Some(SingleOrVec::Single(InstanceType::Number)),
        properties: None,
        required: None,
        minimum: None,
        maximum: None,
        multiple_of: Some(multiple),
        ..Schema::default()
    };

    // 4.0 as Decimal64 with scale 1 -> value 40, scale 1.
    // as_i128() returns None for scale > 0.
    let val = jsonb_schema::Decimal64 {
        scale: 1,
        value: 40,
    };
    let value = Value::Number(Number::Decimal64(val.clone()));

    let mut buf = Vec::new();
    encode(&value, &schema, &mut buf);

    assert!(buf.len() > 1);

    let decoded = decode(&buf, &schema);
    if let Value::Number(n) = decoded {
        assert_eq!(n.as_f64(), 4.0);
    } else {
        panic!("Expected Number");
    }
}
