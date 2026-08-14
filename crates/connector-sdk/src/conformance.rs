use crate::connector::{validate_connector_manifest, Connector, ConnectorContext, ConnectorSummary};
use crate::port::{MachineInspectionInput, MachineInspectionOutput, WorkDiscoveryInput, MACHINE_INSPECTOR_PORT, WORK_DISCOVERY_PORT};
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

pub fn run_work_discovery_conformance(
    connector: &dyn Connector,
    fixture: &WorkDiscoveryConformanceFixture,
) -> Result<ConformanceReport, ConformanceFailure> {
    validate_connector_manifest(connector.manifest())
        .map_err(|error| ConformanceFailure::new("manifest", format!("{}: {}", error.code, error.message)))?;
    compatible_port(connector, WORK_DISCOVERY_PORT.id, WORK_DISCOVERY_PORT.version)?;

    let context = ConnectorContext { platform: fixture.platform.clone() };
    let probe = connector.probe(&WORK_DISCOVERY_PORT, &context);
    if !probe.available {
        return Err(ConformanceFailure::new(
            "probe",
            probe.reason.unwrap_or_else(|| "Capability probe reported unavailable.".to_owned()),
        ));
    }

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
    validate_connector_manifest(connector.manifest())
        .map_err(|error| ConformanceFailure::new("manifest", format!("{}: {}", error.code, error.message)))?;
    compatible_port(connector, MACHINE_INSPECTOR_PORT.id, MACHINE_INSPECTOR_PORT.version)?;

    let context = ConnectorContext { platform: fixture.platform.clone() };
    let probe = connector.probe(&MACHINE_INSPECTOR_PORT, &context);
    if !probe.available {
        return Err(ConformanceFailure::new(
            "probe",
            probe.reason.unwrap_or_else(|| "Capability probe reported unavailable.".to_owned()),
        ));
    }
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
