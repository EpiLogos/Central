//! Owner read model over Central's canonical Agent Wiki sources.
//!
//! Wave 2 / U3.4 slice 2: the graph consumes owner data through these Actions
//! instead of a desktop parse of `wiki.json`. The canonical sources stay the
//! root `Control/agents/wiki/wiki.json` and each Project's
//! `ProjectCentral/agents/wiki/wiki.json` (`okf-wiki/v1`, agent-maintained via
//! `aikit wiki` only). This module never writes a wiki source; the return is
//! the only door. Nothing here duplicates AIKit's semantic/relations reading —
//! Central owns the source ground and its structural truth; relation rows are
//! derived from the declared wiki structure at read time.
//!
//! Every disclosed node and space carries the U0.2 source ref of the wiki
//! source it was read from, so a consumer can open the exact payload through
//! the owner source Actions (`projectcentral.source.read` for Project wiki
//! sources). Absent or corrupt wiki ground is an explicit state, never an
//! empty success.
use crate::action::{
    ActionAvailability, ActionDescriptor, ActionExecutionContext, ActionInputDefinition,
    ActionOutputDefinition, ActionRegistry, MutationClass,
};
use crate::projectcentral::{read_project_manifest, ROOT_WIKI_SOURCE, WIKI_PROFILE};
use crate::result::{ActionResult, ResultStatus};
use crate::root::resolve_central_root;
use crate::source_horizon::source_ref;
use serde::Serialize;
use serde_json::{json, Map, Value};
use std::collections::BTreeSet;
use std::io;
use std::path::{Path, PathBuf};

pub const WIKI_READING_SCHEMA: &str = "central.wiki-reading/v1";

/// The wiki source this reading was derived from, addressed in the U0.2
/// source-ref grammar so it round-trips through the owner source Actions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WikiSourcePointer {
    #[serde(rename = "ref")]
    pub source_ref: String,
    pub path: String,
    pub revision: String,
}

/// One `okf-wiki/v1` space object as disclosed by the read model.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WikiSpaceReading {
    #[serde(rename = "ref")]
    pub space_ref: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revision: Option<u64>,
    pub parent_space_refs: Vec<String>,
    pub child_space_refs: Vec<String>,
    pub node_refs: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub anchor_ref: Option<String>,
}

/// One `okf-wiki/v1` node object as disclosed by the read model. `source_refs`
/// and `provenance_source_refs` are the node's own declared grounds; they are
/// wiki-relative or world-relative paths as authored, not re-minted refs.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WikiNodeReading {
    #[serde(rename = "ref")]
    pub node_ref: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revision: Option<u64>,
    pub space_refs: Vec<String>,
    pub source_refs: Vec<String>,
    pub provenance_source_refs: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ql: Option<Value>,
}

/// One structural relation between wiki objects or between a node and one of
/// its declared sources, derived at read time from the wiki document itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WikiRelation {
    pub kind: String,
    pub from_ref: String,
    pub to_ref: String,
}

/// One typed WikiEdge as the wiki carries it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WikiEdgeReading {
    #[serde(rename = "ref")]
    pub edge_ref: String,
    pub from_ref: String,
    pub relation: String,
    pub to_ref: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub origin: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub origin_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revision: Option<u64>,
}

/// Owner counts derived at read time: node/edge totals plus the raw object
/// census so a consumer can detect a wiki whose shape it does not know.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WikiCounts {
    pub spaces: usize,
    pub nodes: usize,
    pub edges: usize,
    /// Typed knowledge edges (`object: edge`) carried in `knowledge_edges`.
    pub knowledge_edges: usize,
    pub objects: usize,
    pub other_objects: usize,
}

/// The canonical read model of one register's wiki source.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WikiReading {
    pub schema: String,
    /// Which register this reading belongs to: `root` or `project`.
    pub register: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    /// The U0.2 world identity: `control:root` or `project:{project_id}`.
    pub world_ref: String,
    pub profile: String,
    pub source: WikiSourcePointer,
    pub spaces: Vec<WikiSpaceReading>,
    pub nodes: Vec<WikiNodeReading>,
    pub relations: Vec<WikiRelation>,
    /// The wiki's own typed knowledge edges (`object: edge` — references,
    /// contemplates, explains, …) with their origin and revision. Structural
    /// `relations` above stay derived from spaces and nodes; these are read
    /// verbatim so a consumer can reveal what the wiki actually relates.
    pub knowledge_edges: Vec<WikiEdgeReading>,
    pub counts: WikiCounts,
    pub automatic_agent_or_model_invocation: bool,
}

/// Why one wiki source could not be read, stated explicitly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WikiReadFailure {
    /// `absent` (no wiki.json), `unreadable` (IO/UTF-8/bounds) or `invalid`
    /// (not an `okf-wiki/v1` objects document).
    pub state: &'static str,
    pub message: String,
}

impl WikiReadFailure {
    fn absent(message: impl Into<String>) -> Self {
        Self {
            state: "absent",
            message: message.into(),
        }
    }
    fn unreadable(message: impl Into<String>) -> Self {
        Self {
            state: "unreadable",
            message: message.into(),
        }
    }
    fn invalid(message: impl Into<String>) -> Self {
        Self {
            state: "invalid",
            message: message.into(),
        }
    }
}

