//! Native continuous-work operations over Central's existing source ownership.
//! No personal installation, default-policy adoption, model call or shell rewrite.
pub mod placement;
pub mod source;

use crate::action::{ActionAvailability, ActionDescriptor, ActionExecutionContext, ActionInputDefinition, ActionOutputDefinition, ActionRegistry, MutationClass};
use crate::result::{ActionResult, ResultStatus};
use serde_json::{json, Value};
use std::io;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};
use source::{invalid, Scope};

/// Injectable owner clock for controlled Worlds; the public Action always uses
/// the process clock rather than accepting an untrusted caller's timestamp.
pub fn execute_at(central: &Path, operation: &str, input: &Value, now: u64) -> io::Result<Value> {
    let project = match input.get("project") {
        None | Some(Value::Null) => None,
        Some(Value::String(project)) => Some(project.as_str()),
        _ => return Err(invalid("project must be a string or absent for root agency")),
    };
    let scope = Scope::resolve(central, project)?;
    match operation {
        "policy" => {
            let _locks = source::lock(&scope)?;
            Ok(serde_json::to_value(placement::effective_policy(&scope, now)?)?)
        }
        "allocate" => placement::allocate(&scope, input, now),
        "validate" => placement::validate(&scope, input, now),
        "now_read" => {
            let _locks = source::lock(&scope)?;
            let (record, reading) = placement::read_now(&scope, source::text(input, "now_ref")?)?;
            Ok(json!({"schema":"central.now-reading/v1","record":record,"source":reading.source,"revision":reading.revision,"automatic_agent_or_model_invocation":false}))
        }
        _ => Err(invalid("unknown continuous-work operation")),
    }
}
fn execute(action: &str, operation: &str, input: &Value, context: &ActionExecutionContext<'_>) -> ActionResult {
    let result = (|| {
        let root = crate::root::resolve_central_root(context.root_options).map_err(invalid)?;
        let now = SystemTime::now().duration_since(UNIX_EPOCH).map_err(io::Error::other)?.as_secs();
        execute_at(&root.path, operation, input, now)
    })();
    match result {
        Ok(value) if value.get("allowed") == Some(&Value::Bool(false)) => ActionResult::failure_coded(
            Some(action), ResultStatus::UnavailableCapability, "placement_rejected",
            "Destination is outside the current task's authorised write bounds.", Some(value)),
        Ok(value) => ActionResult::success(action, value),
        Err(error) => {
            let (status, code) = match error.kind() {
                io::ErrorKind::AlreadyExists => (ResultStatus::VerificationFailure, "stale_basis_or_identity_conflict"),
                io::ErrorKind::PermissionDenied => (ResultStatus::UnavailableCapability, "policy_or_source_denied"),
                io::ErrorKind::NotFound => (ResultStatus::UnavailableCapability, "source_or_scope_unavailable"),
                io::ErrorKind::InvalidInput | io::ErrorKind::InvalidData => (ResultStatus::InvalidInput, "invalid_native_source_or_request"),
                _ => (ResultStatus::InternalFailure, "native_operation_failed"),
            };
            ActionResult::failure_coded(Some(action), status, code, error.to_string(), Some(json!({"retry_policy_action":"central.work.policy","automatic_authority_widening":false,"outside_writes_prevented":false})))
        }
    }
}
macro_rules! handler {
    ($name:ident, $action:literal, $operation:literal) => {
        fn $name(_: &ActionRegistry, input: &Value, context: &ActionExecutionContext<'_>) -> ActionResult {
            execute($action, $operation, input, context)
        }
    };
}
handler!(policy_action, "central.work.policy", "policy");
handler!(allocate_action, "central.now.allocate", "allocate");
handler!(validate_action, "central.work.validate", "validate");
handler!(now_read_action, "central.now.read", "now_read");

pub fn register_actions(registry: &mut ActionRegistry) {
    use crate::action::ActionHandler;
    let actions: [(&str, &str, &str, bool, ActionHandler, &[(&str, &str, bool)]); 4] = [
        ("central.work.policy", "Read effective Work placement policy", "Resolve exact recognised root/Project source bases and bounded writable destinations without promoting draft policy.", false, policy_action, &[]),
        ("central.now.allocate", "Allocate an agent NOW clearing", "Idempotently allocate one source-owned NOW and T destination per task; preserve ordinary authorised repository writes.", true, allocate_action, &[("task_ref","string",true),("purpose","string",true),("expected_policy_revision","string",true),("participant_refs","array",false),("source_refs","array",false)]),
        ("central.work.validate", "Validate current task write placement", "Revalidate policy, NOW and destination anchors. Return a usable NOW retry destination; do not claim OS enforcement.", false, validate_action, &[("now_ref","string",true),("expected_now_revision","string",true),("expected_policy_revision","string",true),("destination","string",true),("expected_destination_anchor","object",false)]),
        ("central.now.read", "Read allocated NOW source", "Read exact NOW identity, lifecycle and source revision at root or Project scope.", false, now_read_action, &[("now_ref","string",true)]),
    ];
    for (id, title, description, mutates, handler, fields) in actions {
        let mut inputs = vec![ActionInputDefinition { name: "project".into(), input_type: "string".into(), required: false, choices: None, selection: None }];
        inputs.extend(fields.iter().map(|(name, kind, required)| ActionInputDefinition {
            name: (*name).into(), input_type: (*kind).into(), required: *required, choices: None, selection: None,
        }));
        registry.register(ActionDescriptor {
            id: id.into(), title: title.into(), description: description.into(), inputs,
            output: ActionOutputDefinition { output_type: "object".into() },
            mutation_class: if mutates { MutationClass::LocallyMutating } else { MutationClass::ReadOnly },
            preview_supported: false, required_ports: vec![], availability: ActionAvailability { available: true, reason: None },
        }, handler).expect("continuous-work Action ids are unique");
    }
}

#[cfg(test)]
mod tests;
