use super::MacOsNativeConnector;
use central_connector_sdk::{
    NotificationAuthorizationState, NotificationCapabilities, NotificationCapabilityRequest,
    NotificationDelivery, NotificationDeliveryState, NotificationRequest, PortError, PortErrorCode,
    UserNotification,
};
use std::process::Command;

const PROVIDER: &str = "macos.applescript-notification-center";
const OSASCRIPT: &str = "/usr/bin/osascript";

fn applescript_string(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\r', "\\r")
        .replace('\n', "\\n")
}

impl UserNotification for MacOsNativeConnector {
    fn capabilities(
        &self,
        _input: &NotificationCapabilityRequest,
    ) -> Result<NotificationCapabilities, PortError> {
        Ok(NotificationCapabilities {
            available: std::path::Path::new(OSASCRIPT).is_file(),
            // AppleScript's display-notification command is governed by the user's
            // Notification Center settings. This Connector does not claim to read or
            // override that policy state.
            authorization: NotificationAuthorizationState::ProviderManaged,
            supports_callback: false,
            supports_urgency: false,
            supports_category: false,
            provider: PROVIDER.to_owned(),
            notes: vec![
                "Presentation is controlled by the user's macOS Notification settings.".to_owned(),
                "Posting does not prove that the user saw, acknowledged or approved the notification.".to_owned(),
            ],
        })
    }

    fn deliver(&self, input: &NotificationRequest) -> Result<NotificationDelivery, PortError> {
        input.validate()?;
        if input.callback.is_some() {
            return Err(PortError::new(
                PortErrorCode::CapabilityUnavailable,
                "The AppleScript notification Connector does not support an explicit callback target.",
            ));
        }

        let body = applescript_string(&input.body);
        let title = applescript_string(&input.title);
        let script = format!("display notification \"{body}\" with title \"{title}\"");
        let status = Command::new(OSASCRIPT)
            .args(["-e", &script])
            .status()
            .map_err(|error| {
                let code = if error.kind() == std::io::ErrorKind::PermissionDenied {
                    PortErrorCode::PermissionFailure
                } else if error.kind() == std::io::ErrorKind::NotFound {
                    PortErrorCode::MissingDependency
                } else {
                    PortErrorCode::ProviderOperationFailed
                };
                PortError::new(code, "macOS notification command failed to start.")
                    .with_provider_detail(error.to_string())
            })?;
        if !status.success() {
            return Err(
                PortError::provider("macOS Notification Center rejected the AppleScript request.")
                    .with_provider_detail(format!("exit status: {status}")),
            );
        }

        let mut unsupported_requested_features = Vec::new();
        if input.urgency.is_some() {
            unsupported_requested_features.push("urgency".to_owned());
        }
        if input.category.is_some() {
            unsupported_requested_features.push("category".to_owned());
        }

        Ok(NotificationDelivery {
            state: NotificationDeliveryState::Posted,
            provider: PROVIDER.to_owned(),
            subject_ref: input.subject_ref.clone(),
            action_ref: input.action_ref.clone(),
            caller_ref: input.caller_ref.clone(),
            human_acknowledgement_observed: false,
            unsupported_requested_features,
            provenance_refs: input.provenance_refs.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_script_control_characters() {
        assert_eq!(applescript_string("a\"b\\c\nd"), "a\\\"b\\\\c\\nd");
    }
}
