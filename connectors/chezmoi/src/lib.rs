use central_connector_sdk::{
    CapabilityProbe, ConfigurationManager, ConfigurationStateRequest, Connector, ConnectorContext,
    ConnectorManifest, ConnectorPortDeclaration, PortContract, PortError, PortErrorCode,
    StateChangePreview, StateChangeResult, CONNECTOR_API_VERSION, CONFIGURATION_MANAGER_PORT,
};
use std::ffi::OsStr;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Output};

const CONNECTOR_ID: &str = "personal.chezmoi";
const SOURCE_KIND: &str = "chezmoi";

pub struct ChezmoiConnector {
    manifest: ConnectorManifest,
    executable: PathBuf,
    destination: PathBuf,
}

impl ChezmoiConnector {
    pub fn new() -> Self {
        let executable = std::env::var_os("CENTRAL_CHEZMOI_EXECUTABLE")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("chezmoi"));
        let destination = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/"));
        Self::with_paths(executable, destination)
    }

    pub fn with_paths(executable: PathBuf, destination: PathBuf) -> Self {
        Self {
            manifest: ConnectorManifest {
                api_version: CONNECTOR_API_VERSION.to_owned(),
                id: CONNECTOR_ID.to_owned(),
                version: "0.1.0".to_owned(),
                display_name: "chezmoi configuration integration".to_owned(),
                ports: vec![ConnectorPortDeclaration {
                    id: CONFIGURATION_MANAGER_PORT.id.to_owned(),
                    version: CONFIGURATION_MANAGER_PORT.version.to_owned(),
                }],
                platforms: vec!["macos".to_owned()],
                entrypoint: "rust:central-chezmoi-connector::ChezmoiConnector".to_owned(),
                runtime_requirements: vec!["macOS".to_owned()],
                dependency_probes: vec!["chezmoi --version".to_owned()],
                configuration_requirements: vec![
                    "present configuration requests require source.kind=chezmoi and a source-state directory reference"
                        .to_owned(),
                ],
                mutation_scope: "locally-mutating".to_owned(),
            },
            executable,
            destination,
        }
    }

    fn validate_target(&self, id: &str) -> Result<(PathBuf, PathBuf), PortError> {
        let trimmed = id.trim();
        if trimmed.is_empty() {
            return Err(PortError::new(
                PortErrorCode::InvalidInput,
                "chezmoi configuration id must be a non-empty destination path.",
            ));
        }
        let id_path = PathBuf::from(trimmed);
        if id_path.is_absolute() {
            if !id_path.starts_with(&self.destination) {
                return Err(PortError::new(
                    PortErrorCode::InvalidInput,
                    format!(
                        "chezmoi configuration target must remain beneath destination {}.",
                        self.destination.display()
                    ),
                ));
            }
            let relative = id_path
                .strip_prefix(&self.destination)
                .map_err(|error| {
                    PortError::new(PortErrorCode::InvalidInput, "Invalid chezmoi destination target.")
                        .with_provider_detail(error.to_string())
                })?
                .to_path_buf();
            if relative.as_os_str().is_empty() {
                return Err(PortError::new(
                    PortErrorCode::InvalidInput,
                    "chezmoi configuration target may not be the destination root itself.",
                ));
            }
            return Ok((id_path, relative));
        }

        if id_path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        }) {
            return Err(PortError::new(
                PortErrorCode::InvalidInput,
                "chezmoi configuration target may not escape its destination root.",
            ));
        }
        Ok((self.destination.join(&id_path), id_path))
    }

    fn source_directory(input: &ConfigurationStateRequest) -> Result<PathBuf, PortError> {
        let source = input.source.as_ref().ok_or_else(|| {
            PortError::new(
                PortErrorCode::InvalidConfiguration,
                "Present chezmoi configuration requires an authored source reference.",
            )
        })?;
        if source.kind != SOURCE_KIND {
            return Err(PortError::new(
                PortErrorCode::InvalidConfiguration,
                format!(
                    "chezmoi configuration source kind must be '{SOURCE_KIND}', not '{}'.",
                    source.kind
                ),
            ));
        }
        let directory = PathBuf::from(source.reference.trim());
        if !directory.is_dir() {
            return Err(PortError::new(
                PortErrorCode::InvalidConfiguration,
                format!(
                    "chezmoi source-state directory does not exist: {}",
                    directory.display()
                ),
            ));
        }
        Ok(directory)
    }

    fn run<I, S>(&self, args: I) -> Result<Output, PortError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        Command::new(&self.executable).args(args).output().map_err(|error| {
            let code = match error.kind() {
                std::io::ErrorKind::NotFound => PortErrorCode::MissingDependency,
                std::io::ErrorKind::PermissionDenied => PortErrorCode::PermissionFailure,
                _ => PortErrorCode::ProviderOperationFailed,
            };
            PortError::new(code, "chezmoi command could not be started.")
                .with_provider_detail(error.to_string())
        })
    }

    fn present_diff(&self, source: &Path, target: &Path) -> Result<StateChangePreview, PortError> {
        let output = self.run([
            OsStr::new("--source"),
            source.as_os_str(),
            OsStr::new("--destination"),
            self.destination.as_os_str(),
            OsStr::new("--no-tty"),
            OsStr::new("diff"),
            target.as_os_str(),
        ])?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
            return Err(PortError::provider("chezmoi diff failed.").with_provider_detail(
                if stderr.is_empty() {
                    format!("exit status: {}", output.status)
                } else {
                    stderr
                },
            ));
        }
        let changed = !output.stdout.is_empty();
        Ok(StateChangePreview {
            changed,
            summary: if changed {
                format!("chezmoi would update '{}'.", target.display())
            } else {
                format!("chezmoi target '{}' already matches authored source.", target.display())
            },
        })
    }

    fn apply_present(&self, source: &Path, target: &Path) -> Result<(), PortError> {
        let output = self.run([
            OsStr::new("--source"),
            source.as_os_str(),
            OsStr::new("--destination"),
            self.destination.as_os_str(),
            OsStr::new("--no-tty"),
            OsStr::new("--force"),
            OsStr::new("apply"),
            target.as_os_str(),
        ])?;
        if output.status.success() {
            return Ok(());
        }
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        Err(PortError::provider("chezmoi apply failed.").with_provider_detail(
            if stderr.is_empty() {
                format!("exit status: {}", output.status)
            } else {
                stderr
            },
        ))
    }

    fn remove_target(target: &Path) -> Result<(), PortError> {
        let metadata = match fs::symlink_metadata(target) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => {
                return Err(PortError::provider("Could not inspect configuration target before removal.")
                    .with_provider_detail(error.to_string()))
            }
        };
        if metadata.file_type().is_dir() && !metadata.file_type().is_symlink() {
            fs::remove_dir_all(target)
        } else {
            fs::remove_file(target)
        }
        .map_err(|error| {
            let code = if error.kind() == std::io::ErrorKind::PermissionDenied {
                PortErrorCode::PermissionFailure
            } else {
                PortErrorCode::ProviderOperationFailed
            };
            PortError::new(code, "Could not remove configuration target.")
                .with_provider_detail(error.to_string())
        })
    }

    fn dependency_available(&self) -> bool {
        Command::new(&self.executable)
            .arg("--version")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    pub fn destination(&self) -> &Path {
        &self.destination
    }
}

