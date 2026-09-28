use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Unknown provider fields preserved at the normalization boundary.
///
/// Fields are stored in a sorted map for stable serialization. The provider's nested JSON values
/// are retained without interpreting them as Forgesync domain state.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ProviderData(BTreeMap<String, Value>);

impl ProviderData {
    /// Creates empty provider extension data.
    pub fn new() -> Self {
        Self::default()
    }

    /// Parses an object of unknown provider fields.
    pub fn from_value(value: Value) -> Result<Self, Value> {
        match value {
            Value::Object(object) => Ok(Self(object.into_iter().collect())),
            other => Err(other),
        }
    }

    /// Inserts a provider extension field, returning any previous value.
    pub fn insert(&mut self, key: impl Into<String>, value: Value) -> Option<Value> {
        self.0.insert(key.into(), value)
    }

    /// Reads one preserved provider extension field.
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.0.get(key)
    }

    /// Returns all preserved provider extension fields.
    pub fn fields(&self) -> &BTreeMap<String, Value> {
        &self.0
    }
}
