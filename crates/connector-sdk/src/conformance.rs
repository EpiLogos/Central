use crate::connector::{validate_connector_manifest, Connector, ConnectorContext, ConnectorSummary};
use crate::port::{WorkDiscoveryInput, WORK_DISCOVERY_PORT};
use serde::Serialize;
use std::collections::BTreeSet;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkDiscoveryConformanceFixture {
    pub work_root: PathBuf,
    pub platform: String,
    pub expected_names: Option<Vec<String>>,
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

pub fn run_work_discovery_conformance(
    connector: &dyn Connector,
    fixture: &WorkDiscoveryConformanceFixture,
) -> Result<ConformanceReport, ConformanceFailure> {
    validate_connector_manifest(connector.manifest())
        .map_err(|error| ConformanceFailure::new("manifest", format!("{}: {}", error.code, error.message)))?;

    let declaration = connector
        .manifest()
        .ports
        .iter()
        .find(|port| port.id == WORK_DISCOVERY_PORT.id)
        .ok_or_else(|| ConformanceFailure::new("port-compatibility", "Connector does not declare WorkDiscovery."))?;
    if declaration.version != WORK_DISCOVERY_PORT.version {
        return Err(ConformanceFailure::new(
            "port-compatibility",
            format!("Connector declares WorkDiscovery {}; expected {}.", declaration.version, WORK_DISCOVERY_PORT.version),
        ));
    }

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
