//! `central.agent-set.*` / `central.world-relations.*` write Actions and the
//! `central.world.effective-sources` / `central.agent-set.resolve` read
//! Actions — W10 V2, in the agent-profile-actions pattern: descriptors with
//! typed inputs, handlers that resolve the store for the requested scope,
//! failures mapped to Action result statuses.

use std::collections::BTreeSet;
use serde_json::{json, Value};

use crate::action::{
    ActionAvailability, ActionDescriptor, ActionExecutionContext, ActionInputDefinition,
    ActionOutputDefinition, ActionRegistry, MutationClass,
};
use crate::agent_set_store::{RelationRecordKind, RelationRecordStore, RelationRecordStoreError};
use crate::projectcentral::read_project_manifest;
use crate::result::{ActionResult, ResultStatus};
use crate::root::resolve_central_root;
use crate::world::{AgentSetRegistry, WORLD_DECLARATION_ABSENT_CODE, WorldError, WorldGraph};

pub const AGENT_SET_SAVE_ACTION: &str = "central.agent-set.save";
pub const AGENT_SET_LIST_ACTION: &str = "central.agent-set.list";
pub const AGENT_SET_READ_ACTION: &str = "central.agent-set.read";
pub const AGENT_SET_REMOVE_ACTION: &str = "central.agent-set.remove";
pub const AGENT_SET_RESOLVE_ACTION: &str = "central.agent-set.resolve";
pub const WORLD_RELATIONS_SAVE_ACTION: &str = "central.world-relations.save";
pub const WORLD_RELATIONS_LIST_ACTION: &str = "central.world-relations.list";
pub const WORLD_RELATIONS_READ_ACTION: &str = "central.world-relations.read";
pub const WORLD_RELATIONS_REMOVE_ACTION: &str = "central.world-relations.remove";
pub const WORLD_EFFECTIVE_SOURCES_ACTION: &str = "central.world.effective-sources";

const REF_OUTPUT: &str = "central.relation-record-reading";
const RECEIPT_OUTPUT: &str = "central.relation-record-write-receipt";
const RESOLVE_OUTPUT: &str = "central.resolved-agent-set";
const SOURCES_OUTPUT: &str = "central.effective-world-sources";

