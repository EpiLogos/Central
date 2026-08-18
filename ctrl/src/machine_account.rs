use crate::action::{
    ActionAvailability, ActionDescriptor, ActionExecutionContext, ActionInputDefinition,
    ActionOutputDefinition, ActionRegistry, MutationClass,
};
use crate::machine::{read_machine_declaration, MachineDeclaration};
use crate::result::{ActionResult, ResultStatus};
use crate::root::resolve_central_root;
use central_connector_sdk::{MachineInspectionOutput, MACHINE_INSPECTOR_PORT};
use serde::{Deserialize, Serialize};
use serde_json::{json, to_value, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Stable local machine identity. The machine_id is locally generated and
/// deliberately non-sensitive (no serials, hardware UUIDs or MAC addresses);
/// the hostname is a convenience label. The identity survives observation
/// detail changes and is the stable key for derived observation records.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MachineIdentity {
    pub machine_id: String,
    pub hostname: String,
}

/// One persisted observation record under derived local state
/// (`.central/machines/observed/<machine_id>.json`). Never authored Control
/// material and never a Git commit target.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MachineObservationRecord {
    pub machine_id: String,
    pub hostname: String,
    pub observed_at: String,
    pub connector_id: String,
    pub connector_version: String,
    pub observation: MachineInspectionOutput,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DriftStatus {
    Present,
    Missing,
    Changeable,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MachineDriftEntry {
    pub role: String,
    pub kind: String,
    pub id: String,
    pub status: DriftStatus,
    pub intended: Value,
    pub observed: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AuthoredRoleSummary {
    pub role: String,
    pub path: String,
    pub capabilities: Vec<String>,
    pub requirement_count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MachineAccount {
    pub current: MachineIdentity,
    /// Most recent observation, fresh or cached. When the observation could
    /// not be refreshed, `stale` is true and the record is the last capture.
    pub last_observation: Option<MachineObservationRecord>,
    pub observation_stale: bool,
    /// Authored machine intent from Control/machines — read-only here; never
    /// written by observation.
    pub authored: Vec<AuthoredRoleSummary>,
    /// Differences between authored intent and observed reality.
    pub drift: Vec<MachineDriftEntry>,
    /// True when at least one authored role has reconciliation available
    /// through the canonical machine.plan/machine.apply path.
    pub reconciliation_available: bool,
    pub provenance: String,
}

fn timestamp() -> String {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    format!("{}", now.as_secs())
}

fn hostname() -> String {
    std::env::var("HOSTNAME")
        .ok()
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "unknown-host".to_owned())
}

fn machine_identity_path(root: &Path) -> PathBuf {
    root.join(".central").join("machines").join("identity.json")
}

fn observation_dir(root: &Path) -> PathBuf {
    root.join(".central").join("machines").join("observed")
}

fn load_or_create_identity(root: &Path) -> MachineIdentity {
    let path = machine_identity_path(root);
    if let Ok(text) = fs::read_to_string(&path) {
        if let Ok(identity) = serde_json::from_str::<MachineIdentity>(&text) {
            return identity;
        }
    }
    // Locally generated stable id: time + process id, rendered as hex. Not a
    // hardware identifier and not sensitive.
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let identity = MachineIdentity {
        machine_id: format!("central-{:x}-{:x}", nonce, std::process::id()),
        hostname: hostname(),
    };
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
        let _ = fs::write(&path, serde_json::to_string_pretty(&identity).unwrap_or_default());
    }
    identity
}

fn save_observation(root: &Path, identity: &MachineIdentity, connector_id: &str, connector_version: &str, observation: &MachineInspectionOutput) {
    let dir = observation_dir(root);
    let _ = fs::create_dir_all(&dir);
    let record = MachineObservationRecord {
        machine_id: identity.machine_id.clone(),
        hostname: identity.hostname.clone(),
        observed_at: timestamp(),
        connector_id: connector_id.to_owned(),
        connector_version: connector_version.to_owned(),
        observation: observation.clone(),
    };
    let path = dir.join(format!("{}.json", identity.machine_id));
    let _ = fs::write(&path, serde_json::to_string_pretty(&record).unwrap_or_default());
}

fn latest_observation(root: &Path, identity: &MachineIdentity) -> Option<MachineObservationRecord> {
    let path = observation_dir(root).join(format!("{}.json", identity.machine_id));
    let text = fs::read_to_string(&path).ok()?;
    serde_json::from_str(&text).ok()
}

fn authored_roles(root: &Path) -> Vec<AuthoredRoleSummary> {
    let dir = root.join("Control").join("machines");
    let mut roles = Vec::new();
    let Ok(entries) = fs::read_dir(&dir) else {
        return roles;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let Some(role) = name.strip_suffix(".json") else {
            continue;
        };
        if role.is_empty() {
            continue;
        }
        if let Ok(authored) = read_machine_declaration(root, role) {
            roles.push(AuthoredRoleSummary {
                role: authored.declaration.role.clone(),
                path: authored.source.path.display().to_string(),
                capabilities: authored.declaration.capabilities.clone(),
                requirement_count: authored.declaration.requirements.packages.len()
                    + authored.declaration.requirements.configurations.len()
                    + authored.declaration.requirements.services.len(),
            });
        }
    }
    roles.sort_by(|left, right| left.role.cmp(&right.role));
    roles
}

fn capability_present(observation: &MachineInspectionOutput, capability: &str) -> bool {
    observation.capabilities.iter().any(|value| value == capability)
}

fn package_present(observation: &MachineInspectionOutput, id: &str) -> bool {
    observation.packages.iter().any(|package| package.id == id)
}

fn configuration_present(observation: &MachineInspectionOutput, id: &str) -> bool {
    observation.configurations.iter().any(|configuration| configuration.id == id)
}

fn service_present(observation: &MachineInspectionOutput, id: &str) -> bool {
    observation.services.iter().any(|service| service.id == id)
}

fn drift_for_role(
    role: &str,
    declaration: &MachineDeclaration,
    observation: &MachineInspectionOutput,
    drift: &mut Vec<MachineDriftEntry>,
) {
    for capability in &declaration.capabilities {
        let present = capability_present(observation, capability);
        if !present {
            drift.push(MachineDriftEntry {
                role: role.to_owned(),
                kind: "capability".to_owned(),
                id: capability.clone(),
                status: DriftStatus::Missing,
                intended: json!(capability),
                observed: json!(null),
            });
        }
    }
    for package in &declaration.requirements.packages {
        let observed = package_present(observation, &package.id);
        let status = match (&package.state, observed) {
            (crate::machine::PresenceState::Present, true) | (crate::machine::PresenceState::Absent, false) => DriftStatus::Present,
            (crate::machine::PresenceState::Present, false) => DriftStatus::Missing,
            (crate::machine::PresenceState::Absent, true) => DriftStatus::Changeable,
        };
        if status != DriftStatus::Present {
            drift.push(MachineDriftEntry {
                role: role.to_owned(),
                kind: "package".to_owned(),
                id: package.id.clone(),
                status,
                intended: to_value(&package.state).unwrap_or(Value::Null),
                observed: json!(observed),
            });
        }
    }
    for configuration in &declaration.requirements.configurations {
        let observed = configuration_present(observation, &configuration.id);
        let status = match (&configuration.state, observed) {
            (crate::machine::PresenceState::Present, true) | (crate::machine::PresenceState::Absent, false) => DriftStatus::Present,
            (crate::machine::PresenceState::Present, false) => DriftStatus::Missing,
            (crate::machine::PresenceState::Absent, true) => DriftStatus::Changeable,
        };
        if status != DriftStatus::Present {
            drift.push(MachineDriftEntry {
                role: role.to_owned(),
                kind: "configuration".to_owned(),
                id: configuration.id.clone(),
                status,
                intended: to_value(&configuration.state).unwrap_or(Value::Null),
                observed: json!(observed),
            });
        }
    }
    for service in &declaration.requirements.services {
        let observed = service_present(observation, &service.id);
        let mut status = if observed { DriftStatus::Present } else { DriftStatus::Missing };
        if let Some(running) = service.running {
            let running_observed = observation
                .services
                .iter()
                .find(|candidate| candidate.id == service.id)
                .map(|candidate| candidate.running)
                .unwrap_or(false);
            if running != running_observed {
                status = DriftStatus::Changeable;
            }
        }
        if status != DriftStatus::Present {
            drift.push(MachineDriftEntry {
                role: role.to_owned(),
                kind: "service".to_owned(),
                id: service.id.clone(),
                status,
                intended: json!({
                    "running": service.running,
                    "enabled": service.enabled,
                }),
                observed: json!(observed),
            });
        }
    }
}

pub fn compose_account(
    root: &Path,
    fresh: Option<(MachineInspectionOutput, String, String)>,
) -> MachineAccount {
    let identity = load_or_create_identity(root);
    let (record, stale) = match fresh {
        Some((observation, connector_id, connector_version)) => {
            let record = MachineObservationRecord {
                machine_id: identity.machine_id.clone(),
                hostname: identity.hostname.clone(),
                observed_at: timestamp(),
                connector_id,
                connector_version,
                observation,
            };
            save_observation(root, &identity, &record.connector_id, &record.connector_version, &record.observation);
            (Some(record), false)
        }
        None => (latest_observation(root, &identity), true),
    };

    let authored = authored_roles(root);
    let mut drift = Vec::new();
    if let Some(record) = &record {
        let dir = root.join("Control").join("machines");
        if let Ok(entries) = fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                    continue;
                };
                let Some(role) = name.strip_suffix(".json") else {
                    continue;
                };
                if role.is_empty() {
                    continue;
                }
                if let Ok(authored) = read_machine_declaration(root, role) {
                    drift_for_role(role, &authored.declaration, &record.observation, &mut drift);
                }
            }
        }
    }

    let reconciliation_available = !authored.is_empty()
        && drift.iter().any(|entry| entry.status != DriftStatus::Present);

    MachineAccount {
        current: identity,
        last_observation: record,
        observation_stale: stale,
        authored,
        drift,
        reconciliation_available,
        provenance: "observed".to_owned(),
    }
}