fn string_array(object: &Map<String, Value>, key: &str) -> Vec<String> {
    object
        .get(key)
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

fn optional_string(object: &Map<String, Value>, key: &str) -> Option<String> {
    object.get(key).and_then(Value::as_str).map(str::to_owned)
}

fn parse_wiki_document(content: &str) -> Result<Vec<WikiReadingObject>, WikiReadFailure> {
    let document: Value = serde_json::from_str(content).map_err(|error| {
        WikiReadFailure::invalid(format!("wiki source is not valid JSON: {error}"))
    })?;
    let objects = document
        .get("objects")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            WikiReadFailure::invalid("wiki source is not an okf-wiki/v1 objects document")
        })?;
    let mut parsed = Vec::new();
    for object in objects {
        let Some(map) = object.as_object() else {
            parsed.push(WikiReadingObject::Other);
            continue;
        };
        let profile = optional_string(map, "profile");
        if profile.as_deref() != Some(WIKI_PROFILE) {
            parsed.push(WikiReadingObject::Other);
            continue;
        }
        match optional_string(map, "object").as_deref() {
            Some("space") => {
                let Some(space_ref) = optional_string(map, "ref") else {
                    parsed.push(WikiReadingObject::Other);
                    continue;
                };
                parsed.push(WikiReadingObject::Space(WikiSpaceReading {
                    space_ref,
                    title: optional_string(map, "title"),
                    revision: map.get("revision").and_then(Value::as_u64),
                    parent_space_refs: string_array(map, "parent_space_refs"),
                    child_space_refs: string_array(map, "child_space_refs"),
                    node_refs: string_array(map, "node_refs"),
                    anchor_ref: optional_string(map, "anchor_ref"),
                }));
            }
            Some("node") => {
                let Some(node_ref) = optional_string(map, "ref") else {
                    parsed.push(WikiReadingObject::Other);
                    continue;
                };
                let mut provenance_source_refs = Vec::new();
                if let Some(entries) = map.get("provenance").and_then(Value::as_array) {
                    for entry in entries {
                        if let Some(source) = entry.get("source_ref").and_then(Value::as_str) {
                            provenance_source_refs.push(source.to_owned());
                        }
                    }
                }
                provenance_source_refs.sort();
                provenance_source_refs.dedup();
                parsed.push(WikiReadingObject::Node(WikiNodeReading {
                    node_ref,
                    title: optional_string(map, "title"),
                    node_type: optional_string(map, "type"),
                    revision: map.get("revision").and_then(Value::as_u64),
                    space_refs: string_array(map, "space_refs"),
                    source_refs: string_array(map, "source_refs"),
                    provenance_source_refs,
                    ql: map.get("ql").cloned(),
                }));
            }
            Some("edge") => {
                let (Some(edge_ref), Some(from_ref), Some(relation), Some(to_ref)) = (
                    optional_string(map, "ref"),
                    optional_string(map, "from_ref"),
                    optional_string(map, "relation"),
                    optional_string(map, "to_ref"),
                ) else {
                    parsed.push(WikiReadingObject::Other);
                    continue;
                };
                parsed.push(WikiReadingObject::Edge(WikiEdgeReading {
                    edge_ref,
                    from_ref,
                    relation,
                    to_ref,
                    origin: optional_string(map, "origin"),
                    origin_ref: optional_string(map, "origin_ref"),
                    revision: map.get("revision").and_then(Value::as_u64),
                }));
            }
            _ => parsed.push(WikiReadingObject::Other),
        }
    }
    Ok(parsed)
}

enum WikiReadingObject {
    Space(WikiSpaceReading),
    Node(WikiNodeReading),
    Edge(WikiEdgeReading),
    Other,
}

fn derive_relations(spaces: &[WikiSpaceReading], nodes: &[WikiNodeReading]) -> Vec<WikiRelation> {
    let mut relations = BTreeSet::new();
    for space in spaces {
        for child in &space.child_space_refs {
            relations.insert((
                "space-child-space".to_owned(),
                space.space_ref.clone(),
                child.clone(),
            ));
        }
        for node in &space.node_refs {
            relations.insert((
                "space-node".to_owned(),
                space.space_ref.clone(),
                node.clone(),
            ));
        }
    }
    for node in nodes {
        for space in &node.space_refs {
            relations.insert((
                "node-space".to_owned(),
                node.node_ref.clone(),
                space.clone(),
            ));
        }
        for sources in [&node.source_refs, &node.provenance_source_refs] {
            for source in sources.iter() {
                relations.insert((
                    "node-source".to_owned(),
                    node.node_ref.clone(),
                    source.clone(),
                ));
            }
        }
    }
    relations
        .into_iter()
        .map(|(kind, from_ref, to_ref)| WikiRelation {
            kind,
            from_ref,
            to_ref,
        })
        .collect()
}

/// Read one canonical wiki source at `relative` below `world_root` and derive
/// the structural read model. `world_ref` is the U0.2 world identity used to
/// mint the disclosed source ref; `register`/`project` identify the register.
fn read_wiki(
    world_root: &Path,
    world_ref: &str,
    register: &str,
    project: Option<&str>,
    relative: &str,
) -> Result<WikiReading, WikiReadFailure> {
    let pointer_ref = source_ref(world_ref, relative);
    let content = crate::source_safety::read(world_root, relative).map_err(|error| {
        if error.kind() == io::ErrorKind::NotFound {
            WikiReadFailure::absent(format!(
                "{} has no wiki source at {relative}",
                world_root.display()
            ))
        } else {
            WikiReadFailure::unreadable(format!("wiki source at {relative} is unreadable: {error}"))
        }
    })?;
    let revision = crate::source_safety::content_revision_bytes(content.as_bytes());
    let parsed = parse_wiki_document(&content)?;
    let mut spaces = Vec::new();
    let mut nodes = Vec::new();
    let mut knowledge_edges = Vec::new();
    let mut other_objects = 0usize;
    for object in parsed {
        match object {
            WikiReadingObject::Space(space) => spaces.push(space),
            WikiReadingObject::Node(node) => nodes.push(node),
            WikiReadingObject::Edge(edge) => knowledge_edges.push(edge),
            WikiReadingObject::Other => other_objects += 1,
        }
    }
    let relations = derive_relations(&spaces, &nodes);
    let counts = WikiCounts {
        spaces: spaces.len(),
        nodes: nodes.len(),
        edges: relations.len(),
        knowledge_edges: knowledge_edges.len(),
        objects: spaces.len() + nodes.len() + knowledge_edges.len() + other_objects,
        other_objects,
    };
    Ok(WikiReading {
        schema: WIKI_READING_SCHEMA.to_owned(),
        register: register.to_owned(),
        project: project.map(str::to_owned),
        world_ref: world_ref.to_owned(),
        profile: WIKI_PROFILE.to_owned(),
        source: WikiSourcePointer {
            source_ref: pointer_ref,
            path: relative.to_owned(),
            revision,
        },
        spaces,
        nodes,
        relations,
        knowledge_edges,
        counts,
        automatic_agent_or_model_invocation: false,
    })
}

/// Read the root register wiki (`Control/agents/wiki/wiki.json`).
pub fn read_root_wiki(central_root: &Path) -> Result<WikiReading, WikiReadFailure> {
    read_wiki(central_root, "control:root", "root", None, ROOT_WIKI_SOURCE)
}

/// Read one Project register wiki (`ProjectCentral/agents/wiki/wiki.json` as
/// declared by the Project manifest). The manifest is the identity authority:
/// its validated `project_id` and canonical `wiki.source` mint the U0.2 refs.
pub fn read_project_wiki(project_root: &Path) -> Result<WikiReading, WikiReadFailure> {
    let (project_id, wiki_source) = project_wiki_binding(project_root)?;
    let world_ref = format!("project:{project_id}");
    read_wiki(
        project_root,
        &world_ref,
        "project",
        Some(&project_id),
        &wiki_source,
    )
}

/// The validated Project identity and canonical wiki source path, as declared
/// by the Project manifest (the identity authority for every minted ref).
fn project_wiki_binding(project_root: &Path) -> Result<(String, String), WikiReadFailure> {
    let manifest = read_project_manifest(project_root).map_err(|error| {
        WikiReadFailure::invalid(format!(
            "Project manifest is required to read the wiki source: {error}"
        ))
    })?;
    let validation = manifest.validate();
    if !validation.valid {
        return Err(WikiReadFailure::invalid(format!(
            "Project manifest is not a valid ProjectCentral manifest: {}",
            validation.errors.join("; ")
        )));
    }
    Ok((manifest.project_id, manifest.wiki.source))
}

