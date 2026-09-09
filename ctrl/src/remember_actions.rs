//! Remember-this Actions (wayfinder U3.3, wave 4 cell W4-A): remember a
//! selection (text + source ref + destination) into durable owner ground as a
//! generated proposal. The note is stamped `generated-proposal` /
//! `unrecognised`; recognition is the human owner's separate act and no input
//! field, parameter or store path of these Actions can claim it.
use crate::action::{
    ActionAvailability, ActionDescriptor, ActionExecutionContext, ActionInputDefinition,
    ActionOutputDefinition, ActionRegistry, MutationClass,
};
use crate::files::CentralPathRef;
use crate::projectcentral::read_project_manifest;
use crate::remember::{
    REMEMBERED_DESTINATION, REMEMBERED_NOTE_PROVENANCE_SCHEMA, RememberError, RememberedNote,
};
use crate::remember_store::{RememberStore, RememberStoreError, RememberedNoteReceipt};
use crate::result::{ActionResult, ResultStatus};
use crate::root::resolve_central_root;
use serde_json::{Value, json};
use std::path::{Component, Path};
use std::time::{SystemTime, UNIX_EPOCH};

pub const CENTRAL_REMEMBER_ACTION: &str = "central.remember";
pub const PROJECTCENTRAL_REMEMBER_ACTION: &str = "projectcentral.remember";

fn unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
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

fn destination_input() -> ActionInputDefinition {
    let mut value = input("destination", "string", true);
    value.choices = Some(vec![REMEMBERED_DESTINATION.into()]);
    value
}

fn failure(
    action: &str,
    status: ResultStatus,
    message: impl Into<String>,
    state: &str,
) -> ActionResult {
    ActionResult::failure(
        Some(action),
        status,
        message.into(),
        Some(json!({ "state": state })),
    )
}

fn parse_destination(action: &str, input: &Value) -> Result<(), ActionResult> {
    match input.get("destination").and_then(Value::as_str) {
        Some(REMEMBERED_DESTINATION) => Ok(()),
        Some(other) => Err(failure(
            action,
            ResultStatus::InvalidInput,
            format!(
                "{action} requires destination to be one of: {REMEMBERED_DESTINATION}; received {other:?}."
            ),
            "invalid-destination",
        )),
        None => Err(failure(
            action,
            ResultStatus::InvalidInput,
            format!("{action} requires destination."),
            "invalid-destination",
        )),
    }
}

/// The selection is taken verbatim: any rewrite would be a selection the
/// human never made. Empty and unsafe selections are explicit refusal states,
/// never silently normalised.
fn parse_selection(action: &str, input: &Value) -> Result<String, ActionResult> {
    let Some(raw) = input.get("selection") else {
        return Err(failure(
            action,
            ResultStatus::InvalidInput,
            format!("{action} requires selection as a string."),
            "empty-selection",
        ));
    };
    if raw.is_null() {
        return Err(failure(
            action,
            ResultStatus::InvalidInput,
            format!(
                "{action} refuses an empty selection; a remembered note never rewrites what the person selected."
            ),
            "empty-selection",
        ));
    }
    let Some(selection) = raw.as_str() else {
        return Err(failure(
            action,
            ResultStatus::InvalidInput,
            format!("{action} requires selection as a string."),
            "invalid-selection",
        ));
    };
    if selection.trim().is_empty() {
        return Err(failure(
            action,
            ResultStatus::InvalidInput,
            format!(
                "{action} refuses an empty selection; a remembered note never rewrites what the person selected."
            ),
            "empty-selection",
        ));
    }
    if selection != selection.trim() || selection.contains('\0') {
        return Err(failure(
            action,
            ResultStatus::InvalidInput,
            format!(
                "{action} requires selection without surrounding whitespace or unsafe characters; the selection is retained verbatim or refused."
            ),
            "invalid-selection",
        ));
    }
    Ok(selection.to_owned())
}

