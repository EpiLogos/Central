use central_connector_sdk::{
    CapabilityProbe, ConfigurationManager, ConfigurationStateRequest, Connector, ConnectorContext,
    ConnectorManifest, ConnectorPortDeclaration, MachineInspectionInput, MachineInspectionOutput,
    MachineInspector, ObservedConfiguration, ObservedPackage, ObservedService, PackageManager,
    PackageStateRequest, PortContract, PortError, PortErrorCode, StateChangePreview,
    StateChangeResult, CONFIGURATION_MANAGER_PORT, CONNECTOR_API_VERSION, MACHINE_INSPECTOR_PORT,
    PACKAGE_MANAGER_PORT,
};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

pub const UBUNTU_CONNECTOR_ID: &str = "personal.ubuntu-server";

pub struct UbuntuServerConnector {
    manifest: ConnectorManifest,
}

impl UbuntuServerConnector {
    pub fn new() -> Self {
        Self {
            manifest: ConnectorManifest {
                api_version: CONNECTOR_API_VERSION.to_owned(),
                id: UBUNTU_CONNECTOR_ID.to_owned(),
                version: "0.1.0".to_owned(),
                display_name: "Ubuntu server integration".to_owned(),
                ports: [MACHINE_INSPECTOR_PORT, PACKAGE_MANAGER_PORT, CONFIGURATION_MANAGER_PORT]
                    .iter()
                    .map(|port| ConnectorPortDeclaration {
                        id: port.id.to_owned(),
                        version: port.version.to_owned(),
                    })
                    .collect(),
                platforms: vec!["linux".to_owned()],
                entrypoint: "rust:central-ubuntu-connectors::UbuntuServerConnector".to_owned(),
                runtime_requirements: vec!["Ubuntu".to_owned(), "dpkg".to_owned(), "apt".to_owned()],
                dependency_probes: vec!["/usr/bin/dpkg-query".to_owned(), "/usr/bin/apt-get".to_owned()],
                configuration_requirements: vec![
                    "ConfigurationManager uses an absolute target path as id and source kind=file for materialisation."
                        .to_owned(),
                ],
                mutation_scope: "locally-mutating".to_owned(),
            },
        }
    }

    fn error(code: PortErrorCode, message: impl Into<String>, detail: impl Into<String>) -> PortError {
        let mut error = PortError::new(code, message);
        error.provider_detail = Some(detail.into());
        error
    }

    fn command_output(command: &mut Command, operation: &str) -> Result<Output, PortError> {
        command.output().map_err(|error| {
            let code = match error.kind() {
                std::io::ErrorKind::NotFound => PortErrorCode::MissingDependency,
                std::io::ErrorKind::PermissionDenied => PortErrorCode::PermissionFailure,
                _ => PortErrorCode::ProviderOperationFailed,
            };
            Self::error(code, format!("Ubuntu {operation} could not start."), error.to_string())
        })
    }

    fn os_release_value(key: &str) -> Option<String> {
        let text = fs::read_to_string("/etc/os-release").ok()?;
        text.lines().find_map(|line| {
            let (candidate, value) = line.split_once('=')?;
            (candidate == key).then(|| value.trim().trim_matches('"').to_owned())
        })
    }

    fn is_ubuntu() -> bool {
        std::env::consts::OS == "linux"
            && Self::os_release_value("ID").as_deref() == Some("ubuntu")
    }

