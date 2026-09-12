//! Native continuous-work operations over Central's existing source ownership.
//! No personal installation, default-policy adoption, model call or shell rewrite.
pub mod authority;
pub mod documents;
mod extended;
mod history;
pub mod migration;
pub mod placement;
pub mod receiving;
pub mod source;
pub mod temporal;

use crate::action::{
    ActionAvailability, ActionDescriptor, ActionExecutionContext, ActionInputDefinition,
    ActionOutputDefinition, ActionRegistry, MutationClass,
};
use crate::result::{ActionResult, ResultStatus};
use serde_json::{json, Value};
use source::{invalid, Scope};
use std::{
    io,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

/// Controlled-clock native execution. Authenticated operations need the separate
/// credential channel; JSON actor labels never establish a principal.
pub fn execute_at(central: &Path, operation: &str, input: &Value, now: u64) -> io::Result<Value> {
    execute_with_token_at(central, operation, input, None, now)
}
pub fn execute_with_token_at(
    central: &Path,
    operation: &str,
    input: &Value,
    token: Option<&str>,
    now: u64,
) -> io::Result<Value> {
    if !input.is_object() {
        return Err(invalid("native operation requires an object"));
    }
    let project = match input.get("project") {
        None | Some(Value::Null) => None,
        Some(Value::String(project)) => Some(project.as_str()),
        _ => {
            return Err(invalid(
                "project must be a string or absent for root agency",
            ))
        }
    };
    let scope = Scope::resolve(central, project)?;
    // Receiving retains the existing return-lock -> source-lock order. Never
    // enter its dispatcher while holding the source-mutation lock already.
    match operation {
        "allocate" => return placement::allocate(&scope, input, now),
        "validate" => return placement::validate(&scope, input, now),
        operation if operation.starts_with("receiving_") => {
            return receiving::dispatch(&scope, operation, input, token, now)
        }
        _ => {}
    }
    let _locks = source::lock(&scope)?;
    match operation {
        "policy" => Ok(serde_json::to_value(placement::effective_policy(
            &scope, now,
        )?)?),
        "now_read" => {
            let (record, reading) = placement::read_now(&scope, source::text(input, "now_ref")?)?;
            match input.get("with_placement") {
                Some(Value::Bool(true)) => {
                    // Current acting facts are explicitly requested. Ordinary
                    // history remains readable even when policy is unavailable.
                    // allocation_reading only opens/verifies the existing T
                    // directory; this read never allocates, re-enters or renews
                    // an already-issued material authority lease.
                    let policy = placement::effective_policy(&scope, now)?;
                    let mut result =
                        placement::allocation_reading(&scope, &record, &reading, policy, false)?;
                    result["schema"] = json!("central.now-reading/v1");
                    result["placement_included"] = json!(true);
                    result
                        .as_object_mut()
                        .expect("native reading object")
                        .remove("created");
                    Ok(result)
                }
                None | Some(Value::Bool(false)) => Ok(
                    json!({"schema":"central.now-reading/v1","record":record,"source":reading.source,"revision":reading.revision,"automatic_agent_or_model_invocation":false,
                        "pointer_note":"Records are pointers, not authority: follow the governing guidance, and search the native surface before building anything new."}),
                ),
                _ => Err(invalid("with_placement must be a boolean")),
            }
        }
        "now_list" => Ok(json!({
            "schema": "central.now-listing/v1",
            "records": placement::list_now(&scope, input)?,
            "automatic_agent_or_model_invocation": false,
            "pointer_note":"Records are pointers, not authority: follow the governing guidance, and search the native surface before building anything new."
        })),
        "time_policy" => Ok(serde_json::to_value(temporal::time_policy(&scope, now)?)?),
        "day_read" => temporal::day_read(&scope, input),
        "source_history" => history::read(&scope, input),
        "day_ensure" | "day_lifecycle" | "now_lifecycle" | "now_obligations" => {
            let action = match operation {
                "day_ensure" => "central.day.ensure",
                "day_lifecycle" => "central.day.lifecycle",
                "now_lifecycle" => "central.now.lifecycle",
                _ => "central.now.obligations",
            };
            let principal = authority::authenticate(
                &scope,
                token,
                action,
                input
                    .get("expected_authority_revision")
                    .and_then(Value::as_str),
                now,
            )?;
            match operation {
                "day_ensure" => temporal::ensure_day(&scope, input, &principal, now),
                "day_lifecycle" => temporal::day_lifecycle(&scope, input, &principal, now),
                "now_lifecycle" => temporal::now_lifecycle(&scope, input, &principal, now),
                _ => temporal::now_obligations(&scope, input, &principal, now),
            }
        }
        _ => extended::dispatch(&scope, operation, input, token, now),
    }
}
fn execute(
    action: &str,
    operation: &str,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let result = (|| {
        let root = crate::root::resolve_central_root(context.root_options).map_err(invalid)?;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(io::Error::other)?
            .as_secs();
        let token = std::env::var("CENTRAL_NATIVE_TOKEN").ok();
        execute_with_token_at(&root.path, operation, input, token.as_deref(), now)
    })();
    match result {
        Ok(value) if value.get("allowed") == Some(&Value::Bool(false)) => {
            ActionResult::failure_coded(
                Some(action),
                ResultStatus::UnavailableCapability,
                "placement_rejected",
                "Destination is outside the current task's authorised write bounds.",
                Some(value),
            )
        }
        Ok(value) => ActionResult::success(action, value),
        Err(error) => {
            let (status, code) = match error.kind() {
                io::ErrorKind::AlreadyExists => (
                    ResultStatus::VerificationFailure,
                    "stale_basis_or_identity_conflict",
                ),
                io::ErrorKind::PermissionDenied => (
                    ResultStatus::UnavailableCapability,
                    "policy_or_source_denied",
                ),
                io::ErrorKind::NotFound => (
                    ResultStatus::UnavailableCapability,
                    "source_or_scope_unavailable",
                ),
                io::ErrorKind::InvalidInput | io::ErrorKind::InvalidData => (
                    ResultStatus::InvalidInput,
                    "invalid_native_source_or_request",
                ),
                _ => (ResultStatus::InternalFailure, "native_operation_failed"),
            };
            ActionResult::failure_coded(
                Some(action),
                status,
                code,
                error.to_string(),
                Some(
                    json!({"retry_policy_action":"central.work.policy","automatic_authority_widening":false,"outside_writes_prevented":false}),
                ),
            )
        }
    }
}
macro_rules! handler {
    ($name:ident, $action:literal, $operation:literal) => {
        fn $name(
            _: &ActionRegistry,
            input: &Value,
            context: &ActionExecutionContext<'_>,
        ) -> ActionResult {
            execute($action, $operation, input, context)
        }
    };
}
handler!(policy_action, "central.work.policy", "policy");
handler!(allocate_action, "central.now.allocate", "allocate");
handler!(validate_action, "central.work.validate", "validate");
handler!(now_read_action, "central.now.read", "now_read");
handler!(now_list_action, "central.now.list", "now_list");
handler!(time_action, "central.time.policy", "time_policy");
handler!(day_read_action, "central.day.read", "day_read");
handler!(day_ensure_action, "central.day.ensure", "day_ensure");
handler!(
    day_lifecycle_action,
    "central.day.lifecycle",
    "day_lifecycle"
);
handler!(
    now_lifecycle_action,
    "central.now.lifecycle",
    "now_lifecycle"
);
handler!(
    now_obligations_action,
    "central.now.obligations",
    "now_obligations"
);
handler!(
    history_action,
    "central.temporal.source-history",
    "source_history"
);

type Definition<'a> = (
    &'a str,
    &'a str,
    &'a str,
    bool,
    crate::action::ActionHandler,
    &'a [(&'a str, &'a str, bool)],
);
pub(crate) fn register_definitions(registry: &mut ActionRegistry, definitions: &[Definition<'_>]) {
    for (id, title, description, mutates, handler, fields) in definitions {
        let mut inputs = vec![ActionInputDefinition {
            name: "project".into(),
            input_type: "string".into(),
            required: false,
            choices: None,
            selection: None,
        }];
        inputs.extend(
            fields
                .iter()
                .map(|(name, kind, required)| ActionInputDefinition {
                    name: (*name).into(),
                    input_type: (*kind).into(),
                    required: *required,
                    choices: None,
                    selection: None,
                }),
        );
        registry
            .register(
                ActionDescriptor {
                    id: (*id).into(),
                    title: (*title).into(),
                    description: (*description).into(),
                    inputs,
                    output: ActionOutputDefinition {
                        output_type: "object".into(),
                    },
                    mutation_class: if *mutates {
                        MutationClass::LocallyMutating
                    } else {
                        MutationClass::ReadOnly
                    },
                    preview_supported: false,
                    required_ports: vec![],
                    availability: ActionAvailability {
                        available: true,
                        reason: None,
                    },
                },
                *handler,
            )
            .expect("continuous-work Action ids are unique");
    }
}
pub fn register_actions(registry: &mut ActionRegistry) {
    register_definitions(registry, &[
        ("central.work.policy", "Read effective Work placement policy", "Resolve exact recognised root/Project source bases and bounded writable destinations without promoting draft policy.", false, policy_action, &[]),
        ("central.now.allocate", "Allocate an agent NOW clearing", "Idempotently allocate one source-owned NOW and T destination per task; preserve ordinary authorised repository writes.", true, allocate_action, &[("task_ref","string",true),("purpose","string",true),("expected_policy_revision","string",true),("participant_refs","array",false),("source_refs","array",false)]),
        ("central.work.validate", "Validate current task write placement", "Revalidate policy, NOW and destination anchors. Return a usable NOW retry destination; do not claim OS enforcement.", false, validate_action, &[("now_ref","string",true),("expected_now_revision","string",true),("expected_policy_revision","string",true),("destination","string",true),("expected_destination_anchor","object",false)]),
        ("central.now.read", "Read allocated NOW source", "Read exact NOW identity, lifecycle and source revision; optionally include current native placement without allocating or re-entering the task.", false, now_read_action, &[("now_ref","string",true),("with_placement","boolean",false)]),
        ("central.now.list", "List allocated NOWs by participant", "List the World's allocated NOW clearings with identity, lifecycle and source revision, optionally filtered to records carrying any of the given participant refs; read-only.", false, now_list_action, &[("participant_refs","array",false)]),
        ("central.time.policy", "Read native civil-time policy", "Read the recognised root IANA timezone and local Day boundary, never the harness timezone.", false, time_action, &[]),
        ("central.day.read", "Read human Day source", "Read a stable DayRef or the current today pointer without replacing the open editor.", false, day_read_action, &[("day_ref","string",false)]),
        ("central.day.ensure", "Ensure the current blank Day", "Create a blank native Day and advance today only under the current authenticated human time policy; never close old writing or clear NOW.", true, day_ensure_action, &[("expected_time_policy_revision","string",true),("expected_authority_revision","string",false)]),
        ("central.day.lifecycle", "Change human Day lifecycle", "Human-authenticated close or reopen with exact content and relation revisions; retain every authored byte.", true, day_lifecycle_action, &[("day_ref","string",true),("expected_revision","string",true),("expected_relations_revision","string",true),("lifecycle","string",true),("expected_authority_revision","string",false)]),
        ("central.now.lifecycle", "Change agent NOW lifecycle", "Independent active/quiescent/closed/archive transitions with fresh placement policy and recorded receiving/obligation checks.", true, now_lifecycle_action, &[("now_ref","string",true),("expected_revision","string",true),("expected_policy_revision","string",true),("lifecycle","string",true),("expected_authority_revision","string",false)]),
        ("central.now.obligations", "Retain NOW obligations", "Append exact native obligation SourceRefs without silently dropping outstanding work.", true, now_obligations_action, &[("now_ref","string",true),("expected_revision","string",true),("obligation_refs","array",true),("expected_authority_revision","string",false)]),
        ("central.temporal.source-history", "Read native temporal source history", "Read SourceRef snapshots from the existing native file-history owner store.", false, history_action, &[("source_ref","string",true),("limit","integer",false),("before","integer",false)]),
    ]);
    extended::register_actions(registry);
}

#[cfg(test)]
mod tests;
