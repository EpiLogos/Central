use crate::action::{
    ActionAvailability, ActionDescriptor, ActionExecutionContext, ActionInputDefinition,
    ActionOutputDefinition, ActionRegistry, MutationClass,
};
use crate::agent_profile::{
    AGENT_PROFILE_PROVENANCE_SCHEMA, AGENT_PROFILE_SCHEMA, AgentProfile, AgentProfileError,
    AgentProfileScope,
};
use crate::agent_profile_store::{AgentProfileStore, AgentProfileStoreError};
use crate::projectcentral::read_project_manifest;
use crate::result::{ActionResult, ResultStatus};
use crate::root::resolve_central_root;
use serde_json::{Value, json, to_value};
use std::collections::BTreeSet;
use std::path::{Component, Path};

pub const AGENT_PROFILE_LIST_ACTION: &str = "agent-profile.list";
pub const AGENT_PROFILE_READ_ACTION: &str = "agent-profile.read";
pub const AGENT_PROFILE_SAVE_ACTION: &str = "agent-profile.save";
pub const AGENT_PROFILE_REMOVE_ACTION: &str = "agent-profile.remove";
pub const AGENT_PROFILE_PROPOSE_ACTION: &str = "agent-profile.propose";

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
    value.choices = Some(vec!["root".into(), "project".into()]);
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
        // `root` is the register's name; `personal` is the legacy synonym the
        // profile store was first published under. Both name the same register,
        // and the agent-set and world registers already answer to `root` — a
        // consumer must not have to know which noun an Action happens to use.
        "root" | "personal" => Ok(AgentProfileStore::personal(root.path)),
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
            return Err(format!(
                "{field} must be non-empty without surrounding whitespace"
            ));
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
        (
            "Knowledge source refs",
            profile.knowledge_source_refs.as_slice(),
        ),
        (
            "Central Computer access-intent refs",
            profile.computer_access_intent_refs.as_slice(),
        ),
        (
            "placement-intent refs",
            profile.placement_intent_refs.as_slice(),
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
    // Intent provenance is only mintable by the intent authoring Action; the
    // typed enums already make recognised states unparseable, and the schema /
    // verbatim-intent contract is enforced at every ingestion boundary.
    if let Some(provenance) = &profile.intent_provenance {
        provenance
            .validate()
            .map_err(|error| format!("invalid intent provenance: {error}"))?;
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
            );
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

/// Map store failures for the intent authoring boundary. Every failed state is
/// explicit: invalid intent and duplicate identity are caller-correctable
/// InvalidInput, while absent or unwritable target ground is a VerificationFailure
/// carrying a machine-readable state, never a silent empty success.
fn propose_store_failure(
    action: &str,
    error: AgentProfileStoreError,
    profile_ref: &str,
) -> ActionResult {
    match error {
        AgentProfileStoreError::AlreadyExists { .. } => ActionResult::failure(
            Some(action),
            ResultStatus::InvalidInput,
            format!("duplicate profile identity: {error}"),
            Some(json!({
                "state": "duplicate-profile-identity",
                "profile_ref": profile_ref,
            })),
        ),
        AgentProfileStoreError::Io(error) => ActionResult::failure(
            Some(action),
            ResultStatus::VerificationFailure,
            format!("target ground absent or unwritable: {error}"),
            Some(json!({ "state": "target-ground-absent-or-unwritable" })),
        ),
        AgentProfileStoreError::UnsafeRoot(_) | AgentProfileStoreError::UnsafeSource(_) => {
            ActionResult::failure(
                Some(action),
                ResultStatus::VerificationFailure,
                format!("target ground absent or unwritable: {error}"),
                Some(json!({ "state": "target-ground-absent-or-unwritable" })),
            )
        }
        other => store_failure(action, other),
    }
}

fn optional_ref_list(input: &Value, field: &str) -> Result<Vec<String>, String> {
    let Some(raw) = input.get(field) else {
        return Ok(Vec::new());
    };
    let Some(values) = raw.as_array() else {
        return Err(format!("{field} must be an array of refs."));
    };
    let refs = values
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| format!("{field} must be an array of refs."))
        })
        .collect::<Result<Vec<_>, _>>()?;
    validate_ref_list(field, &refs)?;
    Ok(refs)
}

fn optional_world_ref_list(
    input: &Value,
    field: &str,
) -> Result<Vec<crate::world::WorldRef>, String> {
    let refs = optional_ref_list(input, field)?;
    refs.iter()
        .map(|value| {
            crate::world::WorldRef::new(value.clone())
                .map_err(|error| format!("{field} contains an invalid World ref: {error}"))
        })
        .collect()
}

