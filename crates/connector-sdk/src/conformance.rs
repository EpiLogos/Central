use crate::connector::{validate_connector_manifest, Connector, ConnectorContext, ConnectorSummary};
use crate::port::{
    ConfigurationStateRequest, MachineInspectionInput, MachineInspectionOutput, PackageStateRequest,
    ServiceStateRequest, StateChangePreview, StateChangeResult, WorkDiscoveryInput,
    CONFIGURATION_MANAGER_PORT, MACHINE_INSPECTOR_PORT, PACKAGE_MANAGER_PORT,
    SERVICE_MANAGER_PORT, WORK_DISCOVERY_PORT,
};
use serde::Serialize;
use std::collections::BTreeSet;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkDiscoveryConformanceFixture {
    pub work_root: PathBuf,
    pub platform: String,
    pub expected_names: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MachineInspectorConformanceFixture {
    pub platform: String,
    pub expected: Option<MachineInspectionOutput>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageManagerConformanceFixture {
    pub platform: String,
    pub request: PackageStateRequest,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigurationManagerConformanceFixture {
    pub platform: String,
    pub request: ConfigurationStateRequest,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceManagerConformanceFixture {
    pub platform: String,
    pub request: ServiceStateRequest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ConformanceReport {
    pub port_id: String,
    pub port_version: String,
    pub connector: ConnectorSummary,
    pub checks: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ConformanceFailure {
    pub check: String,
    pub message: String,
}

impl ConformanceFailure {
    fn new(check: &str, message: impl Into<String>) -> Self {
        Self { check: check.to_owned(), message: message.into() }
    }
}

fn compatible_port(connector: &dyn Connector, id: &str, version: &str) -> Result<(), ConformanceFailure> {
    let declaration = connector
        .manifest()
        .ports
        .iter()
        .find(|port| port.id == id)
        .ok_or_else(|| ConformanceFailure::new("port-compatibility", format!("Connector does not declare {id}.")))?;
    if declaration.version != version {
        return Err(ConformanceFailure::new(
            "port-compatibility",
            format!("Connector declares {id} {}; expected {version}.", declaration.version),
        ));
    }
    Ok(())
}

fn prepare_connector(
    connector: &dyn Connector,
    port: &crate::port::PortContract,
    platform: &str,
) -> Result<(), ConformanceFailure> {
    validate_connector_manifest(connector.manifest())
        .map_err(|error| ConformanceFailure::new("manifest", format!("{}: {}", error.code, error.message)))?;
    compatible_port(connector, port.id, port.version)?;
    let context = ConnectorContext { platform: platform.to_owned() };
    let probe = connector.probe(port, &context);
    if !probe.available {
        return Err(ConformanceFailure::new(
            "probe",
            probe.reason.unwrap_or_else(|| "Capability probe reported unavailable.".to_owned()),
        ));
    }
    Ok(())
}

pub fn run_work_discovery_conformance(
    connector: &dyn Connector,
    fixture: &WorkDiscoveryConformanceFixture,
) -> Result<ConformanceReport, ConformanceFailure> {
    prepare_connector(connector, &WORK_DISCOVERY_PORT, &fixture.platform)?;

    let implementation = connector
        .work_discovery()
        .ok_or_else(|| ConformanceFailure::new("implementation", "Connector does not expose WorkDiscovery implementation."))?;
    let input = WorkDiscoveryInput { work_root: fixture.work_root.clone() };
    let first = implementation
        .list(&input)
        .map_err(|error| ConformanceFailure::new("typed-operation", format!("{:?}: {}", error.code, error.message)))?;
    let second = implementation
        .list(&input)
        .map_err(|error| ConformanceFailure::new("repeat-stability", format!("{:?}: {}", error.code, error.message)))?;

    if first != second {
        return Err(ConformanceFailure::new(
            "repeat-stability",
            "WorkDiscovery.list changed while the fixture source was unchanged.",
        ));
    }

    let mut seen = BTreeSet::new();
    for item in &first.items {
        if item.name.trim().is_empty() || item.path.as_os_str().is_empty() {
            return Err(ConformanceFailure::new("typed-operation", "WorkDiscovery items require non-empty name and path."));
        }
        if !seen.insert(item.name.clone()) {
            return Err(ConformanceFailure::new(
                "typed-operation",
                format!("WorkDiscovery returned duplicate item name: {}", item.name),
            ));
        }
    }

    if let Some(expected_names) = &fixture.expected_names {
        let actual = first.items.iter().map(|item| item.name.clone()).collect::<Vec<_>>();
        if &actual != expected_names {
            return Err(ConformanceFailure::new(
                "expected-items",
                format!("Unexpected Work items: {:?}; expected {:?}.", actual, expected_names),
            ));
        }
    }

    Ok(ConformanceReport {
        port_id: WORK_DISCOVERY_PORT.id.to_owned(),
        port_version: WORK_DISCOVERY_PORT.version.to_owned(),
        connector: ConnectorSummary::from_connector(connector),
        checks: vec![
            "manifest".to_owned(),
            "port-compatibility".to_owned(),
            "probe".to_owned(),
            "typed-operation".to_owned(),
            "repeat-stability".to_owned(),
            "expected-items".to_owned(),
        ],
    })
}

fn ensure_unique_nonempty<'a>(label: &str, values: impl IntoIterator<Item = &'a str>) -> Result<(), ConformanceFailure> {
    let mut seen = BTreeSet::new();
    for value in values {
        if value.trim().is_empty() {
            return Err(ConformanceFailure::new("typed-operation", format!("MachineInspector {label} contains an empty identifier.")));
        }
        if !seen.insert(value) {
            return Err(ConformanceFailure::new(
                "typed-operation",
                format!("MachineInspector {label} contains duplicate identifier: {value}"),
            ));
        }
    }
    Ok(())
}

pub fn run_machine_inspector_conformance(
    connector: &dyn Connector,
    fixture: &MachineInspectorConformanceFixture,
) -> Result<ConformanceReport, ConformanceFailure> {
    prepare_connector(connector, &MACHINE_INSPECTOR_PORT, &fixture.platform)?;

    let implementation = connector
        .machine_inspector()
        .ok_or_else(|| ConformanceFailure::new("implementation", "Connector does not expose MachineInspector implementation."))?;
    let input = MachineInspectionInput::default();
    let first = implementation
        .inspect(&input)
        .map_err(|error| ConformanceFailure::new("typed-operation", format!("{:?}: {}", error.code, error.message)))?;
    let second = implementation
        .inspect(&input)
        .map_err(|error| ConformanceFailure::new("repeat-stability", format!("{:?}: {}", error.code, error.message)))?;
    if first != second {
        return Err(ConformanceFailure::new(
            "repeat-stability",
            "MachineInspector.inspect changed while the fixture source was unchanged.",
        ));
    }
    if first.platform.trim().is_empty() || first.architecture.trim().is_empty() {
        return Err(ConformanceFailure::new(
            "typed-operation",
            "MachineInspector output requires non-empty platform and architecture.",
        ));
    }
    ensure_unique_nonempty("capabilities", first.capabilities.iter().map(String::as_str))?;
    ensure_unique_nonempty("packages", first.packages.iter().map(|item| item.id.as_str()))?;
    ensure_unique_nonempty("configurations", first.configurations.iter().map(|item| item.id.as_str()))?;
    ensure_unique_nonempty("services", first.services.iter().map(|item| item.id.as_str()))?;

    if let Some(expected) = &fixture.expected {
        if &first != expected {
            return Err(ConformanceFailure::new(
                "expected-observation",
                format!("Unexpected MachineInspector output: {:?}; expected {:?}.", first, expected),
            ));
        }
    }

    Ok(ConformanceReport {
        port_id: MACHINE_INSPECTOR_PORT.id.to_owned(),
        port_version: MACHINE_INSPECTOR_PORT.version.to_owned(),
        connector: ConnectorSummary::from_connector(connector),
        checks: vec![
            "manifest".to_owned(),
            "port-compatibility".to_owned(),
            "probe".to_owned(),
            "typed-operation".to_owned(),
            "repeat-stability".to_owned(),
            "expected-observation".to_owned(),
        ],
    })
}

fn validate_preview(preview: &StateChangePreview, label: &str) -> Result<(), ConformanceFailure> {
    if preview.summary.trim().is_empty() {
        return Err(ConformanceFailure::new(
            "preview",
            format!("{label} preview must provide a non-empty summary."),
        ));
    }
    Ok(())
}

fn validate_apply(result: &StateChangeResult, label: &str) -> Result<(), ConformanceFailure> {
    if result.summary.trim().is_empty() {
        return Err(ConformanceFailure::new(
            "apply",
            format!("{label} apply must provide a non-empty summary."),
        ));
    }
    Ok(())
}

fn reconciliation_report(
    connector: &dyn Connector,
    port: &crate::port::PortContract,
) -> ConformanceReport {
    ConformanceReport {
        port_id: port.id.to_owned(),
        port_version: port.version.to_owned(),
        connector: ConnectorSummary::from_connector(connector),
        checks: vec![
            "manifest".to_owned(),
            "port-compatibility".to_owned(),
            "probe".to_owned(),
            "preview".to_owned(),
            "preview-nonmutating".to_owned(),
            "apply".to_owned(),
            "post-apply-preview".to_owned(),
            "idempotent-apply".to_owned(),
        ],
    }
}

pub fn run_package_manager_conformance(
    connector: &dyn Connector,
    fixture: &PackageManagerConformanceFixture,
) -> Result<ConformanceReport, ConformanceFailure> {
    prepare_connector(connector, &PACKAGE_MANAGER_PORT, &fixture.platform)?;
    let implementation = connector
        .package_manager()
        .ok_or_else(|| ConformanceFailure::new("implementation", "Connector does not expose PackageManager implementation."))?;

    let preview = implementation.preview(&fixture.request)
        .map_err(|error| ConformanceFailure::new("preview", format!("{:?}: {}", error.code, error.message)))?;
    validate_preview(&preview, PACKAGE_MANAGER_PORT.id)?;
    let repeated_preview = implementation.preview(&fixture.request)
        .map_err(|error| ConformanceFailure::new("preview-nonmutating", format!("{:?}: {}", error.code, error.message)))?;
    if repeated_preview != preview {
        return Err(ConformanceFailure::new(
            "preview-nonmutating",
            "PackageManager.preview changed state or returned unstable output before apply.",
        ));
    }

    let applied = implementation.apply(&fixture.request)
        .map_err(|error| ConformanceFailure::new("apply", format!("{:?}: {}", error.code, error.message)))?;
    validate_apply(&applied, PACKAGE_MANAGER_PORT.id)?;
    if applied.changed != preview.changed {
        return Err(ConformanceFailure::new(
            "apply",
            "PackageManager apply changed flag does not match its preview.",
        ));
    }
    let after = implementation.preview(&fixture.request)
        .map_err(|error| ConformanceFailure::new("post-apply-preview", format!("{:?}: {}", error.code, error.message)))?;
    validate_preview(&after, PACKAGE_MANAGER_PORT.id)?;
    if after.changed {
        return Err(ConformanceFailure::new(
            "post-apply-preview",
            "PackageManager preview still requests a change after successful apply.",
        ));
    }
    let repeated = implementation.apply(&fixture.request)
        .map_err(|error| ConformanceFailure::new("idempotent-apply", format!("{:?}: {}", error.code, error.message)))?;
    validate_apply(&repeated, PACKAGE_MANAGER_PORT.id)?;
    if repeated.changed {
        return Err(ConformanceFailure::new(
            "idempotent-apply",
            "PackageManager repeated apply was not stable.",
        ));
    }

    Ok(reconciliation_report(connector, &PACKAGE_MANAGER_PORT))
}

pub fn run_configuration_manager_conformance(
    connector: &dyn Connector,
    fixture: &ConfigurationManagerConformanceFixture,
) -> Result<ConformanceReport, ConformanceFailure> {
    prepare_connector(connector, &CONFIGURATION_MANAGER_PORT, &fixture.platform)?;
    let implementation = connector
        .configuration_manager()
        .ok_or_else(|| ConformanceFailure::new("implementation", "Connector does not expose ConfigurationManager implementation."))?;

    let preview = implementation.preview(&fixture.request)
        .map_err(|error| ConformanceFailure::new("preview", format!("{:?}: {}", error.code, error.message)))?;
    validate_preview(&preview, CONFIGURATION_MANAGER_PORT.id)?;
    let repeated_preview = implementation.preview(&fixture.request)
        .map_err(|error| ConformanceFailure::new("preview-nonmutating", format!("{:?}: {}", error.code, error.message)))?;
    if repeated_preview != preview {
        return Err(ConformanceFailure::new(
            "preview-nonmutating",
            "ConfigurationManager.preview changed state or returned unstable output before apply.",
        ));
    }

    let applied = implementation.apply(&fixture.request)
        .map_err(|error| ConformanceFailure::new("apply", format!("{:?}: {}", error.code, error.message)))?;
    validate_apply(&applied, CONFIGURATION_MANAGER_PORT.id)?;
    if applied.changed != preview.changed {
        return Err(ConformanceFailure::new(
            "apply",
            "ConfigurationManager apply changed flag does not match its preview.",
        ));
    }
    let after = implementation.preview(&fixture.request)
        .map_err(|error| ConformanceFailure::new("post-apply-preview", format!("{:?}: {}", error.code, error.message)))?;
    validate_preview(&after, CONFIGURATION_MANAGER_PORT.id)?;
    if after.changed {
        return Err(ConformanceFailure::new(
            "post-apply-preview",
            "ConfigurationManager preview still requests a change after successful apply.",
        ));
    }
    let repeated = implementation.apply(&fixture.request)
        .map_err(|error| ConformanceFailure::new("idempotent-apply", format!("{:?}: {}", error.code, error.message)))?;
    validate_apply(&repeated, CONFIGURATION_MANAGER_PORT.id)?;
    if repeated.changed {
        return Err(ConformanceFailure::new(
            "idempotent-apply",
            "ConfigurationManager repeated apply was not stable.",
        ));
    }

    Ok(reconciliation_report(connector, &CONFIGURATION_MANAGER_PORT))
}

pub fn run_service_manager_conformance(
    connector: &dyn Connector,
    fixture: &ServiceManagerConformanceFixture,
) -> Result<ConformanceReport, ConformanceFailure> {
    prepare_connector(connector, &SERVICE_MANAGER_PORT, &fixture.platform)?;
    let implementation = connector
        .service_manager()
        .ok_or_else(|| ConformanceFailure::new("implementation", "Connector does not expose ServiceManager implementation."))?;

    let preview = implementation.preview(&fixture.request)
        .map_err(|error| ConformanceFailure::new("preview", format!("{:?}: {}", error.code, error.message)))?;
    validate_preview(&preview, SERVICE_MANAGER_PORT.id)?;
    let repeated_preview = implementation.preview(&fixture.request)
        .map_err(|error| ConformanceFailure::new("preview-nonmutating", format!("{:?}: {}", error.code, error.message)))?;
    if repeated_preview != preview {
        return Err(ConformanceFailure::new(
            "preview-nonmutating",
            "ServiceManager.preview changed state or returned unstable output before apply.",
        ));
    }

    let applied = implementation.apply(&fixture.request)
        .map_err(|error| ConformanceFailure::new("apply", format!("{:?}: {}", error.code, error.message)))?;
    validate_apply(&applied, SERVICE_MANAGER_PORT.id)?;
    if applied.changed != preview.changed {
        return Err(ConformanceFailure::new(
            "apply",
            "ServiceManager apply changed flag does not match its preview.",
        ));
    }
    let after = implementation.preview(&fixture.request)
        .map_err(|error| ConformanceFailure::new("post-apply-preview", format!("{:?}: {}", error.code, error.message)))?;
    validate_preview(&after, SERVICE_MANAGER_PORT.id)?;
    if after.changed {
        return Err(ConformanceFailure::new(
            "post-apply-preview",
            "ServiceManager preview still requests a change after successful apply.",
        ));
    }
    let repeated = implementation.apply(&fixture.request)
        .map_err(|error| ConformanceFailure::new("idempotent-apply", format!("{:?}: {}", error.code, error.message)))?;
    validate_apply(&repeated, SERVICE_MANAGER_PORT.id)?;
    if repeated.changed {
        return Err(ConformanceFailure::new(
            "idempotent-apply",
            "ServiceManager repeated apply was not stable.",
        ));
    }

    Ok(reconciliation_report(connector, &SERVICE_MANAGER_PORT))
}