fn parse_source_ref(action: &str, input: &Value) -> Result<String, ActionResult> {
    match input.get("source_ref").and_then(Value::as_str) {
        Some(source_ref) if !source_ref.trim().is_empty() && source_ref == source_ref.trim() => {
            Ok(source_ref.to_owned())
        }
        Some(_) => Err(failure(
            action,
            ResultStatus::InvalidInput,
            format!(
                "{action} requires source_ref as a non-empty, trimmed string naming where the selection was read from."
            ),
            "invalid-source-ref",
        )),
        None => Err(failure(
            action,
            ResultStatus::InvalidInput,
            format!("{action} requires source_ref."),
            "invalid-source-ref",
        )),
    }
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

fn mint_failure(action: &str, error: RememberError) -> ActionResult {
    match error {
        RememberError::EmptySelection => failure(
            action,
            ResultStatus::InvalidInput,
            "invalid selection: selection must be non-empty; the remembered note never rewrites what the person selected.",
            "empty-selection",
        ),
        RememberError::InvalidSelection => failure(
            action,
            ResultStatus::InvalidInput,
            "invalid selection: selection must not carry surrounding whitespace or unsafe characters.",
            "invalid-selection",
        ),
        RememberError::InvalidSourceRef => failure(
            action,
            ResultStatus::InvalidInput,
            "invalid source ref: source_ref must be non-empty and trimmed.",
            "invalid-source-ref",
        ),
        other => ActionResult::failure(
            Some(action),
            ResultStatus::InvalidInput,
            format!("invalid remembered note: {other}"),
            None,
        ),
    }
}

/// Map store failures for the Remember-this boundary. Every failed state is
/// explicit and machine-readable: duplicate identity is caller-correctable
/// InvalidInput, while absent or unwritable target ground is a
/// VerificationFailure, never a silent empty success.
fn store_failure(action: &str, error: RememberStoreError, note_ref: &str) -> ActionResult {
    match error {
        RememberStoreError::AlreadyExists {
            note_ref,
            recorded_at_unix_seconds,
        } => ActionResult::failure(
            Some(action),
            ResultStatus::InvalidInput,
            format!("duplicate note identity: {note_ref} is already remembered (recorded at unix second {recorded_at_unix_seconds}); the original ground is untouched."),
            Some(json!({
                "state": "duplicate-note-identity",
                "note_ref": note_ref,
            })),
        ),
        RememberStoreError::Io(_)
        | RememberStoreError::UnsafeRoot(_)
        | RememberStoreError::UnsafeSource(_) => ActionResult::failure(
            Some(action),
            ResultStatus::VerificationFailure,
            format!("target ground absent or unwritable: {error}"),
            Some(json!({ "state": "target-ground-absent-or-unwritable" })),
        ),
        RememberStoreError::InvalidNote(_)
        | RememberStoreError::InvalidNoteRef(_)
        | RememberStoreError::NotFound(_)
        | RememberStoreError::RefMismatch { .. } => ActionResult::failure(
            Some(action),
            ResultStatus::VerificationFailure,
            format!("remembered note ground failed verification: {error}"),
            Some(json!({ "state": "target-ground-absent-or-unwritable" })),
        ),
    }
    .with_note_ref(note_ref)
}

trait WithNoteRef {
    fn with_note_ref(self, note_ref: &str) -> Self;
}

impl WithNoteRef for ActionResult {
    fn with_note_ref(mut self, note_ref: &str) -> Self {
        if let Some(details) = self
            .error
            .as_mut()
            .and_then(|error| error.details.as_mut())
            .and_then(Value::as_object_mut)
        {
            details.insert("note_ref".into(), Value::String(note_ref.to_owned()));
        }
        self
    }
}

/// Disclose the exact canonical read path for the landed note: the typed
/// ground is readable back through `central.files.read` at the disclosed
/// location, at both registers.
fn read_path(central_root: &Path, root_relative_path: &str) -> Value {
    let canonical = central_root
        .canonicalize()
        .unwrap_or_else(|_| central_root.to_path_buf());
    let location = CentralPathRef::new(&canonical, root_relative_path.to_owned())
        .map(to_value_keep)
        .unwrap_or(Value::Null);
    json!({
        "action": "central.files.read",
        "input": { "location": location },
    })
}

fn to_value_keep(value: CentralPathRef) -> Value {
    serde_json::to_value(value).unwrap_or(Value::Null)
}

fn remember_with_store(
    action: &str,
    input: &Value,
    store: RememberStore,
    central_root: &Path,
    root_relative_path: impl FnOnce(&RememberedNoteReceipt) -> String,
) -> ActionResult {
    if let Err(result) = parse_destination(action, input) {
        return result;
    }
    let selection = match parse_selection(action, input) {
        Ok(selection) => selection,
        Err(result) => return result,
    };
    let source_ref = match parse_source_ref(action, input) {
        Ok(source_ref) => source_ref,
        Err(result) => return result,
    };
    let note =
        match RememberedNote::generated_proposal(selection, source_ref, action, unix_seconds()) {
            Ok(note) => note,
            Err(error) => return mint_failure(action, error),
        };
    let note_ref = note.note_ref.clone();
    store
        .save(&note)
        .map(|receipt| {
            ActionResult::success(
                action,
                json!({
                    "receipt": receipt,
                    "note": note,
                    "provenance_schema": REMEMBERED_NOTE_PROVENANCE_SCHEMA,
                    "authorship": "generated-proposal",
                    "recognition": "unrecognised",
                    "human_recognised": false,
                    "recognition_owner": "human",
                    "read_path": read_path(central_root, &root_relative_path(&receipt)),
                }),
            )
        })
        .unwrap_or_else(|error| store_failure(action, error, &note_ref))
}

/// Root-register Remember-this: the selection lands as durable ground under
/// `Control/agents/remembered/` in the resolved Central root.
fn central_remember_action(
    _: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let root = match resolve_central_root(context.root_options) {
        Ok(root) => root,
        Err(message) => {
            return ActionResult::failure(
                Some(CENTRAL_REMEMBER_ACTION),
                ResultStatus::VerificationFailure,
                format!("target ground absent or unwritable: {message}"),
                Some(json!({ "state": "target-ground-absent-or-unwritable" })),
            );
        }
    };
    let store = RememberStore::root(root.path.clone());
    remember_with_store(
        CENTRAL_REMEMBER_ACTION,
        input,
        store,
        &root.path,
        |receipt| receipt.source_path.clone(),
    )
}

/// Project-register Remember-this: the selection lands as durable ground under
/// `ProjectCentral/agents/remembered/` in the chosen Project.
fn projectcentral_remember_action(
    _: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let root = match resolve_central_root(context.root_options) {
        Ok(root) => root,
        Err(message) => {
            return ActionResult::failure(
                Some(PROJECTCENTRAL_REMEMBER_ACTION),
                ResultStatus::VerificationFailure,
                format!("target ground absent or unwritable: {message}"),
                Some(json!({ "state": "target-ground-absent-or-unwritable" })),
            );
        }
    };
    let Some(project) = input
        .get("project")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return ActionResult::failure(
            Some(PROJECTCENTRAL_REMEMBER_ACTION),
            ResultStatus::InvalidInput,
            "projectcentral.remember requires project.",
            Some(json!({ "state": "invalid-project-ground" })),
        );
    };
    if !valid_project_member(project) {
        return ActionResult::failure(
            Some(PROJECTCENTRAL_REMEMBER_ACTION),
            ResultStatus::InvalidInput,
            "project must be a Central Work-relative path without parent/root components.",
            Some(json!({ "state": "invalid-project-ground" })),
        );
    }
    let project_root = root.path.join("Work").join(project);
    if !project_root.is_dir() {
        return ActionResult::failure(
            Some(PROJECTCENTRAL_REMEMBER_ACTION),
            ResultStatus::InvalidInput,
            "Project directory does not exist in Central Work.",
            Some(json!({ "state": "invalid-project-ground" })),
        );
    }
    let manifest = match read_project_manifest(&project_root) {
        Ok(manifest) => manifest,
        Err(error) => {
            return ActionResult::failure(
                Some(PROJECTCENTRAL_REMEMBER_ACTION),
                ResultStatus::InvalidInput,
                format!("Project does not expose a valid ProjectCentral source: {error}"),
                Some(json!({ "state": "invalid-project-ground" })),
            );
        }
    };
    if !manifest.validate().valid {
        return ActionResult::failure(
            Some(PROJECTCENTRAL_REMEMBER_ACTION),
            ResultStatus::VerificationFailure,
            "ProjectCentral manifest is invalid.",
            Some(json!({ "state": "invalid-project-ground" })),
        );
    }
    let store = RememberStore::project(project_root);
    let project = project.to_owned();
    remember_with_store(
        PROJECTCENTRAL_REMEMBER_ACTION,
        input,
        store,
        &root.path,
        move |receipt| format!("Work/{project}/{}", receipt.source_path),
    )
}

