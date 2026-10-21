//! The person's host ports in the shipped CLI (`--features cli-macos-native`).
//!
//! The O:I kernel, the AIKit TUI and agents reach Central through the `ctrl`
//! executable, whose default Connector registry is the reference set: it
//! implements no NativeOpen, NativeReveal or UserNotification, so `work.open`,
//! `work.reveal` and `personal.notify` refused there and answered only under the
//! macOS host surface. On macOS the shipped cut composes exactly those three
//! ports from the macOS native Connector (`central-macos-connectors`), and
//! nothing else of it: its MachineInspector and TagStore stay with the host
//! surface, so no other Action's provider changes under the CLI.
use central_connector_sdk::{
    CapabilityProbe, Connector, ConnectorContext, ConnectorManifest, ConnectorPortDeclaration,
    NativeOpen, NativeReveal, PortContract, UserNotification, NATIVE_OPEN_PORT, NATIVE_REVEAL_PORT,
    USER_NOTIFICATION_PORT,
};
use central_macos_connectors::MacOsNativeConnector;

pub const CLI_HOST_PORTS_CONNECTOR_ID: &str = "macos.cli-host-ports";
const PORTS: [PortContract; 3] = [NATIVE_OPEN_PORT, NATIVE_REVEAL_PORT, USER_NOTIFICATION_PORT];

/// The macOS native Connector, narrowed to the person's host ports.
pub struct CliHostPortsConnector {
    manifest: ConnectorManifest,
    native: MacOsNativeConnector,
}

impl CliHostPortsConnector {
    pub fn new() -> Self {
        let native = MacOsNativeConnector::new();
        let mut manifest = native.manifest().clone();
        manifest.id = CLI_HOST_PORTS_CONNECTOR_ID.to_owned();
        manifest.display_name = "macOS host ports for the CLI (open, reveal, notify)".to_owned();
        manifest.entrypoint = "rust:ctrl::cli_host_ports::CliHostPortsConnector".to_owned();
        manifest.ports = PORTS
            .iter()
            .map(|port| ConnectorPortDeclaration {
                id: port.id.to_owned(),
                version: port.version.to_owned(),
            })
            .collect();
        Self { manifest, native }
    }
}

impl Default for CliHostPortsConnector {
    fn default() -> Self {
        Self::new()
    }
}

impl Connector for CliHostPortsConnector {
    fn manifest(&self) -> &ConnectorManifest {
        &self.manifest
    }

    fn probe(&self, port: &PortContract, context: &ConnectorContext) -> CapabilityProbe {
        if !PORTS
            .iter()
            .any(|own| own.id == port.id && own.version == port.version)
        {
            return CapabilityProbe::unavailable(format!(
                "The CLI's macOS host ports do not include {} {}.",
                port.id, port.version
            ));
        }
        self.native.probe(port, context)
    }

    fn native_open(&self) -> Option<&dyn NativeOpen> {
        self.native.native_open()
    }

    fn native_reveal(&self) -> Option<&dyn NativeReveal> {
        self.native.native_reveal()
    }

    fn user_notification(&self) -> Option<&dyn UserNotification> {
        self.native.user_notification()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use central_connector_sdk::{
        NotificationCapabilityRequest, MACHINE_INSPECTOR_PORT, TAG_STORE_PORT,
    };

    #[test]
    fn the_cli_resolves_open_reveal_and_notify_on_macos_and_nothing_else_of_the_native_connector() {
        let connectors = crate::cli::create_cli_connector_registry();
        let context = ConnectorContext {
            platform: "macos".into(),
        };
        for port in PORTS {
            let resolution = connectors.resolve(&port, &context);
            let connector = resolution
                .connector
                .unwrap_or_else(|| panic!("{} resolves under the CLI", port.id));
            assert_eq!(connector.manifest().id, CLI_HOST_PORTS_CONNECTOR_ID);
        }
        for port in [MACHINE_INSPECTOR_PORT, TAG_STORE_PORT] {
            let resolution = connectors.resolve(&port, &context);
            assert!(resolution
                .connector
                .is_none_or(|connector| connector.manifest().id != CLI_HOST_PORTS_CONNECTOR_ID));
        }
        // Reading the provider's capabilities posts nothing.
        let notification = connectors
            .resolve(&USER_NOTIFICATION_PORT, &context)
            .connector
            .unwrap()
            .user_notification()
            .unwrap()
            .capabilities(&NotificationCapabilityRequest)
            .unwrap();
        assert_eq!(
            notification.provider,
            "macos.applescript-notification-center"
        );
    }
}