/// The intent → AgentProfile authoring operation (Central#51, wave 3): one
/// expressed intent becomes durable authored Control ground. The authored record
/// is a generated proposal from birth: provenance carries the verbatim intent
/// expression, the authoring Action, and `generated-proposal` / `unrecognised`
/// standing. Recognition is the human owner's separate act; no input field of
/// this Action can claim it, and the typed record cannot even parse a claim of
/// human acceptance.
///
/// The authored ground is readable by any downstream consumer (the AIKit
/// composition preparation cell included) through the canonical read Action
/// `agent-profile.read` with the same scope/project/profile_ref; the success
/// payload discloses that exact read path.
fn propose_action(
    _registry: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let store = match resolve_store(AGENT_PROFILE_PROPOSE_ACTION, input, context) {
        Ok(store) => store,
        Err(result) => return result,
    };
    let profile_ref = match required_text(input, "profile_ref", AGENT_PROFILE_PROPOSE_ACTION) {
        Ok(value) => value,
        Err(result) => return result,
    };
    let agent_ref = match required_text(input, "agent_ref", AGENT_PROFILE_PROPOSE_ACTION) {
        Ok(value) => value,
        Err(result) => return result,
    };
    let revision = match required_text(input, "revision", AGENT_PROFILE_PROPOSE_ACTION) {
        Ok(value) => value,
        Err(result) => return result,
    };
    // The intent expression is taken verbatim: any rewrite would be the Action
    // authoring intent the human never expressed.
    let Some(raw_intent) = input.get("intent_expression").and_then(Value::as_str) else {
        return ActionResult::failure(
            Some(AGENT_PROFILE_PROPOSE_ACTION),
            ResultStatus::InvalidInput,
            "agent-profile.propose requires intent_expression as a string.",
            Some(json!({ "state": "invalid-intent" })),
        );
    };
    let world_ref = match required_text(input, "world_ref", AGENT_PROFILE_PROPOSE_ACTION) {
        Ok(value) => match crate::world::WorldRef::new(value) {
            Ok(world_ref) => world_ref,
            Err(error) => {
                return ActionResult::failure(
                    Some(AGENT_PROFILE_PROPOSE_ACTION),
                    ResultStatus::InvalidInput,
                    format!("world_ref is not a valid World ref: {error}"),
                    None,
                );
            }
        },
        Err(result) => return result,
    };
    let scope = match store.scope() {
        AgentProfileScope::Personal => AgentProfileScope::Personal,
        AgentProfileScope::Project => AgentProfileScope::Project,
    };
    let mut profile = match AgentProfile::propose_from_intent(
        profile_ref.clone(),
        revision,
        agent_ref,
        scope,
        world_ref,
        raw_intent,
        AGENT_PROFILE_PROPOSE_ACTION,
    ) {
        Ok(profile) => profile,
        Err(AgentProfileError::InvalidIntentExpression) => {
            return ActionResult::failure(
                Some(AGENT_PROFILE_PROPOSE_ACTION),
                ResultStatus::InvalidInput,
                "invalid intent: intent_expression must be non-empty, trimmed, and free of unsafe characters; the authored profile never rewrites intent.",
                Some(json!({ "state": "invalid-intent" })),
            );
        }
        Err(error) => {
            return ActionResult::failure(
                Some(AGENT_PROFILE_PROPOSE_ACTION),
                ResultStatus::InvalidInput,
                format!("invalid AgentProfile proposal: {error}"),
                None,
            );
        }
    };
    if let Some(value) = optional_text(input, "role") {
        profile.role = Some(value);
    }
    if let Some(value) = optional_text(input, "purpose") {
        profile.purpose = Some(value);
    }
    if let Some(value) = optional_text(input, "source_profile_ref") {
        profile.source_profile_ref = Some(value);
    }
    let assignments: Result<(), String> = (|| {
        profile.governance_refs = optional_ref_list(input, "governance_refs")?;
        profile.skill_refs = optional_ref_list(input, "skill_refs")?;
        profile.skill_set_refs = optional_ref_list(input, "skill_set_refs")?;
        profile.method_refs = optional_ref_list(input, "method_refs")?;
        profile.routine_refs = optional_ref_list(input, "routine_refs")?;
        profile.ratified_world_refs = optional_world_ref_list(input, "ratified_world_refs")?;
        profile.knowledge_source_refs = optional_ref_list(input, "knowledge_source_refs")?;
        profile.computer_access_intent_refs =
            optional_ref_list(input, "computer_access_intent_refs")?;
        profile.placement_intent_refs = optional_ref_list(input, "placement_intent_refs")?;
        profile.provenance_refs = optional_ref_list(input, "provenance_refs")?;
        Ok(())
    })();
    if let Err(error) = assignments {
        return ActionResult::failure(
            Some(AGENT_PROFILE_PROPOSE_ACTION),
            ResultStatus::InvalidInput,
            error,
            None,
        );
    }
    if let Err(error) = validate_ingested_profile(&profile) {
        return ActionResult::failure(
            Some(AGENT_PROFILE_PROPOSE_ACTION),
            ResultStatus::InvalidInput,
            error,
            None,
        );
    }
    // Create-only: duplicate profile identity is surfaced by the store and
    // mapped to an explicit state above; expected_revision is never taken here.
    store
        .save(&profile, None)
        .map(|receipt| {
            let provenance = profile
                .intent_provenance
                .as_ref()
                .expect("agent-profile.propose always stamps generated-proposal provenance");
            let mut read_path = json!({
                "action": AGENT_PROFILE_READ_ACTION,
                "input": {
                    "scope": match store.scope() {
                        AgentProfileScope::Personal => "personal",
                        AgentProfileScope::Project => "project",
                    },
                    "profile_ref": profile.profile_ref,
                },
            });
            if let Some(project) = input.get("project").and_then(Value::as_str) {
                read_path["input"]["project"] = Value::String(project.to_owned());
            }
            ActionResult::success(
                AGENT_PROFILE_PROPOSE_ACTION,
                json!({
                    "receipt": receipt,
                    "profile": profile,
                    "intent_expression": provenance.intent_expression,
                    "provenance_schema": AGENT_PROFILE_PROVENANCE_SCHEMA,
                    "authorship": "generated-proposal",
                    "recognition": "unrecognised",
                    "human_recognised": false,
                    "read_path": read_path,
                }),
            )
        })
        .unwrap_or_else(|error| {
            propose_store_failure(AGENT_PROFILE_PROPOSE_ACTION, error, &profile_ref)
        })
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

    let mut propose_inputs = vec![scope_input(), input("project", "string", false)];
    for (name, required) in [
        ("profile_ref", true),
        ("agent_ref", true),
        ("revision", true),
        ("world_ref", true),
        ("intent_expression", true),
        ("role", false),
        ("purpose", false),
        ("source_profile_ref", false),
        ("governance_refs", false),
        ("skill_refs", false),
        ("skill_set_refs", false),
        ("method_refs", false),
        ("routine_refs", false),
        ("ratified_world_refs", false),
        ("knowledge_source_refs", false),
        ("computer_access_intent_refs", false),
        ("placement_intent_refs", false),
        ("provenance_refs", false),
    ] {
        propose_inputs.push(input(
            name,
            if name.ends_with("_refs") {
                "array"
            } else {
                "string"
            },
            required,
        ));
    }
    registry
        .register(
            descriptor(
                AGENT_PROFILE_PROPOSE_ACTION,
                "Propose Agent Profile from Intent",
                "Author one canonical Central AgentProfile source relation as durable Control ground from an expressed intent. The record carries central.agent-profile/v1 plus a central.agent-profile-provenance/v1 block stamped generated-proposal/unrecognised with the verbatim intent expression; recognition is the human owner's separate act and no input can claim it. Invalid intent, absent/unwritable ground and duplicate profile identity are explicit states. The authored ground reads back through agent-profile.read for downstream composition consumers.",
                MutationClass::LocallyMutating,
                "agent-profile-proposal",
                propose_inputs,
            ),
            propose_action,
        )
        .expect("AgentProfile Action ids are valid");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::projectcentral::ProjectCentralManifest;
    use crate::projectcentral_ops::initialize_projectcentral;
    use crate::root::{RootOptions, initialize_central};
    use central_connector_sdk::{ConnectorContext, ConnectorRegistry};
    use serde_json::json;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn fixture_root() -> PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(0);
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let sequence = NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "central-agent-profile-actions-{}-{nonce}-{sequence}",
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
        let context = context(&root, &mut options, &mut connectors, &mut connector_context);

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
        assert_eq!(
            listed.data.as_ref().unwrap()["profiles"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            listed.data.as_ref().unwrap()["source_payloads_disclosed"],
            false
        );

        let read = registry.execute(
            AGENT_PROFILE_READ_ACTION,
            &json!({"scope":"personal", "profile_ref":"agent-profile:guardian"}),
            &context,
        );
        assert!(read.ok, "{read:?}");
        assert_eq!(
            read.data.as_ref().unwrap()["profile"]["agent_ref"],
            "agent:guardian"
        );

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
        assert_eq!(
            removed.data.as_ref().unwrap()["agent_identity_deleted"],
            false
        );
        assert_eq!(
            removed.data.as_ref().unwrap()["runtime_state_deleted"],
            false
        );
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
        let context = context(&root, &mut options, &mut connectors, &mut connector_context);
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
        let context = context(&root, &mut options, &mut connectors, &mut connector_context);
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

    const INTENT: &str =
        "I want a guardian agent that orients every session return and never writes human source.";

    fn propose_input(scope_project: Option<&str>) -> Value {
        let mut input = json!({
            "scope": if scope_project.is_some() { "project" } else { "personal" },
            "profile_ref": "agent-profile:guardian",
            "agent_ref": "agent:guardian",
            "revision": "p1",
            "world_ref": if scope_project.is_some() {
                "world:project:example"
            } else {
                "world:personal"
            },
            "intent_expression": INTENT,
            "role": "Guardian",
            "purpose": "Care for the personal O:I world.",
            "skill_refs": ["skill:orientation"],
            "ratified_world_refs": [if scope_project.is_some() {
                "world:project:example"
            } else {
                "world:personal"
            }],
        });
        if let Some(project) = scope_project {
            input["project"] = Value::String(project.to_owned());
        }
        input
    }

    #[test]
    fn propose_authors_generated_proposal_ground_with_verbatim_intent_and_disclosed_read_path() {
        let root = fixture_root();
        let registry = registry();
        let mut options = None;
        let mut connectors = None;
        let mut connector_context = None;
        let context = context(&root, &mut options, &mut connectors, &mut connector_context);

        let proposed =
            registry.execute(AGENT_PROFILE_PROPOSE_ACTION, &propose_input(None), &context);
        assert!(proposed.ok, "{proposed:?}");
        let data = proposed.data.as_ref().unwrap();
        assert_eq!(data["receipt"]["created"], true);
        assert_eq!(data["intent_expression"], INTENT);
        assert_eq!(data["authorship"], "generated-proposal");
        assert_eq!(data["recognition"], "unrecognised");
        assert_eq!(data["human_recognised"], false);
        assert_eq!(data["provenance_schema"], AGENT_PROFILE_PROVENANCE_SCHEMA);
        // The Action discloses the exact canonical read path for downstream
        // composition consumers (the AIKit preparation cell reads this ground
        // through agent-profile.read and nothing else).
        assert_eq!(data["read_path"]["action"], AGENT_PROFILE_READ_ACTION);
        assert_eq!(
            data["read_path"]["input"],
            json!({"scope": "personal", "profile_ref": "agent-profile:guardian"})
        );

        // The authored ground on disk carries the typed, schema-stamped record
        // with the provenance block, not a paraphrase of the intent.
        let dir = root.join("Control/agents/profiles");
        let mut files = fs::read_dir(&dir).unwrap();
        let document: Value =
            serde_json::from_slice(&fs::read(files.next().unwrap().unwrap().path()).unwrap())
                .unwrap();
        assert!(files.next().is_none());
        assert_eq!(document["schema"], AGENT_PROFILE_SCHEMA);
        assert_eq!(
            document["intent_provenance"]["schema"],
            AGENT_PROFILE_PROVENANCE_SCHEMA
        );
        assert_eq!(document["intent_provenance"]["intent_expression"], INTENT);
        assert_eq!(
            document["intent_provenance"]["authorship"],
            "generated-proposal"
        );
        assert_eq!(document["intent_provenance"]["recognition"], "unrecognised");
        assert_eq!(
            document["intent_provenance"]["origin_action"],
            AGENT_PROFILE_PROPOSE_ACTION
        );

        // The disclosed read path actually round-trips: the canonical read
        // Action returns the authored ground with its provenance intact.
        let read = registry.execute(
            AGENT_PROFILE_READ_ACTION,
            &json!({"scope":"personal", "profile_ref":"agent-profile:guardian"}),
            &context,
        );
        assert!(read.ok, "{read:?}");
        assert_eq!(
            read.data.as_ref().unwrap()["profile"]["agent_ref"],
            "agent:guardian"
        );
        assert_eq!(
            read.data.as_ref().unwrap()["profile"]["intent_provenance"]["intent_expression"],
            INTENT
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn propose_authors_project_ground_and_discloses_project_read_path() {
        let root = fixture_root();
        let project = root.join("Work/example");
        fs::create_dir_all(&project).unwrap();
        initialize_projectcentral(&root, &project, "example/project").unwrap();
        let registry = registry();
        let mut options = None;
        let mut connectors = None;
        let mut connector_context = None;
        let context = context(&root, &mut options, &mut connectors, &mut connector_context);

        let proposed = registry.execute(
            AGENT_PROFILE_PROPOSE_ACTION,
            &propose_input(Some("example")),
            &context,
        );
        assert!(proposed.ok, "{proposed:?}");
        let data = proposed.data.as_ref().unwrap();
        assert_eq!(
            data["read_path"]["input"],
            json!({"scope": "project", "project": "example", "profile_ref": "agent-profile:guardian"})
        );
        assert!(project.join("ProjectCentral/agents/profiles").is_dir());
        assert_eq!(data["profile"]["scope"], "project");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn propose_rejects_invalid_intent_without_writing_ground() {
        for intent in [
            Value::Null,
            Value::String(String::new()),
            json!("   "),
            json!(" padded "),
            json!("unsafe\0intent"),
        ] {
            let root = fixture_root();
            let registry = registry();
            let mut options = None;
            let mut connectors = None;
            let mut connector_context = None;
            let context = context(&root, &mut options, &mut connectors, &mut connector_context);
            let mut input = propose_input(None);
            input["intent_expression"] = intent.clone();
            let result = registry.execute(AGENT_PROFILE_PROPOSE_ACTION, &input, &context);
            assert!(!result.ok, "intent {intent:?} must not author ground");
            assert_eq!(result.status, ResultStatus::InvalidInput);
            assert_eq!(
                result.error.as_ref().unwrap().details.as_ref().unwrap()["state"],
                "invalid-intent"
            );
            assert!(
                !root.join("Control/agents/profiles").exists(),
                "invalid intent must never mutate ground"
            );
            fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn propose_surfaces_duplicate_profile_identity_explicitly() {
        let root = fixture_root();
        let registry = registry();
        let mut options = None;
        let mut connectors = None;
        let mut connector_context = None;
        let context = context(&root, &mut options, &mut connectors, &mut connector_context);
        let first = registry.execute(AGENT_PROFILE_PROPOSE_ACTION, &propose_input(None), &context);
        assert!(first.ok, "{first:?}");

        let duplicate =
            registry.execute(AGENT_PROFILE_PROPOSE_ACTION, &propose_input(None), &context);
        assert!(!duplicate.ok);
        assert_eq!(duplicate.status, ResultStatus::InvalidInput);
        let details = duplicate.error.as_ref().unwrap().details.as_ref().unwrap();
        assert_eq!(details["state"], "duplicate-profile-identity");
        assert_eq!(details["profile_ref"], "agent-profile:guardian");
        // The original authored ground is untouched by the duplicate attempt.
        let read = registry.execute(
            AGENT_PROFILE_READ_ACTION,
            &json!({"scope":"personal", "profile_ref":"agent-profile:guardian"}),
            &context,
        );
        assert!(read.ok);
        assert_eq!(read.data.as_ref().unwrap()["profile"]["revision"], "p1");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn propose_surfaces_absent_project_ground_explicitly() {
        let root = fixture_root();
        let registry = registry();
        let mut options = None;
        let mut connectors = None;
        let mut connector_context = None;
        let context = context(&root, &mut options, &mut connectors, &mut connector_context);
        let result = registry.execute(
            AGENT_PROFILE_PROPOSE_ACTION,
            &propose_input(Some("ghost")),
            &context,
        );
        assert!(!result.ok);
        assert_eq!(result.status, ResultStatus::InvalidInput);
        assert!(
            result
                .error
                .as_ref()
                .unwrap()
                .message
                .contains("Project directory does not exist in Central Work.")
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn propose_surfaces_unwritable_target_ground_explicitly() {
        let root = fixture_root();
        // A file where the profile source directory must be is unsafe ground:
        // the store refuses to create below it and the Action surfaces that as
        // an explicit verification failure, never a silent no-op.
        let blocked = root.join("Control/agents/profiles");
        fs::create_dir_all(blocked.parent().unwrap()).unwrap();
        fs::write(&blocked, "not a directory").unwrap();
        let registry = registry();
        let mut options = None;
        let mut connectors = None;
        let mut connector_context = None;
        let context = context(&root, &mut options, &mut connectors, &mut connector_context);
        let result = registry.execute(AGENT_PROFILE_PROPOSE_ACTION, &propose_input(None), &context);
        assert!(!result.ok);
        assert_eq!(result.status, ResultStatus::VerificationFailure);
        assert_eq!(
            result.error.as_ref().unwrap().details.as_ref().unwrap()["state"],
            "target-ground-absent-or-unwritable"
        );
        fs::remove_dir_all(root).unwrap();
    }
}