impl Default for ChezmoiConnector {
    fn default() -> Self {
        Self::new()
    }
}

impl ConfigurationManager for ChezmoiConnector {
    fn preview(&self, input: &ConfigurationStateRequest) -> Result<StateChangePreview, PortError> {
        let (target, _) = self.validate_target(&input.id)?;
        if !input.present {
            let changed = target.exists();
            return Ok(StateChangePreview {
                changed,
                summary: if changed {
                    format!("Configuration target '{}' would be removed.", target.display())
                } else {
                    format!("Configuration target '{}' is already absent.", target.display())
                },
            });
        }
        let source = Self::source_directory(input)?;
        self.present_diff(&source, &target)
    }

    fn apply(&self, input: &ConfigurationStateRequest) -> Result<StateChangeResult, PortError> {
        let (target, _) = self.validate_target(&input.id)?;
        let preview = self.preview(input)?;
        if !preview.changed {
            return Ok(StateChangeResult {
                changed: false,
                summary: preview.summary,
            });
        }

        if input.present {
            let source = Self::source_directory(input)?;
            self.apply_present(&source, &target)?;
        } else {
            Self::remove_target(&target)?;
        }

        let after = self.preview(input)?;
        if after.changed {
            return Err(PortError::new(
                PortErrorCode::VerificationFailure,
                format!(
                    "Configuration target '{}' did not match requested state after apply.",
                    target.display()
                ),
            ));
        }
        Ok(StateChangeResult {
            changed: true,
            summary: if input.present {
                format!("chezmoi applied and verified '{}'.", target.display())
            } else {
                format!("Configuration target '{}' removed and verified.", target.display())
            },
        })
    }
}

impl Connector for ChezmoiConnector {
    fn manifest(&self) -> &ConnectorManifest {
        &self.manifest
    }

    fn probe(&self, port: &PortContract, context: &ConnectorContext) -> CapabilityProbe {
        if context.platform != "macos" {
            return CapabilityProbe::unavailable(format!(
                "chezmoi Connector does not support platform {}.",
                context.platform
            ));
        }
        if port.id != CONFIGURATION_MANAGER_PORT.id
            || port.version != CONFIGURATION_MANAGER_PORT.version
        {
            return CapabilityProbe::unavailable(format!(
                "chezmoi Connector does not implement {} {}.",
                port.id, port.version
            ));
        }
        if !self.destination.is_dir() {
            return CapabilityProbe::unavailable(format!(
                "chezmoi destination directory is unavailable: {}",
                self.destination.display()
            ));
        }
        if !self.dependency_available() {
            return CapabilityProbe::unavailable(format!(
                "Required chezmoi executable is unavailable: {}",
                self.executable.display()
            ));
        }
        CapabilityProbe::available()
    }

    fn configuration_manager(&self) -> Option<&dyn ConfigurationManager> {
        Some(self)
    }
}
