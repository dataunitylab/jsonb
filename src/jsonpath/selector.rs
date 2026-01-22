// Copyright 2023 Dataෙන Labs.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::borrow::Cow;
use std::cmp::Ordering;
use std::collections::VecDeque;
use std::io::Cursor;

use crate::core::ArrayBuilder;
use crate::core::ArrayIterator;
use crate::core::JsonbItem;
use crate::core::JsonbItemType;
use crate::core::ObjectValueIterator;
use crate::error::Result;
use crate::jsonpath::ArrayIndex;
use crate::jsonpath::BinaryOperator;
use crate::jsonpath::Expr;
use crate::jsonpath::JsonPath;
use crate::jsonpath::Path;
use crate::jsonpath::PathValue;
use crate::jsonpath::RecursiveLevel;
use crate::jsonpath::UnaryOperator;
use crate::number::Number;
use crate::schema::decoder::*;
use crate::schema::encoder::encode;
use crate::schema::{InstanceType, Schema, SingleOrVec};
use crate::to_owned_jsonb;
use crate::Error;
use crate::OwnedJsonb;
use crate::RawJsonb;
use crate::Value;

#[derive(Debug, Clone)]
enum EvaluationValue<'a> {
    Path(PathValue<'a>),
    SchemaEncoded(RawJsonb<'a>, &'a Schema),
}

impl<'a> EvaluationValue<'a> {
    fn as_path_value(&self) -> Result<PathValue<'a>> {
        match self {
            EvaluationValue::Path(p) => Ok(p.clone()),
            EvaluationValue::SchemaEncoded(raw, schema) => {
                // We need to decode.
                let val = crate::schema::decode(raw.data, schema);
                // Convert Value to PathValue
                Ok(value_to_path_value(val))
            }
        }
    }
}

fn value_to_path_value(v: Value<'_>) -> PathValue<'_> {
    match v {
        Value::Null => PathValue::Null,
        Value::Bool(b) => PathValue::Boolean(b),
        Value::Number(n) => PathValue::Number(n),
        Value::String(s) => PathValue::String(s),
        Value::Array(_) | Value::Object(_) => {
            // Map complex to Null for scalar comparisons
            PathValue::Null
        }
        _ => PathValue::Null,
    }
}

#[derive(Debug)]
enum ExprValue<'a> {
    Values(Vec<EvaluationValue<'a>>),
    Value(Box<PathValue<'a>>),
}

impl ExprValue<'_> {
    fn convert_to_number(self) -> Result<Number> {
        match self {
            ExprValue::Values(mut vals) => {
                if vals.len() != 1 {
                    return Err(Error::InvalidJsonPath);
                }
                let val = vals.pop().unwrap();
                match val {
                    EvaluationValue::Path(PathValue::Number(num)) => Ok(num),
                    EvaluationValue::SchemaEncoded(raw, schema) => {
                        let val = crate::schema::decode(raw.data, schema);
                        if let Value::Number(n) = val {
                            Ok(n)
                        } else {
                            Err(Error::InvalidJsonPath)
                        }
                    }
                    _ => Err(Error::InvalidJsonPath),
                }
            }
            ExprValue::Value(val) => match *val {
                PathValue::Number(num) => Ok(num),
                _ => Err(Error::InvalidJsonPath),
            },
        }
    }

    fn convert_to_numbers(self) -> Result<Vec<Number>> {
        match self {
            ExprValue::Values(vals) => {
                let mut nums = Vec::with_capacity(vals.len());
                for val in vals {
                    match val {
                        EvaluationValue::Path(PathValue::Number(num)) => nums.push(num),
                        EvaluationValue::SchemaEncoded(raw, schema) => {
                            let val = crate::schema::decode(raw.data, schema);
                            if let Value::Number(n) = val {
                                nums.push(n);
                            } else {
                                return Err(Error::InvalidJsonPath);
                            }
                        }
                        _ => return Err(Error::InvalidJsonPath),
                    }
                }
                Ok(nums)
            }
            ExprValue::Value(val) => match *val {
                PathValue::Number(num) => Ok(vec![num]),
                _ => Err(Error::InvalidJsonPath),
            },
        }
    }
}

#[derive(Clone, Debug)]
struct SchemaItem<'a> {
    item: JsonbItem<'a>,
    schema: Option<&'a Schema>,
}

/// Represents the state of a JSON Path selection process.
///
/// It holds the root JSONB value and the intermediate results (items) found during
/// the execution of a `JsonPath`.
pub struct Selector<'a> {
    /// The root JSONB value against which the path is executed.
    root_jsonb: RawJsonb<'a>,
    /// A queue holding the JSONB items that match the path criteria during execution.
    items: VecDeque<SchemaItem<'a>>,
    /// Optional schema for the root value.
    root_schema: Option<&'a Schema>,
}

