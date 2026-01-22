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
                Ok(Schema::default())
            } else {
                // Schema: false (always invalid) -> enum: []
                Ok(Schema {
                    enum_values: Some(vec![]),
                    ..Schema::default()
                })
            }
        }
        Value::Object(map) => {
            let mut schema = Schema::default();

            for (k, v) in map {
                match k.as_str() {
                    "type" => {
                        schema.instance_type = Some(parse_type(v)?);
                    }
                    "format" => {
                        if let Value::String(s) = v {
                            schema.format = Some(s.clone());
                        } else {
                            return Err(Error::Message("format must be a string".to_string()));
                        }
                    }
                    "pattern" => {
                        if let Value::String(s) = v {
                            schema.pattern = Some(s.clone());
                            let (prefix, suffix) = extract_pattern_optimization(s);
                            schema.pattern_prefix = prefix;
                            schema.pattern_suffix = suffix;
                        } else {
                            return Err(Error::Message("pattern must be a string".to_string()));
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
                    "minItems" => {
                        schema.min_items = Some(parse_u64(v, "minItems")?);
                    }
                    "maxItems" => {
                        schema.max_items = Some(parse_u64(v, "maxItems")?);
                    }
                    "minLength" => {
                        schema.min_length = Some(parse_u64(v, "minLength")?);
                    }
                    "maxLength" => {
                        schema.max_length = Some(parse_u64(v, "maxLength")?);
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

fn parse_u64(v: &Value, field: &str) -> Result<u64> {
    match v {
        Value::Number(n) => {
            if let Some(u) = n.as_u64() {
                Ok(u)
            } else {
                Err(Error::Message(format!(
                    "{} must be a non-negative integer",
                    field
                )))
            }
        }
        _ => Err(Error::Message(format!("{} must be a number", field))),
    }
}

fn extract_pattern_optimization(s: &str) -> (Option<String>, Option<String>) {
    let mut prefix = None;
    let mut suffix = None;

    // Prefix extraction
    if s.starts_with('^') {
        let mut p = String::new();
        let chars = s.chars().skip(1); // Skip ^
        let mut escaped = false;
        for c in chars {
            if escaped {
                p.push(c);
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if "^$.*+?()[]{}|".contains(c) {
                // Special char, stop
                break;
            } else {
                p.push(c);
            }
        }
        if !p.is_empty() {
            prefix = Some(p);
        }
    }

    // Suffix extraction
    if let Some(s_no_anchor) = s.strip_suffix('$') {
        let chars: Vec<char> = s_no_anchor.chars().collect();
        let mut suf = String::new();

        let mut i = chars.len();
        while i > 0 {
            i -= 1;
            let c = chars[i];

            let is_escaped = if i > 0 && chars[i - 1] == '\\' {
                let mut backslash_count = 0;
                let mut j = i;
                while j > 0 && chars[j - 1] == '\\' {
                    backslash_count += 1;
                    j -= 1;
                }
                backslash_count % 2 != 0
            } else {
                false
            };

            if is_escaped {
                suf.insert(0, c);
                i -= 1; // Consume backslash
            } else if "^$.*+?()[]{}|".contains(c) {
                break;
            } else {
                suf.insert(0, c);
            }
        }
        if !suf.is_empty() {
            suffix = Some(suf);
        }
    }

    (prefix, suffix)
}
