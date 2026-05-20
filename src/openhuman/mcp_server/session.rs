use serde_json::Value;

const DEFAULT_SOURCE_TYPE: &str = "mcp";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct McpSession {
    client_source_type: Option<String>,
}

impl McpSession {
    pub(crate) fn observe_initialize_params(&mut self, params: &Value) {
        self.client_source_type = params
            .as_object()
            .and_then(|obj| obj.get("clientInfo"))
            .and_then(Value::as_object)
            .and_then(|client_info| client_info.get("name"))
            .and_then(Value::as_str)
            .and_then(Self::normalize_client_name)
            .map(|name| format!("{DEFAULT_SOURCE_TYPE}:{name}"));
    }

    pub(crate) fn source_type(&self) -> &str {
        self.client_source_type
            .as_deref()
            .unwrap_or(DEFAULT_SOURCE_TYPE)
    }

    pub(crate) fn normalize_client_name(raw: &str) -> Option<String> {
        let mut normalized = String::new();
        let mut previous_was_separator = false;

        for ch in raw.trim().chars() {
            if ch.is_ascii_alphanumeric() {
                normalized.push(ch.to_ascii_lowercase());
                previous_was_separator = false;
            } else if !normalized.is_empty() && !previous_was_separator {
                normalized.push('-');
                previous_was_separator = true;
            }
        }

        while normalized.ends_with('-') {
            normalized.pop();
        }

        if normalized.is_empty() {
            None
        } else {
            Some(normalized)
        }
    }
}
