use crate::action::{
    ActionAvailability, ActionDescriptor, ActionExecutionContext, ActionInputDefinition,
    ActionOutputDefinition, ActionRegistry, MutationClass,
};
use crate::machine::MachineSourceReference;
use crate::result::{ActionResult, ResultStatus};
use crate::root::resolve_central_root;
use central_connector_sdk::{
    ConnectorDiagnostics, ConnectorSummary, ReconciliationSourceReference, StateChangePreview,
    StateChangeResult, SynchronizationRequest, SYNCHRONIZER_PORT,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, to_value, Value};
use std::fs;
use std::path::PathBuf;

pub const RECOVERY_DECLARATION_SCHEMA: &str = "central.recovery";
pub const RECOVERY_DECLARATION_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SynchronizationDeclaration {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<MachineSourceReference>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecoveryDeclaration {
    pub schema: String,
    pub version: u32,
    pub role: String,
    pub synchronization: SynchronizationDeclaration,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RecoveryDeclarationSource {
    pub path: PathBuf,
    pub source_class: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AuthoredRecoveryDeclaration {
    pub declaration: RecoveryDeclaration,
    pub source: RecoveryDeclarationSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoverySynchronizationStatus {
    NotConfigured,
    Satisfied,
    Changeable,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RecoverySynchronizationPlan {
    pub status: RecoverySynchronizationStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authored: Option<AuthoredRecoveryDeclaration>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub port: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connector: Option<ConnectorSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preview: Option<StateChangePreview>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diagnostics: Option<ConnectorDiagnostics>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RecoveryPlan {
    pub role: String,
    pub synchronization: RecoverySynchronizationPlan,
    pub machine: Value,
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

fn nested_failure(action: &str, stage: &str, nested: ActionResult) -> ActionResult {
    let status = nested.status;
    let message = nested
        .error
        .as_ref()
        .map(|error| error.message.clone())
        .unwrap_or_else(|| format!("{stage} failed."));
    ActionResult::failure(
        Some(action),
        status,
        format!("Recovery {stage} failed: {message}"),
        Some(json!({ "stage": stage, "result": nested })),
    )
}

fn recovery_path(root: &std::path::Path, role: &str) -> PathBuf {
    root.join("Control")
        .join("machines")
        .join(format!("{role}.recovery.json"))
}

fn validate_recovery_declaration(
    role: &str,
    declaration: &RecoveryDeclaration,
    relative: &PathBuf,
) -> Result<(), ActionResult> {
    if declaration.schema != RECOVERY_DECLARATION_SCHEMA {
        return Err(ActionResult::failure(
            Some("central.recovery.plan"),
            ResultStatus::InvalidInput,
            format!(
                "Recovery declaration schema '{}' is unsupported; expected '{RECOVERY_DECLARATION_SCHEMA}'.",
                declaration.schema
            ),
            Some(json!({ "path": relative, "field": "schema" })),
        ));
    }
    if declaration.version != RECOVERY_DECLARATION_VERSION {
        return Err(ActionResult::failure(
            Some("central.recovery.plan"),
            ResultStatus::InvalidInput,
            format!(
                "Recovery declaration version {} is unsupported; expected {RECOVERY_DECLARATION_VERSION}.",
                declaration.version
            ),
            Some(json!({ "path": relative, "field": "version" })),
        ));
    }
    if declaration.role != role {
        return Err(ActionResult::failure(
            Some("central.recovery.plan"),
            ResultStatus::InvalidInput,
            format!(
                "Recovery declaration role '{}' does not match machine role '{role}'.",
                declaration.role
            ),
            Some(json!({ "path": relative, "field": "role" })),
        ));
    }
    if declaration.synchronization.id.trim().is_empty() {
        return Err(ActionResult::failure(
            Some("central.recovery.plan"),
            ResultStatus::InvalidInput,
            "Recovery synchronization id must be non-empty.",
            Some(json!({ "path": relative, "field": "synchronization.id" })),
        ));
    }
    if let Some(source) = &declaration.synchronization.source {
        if source.kind.trim().is_empty() || source.reference.trim().is_empty() {
            return Err(ActionResult::failure(
                Some("central.recovery.plan"),
                ResultStatus::InvalidInput,
                "Recovery synchronization source kind and reference must be non-empty.",
                Some(json!({ "path": relative, "field": "synchronization.source" })),
            ));
        }
    }
    Ok(())
}

fn read_recovery_declaration(
    root: &std::path::Path,
    role: &str,
) -> Result<Option<AuthoredRecoveryDeclaration>, ActionResult> {
    let relative = PathBuf::from("Control")
        .join("machines")
        .join(format!("{role}.recovery.json"));
    let path = recovery_path(root, role);
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(ActionResult::failure(
                Some("central.recovery.plan"),
                ResultStatus::InternalFailure,
                format!("Cannot read recovery declaration for {role}: {error}"),
                Some(json!({ "path": relative })),
            ))
        }
    };
    let declaration: RecoveryDeclaration = serde_json::from_str(&text).map_err(|error| {
        ActionResult::failure(
            Some("central.recovery.plan"),
            ResultStatus::InvalidInput,
            format!("Recovery declaration for {role} is invalid: {error}"),
            Some(json!({ "path": relative })),
        )
    })?;
    validate_recovery_declaration(role, &declaration, &relative)?;
    Ok(Some(AuthoredRecoveryDeclaration {
        declaration,
        source: RecoveryDeclarationSource {
            path: relative,
            source_class: "authored".to_owned(),
        },
    }))
}

fn synchronization_request(
    authored: &AuthoredRecoveryDeclaration,
) -> SynchronizationRequest {
    let declaration = &authored.declaration.synchronization;
    SynchronizationRequest {
        id: declaration.id.clone(),
        source: declaration.source.as_ref().map(|source| ReconciliationSourceReference {
            kind: source.kind.clone(),
            reference: source.reference.clone(),
        }),
    }
}

fn plan_synchronization(
    authored: Option<AuthoredRecoveryDeclaration>,
    context: &ActionExecutionContext<'_>,
) -> Result<RecoverySynchronizationPlan, ActionResult> {
    let Some(authored) = authored else {
        return Ok(RecoverySynchronizationPlan {
            status: RecoverySynchronizationStatus::NotConfigured,
            authored: None,
            port: None,
            connector: None,
            preview: None,
            diagnostics: None,
            reason: None,
        });
    };

    let resolution = context
        .connectors
        .resolve(&SYNCHRONIZER_PORT, context.connector_context);
    let diagnostics = resolution.diagnostics.clone();
    let Some(connector) = resolution.connector else {
        return Ok(RecoverySynchronizationPlan {
            status: RecoverySynchronizationStatus::Unavailable,
            authored: Some(authored),
            port: Some(SYNCHRONIZER_PORT.id.to_owned()),
            connector: None,
            preview: None,
            diagnostics: Some(diagnostics),
            reason: Some(format!(
                "Recovery synchronization is configured, but no eligible {} Connector is available.",
                SYNCHRONIZER_PORT.id
            )),
        });
    };
    let summary = ConnectorSummary::from_connector(connector);
    let Some(synchronizer) = connector.synchronizer() else {
        return Err(ActionResult::failure(
            Some("central.recovery.plan"),
            ResultStatus::ConnectorFailure,
            format!(
                "Selected Connector does not expose {} implementation.",
                SYNCHRONIZER_PORT.id
            ),
            Some(json!({
                "port": SYNCHRONIZER_PORT.id,
                "connector": summary,
                "diagnostics": diagnostics
            })),
        ));
    };
    let request = synchronization_request(&authored);
    let preview = synchronizer.preview(&request).map_err(|error| {
        ActionResult::failure(
            Some("central.recovery.plan"),
            ResultStatus::ConnectorFailure,
            "Connector failed while previewing recovery synchronization.",
            Some(json!({
                "port": SYNCHRONIZER_PORT.id,
                "connector": summary,
                "provider_error": error,
                "diagnostics": diagnostics
            })),
        )
    })?;
    Ok(RecoverySynchronizationPlan {
        status: if preview.changed {
            RecoverySynchronizationStatus::Changeable
        } else {
            RecoverySynchronizationStatus::Satisfied
        },
        authored: Some(authored),
        port: Some(SYNCHRONIZER_PORT.id.to_owned()),
        connector: Some(summary),
        preview: Some(preview),
        diagnostics: Some(diagnostics),
        reason: None,
    })
}

fn build_recovery_plan(
    registry: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
    action: &str,
) -> Result<RecoveryPlan, ActionResult> {
    let machine_plan = registry.execute("machine.plan", input, context);
    if !machine_plan.ok {
        return Err(nested_failure(action, "machine plan", machine_plan));
    }
    let machine = machine_plan.data.expect("successful machine.plan has data");
    let role = machine
        .get("role")
        .and_then(Value::as_str)
        .expect("machine.plan emits validated role")
        .to_owned();
    let root = resolve_central_root(context.root_options).map_err(|message| {
        ActionResult::failure(Some(action), ResultStatus::InvalidInput, message, None)
    })?;
    let authored = read_recovery_declaration(&root.path, &role).map_err(|result| {
        if result.action.as_deref() == Some(action) {
            result
        } else {
            nested_failure(action, "recovery declaration", result)
        }
    })?;
    let synchronization = plan_synchronization(authored, context).map_err(|result| {
        nested_failure(action, "synchronization preview", result)
    })?;
    Ok(RecoveryPlan {
        role,
        synchronization,
        machine,
    })
}

fn recovery_plan_action(
    registry: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    match build_recovery_plan(registry, input, context, "central.recovery.plan") {
        Ok(plan) => ActionResult::success(
            "central.recovery.plan",
            to_value(plan).expect("recovery plan serializes"),
        ),
        Err(result) => result,
    }
}

fn apply_synchronization(
    plan: &RecoverySynchronizationPlan,
    context: &ActionExecutionContext<'_>,
) -> Result<Option<StateChangeResult>, ActionResult> {
    match plan.status {
        RecoverySynchronizationStatus::NotConfigured | RecoverySynchronizationStatus::Satisfied => {
            return Ok(None)
        }
        RecoverySynchronizationStatus::Unavailable => {
            return Err(ActionResult::failure(
                Some("central.recover"),
                ResultStatus::UnavailableCapability,
                plan.reason
                    .clone()
                    .unwrap_or_else(|| "Recovery synchronization is unavailable.".to_owned()),
                Some(to_value(plan).expect("synchronization plan serializes")),
            ));
        }
        RecoverySynchronizationStatus::Changeable => {}
    }

    let authored = plan
        .authored
        .as_ref()
        .expect("changeable synchronization has authored declaration");
    let expected = plan
        .connector
        .as_ref()
        .expect("changeable synchronization has connector");
    let resolution = context
        .connectors
        .resolve(&SYNCHRONIZER_PORT, context.connector_context);
    let Some(connector) = resolution.connector else {
        return Err(ActionResult::failure(
            Some("central.recover"),
            ResultStatus::UnavailableCapability,
            "Planned recovery Synchronizer is no longer eligible.",
            Some(json!({
                "planned": plan,
                "diagnostics": resolution.diagnostics
            })),
        ));
    };
    if connector.manifest().id != expected.id {
        return Err(ActionResult::failure(
            Some("central.recover"),
            ResultStatus::ConnectorFailure,
            "Selected recovery Synchronizer changed after planning.",
            Some(json!({
                "planned": expected,
                "selected": ConnectorSummary::from_connector(connector),
                "diagnostics": resolution.diagnostics
            })),
        ));
    }
    let Some(synchronizer) = connector.synchronizer() else {
        return Err(ActionResult::failure(
            Some("central.recover"),
            ResultStatus::ConnectorFailure,
            "Selected Connector no longer exposes Synchronizer implementation.",
            Some(json!({ "connector": expected })),
        ));
    };
    let request = synchronization_request(authored);
    synchronizer.apply(&request).map(Some).map_err(|error| {
        ActionResult::failure(
            Some("central.recover"),
            ResultStatus::ConnectorFailure,
            "Connector failed while applying recovery synchronization.",
            Some(json!({
                "port": SYNCHRONIZER_PORT.id,
                "connector": expected,
                "provider_error": error
            })),
        )
    })
}

fn recover_action(
    registry: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let plan = match build_recovery_plan(registry, input, context, "central.recover") {
        Ok(plan) => plan,
        Err(result) => return result,
    };

    let synchronization_result = match apply_synchronization(&plan.synchronization, context) {
        Ok(result) => result,
        Err(result) => return result,
    };

    let machine_apply = registry.execute("machine.apply", input, context);
    if !machine_apply.ok {
        let status = match machine_apply.status {
            ResultStatus::VerificationFailure => ResultStatus::VerificationFailure,
            ResultStatus::PartialCompletion => ResultStatus::PartialCompletion,
            other if synchronization_result.as_ref().is_some_and(|result| result.changed) => {
                let _ = other;
                ResultStatus::PartialCompletion
            }
            other => other,
        };
        let message = machine_apply
            .error
            .as_ref()
            .map(|error| error.message.clone())
            .unwrap_or_else(|| "Machine recovery failed.".to_owned());
        return ActionResult::failure(
            Some("central.recover"),
            status,
            format!("Recovery machine application did not complete: {message}"),
            Some(json!({
                "initial_plan": plan,
                "synchronization": synchronization_result,
                "machine_apply": machine_apply
            })),
        );
    }

    let verification = registry.execute("machine.verify", input, context);
    if !verification.ok {
        let message = verification
            .error
            .as_ref()
            .map(|error| error.message.clone())
            .unwrap_or_else(|| "Machine verification failed.".to_owned());
        return ActionResult::failure(
            Some("central.recover"),
            ResultStatus::VerificationFailure,
            format!("Recovery completed its mutation steps but final verification failed: {message}"),
            Some(json!({
                "initial_plan": plan,
                "synchronization": synchronization_result,
                "machine_apply": machine_apply,
                "verification": verification
            })),
        );
    }

    ActionResult::success(
        "central.recover",
        json!({
            "outcome": "complete",
            "initial_plan": plan,
            "synchronization": synchronization_result,
            "machine_apply": machine_apply.data,
            "verification": verification.data
        }),
    )
}

pub(crate) fn register_recovery_actions(registry: &mut ActionRegistry) {
    registry
        .register(
            ActionDescriptor {
                id: "central.recovery.plan".to_owned(),
                title: "Plan Central recovery".to_owned(),
                description: "Preview configured synchronization and the existing canonical machine reconciliation plan without mutating either.".to_owned(),
                inputs: vec![role_input()],
                output: ActionOutputDefinition {
                    output_type: "central-recovery-plan".to_owned(),
                },
                mutation_class: MutationClass::ReadOnly,
                preview_supported: true,
                required_ports: vec!["MachineInspector".to_owned()],
                availability: ActionAvailability {
                    available: true,
                    reason: None,
                },
            },
            recovery_plan_action,
        )
        .expect("recovery Action id is valid");

    registry
        .register(
            ActionDescriptor {
                id: "central.recover".to_owned(),
                title: "Recover Central machine state".to_owned(),
                description: "Apply configured synchronization when present, then reuse canonical machine.apply and machine.verify for package/configuration/service recovery.".to_owned(),
                inputs: vec![role_input()],
                output: ActionOutputDefinition {
                    output_type: "central-recovery-report".to_owned(),
                },
                mutation_class: MutationClass::ExternallyMutating,
                preview_supported: true,
                required_ports: vec!["MachineInspector".to_owned()],
                availability: ActionAvailability {
                    available: true,
                    reason: None,
                },
            },
            recover_action,
        )
        .expect("recovery Action id is valid");
}

pub fn explain_recovery_plan(data: &Value) -> String {
    let role = data.get("role").and_then(Value::as_str).unwrap_or_default();
    let sync = data
        .get("synchronization")
        .and_then(|value| value.get("status"))
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let machine = data.get("machine").unwrap_or(&Value::Null);
    format!(
        "Central recovery plan: {role}\nSynchronization: {sync}\n{}",
        crate::machine::explain_machine_plan(machine)
    )
}

pub fn explain_recovery(data: &Value) -> String {
    let outcome = data.get("outcome").and_then(Value::as_str).unwrap_or("unknown");
    let sync_changed = data
        .get("synchronization")
        .and_then(|value| value.get("changed"))
        .and_then(Value::as_bool)
        .map(|changed| if changed { "yes" } else { "no" })
        .unwrap_or("not-run");
    let machine_operations = data
        .get("machine_apply")
        .and_then(|value| value.get("operations"))
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    format!(
        "Central recovery: {outcome}\nSynchronization changed: {sync_changed}\nMachine operations: {machine_operations}\nVerified: yes"
    )
}
