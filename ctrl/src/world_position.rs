//! World Position definitions — `central.world-position/v1`.
//!
//! A Position is a stable address inside a World: it survives changes of
//! Agent, Agency, AgentSession, SessionSpace, model, harness and Workcell. It
//! never mints an Agent identity; `eligible_agent_refs` point at existing
//! canonical Agents and `profile_ref` at an existing AgentProfile in the
//! World's ancestry. Occupancy and tenure belong to Actuation, custody of work
//! to Factory; Central owns only the definition.
//!
//! Durable ground relation, one file per Position:
//! `{Control|ProjectCentral}/relations/positions/<slug>.json`. Validation is
//! strict — unknown keys are refused, because a silently dropped key alters
//! the address. A listing reports invalid files in `invalid[]` instead of
//! failing; reading an invalid Position refuses.

use crate::action::{
    ActionAvailability, ActionDescriptor, ActionExecutionContext, ActionInputDefinition,
    ActionOutputDefinition, ActionRegistry, MutationClass,
};
use crate::agent_profile_store::{AgentProfileStore, AgentProfileStoreError};
use crate::continuous_work::source::Scope;
use crate::result::{ActionResult, ResultStatus};
use crate::root::resolve_central_root;
use crate::source_horizon::CONTROL_WORLD_REF;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub const POSITION_SCHEMA: &str = "central.world-position/v1";
pub const POSITION_LISTING_SCHEMA: &str = "central.position-listing/v1";
pub const POSITION_READING_SCHEMA: &str = "central.position-reading/v1";
pub const POSITION_REF_PREFIX: &str = "central:position:";
pub const POSITION_NOT_FOUND_CODE: &str = "central.position_not_found";
pub const POSITION_INVALID_CODE: &str = "central.position_invalid";
pub const ROOT_POSITION_DIR: &str = "Control/relations/positions";
pub const PROJECT_POSITION_DIR: &str = "ProjectCentral/relations/positions";
const MAX_POSITION_BYTES: u64 = 256 * 1024;

/// The authored record. Deserialisation is the unknown-key gate; the values
/// are checked by [`validate`]. Readers return the authored JSON itself, never
/// a re-serialisation, so absent optional fields stay absent.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct WorldPosition {
    schema: String,
    #[serde(rename = "ref")]
    position_ref: String,
    revision: String,
    slug: String,
    label: String,
    enclosing_world_ref: String,
    #[serde(default)]
    enclosing_co_internality_ref: Option<String>,
    #[serde(default)]
    role_ref: Option<String>,
    #[serde(default)]
    purpose: Option<String>,
    #[serde(default)]
    purpose_ref: Option<String>,
    #[serde(default)]
    stewards_ref: Option<String>,
    #[serde(default)]
    eligible_agent_refs: Option<Vec<String>>,
    #[serde(default)]
    profile_ref: Option<String>,
    #[serde(default)]
    continuity_ref: Option<String>,
    #[serde(default)]
    handle: Option<String>,
}

/// `[a-z0-9][a-z0-9-]{0,63}`
pub fn valid_slug(slug: &str) -> bool {
    let bytes = slug.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= 64
        && (bytes[0].is_ascii_lowercase() || bytes[0].is_ascii_digit())
        && bytes
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-')
}

/// A World ref a Position may be enclosed by: `control:root` or
/// `project:<project_id>`.
fn valid_world_ref(world_ref: &str) -> bool {
    world_ref == CONTROL_WORLD_REF
        || world_ref.strip_prefix("project:").is_some_and(|id| {
            !id.is_empty() && id == id.trim() && !id.chars().any(char::is_control)
        })
}

/// Split `central:position:<world_ref>:<slug>` into its World and slug.
pub fn parse_position_ref(position_ref: &str) -> Option<(&str, &str)> {
    let (world, slug) = position_ref
        .strip_prefix(POSITION_REF_PREFIX)?
        .rsplit_once(':')?;
    (valid_world_ref(world) && valid_slug(slug)).then_some((world, slug))
}

fn check_ref_text(field: &str, value: &Option<String>) -> Result<(), String> {
    if let Some(value) = value {
        if value.trim().is_empty()
            || value != value.trim()
            || value.len() > 4096
            || value.chars().any(char::is_control)
        {
            return Err(format!(
                "{field} must be a non-empty trimmed single-line string or null"
            ));
        }
    }
    Ok(())
}

