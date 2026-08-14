use crate::action::{
    ActionAvailability, ActionDescriptor, ActionExecutionContext, ActionInputDefinition,
    ActionOutputDefinition, ActionRegistry, MutationClass,
};
use crate::control::SourceClass;
use crate::result::{ActionResult, ResultStatus};
use crate::root::resolve_central_root;
use central_connector_sdk::{
    ConnectorDiagnostics, ConnectorSummary, MachineInspectionInput, MachineInspectionOutput,
    PortContract, CONFIGURATION_MANAGER_PORT, MACHINE_INSPECTOR_PORT, PACKAGE_MANAGER_PORT,
    SERVICE_MANAGER_PORT,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, to_value, Value};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

pub const MACHINE_DECLARATION_SCHEMA: &str = "central.machine";
pub const MACHINE_DECLARATION_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PresenceState {
    Present,
    Absent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MachineSourceReference {
    pub kind: String,
    pub reference: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageRequirement {
    pub id: String,
    pub state: PresenceState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<MachineSourceReference>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfigurationRequirement {
    pub id: String,
    pub state: PresenceState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<MachineSourceReference>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceRequirement {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub running: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<MachineSourceReference>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct MachineRequirements {
    #[serde(default)]
    pub packages: Vec<PackageRequirement>,
    #[serde(default)]
    pub configurations: Vec<ConfigurationRequirement>,
    #[serde(default)]
    pub services: Vec<ServiceRequirement>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MachineDeclaration {
    pub schema: String,
    pub version: u32,
    pub role: String,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub requirements: MachineRequirements,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MachineDeclarationSource {
    pub path: PathBuf,
    pub source_class: SourceClass,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AuthoredMachineDeclaration {
    pub declaration: MachineDeclaration,
    pub source: MachineDeclarationSource,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MachineDeclarationError {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
}

impl MachineDeclarationError {
    fn new(
        code: &str,
        message: impl Into<String>,
        path: Option<PathBuf>,
        field: Option<&str>,
    ) -> Self {
        Self {
            code: code.to_owned(),
            message: message.into(),
            path,
            field: field.map(str::to_owned),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MachineObservationSource {
    pub source_class: String,
    pub connector: ConnectorSummary,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ObservedMachine {
    pub observation: MachineInspectionOutput,
    pub source: MachineObservationSource,
    pub diagnostics: ConnectorDiagnostics,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MachinePlanStatus {
    Satisfied,
    Missing,
    Changeable,
    Unsupported,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MachinePlanEntry {
    pub kind: String,
    pub id: String,
    pub status: MachinePlanStatus,
    pub desired: Value,
    pub observed: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub port: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connector: Option<ConnectorSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diagnostics: Option<ConnectorDiagnostics>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct MachinePlanSummary {
    pub satisfied: usize,
    pub missing: usize,
    pub changeable: usize,
    pub unsupported: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MachinePlan {
    pub role: String,
    pub authored: AuthoredMachineDeclaration,
    pub observed: ObservedMachine,
    pub entries: Vec<MachinePlanEntry>,
    pub summary: MachinePlanSummary,
}

fn validate_role_name(role: &str) -> Result<&str, MachineDeclarationError> {
    let role = role.trim();
    if role.is_empty() {
        return Err(MachineDeclarationError::new(
            "invalid_role",
            "Machine role must be non-empty.",
            None,
            Some("role"),
        ));
    }
    if !role.chars().all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
        || matches!(role, "." | "..")
    {
        return Err(MachineDeclarationError::new(
            "invalid_role",
            "Machine role may contain only letters, digits, '.', '-', and '_' and may not be '.' or '..'.",
            None,
            Some("role"),
        ));
    }
    Ok(role)
}

fn validate_nonempty(value: &str, field: &str, path: &Path) -> Result<(), MachineDeclarationError> {
    if value.trim().is_empty() {
        return Err(MachineDeclarationError::new(
            "invalid_declaration",
            format!("{field} must be non-empty."),
            Some(path.to_path_buf()),
            Some(field),
        ));
    }
    Ok(())
}

fn validate_source(
    source: &MachineSourceReference,
    field: &str,
    path: &Path,
) -> Result<(), MachineDeclarationError> {
    validate_nonempty(&source.kind, &format!("{field}.kind"), path)?;
    validate_nonempty(&source.reference, &format!("{field}.reference"), path)
}

fn validate_capabilities(declaration: &MachineDeclaration, path: &Path) -> Result<(), MachineDeclarationError> {
    let mut seen = BTreeSet::new();
    for capability in &declaration.capabilities {
        validate_nonempty(capability, "capabilities[]", path)?;
        if !seen.insert(capability) {
            return Err(MachineDeclarationError::new(
                "duplicate_capability",
                format!("Duplicate machine capability: {capability}"),
                Some(path.to_path_buf()),
                Some("capabilities"),
            ));
        }
    }
    Ok(())
}

fn validate_presence_requirements<T, FId, FSource>(
    kind: &str,
    items: &[T],
    path: &Path,
    id: FId,
    source: FSource,
) -> Result<(), MachineDeclarationError>
where
    FId: Fn(&T) -> &str,
    FSource: Fn(&T) -> Option<&MachineSourceReference>,
{
    let mut seen = BTreeSet::new();
    for (index, item) in items.iter().enumerate() {
        let item_id = id(item);
        validate_nonempty(item_id, &format!("requirements.{kind}[{index}].id"), path)?;
        if !seen.insert(item_id) {
            return Err(MachineDeclarationError::new(
                "duplicate_requirement",
                format!("Duplicate machine {kind} requirement: {item_id}"),
                Some(path.to_path_buf()),
                Some(&format!("requirements.{kind}")),
            ));
        }
        if let Some(source) = source(item) {
            validate_source(source, &format!("requirements.{kind}[{index}].source"), path)?;
        }
    }
    Ok(())
}

fn validate_declaration(
    requested_role: &str,
    declaration: &MachineDeclaration,
    path: &Path,
) -> Result<(), MachineDeclarationError> {
    if declaration.schema != MACHINE_DECLARATION_SCHEMA {
        return Err(MachineDeclarationError::new(
            "unsupported_schema",
            format!(
                "Machine declaration schema '{}' is unsupported; expected '{MACHINE_DECLARATION_SCHEMA}'.",
                declaration.schema
            ),
            Some(path.to_path_buf()),
            Some("schema"),
        ));
    }
    if declaration.version != MACHINE_DECLARATION_VERSION {
        return Err(MachineDeclarationError::new(
            "unsupported_version",
            format!(
                "Machine declaration version {} is unsupported; expected version {MACHINE_DECLARATION_VERSION}.",
                declaration.version
            ),
            Some(path.to_path_buf()),
            Some("version"),
        ));
    }
    validate_nonempty(&declaration.role, "role", path)?;
    if declaration.role != requested_role {
        return Err(MachineDeclarationError::new(
            "role_mismatch",
            format!(
                "Machine declaration role '{}' does not match requested role '{requested_role}'.",
                declaration.role
            ),
            Some(path.to_path_buf()),
            Some("role"),
        ));
    }
    validate_capabilities(declaration, path)?;
    validate_presence_requirements(
        "packages",
        &declaration.requirements.packages,
        path,
        |item| item.id.as_str(),
        |item| item.source.as_ref(),
    )?;
    validate_presence_requirements(
        "configurations",
        &declaration.requirements.configurations,
        path,
        |item| item.id.as_str(),
        |item| item.source.as_ref(),
    )?;

    let mut services = BTreeSet::new();
    for (index, service) in declaration.requirements.services.iter().enumerate() {
        validate_nonempty(&service.id, &format!("requirements.services[{index}].id"), path)?;
        if !services.insert(service.id.as_str()) {
            return Err(MachineDeclarationError::new(
                "duplicate_requirement",
                format!("Duplicate machine service requirement: {}", service.id),
                Some(path.to_path_buf()),
                Some("requirements.services"),
            ));
        }
        if service.running.is_none() && service.enabled.is_none() {
            return Err(MachineDeclarationError::new(
                "invalid_service_requirement",
                format!(
                    "Service requirement '{}' must declare at least one of running or enabled.",
                    service.id
                ),
                Some(path.to_path_buf()),
                Some(&format!("requirements.services[{index}]")),
            ));
        }
        if let Some(source) = &service.source {
            validate_source(source, &format!("requirements.services[{index}].source"), path)?;
        }
    }
    Ok(())
}

pub fn read_machine_declaration(
    central_root: &Path,
    role: &str,
) -> Result<AuthoredMachineDeclaration, MachineDeclarationError> {
    let role = validate_role_name(role)?;
    let relative = PathBuf::from("Control").join("machines").join(format!("{role}.json"));
    let path = central_root.join(&relative);
    let text = fs::read_to_string(&path).map_err(|error| {
        let code = if error.kind() == std::io::ErrorKind::NotFound {
            "missing_declaration"
        } else {
            "read_failure"
        };
        MachineDeclarationError::new(
            code,
            format!("Cannot read machine declaration for {role}: {error}"),
            Some(relative.clone()),
            None,
        )
    })?;
    let raw = serde_json::from_str::<Value>(&text).map_err(|error| {
        MachineDeclarationError::new(
            "invalid_json",
            format!("Machine declaration for {role} is not valid JSON: {error}"),
            Some(relative.clone()),
            None,
        )
    })?;
    let declaration = serde_json::from_value::<MachineDeclaration>(raw).map_err(|error| {
        MachineDeclarationError::new(
            "invalid_declaration",
            format!("Machine declaration for {role} has an invalid structured form: {error}"),
            Some(relative.clone()),
            None,
        )
    })?;
    validate_declaration(role, &declaration, &relative)?;
    Ok(AuthoredMachineDeclaration {
        declaration,
        source: MachineDeclarationSource {
            path: relative,
            source_class: SourceClass::Authored,
        },
    })
}

fn required_role(input: &Value, action: &str) -> Result<String, ActionResult> {
    let Some(role) = input
        .get("role")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Err(ActionResult::failure(
            Some(action),
            ResultStatus::InvalidInput,
            format!("{action} requires role."),
            None,
        ));
    };
    Ok(role.to_owned())
}

fn declaration_failure(action: &str, error: MachineDeclarationError) -> ActionResult {
    ActionResult::failure(
        Some(action),
        ResultStatus::InvalidInput,
        error.message.clone(),
        Some(to_value(error).expect("machine declaration error serializes")),
    )
}

fn declaration_action(
    _registry: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let role = match required_role(input, "machine.declaration") {
        Ok(role) => role,
        Err(result) => return result,
    };
    let root = match resolve_central_root(context.root_options) {
        Ok(root) => root,
        Err(message) => {
            return ActionResult::failure(
                Some("machine.declaration"),
                ResultStatus::InvalidInput,
                message,
                None,
            );
        }
    };
    match read_machine_declaration(&root.path, &role) {
        Ok(declaration) => ActionResult::success(
            "machine.declaration",
            to_value(declaration).expect("machine declaration serializes"),
        ),
        Err(error) => declaration_failure("machine.declaration", error),
    }
}

fn inspect_current_machine(context: &ActionExecutionContext<'_>, action: &str) -> Result<ObservedMachine, ActionResult> {
    let resolution = context.connectors.resolve(&MACHINE_INSPECTOR_PORT, context.connector_context);
    let diagnostics = resolution.diagnostics.clone();
    let Some(connector) = resolution.connector else {
        return Err(ActionResult::failure(
            Some(action),
            ResultStatus::UnavailableCapability,
            format!("No eligible Connector implements {}.", MACHINE_INSPECTOR_PORT.id),
            Some(json!({ "port": MACHINE_INSPECTOR_PORT.id, "diagnostics": diagnostics })),
        ));
    };
    let Some(implementation) = connector.machine_inspector() else {
        return Err(ActionResult::failure(
            Some(action),
            ResultStatus::ConnectorFailure,
            format!("Selected Connector does not expose {} implementation.", MACHINE_INSPECTOR_PORT.id),
            Some(json!({
                "port": MACHINE_INSPECTOR_PORT.id,
                "connector": connector.manifest().id,
                "diagnostics": diagnostics,
            })),
        ));
    };
    match implementation.inspect(&MachineInspectionInput::default()) {
        Ok(observation) => Ok(ObservedMachine {
            observation,
            source: MachineObservationSource {
                source_class: "observed".to_owned(),
                connector: ConnectorSummary::from_connector(connector),
            },
            diagnostics,
        }),
        Err(error) => Err(ActionResult::failure(
            Some(action),
            ResultStatus::ConnectorFailure,
            format!("Connector failed while executing {}.", MACHINE_INSPECTOR_PORT.id),
            Some(json!({
                "port": MACHINE_INSPECTOR_PORT.id,
                "connector": connector.manifest().id,
                "provider_error": error,
                "diagnostics": diagnostics,
            })),
        )),
    }
}

fn inspect_action(
    _registry: &ActionRegistry,
    _input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    match inspect_current_machine(context, "machine.inspect") {
        Ok(observed) => ActionResult::success(
            "machine.inspect",
            to_value(observed).expect("machine observation serializes"),
        ),
        Err(result) => result,
    }
}

fn satisfied_entry(kind: &str, id: &str, desired: Value, observed: Value) -> MachinePlanEntry {
    MachinePlanEntry {
        kind: kind.to_owned(),
        id: id.to_owned(),
        status: MachinePlanStatus::Satisfied,
        desired,
        observed,
        port: None,
        connector: None,
        reason: None,
        diagnostics: None,
    }
}

fn unsupported_entry(kind: &str, id: &str, desired: Value, observed: Value, reason: String) -> MachinePlanEntry {
    MachinePlanEntry {
        kind: kind.to_owned(),
        id: id.to_owned(),
        status: MachinePlanStatus::Unsupported,
        desired,
        observed,
        port: None,
        connector: None,
        reason: Some(reason),
        diagnostics: None,
    }
}

fn difference_entry(
    kind: &str,
    id: &str,
    desired: Value,
    observed: Value,
    port: &PortContract,
    context: &ActionExecutionContext<'_>,
) -> MachinePlanEntry {
    let resolution = context.connectors.resolve(port, context.connector_context);
    let connector = resolution.diagnostics.selected_connector.clone();
    let status = if connector.is_some() {
        MachinePlanStatus::Changeable
    } else {
        MachinePlanStatus::Missing
    };
    let reason = if connector.is_none() {
        Some(format!(
            "{kind} requirement '{id}' differs from observed state, but no eligible {} Connector is available.",
            port.id
        ))
    } else {
        None
    };
    MachinePlanEntry {
        kind: kind.to_owned(),
        id: id.to_owned(),
        status,
        desired,
        observed,
        port: Some(port.id.to_owned()),
        connector,
        reason,
        diagnostics: Some(resolution.diagnostics),
    }
}

fn compare_machine(
    authored: AuthoredMachineDeclaration,
    observed: ObservedMachine,
    context: &ActionExecutionContext<'_>,
) -> MachinePlan {
    let declaration = &authored.declaration;
    let observation = &observed.observation;
    let mut entries = Vec::new();

    for capability in &declaration.capabilities {
        if observation.capabilities.iter().any(|value| value == capability) {
            entries.push(satisfied_entry(
                "capability",
                capability,
                json!({ "available": true }),
                json!({ "available": true }),
            ));
        } else {
            entries.push(unsupported_entry(
                "capability",
                capability,
                json!({ "available": true }),
                json!({ "available": false }),
                format!(
                    "Required capability '{capability}' is not observed and no general reconciliation Port is defined for this capability intent."
                ),
            ));
        }
    }

    for requirement in &declaration.requirements.packages {
        let desired_present = requirement.state == PresenceState::Present;
        let desired = to_value(requirement).expect("package requirement serializes");
        let Some(item) = observation.packages.iter().find(|item| item.id == requirement.id) else {
            entries.push(unsupported_entry(
                "package",
                &requirement.id,
                desired,
                Value::Null,
                format!(
                    "MachineInspector did not report package state for '{}'; current state cannot be compared.",
                    requirement.id
                ),
            ));
            continue;
        };
        let actual = json!({ "present": item.present });
        if item.present == desired_present {
            entries.push(satisfied_entry("package", &requirement.id, desired, actual));
        } else {
            entries.push(difference_entry(
                "package",
                &requirement.id,
                desired,
                actual,
                &PACKAGE_MANAGER_PORT,
                context,
            ));
        }
    }

    for requirement in &declaration.requirements.configurations {
        let desired_present = requirement.state == PresenceState::Present;
        let desired = to_value(requirement).expect("configuration requirement serializes");
        let Some(item) = observation.configurations.iter().find(|item| item.id == requirement.id) else {
            entries.push(unsupported_entry(
                "configuration",
                &requirement.id,
                desired,
                Value::Null,
                format!(
                    "MachineInspector did not report configuration state for '{}'; current state cannot be compared.",
                    requirement.id
                ),
            ));
            continue;
        };
        let actual = json!({ "present": item.present });
        if item.present == desired_present {
            entries.push(satisfied_entry("configuration", &requirement.id, desired, actual));
        } else {
            entries.push(difference_entry(
                "configuration",
                &requirement.id,
                desired,
                actual,
                &CONFIGURATION_MANAGER_PORT,
                context,
            ));
        }
    }

    for requirement in &declaration.requirements.services {
        let desired = to_value(requirement).expect("service requirement serializes");
        let Some(item) = observation.services.iter().find(|item| item.id == requirement.id) else {
            entries.push(unsupported_entry(
                "service",
                &requirement.id,
                desired,
                Value::Null,
                format!(
                    "MachineInspector did not report service state for '{}'; current state cannot be compared.",
                    requirement.id
                ),
            ));
            continue;
        };
        let actual = json!({
            "present": item.present,
            "running": item.running,
            "enabled": item.enabled,
        });
        let matches_running = requirement.running.map_or(true, |desired| desired == item.running);
        let matches_enabled = requirement.enabled.map_or(true, |desired| desired == item.enabled);
        if matches_running && matches_enabled {
            entries.push(satisfied_entry("service", &requirement.id, desired, actual));
        } else {
            entries.push(difference_entry(
                "service",
                &requirement.id,
                desired,
                actual,
                &SERVICE_MANAGER_PORT,
                context,
            ));
        }
    }

    let mut summary = MachinePlanSummary::default();
    for entry in &entries {
        match entry.status {
            MachinePlanStatus::Satisfied => summary.satisfied += 1,
            MachinePlanStatus::Missing => summary.missing += 1,
            MachinePlanStatus::Changeable => summary.changeable += 1,
            MachinePlanStatus::Unsupported => summary.unsupported += 1,
        }
    }

    MachinePlan {
        role: declaration.role.clone(),
        authored,
        observed,
        entries,
        summary,
    }
}

fn plan_action(
    _registry: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let role = match required_role(input, "machine.plan") {
        Ok(role) => role,
        Err(result) => return result,
    };
    let root = match resolve_central_root(context.root_options) {
        Ok(root) => root,
        Err(message) => {
            return ActionResult::failure(
                Some("machine.plan"),
                ResultStatus::InvalidInput,
                message,
                None,
            );
        }
    };
    let authored = match read_machine_declaration(&root.path, &role) {
        Ok(declaration) => declaration,
        Err(error) => return declaration_failure("machine.plan", error),
    };
    let observed = match inspect_current_machine(context, "machine.plan") {
        Ok(observed) => observed,
        Err(result) => return result,
    };
    ActionResult::success(
        "machine.plan",
        to_value(compare_machine(authored, observed, context)).expect("machine plan serializes"),
    )
}

fn role_input() -> ActionInputDefinition {
    ActionInputDefinition {
        name: "role".to_owned(),
        input_type: "string".to_owned(),
        required: true,
        choices: None,
        selection: None,
    }
}

pub(crate) fn register_machine_actions(registry: &mut ActionRegistry) {
    registry.register(
        ActionDescriptor {
            id: "machine.declaration".to_owned(),
            title: "Read machine declaration".to_owned(),
            description: "Read and validate one versioned authored machine-role declaration from Control/machines.".to_owned(),
            inputs: vec![role_input()],
            output: ActionOutputDefinition { output_type: "authored-machine-declaration".to_owned() },
            mutation_class: MutationClass::ReadOnly,
            preview_supported: false,
            required_ports: Vec::new(),
            availability: ActionAvailability { available: true, reason: None },
        },
        declaration_action,
    ).expect("machine Action id is valid");

    registry.register(
        ActionDescriptor {
            id: "machine.inspect".to_owned(),
            title: "Inspect current machine".to_owned(),
            description: "Collect structured observed host state through the public MachineInspector Port.".to_owned(),
            inputs: Vec::new(),
            output: ActionOutputDefinition { output_type: "observed-machine-state".to_owned() },
            mutation_class: MutationClass::ReadOnly,
            preview_supported: false,
            required_ports: vec![MACHINE_INSPECTOR_PORT.id.to_owned()],
            availability: ActionAvailability { available: true, reason: None },
        },
        inspect_action,
    ).expect("machine Action id is valid");

    registry.register(
        ActionDescriptor {
            id: "machine.plan".to_owned(),
            title: "Plan machine changes".to_owned(),
            description: "Compare authored machine intent with observed host state and produce a non-mutating structured change plan.".to_owned(),
            inputs: vec![role_input()],
            output: ActionOutputDefinition { output_type: "machine-change-plan".to_owned() },
            mutation_class: MutationClass::ReadOnly,
            preview_supported: false,
            required_ports: vec![MACHINE_INSPECTOR_PORT.id.to_owned()],
            availability: ActionAvailability { available: true, reason: None },
        },
        plan_action,
    ).expect("machine Action id is valid");
}

fn render_reference(source: Option<&Value>) -> String {
    let Some(source) = source else { return String::new(); };
    let kind = source.get("kind").and_then(Value::as_str).unwrap_or_default();
    let reference = source.get("reference").and_then(Value::as_str).unwrap_or_default();
    if kind.is_empty() && reference.is_empty() {
        String::new()
    } else {
        format!(" ({kind}: {reference})")
    }
}

pub fn explain_machine_declaration(data: &Value) -> String {
    let declaration = data.get("declaration").unwrap_or(&Value::Null);
    let schema = declaration.get("schema").and_then(Value::as_str).unwrap_or_default();
    let version = declaration.get("version").and_then(Value::as_u64).unwrap_or_default();
    let role = declaration.get("role").and_then(Value::as_str).unwrap_or_default();
    let source = data
        .get("source")
        .and_then(|value| value.get("path"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    let mut lines = vec![
        format!("Machine role: {role}"),
        format!("Declaration: {schema} v{version}"),
        format!("Source: {source} [authored]"),
    ];

    let capabilities = declaration
        .get("capabilities")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if capabilities.is_empty() {
        lines.push("Capabilities: none".to_owned());
    } else {
        lines.push("Capabilities:".to_owned());
        for capability in capabilities {
            lines.push(format!("  - {}", capability.as_str().unwrap_or_default()));
        }
    }

    let requirements = declaration.get("requirements").unwrap_or(&Value::Null);
    for (label, key) in [("Packages", "packages"), ("Configurations", "configurations")] {
        lines.push(format!("{label}:"));
        let items = requirements.get(key).and_then(Value::as_array).cloned().unwrap_or_default();
        if items.is_empty() {
            lines.push("  - none".to_owned());
        } else {
            for item in items {
                let id = item.get("id").and_then(Value::as_str).unwrap_or_default();
                let state = item.get("state").and_then(Value::as_str).unwrap_or_default();
                lines.push(format!("  - {id}: {state}{}", render_reference(item.get("source"))));
            }
        }
    }

    lines.push("Services:".to_owned());
    let services = requirements
        .get("services")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if services.is_empty() {
        lines.push("  - none".to_owned());
    } else {
        for service in services {
            let id = service.get("id").and_then(Value::as_str).unwrap_or_default();
            let mut states = Vec::new();
            if let Some(running) = service.get("running").and_then(Value::as_bool) {
                states.push(format!("running={running}"));
            }
            if let Some(enabled) = service.get("enabled").and_then(Value::as_bool) {
                states.push(format!("enabled={enabled}"));
            }
            lines.push(format!(
                "  - {id}: {}{}",
                states.join(", "),
                render_reference(service.get("source"))
            ));
        }
    }
    lines.join("\n")
}

pub fn explain_machine_inspection(data: &Value) -> String {
    let observation = data.get("observation").unwrap_or(&Value::Null);
    let platform = observation.get("platform").and_then(Value::as_str).unwrap_or_default();
    let architecture = observation.get("architecture").and_then(Value::as_str).unwrap_or_default();
    let connector = data
        .get("source")
        .and_then(|value| value.get("connector"))
        .and_then(|value| value.get("id"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    format!(
        "Observed host: {platform}/{architecture}\nSource: {connector} [observed]\nCapabilities: {}\nPackages reported: {}\nConfigurations reported: {}\nServices reported: {}",
        observation.get("capabilities").and_then(Value::as_array).map_or(0, Vec::len),
        observation.get("packages").and_then(Value::as_array).map_or(0, Vec::len),
        observation.get("configurations").and_then(Value::as_array).map_or(0, Vec::len),
        observation.get("services").and_then(Value::as_array).map_or(0, Vec::len),
    )
}

pub fn explain_machine_plan(data: &Value) -> String {
    let role = data.get("role").and_then(Value::as_str).unwrap_or_default();
    let summary = data.get("summary").unwrap_or(&Value::Null);
    let mut lines = vec![
        format!("Machine plan: {role}"),
        format!(
            "Satisfied: {} | Changeable: {} | Missing: {} | Unsupported: {}",
            summary.get("satisfied").and_then(Value::as_u64).unwrap_or_default(),
            summary.get("changeable").and_then(Value::as_u64).unwrap_or_default(),
            summary.get("missing").and_then(Value::as_u64).unwrap_or_default(),
            summary.get("unsupported").and_then(Value::as_u64).unwrap_or_default(),
        ),
    ];
    if let Some(entries) = data.get("entries").and_then(Value::as_array) {
        for entry in entries {
            let status = entry.get("status").and_then(Value::as_str).unwrap_or_default();
            let kind = entry.get("kind").and_then(Value::as_str).unwrap_or_default();
            let id = entry.get("id").and_then(Value::as_str).unwrap_or_default();
            let port = entry.get("port").and_then(Value::as_str).map(|value| format!(" via {value}")).unwrap_or_default();
            let connector = entry
                .get("connector")
                .and_then(|value| value.get("id"))
                .and_then(Value::as_str)
                .map(|value| format!(" -> {value}"))
                .unwrap_or_default();
            lines.push(format!("[{status}] {kind} {id}{port}{connector}"));
            if let Some(reason) = entry.get("reason").and_then(Value::as_str) {
                lines.push(format!("  {reason}"));
            }
        }
    }
    lines.join("\n")
}