impl<'a> Selector<'a> {
    /// Creates a new `Selector` for the given root `RawJsonb`.
    ///
    /// # Arguments
    ///
    /// * `root_jsonb` - The `RawJsonb` data to select from.
    pub fn new(root_jsonb: RawJsonb<'a>) -> Selector<'a> {
        Self {
            root_jsonb,
            items: VecDeque::new(),
            root_schema: None,
        }
    }

    /// Attaches a schema to the selector, enabling schema-aware traversal and optimizations.
    pub fn with_schema(mut self, schema: &'a Schema) -> Self {
        self.root_schema = Some(schema);
        self
    }

    /// Executes the `JsonPath` and collects all matching items into a `Vec<OwnedJsonb>`.
    ///
    /// This function returns all matching elements as a `Vec<OwnedJsonb>`.
    ///
    /// # Arguments
    ///
    /// * `self` - The JSONPath selector.
    /// * `json_path` - The JSONPath expression.
    ///
    /// # Returns
    ///
    /// * `Ok(Vec<OwnedJsonb>)` - A vector containing the selected `OwnedJsonb` values.
    /// * `Err(Error)` - If the JSONB data is invalid or if an error occurs during path evaluation.
    ///
    /// # Examples
    ///
    /// ```
    /// use jsonb_schema::jsonpath::parse_json_path;
    /// use jsonb_schema::jsonpath::Selector;
    /// use jsonb_schema::OwnedJsonb;
    ///
    /// let jsonb_value = r#"{"a": {"b": [1, 2, 3]}, "c": 4}"#.parse::<OwnedJsonb>().unwrap();
    /// let raw_jsonb = jsonb_value.as_raw();
    /// let mut selector = Selector::new(raw_jsonb);
    ///
    /// let path = parse_json_path("$.a.b[*]".as_bytes()).unwrap();
    /// let result = selector.select_values(&path).unwrap();
    /// assert_eq!(result.len(), 3);
    /// assert_eq!(result[0].to_string(), "1");
    /// assert_eq!(result[1].to_string(), "2");
    /// assert_eq!(result[2].to_string(), "3");
    /// ```
    ///
    /// # See Also
    ///
    /// * `RawJsonb::select_by_path`.
    pub fn select_values(&mut self, json_path: &'a JsonPath<'a>) -> Result<Vec<OwnedJsonb>> {
        self.execute(json_path)?;
        let mut values = Vec::with_capacity(self.items.len());
        while let Some(schema_item) = self.items.pop_front() {
            let item = schema_item.item;
            if let Some(schema) = schema_item.schema {
                if let JsonbItem::Raw(raw) = item {
                    let decoded = crate::schema::decode(raw.data, schema);
                    let mut buf = Vec::new();
                    decoded.write_to_vec(&mut buf);
                    values.push(OwnedJsonb::new(buf));
                } else {
                    let value = OwnedJsonb::from_item(item)?;
                    values.push(value);
                }
            } else {
                let value = OwnedJsonb::from_item(item)?;
                values.push(value);
            }
        }
        Ok(values)
    }

    /// Executes the `JsonPath` and builds a JSON array `OwnedJsonb` from all matching items.
    ///
    /// This function returns all matching elements as a single `OwnedJsonb` representing a JSON array.
    ///
    /// # Arguments
    ///
    /// * `self` - The JSONPath selector.
    /// * `json_path` - The JSONPath expression.
    ///
    /// # Returns
    ///
    /// * `Ok(OwnedJsonb)` - A single `OwnedJsonb` (a JSON array) containing the selected values.
    /// * `Err(Error)` - If the JSONB data is invalid or if an error occurs during path evaluation.
    ///
    /// # Examples
    ///
    /// ```
    /// use jsonb_schema::jsonpath::parse_json_path;
    /// use jsonb_schema::jsonpath::Selector;
    /// use jsonb_schema::OwnedJsonb;
    ///
    /// let jsonb_value = r#"{"a": {"b": [1, 2, 3]}, "c": 4}"#.parse::<OwnedJsonb>().unwrap();
    /// let raw_jsonb = jsonb_value.as_raw();
    /// let mut selector = Selector::new(raw_jsonb);
    ///
    /// let path = parse_json_path("$.a.b[*]".as_bytes()).unwrap();
    /// let result = selector.select_array(&path).unwrap();
    /// assert_eq!(result.to_string(), "[1,2,3]");
    /// ```
    ///
    /// # See Also
    ///
    /// * `RawJsonb::select_array_by_path`.
    pub fn select_array(&mut self, json_path: &'a JsonPath<'a>) -> Result<OwnedJsonb> {
        self.execute(json_path)?;
        let mut builder = ArrayBuilder::with_capacity(self.items.len());
        while let Some(schema_item) = self.items.pop_front() {
            if let Some(schema) = schema_item.schema {
                if let JsonbItem::Raw(raw) = schema_item.item {
                    let decoded = crate::schema::decode(raw.data, schema);
                    let mut buf = Vec::new();
                    decoded.write_to_vec(&mut buf);
                    builder.push_jsonb_item(JsonbItem::Owned(OwnedJsonb::new(buf)));
                } else {
                    builder.push_jsonb_item(schema_item.item);
                }
            } else {
                builder.push_jsonb_item(schema_item.item);
            }
        }
        builder.build()
    }

    /// Executes the `JsonPath` and returns the first matching item as an `Option<OwnedJsonb>`.
    ///
    /// This function returns the first matched element wrapped in `Some`, or `None` if no element matches the path.
    ///
    /// # Arguments
    ///
    /// * `self` - The JSONPath selector.
    /// * `json_path` - The JSONPath expression.
    ///
    /// # Returns
    ///
    /// * `Ok(Some(OwnedJsonb))` - A single `OwnedJsonb` containing the first matched value.
    /// * `Ok(None)` - The path does not match any values.
    /// * `Err(Error)` - If the JSONB data is invalid or if an error occurs during path evaluation.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use jsonb_schema::jsonpath::parse_json_path;
    /// use jsonb_schema::jsonpath::Selector;
    /// use jsonb_schema::OwnedJsonb;
    ///
    /// let jsonb_value = r#"{"a": [{"b": 1}, {"b": 2}], "c": 3}"#.parse::<OwnedJsonb>().unwrap();
    /// let raw_jsonb = jsonb_value.as_raw();
    /// let mut selector = Selector::new(raw_jsonb);
    ///
    /// let path = parse_json_path("$.a[0].b".as_bytes()).unwrap(); // Matches multiple values.
    /// let result = selector.select_first(&path).unwrap();
    /// assert_eq!(result.unwrap().to_string(), "1");
    ///
    /// let path = parse_json_path("$.d".as_bytes()).unwrap(); // No match.
    /// let result = selector.select_first(&path).unwrap();
    /// assert!(result.is_none());
    /// ```
    ///
    /// # See Also
    ///
    /// * `RawJsonb::select_first_by_path`.
    pub fn select_first(&mut self, json_path: &'a JsonPath<'a>) -> Result<Option<OwnedJsonb>> {
        self.execute(json_path)?;
        if let Some(schema_item) = self.items.pop_front() {
            if let Some(schema) = schema_item.schema {
                if let JsonbItem::Raw(raw) = schema_item.item {
                    let decoded = crate::schema::decode(raw.data, schema);
                    let mut buf = Vec::new();
                    decoded.write_to_vec(&mut buf);
                    Ok(Some(OwnedJsonb::new(buf)))
                } else {
                    let value = OwnedJsonb::from_item(schema_item.item)?;
                    Ok(Some(value))
                }
            } else {
                let value = OwnedJsonb::from_item(schema_item.item)?;
                Ok(Some(value))
            }
        } else {
            Ok(None)
        }
    }

    /// Executes the `JsonPath` and returns a single value or an array of values.
    ///
    /// If exactly one element matches, it is returned directly (wrapped in `Some`).
    /// If multiple elements match, they are returned as a JSON array (wrapped in `Some`).
    /// If no elements match, `None` is returned.
    ///
    /// # Arguments
    ///
    /// * `self` - The JSONPath selector.
    /// * `json_path` - The JSONPath expression.
    ///
    /// # Returns
    ///
    /// * `Ok(Some(OwnedJsonb))` - A single `OwnedJsonb` containing the matched values.
    /// * `Ok(None)` - The path does not match any values.
    /// * `Err(Error)` - If the JSONB data is invalid or if an error occurs during path evaluation.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use jsonb_schema::jsonpath::parse_json_path;
    /// use jsonb_schema::jsonpath::Selector;
    /// use jsonb_schema::OwnedJsonb;
    ///
    /// let jsonb_value = r#"{"a": [{"b": 1}, {"b": 2}], "c": 3}"#.parse::<OwnedJsonb>().unwrap();
    /// let raw_jsonb = jsonb_value.as_raw();
    /// let mut selector = Selector::new(raw_jsonb);
    ///
    /// let path = parse_json_path("$.c".as_bytes()).unwrap(); // Matches a single value.
    /// let result = selector.select_value(&path).unwrap();
    /// assert_eq!(result.unwrap().to_string(), "3");
    ///
    /// let path = parse_json_path("$.a[*].b".as_bytes()).unwrap(); // Matches multiple values.
    /// let result = selector.select_value(&path).unwrap();
    /// assert_eq!(result.unwrap().to_string(), "[1,2]");
    ///
    /// let path = parse_json_path("$.x".as_bytes()).unwrap(); // No match.
    /// let result = selector.select_value(&path).unwrap();
    /// assert!(result.is_none());
    /// ```
    ///
    /// # See Also
    ///
    /// * `RawJsonb::select_value_by_path`.
    pub fn select_value(&mut self, json_path: &'a JsonPath<'a>) -> Result<Option<OwnedJsonb>> {
        self.execute(json_path)?;
        if self.items.len() > 1 {
            let mut builder = ArrayBuilder::with_capacity(self.items.len());
            while let Some(schema_item) = self.items.pop_front() {
                if let Some(schema) = schema_item.schema {
                    if let JsonbItem::Raw(raw) = schema_item.item {
                        let decoded = crate::schema::decode(raw.data, schema);
                        let mut buf = Vec::new();
                        decoded.write_to_vec(&mut buf);
                        builder.push_jsonb_item(JsonbItem::Owned(OwnedJsonb::new(buf)));
                    } else {
                        builder.push_jsonb_item(schema_item.item);
                    }
                } else {
                    builder.push_jsonb_item(schema_item.item);
                }
            }
            let array = builder.build()?;
            Ok(Some(array))
        } else if let Some(schema_item) = self.items.pop_front() {
            if let Some(schema) = schema_item.schema {
                if let JsonbItem::Raw(raw) = schema_item.item {
                    let decoded = crate::schema::decode(raw.data, schema);
                    let mut buf = Vec::new();
                    decoded.write_to_vec(&mut buf);
                    Ok(Some(OwnedJsonb::new(buf)))
                } else {
                    let value = OwnedJsonb::from_item(schema_item.item)?;
                    Ok(Some(value))
                }
            } else {
                let value = OwnedJsonb::from_item(schema_item.item)?;
                Ok(Some(value))
            }
        } else {
            Ok(None)
        }
    }

    /// Executes the `JsonPath` and checks if any item matches.
    ///
    /// # Arguments
    ///
    /// * `self` - The JSONPath selector.
    /// * `json_path` - The JSONPath expression.
    ///
    /// # Returns
    ///
    /// * `Ok(true)` - If the JSON path exists.
    /// * `Ok(false)` - If the JSON path does not exist.
    /// * `Err(Error)` - If the JSONB data is invalid or if an error occurs during path evaluation.
    ///   This could also indicate issues with the `json_path` itself.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use jsonb_schema::jsonpath::parse_json_path;
    /// use jsonb_schema::jsonpath::Selector;
    /// use jsonb_schema::OwnedJsonb;
    ///
    /// let jsonb_value = r#"{"a": {"b": [1, 2, 3]}, "c": 4}"#.parse::<OwnedJsonb>().unwrap();
    /// let raw_jsonb = jsonb_value.as_raw();
    /// let mut selector = Selector::new(raw_jsonb);
    ///
    /// // Valid paths
    /// let path1 = parse_json_path("$.a.b[1]".as_bytes()).unwrap();
    /// assert!(selector.exists(&path1).unwrap());
    ///
    /// let path2 = parse_json_path("$.c".as_bytes()).unwrap();
    /// assert!(selector.exists(&path2).unwrap());
    ///
    /// // Invalid paths
    /// let path3 = parse_json_path("$.a.x".as_bytes()).unwrap(); // "x" does not exist
    /// assert!(!selector.exists(&path3).unwrap());
    /// ```
    ///
    /// # See Also
    ///
    /// * `RawJsonb::path_exists`.
    pub fn exists(&mut self, json_path: &'a JsonPath<'a>) -> Result<bool> {
        self.execute(json_path)?;
        Ok(!self.items.is_empty())
    }

    /// Executes a `JsonPath` predicate and returns the boolean result.
    ///
    /// This function requires that the `JsonPath` represents a predicate expression
    /// (e.g., `$.c > 1`, `exists($.a)`). It executes the path and expects a single
    /// boolean value as the result.
    ///
    /// # Arguments
    ///
    /// * `self` - The JSONPath selector.
    /// * `json_path` - The JSONPath expression.
    ///
    /// # Returns
    ///
    /// * `Ok(Some(true))` - If the JSON path with its predicate matches at least one value in the JSONB data.
    /// * `Ok(Some(false))` - If the JSON path with its predicate does not match any values.
    /// * `Ok(None)` - If the JSON path is not a predicate expr or predicate result is not a boolean value.
    /// * `Err(Error)` - If the JSONB data is invalid or if an error occurs during path evaluation or predicate checking.
    ///   This could also indicate issues with the `json_path` itself (invalid syntax, etc.).
    ///
    /// # Examples
    ///
    /// ```rust
    /// use jsonb_schema::jsonpath::parse_json_path;
    /// use jsonb_schema::jsonpath::Selector;
    /// use jsonb_schema::OwnedJsonb;
    ///
    /// let jsonb_value = r#"[
    ///     {"price": 12, "title": "Book A"},
    ///     {"price": 8, "title": "Book B"},
    ///     {"price": 5, "title": "Book C"}
    /// ]"#
    /// .parse::<OwnedJsonb>()
    /// .unwrap();
    /// let raw_jsonb = jsonb_value.as_raw();
    /// let mut selector = Selector::new(raw_jsonb);
    ///
    /// // Path with predicate (select books with price < 10)
    /// let path = parse_json_path("$[*].price < 10".as_bytes()).unwrap();
    /// assert_eq!(selector.predicate_match(&path).unwrap(), Some(true)); // True because Book B and Book C match.
    ///
    /// // Path with predicate (select books with title "Book D")
    /// let path = parse_json_path("$[*].title == \"Book D\"".as_bytes()).unwrap();
    /// assert_eq!(selector.predicate_match(&path).unwrap(), Some(false)); // False because no book has this title.
    ///
    /// // Path is not a predicate expr
    /// let path = parse_json_path("$[*].title".as_bytes()).unwrap();
    /// assert_eq!(raw_jsonb.path_match(&path).unwrap(), None);
    /// ```
    ///
    /// # See Also
    ///
    /// * `RawJsonb::path_match`.
    pub fn predicate_match(&mut self, json_path: &'a JsonPath<'a>) -> Result<Option<bool>> {
        if !json_path.is_predicate() {
            return Ok(None);
        }
        self.execute(json_path)?;
        if let Some(schema_item) = self.items.pop_front() {
            match schema_item.item {
                JsonbItem::Boolean(v) => return Ok(Some(v)),
                JsonbItem::Raw(raw) if schema_item.schema.is_some() => {
                    let val = crate::schema::decode(raw.data, schema_item.schema.unwrap());
                    if let Value::Bool(b) = val {
                        return Ok(Some(b));
                    }
                }
                _ => {}
            }
        }
        Ok(None)
    }

    fn execute(&mut self, json_path: &'a JsonPath<'a>) -> Result<()> {
        let root_item = SchemaItem {
            item: JsonbItem::Raw(self.root_jsonb),
            schema: self.root_schema,
        };
        self.items.clear();
        self.items.push_front(root_item);

        if json_path.paths.len() == 1 {
            if let Path::Expr(expr) = &json_path.paths[0] {
                let root_item = self.items.pop_front().unwrap();
                self.eval_expr(root_item, expr)?;
                return Ok(());
            }
        }
        self.select_by_paths(&json_path.paths)?;

        Ok(())
    }

    fn select_by_paths(&mut self, paths: &'a [Path<'a>]) -> Result<()> {
        if let Some(Path::Current) = paths.first() {
            return Err(Error::InvalidJsonPath);
        }

        for path in paths.iter() {
            match path {
                &Path::Root | &Path::Current => {
                    continue;
                }
                Path::FilterExpr(expr) | Path::Expr(expr) => {
                    let len = self.items.len();
                    for _ in 0..len {
                        let item = self.items.pop_front().unwrap();
                        let res = self.eval_filter_expr(item.clone(), expr)?.unwrap_or(false);
                        if res {
                            self.items.push_back(item);
                        }
                    }
                }
                _ => {
                    self.select_by_path(path)?;
                }
            }
        }
        Ok(())
    }

