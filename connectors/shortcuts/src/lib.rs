use central_connector_sdk::{
    Automation, AutomationRunInput, AutomationRunOutput, CapabilityProbe, Connector,
    ConnectorContext, ConnectorManifest, ConnectorPortDeclaration, PortContract, PortError,
    PortErrorCode, AUTOMATION_PORT, CONNECTOR_API_VERSION,
};
use std::path::{Path, PathBuf};
use std::process::Command;

pub const SHORTCUTS_CONNECTOR_ID: &str = "personal.macos-shortcuts";

pub struct ShortcutsAutomationConnector {
    manifest: ConnectorManifest,
    executable: PathBuf,
}

impl ShortcutsAutomationConnector {
    pub fn new() -> Self {
        Self::with_executable("/usr/bin/shortcuts")
    }

    pub fn with_executable(path: impl Into<PathBuf>) -> Self {
        Self {
            manifest: ConnectorManifest {
                api_version: CONNECTOR_API_VERSION.to_owned(),
                id: SHORTCUTS_CONNECTOR_ID.to_owned(),
                version: "0.1.0".to_owned(),
                display_name: "macOS Shortcuts automation".to_owned(),
                ports: vec![ConnectorPortDeclaration {
                    id: AUTOMATION_PORT.id.to_owned(),
                    version: AUTOMATION_PORT.version.to_owned(),
                }],
                platforms: vec!["macos".to_owned()],
                entrypoint: "rust:central-shortcuts-connector::ShortcutsAutomationConnector".to_owned(),
                runtime_requirements: vec!["macOS".to_owned()],
                dependency_probes: vec!["/usr/bin/shortcuts".to_owned()],
                configuration_requirements: Vec::new(),
                mutation_scope: "externally-mutating".to_owned(),
            },
            executable: path.into(),
        }
    }

    pub fn executable(&self) -> &Path {
        &self.executable
    }

    fn validate_name(name: &str) -> Result<&str, PortError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(PortError::new(
                PortErrorCode::InvalidInput,
                "Automation name must be non-empty.",
            ));
        }
        Ok(name)
    }
}

impl Default for ShortcutsAutomationConnector {
    fn default() -> Self {
        Self::new()
    }
}

impl Automation for ShortcutsAutomationConnector {
    fn run(&self, input: &AutomationRunInput) -> Result<AutomationRunOutput, PortError> {
        let automation = Self::validate_name(&input.automation)?;
        let output = Command::new(&self.executable)
            .arg("run")
            .arg(automation)
            .output()
            .map_err(|error| {
                let code = match error.kind() {
                    std::io::ErrorKind::NotFound => PortErrorCode::MissingDependency,
                    std::io::ErrorKind::PermissionDenied => PortErrorCode::PermissionFailure,
                    _ => PortErrorCode::ProviderOperationFailed,
                };
                PortError::new(code, "Could not start the Shortcuts command-line provider.")
                    .with_provider_detail(error.to_string())
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
            let detail = if stderr.is_empty() {
                format!("exit status: {}", output.status)
            } else {
                format!("exit status: {}; stderr: {stderr}", output.status)
            };
            return Err(
                PortError::provider(format!("Shortcut '{automation}' did not complete successfully."))
                    .with_provider_detail(detail),
            );
        }

        Ok(AutomationRunOutput {
            automation: automation.to_owned(),
        })
    }
}

impl Connector for ShortcutsAutomationConnector {
    fn manifest(&self) -> &ConnectorManifest {
        &self.manifest
    }

    fn probe(&self, port: &PortContract, context: &ConnectorContext) -> CapabilityProbe {
        if context.platform != "macos" {
            return CapabilityProbe::unavailable(format!(
                "Shortcuts Automation Connector does not support platform {}.",
                context.platform
            ));
        }
        if port.id != AUTOMATION_PORT.id || port.version != AUTOMATION_PORT.version {
            return CapabilityProbe::unavailable(format!(
                "Shortcuts Automation Connector does not implement {} {}.",
                port.id, port.version
            ));
        }
        if !self.executable.is_file() {
            return CapabilityProbe::unavailable(format!(
                "Required Shortcuts executable is missing: {}",
                self.executable.display()
            ));
        }
        CapabilityProbe::available()
    }

    fn automation(&self) -> Option<&dyn Automation> {
        Some(self)
    }
}
