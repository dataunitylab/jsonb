use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Schema {
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub instance_type: Option<SingleOrVec<InstanceType>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub properties: Option<BTreeMap<String, Schema>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<BTreeSet<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub minimum: Option<i128>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub maximum: Option<i128>,
    #[serde(rename = "enum", skip_serializing_if = "Option::is_none")]
    pub enum_values: Option<Vec<serde_json::Value>>,
    #[serde(rename = "const", skip_serializing_if = "Option::is_none")]
    pub const_value: Option<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
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
