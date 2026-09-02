use crate::action::{
    ActionAvailability, ActionDescriptor, ActionExecutionContext, ActionInputDefinition,
    ActionOutputDefinition, ActionRegistry, MutationClass,
};
use crate::agent_profile::{AgentProfile, AgentProfileScope, AGENT_PROFILE_SCHEMA};
use crate::agent_profile_store::{AgentProfileStore, AgentProfileStoreError};
use crate::projectcentral::read_project_manifest;
use crate::result::{ActionResult, ResultStatus};
use crate::root::resolve_central_root;
use serde_json::{json, to_value, Value};
use std::collections::BTreeSet;
use std::path::{Component, Path};

pub const AGENT_PROFILE_LIST_ACTION: &str = "agent-profile.list";
pub const AGENT_PROFILE_READ_ACTION: &str = "agent-profile.read";
pub const AGENT_PROFILE_SAVE_ACTION: &str = "agent-profile.save";
pub const AGENT_PROFILE_REMOVE_ACTION: &str = "agent-profile.remove";

fn input(name: &str, input_type: &str, required: bool) -> ActionInputDefinition {
    ActionInputDefinition {
        name: name.to_owned(),
        input_type: input_type.to_owned(),
        required,
        choices: None,
        selection: None,
    }
}

fn scope_input() -> ActionInputDefinition {
    let mut value = input("scope", "string", true);
    value.choices = Some(vec!["personal".into(), "project".into()]);
    value
}

fn descriptor(
    id: &str,
    title: &str,
    description: &str,
    mutation_class: MutationClass,
    output_type: &str,
    inputs: Vec<ActionInputDefinition>,
) -> ActionDescriptor {
    ActionDescriptor {
        id: id.into(),
        title: title.into(),
        description: description.into(),
        inputs,
        output: ActionOutputDefinition {
            output_type: output_type.into(),
        },
        mutation_class,
        preview_supported: false,
        required_ports: vec![],
        availability: ActionAvailability {
            available: true,
            reason: None,
        },
    }
}

fn required_text(input: &Value, field: &str, action: &str) -> Result<String, ActionResult> {
    input
        .get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| {
            ActionResult::failure(
                Some(action),
                ResultStatus::InvalidInput,
                format!("{action} requires {field}."),
                None,
            )
        })
}