pub fn register_remember_actions(registry: &mut ActionRegistry) {
    registry
        .register(
            descriptor(
                CENTRAL_REMEMBER_ACTION,
                "Remember selection at Central root",
                "Remember one selection (verbatim text + source ref) into durable root-register ground under Control/agents/remembered as a generated proposal stamped generated-proposal/unrecognised. Recognition is the human owner's separate act; no input can claim it. Invalid destination, empty selection, invalid source ref and absent/unwritable ground are explicit machine-readable states, never silent.",
                MutationClass::LocallyMutating,
                "remembered-note-proposal",
                vec![
                    input("selection", "string", true),
                    input("source_ref", "string", true),
                    destination_input(),
                ],
            ),
            central_remember_action,
        )
        .expect("Remember Action ids are valid");
    registry
        .register(
            descriptor(
                PROJECTCENTRAL_REMEMBER_ACTION,
                "Remember selection into Project",
                "Remember one selection (verbatim text + source ref) into durable Project ground under ProjectCentral/agents/remembered as a generated proposal stamped generated-proposal/unrecognised. Recognition is the human owner's separate act; no input can claim it. Invalid destination, empty selection, invalid source ref, absent Project ground and absent/unwritable target ground are explicit machine-readable states, never silent.",
                MutationClass::LocallyMutating,
                "remembered-note-proposal",
                vec![
                    input("project", "string", true),
                    input("selection", "string", true),
                    input("source_ref", "string", true),
                    destination_input(),
                ],
            ),
            projectcentral_remember_action,
        )
        .expect("Remember Action ids are valid");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::projectcentral_ops::initialize_projectcentral;
    use crate::root::{RootOptions, initialize_central};
    use crate::{ConnectorContext, ConnectorRegistry};
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    const SELECTION: &str =
        "The field you stand in is the floor; strapping in fully is the ceiling.";
    const SOURCE: &str = "Control/agents/governance/field-and-now/session-work-placement.md";

    fn fixture_root() -> PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(0);
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let sequence = NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "central-remember-actions-{}-{nonce}-{sequence}",
            std::process::id()
        ));
        initialize_central(&root).unwrap();
        root
    }

    fn registry() -> ActionRegistry {
        let mut registry = crate::action::create_core_action_registry();
        register_remember_actions(&mut registry);
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

    fn central_input() -> Value {
        json!({
            "selection": SELECTION,
            "source_ref": SOURCE,
            "destination": "remembered",
        })
    }

    #[test]
    fn central_remember_lands_durable_ground_with_verbatim_generated_proposal_provenance() {
        let root = fixture_root();
        let registry = registry();
        let mut options = None;
        let mut connectors = None;
        let mut connector_context = None;
        let context = context(&root, &mut options, &mut connectors, &mut connector_context);

        let remembered = registry.execute(CENTRAL_REMEMBER_ACTION, &central_input(), &context);
        assert!(remembered.ok, "{remembered:?}");
        let data = remembered.data.as_ref().unwrap();
        assert_eq!(data["receipt"]["created"], true);
        assert_eq!(data["authorship"], "generated-proposal");
        assert_eq!(data["recognition"], "unrecognised");
        assert_eq!(data["human_recognised"], false);
        assert_eq!(data["recognition_owner"], "human");
        assert_eq!(data["provenance_schema"], REMEMBERED_NOTE_PROVENANCE_SCHEMA);
        assert_eq!(data["note"]["destination"], "remembered");
        assert_eq!(data["note"]["provenance"]["selection"], SELECTION);
        assert_eq!(data["note"]["provenance"]["source_ref"], SOURCE);
        assert_eq!(
            data["note"]["provenance"]["origin_action"],
            CENTRAL_REMEMBER_ACTION
        );
        assert!(
            data["note"]["provenance"]["recorded_at_unix_seconds"]
                .as_u64()
                .unwrap()
                > 0
        );

        // The authored ground on disk is the typed, schema-stamped record with
        // the provenance block — durable owner ground, not a side store.
        let source_path = data["receipt"]["source_path"].as_str().unwrap().to_owned();
        assert!(source_path.starts_with("Control/agents/remembered/"));
        let document: Value =
            serde_json::from_slice(&fs::read(root.join(&source_path)).unwrap()).unwrap();
        assert_eq!(document["schema"], "central.remembered-note/v1");
        assert_eq!(
            document["provenance"]["schema"],
            REMEMBERED_NOTE_PROVENANCE_SCHEMA
        );
        assert_eq!(document["provenance"]["selection"], SELECTION);
        assert_eq!(document["provenance"]["authorship"], "generated-proposal");
        assert_eq!(document["provenance"]["recognition"], "unrecognised");

        // The disclosed read path actually round-trips through the canonical
        // file read Action.
        assert_eq!(data["read_path"]["action"], "central.files.read");
        let read = registry.execute(
            "central.files.read",
            &json!({ "location": data["read_path"]["input"]["location"] }),
            &context,
        );
        assert!(read.ok, "{read:?}");
        assert!(
            read.data.as_ref().unwrap()["content"]
                .as_str()
                .unwrap()
                .contains(SELECTION)
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn projectcentral_remember_lands_in_project_ground_and_discloses_project_read_path() {
        let root = fixture_root();
        let project = root.join("Work/example");
        fs::create_dir_all(&project).unwrap();
        initialize_projectcentral(&root, &project, "example/project").unwrap();
        let registry = registry();
        let mut options = None;
        let mut connectors = None;
        let mut connector_context = None;
        let context = context(&root, &mut options, &mut connectors, &mut connector_context);

        let input = json!({
            "project": "example",
            "selection": SELECTION,
            "source_ref": "ProjectCentral/user/notes.md#L12",
            "destination": "remembered",
        });
        let remembered = registry.execute(PROJECTCENTRAL_REMEMBER_ACTION, &input, &context);
        assert!(remembered.ok, "{remembered:?}");
        let data = remembered.data.as_ref().unwrap();
        assert_eq!(
            data["note"]["provenance"]["origin_action"],
            PROJECTCENTRAL_REMEMBER_ACTION
        );
        let source_path = data["receipt"]["source_path"].as_str().unwrap().to_owned();
        assert!(source_path.starts_with("ProjectCentral/agents/remembered/"));
        assert!(project.join(&source_path).is_file());

        // The disclosed read path is root-relative and round-trips through
        // central.files.read at the resolved Central root.
        let read_location = data["read_path"]["input"]["location"].clone();
        assert_eq!(read_location["path"], format!("Work/example/{source_path}"));
        let read = registry.execute(
            "central.files.read",
            &json!({ "location": read_location }),
            &context,
        );
        assert!(read.ok, "{read:?}");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn remember_rejects_empty_and_malformed_selection_without_writing_ground() {
        for (selection, state) in [
            (Value::Null, "empty-selection"),
            (json!(""), "empty-selection"),
            (json!("   "), "empty-selection"),
            (json!(" padded "), "invalid-selection"),
            (json!("unsafe\0selection"), "invalid-selection"),
        ] {
            let root = fixture_root();
            let registry = registry();
            let mut options = None;
            let mut connectors = None;
            let mut connector_context = None;
            let context = context(&root, &mut options, &mut connectors, &mut connector_context);
            let mut input = central_input();
            input["selection"] = selection.clone();
            let result = registry.execute(CENTRAL_REMEMBER_ACTION, &input, &context);
            assert!(!result.ok, "selection {selection:?} must not author ground");
            assert_eq!(result.status, ResultStatus::InvalidInput);
            assert_eq!(
                result.error.as_ref().unwrap().details.as_ref().unwrap()["state"],
                state
            );
            assert!(
                !root.join("Control/agents/remembered").exists(),
                "invalid selection must never mutate ground"
            );
            fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn remember_rejects_invalid_destination_explicitly() {
        for destination in [Value::Null, json!(""), json!("now"), json!("wiki")] {
            let root = fixture_root();
            let registry = registry();
            let mut options = None;
            let mut connectors = None;
            let mut connector_context = None;
            let context = context(&root, &mut options, &mut connectors, &mut connector_context);
            let mut input = central_input();
            input["destination"] = destination.clone();
            let result = registry.execute(CENTRAL_REMEMBER_ACTION, &input, &context);
            assert!(!result.ok, "destination {destination:?} must be refused");
            assert_eq!(result.status, ResultStatus::InvalidInput);
            assert_eq!(
                result.error.as_ref().unwrap().details.as_ref().unwrap()["state"],
                "invalid-destination"
            );
            assert!(!root.join("Control/agents/remembered").exists());
            fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn remember_rejects_missing_source_ref_explicitly() {
        for source_ref in [Value::Null, json!(""), json!("  ")] {
            let root = fixture_root();
            let registry = registry();
            let mut options = None;
            let mut connectors = None;
            let mut connector_context = None;
            let context = context(&root, &mut options, &mut connectors, &mut connector_context);
            let mut input = central_input();
            input["source_ref"] = source_ref.clone();
            let result = registry.execute(CENTRAL_REMEMBER_ACTION, &input, &context);
            assert!(!result.ok, "source_ref {source_ref:?} must be refused");
            assert_eq!(result.status, ResultStatus::InvalidInput);
            assert_eq!(
                result.error.as_ref().unwrap().details.as_ref().unwrap()["state"],
                "invalid-source-ref"
            );
            fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn remember_surfaces_absent_root_ground_explicitly() {
        let base = crate::tempdir().unwrap();
        let missing = base.path().join("missing-central");
        let registry = registry();
        let mut options = None;
        let mut connectors = None;
        let mut connector_context = None;
        let context = context(
            &missing,
            &mut options,
            &mut connectors,
            &mut connector_context,
        );
        let result = registry.execute(CENTRAL_REMEMBER_ACTION, &central_input(), &context);
        assert!(!result.ok);
        assert_eq!(result.status, ResultStatus::VerificationFailure);
        assert_eq!(
            result.error.as_ref().unwrap().details.as_ref().unwrap()["state"],
            "target-ground-absent-or-unwritable"
        );
    }

    #[test]
    fn remember_surfaces_unwritable_target_ground_explicitly() {
        let root = fixture_root();
        // A file where the remembered source directory must be is unsafe
        // ground: the store refuses to create below it and the Action surfaces
        // that as an explicit verification failure, never a silent no-op.
        let blocked = root.join("Control/agents/remembered");
        fs::create_dir_all(blocked.parent().unwrap()).unwrap();
        fs::write(&blocked, "not a directory").unwrap();
        let registry = registry();
        let mut options = None;
        let mut connectors = None;
        let mut connector_context = None;
        let context = context(&root, &mut options, &mut connectors, &mut connector_context);
        let result = registry.execute(CENTRAL_REMEMBER_ACTION, &central_input(), &context);
        assert!(!result.ok);
        assert_eq!(result.status, ResultStatus::VerificationFailure);
        let details = result.error.as_ref().unwrap().details.as_ref().unwrap();
        assert_eq!(details["state"], "target-ground-absent-or-unwritable");
        assert!(
            details["note_ref"]
                .as_str()
                .unwrap()
                .starts_with("remembered-note:")
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn remember_surfaces_absent_project_ground_explicitly() {
        let root = fixture_root();
        let registry = registry();
        let mut options = None;
        let mut connectors = None;
        let mut connector_context = None;
        let context = context(&root, &mut options, &mut connectors, &mut connector_context);
        let input = json!({
            "project": "ghost",
            "selection": SELECTION,
            "source_ref": SOURCE,
            "destination": "remembered",
        });
        let result = registry.execute(PROJECTCENTRAL_REMEMBER_ACTION, &input, &context);
        assert!(!result.ok);
        assert_eq!(result.status, ResultStatus::InvalidInput);
        assert_eq!(
            result.error.as_ref().unwrap().details.as_ref().unwrap()["state"],
            "invalid-project-ground"
        );
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
    fn remember_surfaces_duplicate_note_identity_and_leaves_original_ground_untouched() {
        let root = fixture_root();
        let registry = registry();
        let mut options = None;
        let mut connectors = None;
        let mut connector_context = None;
        let context = context(&root, &mut options, &mut connectors, &mut connector_context);

        let first = registry.execute(CENTRAL_REMEMBER_ACTION, &central_input(), &context);
        assert!(first.ok, "{first:?}");
        let note_ref = first.data.as_ref().unwrap()["note"]["ref"].clone();

        let duplicate = registry.execute(CENTRAL_REMEMBER_ACTION, &central_input(), &context);
        assert!(!duplicate.ok);
        assert_eq!(duplicate.status, ResultStatus::InvalidInput);
        let details = duplicate.error.as_ref().unwrap().details.as_ref().unwrap();
        assert_eq!(details["state"], "duplicate-note-identity");
        assert_eq!(details["note_ref"], note_ref);

        // The original authored ground still carries the first note, verbatim.
        let store = RememberStore::root(&root);
        let reading = store.read(note_ref.as_str().unwrap()).unwrap();
        assert_eq!(reading.note.provenance.selection, SELECTION);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn recognition_is_human_owned_and_unforgeable_at_the_store_boundary() {
        let root = fixture_root();
        let registry = registry();
        let mut options = None;
        let mut connectors = None;
        let mut connector_context = None;
        let context = context(&root, &mut options, &mut connectors, &mut connector_context);
        let remembered = registry.execute(CENTRAL_REMEMBER_ACTION, &central_input(), &context);
        assert!(remembered.ok, "{remembered:?}");
        let note_ref = remembered.data.as_ref().unwrap()["note"]["ref"]
            .as_str()
            .unwrap()
            .to_owned();

        // No recognition Action exists and none was added: a machine attempt to
        // promote the note by rewriting its ground is refused by the typed
        // record. This is the whole promotion boundary recognition machinery
        // currently provides.
        let store = RememberStore::root(&root);
        let path = store.source_path(&note_ref).unwrap();
        let mut tampered: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        tampered["provenance"]["recognition"] = json!("recognised");
        fs::write(&path, serde_json::to_vec_pretty(&tampered).unwrap()).unwrap();
        assert!(matches!(
            store.read(&note_ref),
            Err(RememberStoreError::InvalidNote(_))
        ));

        // Removing the provenance block outright is equally unparseable ground.
        let mut stripped: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        stripped["provenance"] = json!(null);
        fs::write(&path, serde_json::to_vec_pretty(&stripped).unwrap()).unwrap();
        assert!(matches!(
            store.read(&note_ref),
            Err(RememberStoreError::InvalidNote(_))
        ));
        fs::remove_dir_all(root).unwrap();
    }
}
