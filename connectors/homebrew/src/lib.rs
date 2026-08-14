use central_connector_sdk::{
    CapabilityProbe, Connector, ConnectorContext, ConnectorManifest, ConnectorPortDeclaration,
    PackageManager, PackageStateRequest, PortContract, PortError, PortErrorCode, StateChangePreview,
    StateChangeResult, CONNECTOR_API_VERSION, PACKAGE_MANAGER_PORT,
};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const CONNECTOR_ID: &str = "personal.homebrew";

pub struct HomebrewConnector {
    manifest: ConnectorManifest,
    executable: PathBuf,
}

impl HomebrewConnector {
    pub fn new() -> Self {
        let executable = std::env::var_os("CENTRAL_BREW_EXECUTABLE")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("brew"));
        Self::with_executable(executable)
    }

    pub fn with_executable(executable: PathBuf) -> Self {
        Self {
            manifest: ConnectorManifest {
                api_version: CONNECTOR_API_VERSION.to_owned(),
                id: CONNECTOR_ID.to_owned(),
                version: "0.1.0".to_owned(),
                display_name: "Homebrew package integration".to_owned(),
                ports: vec![ConnectorPortDeclaration {
                    id: PACKAGE_MANAGER_PORT.id.to_owned(),
                    version: PACKAGE_MANAGER_PORT.version.to_owned(),
                }],
                platforms: vec!["macos".to_owned()],
                entrypoint: "rust:central-homebrew-connector::HomebrewConnector".to_owned(),
                runtime_requirements: vec!["macOS".to_owned()],
                dependency_probes: vec!["brew --version".to_owned()],
                configuration_requirements: Vec::new(),
                mutation_scope: "locally-mutating".to_owned(),
            },
            executable,
        }
    }

    fn validate_id(id: &str) -> Result<&str, PortError> {
        let id = id.trim();
        if id.is_empty() || id.starts_with('-') || id.contains(char::is_whitespace) {
            return Err(PortError::new(
                PortErrorCode::InvalidInput,
                "Homebrew package id must be a non-empty formula name without whitespace or leading '-'.",
            ));
        }
        Ok(id)
    }

    fn run(&self, args: &[&str]) -> Result<Output, PortError> {
        Command::new(&self.executable).args(args).output().map_err(|error| {
            let code = match error.kind() {
                std::io::ErrorKind::NotFound => PortErrorCode::MissingDependency,
                std::io::ErrorKind::PermissionDenied => PortErrorCode::PermissionFailure,
                _ => PortErrorCode::ProviderOperationFailed,
            };
            PortError::new(code, "Homebrew command could not be started.")
                .with_provider_detail(error.to_string())
        })
    }

    fn package_present(&self, id: &str) -> Result<bool, PortError> {
        let output = self.run(&["list", "--formula", "--versions", id])?;
        Ok(output.status.success())
    }

    fn apply_command(&self, args: &[&str], operation: &str) -> Result<(), PortError> {
        let output = self.run(args)?;
        if output.status.success() {
            return Ok(());
        }
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        Err(PortError::provider(format!("Homebrew {operation} failed.")).with_provider_detail(
            if stderr.is_empty() {
                format!("exit status: {}", output.status)
            } else {
                stderr
            },
        ))
    }

    fn dependency_available(&self) -> bool {
        Command::new(&self.executable)
            .arg("--version")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    pub fn executable(&self) -> &Path {
        &self.executable
    }
}

impl Default for HomebrewConnector {
    fn default() -> Self {
        Self::new()
    }
}

impl PackageManager for HomebrewConnector {
    fn preview(&self, input: &PackageStateRequest) -> Result<StateChangePreview, PortError> {
        let id = Self::validate_id(&input.id)?;
        let present = self.package_present(id)?;
        let changed = present != input.present;
        let summary = match (input.present, present) {
            (true, false) => format!("Homebrew formula '{id}' would be installed."),
            (false, true) => format!("Homebrew formula '{id}' would be uninstalled."),
            (true, true) => format!("Homebrew formula '{id}' is already installed."),
            (false, false) => format!("Homebrew formula '{id}' is already absent."),
        };
        Ok(StateChangePreview { changed, summary })
    }

    fn apply(&self, input: &PackageStateRequest) -> Result<StateChangeResult, PortError> {
        let id = Self::validate_id(&input.id)?;
        let preview = self.preview(input)?;
        if !preview.changed {
            return Ok(StateChangeResult {
                changed: false,
                summary: preview.summary,
            });
        }

        if input.present {
            self.apply_command(&["install", "--formula", id], "install")?;
        } else {
            self.apply_command(&["uninstall", "--formula", id], "uninstall")?;
        }

        let after = self.preview(input)?;
        if after.changed {
            return Err(PortError::new(
                PortErrorCode::VerificationFailure,
                format!("Homebrew package state for '{id}' did not match the requested state after apply."),
            ));
        }
        Ok(StateChangeResult {
            changed: true,
            summary: if input.present {
                format!("Homebrew formula '{id}' installed and verified.")
            } else {
                format!("Homebrew formula '{id}' uninstalled and verified.")
            },
        })
    }
}

impl Connector for HomebrewConnector {
    fn manifest(&self) -> &ConnectorManifest {
        &self.manifest
    }

    fn probe(&self, port: &PortContract, context: &ConnectorContext) -> CapabilityProbe {
        if context.platform != "macos" {
            return CapabilityProbe::unavailable(format!(
                "Homebrew Connector does not support platform {}.",
                context.platform
            ));
        }
        if port.id != PACKAGE_MANAGER_PORT.id || port.version != PACKAGE_MANAGER_PORT.version {
            return CapabilityProbe::unavailable(format!(
                "Homebrew Connector does not implement {} {}.",
                port.id, port.version
            ));
        }
        if !self.dependency_available() {
            return CapabilityProbe::unavailable(format!(
                "Required Homebrew executable is unavailable: {}",
                self.executable.display()
            ));
        }
        CapabilityProbe::available()
    }

    fn package_manager(&self) -> Option<&dyn PackageManager> {
        Some(self)
    }
}
