use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ResultStatus {
    Success,
    Cancelled,
    InvalidInput,
    InvalidCentralStructure,
    UnavailableCapability,
    ConnectorFailure,
    PartialCompletion,
    VerificationFailure,
    InternalFailure,
}

impl ResultStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Cancelled => "cancelled",
            Self::InvalidInput => "invalid_input",
            Self::InvalidCentralStructure => "invalid_central_structure",
            Self::UnavailableCapability => "unavailable_capability",
            Self::ConnectorFailure => "connector_failure",
            Self::PartialCompletion => "partial_completion",
            Self::VerificationFailure => "verification_failure",
            Self::InternalFailure => "internal_failure",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ActionError {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ActionResult {
    pub ok: bool,
    pub status: ResultStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ActionError>,
}

impl ActionResult {
    pub fn success(action: &str, data: Value) -> Self {
        Self {
            ok: true,
            status: ResultStatus::Success,
            action: Some(action.to_owned()),
            data: Some(data),
            error: None,
        }
    }

    pub fn cancelled(action: Option<&str>, message: impl Into<String>) -> Self {
        Self::failure(action, ResultStatus::Cancelled, message, None)
    }

    pub fn failure(
        action: Option<&str>,
        status: ResultStatus,
        message: impl Into<String>,
        details: Option<Value>,
    ) -> Self {
        Self {
            ok: false,
            status,
            action: action.map(str::to_owned),
            data: None,
            error: Some(ActionError {
                code: status.as_str().to_owned(),
                message: message.into(),
                details,
            }),
        }
    }
}