/// Every rule that a single file can be checked against on its own. Handle
/// uniqueness and profile resolution need the listing and the stores.
fn validate(position: &WorldPosition, file_name: &str, world_ref: &str) -> Result<(), String> {
    if position.schema != POSITION_SCHEMA {
        return Err(format!("schema must be {POSITION_SCHEMA}"));
    }
    if !valid_slug(&position.slug) {
        return Err(format!(
            "slug {:?} must match [a-z0-9][a-z0-9-]{{0,63}}",
            position.slug
        ));
    }
    if file_name != format!("{}.json", position.slug) {
        return Err(format!(
            "file name {file_name} must be {}.json (the slug)",
            position.slug
        ));
    }
    if position.enclosing_world_ref != world_ref {
        return Err(format!(
            "enclosing_world_ref {} must be {world_ref}, the World that holds this file",
            position.enclosing_world_ref
        ));
    }
    let expected = format!("{POSITION_REF_PREFIX}{world_ref}:{}", position.slug);
    if position.position_ref != expected {
        return Err(format!(
            "ref {} must be {expected} (central:position:<enclosing_world_ref>:<slug>)",
            position.position_ref
        ));
    }
    for (field, value) in [("revision", &position.revision), ("label", &position.label)] {
        if value.trim().is_empty() || value.chars().any(char::is_control) {
            return Err(format!("{field} must be non-empty single-line text"));
        }
    }
    for (field, value) in [
        (
            "enclosing_co_internality_ref",
            &position.enclosing_co_internality_ref,
        ),
        ("role_ref", &position.role_ref),
        ("purpose_ref", &position.purpose_ref),
        ("stewards_ref", &position.stewards_ref),
        ("profile_ref", &position.profile_ref),
        ("continuity_ref", &position.continuity_ref),
    ] {
        check_ref_text(field, value)?;
    }
    if position
        .purpose
        .as_ref()
        .is_some_and(|purpose| purpose.trim().is_empty())
    {
        return Err("purpose must be non-empty text or null".into());
    }
    if let Some(agents) = &position.eligible_agent_refs {
        let mut seen = BTreeSet::new();
        for agent in agents {
            check_ref_text("eligible_agent_refs entry", &Some(agent.clone()))?;
            if !seen.insert(agent) {
                return Err(format!("eligible_agent_refs repeats {agent}"));
            }
        }
    }
    if let Some(handle) = &position.handle {
        if !handle.strip_prefix('@').is_some_and(valid_slug) {
            return Err(format!(
                "handle {handle:?} must be @ followed by [a-z0-9][a-z0-9-]{{0,63}}"
            ));
        }
    }
    Ok(())
}

/// One World scope whose positions are read.
struct PositionScope {
    world_ref: String,
    /// Scope root: the Central root, or the Project root.
    owner_root: PathBuf,
    /// The Project root, for Project profile resolution.
    project_root: Option<PathBuf>,
    /// Central-relative prefix of the owner root ("" or "Work/<member>/").
    prefix: String,
    /// Owner-relative positions directory.
    dir: &'static str,
}

impl PositionScope {
    fn of(scope: &Scope) -> Self {
        let project = scope.project.as_ref();
        Self {
            world_ref: scope.world_ref.clone(),
            owner_root: scope.root.clone(),
            project_root: project.map(|_| scope.root.clone()),
            prefix: project
                .map(|member| format!("Work/{member}/"))
                .unwrap_or_default(),
            dir: if project.is_some() {
                PROJECT_POSITION_DIR
            } else {
                ROOT_POSITION_DIR
            },
        }
    }
    fn ancestry(&self) -> Vec<&str> {
        let mut ancestry = vec![CONTROL_WORLD_REF];
        if self.world_ref != CONTROL_WORLD_REF {
            ancestry.push(&self.world_ref);
        }
        ancestry
    }
}

/// A valid Position with its source basis.
struct Entry {
    path: String,
    record: Value,
    position: WorldPosition,
    source_ref: String,
    revision: String,
}

impl Entry {
    fn reading(&self) -> Value {
        json!({
            "record": self.record,
            "source": {"ref": self.source_ref, "revision": self.revision, "path": self.path},
        })
    }
}

fn invalid_entry(path: &str, error: impl Into<String>) -> Value {
    json!({"path": path, "error": error.into()})
}

