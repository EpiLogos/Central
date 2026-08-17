use crate::action::{
    ActionAvailability, ActionDescriptor, ActionExecutionContext, ActionInputDefinition,
    ActionOutputDefinition, ActionRegistry, MutationClass,
};
use crate::control::SourceClass;
use crate::result::{ActionResult, ResultStatus};
use crate::root::resolve_central_root;
use central_connector_sdk::{
    ConfigurationStateRequest, ConnectorDiagnostics, ConnectorSummary, MachineInspectionInput,
    MachineInspectionOutput, PackageStateRequest, PortContract, PortError,
    ReconciliationSourceReference, ServiceStateRequest, StateChangePreview, StateChangeResult,
    CONFIGURATION_MANAGER_PORT, MACHINE_INSPECTOR_PORT, PACKAGE_MANAGER_PORT,
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
    pub preview: Option<StateChangePreview>,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MachineApplyOutcome {
    Complete,
    Partial,
    Unavailable,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MachineApplyOperation {
    pub kind: String,
    pub id: String,
    pub port: String,
    pub connector: ConnectorSummary,
    pub result: StateChangeResult,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MachineVerification {
    pub satisfied: bool,
    pub plan: MachinePlan,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MachineApplyReport {
    pub outcome: MachineApplyOutcome,
    pub initial_plan: MachinePlan,
    pub operations: Vec<MachineApplyOperation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verification: Option<MachineVerification>,
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

fn inspect_current_machine(
    context: &ActionExecutionContext<'_>,
    action: &str,
) -> Result<ObservedMachine, ActionResult> {
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

fn source_ref(source: Option<&MachineSourceReference>) -> Option<ReconciliationSourceReference> {
    source.map(|source| ReconciliationSourceReference {
        kind: source.kind.clone(),
        reference: source.reference.clone(),
    })
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
        preview: None,
        reason: None,
        diagnostics: None,
    }
}

fn unsupported_entry(
    kind: &str,
    id: &str,
    desired: Value,
    observed: Value,
    reason: String,
) -> MachinePlanEntry {
    MachinePlanEntry {
        kind: kind.to_owned(),
        id: id.to_owned(),
        status: MachinePlanStatus::Unsupported,
        desired,
        observed,
        port: None,
        connector: None,
        preview: None,
        reason: Some(reason),
        diagnostics: None,
    }
}

fn unavailable_difference(
    kind: &str,
    id: &str,
    desired: Value,
    observed: Value,
    port: &PortContract,
    diagnostics: ConnectorDiagnostics,
) -> MachinePlanEntry {
    MachinePlanEntry {
        kind: kind.to_owned(),
        id: id.to_owned(),
        status: MachinePlanStatus::Missing,
        desired,
        observed,
        port: Some(port.id.to_owned()),
        connector: None,
        preview: None,
        reason: Some(format!(
            "{kind} requirement '{id}' differs from observed state, but no eligible {} Connector is available.",
            port.id
        )),
        diagnostics: Some(diagnostics),
    }
}

fn unavailable_source_verification(
    requirement: &ConfigurationRequirement,
    desired: Value,
    observed: Value,
    diagnostics: ConnectorDiagnostics,
) -> MachinePlanEntry {
    MachinePlanEntry {
        kind: "configuration".to_owned(),
        id: requirement.id.clone(),
        status: MachinePlanStatus::Missing,
        desired,
        observed,
        port: Some(CONFIGURATION_MANAGER_PORT.id.to_owned()),
        connector: None,
        preview: None,
        reason: Some(format!(
            "configuration requirement '{}' cannot be verified against its authored source because no eligible {} Connector is available.",
            requirement.id, CONFIGURATION_MANAGER_PORT.id
        )),
        diagnostics: Some(diagnostics),
    }
}

fn preview_failure(
    action: &str,
    port: &PortContract,
    connector: &ConnectorSummary,
    diagnostics: ConnectorDiagnostics,
    error: PortError,
) -> ActionResult {
    ActionResult::failure(
        Some(action),
        ResultStatus::ConnectorFailure,
        format!("Connector failed while previewing {}.", port.id),
        Some(json!({
            "port": port.id,
            "connector": connector,
            "provider_error": error,
            "diagnostics": diagnostics,
        })),
    )
}

fn package_difference(
    requirement: &PackageRequirement,
    desired: Value,
    observed: Value,
    context: &ActionExecutionContext<'_>,
    action: &str,
) -> Result<MachinePlanEntry, ActionResult> {
    let resolution = context.connectors.resolve(&PACKAGE_MANAGER_PORT, context.connector_context);
    let diagnostics = resolution.diagnostics.clone();
    let Some(connector) = resolution.connector else {
        return Ok(unavailable_difference(
            "package",
            &requirement.id,
            desired,
            observed,
            &PACKAGE_MANAGER_PORT,
            diagnostics,
        ));
    };
    let summary = ConnectorSummary::from_connector(connector);
    let Some(manager) = connector.package_manager() else {
        return Err(ActionResult::failure(
            Some(action),
            ResultStatus::ConnectorFailure,
            format!("Selected Connector does not expose {} implementation.", PACKAGE_MANAGER_PORT.id),
            Some(json!({ "port": PACKAGE_MANAGER_PORT.id, "connector": summary, "diagnostics": diagnostics })),
        ));
    };
    let request = PackageStateRequest {
        id: requirement.id.clone(),
        present: requirement.state == PresenceState::Present,
        source: source_ref(requirement.source.as_ref()),
    };
    let preview = manager.preview(&request).map_err(|error| {
        preview_failure(action, &PACKAGE_MANAGER_PORT, &summary, diagnostics.clone(), error)
    })?;
    Ok(MachinePlanEntry {
        kind: "package".to_owned(),
        id: requirement.id.clone(),
        status: MachinePlanStatus::Changeable,
        desired,
        observed,
        port: Some(PACKAGE_MANAGER_PORT.id.to_owned()),
        connector: Some(summary),
        preview: Some(preview),
        reason: None,
        diagnostics: Some(diagnostics),
    })
}

fn configuration_difference(
    requirement: &ConfigurationRequirement,
    desired: Value,
    observed: Value,
    context: &ActionExecutionContext<'_>,
    action: &str,
) -> Result<MachinePlanEntry, ActionResult> {
    let resolution = context.connectors.resolve(&CONFIGURATION_MANAGER_PORT, context.connector_context);
    let diagnostics = resolution.diagnostics.clone();
    let Some(connector) = resolution.connector else {
        return Ok(unavailable_difference(
            "configuration",
            &requirement.id,
            desired,
            observed,
            &CONFIGURATION_MANAGER_PORT,
            diagnostics,
        ));
    };
    let summary = ConnectorSummary::from_connector(connector);
    let Some(manager) = connector.configuration_manager() else {
        return Err(ActionResult::failure(
            Some(action),
            ResultStatus::ConnectorFailure,
            format!("Selected Connector does not expose {} implementation.", CONFIGURATION_MANAGER_PORT.id),
            Some(json!({ "port": CONFIGURATION_MANAGER_PORT.id, "connector": summary, "diagnostics": diagnostics })),
        ));
    };
    let request = ConfigurationStateRequest {
        id: requirement.id.clone(),
        present: requirement.state == PresenceState::Present,
        source: source_ref(requirement.source.as_ref()),
    };
    let preview = manager.preview(&request).map_err(|error| {
        preview_failure(action, &CONFIGURATION_MANAGER_PORT, &summary, diagnostics.clone(), error)
    })?;
    Ok(MachinePlanEntry {
        kind: "configuration".to_owned(),
        id: requirement.id.clone(),
        status: MachinePlanStatus::Changeable,
        desired,
        observed,
        port: Some(CONFIGURATION_MANAGER_PORT.id.to_owned()),
        connector: Some(summary),
        preview: Some(preview),
        reason: None,
        diagnostics: Some(diagnostics),
    })
}

fn configuration_source_verification(
    requirement: &ConfigurationRequirement,
    desired: Value,
    observed: Value,
    context: &ActionExecutionContext<'_>,
    action: &str,
) -> Result<MachinePlanEntry, ActionResult> {
    let resolution = context.connectors.resolve(&CONFIGURATION_MANAGER_PORT, context.connector_context);
    let diagnostics = resolution.diagnostics.clone();
    let Some(connector) = resolution.connector else {
        return Ok(unavailable_source_verification(requirement, desired, observed, diagnostics));
    };
    let summary = ConnectorSummary::from_connector(connector);
    let Some(manager) = connector.configuration_manager() else {
        return Err(ActionResult::failure(
            Some(action),
            ResultStatus::ConnectorFailure,
            format!("Selected Connector does not expose {} implementation.", CONFIGURATION_MANAGER_PORT.id),
            Some(json!({ "port": CONFIGURATION_MANAGER_PORT.id, "connector": summary, "diagnostics": diagnostics })),
        ));
    };
    let request = ConfigurationStateRequest {
        id: requirement.id.clone(),
        present: true,
        source: source_ref(requirement.source.as_ref()),
    };
    let preview = manager.preview(&request).map_err(|error| {
        preview_failure(action, &CONFIGURATION_MANAGER_PORT, &summary, diagnostics.clone(), error)
    })?;
    Ok(MachinePlanEntry {
        kind: "configuration".to_owned(),
        id: requirement.id.clone(),
        status: if preview.changed {
            MachinePlanStatus::Changeable
        } else {
            MachinePlanStatus::Satisfied
        },
        desired,
        observed,
        port: Some(CONFIGURATION_MANAGER_PORT.id.to_owned()),
        connector: Some(summary),
        preview: Some(preview),
        reason: None,
        diagnostics: Some(diagnostics),
    })
}

fn service_difference(
    requirement: &ServiceRequirement,
    desired: Value,
    observed: Value,
    context: &ActionExecutionContext<'_>,
    action: &str,
) -> Result<MachinePlanEntry, ActionResult> {
    let resolution = context.connectors.resolve(&SERVICE_MANAGER_PORT, context.connector_context);
    let diagnostics = resolution.diagnostics.clone();
    let Some(connector) = resolution.connector else {
        return Ok(unavailable_difference(
            "service",
            &requirement.id,
            desired,
            observed,
            &SERVICE_MANAGER_PORT,
            diagnostics,
        ));
    };
    let summary = ConnectorSummary::from_connector(connector);
    let Some(manager) = connector.service_manager() else {
        return Err(ActionResult::failure(
            Some(action),
            ResultStatus::ConnectorFailure,
            format!("Selected Connector does not expose {} implementation.", SERVICE_MANAGER_PORT.id),
            Some(json!({ "port": SERVICE_MANAGER_PORT.id, "connector": summary, "diagnostics": diagnostics })),
        ));
    };
    let request = ServiceStateRequest {
        id: requirement.id.clone(),
        running: requirement.running,
        enabled: requirement.enabled,
        source: source_ref(requirement.source.as_ref()),
    };
    let preview = manager.preview(&request).map_err(|error| {
        preview_failure(action, &SERVICE_MANAGER_PORT, &summary, diagnostics.clone(), error)
    })?;
    Ok(MachinePlanEntry {
        kind: "service".to_owned(),
        id: requirement.id.clone(),
        status: MachinePlanStatus::Changeable,
        desired,
        observed,
        port: Some(SERVICE_MANAGER_PORT.id.to_owned()),
        connector: Some(summary),
        preview: Some(preview),
        reason: None,
        diagnostics: Some(diagnostics),
    })
}

fn compare_machine(
    authored: AuthoredMachineDeclaration,
    observed: ObservedMachine,
    context: &ActionExecutionContext<'_>,
    action: &str,
) -> Result<MachinePlan, ActionResult> {
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
            entries.push(package_difference(requirement, desired, actual, context, action)?);
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
        if item.present != desired_present {
            entries.push(configuration_difference(requirement, desired, actual, context, action)?);
        } else if desired_present && requirement.source.is_some() {
            entries.push(configuration_source_verification(requirement, desired, actual, context, action)?);
        } else {
            entries.push(satisfied_entry("configuration", &requirement.id, desired, actual));
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
            entries.push(service_difference(requirement, desired, actual, context, action)?);
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

    Ok(MachinePlan {
        role: declaration.role.clone(),
        authored,
        observed,
        entries,
        summary,
    })
}

fn load_plan(
    input: &Value,
    context: &ActionExecutionContext<'_>,
    action: &str,
) -> Result<MachinePlan, ActionResult> {
    let role = required_role(input, action)?;
    let root = resolve_central_root(context.root_options)
        .map_err(|message| ActionResult::failure(Some(action), ResultStatus::InvalidInput, message, None))?;
    let authored = read_machine_declaration(&root.path, &role)
        .map_err(|error| declaration_failure(action, error))?;
    let observed = inspect_current_machine(context, action)?;
    compare_machine(authored, observed, context, action)
}

fn plan_action(
    _registry: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    match load_plan(input, context, "machine.plan") {
        Ok(plan) => ActionResult::success(
            "machine.plan",
            to_value(plan).expect("machine plan serializes"),
        ),
        Err(result) => result,
    }
}

fn plan_satisfied(plan: &MachinePlan) -> bool {
    plan.summary.changeable == 0 && plan.summary.missing == 0 && plan.summary.unsupported == 0
}

fn verify_plan(
    input: &Value,
    context: &ActionExecutionContext<'_>,
    action: &str,
) -> Result<MachineVerification, ActionResult> {
    let plan = load_plan(input, context, action)?;
    Ok(MachineVerification {
        satisfied: plan_satisfied(&plan),
        plan,
    })
}

fn verify_action(
    _registry: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    match verify_plan(input, context, "machine.verify") {
        Ok(verification) if verification.satisfied => ActionResult::success(
            "machine.verify",
            to_value(verification).expect("machine verification serializes"),
        ),
        Ok(verification) => ActionResult::failure(
            Some("machine.verify"),
            ResultStatus::VerificationFailure,
            "Observed machine state does not satisfy the authored declaration.",
            Some(to_value(verification).expect("machine verification serializes")),
        ),
        Err(result) => result,
    }
}

fn selected_connector_for_plan<'a>(
    entry: &MachinePlanEntry,
    port: &PortContract,
    context: &'a ActionExecutionContext<'_>,
) -> Result<&'a dyn central_connector_sdk::Connector, ActionResult> {
    let resolution = context.connectors.resolve(port, context.connector_context);
    let Some(connector) = resolution.connector else {
        return Err(ActionResult::failure(
            Some("machine.apply"),
            ResultStatus::UnavailableCapability,
            format!("Planned {} Connector is no longer eligible.", port.id),
            Some(json!({ "entry": entry, "diagnostics": resolution.diagnostics })),
        ));
    };
    if entry.connector.as_ref().map(|value| value.id.as_str()) != Some(connector.manifest().id.as_str()) {
        return Err(ActionResult::failure(
            Some("machine.apply"),
            ResultStatus::ConnectorFailure,
            format!("The selected {} Connector changed after planning.", port.id),
            Some(json!({
                "entry": entry,
                "selected_connector": ConnectorSummary::from_connector(connector),
                "diagnostics": resolution.diagnostics,
            })),
        ));
    }
    Ok(connector)
}

fn package_request(entry: &MachinePlanEntry) -> Result<PackageStateRequest, ActionResult> {
    let requirement: PackageRequirement = serde_json::from_value(entry.desired.clone()).map_err(|error| {
        ActionResult::failure(
            Some("machine.apply"),
            ResultStatus::InternalFailure,
            format!("Planned package request is invalid: {error}"),
            Some(json!({ "entry": entry })),
        )
    })?;
    Ok(PackageStateRequest {
        id: requirement.id,
        present: requirement.state == PresenceState::Present,
        source: source_ref(requirement.source.as_ref()),
    })
}

fn configuration_request(entry: &MachinePlanEntry) -> Result<ConfigurationStateRequest, ActionResult> {
    let requirement: ConfigurationRequirement = serde_json::from_value(entry.desired.clone()).map_err(|error| {
        ActionResult::failure(
            Some("machine.apply"),
            ResultStatus::InternalFailure,
            format!("Planned configuration request is invalid: {error}"),
            Some(json!({ "entry": entry })),
        )
    })?;
    Ok(ConfigurationStateRequest {
        id: requirement.id,
        present: requirement.state == PresenceState::Present,
        source: source_ref(requirement.source.as_ref()),
    })
}

fn service_request(entry: &MachinePlanEntry) -> Result<ServiceStateRequest, ActionResult> {
    let requirement: ServiceRequirement = serde_json::from_value(entry.desired.clone()).map_err(|error| {
        ActionResult::failure(
            Some("machine.apply"),
            ResultStatus::InternalFailure,
            format!("Planned service request is invalid: {error}"),
            Some(json!({ "entry": entry })),
        )
    })?;
    Ok(ServiceStateRequest {
        id: requirement.id,
        running: requirement.running,
        enabled: requirement.enabled,
        source: source_ref(requirement.source.as_ref()),
    })
}

fn execute_planned_entry(
    entry: &MachinePlanEntry,
    context: &ActionExecutionContext<'_>,
) -> Result<MachineApplyOperation, ActionResult> {
    let port = entry.port.as_deref().ok_or_else(|| {
        ActionResult::failure(
            Some("machine.apply"),
            ResultStatus::InternalFailure,
            "Changeable machine plan entry has no Port.",
            Some(json!({ "entry": entry })),
        )
    })?;

    let (connector, result) = if port == PACKAGE_MANAGER_PORT.id {
        let connector = selected_connector_for_plan(entry, &PACKAGE_MANAGER_PORT, context)?;
        let manager = connector.package_manager().ok_or_else(|| {
            ActionResult::failure(
                Some("machine.apply"),
                ResultStatus::ConnectorFailure,
                format!("Selected Connector does not expose {} implementation.", PACKAGE_MANAGER_PORT.id),
                Some(json!({ "entry": entry })),
            )
        })?;
        let request = package_request(entry)?;
        let result = manager.apply(&request).map_err(|error| apply_port_failure(entry, error))?;
        (connector, result)
    } else if port == CONFIGURATION_MANAGER_PORT.id {
        let connector = selected_connector_for_plan(entry, &CONFIGURATION_MANAGER_PORT, context)?;
        let manager = connector.configuration_manager().ok_or_else(|| {
            ActionResult::failure(
                Some("machine.apply"),
                ResultStatus::ConnectorFailure,
                format!("Selected Connector does not expose {} implementation.", CONFIGURATION_MANAGER_PORT.id),
                Some(json!({ "entry": entry })),
            )
        })?;
        let request = configuration_request(entry)?;
        let result = manager.apply(&request).map_err(|error| apply_port_failure(entry, error))?;
        (connector, result)
    } else if port == SERVICE_MANAGER_PORT.id {
        let connector = selected_connector_for_plan(entry, &SERVICE_MANAGER_PORT, context)?;
        let manager = connector.service_manager().ok_or_else(|| {
            ActionResult::failure(
                Some("machine.apply"),
                ResultStatus::ConnectorFailure,
                format!("Selected Connector does not expose {} implementation.", SERVICE_MANAGER_PORT.id),
                Some(json!({ "entry": entry })),
            )
        })?;
        let request = service_request(entry)?;
        let result = manager.apply(&request).map_err(|error| apply_port_failure(entry, error))?;
        (connector, result)
    } else {
        return Err(ActionResult::failure(
            Some("machine.apply"),
            ResultStatus::InternalFailure,
            format!("Unknown reconciliation Port in machine plan: {port}"),
            Some(json!({ "entry": entry })),
        ));
    };

    Ok(MachineApplyOperation {
        kind: entry.kind.clone(),
        id: entry.id.clone(),
        port: port.to_owned(),
        connector: ConnectorSummary::from_connector(connector),
        result,
    })
}

fn apply_port_failure(entry: &MachinePlanEntry, error: PortError) -> ActionResult {
    ActionResult::failure(
        Some("machine.apply"),
        ResultStatus::ConnectorFailure,
        format!("Connector failed while applying planned {} '{}'.", entry.kind, entry.id),
        Some(json!({ "entry": entry, "provider_error": error })),
    )
}

fn apply_action(
    _registry: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let initial_plan = match load_plan(input, context, "machine.apply") {
        Ok(plan) => plan,
        Err(result) => return result,
    };

    if plan_satisfied(&initial_plan) {
        let verification = MachineVerification { satisfied: true, plan: initial_plan.clone() };
        return ActionResult::success(
            "machine.apply",
            to_value(MachineApplyReport {
                outcome: MachineApplyOutcome::Complete,
                initial_plan,
                operations: Vec::new(),
                verification: Some(verification),
            }).expect("machine apply report serializes"),
        );
    }

    if initial_plan.summary.changeable == 0 {
        let report = MachineApplyReport {
            outcome: MachineApplyOutcome::Unavailable,
            initial_plan,
            operations: Vec::new(),
            verification: None,
        };
        return ActionResult::failure(
            Some("machine.apply"),
            ResultStatus::UnavailableCapability,
            "The machine plan contains no supported change that can be applied.",
            Some(to_value(report).expect("machine apply report serializes")),
        );
    }

    let has_unavailable = initial_plan.summary.missing > 0 || initial_plan.summary.unsupported > 0;
    let mut operations = Vec::new();
    for entry in initial_plan.entries.iter().filter(|entry| entry.status == MachinePlanStatus::Changeable) {
        match execute_planned_entry(entry, context) {
            Ok(operation) => operations.push(operation),
            Err(failure) if operations.is_empty() => return failure,
            Err(failure) => {
                let report = MachineApplyReport {
                    outcome: MachineApplyOutcome::Partial,
                    initial_plan,
                    operations,
                    verification: None,
                };
                let message = failure
                    .error
                    .as_ref()
                    .map(|error| error.message.clone())
                    .unwrap_or_else(|| "Machine reconciliation failed after partial application.".to_owned());
                return ActionResult::failure(
                    Some("machine.apply"),
                    ResultStatus::PartialCompletion,
                    message,
                    Some(json!({
                        "report": report,
                        "failure": failure.error,
                    })),
                );
            }
        }
    }

    let verification = match verify_plan(input, context, "machine.apply") {
        Ok(verification) => verification,
        Err(failure) => {
            let report = MachineApplyReport {
                outcome: MachineApplyOutcome::Partial,
                initial_plan,
                operations,
                verification: None,
            };
            return ActionResult::failure(
                Some("machine.apply"),
                ResultStatus::VerificationFailure,
                "Machine changes were applied but post-apply verification could not complete.",
                Some(json!({ "report": report, "verification_failure": failure.error })),
            );
        }
    };

    let report = MachineApplyReport {
        outcome: if verification.satisfied && !has_unavailable {
            MachineApplyOutcome::Complete
        } else {
            MachineApplyOutcome::Partial
        },
        initial_plan,
        operations,
        verification: Some(verification.clone()),
    };

    if verification.satisfied && !has_unavailable {
        ActionResult::success(
            "machine.apply",
            to_value(report).expect("machine apply report serializes"),
        )
    } else if has_unavailable {
        ActionResult::failure(
            Some("machine.apply"),
            ResultStatus::PartialCompletion,
            "Supported machine changes were applied, but the authored declaration remains only partially satisfiable.",
            Some(to_value(report).expect("machine apply report serializes")),
        )
    } else {
        ActionResult::failure(
            Some("machine.apply"),
            ResultStatus::VerificationFailure,
            "Machine changes were applied, but verification does not satisfy the authored declaration.",
            Some(to_value(report).expect("machine apply report serializes")),
        )
    }
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
            description: "Compare authored machine intent with observed host state and produce a non-mutating structured change plan with Connector previews.".to_owned(),
            inputs: vec![role_input()],
            output: ActionOutputDefinition { output_type: "machine-change-plan".to_owned() },
            mutation_class: MutationClass::ReadOnly,
            preview_supported: true,
            required_ports: vec![MACHINE_INSPECTOR_PORT.id.to_owned()],
            availability: ActionAvailability { available: true, reason: None },
        },
        plan_action,
    ).expect("machine Action id is valid");

    registry.register(
        ActionDescriptor {
            id: "machine.apply".to_owned(),
            title: "Apply machine plan".to_owned(),
            description: "Apply the currently planned supported machine changes through the Ports and Connectors named by the plan, then verify the authored declaration.".to_owned(),
            inputs: vec![role_input()],
            output: ActionOutputDefinition { output_type: "machine-apply-report".to_owned() },
            mutation_class: MutationClass::LocallyMutating,
            preview_supported: true,
            required_ports: vec![MACHINE_INSPECTOR_PORT.id.to_owned()],
            availability: ActionAvailability { available: true, reason: None },
        },
        apply_action,
    ).expect("machine Action id is valid");

    registry.register(
        ActionDescriptor {
            id: "machine.verify".to_owned(),
            title: "Verify machine declaration".to_owned(),
            description: "Observe the current machine and verify it against the authored machine-role declaration, including source-backed configuration state through public ConfigurationManager preview when required.".to_owned(),
            inputs: vec![role_input()],
            output: ActionOutputDefinition { output_type: "machine-verification".to_owned() },
            mutation_class: MutationClass::ReadOnly,
            preview_supported: false,
            required_ports: vec![MACHINE_INSPECTOR_PORT.id.to_owned()],
            availability: ActionAvailability { available: true, reason: None },
        },
        verify_action,
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
            if let Some(preview) = entry.get("preview") {
                if let Some(summary) = preview.get("summary").and_then(Value::as_str) {
                    lines.push(format!("  preview: {summary}"));
                }
            }
            if let Some(reason) = entry.get("reason").and_then(Value::as_str) {
                lines.push(format!("  {reason}"));
            }
        }
    }
    lines.join("\n")
}

pub fn explain_machine_verification(data: &Value) -> String {
    let satisfied = data.get("satisfied").and_then(Value::as_bool).unwrap_or(false);
    let plan = data.get("plan").unwrap_or(&Value::Null);
    format!(
        "Machine verification: {}\n{}",
        if satisfied { "satisfied" } else { "mismatch" },
        explain_machine_plan(plan),
    )
}

pub fn explain_machine_apply(data: &Value) -> String {
    let outcome = data.get("outcome").and_then(Value::as_str).unwrap_or("unknown");
    let operations = data.get("operations").and_then(Value::as_array).map_or(0, Vec::len);
    let verification = data
        .get("verification")
        .and_then(|value| value.get("satisfied"))
        .and_then(Value::as_bool);
    format!(
        "Machine apply: {outcome}\nOperations: {operations}\nVerified: {}",
        verification.map(|value| if value { "yes" } else { "no" }).unwrap_or("not-run"),
    )
}
