use crate::{
    validate_connector_manifest, Connector, ConnectorContext, ConnectorSummary,
    MachineInspectionInput, MachineInspectionOutput, MACHINE_INSPECTOR_PORT,
};
use serde::Serialize;
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopedMachineInspectorConformanceFixture {
    pub platform: String,
    pub input: MachineInspectionInput,
    pub expected: MachineInspectionOutput,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ScopedMachineInspectorConformanceReport {
    pub port_id: String,
    pub port_version: String,
    pub connector: ConnectorSummary,
    pub checks: Vec<String>,
}

fn ensure_unique_nonempty<'a>(
    label: &str,
    values: impl IntoIterator<Item = &'a str>,
) -> Result<(), String> {
    let mut seen = BTreeSet::new();
    for value in values {
        if value.trim().is_empty() {
            return Err(format!("MachineInspector {label} contains an empty identifier."));
        }
        if !seen.insert(value) {
            return Err(format!(
                "MachineInspector {label} contains duplicate identifier: {value}"
            ));
        }
    }
    Ok(())
}

fn ensure_requested_observations(
    input: &MachineInspectionInput,
    output: &MachineInspectionOutput,
) -> Result<(), String> {
    for id in &input.package_ids {
        if !output.packages.iter().any(|item| &item.id == id) {
            return Err(format!(
                "MachineInspector omitted requested package observation: {id}"
            ));
        }
    }
    for id in &input.configuration_ids {
        if !output.configurations.iter().any(|item| &item.id == id) {
            return Err(format!(
                "MachineInspector omitted requested configuration observation: {id}"
            ));
        }
    }
    for id in &input.service_ids {
        if !output.services.iter().any(|item| &item.id == id) {
            return Err(format!(
                "MachineInspector omitted requested service observation: {id}"
            ));
        }
    }
    Ok(())
}

pub fn run_scoped_machine_inspector_conformance(
    connector: &dyn Connector,
    fixture: &ScopedMachineInspectorConformanceFixture,
) -> Result<ScopedMachineInspectorConformanceReport, String> {
    validate_connector_manifest(connector.manifest())
        .map_err(|error| format!("manifest: {}: {}", error.code, error.message))?;

    let declaration = connector
        .manifest()
        .ports
        .iter()
        .find(|port| port.id == MACHINE_INSPECTOR_PORT.id)
        .ok_or_else(|| {
            format!(
                "port-compatibility: Connector does not declare {}.",
                MACHINE_INSPECTOR_PORT.id
            )
        })?;
    if declaration.version != MACHINE_INSPECTOR_PORT.version {
        return Err(format!(
            "port-compatibility: Connector declares {} {}; expected {}.",
            MACHINE_INSPECTOR_PORT.id, declaration.version, MACHINE_INSPECTOR_PORT.version
        ));
    }

    let context = ConnectorContext {
        platform: fixture.platform.clone(),
    };
    let probe = connector.probe(&MACHINE_INSPECTOR_PORT, &context);
    if !probe.available {
        return Err(format!(
            "probe: {}",
            probe
                .reason
                .unwrap_or_else(|| "Capability probe reported unavailable.".to_owned())
        ));
    }

    let implementation = connector
        .machine_inspector()
        .ok_or_else(|| "implementation: Connector does not expose MachineInspector implementation.".to_owned())?;

    let first = implementation
        .inspect(&fixture.input)
        .map_err(|error| format!("typed-operation: {:?}: {}", error.code, error.message))?;
    let second = implementation
        .inspect(&fixture.input)
        .map_err(|error| format!("repeat-stability: {:?}: {}", error.code, error.message))?;
    if first != second {
        return Err(
            "repeat-stability: MachineInspector.inspect changed while the scoped request was unchanged."
                .to_owned(),
        );
    }
    if first.platform.trim().is_empty() || first.architecture.trim().is_empty() {
        return Err(
            "typed-operation: MachineInspector output requires non-empty platform and architecture."
                .to_owned(),
        );
    }
    ensure_unique_nonempty("capabilities", first.capabilities.iter().map(String::as_str))
        .map_err(|message| format!("typed-operation: {message}"))?;
    ensure_unique_nonempty("packages", first.packages.iter().map(|item| item.id.as_str()))
        .map_err(|message| format!("typed-operation: {message}"))?;
    ensure_unique_nonempty(
        "configurations",
        first.configurations.iter().map(|item| item.id.as_str()),
    )
    .map_err(|message| format!("typed-operation: {message}"))?;
    ensure_unique_nonempty("services", first.services.iter().map(|item| item.id.as_str()))
        .map_err(|message| format!("typed-operation: {message}"))?;
    ensure_requested_observations(&fixture.input, &first)
        .map_err(|message| format!("requested-observations: {message}"))?;

    if first != fixture.expected {
        return Err(format!(
            "expected-observation: Unexpected MachineInspector output: {:?}; expected {:?}.",
            first, fixture.expected
        ));
    }

    Ok(ScopedMachineInspectorConformanceReport {
        port_id: MACHINE_INSPECTOR_PORT.id.to_owned(),
        port_version: MACHINE_INSPECTOR_PORT.version.to_owned(),
        connector: ConnectorSummary::from_connector(connector),
        checks: vec![
            "manifest".to_owned(),
            "port-compatibility".to_owned(),
            "probe".to_owned(),
            "typed-operation".to_owned(),
            "repeat-stability".to_owned(),
            "requested-observations".to_owned(),
            "expected-observation".to_owned(),
        ],
    })
}