    fn select_by_path(&mut self, path: &'a Path<'a>) -> Result<bool> {
        if self.items.is_empty() {
            return Ok(false);
        }

        let len = self.items.len();
        for _ in 0..len {
            let item = self.items.pop_front().unwrap();

            match path {
                Path::DotWildcard => {
                    self.select_object_values(item)?;
                }
                Path::RecursiveDotWildcard(index_opt) => {
                    if item.schema.is_some() {
                        // Recursive schema not implemented
                    } else {
                        self.recursive_select_values(item.item, 0, index_opt)?;
                    }
                }
                Path::BracketWildcard => {
                    self.select_array_values(item)?;
                }
                Path::ColonField(name) | Path::DotField(name) | Path::ObjectField(name) => {
                    self.select_object_values_by_name(item, name)?;
                }
                Path::ArrayIndices(array_indices) => {
                    self.select_array_values_by_indices(item, array_indices)?;
                }
                _ => todo!(),
            }
        }
        Ok(true)
    }

    fn select_object_values(&mut self, parent_item: SchemaItem<'a>) -> Result<()> {
        if let Some(schema) = parent_item.schema {
            if let JsonbItem::Raw(raw) = parent_item.item {
                let mut cursor = Cursor::new(raw.data);
                let mut children = Vec::new();
                Self::traverse_schema_object(
                    &mut cursor,
                    schema,
                    |_key, val_slice, val_schema| {
                        let child = SchemaItem {
                            item: JsonbItem::Raw(RawJsonb::new(val_slice)),
                            schema: val_schema,
                        };
                        children.push(child);
                        Ok(())
                    },
                )?;
                for child in children {
                    self.items.push_back(child);
                }
                return Ok(());
            }
        }

        let jsonb_item_type = parent_item.item.jsonb_item_type()?;
        if !matches!(jsonb_item_type, JsonbItemType::Object(_)) {
            return Ok(());
        };

        match parent_item.item {
            JsonbItem::Raw(raw) => {
                let object_val_iter_opt = ObjectValueIterator::new(raw)?;
                if let Some(mut object_val_iter) = object_val_iter_opt {
                    for result in &mut object_val_iter {
                        let val_item = result?;
                        self.items.push_back(SchemaItem {
                            item: val_item,
                            schema: None,
                        });
                    }
                }
            }
            JsonbItem::Owned(ref owned) => {
                let object_val_iter_opt = ObjectValueIterator::new(owned.as_raw())?;
                if let Some(mut object_val_iter) = object_val_iter_opt {
                    for result in &mut object_val_iter {
                        let val_item = result?;
                        let owned_item = OwnedJsonb::from_item(val_item)?;
                        self.items.push_back(SchemaItem {
                            item: JsonbItem::Owned(owned_item),
                            schema: None,
                        });
                    }
                }
            }
            _ => {}
        }

        Ok(())
    }

