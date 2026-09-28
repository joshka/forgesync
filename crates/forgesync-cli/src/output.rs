use serde::Serialize;

/// Version of the stable Forgesync result envelope.
pub const JSON_SCHEMA_VERSION: u32 = 1;

/// Stable structured error information included in a JSON result.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ErrorOutput {
    /// Machine-readable error classification.
    pub code: String,
    /// Human-readable summary that does not expose secrets or private payloads.
    pub message: String,
}

/// Versioned process output for successful, partial, or failed application operations.
#[derive(Clone, Debug, Serialize)]
pub struct JsonEnvelope<T> {
    /// Version of this output contract.
    pub schema_version: u32,
    /// Stable command path that produced this result.
    pub command: String,
    /// Command result, omitted when startup or execution failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    /// Non-fatal, actionable notes.
    pub warnings: Vec<String>,
    /// Structured failure, when present.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ErrorOutput>,
}

impl<T> JsonEnvelope<T> {
    /// Creates a successful result envelope.
    pub fn success(command: impl Into<String>, data: T) -> Self {
        Self {
            schema_version: JSON_SCHEMA_VERSION,
            command: command.into(),
            data: Some(data),
            warnings: Vec::new(),
            error: None,
        }
    }

    /// Creates an error envelope for failures after successful argument parsing.
    pub fn failure(
        command: impl Into<String>,
        code: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            schema_version: JSON_SCHEMA_VERSION,
            command: command.into(),
            data: None,
            warnings: Vec::new(),
            error: Some(ErrorOutput {
                code: code.into(),
                message: message.into(),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::JsonEnvelope;

    #[test]
    fn failure_uses_versioned_envelope_without_data() {
        let envelope = JsonEnvelope::<serde_json::Value>::failure(
            "archive status",
            "archive_missing",
            "archive path does not exist",
        );
        let value = serde_json::to_value(envelope).expect("envelope should serialize");

        assert_eq!(
            value,
            json!({
                "schema_version": 1,
                "command": "archive status",
                "warnings": [],
                "error": {
                    "code": "archive_missing",
                    "message": "archive path does not exist"
                }
            })
        );
    }

    #[test]
    fn success_includes_data_and_empty_warnings() {
        let envelope = JsonEnvelope::success("archive status", json!({ "repositories": 2 }));
        let value = serde_json::to_value(envelope).expect("envelope should serialize");

        assert_eq!(value["schema_version"], 1);
        assert_eq!(value["data"]["repositories"], 2);
        assert_eq!(value["warnings"], json!([]));
        assert!(value.get("error").is_none());
    }
}
