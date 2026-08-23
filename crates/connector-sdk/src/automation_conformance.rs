use crate::{
    validate_connector_manifest, AutomationRunInput, Connector, ConnectorContext, ConnectorSummary,
    ConformanceFailure, ConformanceReport, AUTOMATION_PORT,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutomationConformanceFixture {
    pub platform: String,
    pub automation: String,
}

pub fn run_automation_conformance(
    connector: &dyn Connector,
    fixture: &AutomationConformanceFixture,
) -> Result<ConformanceReport, ConformanceFailure> {
    validate_connector_manifest(connector.manifest()).map_err(|error| ConformanceFailure {
        check: "manifest".to_owned(),
        message: format!("{}: {}", error.code, error.message),
    })?;

    let declaration = connector
        .manifest()
        .ports
        .iter()
        .find(|port| port.id == AUTOMATION_PORT.id)
        .ok_or_else(|| ConformanceFailure {
            check: "port-compatibility".to_owned(),
            message: format!("Connector does not declare {}.", AUTOMATION_PORT.id),
        })?;
    if declaration.version != AUTOMATION_PORT.version {
        return Err(ConformanceFailure {
            check: "port-compatibility".to_owned(),
            message: format!(
                "Connector declares {} {}; expected {}.",
                AUTOMATION_PORT.id, declaration.version, AUTOMATION_PORT.version
            ),
        });
    }

    let probe = connector.probe(
        &AUTOMATION_PORT,
        &ConnectorContext {
            platform: fixture.platform.clone(),
        },
    );
    if !probe.available {
        return Err(ConformanceFailure {
            check: "probe".to_owned(),
            message: probe
                .reason
                .unwrap_or_else(|| "Capability probe reported unavailable.".to_owned()),
        });
    }

    if fixture.automation.trim().is_empty() {
        return Err(ConformanceFailure {
            check: "fixture".to_owned(),
            message: "Automation conformance requires a non-empty automation name.".to_owned(),
        });
    }

    let implementation = connector.automation().ok_or_else(|| ConformanceFailure {
        check: "implementation".to_owned(),
        message: "Connector does not expose Automation implementation.".to_owned(),
    })?;
    let output = implementation
        .run(&AutomationRunInput {
            automation: fixture.automation.clone(),
        })
        .map_err(|error| ConformanceFailure {
            check: "typed-operation".to_owned(),
            message: format!("{:?}: {}", error.code, error.message),
        })?;
    if output.automation != fixture.automation {
        return Err(ConformanceFailure {
            check: "typed-operation".to_owned(),
            message: "Automation.run must report the automation it was asked to invoke.".to_owned(),
        });
    }

    Ok(ConformanceReport {
        port_id: AUTOMATION_PORT.id.to_owned(),
        port_version: AUTOMATION_PORT.version.to_owned(),
        connector: ConnectorSummary::from_connector(connector),
        checks: vec![
            "manifest".to_owned(),
            "port-compatibility".to_owned(),
            "probe".to_owned(),
            "typed-operation".to_owned(),
        ],
    })
}
