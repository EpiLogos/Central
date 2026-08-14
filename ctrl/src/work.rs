use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::{
    connector::{ConnectorDiagnostics, ConnectorRegistry},
    port::{WorkDiscoveryError, WorkItem},
    result::{ActionResult, FailureCode, ResultStatus},
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkListReport {
    pub items: Vec<WorkItem>,
    pub diagnostics: ConnectorDiagnostics,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct WorkListFailureData {
    diagnostics: ConnectorDiagnostics,
    connector_error: Option<WorkDiscoveryError>,
}

pub fn list(
    connectors: &ConnectorRegistry,
    environment: &str,
    central_root: &Path,
) -> ActionResult {
    let resolution = connectors.resolve_work_discovery(environment);
    let selected = resolution.selected;
    let diagnostics = resolution.diagnostics;

    let Some(connector) = selected else {
        return ActionResult::failure_with_data(
            "work.list",
            ResultStatus::UnavailableCapability,
            FailureCode::UnavailableCapability,
            "no eligible Connector can satisfy WorkDiscovery",
            serde_json::to_value(WorkListFailureData {
                diagnostics,
                connector_error: None,
            })
            .expect("WorkListFailureData serializes"),
        );
    };

    match connector.list(central_root) {
        Ok(items) => ActionResult::success(
            "work.list",
            serde_json::to_value(WorkListReport { items, diagnostics })
                .expect("WorkListReport serializes"),
        ),
        Err(error) => ActionResult::failure_with_data(
            "work.list",
            ResultStatus::ConnectorFailure,
            FailureCode::ConnectorFailure,
            error.message.clone(),
            serde_json::to_value(WorkListFailureData {
                diagnostics,
                connector_error: Some(error),
            })
            .expect("WorkListFailureData serializes"),
        ),
    }
}