    fn recursive_select_values(
        &mut self,
        parent_item: JsonbItem<'a>,
        curr_level: u8,
        recursive_level_opt: &Option<RecursiveLevel>,
    ) -> Result<()> {
        let (is_match, should_continue) = if let Some(recursive_level) = recursive_level_opt {
            recursive_level.check_recursive_level(curr_level)
        } else {
            (true, true)
        };
        if is_match {
            self.items.push_back(SchemaItem {
                item: parent_item.clone(),
                schema: None,
            });
        }
        if !should_continue {
            return Ok(());
        }

        match parent_item {
            JsonbItem::Raw(raw) => {
                let object_val_iter_opt = ObjectValueIterator::new(raw)?;
                if let Some(mut object_val_iter) = object_val_iter_opt {
                    for result in &mut object_val_iter {
                        let val_item = result?;
                        self.recursive_select_values(
                            val_item,
                            curr_level + 1,
                            recursive_level_opt,
                        )?;
                    }
                }
                let array_iter_opt = ArrayIterator::new(raw)?;
                if let Some(mut array_iter) = array_iter_opt {
                    for item_result in &mut array_iter {
                        let item = item_result?;
                        self.recursive_select_values(item, curr_level + 1, recursive_level_opt)?;
                    }
                }
            }
            JsonbItem::Owned(ref owned) => {
                let object_val_iter_opt = ObjectValueIterator::new(owned.as_raw())?;
                if let Some(mut object_val_iter) = object_val_iter_opt {
                    for result in &mut object_val_iter {
                        let val_item = result?;
                        let owned_item = OwnedJsonb::from_item(val_item)?;
                        self.recursive_select_values(
                            JsonbItem::Owned(owned_item),
                            curr_level + 1,
                            recursive_level_opt,
                        )?;
                    }
                }
                let array_iter_opt = ArrayIterator::new(owned.as_raw())?;
                if let Some(mut array_iter) = array_iter_opt {
                    for item_result in &mut array_iter {
                        let item = item_result?;
                        let owned_item = OwnedJsonb::from_item(item)?;
                        self.recursive_select_values(
                            JsonbItem::Owned(owned_item),
                            curr_level + 1,
                            recursive_level_opt,
                        )?;
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn select_object_values_by_name(
        &mut self,
        parent_item: SchemaItem<'a>,
        name: &'a str,
    ) -> Result<()> {
        if let Some(schema) = parent_item.schema {
            if let JsonbItem::Raw(raw) = parent_item.item {
                let mut cursor = Cursor::new(raw.data);
                let mut found = None;
                Self::traverse_schema_object(&mut cursor, schema, |key, val_slice, val_schema| {
                    if key == name {
                        found = Some(SchemaItem {
                            item: JsonbItem::Raw(RawJsonb::new(val_slice)),
                            schema: val_schema,
                        });
                    }
                    Ok(())
                })?;
                if let Some(child) = found {
                    self.items.push_back(child);
                }
                return Ok(());
            }
        }

        let jsonb_item_type = parent_item.item.jsonb_item_type()?;
        if !matches!(jsonb_item_type, JsonbItemType::Object(_)) {
            return Ok(());
        };

        let key_name = Cow::Borrowed(name);
        match parent_item.item {
            JsonbItem::Raw(raw) => {
                if let Some(val_item) =
                    raw.get_object_value_by_key_name(&key_name, |name, key| key.eq(name))?
                {
                    self.items.push_back(SchemaItem {
                        item: val_item,
                        schema: None,
                    });
                }
            }
            JsonbItem::Owned(ref owned) => {
                let raw = owned.as_raw();
                if let Some(val_item) =
                    raw.get_object_value_by_key_name(&key_name, |name, key| key.eq(name))?
                {
                    let owned_item = OwnedJsonb::from_item(val_item)?;
                    self.items.push_back(SchemaItem {
                        item: JsonbItem::Owned(owned_item),
                        schema: None,
                    });
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn select_array_values(&mut self, parent_item: SchemaItem<'a>) -> Result<()> {
        if let Some(schema) = parent_item.schema {
            if let JsonbItem::Raw(raw) = parent_item.item {
                let mut cursor = Cursor::new(raw.data);
                let mut children = Vec::new();
                Self::traverse_schema_array(&mut cursor, schema, |_idx, val_slice, val_schema| {
                    let child = SchemaItem {
                        item: JsonbItem::Raw(RawJsonb::new(val_slice)),
                        schema: val_schema,
                    };
                    children.push(child);
                    Ok(())
                })?;
                for child in children {
                    self.items.push_back(child);
                }
                return Ok(());
            }
        }

        let jsonb_item_type = parent_item.item.jsonb_item_type()?;
        if !matches!(jsonb_item_type, JsonbItemType::Array(_)) {
            self.items.push_back(parent_item);
            return Ok(());
        };

        match parent_item.item {
            JsonbItem::Raw(raw) => {
                let array_iter_opt = ArrayIterator::new(raw)?;
                if let Some(mut array_iter) = array_iter_opt {
                    for item_result in &mut array_iter {
                        let item = item_result?;
                        self.items.push_back(SchemaItem { item, schema: None });
                    }
                }
            }
            JsonbItem::Owned(ref owned) => {
                let array_iter_opt = ArrayIterator::new(owned.as_raw())?;
                if let Some(mut array_iter) = array_iter_opt {
                    for item_result in &mut array_iter {
                        let item = item_result?;
                        let owned_item = OwnedJsonb::from_item(item)?;
                        self.items.push_back(SchemaItem {
                            item: JsonbItem::Owned(owned_item),
                            schema: None,
                        });
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn select_array_values_by_indices(
        &mut self,
        parent_item: SchemaItem<'a>,
        array_indices: &Vec<ArrayIndex>,
    ) -> Result<()> {
        if let Some(schema) = parent_item.schema {
            if let JsonbItem::Raw(raw) = parent_item.item {
                let (len, mut cursor) = Self::peek_schema_array_length(raw.data, schema)?;

                let mut indices_to_select = std::collections::HashSet::new();
                for array_index in array_indices {
                    indices_to_select.extend(array_index.to_indices(len));
                }
                if indices_to_select.is_empty() {
                    return Ok(());
                }

                let mut children = Vec::new();
                Self::traverse_schema_array_content(
                    &mut cursor,
                    len,
                    schema,
                    |idx, val_slice, val_schema| {
                        if indices_to_select.contains(&idx) {
                            let child = SchemaItem {
                                item: JsonbItem::Raw(RawJsonb::new(val_slice)),
                                schema: val_schema,
                            };
                            children.push(child);
                        }
                        Ok(())
                    },
                )?;
                for child in children {
                    self.items.push_back(child);
                }
                return Ok(());
            }
        }

        let jsonb_item_type = parent_item.item.jsonb_item_type()?;
        let JsonbItemType::Array(arr_len) = jsonb_item_type else {
            return Ok(());
        };
        for array_index in array_indices {
            let indices = array_index.to_indices(arr_len);
            if indices.is_empty() {
                continue;
            }
            match parent_item.item {
                JsonbItem::Raw(raw) => {
                    let array_iter_opt = ArrayIterator::new(raw)?;
                    if let Some(array_iter) = array_iter_opt {
                        for (i, item_result) in &mut array_iter.enumerate() {
                            let item = item_result?;
                            if indices.contains(&i) {
                                self.items.push_back(SchemaItem { item, schema: None });
                            }
                        }
                    }
                }
                JsonbItem::Owned(ref owned) => {
                    let array_iter_opt = ArrayIterator::new(owned.as_raw())?;
                    if let Some(array_iter) = array_iter_opt {
                        for (i, item_result) in &mut array_iter.enumerate() {
                            let item = item_result?;
                            if indices.contains(&i) {
                                let owned_item = OwnedJsonb::from_item(item)?;
                                self.items.push_back(SchemaItem {
                                    item: JsonbItem::Owned(owned_item),
                                    schema: None,
                                });
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn eval_expr(&mut self, item: SchemaItem<'a>, expr: &'a Expr<'a>) -> Result<()> {
        match expr {
            Expr::UnaryOp { op, operand } => {
                let res_items = self.eval_unary_arithmetic_func(item.clone(), op, operand)?;
                for res_item in res_items {
                    self.items.push_back(res_item);
                }
            }
            Expr::BinaryOp { op, left, right } => match op {
                BinaryOperator::Add
                | BinaryOperator::Subtract
                | BinaryOperator::Multiply
                | BinaryOperator::Divide
                | BinaryOperator::Modulo => {
                    let res_items =
                        self.eval_binary_arithmetic_func(item.clone(), op, left, right)?;
                    for res_item in res_items {
                        self.items.push_back(res_item);
                    }
                }
                _ => {
                    let res = self.eval_filter_expr(item, expr)?;
                    let res_item = if let Some(res) = res {
                        JsonbItem::Boolean(res)
                    } else {
                        JsonbItem::Null
                    };
                    self.items.push_back(SchemaItem {
                        item: res_item,
                        schema: None,
                    });
                }
            },
            Expr::ExistsFunc(_) => {
                let res = self.eval_filter_expr(item, expr)?;
                let res_item = if let Some(res) = res {
                    JsonbItem::Boolean(res)
                } else {
                    JsonbItem::Null
                };
                self.items.push_back(SchemaItem {
                    item: res_item,
                    schema: None,
                });
            }
            Expr::Value(val) => {
                let res_item = self.eval_value(val)?;
                self.items.push_back(SchemaItem {
                    item: res_item,
                    schema: None,
                });
            }
            Expr::Paths(_) => {
                return Err(Error::InvalidJsonPath);
            }
        }
        Ok(())
    }

    fn eval_unary_arithmetic_func(
        &mut self,
        item: SchemaItem<'a>,
        op: &UnaryOperator,
        operand: &'a Expr<'a>,
    ) -> Result<Vec<SchemaItem<'a>>> {
        let operand = self.convert_expr_val(item, operand)?;
        let Ok(nums) = operand.convert_to_numbers() else {
            return Err(Error::InvalidJsonPath);
        };
        let mut num_vals = Vec::with_capacity(nums.len());
        match op {
            UnaryOperator::Add => {
                for num in nums {
                    let owned_num = to_owned_jsonb(&num)?;
                    num_vals.push(SchemaItem {
                        item: JsonbItem::Owned(owned_num),
                        schema: None,
                    });
                }
            }
            UnaryOperator::Subtract => {
                for num in nums {
                    let neg_num = num.neg()?;
                    let owned_num = to_owned_jsonb(&neg_num)?;
                    num_vals.push(SchemaItem {
                        item: JsonbItem::Owned(owned_num),
                        schema: None,
                    });
                }
            }
        };
        Ok(num_vals)
    }

    fn eval_binary_arithmetic_func(
        &mut self,
        item: SchemaItem<'a>,
        op: &BinaryOperator,
        left: &'a Expr<'a>,
        right: &'a Expr<'a>,
    ) -> Result<Vec<SchemaItem<'a>>> {
        let lhs = self.convert_expr_val(item.clone(), left)?;
        let rhs = self.convert_expr_val(item.clone(), right)?;
        let Ok(lnum) = lhs.convert_to_number() else {
            return Err(Error::InvalidJsonPath);
        };
        let Ok(rnum) = rhs.convert_to_number() else {
            return Err(Error::InvalidJsonPath);
        };

        let num_val = match op {
            BinaryOperator::Add => lnum.add(rnum)?,
            BinaryOperator::Subtract => lnum.sub(rnum)?,
            BinaryOperator::Multiply => lnum.mul(rnum)?,
            BinaryOperator::Divide => lnum.div(rnum)?,
            BinaryOperator::Modulo => lnum.rem(rnum)?,
            _ => return Ok(vec![]),
        };
        let owned_num = to_owned_jsonb(&num_val)?;
        Ok(vec![SchemaItem {
            item: JsonbItem::Owned(owned_num),
            schema: None,
        }])
    }

    fn eval_value(&mut self, val: &PathValue<'a>) -> Result<JsonbItem<'a>> {
        let owned_val = match val {
            PathValue::Null => to_owned_jsonb(&vec![&()])?,
            PathValue::Boolean(v) => to_owned_jsonb(&vec![v])?,
            PathValue::Number(v) => to_owned_jsonb(&vec![v])?,
            PathValue::String(v) => to_owned_jsonb(&vec![v.to_string()])?,
            PathValue::Raw(v) => {
                return Ok(JsonbItem::Raw(*v));
            }
        };
        Ok(JsonbItem::Owned(owned_val))
    }

    fn eval_filter_expr(
        &mut self,
        item: SchemaItem<'a>,
        expr: &'a Expr<'a>,
    ) -> Result<Option<bool>> {
        match expr {
            Expr::BinaryOp { op, left, right } => match op {
                BinaryOperator::Or => {
                    let lhs = self.eval_filter_expr(item.clone(), left)?;
                    let rhs = self.eval_filter_expr(item.clone(), right)?;
                    match (lhs, rhs) {
                        (Some(lhs), Some(rhs)) => Ok(Some(lhs || rhs)),
                        (_, _) => Ok(None),
                    }
                }
                BinaryOperator::And => {
                    let lhs = self.eval_filter_expr(item.clone(), left)?;
                    let rhs = self.eval_filter_expr(item.clone(), right)?;
                    match (lhs, rhs) {
                        (Some(lhs), Some(rhs)) => Ok(Some(lhs && rhs)),
                        (_, _) => Ok(None),
                    }
                }
                BinaryOperator::Eq
                | BinaryOperator::NotEq
                | BinaryOperator::Lt
                | BinaryOperator::Lte
                | BinaryOperator::Gt
                | BinaryOperator::Gte
                | BinaryOperator::StartsWith => {
                    let lhs = self.convert_expr_val(item.clone(), left)?;
                    let rhs = self.convert_expr_val(item.clone(), right)?;
                    let res = self.eval_compare(op, &lhs, &rhs);
                    Ok(res)
                }
                _ => Ok(None),
            },
            Expr::ExistsFunc(paths) => {
                let res = self.eval_exists(item, paths)?;
                Ok(Some(res))
            }
            _ => Err(Error::InvalidJsonPath),
        }
    }

    fn eval_exists(&mut self, item: SchemaItem<'a>, paths: &'a [Path<'a>]) -> Result<bool> {
        let filter_items = self.select_by_filter_paths(item, paths)?;
        let res = !filter_items.is_empty();
        Ok(res)
    }

    fn select_by_filter_paths(
        &mut self,
        item: SchemaItem<'a>,
        paths: &'a [Path<'a>],
    ) -> Result<VecDeque<SchemaItem<'a>>> {
        let mut items = VecDeque::new();
        if let Some(Path::Current) = paths.first() {
            items.push_front(item.clone());
        } else {
            let root_item = SchemaItem {
                item: JsonbItem::Raw(self.root_jsonb),
                schema: self.root_schema,
            };
            items.push_front(root_item);
        }
        std::mem::swap(&mut self.items, &mut items);

        for path in paths.iter() {
            match path {
                &Path::Root | &Path::Current => {
                    continue;
                }
                Path::FilterExpr(expr) => {
                    let len = self.items.len();
                    for _ in 0..len {
                        let item = self.items.pop_front().unwrap();
                        let res = self.eval_filter_expr(item.clone(), expr)?.unwrap_or(false);
                        if res {
                            self.items.push_back(item);
                        }
                    }
                }
                _ => {
                    self.select_by_path(path)?;
                }
            }
        }
        std::mem::swap(&mut self.items, &mut items);
        Ok(items)
    }

    fn convert_expr_val(
        &mut self,
        item: SchemaItem<'a>,
        expr: &'a Expr<'a>,
    ) -> Result<ExprValue<'a>> {
        match expr {
            Expr::Value(value) => Ok(ExprValue::Value(value.clone())),
            Expr::Paths(paths) => {
                let mut filter_items = self.select_by_filter_paths(item, paths)?;

                let mut values = Vec::with_capacity(filter_items.len());
                while let Some(schema_item) = filter_items.pop_front() {
                    // Check if schema encoded
                    if let Some(schema) = schema_item.schema {
                        if let JsonbItem::Raw(raw) = schema_item.item {
                            values.push(EvaluationValue::SchemaEncoded(raw, schema));
                            continue;
                        }
                    }

                    let value = match schema_item.item {
                        JsonbItem::Null => PathValue::Null,
                        JsonbItem::Boolean(v) => PathValue::Boolean(v),
                        JsonbItem::Number(num) => {
                            let n = num.as_number()?;
                            PathValue::Number(n)
                        }
                        JsonbItem::String(s) => PathValue::String(s),
                        JsonbItem::Raw(raw) => {
                            // Standard Raw: collect values in the array.
                            let array_iter_opt = ArrayIterator::new(raw)?;
                            if let Some(mut array_iter) = array_iter_opt {
                                for item_result in &mut array_iter {
                                    let item = item_result?;
                                    let value = match item {
                                        JsonbItem::Null => PathValue::Null,
                                        JsonbItem::Boolean(v) => PathValue::Boolean(v),
                                        JsonbItem::Number(num) => {
                                            let n = num.as_number()?;
                                            PathValue::Number(n)
                                        }
                                        JsonbItem::String(s) => PathValue::String(s),
                                        JsonbItem::Raw(raw) => PathValue::Raw(raw),
                                        _ => continue,
                                    };
                                    values.push(EvaluationValue::Path(value));
                                }
                            } else {
                                let jsonb_item = JsonbItem::from_raw_jsonb(raw)?;
                                let value = match jsonb_item {
                                    JsonbItem::Null => PathValue::Null,
                                    JsonbItem::Boolean(v) => PathValue::Boolean(v),
                                    JsonbItem::Number(num) => {
                                        let n = num.as_number()?;
                                        PathValue::Number(n)
                                    }
                                    JsonbItem::String(s) => PathValue::String(s),
                                    JsonbItem::Raw(raw) => PathValue::Raw(raw),
                                    _ => continue,
                                };
                                values.push(EvaluationValue::Path(value));
                            }
                            continue;
                        }
                        _ => {
                            continue;
                        }
                    };
                    values.push(EvaluationValue::Path(value));
                }
                Ok(ExprValue::Values(values))
            }
            _ => unreachable!(),
        }
    }

    fn eval_compare(
        &mut self,
        op: &BinaryOperator,
        lhs: &ExprValue<'a>,
        rhs: &ExprValue<'a>,
    ) -> Option<bool> {
        let (lvals, rvals) = match (lhs, rhs) {
            (ExprValue::Value(lhs), ExprValue::Value(rhs)) => (
                vec![EvaluationValue::Path(*lhs.clone())],
                vec![EvaluationValue::Path(*rhs.clone())],
            ),
            (ExprValue::Values(lhses), ExprValue::Value(rhs)) => {
                (lhses.clone(), vec![EvaluationValue::Path(*rhs.clone())])
            }
            (ExprValue::Value(lhs), ExprValue::Values(rhses)) => {
                (vec![EvaluationValue::Path(*lhs.clone())], rhses.clone())
            }
            (ExprValue::Values(lhses), ExprValue::Values(rhses)) => (lhses.clone(), rhses.clone()),
        };

        for lval in lvals.iter() {
            for rval in rvals.iter() {
                if let Some(res) = self.compare_evaluation_value(op, lval, rval) {
                    if res {
                        return Some(true);
                    }
                } else {
                    return None;
                }
            }
        }
        Some(false)
    }

    fn compare_evaluation_value(
        &mut self,
        op: &BinaryOperator,
        lhs: &EvaluationValue<'a>,
        rhs: &EvaluationValue<'a>,
    ) -> Option<bool> {
        // Optimization check
        if let (EvaluationValue::SchemaEncoded(raw_lhs, schema), EvaluationValue::Path(val_rhs)) =
            (lhs, rhs)
        {
            if *op == BinaryOperator::Eq {
                if let Some(mul) = schema.multiple_of {
                    if let PathValue::Number(n) = val_rhs {
                        if let Some(i) = n.as_i128() {
                            if i % mul != 0 {
                                return Some(false);
                            }
                        }
                    }
                }

                let val = value_to_value(val_rhs);
                let mut buf = Vec::new();
                encode(&val, schema, &mut buf);
                if raw_lhs.data == buf {
                    return Some(true);
                } else {
                    // Fallback to decode
                }
            }
        }

        let lhs_pv = lhs.clone().as_path_value().ok()?;
        let rhs_pv = rhs.clone().as_path_value().ok()?;
        self.compare_value(op, lhs_pv, rhs_pv)
    }

    fn compare_value(
        &mut self,
        op: &BinaryOperator,
        lhs: PathValue<'a>,
        rhs: PathValue<'a>,
    ) -> Option<bool> {
        // container value can't compare values.
        if matches!(lhs, PathValue::Raw(_)) || matches!(rhs, PathValue::Raw(_)) {
            return None;
        }
        if op == &BinaryOperator::StartsWith {
            let res = match (lhs, rhs) {
                (PathValue::String(lhs), PathValue::String(rhs)) => Some(lhs.starts_with(&*rhs)),
                (_, _) => None,
            };
            return res;
        }
        let order = lhs.partial_cmp(&rhs);
        if let Some(order) = order {
            let res = match op {
                BinaryOperator::Eq => order == Ordering::Equal,
                BinaryOperator::NotEq => order != Ordering::Equal,
                BinaryOperator::Lt => order == Ordering::Less,
                BinaryOperator::Lte => order == Ordering::Equal || order == Ordering::Less,
                BinaryOperator::Gt => order == Ordering::Greater,
                BinaryOperator::Gte => order == Ordering::Equal || order == Ordering::Greater,
                _ => {
                    return None;
                }
            };
            Some(res)
        } else if matches!(op, BinaryOperator::NotEq) {
            Some(true)
        } else {
            None
        }
    }

    // Schema Traversal Helpers

    fn traverse_schema_object<'b, F>(
        cursor: &mut Cursor<&'b [u8]>,
        schema: &'a Schema,
        mut cb: F,
    ) -> Result<()>
    where
        F: FnMut(&str, &'b [u8], Option<&'a Schema>) -> Result<()>,
    {
        let properties = schema.properties.as_ref();

        if let Some(required) = &schema.required {
            for key in required {
                let sub_schema = properties.and_then(|p| p.get(key));
                let start = cursor.position() as usize;
                Self::skip_value(cursor, sub_schema)?;
                let end = cursor.position() as usize;
                let val_slice = &cursor.get_ref()[start..end];
                cb(key, val_slice, sub_schema)?;
            }
        }

        let count = read_uvarint(cursor);
        for _ in 0..count {
            let k_len = read_uvarint(cursor) as usize;
            let k_bytes = read_bytes(cursor, k_len);
            let k = std::str::from_utf8(k_bytes).unwrap_or("");

            let sub_schema = properties.and_then(|p| p.get(k));
            let start = cursor.position() as usize;
            Self::skip_value(cursor, sub_schema)?;
            let end = cursor.position() as usize;
            let val_slice = &cursor.get_ref()[start..end];

            cb(k, val_slice, sub_schema)?;
        }
        Ok(())
    }

    fn traverse_schema_array<'b, F>(
        cursor: &mut Cursor<&'b [u8]>,
        schema: &'a Schema,
        cb: F,
    ) -> Result<()>
    where
        F: FnMut(usize, &'b [u8], Option<&'a Schema>) -> Result<()>,
    {
        let (len, mut cursor) = Self::peek_schema_array_length(cursor.get_ref(), schema)?;
        Self::traverse_schema_array_content(&mut cursor, len, schema, cb)
    }

    fn peek_schema_array_length<'b>(
        buf: &'b [u8],
        schema: &Schema,
    ) -> Result<(usize, Cursor<&'b [u8]>)> {
        let mut cursor = Cursor::new(buf);
        let encoded_len = read_uvarint(&mut cursor);
        let len = if let Some(min) = schema.min_items {
            encoded_len + min
        } else {
            encoded_len
        };
        Ok((len as usize, cursor))
    }

    fn traverse_schema_array_content<'b, F>(
        cursor: &mut Cursor<&'b [u8]>,
        len: usize,
        schema: &'a Schema,
        mut cb: F,
    ) -> Result<()>
    where
        F: FnMut(usize, &'b [u8], Option<&'a Schema>) -> Result<()>,
    {
        for i in 0..len {
            let mut sub_schema = None;
            if let Some(prefix_items) = &schema.prefix_items {
                if i < prefix_items.len() {
                    sub_schema = Some(&prefix_items[i]);
                }
            }
            if sub_schema.is_none() {
                if let Some(items) = &schema.items {
                    sub_schema = Some(items);
                }
            }

            let start = cursor.position() as usize;
            Self::skip_value(cursor, sub_schema)?;
            let end = cursor.position() as usize;
            let val_slice = &cursor.get_ref()[start..end];

            cb(i, val_slice, sub_schema)?;
        }
        Ok(())
    }

    fn skip_value(cursor: &mut Cursor<&[u8]>, schema: Option<&Schema>) -> Result<()> {
        if let Some(schema) = schema {
            if schema.const_value.is_some() {
                return Ok(());
            }
            if let Some(_enums) = &schema.enum_values {
                let _idx = read_uvarint(cursor);
                return Ok(());
            }

            match &schema.instance_type {
                Some(SingleOrVec::Single(instance_type)) => {
                    Self::skip_typed_value(cursor, instance_type, schema)?;
                    return Ok(());
                }
                Some(SingleOrVec::Vec(types)) => {
                    let pos = cursor.position();
                    let tag = read_byte(cursor);
                    if tag == TAG_NUMBER
                        && (types.contains(&InstanceType::Integer)
                            || types.contains(&InstanceType::Number))
                    {
                        let inner_tag = read_byte(cursor);
                        if inner_tag == TAG_OPTIMIZED_NUMBER {
                            let _ = read_uvarint128(cursor);
                            return Ok(());
                        }
                    }
                    cursor.set_position(pos);
                }
                None => {}
            }
        }
        Self::skip_untyped_value(cursor)
    }

    fn skip_typed_value(
        cursor: &mut Cursor<&[u8]>,
        instance_type: &InstanceType,
        schema: &Schema,
    ) -> Result<()> {
        match instance_type {
            InstanceType::Null => {}
            InstanceType::Boolean => {
                let _ = read_byte(cursor);
            }
            InstanceType::Number | InstanceType::Integer => {
                let tag = read_byte(cursor);
                if tag == TAG_OPTIMIZED_NUMBER {
                    let _ = read_uvarint128(cursor);
                } else {
                    cursor.set_position(cursor.position() - 1);
                    let len = read_uvarint(cursor) as usize;
                    let _ = read_bytes(cursor, len);
                }
            }
            InstanceType::String => {
                if let Some(format) = &schema.format {
                    if format == "date" {
                        let tag = read_byte(cursor);
                        if tag == TAG_DATE_COMPRESSED {
                            let _ = read_bytes(cursor, 4);
                            return Ok(());
                        }
                    } else if format == "time" {
                        let tag = read_byte(cursor);
                        if tag == TAG_TIME_COMPRESSED {
                            let _ = read_bytes(cursor, 10);
                            return Ok(());
                        }
                    } else if format == "date-time" {
                        let tag = read_byte(cursor);
                        if tag == TAG_DATE_TIME_COMPRESSED {
                            let _ = read_bytes(cursor, 14);
                            return Ok(());
                        }
                    } else if format == "ipv4" {
                        let tag = read_byte(cursor);
                        if tag == TAG_IPV4_COMPRESSED {
                            let _ = read_bytes(cursor, 4);
                            return Ok(());
                        }
                    } else if format == "ipv6" {
                        let tag = read_byte(cursor);
                        if tag == TAG_IPV6_COMPRESSED {
                            let _ = read_bytes(cursor, 16);
                            return Ok(());
                        }
                    } else if format == "uuid" {
                        let tag = read_byte(cursor);
                        if tag == TAG_UUID_COMPRESSED {
                            let _ = read_bytes(cursor, 16);
                            return Ok(());
                        }
                    }
                } else if schema.pattern_prefix.is_some() || schema.pattern_suffix.is_some() {
                    let tag = read_byte(cursor);
                    if tag == TAG_PATTERN_COMPRESSED {
                        let len = read_uvarint(cursor) as usize;
                        let _ = read_bytes(cursor, len);
                        return Ok(());
                    }
                }

                let len = read_uvarint(cursor) as usize;
                let _ = read_bytes(cursor, len);
            }
            InstanceType::Object => {
                let properties = schema.properties.as_ref();

                if let Some(required) = &schema.required {
                    for key in required {
                        let sub_schema = properties.and_then(|p| p.get(key));
                        Self::skip_value(cursor, sub_schema)?;
                    }
                }
                let count = read_uvarint(cursor);
                for _ in 0..count {
                    let k_len = read_uvarint(cursor) as usize;
                    let k_bytes = read_bytes(cursor, k_len);
                    let k = std::str::from_utf8(k_bytes).unwrap_or("");
                    let sub_schema = properties.and_then(|p| p.get(k));
                    Self::skip_value(cursor, sub_schema)?;
                }
            }
            InstanceType::Array => {
                let (len, _) = Self::peek_schema_array_length(cursor.get_ref(), schema)?;
                let _ = read_uvarint(cursor);

                for i in 0..len {
                    let mut sub_schema = None;
                    if let Some(prefix_items) = &schema.prefix_items {
                        if i < prefix_items.len() {
                            sub_schema = Some(&prefix_items[i]);
                        }
                    }
                    if sub_schema.is_none() {
                        if let Some(items) = &schema.items {
                            sub_schema = Some(items);
                        }
                    }
                    Self::skip_value(cursor, sub_schema)?;
                }
            }
        }
        Ok(())
    }

    fn skip_untyped_value(cursor: &mut Cursor<&[u8]>) -> Result<()> {
        let tag = read_byte(cursor);
        match tag {
            TAG_NULL | TAG_BOOL_FALSE | TAG_BOOL_TRUE => {}
            TAG_NUMBER | TAG_STRING => {
                let len = read_uvarint(cursor) as usize;
                let _ = read_bytes(cursor, len);
            }
            TAG_ARRAY => {
                let len = read_uvarint(cursor);
                for _ in 0..len {
                    Self::skip_untyped_value(cursor)?;
                }
            }
            TAG_OBJECT => {
                let len = read_uvarint(cursor);
                for _ in 0..len {
                    let k_len = read_uvarint(cursor) as usize;
                    let _ = read_bytes(cursor, k_len);
                    Self::skip_untyped_value(cursor)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
}

fn value_to_value(pv: &PathValue) -> Value<'static> {
    match pv {
        PathValue::Null => Value::Null,
        PathValue::Boolean(b) => Value::Bool(*b),
        PathValue::Number(n) => Value::Number(n.clone()),
        PathValue::String(s) => Value::String(Cow::Owned(s.clone().into_owned())),
        _ => Value::Null,
    }
}
