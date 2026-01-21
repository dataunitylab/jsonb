use clap::Parser;
use jsonb_schema::schema::{encode, from_serde_json, Schema};
use jsonb_schema::Value;
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path to the JSON Lines input file
    #[arg(short, long)]
    input: PathBuf,

    /// Path to the output file
    #[arg(short, long)]
    output: PathBuf,

    /// Optional path to a JSON Schema file
    #[arg(short, long)]
    schema: Option<PathBuf>,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    let schema = if let Some(schema_path) = args.schema {
        let file = File::open(schema_path)?;
        let reader = BufReader::new(file);
        let json_schema: serde_json::Value = serde_json::from_reader(reader)?;
        Some(from_serde_json(&json_schema).map_err(|e| anyhow::anyhow!(e))?)
    } else {
        None
    };

    let input_file = File::open(args.input)?;
    let reader = BufReader::new(input_file);
    let mut output_file = File::create(args.output)?;

    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }

        let json_val: serde_json::Value = serde_json::from_str(&line)?;
        // Convert serde_json::Value to jsonb_schema::Value
        // We need a helper for this conversion as it's not directly exposed in public API simply
        // But we can rely on the fact that Value implements From<serde_json::Value> if available,
        // or we can implement a simple converter here similar to what we did in decoder.

        let value = serde_to_jsonb_value(&json_val);

        let mut buf = Vec::new();
        if let Some(s) = &schema {
            encode(&value, s, &mut buf);
        } else {
            // If no schema, encode with empty/default schema logic or standard encoding?
            // The prompt says "jsonb or jsonb_schema encoded".
            // We can use an empty schema which falls back to untyped encoding.
            let empty_schema = Schema {
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
            encode(&value, &empty_schema, &mut buf);
        }

        // Write length prefix (u32 LE for simplicity in this test binary, or uvarint)
        // Standard usually uses uvarint or fixed length. Let's use u32 LE length prefix for easy reading.
        let len = buf.len() as u32;
        output_file.write_all(&len.to_le_bytes())?;
        output_file.write_all(&buf)?;
    }

    Ok(())
}

fn serde_to_jsonb_value(v: &serde_json::Value) -> Value<'static> {
    use jsonb_schema::Number;
    use std::borrow::Cow;
    use std::collections::BTreeMap;

    match v {
        serde_json::Value::Null => Value::Null,
        serde_json::Value::Bool(b) => Value::Bool(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::Number(Number::Int64(i))
            } else if let Some(u) = n.as_u64() {
                Value::Number(Number::UInt64(u))
            } else if let Some(f) = n.as_f64() {
                Value::Number(Number::Float64(f))
            } else {
                Value::Number(Number::Float64(n.as_f64().unwrap_or(0.0)))
            }
        }
        serde_json::Value::String(s) => Value::String(Cow::Owned(s.clone())),
        serde_json::Value::Array(arr) => {
            let mut res = Vec::with_capacity(arr.len());
            for item in arr {
                res.push(serde_to_jsonb_value(item));
            }
            Value::Array(res)
        }
        serde_json::Value::Object(obj) => {
            let mut res = BTreeMap::new();
            for (k, val) in obj {
                res.insert(k.clone(), serde_to_jsonb_value(val));
            }
            Value::Object(res)
        }
    }
}
