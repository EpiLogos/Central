mod notification;

use central_connector_sdk::{
    CapabilityProbe, Connector, ConnectorContext, ConnectorManifest, ConnectorPortDeclaration,
    MachineInspectionInput, MachineInspectionOutput, MachineInspector, NativeOpen, NativeOpenInput,
    NativeOpenOutput, NativeReveal, NativeRevealInput, NativeRevealOutput, PortContract, PortError,
    PortErrorCode, TagReadInput, TagReadOutput, TagReplaceInput, TagReplaceOutput, TagStore,
    CONNECTOR_API_VERSION, MACHINE_INSPECTOR_PORT, NATIVE_OPEN_PORT, NATIVE_REVEAL_PORT,
    TAG_STORE_PORT, USER_NOTIFICATION_PORT,
};
use std::collections::BTreeSet;
use std::io::Cursor;
use std::path::Path;
use std::process::Command;

const CONNECTOR_ID: &str = "personal.macos-native";
const FINDER_TAGS_XATTR: &str = "com.apple.metadata:_kMDItemUserTags";

pub struct MacOsNativeConnector {
    manifest: ConnectorManifest,
}

impl MacOsNativeConnector {
    pub fn new() -> Self {
        Self {
            manifest: ConnectorManifest {
                api_version: CONNECTOR_API_VERSION.to_owned(),
                id: CONNECTOR_ID.to_owned(),
                version: "0.2.0".to_owned(),
                display_name: "macOS native host integration".to_owned(),
                ports: [
                    NATIVE_OPEN_PORT,
                    NATIVE_REVEAL_PORT,
                    TAG_STORE_PORT,
                    MACHINE_INSPECTOR_PORT,
                    USER_NOTIFICATION_PORT,
                ]
                .iter()
                .map(|port| ConnectorPortDeclaration {
                    id: port.id.to_owned(),
                    version: port.version.to_owned(),
                })
                .collect(),
                platforms: vec!["macos".to_owned()],
                entrypoint: "rust:central-macos-connectors::MacOsNativeConnector".to_owned(),
                runtime_requirements: vec!["macOS".to_owned()],
                dependency_probes: vec!["/usr/bin/open".to_owned(), "/usr/bin/osascript".to_owned()],
                configuration_requirements: vec!["macOS Notification settings govern notification presentation".to_owned()],
                mutation_scope: "externally-mutating".to_owned(),
            },
        }
    }

    fn ensure_target(target: &Path, operation: &str) -> Result<(), PortError> {
        if target.as_os_str().is_empty() {
            return Err(PortError::new(
                PortErrorCode::InvalidInput,
                format!("{operation} target must be a non-empty path."),
            ));
        }
        if !target.exists() {
            return Err(PortError::new(
                PortErrorCode::InvalidInput,
                format!("{operation} target does not exist: {}", target.display()),
            ));
        }
        Ok(())
    }

    fn run_open(arguments: &[&str], target: &Path, operation: &str) -> Result<(), PortError> {
        Self::ensure_target(target, operation)?;
        let status = Command::new("/usr/bin/open")
            .args(arguments)
            .arg(target)
            .status()
            .map_err(|error| {
                let code = if error.kind() == std::io::ErrorKind::PermissionDenied {
                    PortErrorCode::PermissionFailure
                } else if error.kind() == std::io::ErrorKind::NotFound {
                    PortErrorCode::MissingDependency
                } else {
                    PortErrorCode::ProviderOperationFailed
                };
                PortError::new(code, format!("macOS {operation} failed to start."))
                    .with_provider_detail(error.to_string())
            })?;
        if !status.success() {
            return Err(
                PortError::provider(format!("macOS {operation} command failed."))
                    .with_provider_detail(format!("exit status: {status}")),
            );
        }
        Ok(())
    }

    fn normalize_tags(tags: &[String]) -> Result<Vec<String>, PortError> {
        let mut normalized = BTreeSet::new();
        for tag in tags {
            let tag = tag.trim();
            if tag.is_empty() {
                return Err(PortError::new(
                    PortErrorCode::InvalidInput,
                    "TagStore tags must be non-empty strings.",
                ));
            }
            if tag.contains('\n') {
                return Err(PortError::new(
                    PortErrorCode::InvalidInput,
                    "TagStore semantic tag names must not contain newlines.",
                ));
            }
            normalized.insert(tag.to_owned());
        }
        Ok(normalized.into_iter().collect())
    }

    fn finder_tag_name(raw: &str) -> Option<String> {
        raw.split('\n')
            .next()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
    }
}

impl Default for MacOsNativeConnector {
    fn default() -> Self {
        Self::new()
    }
}

impl NativeOpen for MacOsNativeConnector {
    fn open(&self, input: &NativeOpenInput) -> Result<NativeOpenOutput, PortError> {
        Self::run_open(&[], &input.target, "open")?;
        Ok(NativeOpenOutput { target: input.target.clone() })
    }
}