/// `profile_ref` resolves when an AgentProfile with that ref exists in the
/// Project store or the root store and belongs to a World in this Position's
/// ancestry.
fn resolve_profile(
    central: &Path,
    scope: &PositionScope,
    profile_ref: &str,
) -> Result<String, String> {
    let ancestry = scope.ancestry();
    let mut stores = Vec::new();
    if let Some(project_root) = &scope.project_root {
        stores.push(AgentProfileStore::project(project_root));
    }
    stores.push(AgentProfileStore::personal(central));
    for store in stores {
        match store.read(profile_ref) {
            Ok(reading) => {
                let world = reading.profile.world_ref.0.as_str();
                return if ancestry.contains(&world) {
                    Ok(reading.source_path)
                } else {
                    Err(format!(
                        "profile_ref {profile_ref} belongs to {world}, outside this Position's World ancestry ({})",
                        ancestry.join(" > ")
                    ))
                };
            }
            Err(AgentProfileStoreError::NotFound(_)) => continue,
            Err(error) => return Err(format!("profile_ref {profile_ref} is unreadable: {error}")),
        }
    }
    Err(format!(
        "profile_ref {profile_ref} does not resolve to an AgentProfile in {}",
        ancestry.join(" or ")
    ))
}

/// Read one scope's Position files. Valid files come back parsed; every other
/// `.json` file comes back as an `{path, error}` entry. Hidden files and
/// non-JSON files are not Position sources and are ignored.
fn read_scope(central: &Path, scope: &PositionScope) -> (Vec<Entry>, Vec<Value>) {
    let dir = scope.owner_root.join(scope.dir);
    let dir_path = format!("{}{}", scope.prefix, scope.dir);
    let mut valid = Vec::new();
    let mut invalid = Vec::new();
    match fs::symlink_metadata(&dir) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => return (valid, invalid),
        Err(error) => {
            invalid.push(invalid_entry(&dir_path, error.to_string()));
            return (valid, invalid);
        }
        Ok(meta) if meta.file_type().is_symlink() || !meta.is_dir() => {
            invalid.push(invalid_entry(
                &dir_path,
                "the positions container must be a plain directory, not a symlink or file",
            ));
            return (valid, invalid);
        }
        Ok(_) => {}
    }
    let mut names: Vec<String> = match fs::read_dir(&dir) {
        Ok(entries) => entries
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect(),
        Err(error) => {
            invalid.push(invalid_entry(&dir_path, error.to_string()));
            return (valid, invalid);
        }
    };
    names.sort();
    for name in names {
        if name.starts_with('.') || !name.ends_with(".json") {
            continue;
        }
        let file = dir.join(&name);
        let path = format!("{dir_path}/{name}");
        let meta = match fs::symlink_metadata(&file) {
            Ok(meta) => meta,
            Err(error) => {
                invalid.push(invalid_entry(&path, error.to_string()));
                continue;
            }
        };
        if meta.file_type().is_symlink() {
            invalid.push(invalid_entry(
                &path,
                "a symlinked Position source is refused",
            ));
            continue;
        }
        if !meta.is_file() {
            continue;
        }
        if meta.len() > MAX_POSITION_BYTES {
            invalid.push(invalid_entry(&path, "Position source exceeds 256 KiB"));
            continue;
        }
        let bytes = match fs::read(&file) {
            Ok(bytes) => bytes,
            Err(error) => {
                invalid.push(invalid_entry(&path, error.to_string()));
                continue;
            }
        };
        let record: Value = match serde_json::from_slice(&bytes) {
            Ok(record) => record,
            Err(error) => {
                invalid.push(invalid_entry(&path, format!("not JSON: {error}")));
                continue;
            }
        };
        let position: WorldPosition = match serde_json::from_value(record.clone()) {
            Ok(position) => position,
            Err(error) => {
                invalid.push(invalid_entry(&path, error.to_string()));
                continue;
            }
        };
        if let Err(error) = validate(&position, &name, &scope.world_ref) {
            invalid.push(invalid_entry(&path, error));
            continue;
        }
        if let Some(profile_ref) = &position.profile_ref {
            if let Err(error) = resolve_profile(central, scope, profile_ref) {
                invalid.push(invalid_entry(&path, error));
                continue;
            }
        }
        valid.push(Entry {
            source_ref: crate::source_horizon::source_ref(
                &scope.world_ref,
                &format!("{}/{name}", scope.dir),
            ),
            revision: crate::source_safety::content_revision_bytes(&bytes),
            path,
            record,
            position,
        });
    }
    (valid, invalid)
}