    fn validate_package_id(id: &str) -> Result<(), PortError> {
        if id.trim().is_empty()
            || !id
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '+' | '-' | '.' | ':'))
        {
            return Err(PortError::new(
                PortErrorCode::InvalidInput,
                format!("Invalid Debian package id: {id}"),
            ));
        }
        Ok(())
    }

    fn package_present(id: &str) -> Result<bool, PortError> {
        Self::validate_package_id(id)?;
        let output = Self::command_output(
            Command::new("dpkg-query")
                .arg("-W")
                .arg("-f=${Status}")
                .arg(id),
            "package inspection",
        )?;
        if !output.status.success() {
            return Ok(false);
        }
        Ok(String::from_utf8_lossy(&output.stdout).trim() == "install ok installed")
    }

    fn is_root() -> bool {
        let Ok(output) = Command::new("id").arg("-u").output() else {
            return false;
        };
        output.status.success() && String::from_utf8_lossy(&output.stdout).trim() == "0"
    }

    fn run_apt(input: &PackageStateRequest) -> Result<(), PortError> {
        let mut command = if Self::is_root() {
            Command::new("apt-get")
        } else {
            let mut command = Command::new("sudo");
            command.arg("-n").arg("apt-get");
            command
        };
        command.env("DEBIAN_FRONTEND", "noninteractive");
        if input.present {
            command.args(["install", "-y", "--no-install-recommends"]);
        } else {
            command.args(["remove", "-y"]);
        }
        command.arg(&input.id);
        let output = Self::command_output(&mut command, "package reconciliation")?;
        if !output.status.success() {
            let detail = format!(
                "status={} stderr={}",
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            );
            return Err(Self::error(
                PortErrorCode::ProviderOperationFailed,
                format!("apt-get failed while reconciling package {}.", input.id),
                detail,
            ));
        }
        Ok(())
    }

    fn configuration_target(id: &str) -> Result<PathBuf, PortError> {
        let target = PathBuf::from(id);
        if id.trim().is_empty() || !target.is_absolute() {
            return Err(PortError::new(
                PortErrorCode::InvalidInput,
                "Ubuntu configuration ids must be absolute target paths.",
            ));
        }
        Ok(target)
    }

    fn configuration_source(input: &ConfigurationStateRequest) -> Result<PathBuf, PortError> {
        let source = input.source.as_ref().ok_or_else(|| {
            PortError::new(
                PortErrorCode::InvalidConfiguration,
                format!("Configuration {} requires source kind=file when present=true.", input.id),
            )
        })?;
        if source.kind != "file" {
            return Err(PortError::new(
                PortErrorCode::InvalidConfiguration,
                format!(
                    "Ubuntu file configuration expects source kind=file, got {}.",
                    source.kind
                ),
            ));
        }
        let path = PathBuf::from(&source.reference);
        if !path.is_file() {
            return Err(PortError::new(
                PortErrorCode::InvalidConfiguration,
                format!("Configuration source is not a readable file: {}", path.display()),
            ));
        }
        Ok(path)
    }

    fn configuration_changed(input: &ConfigurationStateRequest) -> Result<bool, PortError> {
        let target = Self::configuration_target(&input.id)?;
        if !input.present {
            return Ok(target.exists());
        }
        let source = Self::configuration_source(input)?;
        if !target.exists() {
            return Ok(true);
        }
        if !target.is_file() {
            return Err(PortError::new(
                PortErrorCode::InvalidConfiguration,
                format!("Configuration target is not a file: {}", target.display()),
            ));
        }
        let source_bytes = fs::read(&source).map_err(|error| {
            Self::error(
                PortErrorCode::ProviderOperationFailed,
                format!("Could not read configuration source {}.", source.display()),
                error.to_string(),
            )
        })?;
        let target_bytes = fs::read(&target).map_err(|error| {
            Self::error(
                PortErrorCode::ProviderOperationFailed,
                format!("Could not read configuration target {}.", target.display()),
                error.to_string(),
            )
        })?;
        Ok(source_bytes != target_bytes)
    }

    fn systemd_service(id: &str) -> Result<ObservedService, PortError> {
        if id.trim().is_empty() {
            return Err(PortError::new(
                PortErrorCode::InvalidInput,
                "Service id must be non-empty.",
            ));
        }
        let output = Self::command_output(
            Command::new("systemctl")
                .arg("show")
                .arg(id)
                .arg("--property=LoadState")
                .arg("--property=ActiveState")
                .arg("--property=UnitFileState")
                .arg("--no-pager"),
            "service inspection",
        )?;
        let text = String::from_utf8_lossy(&output.stdout);
        let mut load_state = "not-found";
        let mut active_state = "inactive";
        let mut unit_file_state = "disabled";
        for line in text.lines() {
            if let Some(value) = line.strip_prefix("LoadState=") {
                load_state = value;
            } else if let Some(value) = line.strip_prefix("ActiveState=") {
                active_state = value;
            } else if let Some(value) = line.strip_prefix("UnitFileState=") {
                unit_file_state = value;
            }
        }
        Ok(ObservedService {
            id: id.to_owned(),
            present: load_state != "not-found",
            running: active_state == "active",
            enabled: matches!(unit_file_state, "enabled" | "enabled-runtime" | "static"),
        })
    }
}

impl Default for UbuntuServerConnector {
    fn default() -> Self {
        Self::new()
    }
}

impl MachineInspector for UbuntuServerConnector {
    fn inspect(&self, input: &MachineInspectionInput) -> Result<MachineInspectionOutput, PortError> {
        let mut packages = Vec::with_capacity(input.package_ids.len());
        for id in &input.package_ids {
            packages.push(ObservedPackage {
                id: id.clone(),
                present: Self::package_present(id)?,
            });
        }

        let mut configurations = Vec::with_capacity(input.configuration_ids.len());
        for id in &input.configuration_ids {
            let target = Self::configuration_target(id)?;
            configurations.push(ObservedConfiguration {
                id: id.clone(),
                present: target.exists(),
            });
        }

        let mut services = Vec::with_capacity(input.service_ids.len());
        for id in &input.service_ids {
            services.push(Self::systemd_service(id)?);
        }

        Ok(MachineInspectionOutput {
            platform: std::env::consts::OS.to_owned(),
            architecture: std::env::consts::ARCH.to_owned(),
            capabilities: vec![
                "headless".to_owned(),
                "ubuntu-server".to_owned(),
                MACHINE_INSPECTOR_PORT.id.to_owned(),
                PACKAGE_MANAGER_PORT.id.to_owned(),
                CONFIGURATION_MANAGER_PORT.id.to_owned(),
            ],
            packages,
            configurations,
            services,
        })
    }
}