fn account_action(
    _registry: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let refresh = input
        .get("refresh")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let root = match resolve_central_root(context.root_options) {
        Ok(root) => root,
        Err(message) => {
            return ActionResult::failure(
                Some("machine.account"),
                ResultStatus::InvalidInput,
                message,
                None,
            );
        }
    };

    let fresh = if refresh {
        let resolution = context
            .connectors
            .resolve(&MACHINE_INSPECTOR_PORT, context.connector_context);
        match resolution.connector.and_then(|connector| connector.machine_inspector()) {
            Some(inspector) => match inspector.inspect(&central_connector_sdk::MachineInspectionInput::default()) {
                Ok(observation) => {
                    let manifest = resolution
                        .connector
                        .map(|connector| connector.manifest().clone())
                        .unwrap_or_else(|| {
                            central_connector_sdk::ConnectorManifest {
                                api_version: String::new(),
                                id: "unknown".to_owned(),
                                version: String::new(),
                                display_name: String::new(),
                                ports: Vec::new(),
                                platforms: Vec::new(),
                                entrypoint: String::new(),
                                runtime_requirements: Vec::new(),
                                dependency_probes: Vec::new(),
                                configuration_requirements: Vec::new(),
                                mutation_scope: String::new(),
                            }
                        });
                    Some((observation, manifest.id, manifest.version))
                }
                Err(_) => None,
            },
            None => None,
        }
    } else {
        None
    };

    let account = compose_account(&root.path, fresh);
    ActionResult::success(
        "machine.account",
        to_value(account).expect("machine account serializes"),
    )
}

