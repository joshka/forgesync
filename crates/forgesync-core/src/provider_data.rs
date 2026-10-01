//! Provider fields retained outside the normalized domain vocabulary.
//!
//! Opaque provider fields do not define archive identity, evidence completeness, or ordering. Code
//! that consumes a field as a stable rule should promote and validate it as a named domain value.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Unknown provider fields, sorted by key for stable serialization.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ProviderData(BTreeMap<String, Value>);

impl ProviderData {
    pub fn new() -> Self {
        Self::default()
    }

    /// Retains the fields of a JSON object.
    ///
    /// # Errors
    ///
    /// Returns the original value unchanged when it is not an object.
    ///
    /// ```
    /// use forgesync_core::provider_data::ProviderData;
    /// use serde_json::json;
    ///
    /// let data = ProviderData::from_value(json!({"draft": true})).unwrap();
    /// assert_eq!(data.get("draft"), Some(&json!(true)));
    /// assert_eq!(ProviderData::from_value(json!([1, 2])), Err(json!([1, 2])));
    /// ```
    pub fn from_value(value: Value) -> Result<Self, Value> {
        match value {
            Value::Object(object) => Ok(Self(object.into_iter().collect())),
            other => Err(other),
        }
    }

    /// Inserts or replaces one field; a JSON `null` is retained rather than deleting it.
    pub fn insert(&mut self, key: impl Into<String>, value: Value) -> Option<Value> {
        self.0.insert(key.into(), value)
    }

    pub fn get(&self, key: &str) -> Option<&Value> {
        self.0.get(key)
    }

    pub fn fields(&self) -> &BTreeMap<String, Value> {
        &self.0
    }
}

impl From<BTreeMap<String, Value>> for ProviderData {
    fn from(fields: BTreeMap<String, Value>) -> Self {
        Self(fields)
    }
}