pub fn register_agent_set_actions(registry: &mut ActionRegistry) {
    // `kind` names the record kind for the descriptor text only; the input
    // surface is shared by both stores.
    let store_inputs = |kind: &str| -> Vec<ActionInputDefinition> {
        let mut scope = input("scope", "string", true);
        scope.choices = Some(vec!["root".into(), "project".into()]);
        let _ = kind;
        vec![scope, input("project", "string", false)]
    };

    registry.register(ActionDescriptor {
        id: AGENT_SET_SAVE_ACTION.into(),
        title: "Save agent-set".into(),
        description: "Persist an authored central.agent-set/v1 record under compare-and-swap revision discipline (create or update).".into(),
        inputs: {
            let mut inputs = store_inputs("agent-set");
            inputs.push(input("record", "object", true));
            inputs.push(input("expected_revision", "string", false));
            inputs
        },
        output: ActionOutputDefinition { output_type: RECEIPT_OUTPUT.into() },
        mutation_class: MutationClass::LocallyMutating,
        preview_supported: true,
        required_ports: Vec::new(),
        availability: always_available(),
    }, agent_set_save);

    registry.register(ActionDescriptor {
        id: AGENT_SET_LIST_ACTION.into(),
        title: "List agent-sets".into(),
        description: "List authored central.agent-set/v1 records at the requested register.".into(),
        inputs: store_inputs("agent-set"),
        output: ActionOutputDefinition { output_type: format!("list<{REF_OUTPUT}>") },
        mutation_class: MutationClass::ReadOnly,
        preview_supported: false,
        required_ports: Vec::new(),
        availability: always_available(),
    }, agent_set_list);

    registry.register(ActionDescriptor {
        id: AGENT_SET_READ_ACTION.into(),
        title: "Read agent-set".into(),
        description: "Read one authored central.agent-set/v1 record.".into(),
        inputs: {
            let mut inputs = store_inputs("agent-set");
            inputs.push(input("ref", "string", true));
            inputs
        },
        output: ActionOutputDefinition { output_type: REF_OUTPUT.into() },
        mutation_class: MutationClass::ReadOnly,
        preview_supported: false,
        required_ports: Vec::new(),
        availability: always_available(),
    }, agent_set_read);

    registry.register(ActionDescriptor {
        id: AGENT_SET_REMOVE_ACTION.into(),
        title: "Remove agent-set".into(),
        description: "Remove one authored central.agent-set/v1 record at an exact revision.".into(),
        inputs: {
            let mut inputs = store_inputs("agent-set");
            inputs.push(input("ref", "string", true));
            inputs.push(input("expected_revision", "string", true));
            inputs
        },
        output: ActionOutputDefinition { output_type: REF_OUTPUT.into() },
        mutation_class: MutationClass::LocallyMutating,
        preview_supported: true,
        required_ports: Vec::new(),
        availability: always_available(),
    }, agent_set_remove);

    registry.register(ActionDescriptor {
        id: AGENT_SET_RESOLVE_ACTION.into(),
        title: "Resolve agent-set".into(),
        description: "Resolve an authored agent-set: authored membership partitioned against declared availability, nested sets expanded, membership cycles rejected.".into(),
        inputs: {
            let mut inputs = store_inputs("agent-set");
            inputs.push(input("ref", "string", true));
            inputs.push(input("available_agents", "array<string>", false));
            inputs
        },
        output: ActionOutputDefinition { output_type: RESOLVE_OUTPUT.into() },
        mutation_class: MutationClass::ReadOnly,
        preview_supported: false,
        required_ports: Vec::new(),
        availability: always_available(),
    }, agent_set_resolve);

    registry.register(ActionDescriptor {
        id: WORLD_RELATIONS_SAVE_ACTION.into(),
        title: "Save world relations".into(),
        description: "Persist an authored central.world-relations/v1 record under compare-and-swap revision discipline (create or update).".into(),
        inputs: {
            let mut inputs = store_inputs("world-relations");
            inputs.push(input("record", "object", true));
            inputs.push(input("expected_revision", "string", false));
            inputs
        },
        output: ActionOutputDefinition { output_type: RECEIPT_OUTPUT.into() },
        mutation_class: MutationClass::LocallyMutating,
        preview_supported: true,
        required_ports: Vec::new(),
        availability: always_available(),
    }, world_relations_save);

    registry.register(ActionDescriptor {
        id: WORLD_RELATIONS_LIST_ACTION.into(),
        title: "List world relations".into(),
        description: "List authored central.world-relations/v1 records at the requested register.".into(),
        inputs: store_inputs("world-relations"),
        output: ActionOutputDefinition { output_type: format!("list<{REF_OUTPUT}>") },
        mutation_class: MutationClass::ReadOnly,
        preview_supported: false,
        required_ports: Vec::new(),
        availability: always_available(),
    }, world_relations_list);

    registry.register(ActionDescriptor {
        id: WORLD_RELATIONS_READ_ACTION.into(),
        title: "Read world relations".into(),
        description: "Read one authored central.world-relations/v1 record.".into(),
        inputs: {
            let mut inputs = store_inputs("world-relations");
            inputs.push(input("ref", "string", true));
            inputs
        },
        output: ActionOutputDefinition { output_type: REF_OUTPUT.into() },
        mutation_class: MutationClass::ReadOnly,
        preview_supported: false,
        required_ports: Vec::new(),
        availability: always_available(),
    }, world_relations_read);

    registry.register(ActionDescriptor {
        id: WORLD_RELATIONS_REMOVE_ACTION.into(),
        title: "Remove world relations".into(),
        description: "Remove one authored central.world-relations/v1 record at an exact revision.".into(),
        inputs: {
            let mut inputs = store_inputs("world-relations");
            inputs.push(input("ref", "string", true));
            inputs.push(input("expected_revision", "string", true));
            inputs
        },
        output: ActionOutputDefinition { output_type: REF_OUTPUT.into() },
        mutation_class: MutationClass::LocallyMutating,
        preview_supported: true,
        required_ports: Vec::new(),
        availability: always_available(),
    }, world_relations_remove);

    registry.register(ActionDescriptor {
        id: WORLD_EFFECTIVE_SOURCES_ACTION.into(),
        title: "Effective world sources".into(),
        description: "Resolve the effective source relations of a world: ancestry propagation with per-hop provenance, overrides and declared exclusions.".into(),
        inputs: {
            let mut inputs = store_inputs("world-relations");
            inputs.push(input("world_ref", "string", true));
            inputs
        },
        output: ActionOutputDefinition { output_type: SOURCES_OUTPUT.into() },
        mutation_class: MutationClass::ReadOnly,
        preview_supported: false,
        required_ports: Vec::new(),
        availability: always_available(),
    }, world_effective_sources);
}