impl PackageManager for UbuntuServerConnector {
    fn preview(&self, input: &PackageStateRequest) -> Result<StateChangePreview, PortError> {
        let present = Self::package_present(&input.id)?;
        Ok(StateChangePreview {
            changed: present != input.present,
            summary: format!(
                "apt package {} -> {}",
                input.id,
                if input.present { "present" } else { "absent" }
            ),
        })
    }

    fn apply(&self, input: &PackageStateRequest) -> Result<StateChangeResult, PortError> {
        let changed = <Self as PackageManager>::preview(self, input)?.changed;
        if changed {
            Self::run_apt(input)?;
            let observed = Self::package_present(&input.id)?;
            if observed != input.present {
                return Err(PortError::new(
                    PortErrorCode::VerificationFailure,
                    format!("Package {} did not reach the requested state.", input.id),
                ));
            }
        }
        Ok(StateChangeResult {
            changed,
            summary: format!(
                "apt package {} is {}",
                input.id,
                if input.present { "present" } else { "absent" }
            ),
        })
    }
}

impl ConfigurationManager for UbuntuServerConnector {
    fn preview(&self, input: &ConfigurationStateRequest) -> Result<StateChangePreview, PortError> {
        let changed = Self::configuration_changed(input)?;
        Ok(StateChangePreview {
            changed,
            summary: format!(
                "file configuration {} -> {}",
                input.id,
                if input.present { "present" } else { "absent" }
            ),
        })
    }

    fn apply(&self, input: &ConfigurationStateRequest) -> Result<StateChangeResult, PortError> {
        let changed = Self::configuration_changed(input)?;
        let target = Self::configuration_target(&input.id)?;
        if changed && input.present {
            let source = Self::configuration_source(input)?;
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent).map_err(|error| {
                    Self::error(
                        PortErrorCode::PermissionFailure,
                        format!("Could not create configuration directory {}.", parent.display()),
                        error.to_string(),
                    )
                })?;
            }
            fs::copy(&source, &target).map_err(|error| {
                let code = if error.kind() == std::io::ErrorKind::PermissionDenied {
                    PortErrorCode::PermissionFailure
                } else {
                    PortErrorCode::ProviderOperationFailed
                };
                Self::error(
                    code,
                    format!("Could not materialise configuration {}.", target.display()),
                    error.to_string(),
                )
            })?;
        } else if changed {
            fs::remove_file(&target).map_err(|error| {
                let code = if error.kind() == std::io::ErrorKind::PermissionDenied {
                    PortErrorCode::PermissionFailure
                } else {
                    PortErrorCode::ProviderOperationFailed
                };
                Self::error(
                    code,
                    format!("Could not remove configuration {}.", target.display()),
                    error.to_string(),
                )
            })?;
        }
        if Self::configuration_changed(input)? {
            return Err(PortError::new(
                PortErrorCode::VerificationFailure,
                format!("Configuration {} did not reach the requested state.", input.id),
            ));
        }
        Ok(StateChangeResult {
            changed,
            summary: format!(
                "file configuration {} is {}",
                input.id,
                if input.present { "present" } else { "absent" }
            ),
        })
    }
}

impl Connector for UbuntuServerConnector {
    fn manifest(&self) -> &ConnectorManifest {
        &self.manifest
    }

    fn probe(&self, port: &PortContract, context: &ConnectorContext) -> CapabilityProbe {
        if context.platform != "linux" {
            return CapabilityProbe::unavailable(format!(
                "Ubuntu Connector does not support platform {}.",
                context.platform
            ));
        }
        if !Self::is_ubuntu() {
            return CapabilityProbe::unavailable("Linux host is not identified as Ubuntu by /etc/os-release.");
        }
        if !self
            .manifest
            .ports
            .iter()
            .any(|candidate| candidate.id == port.id && candidate.version == port.version)
        {
            return CapabilityProbe::unavailable(format!(
                "Ubuntu Connector does not implement {} {}.",
                port.id, port.version
            ));
        }
        if matches!(port.id, "MachineInspector" | "PackageManager")
            && !Path::new("/usr/bin/dpkg-query").is_file()
        {
            return CapabilityProbe::unavailable("Required dependency is missing: /usr/bin/dpkg-query");
        }
        if port.id == PACKAGE_MANAGER_PORT.id && !Path::new("/usr/bin/apt-get").is_file() {
            return CapabilityProbe::unavailable("Required dependency is missing: /usr/bin/apt-get");
        }
        CapabilityProbe::available()
    }

    fn machine_inspector(&self) -> Option<&dyn MachineInspector> {
        Some(self)
    }

    fn package_manager(&self) -> Option<&dyn PackageManager> {
        Some(self)
    }

    fn configuration_manager(&self) -> Option<&dyn ConfigurationManager> {
        Some(self)
    }
}
