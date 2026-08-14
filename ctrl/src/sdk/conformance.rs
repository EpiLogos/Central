use std::path::Path;
use serde::{Deserialize, Serialize};
use super::{ConnectorMetadata, MutationClass, WorkDiscovery, WORK_DISCOVERY_CONTRACT_ID, WORK_DISCOVERY_PORT_ID, validate_connector_metadata};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConformanceReport { pub passed: bool, pub failures: Vec<String> }

pub fn work_discovery<C: WorkDiscovery>(metadata: &ConnectorMetadata, connector: &C, fixture_root: &Path) -> ConformanceReport {
    let mut failures = validate_connector_metadata(metadata);
    if !metadata.declares(WORK_DISCOVERY_PORT_ID, WORK_DISCOVERY_CONTRACT_ID) { failures.push(format!("manifest must declare {WORK_DISCOVERY_CONTRACT_ID}")); }
    if metadata.mutation_scope != MutationClass::ReadOnly { failures.push("WorkDiscovery must be read_only".into()); }
    let probe = connector.probe();
    if !probe.eligible { failures.push(format!("capability probe is ineligible: {}", probe.reasons.join("; "))); }
    match (connector.list(fixture_root), connector.list(fixture_root)) {
        (Ok(first), Ok(second)) => {
            if first != second { failures.push("WorkDiscovery output must be repeatable for an unchanged fixture".into()); }
            for item in first {
                if item.name.trim().is_empty() { failures.push("Work item name must not be empty".into()); }
                if item.path.trim().is_empty() { failures.push("Work item path must not be empty".into()); }
            }
        }
        (Err(error), _) | (_, Err(error)) => failures.push(format!("fixture call failed: {}", error.message)),
    }
    ConformanceReport { passed: failures.is_empty(), failures }
}