fn failure_result(
    action: &str,
    source_ref: &str,
    relative: &str,
    failure: WikiReadFailure,
) -> ActionResult {
    ActionResult::failure(
        Some(action),
        ResultStatus::InvalidCentralStructure,
        failure.message,
        Some(json!({
            "state": failure.state,
            "path": relative,
            "source_ref": source_ref,
        })),
    )
}

fn required(input: &Value, field: &str, action: &str) -> Result<String, ActionResult> {
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

fn central_wiki_read_action(
    _: &ActionRegistry,
    _: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let action = "central.wiki.read";
    let root = match resolve_central_root(context.root_options) {
        Ok(root) => root,
        Err(message) => {
            return ActionResult::failure(Some(action), ResultStatus::InvalidInput, message, None);
        }
    };
    if let Err(error) =
        crate::source_safety::reject_symlink_components(&root.path, Path::new("Control"))
    {
        return ActionResult::failure(
            Some(action),
            ResultStatus::InvalidInput,
            error.to_string(),
            None,
        );
    }
    match read_root_wiki(&root.path) {
        Ok(reading) => ActionResult::success(
            action,
            serde_json::to_value(reading).expect("wiki reading serializes"),
        ),
        Err(failure) => {
            let source_ref = source_ref("control:root", ROOT_WIKI_SOURCE);
            failure_result(action, &source_ref, ROOT_WIKI_SOURCE, failure)
        }
    }
}

fn project_wiki_context(
    action: &str,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> Result<PathBuf, ActionResult> {
    let project = required(input, "project", action)?;
    let project = crate::source_safety::relative_member(&project).map_err(|error| {
        ActionResult::failure(
            Some(action),
            ResultStatus::InvalidInput,
            error.to_string(),
            None,
        )
    })?;
    let root = resolve_central_root(context.root_options)
        .map_err(|message| {
            ActionResult::failure(Some(action), ResultStatus::InvalidInput, message, None)
        })?
        .path;
    crate::source_safety::reject_symlink_components(&root, &Path::new("Work").join(&project))
        .map_err(|error| {
            ActionResult::failure(
                Some(action),
                ResultStatus::InvalidInput,
                error.to_string(),
                None,
            )
        })?;
    let project_root = root.join("Work").join(project);
    if !project_root.is_dir() {
        return Err(ActionResult::failure(
            Some(action),
            ResultStatus::InvalidInput,
            format!(
                "Project root does not exist as a directory: {}",
                project_root.display()
            ),
            None,
        ));
    }
    Ok(project_root)
}

fn projectcentral_wiki_read_action(
    _: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let action = "projectcentral.wiki.read";
    let project_root = match project_wiki_context(action, input, context) {
        Ok(root) => root,
        Err(result) => return result,
    };
    match read_project_wiki(&project_root) {
        Ok(reading) => ActionResult::success(
            action,
            serde_json::to_value(reading).expect("wiki reading serializes"),
        ),
        Err(failure) => {
            let (source_ref, relative) = match read_project_manifest(&project_root) {
                Ok(manifest) => (
                    source_ref(
                        &format!("project:{}", manifest.project_id),
                        &manifest.wiki.source,
                    ),
                    manifest.wiki.source,
                ),
                Err(_) => {
                    let relative = crate::projectcentral::WIKI_SOURCE;
                    (source_ref("project:unknown", relative), relative.to_owned())
                }
            };
            failure_result(action, &source_ref, &relative, failure)
        }
    }
}

// --- Wiki source bank -------------------------------------------------------
//
// A wiki cites authored corpus sources as `central:source:corpus:<id>` and
// carries their revisioned bodies in its own source bank: `corpus-NNN.json`
// shards, each a JSON array of `{binding, body}` records. Neither the wiki
// document nor the Project manifest names the bank, so it is derived beside
// the wiki source exactly as its writer (`aikit wiki ingest`) places it: a
// `<wiki-file-stem>.sources/` directory next to the manifest-declared
// `wiki.json`. These corpus refs are not participating sources of the World,
// so `projectcentral.source.read` cannot serve them; this reader is their
// native door. It is read-only, records no use, and returns `visibility`
// verbatim without enforcing it.

pub const WIKI_SOURCE_READING_SCHEMA: &str = "central.wiki-source-reading/v1";

/// Only corpus-local refs are served from the bank. A bank copy of a live,
/// owner-held source (`central:source:control:…`, `central:source:project:…`)
/// is a stale body that must not bypass its owner reader.
const CORPUS_SOURCE_PREFIX: &str = "central:source:corpus:";
const SOURCE_BANK_SHARD_PREFIX: &str = "corpus-";
const SOURCE_BANK_SHARD_SUFFIX: &str = ".json";

/// One corpus source record read from a wiki's source bank. `revision` is the
/// record's own revision as the bank carries it; `bank_revision` is the
/// Central content revision of the shard file the record was read from.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WikiSourceReading {
    pub schema: String,
    pub register: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    pub world_ref: String,
    pub source_ref: String,
    pub revision: String,
    pub title: Value,
    pub media_type: Value,
    pub visibility: Value,
    pub tags: Value,
    pub owners: Value,
    pub locator: Value,
    pub metadata: Value,
    pub body: String,
    pub wiki_ref: String,
    pub bank_ref: String,
    pub bank_path: String,
    pub bank_revision: String,
    pub automatic_agent_or_model_invocation: bool,
}

/// Why a bank read did not produce a reading: a refused request (the ref is
/// not a bank source) versus bank ground that is absent or malformed.
#[derive(Debug)]
enum WikiSourceReadFailure {
    Refused(String),
    Ground(WikiReadFailure, String),
}

/// `ProjectCentral/agents/wiki/wiki.json` → `ProjectCentral/agents/wiki/wiki.sources`.
fn source_bank_path(wiki_relative: &str) -> String {
    let path = Path::new(wiki_relative);
    let stem = path
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_else(|| "wiki".to_owned());
    match path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        Some(parent) => format!("{}/{stem}.sources", parent.display()),
        None => format!("{stem}.sources"),
    }
}

