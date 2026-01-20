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