fn always_available() -> ActionAvailability {
    ActionAvailability {
        available: true,
        reason: None,
    }
}

fn input(name: &str, input_type: &str, required: bool) -> ActionInputDefinition {
    ActionInputDefinition {
        name: name.to_owned(),
        input_type: input_type.to_owned(),
        required,
        choices: None,
        selection: None,
    }
}

fn invalid(action: &str, message: String) -> ActionResult {
    ActionResult::failure(
        Some(action),
        ResultStatus::InvalidInput,
        message,
        None,
    )
}

/// Resolve the store for the requested register. Agent-sets live with the agent
/// profiles: root scope → `Control/agents/agent-sets`; project scope →
/// `<project>/ProjectCentral/agents/agent-sets` with manifest validation. World
/// relations remain under `relations/` at both registers.
fn resolve_store(
    action: &str,
    kind: RelationRecordKind,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> Result<RelationRecordStore, ActionResult> {
    let scope = required_scope(action, input)?;
    let root = resolve_central_root(context.root_options).map_err(|message| {
        ActionResult::failure(Some(action), ResultStatus::InvalidInput, message, None)
    })?;
    match scope.as_str() {
        "root" => Ok(match kind {
            RelationRecordKind::AgentSet => RelationRecordStore::agent_sets_at_root(root.path),
            RelationRecordKind::World => RelationRecordStore::worlds_at_root(root.path),
        }),
        "project" => {
            let project = optional(input, "project")
                .ok_or_else(|| invalid(action, "scope `project` requires `project`".into()))?;
            if !valid_project_member(&project) {
                return Err(invalid(
                    action,
                    "project must be a Central Work-relative path without parent/root components."
                        .into(),
                ));
            }
            let project_root = root.path.join("Work").join(&project);
            if !project_root.is_dir() {
                return Err(invalid(
                    action,
                    format!("project directory is absent: Work/{project}"),
                ));
            }
            let manifest = read_project_manifest(&project_root).map_err(|error| {
                invalid(action, format!("Project does not expose a valid ProjectCentral source: {error}"))
            })?;
            let validation = manifest.validate();
            if !validation.valid {
                return Err(invalid(
                    action,
                    format!("ProjectCentral manifest is invalid: {}", validation.errors.join("; ")),
                ));
            }
            Ok(match kind {
                RelationRecordKind::AgentSet => {
                    RelationRecordStore::agent_sets_in_project(project_root)
                }
                RelationRecordKind::World => RelationRecordStore::worlds_in_project(project_root),
            })
        }
        other => Err(invalid(
            action,
            format!("scope must be `root` or `project`, got `{other}`"),
        )),
    }
}

fn root_path(
    context: &ActionExecutionContext<'_>,
    action: &str,
) -> Result<std::path::PathBuf, ActionResult> {
    resolve_central_root(context.root_options)
        .map(|resolved| resolved.path)
        .map_err(|message| invalid(action, message))
}

/// A project name is one plain path segment under Work.
fn valid_project_member(raw: &str) -> bool {
    !raw.is_empty()
        && raw
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

fn required_scope(action: &str, input: &Value) -> Result<String, ActionResult> {
    optional(input, "scope").ok_or_else(|| invalid(action, "`scope` is required".into()))
}

fn optional(input: &Value, field: &str) -> Option<String> {
    input
        .get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn agent_set_save(
    _registry: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let action = AGENT_SET_SAVE_ACTION;
    let store = match resolve_store(action, RelationRecordKind::AgentSet, input, context) {
        Ok(store) => store,
        Err(failure) => return failure,
    };
    save_action(store, input, action)
}

fn world_relations_save(
    _registry: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let action = WORLD_RELATIONS_SAVE_ACTION;
    let store = match resolve_store(action, RelationRecordKind::World, input, context) {
        Ok(store) => store,
        Err(failure) => return failure,
    };
    save_action(store, input, action)
}

fn save_action(
    store: RelationRecordStore,
    input: &Value,
    action: &str,
) -> ActionResult {
    let record = match input.get("record") {
        Some(record) if record.is_object() => record.clone(),
        _ => return invalid(action, "`record` must be an object".into()),
    };
    let expected_revision = optional(input, "expected_revision");
    match store.save(&record, expected_revision.as_deref()) {
        Ok(receipt) => ActionResult::success(action, json!(receipt)),
        Err(error) => store_failure(action, error),
    }
}

fn agent_set_list(
    _registry: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let action = AGENT_SET_LIST_ACTION;
    let store = match resolve_store(action, RelationRecordKind::AgentSet, input, context) {
        Ok(store) => store,
        Err(failure) => return failure,
    };
    list_action(store, action)
}

fn world_relations_list(
    _registry: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let action = WORLD_RELATIONS_LIST_ACTION;
    let store = match resolve_store(action, RelationRecordKind::World, input, context) {
        Ok(store) => store,
        Err(failure) => return failure,
    };
    list_action(store, action)
}

fn list_action(store: RelationRecordStore, action: &str) -> ActionResult {
    match store.list() {
        Ok(readings) => ActionResult::success(action, json!({ "records": readings })),
        Err(error) => store_failure(action, error),
    }
}

fn agent_set_read(
    _registry: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let action = AGENT_SET_READ_ACTION;
    let store = match resolve_store(action, RelationRecordKind::AgentSet, input, context) {
        Ok(store) => store,
        Err(failure) => return failure,
    };
    read_action(store, input, action)
}

fn world_relations_read(
    _registry: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let action = WORLD_RELATIONS_READ_ACTION;
    let store = match resolve_store(action, RelationRecordKind::World, input, context) {
        Ok(store) => store,
        Err(failure) => return failure,
    };
    read_action(store, input, action)
}

fn read_action(store: RelationRecordStore, input: &Value, action: &str) -> ActionResult {
    let ref_ = match optional(input, "ref") {
        Some(ref_) => ref_,
        None => return invalid(action, "`ref` is required".into()),
    };
    match store.read(&ref_) {
        Ok(reading) => ActionResult::success(action, json!(reading)),
        Err(error) => store_failure(action, error),
    }
}

fn agent_set_remove(
    _registry: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let action = AGENT_SET_REMOVE_ACTION;
    let store = match resolve_store(action, RelationRecordKind::AgentSet, input, context) {
        Ok(store) => store,
        Err(failure) => return failure,
    };
    remove_action(store, input, action)
}

fn world_relations_remove(
    _registry: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let action = WORLD_RELATIONS_REMOVE_ACTION;
    let store = match resolve_store(action, RelationRecordKind::World, input, context) {
        Ok(store) => store,
        Err(failure) => return failure,
    };
    remove_action(store, input, action)
}

fn remove_action(store: RelationRecordStore, input: &Value, action: &str) -> ActionResult {
    let ref_ = match optional(input, "ref") {
        Some(ref_) => ref_,
        None => return invalid(action, "`ref` is required".into()),
    };
    let expected_revision = match optional(input, "expected_revision") {
        Some(revision) => revision,
        None => return invalid(action, "`expected_revision` is required".into()),
    };
    match store.remove(&ref_, &expected_revision) {
        Ok(reading) => ActionResult::success(action, json!(reading)),
        Err(error) => store_failure(action, error),
    }
}

fn agent_set_resolve(
    _registry: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let action = AGENT_SET_RESOLVE_ACTION;
    let store = match resolve_store(action, RelationRecordKind::AgentSet, input, context) {
        Ok(store) => store,
        Err(failure) => return failure,
    };
    let ref_ = match optional(input, "ref") {
        Some(ref_) => ref_,
        None => return invalid(action, "`ref` is required".into()),
    };
    let mut registry = AgentSetRegistry::default();
    // Root sets participate so nested refs resolve across registers; a
    // project register is loaded last so its records override same-ref root
    // records without rewriting them.
    if store.is_project_scope() {
        let root_store = match root_path(context, action) {
            Ok(path) => RelationRecordStore::agent_sets_at_root(path),
            Err(failure) => return failure,
        };
        for record in match root_store.load_typed::<crate::world::AgentSetRecord>() {
            Ok(records) => records,
            Err(error) => return store_failure(action, error),
        } {
            let _ = registry.insert(record);
        }
    }
    let records = match store.load_typed::<crate::world::AgentSetRecord>() {
        Ok(records) => records,
        Err(error) => return store_failure(action, error),
    };
    for record in records {
        if let Err(error) = registry.insert(record) {
            return invalid(action, error.to_string());
        }
    }
    let available: Option<BTreeSet<String>> = input
        .get("available_agents")
        .and_then(|value| value.as_array())
        .map(|entries| {
            entries
                .iter()
                .filter_map(|entry| entry.as_str())
                .map(str::to_owned)
                .collect()
        });
    let set_ref = match crate::world::AgentSetRef::new(ref_.clone()) {
        Ok(value) => value,
        Err(error) => return invalid(action, error.to_string()),
    };
    match registry.resolve(&set_ref, available.as_ref()) {
        Ok(resolved) => ActionResult::success(action, json!(resolved)),
        Err(error) => invalid(action, error.to_string()),
    }
}

fn world_effective_sources(
    _registry: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let action = WORLD_EFFECTIVE_SOURCES_ACTION;
    let store = match resolve_store(action, RelationRecordKind::World, input, context) {
        Ok(store) => store,
        Err(failure) => return failure,
    };
    let world_ref = match optional(input, "world_ref") {
        Some(world_ref) => world_ref,
        None => return invalid(action, "`world_ref` is required".into()),
    };
    let mut graph = WorldGraph::default();
    let root_store = match root_path(context, action) {
        Ok(path) => RelationRecordStore::worlds_at_root(path),
        Err(failure) => return failure,
    };
    for record in match root_store.load_typed::<crate::world::WorldRecord>() {
        Ok(records) => records,
        Err(error) => return store_failure(action, error),
    } {
        if let Err(error) = graph.insert(record) {
            return invalid(action, error.to_string());
        }
    }
    let records = match store.load_typed::<crate::world::WorldRecord>() {
        Ok(records) => records,
        Err(error) => return store_failure(action, error),
    };
    for record in records {
        if let Err(error) = graph.insert(record) {
            return invalid(action, error.to_string());
        }
    }
    let target = match crate::world::WorldRef::new(world_ref.clone()) {
        Ok(value) => value,
        Err(error) => return invalid(action, error.to_string()),
    };
    match graph.effective_sources(&target) {
        Ok(sources) => ActionResult::success(action, json!({ "world_ref": world_ref, "sources": sources })),
        // A World ref with no authored record at all is *absent*, not invalid:
        // it is the ordinary state of a project that declares no world of its
        // own, and the answer to it is to apply the root lineage by convention.
        // Naming it in the error code lets a consumer tell that apart from a
        // declaration it could not read, which must never widen what a turn
        // receives. Both share the `invalid_input` status, so without the code
        // the difference would live only in the message text.
        Err(error @ WorldError::MissingWorld(_)) => ActionResult::failure_coded(
            Some(action),
            ResultStatus::InvalidInput,
            WORLD_DECLARATION_ABSENT_CODE,
            error.to_string(),
            Some(json!({ "state": "absent", "world_ref": world_ref })),
        ),
        Err(error) => invalid(action, error.to_string()),
    }
}

fn store_failure(action: &str, error: RelationRecordStoreError) -> ActionResult {
    let status = match &error {
        RelationRecordStoreError::NotFound(_)
        | RelationRecordStoreError::MissingForUpdate { .. } => ResultStatus::VerificationFailure,
        RelationRecordStoreError::AlreadyExists { .. }
        | RelationRecordStoreError::RevisionConflict { .. }
        | RelationRecordStoreError::RevisionNotAdvanced { .. }
        | RelationRecordStoreError::InvalidRecord(_)
        | RelationRecordStoreError::InvalidRef(_)
        | RelationRecordStoreError::RefMismatch { .. }
        | RelationRecordStoreError::SourcePathMismatch { .. }
        | RelationRecordStoreError::UnsafeRoot(_)
        | RelationRecordStoreError::UnsafeSource(_) => ResultStatus::InvalidInput,
        RelationRecordStoreError::Io(_) => ResultStatus::InternalFailure,
    };
    ActionResult::failure(Some(action), status, error.to_string(), None)
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::create_core_action_registry;
    use central_connector_sdk::{ConnectorContext, ConnectorRegistry};
    use crate::root::{initialize_central, RootOptions};
    use serde_json::json;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn fixture_root() -> PathBuf {
        use std::sync::atomic::AtomicU64;
        static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(0);
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let sequence = NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "central-agent-set-actions-{}-{nonce}-{sequence}",
            std::process::id()
        ));
        initialize_central(&root).unwrap();
        root
    }

    fn registry() -> ActionRegistry {
        let mut registry = create_core_action_registry();
        register_agent_set_actions(&mut registry);
        registry
    }

    fn context<'a>(
        root: &'a PathBuf,
        options: &'a mut Option<RootOptions>,
        connectors: &'a mut Option<ConnectorRegistry>,
        connector_context: &'a mut Option<ConnectorContext>,
    ) -> ActionExecutionContext<'a> {
        *options = Some(RootOptions {
            explicit_root: Some(root.clone()),
            configured_root: None,
            home: None,
        });
        *connectors = Some(ConnectorRegistry::default());
        *connector_context = Some(ConnectorContext {
            platform: "test".into(),
        });
        ActionExecutionContext {
            root_options: options.as_ref().unwrap(),
            connectors: connectors.as_ref().unwrap(),
            connector_context: connector_context.as_ref().unwrap(),
        }
    }

    #[test]
    fn agent_set_actions_round_trip_and_resolve_authored_against_availability() {
        let root = fixture_root();
        let registry = registry();
        let mut options = None;
        let mut connectors = None;
        let mut connector_context = None;
        let context = context(&root, &mut options, &mut connectors, &mut connector_context);

        let created = registry.execute(
            AGENT_SET_SAVE_ACTION,
            &json!({
                "scope": "root",
                "record": {
                    "schema": "central.agent-set/v1",
                    "ref": "control-operators",
                    "revision": "r1",
                    "members": [
                        {"kind": "agent", "agent_ref": "agent:hermes"},
                        {"kind": "agent-set", "agent_set_ref": "field-operators"}
                    ]
                }
            }),
            &context,
        );
        assert!(created.ok, "{created:?}");
        assert_eq!(created.data.as_ref().unwrap()["created"], true);

        // Nested set for the resolution.
        let nested = registry.execute(
            AGENT_SET_SAVE_ACTION,
            &json!({
                "scope": "root",
                "record": {
                    "schema": "central.agent-set/v1",
                    "ref": "field-operators",
                    "revision": "r1",
                    "members": [
                        {"kind": "agent", "agent_ref": "agent:picker"},
                        {"kind": "agent", "agent_ref": "agent:gardener"}
                    ]
                }
            }),
            &context,
        );
        assert!(nested.ok, "{nested:?}");

        // Every authored agent available: resolved == authored.
        let all_available = registry.execute(
            AGENT_SET_RESOLVE_ACTION,
            &json!({"scope": "root", "ref": "control-operators"}),
            &context,
        );
        assert!(all_available.ok, "{all_available:?}");
        let resolved = &all_available.data.as_ref().unwrap();
        assert_eq!(resolved["authored_agents"].as_array().unwrap().len(), 3);
        assert_eq!(resolved["resolved_agents"].as_array().unwrap().len(), 3);
        assert_eq!(resolved["unavailable_agents"].as_array().unwrap().len(), 0);
        assert_eq!(resolved["nested_sets"].as_array().unwrap().len(), 1);

        // Availability partitions the resolution without rewriting authored
        // membership: gardener is unavailable, the record still carries it.
        let partial = registry.execute(
            AGENT_SET_RESOLVE_ACTION,
            &json!({
                "scope": "root",
                "ref": "control-operators",
                "available_agents": ["agent:hermes", "agent:picker"]
            }),
            &context,
        );
        assert!(partial.ok, "{partial:?}");
        let resolved = &partial.data.as_ref().unwrap();
        assert_eq!(
            resolved["resolved_agents"].as_array().unwrap(),
            &json!(["agent:hermes", "agent:picker"]).as_array().unwrap().clone()
        );
        assert_eq!(
            resolved["unavailable_agents"].as_array().unwrap(),
            &json!(["agent:gardener"]).as_array().unwrap().clone()
        );
        let stored = registry.execute(
            AGENT_SET_READ_ACTION,
            &json!({"scope": "root", "ref": "control-operators"}),
            &context,
        );
        assert_eq!(
            stored.data.as_ref().unwrap()["record"]["members"]
                .as_array()
                .unwrap()
                .len(),
            2,
            "authored membership is unchanged by resolution"
        );

        // Membership cycles are rejected at resolve time.
        registry
            .execute(
                AGENT_SET_SAVE_ACTION,
                &json!({
                    "scope": "root",
                    "record": {
                        "schema": "central.agent-set/v1",
                        "ref": "loop-a",
                        "revision": "r1",
                        "members": [{"kind": "agent-set", "agent_set_ref": "loop-b"}]
                    }
                }),
                &context,
            );
        registry
            .execute(
                AGENT_SET_SAVE_ACTION,
                &json!({
                    "scope": "root",
                    "record": {
                        "schema": "central.agent-set/v1",
                        "ref": "loop-b",
                        "revision": "r1",
                        "members": [{"kind": "agent-set", "agent_set_ref": "loop-a"}]
                    }
                }),
                &context,
            );
        let cyclic = registry.execute(
            AGENT_SET_RESOLVE_ACTION,
            &json!({"scope": "root", "ref": "loop-a"}),
            &context,
        );
        assert!(!cyclic.ok);
        assert!(cyclic.error.unwrap().message.contains("cycle"));

        // Update discipline through the Action surface.
        let conflict = registry.execute(
            AGENT_SET_SAVE_ACTION,
            &json!({
                "scope": "root",
                "expected_revision": "wrong",
                "record": {
                    "schema": "central.agent-set/v1",
                    "ref": "field-operators",
                    "revision": "r2",
                    "members": []
                }
            }),
            &context,
        );
        assert!(!conflict.ok);
        let updated = registry.execute(
            AGENT_SET_SAVE_ACTION,
            &json!({
                "scope": "root",
                "expected_revision": "r1",
                "record": {
                    "schema": "central.agent-set/v1",
                    "ref": "field-operators",
                    "revision": "r2",
                    "members": [{"kind": "agent", "agent_ref": "agent:picker"}]
                }
            }),
            &context,
        );
        assert!(updated.ok, "{updated:?}");
        assert_eq!(updated.data.as_ref().unwrap()["previous_revision"], "r1");

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn world_relations_actions_persist_and_propagate_effective_sources() {
        let root = fixture_root();
        let registry = registry();
        let mut options = None;
        let mut connectors = None;
        let mut connector_context = None;
        let context = context(&root, &mut options, &mut connectors, &mut connector_context);

        // The root world declares one source; the child world overrides it
        // and excludes another.
        let root_world = registry.execute(
            WORLD_RELATIONS_SAVE_ACTION,
            &json!({
                "scope": "root",
                "record": {
                    "schema": "central.world-relations/v1",
                    "ref": "control:root",
                    "revision": "w1",
                    "sources": [
                        {"ref": "central:source:control:root:identity",
                         "revision": "1",
                         "authority": "human-authored",
                         "treatment": "canonical"},
                        {"ref": "central:source:control:root:sealed",
                         "revision": "1",
                         "authority": "human-authored",
                         "treatment": "canonical"}
                    ]
                }
            }),
            &context,
        );
        assert!(root_world.ok, "{root_world:?}");

        let child = registry.execute(
            WORLD_RELATIONS_SAVE_ACTION,
            &json!({
                "scope": "root",
                "record": {
                    "schema": "central.world-relations/v1",
                    "ref": "project:garden",
                    "revision": "w1",
                    "parent": "control:root",
                    "sources": [
                        {"ref": "central:source:control:root:identity",
                         "revision": "2",
                         "authority": "human-authored",
                         "treatment": "retain-native"}
                    ],
                    "excluded_sources": ["central:source:control:root:sealed"]
                }
            }),
            &context,
        );
        assert!(child.ok, "{child:?}");

        let effective = registry.execute(
            WORLD_EFFECTIVE_SOURCES_ACTION,
            &json!({"scope": "root", "world_ref": "project:garden"}),
            &context,
        );
        assert!(effective.ok, "{effective:?}");
        let sources = effective.data.as_ref().unwrap()["sources"]
            .as_array()
            .unwrap()
            .clone();
        assert_eq!(sources.len(), 2, "{sources:?}");
        let identity = sources
            .iter()
            .find(|s| s["ref"] == "central:source:control:root:identity")
            .expect("identity source is effective");
        assert_eq!(identity["state"], "available");
        assert_eq!(identity["effective_revision"], "2", "the child override wins");
        assert_eq!(
            identity["propagation_path"].as_array().unwrap().len(),
            2,
            "the ancestry path is recorded"
        );
        let sealed = sources
            .iter()
            .find(|s| s["ref"] == "central:source:control:root:sealed")
            .expect("the declared source appears even when excluded");
        assert_eq!(sealed["state"], "excluded", "the child's exclusion wins");

        fs::remove_dir_all(root).unwrap();
    }

    /// Absence and unreadability share the `invalid_input` status, so a
    /// consumer could only tell them apart by reading prose. They are
    /// different facts — absence is the ordinary state of a project that
    /// declares no world of its own, while a malformed declaration must never
    /// widen what a turn receives — so absence is named in the error code.
    #[test]
    fn an_absent_world_is_named_in_the_error_code_and_a_malformed_one_is_not() {
        let root = fixture_root();
        let registry = registry();
        let mut options = None;
        let mut connectors = None;
        let mut connector_context = None;
        let context = context(&root, &mut options, &mut connectors, &mut connector_context);

        let absent = registry.execute(
            WORLD_EFFECTIVE_SOURCES_ACTION,
            &json!({"scope": "root", "world_ref": "project:never-declared"}),
            &context,
        );
        assert!(!absent.ok);
        assert_eq!(absent.status, ResultStatus::InvalidInput);
        let absence = absent.error.as_ref().unwrap();
        assert_eq!(
            absence.code, WORLD_DECLARATION_ABSENT_CODE,
            "absence is named in the code, not only in the message"
        );
        assert!(
            absence.message.contains("missing World"),
            "the prose is unchanged: {}",
            absence.message
        );

        // A malformed request is not absence, and must not claim to be.
        let malformed = registry.execute(
            WORLD_EFFECTIVE_SOURCES_ACTION,
            &json!({"scope": "root"}),
            &context,
        );
        assert!(!malformed.ok);
        assert_ne!(
            malformed.error.as_ref().unwrap().code,
            WORLD_DECLARATION_ABSENT_CODE,
            "only a genuinely absent world wears the absent code"
        );

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn project_scope_uses_the_fractal_container_and_overrides_root_sets_at_resolve() {
        let root = fixture_root();
        let project = root.join("Work/garden");
        fs::create_dir_all(&project).unwrap();
        crate::projectcentral_ops::initialize_projectcentral(&root, &project, "garden").unwrap();
        let registry = registry();
        let mut options = None;
        let mut connectors = None;
        let mut connector_context = None;
        let context = context(&root, &mut options, &mut connectors, &mut connector_context);

        let saved = registry.execute(
            AGENT_SET_SAVE_ACTION,
            &json!({
                "scope": "project",
                "project": "garden",
                "record": {
                    "schema": "central.agent-set/v1",
                    "ref": "garden-crew",
                    "revision": "p1",
                    "members": [{"kind": "agent", "agent_ref": "agent:gardener"}]
                }
            }),
            &context,
        );
        assert!(saved.ok, "{saved:?}");
        assert!(saved.data.as_ref().unwrap()["source_path"]
            .as_str()
            .unwrap()
            .starts_with("ProjectCentral/agents/agent-sets/"));

        let listed = registry.execute(
            AGENT_SET_LIST_ACTION,
            &json!({"scope": "project", "project": "garden"}),
            &context,
        );
        assert_eq!(listed.data.as_ref().unwrap()["records"].as_array().unwrap().len(), 1);

        fs::remove_dir_all(root).unwrap();
    }
}