fn read_wiki_source(
    world_root: &Path,
    world_ref: &str,
    register: &str,
    project: Option<&str>,
    wiki_relative: &str,
    requested: &str,
) -> Result<WikiSourceReading, WikiSourceReadFailure> {
    if !requested.starts_with(CORPUS_SOURCE_PREFIX) {
        return Err(WikiSourceReadFailure::Refused(format!(
            "source_ref is not in this wiki's source bank: only {CORPUS_SOURCE_PREFIX}<id> sources are banked; read owner-held sources through their owner Action (projectcentral.source.read)"
        )));
    }
    let bank = source_bank_path(wiki_relative);
    let ground = |failure: WikiReadFailure| WikiSourceReadFailure::Ground(failure, bank.clone());
    crate::source_safety::reject_symlink_components(world_root, Path::new(&bank)).map_err(
        |error| {
            if error.kind() == io::ErrorKind::NotFound {
                ground(WikiReadFailure::absent(format!(
                    "{} has no wiki source bank at {bank}",
                    world_root.display()
                )))
            } else {
                ground(WikiReadFailure::unreadable(format!(
                    "wiki source bank at {bank} is unreadable: {error}"
                )))
            }
        },
    )?;
    let entries = match std::fs::read_dir(world_root.join(&bank)) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Err(ground(WikiReadFailure::absent(format!(
                "{} has no wiki source bank at {bank}",
                world_root.display()
            ))))
        }
        Err(error) => {
            return Err(ground(WikiReadFailure::unreadable(format!(
                "wiki source bank at {bank} is unreadable: {error}"
            ))))
        }
    };
    let mut shards = entries
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| {
            name.starts_with(SOURCE_BANK_SHARD_PREFIX) && name.ends_with(SOURCE_BANK_SHARD_SUFFIX)
        })
        .collect::<Vec<_>>();
    shards.sort();
    for shard in shards {
        let shard_path = format!("{bank}/{shard}");
        let content = crate::source_safety::read(world_root, &shard_path).map_err(|error| {
            ground(WikiReadFailure::unreadable(format!(
                "wiki source bank shard {shard_path} is unreadable: {error}"
            )))
        })?;
        let records: Vec<Value> = serde_json::from_str(&content).map_err(|error| {
            ground(WikiReadFailure::invalid(format!(
                "wiki source bank shard {shard_path} is not a JSON array of source records: {error}"
            )))
        })?;
        let Some(record) = records.into_iter().find(|record| {
            record.pointer("/binding/source").and_then(Value::as_str) == Some(requested)
        }) else {
            continue;
        };
        let binding = record.get("binding").cloned().unwrap_or(Value::Null);
        let field = |key: &str| binding.get(key).cloned().unwrap_or(Value::Null);
        let revision = binding.get("revision").and_then(Value::as_str);
        let body = record.get("body").and_then(Value::as_str);
        let (Some(revision), Some(body)) = (revision, body) else {
            return Err(ground(WikiReadFailure::invalid(format!(
                "wiki source bank record {requested} in {shard_path} lacks a string revision or body"
            ))));
        };
        return Ok(WikiSourceReading {
            schema: WIKI_SOURCE_READING_SCHEMA.to_owned(),
            register: register.to_owned(),
            project: project.map(str::to_owned),
            world_ref: world_ref.to_owned(),
            source_ref: requested.to_owned(),
            revision: revision.to_owned(),
            title: field("title"),
            media_type: field("media_type"),
            visibility: field("visibility"),
            tags: field("tags"),
            owners: field("owners"),
            locator: field("locator"),
            metadata: field("metadata"),
            body: body.to_owned(),
            wiki_ref: source_ref(world_ref, wiki_relative),
            bank_ref: source_ref(world_ref, &shard_path),
            bank_path: shard_path,
            bank_revision: crate::source_safety::content_revision_bytes(content.as_bytes()),
            automatic_agent_or_model_invocation: false,
        });
    }
    Err(WikiSourceReadFailure::Refused(
        "source_ref is not in this wiki's source bank".to_owned(),
    ))
}

fn wiki_source_result(
    action: &str,
    world_ref: &str,
    requested: &str,
    outcome: Result<WikiSourceReading, WikiSourceReadFailure>,
) -> ActionResult {
    match outcome {
        Ok(reading) => ActionResult::success(
            action,
            serde_json::to_value(reading).expect("wiki source reading serializes"),
        ),
        Err(WikiSourceReadFailure::Refused(message)) => ActionResult::failure(
            Some(action),
            ResultStatus::InvalidInput,
            message,
            Some(json!({"source_ref": requested})),
        ),
        Err(WikiSourceReadFailure::Ground(failure, bank)) => {
            let bank_ref = source_ref(world_ref, &bank);
            failure_result(action, &bank_ref, &bank, failure)
        }
    }
}

fn central_wiki_source_read_action(
    _: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let action = "central.wiki.source.read";
    let requested = match required(input, "source_ref", action) {
        Ok(value) => value,
        Err(result) => return result,
    };
    let root = match resolve_central_root(context.root_options) {
        Ok(root) => root,
        Err(message) => {
            return ActionResult::failure(Some(action), ResultStatus::InvalidInput, message, None);
        }
    };
    if let Err(error) =
        crate::source_safety::reject_symlink_components(&root.path, Path::new("Control"))
    {
        return ActionResult::failure(
            Some(action),
            ResultStatus::InvalidInput,
            error.to_string(),
            None,
        );
    }
    let world_ref = "control:root";
    let outcome = read_wiki_source(
        &root.path,
        world_ref,
        "root",
        None,
        ROOT_WIKI_SOURCE,
        &requested,
    );
    wiki_source_result(action, world_ref, &requested, outcome)
}

fn projectcentral_wiki_source_read_action(
    _: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let action = "projectcentral.wiki.source.read";
    let project_root = match project_wiki_context(action, input, context) {
        Ok(root) => root,
        Err(result) => return result,
    };
    let requested = match required(input, "source_ref", action) {
        Ok(value) => value,
        Err(result) => return result,
    };
    let (project_id, wiki_source) = match project_wiki_binding(&project_root) {
        Ok(binding) => binding,
        Err(failure) => {
            let relative = crate::projectcentral::WIKI_SOURCE;
            let pointer = source_ref("project:unknown", relative);
            return failure_result(action, &pointer, relative, failure);
        }
    };
    let world_ref = format!("project:{project_id}");
    let outcome = read_wiki_source(
        &project_root,
        &world_ref,
        "project",
        Some(&project_id),
        &wiki_source,
        &requested,
    );
    wiki_source_result(action, &world_ref, &requested, outcome)
}

fn text_input(name: &str) -> ActionInputDefinition {
    ActionInputDefinition {
        name: name.to_owned(),
        input_type: "string".to_owned(),
        required: true,
        choices: None,
        selection: None,
    }
}

fn wiki_descriptor(
    id: &str,
    title: &str,
    description: &str,
    inputs: Vec<ActionInputDefinition>,
) -> ActionDescriptor {
    wiki_descriptor_with_output(id, title, description, inputs, "central-wiki-reading")
}

fn wiki_descriptor_with_output(
    id: &str,
    title: &str,
    description: &str,
    inputs: Vec<ActionInputDefinition>,
    output_type: &str,
) -> ActionDescriptor {
    ActionDescriptor {
        id: id.to_owned(),
        title: title.to_owned(),
        description: description.to_owned(),
        inputs,
        output: ActionOutputDefinition {
            output_type: output_type.to_owned(),
        },
        mutation_class: MutationClass::ReadOnly,
        preview_supported: false,
        required_ports: Vec::new(),
        availability: ActionAvailability {
            available: true,
            reason: None,
        },
    }
}