impl NativeReveal for MacOsNativeConnector {
    fn reveal(&self, input: &NativeRevealInput) -> Result<NativeRevealOutput, PortError> {
        Self::run_open(&["-R"], &input.target, "reveal")?;
        Ok(NativeRevealOutput { target: input.target.clone() })
    }
}

impl TagStore for MacOsNativeConnector {
    fn read(&self, input: &TagReadInput) -> Result<TagReadOutput, PortError> {
        Self::ensure_target(&input.target, "tag read")?;
        let Some(bytes) = xattr::get(&input.target, FINDER_TAGS_XATTR).map_err(|error| {
            let code = if error.kind() == std::io::ErrorKind::PermissionDenied {
                PortErrorCode::PermissionFailure
            } else {
                PortErrorCode::ProviderOperationFailed
            };
            PortError::new(code, "Could not read Finder tag metadata.")
                .with_provider_detail(error.to_string())
        })? else {
            return Ok(TagReadOutput { tags: Vec::new() });
        };

        let raw: Vec<String> = plist::from_reader(Cursor::new(bytes)).map_err(|error| {
            PortError::new(
                PortErrorCode::ProviderOperationFailed,
                "Finder tag metadata is not a supported plist string array.",
            )
            .with_provider_detail(error.to_string())
        })?;
        let mut tags = raw
            .iter()
            .filter_map(|tag| Self::finder_tag_name(tag))
            .collect::<Vec<_>>();
        tags.sort();
        tags.dedup();
        Ok(TagReadOutput { tags })
    }

    fn replace(&self, input: &TagReplaceInput) -> Result<TagReplaceOutput, PortError> {
        Self::ensure_target(&input.target, "tag replace")?;
        let tags = Self::normalize_tags(&input.tags)?;
        let mut bytes = Vec::new();
        plist::to_writer_binary(&mut bytes, &tags).map_err(|error| {
            PortError::new(
                PortErrorCode::UnexpectedConnectorFailure,
                "Could not encode Finder tag metadata.",
            )
            .with_provider_detail(error.to_string())
        })?;
        xattr::set(&input.target, FINDER_TAGS_XATTR, &bytes).map_err(|error| {
            let code = if error.kind() == std::io::ErrorKind::PermissionDenied {
                PortErrorCode::PermissionFailure
            } else {
                PortErrorCode::ProviderOperationFailed
            };
            PortError::new(code, "Could not replace Finder tag metadata.")
                .with_provider_detail(error.to_string())
        })?;
        Ok(TagReplaceOutput { tags })
    }
}

impl MachineInspector for MacOsNativeConnector {
    fn inspect(&self, _input: &MachineInspectionInput) -> Result<MachineInspectionOutput, PortError> {
        Ok(MachineInspectionOutput {
            platform: std::env::consts::OS.to_owned(),
            architecture: std::env::consts::ARCH.to_owned(),
            capabilities: vec![
                MACHINE_INSPECTOR_PORT.id.to_owned(),
                NATIVE_OPEN_PORT.id.to_owned(),
                NATIVE_REVEAL_PORT.id.to_owned(),
                TAG_STORE_PORT.id.to_owned(),
                USER_NOTIFICATION_PORT.id.to_owned(),
            ],
            packages: Vec::new(),
            configurations: Vec::new(),
            services: Vec::new(),
        })
    }
}

impl Connector for MacOsNativeConnector {
    fn manifest(&self) -> &ConnectorManifest {
        &self.manifest
    }

    fn probe(&self, port: &PortContract, context: &ConnectorContext) -> CapabilityProbe {
        if context.platform != "macos" {
            return CapabilityProbe::unavailable(format!(
                "macOS native Connector does not support platform {}.",
                context.platform
            ));
        }
        if !self
            .manifest
            .ports
            .iter()
            .any(|declaration| declaration.id == port.id && declaration.version == port.version)
        {
            return CapabilityProbe::unavailable(format!(
                "macOS native Connector does not implement {} {}.",
                port.id, port.version
            ));
        }
        if matches!(port.id, "NativeOpen" | "NativeReveal") && !Path::new("/usr/bin/open").is_file() {
            return CapabilityProbe::unavailable("Required dependency is missing: /usr/bin/open");
        }
        if port.id == USER_NOTIFICATION_PORT.id && !Path::new("/usr/bin/osascript").is_file() {
            return CapabilityProbe::unavailable("Required dependency is missing: /usr/bin/osascript");
        }
        CapabilityProbe::available()
    }

    fn native_open(&self) -> Option<&dyn NativeOpen> {
        Some(self)
    }

    fn native_reveal(&self) -> Option<&dyn NativeReveal> {
        Some(self)
    }

    fn tag_store(&self) -> Option<&dyn TagStore> {
        Some(self)
    }

    fn machine_inspector(&self) -> Option<&dyn MachineInspector> {
        Some(self)
    }

    fn user_notification(&self) -> Option<&dyn central_connector_sdk::UserNotification> {
        Some(self)
    }
}