fn optional_text(input: &Value, field: &str) -> Option<String> {
    input
        .get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn valid_project_member(raw: &str) -> bool {
    let path = Path::new(raw);
    !raw.trim().is_empty()
        && raw == raw.trim()
        && !path.is_absolute()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}

fn resolve_store(
    action: &str,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> Result<AgentProfileStore, ActionResult> {
    let scope = required_text(input, "scope", action)?;
    let root = resolve_central_root(context.root_options).map_err(|message| {
        ActionResult::failure(Some(action), ResultStatus::InvalidInput, message, None)
    })?;
    match scope.as_str() {
        "personal" => Ok(AgentProfileStore::personal(root.path)),
        "project" => {
            let project = required_text(input, "project", action)?;
            if !valid_project_member(&project) {
                return Err(ActionResult::failure(
                    Some(action),
                    ResultStatus::InvalidInput,
                    "project must be a Central Work-relative path without parent/root components.",
                    None,
                ));
            }
            let project_root = root.path.join("Work").join(project);
            if !project_root.is_dir() {
                return Err(ActionResult::failure(
                    Some(action),
                    ResultStatus::InvalidInput,
                    "Project directory does not exist in Central Work.",
                    None,
                ));
            }
            let manifest = read_project_manifest(&project_root).map_err(|error| {
                ActionResult::failure(
                    Some(action),
                    ResultStatus::InvalidInput,
                    format!("Project does not expose a valid ProjectCentral source: {error}"),
                    None,
                )
            })?;
            let validation = manifest.validate();
            if !validation.valid {
                return Err(ActionResult::failure(
                    Some(action),
                    ResultStatus::VerificationFailure,
                    "ProjectCentral manifest is invalid.",
                    Some(json!({ "errors": validation.errors })),
                ));
            }
            Ok(AgentProfileStore::project(project_root))
        }
        _ => Err(ActionResult::failure(
            Some(action),
            ResultStatus::InvalidInput,
            "scope must be personal or project.",
            None,
        )),
    }
}

fn validate_ref_list(field: &str, refs: &[String]) -> Result<(), String> {
    let mut seen = BTreeSet::new();
    for value in refs {
        if value.trim().is_empty() || value != value.trim() {
            return Err(format!("{field} contains an empty or untrimmed ref"));
        }
        if !seen.insert(value) {
            return Err(format!("{field} repeats ref {value}"));
        }
    }
    Ok(())
}

/// JSON Actions are an ingestion boundary. The typed store assumes a canonical
/// AgentProfile value; validate the public v1 shape before passing it through.
/// This does not resolve World ancestry, trust, runtime Profile or authority.
fn validate_ingested_profile(profile: &AgentProfile) -> Result<(), String> {
    if profile.schema != AGENT_PROFILE_SCHEMA {
        return Err(format!(
            "unsupported AgentProfile schema {}; expected {AGENT_PROFILE_SCHEMA}",
            profile.schema
        ));
    }
    for (field, value) in [
        ("AgentProfile ref", profile.profile_ref.as_str()),
        ("AgentProfile revision", profile.revision.as_str()),
        ("Agent ref", profile.agent_ref.as_str()),
    ] {
        if value.trim().is_empty() || value != value.trim() {
            return Err(format!("{field} must be non-empty without surrounding whitespace"));
        }
    }
    if profile.source_profile_ref.as_ref() == Some(&profile.profile_ref) {
        return Err("AgentProfile cannot source itself".into());
    }
    if profile.ratified_world_refs.is_empty() {
        return Err("AgentProfile requires at least one ratified World".into());
    }
    let mut worlds = BTreeSet::new();
    for world in &profile.ratified_world_refs {
        if !worlds.insert(world) {
            return Err(format!("AgentProfile repeats ratified World {world}"));
        }
    }
    for (field, refs) in [
        ("governance refs", profile.governance_refs.as_slice()),
        ("Skill refs", profile.skill_refs.as_slice()),
        ("SkillSet refs", profile.skill_set_refs.as_slice()),
        ("Method refs", profile.method_refs.as_slice()),
        ("Knowledge source refs", profile.knowledge_source_refs.as_slice()),
        (
            "Central Computer access-intent refs",
            profile.computer_access_intent_refs.as_slice(),
        ),
        ("placement-intent refs", profile.placement_intent_refs.as_slice()),
        (
            "operative-requirement refs",
            profile.operative_requirement_refs.as_slice(),
        ),
        (
            "material-requirement refs",
            profile.material_requirement_refs.as_slice(),
        ),
        ("provenance refs", profile.provenance_refs.as_slice()),
    ] {
        validate_ref_list(field, refs)?;
    }
    if profile
        .role
        .as_deref()
        .is_some_and(|value| value.trim().is_empty())
        || profile
            .purpose
            .as_deref()
            .is_some_and(|value| value.trim().is_empty())
    {
        return Err("AgentProfile role/purpose cannot be empty when supplied".into());
    }
    Ok(())
}

fn store_failure(action: &str, error: AgentProfileStoreError) -> ActionResult {
    let status = match error {
        AgentProfileStoreError::Io(_) => ResultStatus::InternalFailure,
        AgentProfileStoreError::UnsafeRoot(_)
        | AgentProfileStoreError::UnsafeSource(_)
        | AgentProfileStoreError::InvalidProfile(_)
        | AgentProfileStoreError::ScopeMismatch { .. }
        | AgentProfileStoreError::RefMismatch { .. }
        | AgentProfileStoreError::SourcePathMismatch { .. } => ResultStatus::VerificationFailure,
        AgentProfileStoreError::InvalidProfileRef(_)
        | AgentProfileStoreError::NotFound(_)
        | AgentProfileStoreError::AlreadyExists { .. }
        | AgentProfileStoreError::MissingForUpdate { .. }
        | AgentProfileStoreError::RevisionConflict { .. }
        | AgentProfileStoreError::RevisionNotAdvanced { .. }
        | AgentProfileStoreError::AgentIdentityChanged { .. } => ResultStatus::InvalidInput,
    };
    ActionResult::failure(Some(action), status, error.to_string(), None)
}

fn list_action(
    _registry: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let store = match resolve_store(AGENT_PROFILE_LIST_ACTION, input, context) {
        Ok(store) => store,
        Err(result) => return result,
    };
    store
        .list()
        .map(|profiles| {
            ActionResult::success(
                AGENT_PROFILE_LIST_ACTION,
                json!({
                    "scope": match store.scope() {
                        AgentProfileScope::Personal => "personal",
                        AgentProfileScope::Project => "project",
                    },
                    "profiles": profiles,
                    "source_payloads_disclosed": false,
                }),
            )
        })
        .unwrap_or_else(|error| store_failure(AGENT_PROFILE_LIST_ACTION, error))
}

fn read_action(
    _registry: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let store = match resolve_store(AGENT_PROFILE_READ_ACTION, input, context) {
        Ok(store) => store,
        Err(result) => return result,
    };
    let profile_ref = match required_text(input, "profile_ref", AGENT_PROFILE_READ_ACTION) {
        Ok(value) => value,
        Err(result) => return result,
    };
    store
        .read(&profile_ref)
        .map(|reading| {
            ActionResult::success(
                AGENT_PROFILE_READ_ACTION,
                to_value(reading).expect("AgentProfile reading serializes"),
            )
        })
        .unwrap_or_else(|error| store_failure(AGENT_PROFILE_READ_ACTION, error))
}

fn save_action(
    _registry: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let store = match resolve_store(AGENT_PROFILE_SAVE_ACTION, input, context) {
        Ok(store) => store,
        Err(result) => return result,
    };
    let Some(raw_profile) = input.get("profile") else {
        return ActionResult::failure(
            Some(AGENT_PROFILE_SAVE_ACTION),
            ResultStatus::InvalidInput,
            "agent-profile.save requires profile.",
            None,
        );
    };
    let profile = match serde_json::from_value::<AgentProfile>(raw_profile.clone()) {
        Ok(profile) => profile,
        Err(error) => {
            return ActionResult::failure(
                Some(AGENT_PROFILE_SAVE_ACTION),
                ResultStatus::InvalidInput,
                format!("profile is not a valid {AGENT_PROFILE_SCHEMA} document: {error}"),
                None,
            )
        }
    };
    if let Err(error) = validate_ingested_profile(&profile) {
        return ActionResult::failure(
            Some(AGENT_PROFILE_SAVE_ACTION),
            ResultStatus::InvalidInput,
            error,
            None,
        );
    }
    let expected_revision = optional_text(input, "expected_revision");
    store
        .save(&profile, expected_revision.as_deref())
        .map(|receipt| {
            ActionResult::success(
                AGENT_PROFILE_SAVE_ACTION,
                to_value(receipt).expect("AgentProfile receipt serializes"),
            )
        })
        .unwrap_or_else(|error| store_failure(AGENT_PROFILE_SAVE_ACTION, error))
}

fn remove_action(
    _registry: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let store = match resolve_store(AGENT_PROFILE_REMOVE_ACTION, input, context) {
        Ok(store) => store,
        Err(result) => return result,
    };
    let profile_ref = match required_text(input, "profile_ref", AGENT_PROFILE_REMOVE_ACTION) {
        Ok(value) => value,
        Err(result) => return result,
    };
    let expected_revision =
        match required_text(input, "expected_revision", AGENT_PROFILE_REMOVE_ACTION) {
            Ok(value) => value,
            Err(result) => return result,
        };
    store
        .remove(&profile_ref, &expected_revision)
        .map(|reading| {
            ActionResult::success(
                AGENT_PROFILE_REMOVE_ACTION,
                json!({
                    "removed": reading,
                    "agent_identity_deleted": false,
                    "runtime_state_deleted": false,
                }),
            )
        })
        .unwrap_or_else(|error| store_failure(AGENT_PROFILE_REMOVE_ACTION, error))
}

pub fn register_agent_profile_actions(registry: &mut ActionRegistry) {
    let common = vec![scope_input(), input("project", "string", false)];

    registry
        .register(
            descriptor(
                AGENT_PROFILE_LIST_ACTION,
                "List Agent Profiles",
                "List durable Central AgentProfile source relations in the selected personal or Project scope without dereferencing their Knowledge/access source refs.",
                MutationClass::ReadOnly,
                "agent-profile-list",
                common.clone(),
            ),
            list_action,
        )
        .expect("AgentProfile Action ids are valid");

    let mut read_inputs = common.clone();
    read_inputs.push(input("profile_ref", "string", true));
    registry
        .register(
            descriptor(
                AGENT_PROFILE_READ_ACTION,
                "Read Agent Profile",
                "Read one canonical Central AgentProfile source relation and provenance without treating it as live Agency or AIKit effective Profile state.",
                MutationClass::ReadOnly,
                "agent-profile-reading",
                read_inputs,
            ),
            read_action,
        )
        .expect("AgentProfile Action ids are valid");

    let mut save_inputs = common.clone();
    save_inputs.push(input("profile", "object", true));
    save_inputs.push(input("expected_revision", "string", false));
    registry
        .register(
            descriptor(
                AGENT_PROFILE_SAVE_ACTION,
                "Save Agent Profile",
                "Create or compare-and-swap one canonical Central AgentProfile source relation for an existing AgentRef.",
                MutationClass::LocallyMutating,
                "agent-profile-write-receipt",
                save_inputs,
            ),
            save_action,
        )
        .expect("AgentProfile Action ids are valid");

    let mut remove_inputs = common;
    remove_inputs.push(input("profile_ref", "string", true));
    remove_inputs.push(input("expected_revision", "string", true));
    registry
        .register(
            descriptor(
                AGENT_PROFILE_REMOVE_ACTION,
                "Remove Agent Profile",
                "Remove only a Central AgentProfile source relation under compare-and-swap revision discipline; never delete Agent identity/runtime/material state.",
                MutationClass::LocallyMutating,
                "agent-profile-removal",
                remove_inputs,
            ),
            remove_action,
        )
        .expect("AgentProfile Action ids are valid");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::projectcentral::ProjectCentralManifest;
    use crate::projectcentral_ops::initialize_projectcentral;
    use crate::root::{initialize_central, RootOptions};
    use central_connector_sdk::{ConnectorContext, ConnectorRegistry};
    use serde_json::json;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn fixture_root() -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "central-agent-profile-actions-{}-{nonce}",
            std::process::id()
        ));
        initialize_central(&root).unwrap();
        root
    }

    fn registry() -> ActionRegistry {
        let mut registry = crate::action::create_core_action_registry();
        register_agent_profile_actions(&mut registry);
        registry
    }

    fn personal_profile(revision: &str) -> Value {
        json!({
            "schema": AGENT_PROFILE_SCHEMA,
            "ref": "agent-profile:guardian",
            "revision": revision,
            "agent_ref": "agent:guardian",
            "scope": "personal",
            "world_ref": "world:personal",
            "role": "Guardian",
            "purpose": "Care for the personal O:I world.",
            "skill_refs": ["skill:orientation"],
            "skill_set_refs": ["skill-set:personal"],
            "method_refs": ["method:orient"],
            "ratified_world_refs": ["world:personal"],
            "computer_access_intent_refs": ["computer-access:guardian"]
        })
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
    fn personal_actions_round_trip_and_preserve_cas_and_agent_identity() {
        let root = fixture_root();
        let registry = registry();
        let mut options = None;
        let mut connectors = None;
        let mut connector_context = None;
        let context = context(
            &root,
            &mut options,
            &mut connectors,
            &mut connector_context,
        );

        let created = registry.execute(
            AGENT_PROFILE_SAVE_ACTION,
            &json!({"scope":"personal", "profile": personal_profile("p1")}),
            &context,
        );
        assert!(created.ok, "{created:?}");
        assert_eq!(created.data.as_ref().unwrap()["created"], true);

        let listed = registry.execute(
            AGENT_PROFILE_LIST_ACTION,
            &json!({"scope":"personal"}),
            &context,
        );
        assert!(listed.ok, "{listed:?}");
        assert_eq!(listed.data.as_ref().unwrap()["profiles"].as_array().unwrap().len(), 1);
        assert_eq!(listed.data.as_ref().unwrap()["source_payloads_disclosed"], false);

        let read = registry.execute(
            AGENT_PROFILE_READ_ACTION,
            &json!({"scope":"personal", "profile_ref":"agent-profile:guardian"}),
            &context,
        );
        assert!(read.ok, "{read:?}");
        assert_eq!(read.data.as_ref().unwrap()["profile"]["agent_ref"], "agent:guardian");

        let conflict = registry.execute(
            AGENT_PROFILE_SAVE_ACTION,
            &json!({
                "scope":"personal",
                "expected_revision":"wrong",
                "profile": personal_profile("p2")
            }),
            &context,
        );
        assert!(!conflict.ok);
        assert_eq!(conflict.status, ResultStatus::InvalidInput);

        let updated = registry.execute(
            AGENT_PROFILE_SAVE_ACTION,
            &json!({
                "scope":"personal",
                "expected_revision":"p1",
                "profile": personal_profile("p2")
            }),
            &context,
        );
        assert!(updated.ok, "{updated:?}");

        let removed = registry.execute(
            AGENT_PROFILE_REMOVE_ACTION,
            &json!({
                "scope":"personal",
                "profile_ref":"agent-profile:guardian",
                "expected_revision":"p2"
            }),
            &context,
        );
        assert!(removed.ok, "{removed:?}");
        assert_eq!(removed.data.as_ref().unwrap()["agent_identity_deleted"], false);
        assert_eq!(removed.data.as_ref().unwrap()["runtime_state_deleted"], false);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn project_actions_require_real_projectcentral_and_keep_scope_distinct() {
        let root = fixture_root();
        let project = root.join("Work/example");
        fs::create_dir_all(&project).unwrap();
        initialize_projectcentral(&root, &project, "example/project").unwrap();
        let manifest = ProjectCentralManifest::new("example/project");
        assert!(manifest.validate().valid);

        let registry = registry();
        let mut options = None;
        let mut connectors = None;
        let mut connector_context = None;
        let context = context(
            &root,
            &mut options,
            &mut connectors,
            &mut connector_context,
        );
        let project_profile = json!({
            "schema": AGENT_PROFILE_SCHEMA,
            "ref": "agent-profile:builder:example",
            "revision": "j1",
            "agent_ref": "agent:builder",
            "scope": "project",
            "world_ref": "world:project:example",
            "source_profile_ref": "agent-profile:builder",
            "skill_set_refs": ["skill-set:development"],
            "method_refs": ["method:develop"],
            "ratified_world_refs": ["world:project:example"],
            "provenance_refs": ["agent-profile:builder"]
        });
        let saved = registry.execute(
            AGENT_PROFILE_SAVE_ACTION,
            &json!({"scope":"project", "project":"example", "profile":project_profile}),
            &context,
        );
        assert!(saved.ok, "{saved:?}");
        assert!(project.join("ProjectCentral/agents/profiles").is_dir());

        let wrong_scope = registry.execute(
            AGENT_PROFILE_SAVE_ACTION,
            &json!({"scope":"personal", "profile":project_profile}),
            &context,
        );
        assert!(!wrong_scope.ok);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn ingestion_rejects_duplicate_praxis_refs_before_store_mutation() {
        let root = fixture_root();
        let registry = registry();
        let mut options = None;
        let mut connectors = None;
        let mut connector_context = None;
        let context = context(
            &root,
            &mut options,
            &mut connectors,
            &mut connector_context,
        );
        let mut profile = personal_profile("p1");
        profile["method_refs"] = json!(["method:orient", "method:orient"]);
        let result = registry.execute(
            AGENT_PROFILE_SAVE_ACTION,
            &json!({"scope":"personal", "profile":profile}),
            &context,
        );
        assert!(!result.ok);
        assert!(!root.join("Control/agents/profiles").exists());
        fs::remove_dir_all(root).unwrap();
    }
}