const ROOT_SOURCE_READ_DESCRIPTION: &str = "Read one corpus source (central:source:corpus:<id>) cited by the root register Agent Wiki from its source bank (the wiki.sources/ corpus-NNN.json shards beside Control/agents/wiki/wiki.json) as central.wiki-source-reading/v1: the record's own revision, title, media type, visibility, tags, verbatim metadata and exact body, with the wiki ref, the bank shard ref and the shard's Central content revision. Visibility is returned verbatim and not enforced here; consumers such as publication decide audience. Read-only and records no use. A source_ref absent from the bank is refused as invalid input; a missing or malformed bank is an explicit absent/unreadable/invalid state.";

const PROJECT_SOURCE_READ_DESCRIPTION: &str = "Read one corpus source (central:source:corpus:<id>) cited by a Project's Agent Wiki from its source bank (the wiki.sources/ corpus-NNN.json shards beside the manifest-declared ProjectCentral/agents/wiki/wiki.json) as central.wiki-source-reading/v1: the record's own revision, title, media type, visibility, tags, verbatim metadata and exact body, with the wiki ref, the bank shard ref and the shard's Central content revision. Corpus sources are not participating World sources, so projectcentral.source.read cannot serve them; this is their native reader. Visibility is returned verbatim and not enforced here; consumers such as publication decide audience. Read-only and records no use. A source_ref absent from the bank is refused as invalid input; a missing or malformed bank is an explicit absent/unreadable/invalid state.";

/// Register the root-register wiki read Actions (`central.wiki.read` and
/// `central.wiki.source.read`) on a core registry, alongside the other
/// `central.*` owner Actions.
pub fn register_central_wiki_read_action(registry: &mut ActionRegistry) {
    registry
        .register(
            wiki_descriptor(
                "central.wiki.read",
                "Read Central root wiki",
                "Read the root register Agent Wiki source (Control/agents/wiki/wiki.json, okf-wiki/v1) into Central's canonical structural read model: spaces, nodes, U0.2 source refs, relation rows derived at read time, and owner counts. Read-only; the wiki is agent-maintained and this Action never writes it. A missing or corrupt wiki source is an explicit absent/unreadable/invalid state, never an empty success.",
                Vec::new(),
            ),
            central_wiki_read_action,
        )
        .expect("core Action ids are valid");
    registry
        .register(
            wiki_descriptor_with_output(
                "central.wiki.source.read",
                "Read Central root wiki source bank",
                ROOT_SOURCE_READ_DESCRIPTION,
                vec![text_input("source_ref")],
                "central-wiki-source-reading",
            ),
            central_wiki_source_read_action,
        )
        .expect("core Action ids are valid");
}

