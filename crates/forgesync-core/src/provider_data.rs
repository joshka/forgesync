//! Provider fields retained outside the normalized domain vocabulary.
//!
//! [`ProviderData`] stores JSON key-value fields that are useful to preserve but do not warrant a
//! first-class field in [`crate::content`]. Normalization produces the map; archive serialization
//! retains it beside typed content so a future reader can inspect source details.
//!
//! Opaque provider fields do not define archive identity, evidence completeness, or ordering.
//! Those contracts live in [`crate::identity`], [`crate::coverage`], and [`crate::observation`].
//! Avoid extracting workflow policy from this map when a checked domain concept exists.
//!
//! Use this type for loss-aware retention at the provider boundary. Code that consumes a field as
//! a stable rule should promote and validate it as a named domain value.

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

    /// Retains the fields of a JSON object without interpreting their nested values.
    ///
    /// # Errors
    ///
    /// Returns the original value unchanged when it is not an object. Arrays, scalars, and `null`
    /// are not extension maps; callers can retain or report the rejected value at their boundary.
    /// An empty object is valid and creates empty extension data.
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

    /// Inserts or replaces one extension field, returning its previous value when present.
    ///
    /// Keys and values are retained as supplied; this method validates no provider-specific schema.
    /// Use named domain fields for facts that control identity, completeness, or workflow policy.
    /// Inserting a JSON `null` retains a present field rather than deleting it.
    pub fn insert(&mut self, key: impl Into<String>, value: Value) -> Option<Value> {
        self.0.insert(key.into(), value)
    }

    /// Borrows a preserved field, distinguishing an absent key from a present JSON `null`.
    ///
    /// The returned value is provider data, not a checked domain value. Consumers that need a
    /// stable rule must parse and validate it before using it as application policy.
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.0.get(key)
    }

    /// Borrows all preserved fields in lexicographic key order.
    ///
    /// Deterministic top-level ordering follows the sorted map; nested JSON values keep their own
    /// representation. The shared reference prevents bypassing the explicit insertion operation.
    pub fn fields(&self) -> &BTreeMap<String, Value> {
        &self.0
    }
}

impl From<BTreeMap<String, Value>> for ProviderData {
    fn from(fields: BTreeMap<String, Value>) -> Self {
        Self(fields)
    }
}
