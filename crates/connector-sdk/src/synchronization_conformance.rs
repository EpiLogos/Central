use crate::{
    validate_connector_manifest, Connector, ConnectorContext, ConnectorSummary, StateChangePreview,
    SynchronizationRequest, SYNCHRONIZER_PORT,
};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SynchronizerConformanceFixture {
    pub platform: String,
    pub request: SynchronizationRequest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SynchronizerConformanceReport {
    pub port_id: String,
    pub port_version: String,
    pub connector: ConnectorSummary,
    pub checks: Vec<String>,
}

fn require_stable_preview(first: &StateChangePreview, second: &StateChangePreview) -> Result<(), String> {
    if first != second {
        return Err("repeat-preview: Synchronizer preview changed without an intervening apply.".to_owned());
    }
    if first.summary.trim().is_empty() {
        return Err("typed-preview: Synchronizer preview summary must be non-empty.".to_owned());
    }
    Ok(())
}

pub fn run_synchronizer_conformance(
    connector: &dyn Connector,
    fixture: &SynchronizerConformanceFixture,
) -> Result<SynchronizerConformanceReport, String> {
    validate_connector_manifest(connector.manifest())
        .map_err(|error| format!("manifest: {}: {}", error.code, error.message))?;

    let declaration = connector
        .manifest()
        .ports
        .iter()
        .find(|port| port.id == SYNCHRONIZER_PORT.id)
        .ok_or_else(|| {
            format!(
                "port-compatibility: Connector does not declare {}.",
                SYNCHRONIZER_PORT.id
            )
        })?;
    if declaration.version != SYNCHRONIZER_PORT.version {
        return Err(format!(
            "port-compatibility: Connector declares {} {}; expected {}.",
            SYNCHRONIZER_PORT.id, declaration.version, SYNCHRONIZER_PORT.version
        ));
    }

    let context = ConnectorContext {
        platform: fixture.platform.clone(),
    };
    let probe = connector.probe(&SYNCHRONIZER_PORT, &context);
    if !probe.available {
        return Err(format!(
            "probe: {}",
            probe
                .reason
                .unwrap_or_else(|| "Capability probe reported unavailable.".to_owned())
        ));
    }

    let implementation = connector
        .synchronizer()
        .ok_or_else(|| "implementation: Connector does not expose Synchronizer implementation.".to_owned())?;

    let first = implementation
        .preview(&fixture.request)
        .map_err(|error| format!("typed-preview: {:?}: {}", error.code, error.message))?;
    let second = implementation
        .preview(&fixture.request)
        .map_err(|error| format!("repeat-preview: {:?}: {}", error.code, error.message))?;
    require_stable_preview(&first, &second)?;
    if !first.changed {
        return Err(
            "fixture-precondition: Synchronizer conformance fixture must begin in a changeable state so apply behavior is actually exercised."
                .to_owned(),
        );
    }

    let applied = implementation
        .apply(&fixture.request)
        .map_err(|error| format!("typed-apply: {:?}: {}", error.code, error.message))?;
    if !applied.changed {
        return Err(
            "typed-apply: Synchronizer apply must report changed=true when the preceding preview was changeable."
                .to_owned(),
        );
    }
    if applied.summary.trim().is_empty() {
        return Err("typed-apply: Synchronizer apply summary must be non-empty.".to_owned());
    }

    let after = implementation
        .preview(&fixture.request)
        .map_err(|error| format!("post-apply-preview: {:?}: {}", error.code, error.message))?;
    if after.changed {
        return Err("post-apply-preview: Synchronizer remains changeable after successful apply.".to_owned());
    }

    let repeated = implementation
        .apply(&fixture.request)
        .map_err(|error| format!("idempotent-apply: {:?}: {}", error.code, error.message))?;
    if repeated.changed {
        return Err("idempotent-apply: Repeating a satisfied synchronization changed state.".to_owned());
    }

    Ok(SynchronizerConformanceReport {
        port_id: SYNCHRONIZER_PORT.id.to_owned(),
        port_version: SYNCHRONIZER_PORT.version.to_owned(),
        connector: ConnectorSummary::from_connector(connector),
        checks: vec![
            "manifest".to_owned(),
            "port-compatibility".to_owned(),
            "probe".to_owned(),
            "typed-preview".to_owned(),
            "repeat-preview".to_owned(),
            "fixture-precondition".to_owned(),
            "typed-apply".to_owned(),
            "post-apply-preview".to_owned(),
            "idempotent-apply".to_owned(),
        ],
    })
}
