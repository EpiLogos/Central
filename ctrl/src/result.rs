use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ResultStatus {
    Success,
    Cancelled,
    UnavailableCapability,
    InvalidInput,
    InvalidCentralStructure,
    ConnectorFailure,
    PolicyRefusal,
    Partial,
    InternalFailure,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FailureCode {
    InvalidInput,
    InvalidCentralStructure,
    InternalFailure,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Failure {
    pub code: FailureCode,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ActionResult {
    pub action: String,
    pub status: ResultStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<Failure>,
}

impl ActionResult {
    pub fn success(action: impl Into<String>, data: Value) -> Self {
        Self {
            action: action.into(),
            status: ResultStatus::Success,
            data: Some(data),
            error: None,
        }
    }

    pub fn failure(
        action: impl Into<String>,
        status: ResultStatus,
        code: FailureCode,
        message: impl Into<String>,
    ) -> Self {
        Self {
            action: action.into(),
            status,
            data: None,
            error: Some(Failure {
                code,
                message: message.into(),
            }),
        }
    }

    pub fn failure_with_data(
        action: impl Into<String>,
        status: ResultStatus,
        code: FailureCode,
        message: impl Into<String>,
        data: Value,
    ) -> Self {
        Self {
            action: action.into(),
            status,
            data: Some(data),
            error: Some(Failure {
                code,
                message: message.into(),
            }),
        }
    }
}
