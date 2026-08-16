use crate::port::{PortContract, PortError, PortOperationContract};
use serde::{Deserialize, Serialize};

pub const USER_NOTIFICATION_OPERATIONS: [PortOperationContract; 2] = [
    PortOperationContract {
        name: "capabilities",
        input_type: "NotificationCapabilityRequest",
        output_type: "NotificationCapabilities",
        mutation_class: "read-only",
        preview_required: false,
        idempotent: true,
    },
    PortOperationContract {
        name: "deliver",
        input_type: "NotificationRequest",
        output_type: "NotificationDelivery",
        mutation_class: "externally-mutating",
        preview_required: false,
        idempotent: false,
    },
];

pub const USER_NOTIFICATION_PORT: PortContract = PortContract {
    id: "UserNotification",
    version: "1.0.0",
    purpose: "Request a user-facing host notification without treating delivery as human acknowledgement or approval.",
    operations: &USER_NOTIFICATION_OPERATIONS,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct NotificationCapabilityRequest;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationAuthorizationState {
    Granted,
    Denied,
    NotDetermined,
    ProviderManaged,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotificationCapabilities {
    pub available: bool,
    pub authorization: NotificationAuthorizationState,
    pub supports_callback: bool,
    pub supports_urgency: bool,
    pub supports_category: bool,
    pub provider: String,
    #[serde(default)]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotificationRequest {
    pub title: String,
    pub body: String,
    pub subject_ref: Option<String>,
    pub urgency: Option<String>,
    pub category: Option<String>,
    pub callback: Option<String>,
    pub action_ref: Option<String>,
    pub caller_ref: String,
    #[serde(default)]
    pub provenance_refs: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationDeliveryState {
    Posted,
    Rejected,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotificationDelivery {
    pub state: NotificationDeliveryState,
    pub provider: String,
    pub subject_ref: Option<String>,
    pub action_ref: Option<String>,
    pub caller_ref: String,
    pub human_acknowledgement_observed: bool,
    #[serde(default)]
    pub unsupported_requested_features: Vec<String>,
    #[serde(default)]
    pub provenance_refs: Vec<String>,
}

impl NotificationRequest {
    pub fn validate(&self) -> Result<(), PortError> {
        use crate::port::PortErrorCode;
        if self.title.trim().is_empty() && self.body.trim().is_empty() {
            return Err(PortError::new(
                PortErrorCode::InvalidInput,
                "Notification requires a non-empty title or body.",
            ));
        }
        if self.caller_ref.trim().is_empty() {
            return Err(PortError::new(
                PortErrorCode::InvalidInput,
                "Notification caller_ref must be a non-empty stable reference.",
            ));
        }
        for (label, value) in [
            ("subject_ref", self.subject_ref.as_deref()),
            ("action_ref", self.action_ref.as_deref()),
        ] {
            if value.is_some_and(|value| value.trim().is_empty()) {
                return Err(PortError::new(
                    PortErrorCode::InvalidInput,
                    format!("Notification {label} must be absent or non-empty."),
                ));
            }
        }
        if self.callback.as_deref().is_some_and(|value| {
            let value = value.trim();
            value.is_empty() || !(value.starts_with("oi://") || value.starts_with("https://"))
        }) {
            return Err(PortError::new(
                PortErrorCode::InvalidInput,
                "Notification callback must use an explicit oi:// or https:// target.",
            ));
        }
        Ok(())
    }
}

pub trait UserNotification: Send + Sync {
    fn capabilities(
        &self,
        input: &NotificationCapabilityRequest,
    ) -> Result<NotificationCapabilities, PortError>;

    fn deliver(&self, input: &NotificationRequest) -> Result<NotificationDelivery, PortError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> NotificationRequest {
        NotificationRequest {
            title: "Build complete".into(),
            body: "Candidate B is ready".into(),
            subject_ref: Some("factory:candidate:B".into()),
            urgency: None,
            category: Some("factory".into()),
            callback: Some("oi://candidate/B".into()),
            action_ref: Some("factory.candidate.open".into()),
            caller_ref: "factory:run:42".into(),
            provenance_refs: vec!["factory:evidence:9".into()],
        }
    }

    #[test]
    fn callback_scheme_is_bounded() {
        assert!(request().validate().is_ok());
        let mut bad = request();
        bad.callback = Some("file:///etc/passwd".into());
        assert!(bad.validate().is_err());
    }

    #[test]
    fn caller_lineage_is_required() {
        let mut bad = request();
        bad.caller_ref.clear();
        assert!(bad.validate().is_err());
    }
}