/// Positions sharing a handle inside one World are all invalid: an address
/// that resolves to two Positions resolves to none. A Project Position that
/// reuses an inherited root handle is invalid for the same reason.
fn enforce_handles(valid: &mut Vec<Entry>, invalid: &mut Vec<Value>, inherited: &[Entry]) {
    let mut holders: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for entry in valid.iter() {
        if let Some(handle) = &entry.position.handle {
            holders
                .entry(handle.clone())
                .or_default()
                .push(entry.position.position_ref.clone());
        }
    }
    let inherited_handles: BTreeMap<&str, &str> = inherited
        .iter()
        .filter_map(|entry| {
            entry
                .position
                .handle
                .as_deref()
                .map(|handle| (handle, entry.position.position_ref.as_str()))
        })
        .collect();
    valid.retain(|entry| {
        let Some(handle) = entry.position.handle.as_deref() else {
            return true;
        };
        if let Some(owner) = inherited_handles.get(handle) {
            invalid.push(invalid_entry(
                &entry.path,
                format!("handle {handle} is already held by inherited Position {owner}"),
            ));
            return false;
        }
        let owners = &holders[handle];
        if owners.len() > 1 {
            invalid.push(invalid_entry(
                &entry.path,
                format!(
                    "handle {handle} is claimed by {} Positions in this World: {}",
                    owners.len(),
                    owners.join(", ")
                ),
            ));
            return false;
        }
        true
    });
}

struct Listing {
    world_ref: String,
    positions: Vec<Entry>,
    inherited: Vec<Entry>,
    invalid: Vec<Value>,
}

fn listing(central: &Path, project: Option<&str>) -> io::Result<Listing> {
    let root = Scope::resolve(central, None)?;
    let root_scope = PositionScope::of(&root);
    let (mut root_valid, mut root_invalid) = read_scope(&root.central_root, &root_scope);
    enforce_handles(&mut root_valid, &mut root_invalid, &[]);
    let Some(project) = project else {
        return Ok(Listing {
            world_ref: root.world_ref,
            positions: root_valid,
            inherited: Vec::new(),
            invalid: root_invalid,
        });
    };
    let scope = Scope::resolve(central, Some(project))?;
    let project_scope = PositionScope::of(&scope);
    let (mut valid, mut invalid) = read_scope(&root.central_root, &project_scope);
    enforce_handles(&mut valid, &mut invalid, &root_valid);
    invalid.extend(root_invalid);
    Ok(Listing {
        world_ref: scope.world_ref,
        positions: valid,
        inherited: root_valid,
        invalid,
    })
}

/// `central.position.list` — `central.position-listing/v1`, uncapped.
pub fn list_positions(central: &Path, project: Option<&str>) -> io::Result<Value> {
    let listing = listing(central, project)?;
    Ok(json!({
        "schema": POSITION_LISTING_SCHEMA,
        "world_ref": listing.world_ref,
        "positions": listing.positions.iter().map(Entry::reading).collect::<Vec<_>>(),
        "inherited": listing.inherited.iter().map(Entry::reading).collect::<Vec<_>>(),
        "invalid": listing.invalid,
    }))
}

/// A three-part refusal: the current fact, what did not happen, and the exact
/// next lawful command.
#[derive(Debug, Clone, PartialEq)]
pub struct PositionRefusal {
    pub code: &'static str,
    pub fact: String,
    pub consequence: String,
    pub action: String,
    pub path: Option<String>,
}

impl PositionRefusal {
    fn result(&self, position_ref: &str) -> ActionResult {
        let status = if self.code == POSITION_INVALID_CODE {
            ResultStatus::VerificationFailure
        } else if self.code == POSITION_NOT_FOUND_CODE {
            ResultStatus::UnavailableCapability
        } else {
            ResultStatus::InvalidInput
        };
        ActionResult::failure_repairable(
            Some(POSITION_READ_ACTION),
            status,
            self.code,
            format!("{} {} {}", self.fact, self.consequence, self.action),
            Some(json!({
                "fact": self.fact,
                "consequence": self.consequence,
                "action": self.action,
                "position_ref": position_ref,
                "path": self.path,
            })),
            Some(self.action.clone()),
        )
    }
}

fn list_command(project: Option<&str>) -> String {
    match project {
        Some(member) => {
            format!("ctrl --json action run central.position.list '{{\"project\":\"{member}\"}}'")
        }
        None => "ctrl --json action run central.position.list '{}'".into(),
    }
}

