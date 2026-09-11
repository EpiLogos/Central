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
use crate::projectcentral::{
    read_project_manifest, ProjectCentralManifest, ROOT_WIKI_SOURCE, WIKI_PROFILE,
};
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

/// Owner counts derived at read time: node/edge totals plus the raw object
/// census so a consumer can detect a wiki whose shape it does not know.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WikiCounts {
    pub spaces: usize,
    pub nodes: usize,
    pub edges: usize,
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
            _ => parsed.push(WikiReadingObject::Other),
        }
    }
    Ok(parsed)
}

enum WikiReadingObject {
    Space(WikiSpaceReading),
    Node(WikiNodeReading),
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
    let revision = crate::projectcentral_flow::content_revision_bytes(content.as_bytes());
    let parsed = parse_wiki_document(&content)?;
    let mut spaces = Vec::new();
    let mut nodes = Vec::new();
    let mut other_objects = 0usize;
    for object in parsed {
        match object {
            WikiReadingObject::Space(space) => spaces.push(space),
            WikiReadingObject::Node(node) => nodes.push(node),
            WikiReadingObject::Other => other_objects += 1,
        }
    }
    let relations = derive_relations(&spaces, &nodes);
    let counts = WikiCounts {
        spaces: spaces.len(),
        nodes: nodes.len(),
        edges: relations.len(),
        objects: spaces.len() + nodes.len() + other_objects,
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
    let world_ref = format!("project:{}", manifest.project_id);
    read_wiki(
        project_root,
        &world_ref,
        "project",
        Some(&manifest.project_id),
        &manifest.wiki.source,
    )
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
        crate::projectcentral_flow::reject_symlink_components(&root.path, Path::new("Control"))
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
    let project = crate::projectcentral_flow::relative_member(&project).map_err(|error| {
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
    crate::projectcentral_flow::reject_symlink_components(&root, &Path::new("Work").join(&project))
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
    ActionDescriptor {
        id: id.to_owned(),
        title: title.to_owned(),
        description: description.to_owned(),
        inputs,
        output: ActionOutputDefinition {
            output_type: "central-wiki-reading".to_owned(),
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

/// Register the root-register wiki read Action (`central.wiki.read`) on a core
/// registry, alongside the other `central.*` owner Actions.
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
}

/// Register the project-register wiki read Action (`projectcentral.wiki.read`).
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
    }
}
