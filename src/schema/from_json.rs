use crate::error::Result;
use crate::schema::{InstanceType, Schema, SingleOrVec};
use crate::Error;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

pub fn from_serde_json(json: &Value) -> Result<Schema> {
    match json {
        Value::Bool(b) => {
            if *b {
                // Schema: true (always valid) -> Empty Schema
                Ok(Schema {
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
                    format: None,
                })
            } else {
                // Schema: false (always invalid) -> enum: []
                Ok(Schema {
                    instance_type: None,
                    properties: None,
                    required: None,
                    minimum: None,
                    maximum: None,
                    multiple_of: None,
                    prefix_items: None,
                    items: None,
                    enum_values: Some(vec![]),
                    const_value: None,
                    format: None,
                })
            }
        }
        Value::Object(map) => {
            let mut schema = Schema {
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
                format: None,
            };

            for (k, v) in map {
                match k.as_str() {
                    "type" => {
                        schema.instance_type = Some(parse_type(v)?);
                    }
                    "format" => {
                        if let Value::String(s) = v {
                            if s == "date"
                                || s == "time"
                                || s == "date-time"
                                || s == "ipv4"
                                || s == "ipv6"
                            {
                                schema.format = Some(s.clone());
                            } else {
                                return Err(Error::Message(format!("Unsupported format: {}", s)));
                            }
                        } else {
                            return Err(Error::Message("format must be a string".to_string()));
                        }
                    }
                    "properties" => {
                        if let Value::Object(props) = v {
                            let mut p_map = BTreeMap::new();
                            for (pk, pv) in props {
                                p_map.insert(pk.clone(), from_serde_json(pv)?);
                            }
                            schema.properties = Some(p_map);
                        } else {
                            return Err(Error::Message("properties must be an object".to_string()));
                        }
                    }
                    "required" => {
                        if let Value::Array(req) = v {
                            let mut r_set = BTreeSet::new();
                            for r in req {
                                if let Value::String(s) = r {
                                    r_set.insert(s.clone());
                                } else {
                                    return Err(Error::Message(
                                        "required items must be strings".to_string(),
                                    ));
                                }
                            }
                            schema.required = Some(r_set);
                        } else {
                            return Err(Error::Message("required must be an array".to_string()));
                        }
                    }
                    "minimum" => {
                        schema.minimum = Some(parse_i128(v, "minimum")?);
                    }
                    "maximum" => {
                        schema.maximum = Some(parse_i128(v, "maximum")?);
                    }
                    "multipleOf" => {
                        schema.multiple_of = Some(parse_i128(v, "multipleOf")?);
                    }
                    "prefixItems" => {
                        if let Value::Array(items) = v {
                            let mut schemas = Vec::new();
                            for item in items {
                                schemas.push(from_serde_json(item)?);
                            }
                            schema.prefix_items = Some(schemas);
                        } else {
                            return Err(Error::Message("prefixItems must be an array".to_string()));
                        }
                    }
                    "items" => {
                        schema.items = Some(Box::new(from_serde_json(v)?));
                    }
                    "enum" => {
                        if let Value::Array(vals) = v {
                            schema.enum_values = Some(vals.clone());
                        } else {
                            return Err(Error::Message("enum must be an array".to_string()));
                        }
                    }
                    "const" => {
                        schema.const_value = Some(v.clone());
                    }
                    "title" | "description" | "examples" => {
                        // Ignore metadata keywords
                    }
                    "$schema" => {
                        if let Value::String(s) = v {
                            if s != "https://json-schema.org/draft/2020-12/schema" {
                                return Err(Error::Message(format!("Unsupported $schema: {}", s)));
                            }
                        } else {
                            return Err(Error::Message("$schema must be a string".to_string()));
                        }
                    }
                    // Ignored keywords (metadata) could be skipped here if desired,
                    // but prompt implies strictly unsupported.
                    // For safety against strict requirements, I will treat everything else as unsupported.
                    _ => {
                        return Err(Error::Message(format!("Unsupported keyword: {}", k)));
                    }
                }
            }
            Ok(schema)
        }
        _ => Err(Error::Message(
            "Schema must be an object or boolean".to_string(),
        )),
    }
}

fn parse_type(v: &Value) -> Result<SingleOrVec<InstanceType>> {
    match v {
        Value::String(s) => Ok(SingleOrVec::Single(parse_instance_type(s)?)),
        Value::Array(arr) => {
            let mut types = Vec::new();
            for item in arr {
                if let Value::String(s) = item {
                    types.push(parse_instance_type(s)?);
                } else {
                    return Err(Error::Message(
                        "type array must contain strings".to_string(),
                    ));
                }
            }
            Ok(SingleOrVec::Vec(types))
        }
        _ => Err(Error::Message(
            "type must be a string or array of strings".to_string(),
        )),
    }
}

fn parse_instance_type(s: &str) -> Result<InstanceType> {
    match s {
        "null" => Ok(InstanceType::Null),
        "boolean" => Ok(InstanceType::Boolean),
        "object" => Ok(InstanceType::Object),
        "array" => Ok(InstanceType::Array),
        "number" => Ok(InstanceType::Number),
        "string" => Ok(InstanceType::String),
        "integer" => Ok(InstanceType::Integer),
        _ => Err(Error::Message(format!("Unknown type: {}", s))),
    }
}

fn parse_i128(v: &Value, field: &str) -> Result<i128> {
    match v {
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(i as i128)
            } else if let Some(u) = n.as_u64() {
                Ok(u as i128)
            } else {
                // Try float? Schema struct uses i128.
                // If float is integer, maybe acceptable?
                if let Some(f) = n.as_f64() {
                    if f.fract() == 0.0 {
                        // Check bounds?
                        // Simple cast for now, strictly it might overflow i128 but f64 precision is lower anyway.
                        Ok(f as i128)
                    } else {
                        Err(Error::Message(format!("{} must be an integer", field)))
                    }
                } else {
                    Err(Error::Message(format!("{} must be a valid number", field)))
                }
            }
        }
        _ => Err(Error::Message(format!("{} must be a number", field))),
    }
}