/// The Work member whose ProjectCentral declares `project_id`.
fn member_for(central: &Path, project_id: &str) -> Result<String, PositionRefusal> {
    let mut members: Vec<String> = fs::read_dir(central.join("Work"))
        .map(|entries| {
            entries
                .filter_map(|entry| entry.ok())
                .filter_map(|entry| {
                    let name = entry.file_name().to_str()?.to_owned();
                    let manifest =
                        crate::projectcentral::read_project_manifest(&entry.path()).ok()?;
                    (manifest.project_id == project_id).then_some(name)
                })
                .collect()
        })
        .unwrap_or_default();
    members.sort();
    match members.as_slice() {
        [member] => Ok(member.clone()),
        [] => Err(PositionRefusal {
            code: POSITION_NOT_FOUND_CODE,
            fact: format!("No Work member declares project_id {project_id} in its ProjectCentral."),
            consequence: "Nothing was read; the Position's World does not exist here.".into(),
            action: "Run `ctrl --json action run central.world '{}'` to list the Project Worlds, then read a Position of one of them.".into(),
            path: None,
        }),
        many => Err(PositionRefusal {
            code: POSITION_NOT_FOUND_CODE,
            fact: format!(
                "{} Work members declare project_id {project_id}: {}.",
                many.len(),
                many.join(", ")
            ),
            consequence: "Nothing was read; Central never chooses between Worlds that share an identity.".into(),
            action: format!(
                "Pass project to name the member, e.g. `ctrl --json action run central.position.read '{{\"position_ref\":\"…\",\"project\":\"{}\"}}'`, and reconcile the duplicate project_id.",
                many[0]
            ),
            path: None,
        }),
    }
}

/// `central.position.read` — the record plus its source basis, or a refusal.
pub fn read_position(
    central: &Path,
    position_ref: &str,
    project: Option<&str>,
) -> Result<Value, PositionRefusal> {
    let Some((world_ref, slug)) = parse_position_ref(position_ref) else {
        return Err(PositionRefusal {
            code: "invalid_input",
            fact: format!(
                "{position_ref:?} is not a Position ref (central:position:<control:root|project:<id>>:<slug>, slug [a-z0-9][a-z0-9-]{{0,63}})."
            ),
            consequence: "Nothing was read.".into(),
            action: format!(
                "Take the exact ref from {}.",
                list_command(project)
            ),
            path: None,
        });
    };
    let member = if world_ref == CONTROL_WORLD_REF {
        None
    } else if let Some(member) = project {
        Some(member.to_owned())
    } else {
        Some(member_for(
            central,
            world_ref.strip_prefix("project:").unwrap_or_default(),
        )?)
    };
    let io_refusal = |error: io::Error| PositionRefusal {
        code: POSITION_NOT_FOUND_CODE,
        fact: format!("The World of {position_ref} cannot be resolved: {error}."),
        consequence: "Nothing was read.".into(),
        action:
            "Run `ctrl --json action run central.world '{}'` to see the Worlds this root holds."
                .into(),
        path: None,
    };
    let listing = listing(central, member.as_deref()).map_err(io_refusal)?;
    if listing.world_ref != world_ref {
        return Err(PositionRefusal {
            code: POSITION_NOT_FOUND_CODE,
            fact: format!(
                "Work/{} is the World {}, not {world_ref}.",
                member.as_deref().unwrap_or_default(),
                listing.world_ref
            ),
            consequence: "Nothing was read.".into(),
            action: format!(
                "Omit project, or name the member whose ProjectCentral declares {world_ref}."
            ),
            path: None,
        });
    }
    let path = format!(
        "{}{}/{slug}.json",
        member
            .as_deref()
            .map(|member| format!("Work/{member}/"))
            .unwrap_or_default(),
        if member.is_some() {
            PROJECT_POSITION_DIR
        } else {
            ROOT_POSITION_DIR
        }
    );
    if let Some(entry) = listing
        .positions
        .iter()
        .find(|entry| entry.position.position_ref == position_ref)
    {
        let mut reading = entry.reading();
        reading["schema"] = json!(POSITION_READING_SCHEMA);
        return Ok(reading);
    }
    if let Some(error) = listing
        .invalid
        .iter()
        .find(|entry| entry["path"] == path)
        .and_then(|entry| entry["error"].as_str())
    {
        return Err(PositionRefusal {
            code: POSITION_INVALID_CODE,
            fact: format!("Position source {path} is invalid: {error}."),
            consequence: "The Position was not read; an invalid definition never resolves as an address.".into(),
            action: format!(
                "Correct {path} so it satisfies {POSITION_SCHEMA}, then rerun `ctrl --json action run central.position.read '{{\"position_ref\":\"{position_ref}\"}}'`."
            ),
            path: Some(path),
        });
    }
    Err(PositionRefusal {
        code: POSITION_NOT_FOUND_CODE,
        fact: format!("No Position {position_ref} is defined: {path} does not exist."),
        consequence: "Nothing was read; no Position address was resolved.".into(),
        action: format!(
            "Run {} to see the defined Positions, or author {path}.",
            list_command(member.as_deref())
        ),
        path: Some(path),
    })
}