pub fn explain_account(data: &Value) -> String {
    let mut lines = Vec::new();
    if let Some(current) = data.get("current") {
        let id = current.get("machine_id").and_then(Value::as_str).unwrap_or_default();
        let host = current.get("hostname").and_then(Value::as_str).unwrap_or_default();
        lines.push(format!("Machine: {host} ({id})"));
    }
    if let Some(observation) = data.get("last_observation") {
        let at = observation.get("observed_at").and_then(Value::as_str).unwrap_or_default();
        let platform = observation.get("observation").and_then(|value| value.get("platform")).and_then(Value::as_str).unwrap_or_default();
        let architecture = observation.get("observation").and_then(|value| value.get("architecture")).and_then(Value::as_str).unwrap_or_default();
        lines.push(format!("Observed: {platform}/{architecture} at {at}"));
    } else {
        lines.push("Observed: none".to_owned());
    }
    if data.get("observation_stale").and_then(Value::as_bool).unwrap_or(false) {
        lines.push("Observation: stale (no MachineInspector Connector available; showing last capture)".to_owned());
    }
    if let Some(authored) = data.get("authored").and_then(Value::as_array) {
        if authored.is_empty() {
            lines.push("Authored roles: none".to_owned());
        } else {
            for role in authored {
                let name = role.get("role").and_then(Value::as_str).unwrap_or_default();
                let requirements = role.get("requirement_count").and_then(Value::as_u64).unwrap_or(0);
                lines.push(format!("Authored role: {name} ({requirements} requirements)"));
            }
        }
    }
    if let Some(drift) = data.get("drift").and_then(Value::as_array) {
        if !drift.is_empty() {
            lines.push(format!("Drift: {} entries", drift.len()));
            for entry in drift.iter().take(8) {
                let role = entry.get("role").and_then(Value::as_str).unwrap_or_default();
                let kind = entry.get("kind").and_then(Value::as_str).unwrap_or_default();
                let id = entry.get("id").and_then(Value::as_str).unwrap_or_default();
                let status = entry.get("status").and_then(Value::as_str).unwrap_or_default();
                lines.push(format!("  {role} {kind} {id}: {status}"));
            }
        } else {
            lines.push("Drift: none".to_owned());
        }
    }
    let reconciliation = data.get("reconciliation_available").and_then(Value::as_bool).unwrap_or(false);
    lines.push(format!(
        "Reconciliation: {}",
        if reconciliation { "available via machine.plan/machine.apply" } else { "not currently needed" }
    ));
    lines.join("\n")
}

pub(crate) fn register_account_action(registry: &mut ActionRegistry) {
    let refresh = ActionInputDefinition {
        name: "refresh".to_owned(),
        input_type: "boolean".to_owned(),
        required: false,
        choices: None,
        selection: None,
    };
    registry
        .register(
            ActionDescriptor {
                id: "machine.account".to_owned(),
                title: "Current machine account".to_owned(),
                description: "Compose the current-machine account: stable identity, latest observed state with provenance, authored role declarations, drift and reconciliation availability, without conflating authored intent with observed reality.".to_owned(),
                inputs: vec![refresh],
                output: ActionOutputDefinition { output_type: "machine-account".to_owned() },
                mutation_class: MutationClass::ReadOnly,
                preview_supported: false,
                required_ports: vec![MACHINE_INSPECTOR_PORT.id.to_owned()],
                availability: ActionAvailability { available: true, reason: None },
            },
            account_action,
        )
        .expect("machine Action id is valid");
}
