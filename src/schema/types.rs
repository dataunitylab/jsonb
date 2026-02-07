use crate::Value;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct Schema {
    pub instance_type: Option<SingleOrVec<InstanceType>>,
    pub properties: Option<BTreeMap<String, Schema>>,
    pub required: Option<BTreeSet<String>>,
    pub minimum: Option<i128>,
    pub maximum: Option<i128>,
    pub multiple_of: Option<i128>,
    pub min_items: Option<u64>,
    pub max_items: Option<u64>,
    pub min_length: Option<u64>,
    pub max_length: Option<u64>,
    pub prefix_items: Option<Vec<Schema>>,
    pub items: Option<Box<Schema>>,
    pub format: Option<String>,
    pub pattern: Option<String>,
    pub pattern_prefix: Option<String>,
    pub pattern_suffix: Option<String>,
    pub enum_values: Option<Vec<Value<'static>>>,
    pub const_value: Option<Value<'static>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InstanceType {
    Null,
    Boolean,
    Object,
    Array,
    Number,
    String,
    Integer,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SingleOrVec<T> {
    Single(T),
    Vec(Vec<T>),
}