pub const POSITION_LIST_ACTION: &str = "central.position.list";
pub const POSITION_READ_ACTION: &str = "central.position.read";

/// The Central root and the optional Work member of a Position request, or
/// the reason the request is malformed.
fn request(
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> Result<(PathBuf, Option<String>), String> {
    let project = match input.get("project") {
        None | Some(Value::Null) => None,
        Some(Value::String(raw))
            if !raw.trim().is_empty()
                && raw == raw.trim()
                && !raw.starts_with('.')
                && Path::new(raw).components().count() == 1
                && !raw.contains('/') =>
        {
            Some(raw.clone())
        }
        Some(_) => {
            return Err(
                "project must name one Central/Work member (a single path component)".into(),
            )
        }
    };
    let central = resolve_central_root(context.root_options)?.path;
    Ok((central, project))
}

fn list_action(
    _: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let action = POSITION_LIST_ACTION;
    let (central, project) = match request(input, context) {
        Ok(request) => request,
        Err(message) => {
            return ActionResult::failure(Some(action), ResultStatus::InvalidInput, message, None)
        }
    };
    match list_positions(&central, project.as_deref()) {
        Ok(listing) => ActionResult::success(action, listing),
        Err(error) => ActionResult::failure_coded(
            Some(action),
            ResultStatus::InvalidInput,
            "source_or_scope_unavailable",
            format!(
                "The Position World cannot be resolved: {error}. Nothing was listed. Run `ctrl --json action run central.world '{{}}'` to see the Worlds this root holds."
            ),
            None,
        ),
    }
}

fn read_action(
    _: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let action = POSITION_READ_ACTION;
    let (central, project) = match request(input, context) {
        Ok(request) => request,
        Err(message) => {
            return ActionResult::failure(Some(action), ResultStatus::InvalidInput, message, None)
        }
    };
    let Some(position_ref) = input.get("position_ref").and_then(Value::as_str) else {
        return ActionResult::failure(
            Some(action),
            ResultStatus::InvalidInput,
            "position_ref is required (central:position:<world_ref>:<slug>).",
            None,
        );
    };
    match read_position(&central, position_ref, project.as_deref()) {
        Ok(reading) => ActionResult::success(action, reading),
        Err(refusal) => refusal.result(position_ref),
    }
}

fn input(name: &str, required: bool) -> ActionInputDefinition {
    ActionInputDefinition {
        name: name.into(),
        input_type: "string".into(),
        required,
        choices: None,
        selection: None,
    }
}

fn descriptor(
    id: &str,
    title: &str,
    description: &str,
    inputs: Vec<ActionInputDefinition>,
    output: &str,
) -> ActionDescriptor {
    ActionDescriptor {
        id: id.into(),
        title: title.into(),
        description: description.into(),
        inputs,
        output: ActionOutputDefinition {
            output_type: output.into(),
        },
        mutation_class: MutationClass::ReadOnly,
        preview_supported: false,
        required_ports: vec![],
        availability: ActionAvailability {
            available: true,
            reason: None,
        },
    }
}

pub fn register_world_position_actions(registry: &mut ActionRegistry) {
    registry
        .register(
            descriptor(
                POSITION_LIST_ACTION,
                "List World Positions",
                "List the Position definitions (central.world-position/v1) of the root World, or of one Project World with the root Positions as inherited. Uncapped; files that fail strict validation are reported in invalid[] with their error, never dropped. Read-only.",
                vec![input("project", false)],
                POSITION_LISTING_SCHEMA,
            ),
            list_action,
        )
        .expect("central.position.list is registered once");
    registry
        .register(
            descriptor(
                POSITION_READ_ACTION,
                "Read one World Position",
                "Read one Position definition by its ref (central:position:<world_ref>:<slug>) with its source ref and revision. An absent or invalid definition refuses with fact, consequence and the next command (central.position_not_found, central.position_invalid). Read-only.",
                vec![input("position_ref", true), input("project", false)],
                POSITION_READING_SCHEMA,
            ),
            read_action,
        )
        .expect("central.position.read is registered once");
}