/// Register the project-register wiki read Actions (`projectcentral.wiki.read`
/// and `projectcentral.wiki.source.read`).
pub fn register_projectcentral_wiki_read_action(registry: &mut ActionRegistry) {
    registry
        .register(
            wiki_descriptor(
                "projectcentral.wiki.read",
                "Read Project wiki",
                "Read one Project's canonical Agent Wiki source (ProjectCentral/agents/wiki/wiki.json as declared by the manifest, okf-wiki/v1) into Central's canonical structural read model: spaces, nodes, U0.2 source refs, relation rows derived at read time, and owner counts. The disclosed source ref round-trips through projectcentral.source.read. Read-only; the wiki is agent-maintained and this Action never writes it. A missing or corrupt wiki source is an explicit absent/unreadable/invalid state, never an empty success.",
                vec![text_input("project")],
            ),
            projectcentral_wiki_read_action,
        )
        .expect("core Action ids are valid");
    registry
        .register(
            wiki_descriptor_with_output(
                "projectcentral.wiki.source.read",
                "Read Project wiki source bank",
                PROJECT_SOURCE_READ_DESCRIPTION,
                vec![text_input("project"), text_input("source_ref")],
                "central-wiki-source-reading",
            ),
            projectcentral_wiki_source_read_action,
        )
        .expect("core Action ids are valid");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::create_core_action_registry;
    use crate::projectcentral_ops::initialize_projectcentral;
    use crate::tempdir;
    use central_connector_sdk::{ConnectorContext, ConnectorRegistry};
    use serde_json::json;
    use std::fs;

    const ROOT_FIXTURE: &str = r#"{"objects":[
        {"profile":"okf-wiki/v1","object":"space","ref":"central:wiki:root","revision":9,
         "title":"Central","parent_space_refs":[],"node_refs":["wiki:node:root/one","wiki:node:root/two"],
         "child_space_refs":["central:wiki:project:example/project"],"anchor_ref":"wiki:node:root/one","provenance":[]},
        {"profile":"okf-wiki/v1","object":"node","ref":"wiki:node:root/one","revision":3,
         "title":"One","type":"identity","space_refs":["central:wiki:root"],
         "source_refs":["Control/user/identity.md"],
         "provenance":[{"source_ref":"Control/user/identity.md"}]},
        {"profile":"okf-wiki/v1","object":"node","ref":"wiki:node:root/two","revision":1,
         "title":"Two","space_refs":["central:wiki:root"],"source_refs":[],"provenance":[]}
    ]}"#;

    const PROJECT_FIXTURE: &str = r#"{"objects":[
        {"profile":"okf-wiki/v1","object":"space","ref":"central:wiki:project:example/project","revision":4,
         "title":"Example","parent_space_refs":["central:wiki:root"],
         "child_space_refs":["central:wiki:project:example/child"],
         "node_refs":["wiki:node:example/alpha","wiki:node:example/beta"],
         "anchor_ref":"wiki:node:example/alpha","provenance":[]},
        {"profile":"okf-wiki/v1","object":"node","ref":"wiki:node:example/alpha","revision":2,
         "title":"Alpha","type":"learning","space_refs":["central:wiki:project:example/project"],
         "source_refs":["ProjectCentral/user/notes/alpha.md"],
         "ql":{"face":"direct","position":3,"unit":"documentation"},
         "provenance":[{"source_ref":"ProjectCentral/user/notes/alpha.md"}]},
        {"profile":"okf-wiki/v1","object":"node","ref":"wiki:node:example/beta","revision":1,
         "title":"Beta","space_refs":["central:wiki:project:example/project"],
         "source_refs":[],"provenance":[]}
    ]}"#;

    #[test]
    fn typed_knowledge_edges_are_read_verbatim_beside_structural_relations() {
        let temp = tempfile::tempdir().unwrap();
        let wiki = r#"{"objects":[
            {"profile":"okf-wiki/v1","object":"node","ref":"wiki:node:a","revision":1,"title":"A","space_refs":[],"source_refs":[],"provenance":[]},
            {"profile":"okf-wiki/v1","object":"node","ref":"wiki:node:b","revision":1,"title":"B","space_refs":[],"source_refs":[],"provenance":[]},
            {"profile":"okf-wiki/v1","object":"edge","ref":"wiki:node:b|re-sites|wiki:node:a","from_ref":"wiki:node:b","relation":"re-sites","to_ref":"wiki:node:a",
             "origin":"inferred","origin_ref":"contribution:x","revision":1,"provenance":[{"source_ref":"central:source:corpus:b"}]},
            {"profile":"okf-wiki/v1","object":"edge","ref":"broken","relation":"references"}
        ]}"#;
        fs::create_dir_all(temp.path().join("w")).unwrap();
        fs::write(temp.path().join("w/wiki.json"), wiki).unwrap();
        let reading = read_wiki(temp.path(), "control:root", "root", None, "w/wiki.json").unwrap();
        assert_eq!(reading.knowledge_edges.len(), 1);
        let edge = &reading.knowledge_edges[0];
        assert_eq!(
            (
                edge.from_ref.as_str(),
                edge.relation.as_str(),
                edge.to_ref.as_str()
            ),
            ("wiki:node:b", "re-sites", "wiki:node:a")
        );
        assert_eq!(edge.origin.as_deref(), Some("inferred"));
        assert_eq!(edge.origin_ref.as_deref(), Some("contribution:x"));
        assert_eq!(reading.counts.knowledge_edges, 1);
        assert_eq!(
            reading.counts.other_objects, 1,
            "an edge without endpoints stays other"
        );
        assert_eq!(reading.counts.objects, 4);
        assert!(
            reading
                .relations
                .iter()
                .all(|relation| relation.kind != "re-sites"),
            "structural relations are unchanged"
        );
    }

    fn fixture_central(temp: &Path, name: &str) -> PathBuf {
        let central = temp.join(name);
        fs::create_dir_all(central.join("Control/agents/wiki")).unwrap();
        fs::write(central.join(ROOT_WIKI_SOURCE), ROOT_FIXTURE).unwrap();
        central
    }

    fn context_for(
        central: &Path,
    ) -> (
        crate::root::RootOptions,
        ConnectorRegistry,
        ConnectorContext,
    ) {
        (
            crate::root::RootOptions {
                explicit_root: Some(central.to_path_buf()),
                configured_root: None,
                home: None,
            },
            ConnectorRegistry::default(),
            ConnectorContext {
                platform: "test".into(),
            },
        )
    }

    fn registry_with_projectcentral_actions() -> ActionRegistry {
        let mut registry = create_core_action_registry();
        crate::projectcentral_ops::register_projectcentral_actions(&mut registry);
        registry
    }

    /// Root fixture hand count: 1 space, 2 nodes, 0 other. Relations:
    /// 1 space-child-space + 2 space-node + 2 node-space + 1 node-source = 6.
    #[test]
    fn root_wiki_reading_matches_hand_counted_fixture() {
        let temp = tempdir().unwrap();
        let central = fixture_central(temp.path(), "root-counts");
        let (options, connectors, connector_context) = context_for(&central);
        let context = ActionExecutionContext {
            root_options: &options,
            connectors: &connectors,
            connector_context: &connector_context,
        };
        let registry = registry_with_projectcentral_actions();
        let result = registry.execute("central.wiki.read", &json!({}), &context);
        assert!(result.ok, "{result:?}");
        let data = result.data.unwrap();
        assert_eq!(data["schema"], WIKI_READING_SCHEMA);
        assert_eq!(data["register"], "root");
        assert_eq!(data["world_ref"], "control:root");
        assert_eq!(data["profile"], WIKI_PROFILE);
        assert_eq!(data["counts"]["spaces"], 1);
        assert_eq!(data["counts"]["nodes"], 2);
        assert_eq!(data["counts"]["objects"], 3);
        assert_eq!(data["counts"]["other_objects"], 0);
        assert_eq!(data["counts"]["edges"], 6);
        assert_eq!(data["spaces"][0]["ref"], "central:wiki:root");
        assert_eq!(data["nodes"][0]["ref"], "wiki:node:root/one");
        assert_eq!(
            data["source"]["ref"],
            "central:source:control:root:Control/agents/wiki/wiki.json"
        );
        assert_eq!(data["source"]["path"], ROOT_WIKI_SOURCE);
        assert!(!data["automatic_agent_or_model_invocation"]
            .as_bool()
            .unwrap());
    }

    /// Project fixture hand count: 1 space, 2 nodes, 0 other. Relations:
    /// 1 space-child-space + 2 space-node + 2 node-space + 1 node-source = 6.
    #[test]
    fn project_wiki_reading_matches_hand_counted_fixture() {
        let temp = tempdir().unwrap();
        let central = temp.path().join("project-counts");
        let project = central.join("Work/example");
        fs::create_dir_all(&project).unwrap();
        initialize_projectcentral(&central, &project, "example/project").unwrap();
        fs::write(
            project.join(crate::projectcentral::WIKI_SOURCE),
            PROJECT_FIXTURE,
        )
        .unwrap();

        let (options, connectors, connector_context) = context_for(&central);
        let context = ActionExecutionContext {
            root_options: &options,
            connectors: &connectors,
            connector_context: &connector_context,
        };
        let registry = registry_with_projectcentral_actions();
        let result = registry.execute(
            "projectcentral.wiki.read",
            &json!({"project": "example"}),
            &context,
        );
        assert!(result.ok, "{result:?}");
        let data = result.data.unwrap();
        assert_eq!(data["register"], "project");
        assert_eq!(data["project"], "example/project");
        assert_eq!(data["world_ref"], "project:example/project");
        assert_eq!(data["counts"]["spaces"], 1);
        assert_eq!(data["counts"]["nodes"], 2);
        assert_eq!(data["counts"]["edges"], 6);
        assert_eq!(data["spaces"][0]["anchor_ref"], "wiki:node:example/alpha");
        assert_eq!(
            data["nodes"][0]["ql"],
            json!({"face": "direct", "position": 3, "unit": "documentation"})
        );
        assert_eq!(
            data["source"]["ref"],
            "central:source:project:example/project:ProjectCentral/agents/wiki/wiki.json"
        );
    }

    /// Every disclosed source ref is U0.2 grammar and resolves through the
    /// owner source Actions: project refs round-trip through
    /// projectcentral.source.read; the root ref is a participating Control
    /// source under the same grammar (control-side payload reading is not an
    /// owner Action today — recorded in the cell receipt).
    #[test]
    fn disclosed_refs_are_u02_and_round_trip_through_owner_source_read() {
        let temp = tempdir().unwrap();
        let central = temp.path().join("round-trip");
        let project = central.join("Work/example");
        fs::create_dir_all(&project).unwrap();
        initialize_projectcentral(&central, &project, "example/project").unwrap();
        fs::write(
            project.join(crate::projectcentral::WIKI_SOURCE),
            PROJECT_FIXTURE,
        )
        .unwrap();

        let (options, connectors, connector_context) = context_for(&central);
        let context = ActionExecutionContext {
            root_options: &options,
            connectors: &connectors,
            connector_context: &connector_context,
        };
        let registry = registry_with_projectcentral_actions();

        let reading = registry
            .execute(
                "projectcentral.wiki.read",
                &json!({"project": "example"}),
                &context,
            )
            .data
            .unwrap();
        let source_ref = reading["source"]["ref"].as_str().unwrap().to_owned();
        assert!(source_ref.starts_with("central:source:project:example/project:"));

        let source = registry.execute(
            "projectcentral.source.read",
            &json!({"project": "example", "source_ref": source_ref}),
            &context,
        );
        assert!(source.ok, "{source:?}");
        let source = source.data.unwrap();
        assert_eq!(source["source"]["ref"], source_ref);
        assert_eq!(
            source["revision"]["revision"],
            reading["source"]["revision"]
        );
        let content: Value = serde_json::from_str(source["content"].as_str().unwrap()).unwrap();
        assert_eq!(content["objects"].as_array().unwrap().len(), 3);

        let root_reading = registry
            .execute("central.wiki.read", &json!({}), &context)
            .data
            .unwrap();
        let root_ref = root_reading["source"]["ref"].as_str().unwrap().to_owned();
        assert_eq!(
            root_ref,
            "central:source:control:root:Control/agents/wiki/wiki.json"
        );
        let bindings = crate::source_horizon::control_source_bindings(&central).unwrap();
        let binding = bindings
            .iter()
            .find(|binding| binding.source_ref == root_ref)
            .unwrap_or_else(|| {
                panic!("root wiki ref is a participating Control source: {root_ref}")
            });
        assert_eq!(binding.path, ROOT_WIKI_SOURCE);
    }

    #[test]
    fn missing_root_wiki_is_an_explicit_absent_state() {
        let temp = tempdir().unwrap();
        let central = temp.path().join("absent");
        fs::create_dir_all(central.join("Control/agents")).unwrap();
        let (options, connectors, connector_context) = context_for(&central);
        let context = ActionExecutionContext {
            root_options: &options,
            connectors: &connectors,
            connector_context: &connector_context,
        };
        let registry = registry_with_projectcentral_actions();
        let result = registry.execute("central.wiki.read", &json!({}), &context);
        assert!(!result.ok);
        assert_eq!(result.status, ResultStatus::InvalidCentralStructure);
        let details = result.error.unwrap().details.unwrap();
        assert_eq!(details["state"], "absent");
        assert_eq!(details["path"], ROOT_WIKI_SOURCE);
    }

    #[test]
    fn corrupt_project_wiki_is_an_explicit_invalid_state() {
        let temp = tempdir().unwrap();
        let central = temp.path().join("corrupt");
        let project = central.join("Work/example");
        fs::create_dir_all(&project).unwrap();
        initialize_projectcentral(&central, &project, "example/project").unwrap();
        fs::write(
            project.join(crate::projectcentral::WIKI_SOURCE),
            "not json {{{",
        )
        .unwrap();

        let (options, connectors, connector_context) = context_for(&central);
        let context = ActionExecutionContext {
            root_options: &options,
            connectors: &connectors,
            connector_context: &connector_context,
        };
        let registry = registry_with_projectcentral_actions();
        let result = registry.execute(
            "projectcentral.wiki.read",
            &json!({"project": "example"}),
            &context,
        );
        assert!(!result.ok);
        assert_eq!(result.status, ResultStatus::InvalidCentralStructure);
        let details = result.error.unwrap().details.unwrap();
        assert_eq!(details["state"], "invalid");
        assert_eq!(
            details["source_ref"],
            "central:source:project:example/project:ProjectCentral/agents/wiki/wiki.json"
        );
    }

    const BANK_SHARD: &str = r#"[
      {"binding":{"source":"central:source:corpus:A03","revision":"1111111111111111",
        "title":"A03","tags":[],"visibility":"team","owners":[],"media_type":"text/markdown",
        "locator":{"kind":"path","value":"A03.md"},"metadata":{}},
       "body":"A03 body"},
      {"binding":{"source":"central:source:control:root:live","revision":"stale",
        "title":"Copied live source","tags":[],"visibility":"team","owners":[],
        "media_type":"text/markdown","locator":{"kind":"path","value":"live.md"},"metadata":{}},
       "body":"A stale copy must not bypass its owner."}
    ]"#;

    const BANK_SHARD_WITH_A04: &str = r#"[
      {"binding":{"source":"central:source:corpus:A04","revision":"4d3e93cce1d87aa1",
        "title":"A04 — Diaphaneity / Contextual Transparency",
        "tags":["source-bank/record","source-bank/gebser"],"visibility":"team","owners":[],
        "media_type":"text/markdown",
        "locator":{"kind":"path","value":"section-rooms/arguments/A04.md"},
        "metadata":{"claim_status":"Argued","corpus_kind":"record","record_type":"argument"}},
       "body":"---\ntitle: A04\n---\n\n# Diaphaneity\n\nThe exact text.\n"}
    ]"#;

    fn project_with_bank(label: &str) -> (crate::TempDir, PathBuf, PathBuf) {
        let temp = tempdir().unwrap();
        let central = temp.path().join(label);
        let project = central.join("Work/example");
        fs::create_dir_all(&project).unwrap();
        initialize_projectcentral(&central, &project, "example/project").unwrap();
        let bank = project.join("ProjectCentral/agents/wiki/wiki.sources");
        fs::create_dir_all(&bank).unwrap();
        fs::write(bank.join("corpus-000.json"), BANK_SHARD).unwrap();
        fs::write(bank.join("corpus-001.json"), BANK_SHARD_WITH_A04).unwrap();
        fs::write(bank.join("notes.json"), "not a shard").unwrap();
        (temp, central, project)
    }

    #[test]
    fn project_wiki_source_read_returns_the_exact_banked_record() {
        let (_temp, central, _project) = project_with_bank("bank-read");
        let (options, connectors, connector_context) = context_for(&central);
        let context = ActionExecutionContext {
            root_options: &options,
            connectors: &connectors,
            connector_context: &connector_context,
        };
        let registry = registry_with_projectcentral_actions();
        let result = registry.execute(
            "projectcentral.wiki.source.read",
            &json!({"project": "example", "source_ref": "central:source:corpus:A04"}),
            &context,
        );
        assert!(result.ok, "{result:?}");
        let data = result.data.unwrap();
        assert_eq!(data["schema"], WIKI_SOURCE_READING_SCHEMA);
        assert_eq!(data["register"], "project");
        assert_eq!(data["project"], "example/project");
        assert_eq!(data["source_ref"], "central:source:corpus:A04");
        assert_eq!(data["revision"], "4d3e93cce1d87aa1");
        assert_eq!(data["title"], "A04 — Diaphaneity / Contextual Transparency");
        assert_eq!(data["media_type"], "text/markdown");
        assert_eq!(data["visibility"], "team");
        assert_eq!(
            data["tags"],
            json!(["source-bank/record", "source-bank/gebser"])
        );
        assert_eq!(
            data["metadata"],
            json!({"claim_status": "Argued", "corpus_kind": "record", "record_type": "argument"})
        );
        assert_eq!(
            data["body"],
            "---\ntitle: A04\n---\n\n# Diaphaneity\n\nThe exact text.\n"
        );
        assert_eq!(
            data["wiki_ref"],
            "central:source:project:example/project:ProjectCentral/agents/wiki/wiki.json"
        );
        assert_eq!(
            data["bank_ref"],
            "central:source:project:example/project:ProjectCentral/agents/wiki/wiki.sources/corpus-001.json"
        );
        assert_eq!(
            data["bank_revision"],
            crate::source_safety::content_revision_bytes(BANK_SHARD_WITH_A04.as_bytes())
        );
        assert!(!data["automatic_agent_or_model_invocation"]
            .as_bool()
            .unwrap());
    }

    #[test]
    fn unknown_or_owner_held_refs_are_refused_as_not_in_the_bank() {
        let (_temp, central, _project) = project_with_bank("bank-refuse");
        let (options, connectors, connector_context) = context_for(&central);
        let context = ActionExecutionContext {
            root_options: &options,
            connectors: &connectors,
            connector_context: &connector_context,
        };
        let registry = registry_with_projectcentral_actions();
        for source_ref in [
            "central:source:corpus:A99",
            "central:source:control:root:live",
        ] {
            let result = registry.execute(
                "projectcentral.wiki.source.read",
                &json!({"project": "example", "source_ref": source_ref}),
                &context,
            );
            assert!(!result.ok);
            assert_eq!(result.status, ResultStatus::InvalidInput);
            let error = result.error.unwrap();
            assert!(
                error
                    .message
                    .starts_with("source_ref is not in this wiki's source bank"),
                "{}",
                error.message
            );
            assert_eq!(error.details.unwrap()["source_ref"], source_ref);
        }
        let missing = registry.execute(
            "projectcentral.wiki.source.read",
            &json!({"project": "example"}),
            &context,
        );
        assert_eq!(missing.status, ResultStatus::InvalidInput);
    }

    #[test]
    fn missing_source_bank_is_an_explicit_absent_state() {
        let temp = tempdir().unwrap();
        let central = temp.path().join("bank-absent");
        let project = central.join("Work/example");
        fs::create_dir_all(&project).unwrap();
        initialize_projectcentral(&central, &project, "example/project").unwrap();
        fs::create_dir_all(central.join("Control/agents/wiki")).unwrap();
        let (options, connectors, connector_context) = context_for(&central);
        let context = ActionExecutionContext {
            root_options: &options,
            connectors: &connectors,
            connector_context: &connector_context,
        };
        let registry = registry_with_projectcentral_actions();
        let result = registry.execute(
            "projectcentral.wiki.source.read",
            &json!({"project": "example", "source_ref": "central:source:corpus:A04"}),
            &context,
        );
        assert!(!result.ok);
        assert_eq!(result.status, ResultStatus::InvalidCentralStructure);
        let details = result.error.unwrap().details.unwrap();
        assert_eq!(details["state"], "absent");
        assert_eq!(details["path"], "ProjectCentral/agents/wiki/wiki.sources");
        assert_eq!(
            details["source_ref"],
            "central:source:project:example/project:ProjectCentral/agents/wiki/wiki.sources"
        );

        let root = registry.execute(
            "central.wiki.source.read",
            &json!({"source_ref": "central:source:corpus:A04"}),
            &context,
        );
        assert_eq!(root.status, ResultStatus::InvalidCentralStructure);
        assert_eq!(
            root.error.unwrap().details.unwrap()["path"],
            "Control/agents/wiki/wiki.sources"
        );
    }

    #[test]
    fn root_wiki_source_read_shares_the_bank_reader() {
        let temp = tempdir().unwrap();
        let central = fixture_central(temp.path(), "root-bank");
        let bank = central.join("Control/agents/wiki/wiki.sources");
        fs::create_dir_all(&bank).unwrap();
        fs::write(bank.join("corpus-000.json"), BANK_SHARD_WITH_A04).unwrap();
        let (options, connectors, connector_context) = context_for(&central);
        let context = ActionExecutionContext {
            root_options: &options,
            connectors: &connectors,
            connector_context: &connector_context,
        };
        let registry = registry_with_projectcentral_actions();
        let result = registry.execute(
            "central.wiki.source.read",
            &json!({"source_ref": "central:source:corpus:A04"}),
            &context,
        );
        assert!(result.ok, "{result:?}");
        let data = result.data.unwrap();
        assert_eq!(data["register"], "root");
        assert_eq!(data["world_ref"], "control:root");
        assert_eq!(data["revision"], "4d3e93cce1d87aa1");
        assert_eq!(
            data["bank_ref"],
            "central:source:control:root:Control/agents/wiki/wiki.sources/corpus-000.json"
        );
    }

    #[test]
    fn source_bank_is_derived_beside_the_declared_wiki_source() {
        assert_eq!(
            source_bank_path("ProjectCentral/agents/wiki/wiki.json"),
            "ProjectCentral/agents/wiki/wiki.sources"
        );
        assert_eq!(source_bank_path("wiki.json"), "wiki.sources");
    }

    #[test]
    fn both_actions_are_disclosed_through_the_registry() {
        let registry = registry_with_projectcentral_actions();
        let central = registry
            .get("central.wiki.read")
            .expect("central.wiki.read is registered");
        assert_eq!(central.mutation_class, MutationClass::ReadOnly);
        assert!(central.availability.available);
        let project = registry
            .get("projectcentral.wiki.read")
            .expect("projectcentral.wiki.read is registered");
        assert_eq!(project.mutation_class, MutationClass::ReadOnly);
        assert_eq!(project.inputs.len(), 1);
        assert_eq!(project.inputs[0].name, "project");
        assert!(project.availability.available);
        for (id, inputs) in [
            ("central.wiki.source.read", vec!["source_ref"]),
            (
                "projectcentral.wiki.source.read",
                vec!["project", "source_ref"],
            ),
        ] {
            let descriptor = registry
                .get(id)
                .unwrap_or_else(|| panic!("{id} is registered"));
            assert_eq!(descriptor.mutation_class, MutationClass::ReadOnly);
            assert_eq!(
                descriptor
                    .inputs
                    .iter()
                    .map(|input| input.name.as_str())
                    .collect::<Vec<_>>(),
                inputs
            );
            assert!(descriptor.description.contains("not enforced"));
        }
    }
}
