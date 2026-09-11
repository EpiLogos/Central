//! Persistent Central/ProjectCentral file maps. bkmr owns its records and search;
//! existing source relations own source identity, locations and managed links.
use crate::projectcentral_flow::{content_revision_bytes, reject_symlink_components};
use crate::source_horizon::{self, SourceBinding};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs, io,
    path::{Component, Path, PathBuf},
};

pub const SCHEMA: &str = "central.file-map/v1";
const INDEX: &str = ".central/bkmr/bindings.json";
const TEXT_LIMIT: usize = 32768;
pub(crate) const MAX_ENTRIES: usize = 10000;
#[derive(Clone, Debug)]
pub(crate) struct Scope {
    pub root: PathBuf,
    pub world: String,
    pub project: Option<String>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub(crate) struct MapGround {
    #[serde(default)]
    pub resources: BTreeMap<String, Resource>,
    #[serde(default)]
    pub links: BTreeMap<String, Link>,
    #[serde(default)]
    pub scopes: BTreeMap<String, String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Resource {
    pub path: String,
    #[serde(default)]
    pub external: bool,
    #[serde(default)]
    pub native_import: bool,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub tags: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Link {
    pub source_ref: String,
    pub world_ref: String,
    pub owner: String,
    pub target: String,
    pub device: u64,
    pub inode: u64,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub(crate) struct Index {
    #[serde(default)]
    pub schema: String,
    #[serde(default)]
    pub world_ref: String,
    #[serde(default)]
    pub entries: BTreeMap<String, Indexed>,
    #[serde(default)]
    pub embeddings: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Indexed {
    pub id: i64,
    pub revision: String,
    pub url: String,
    #[serde(default)]
    pub source_path: String,
    pub generated_title: String,
    pub generated_description: String,
    #[serde(default)]
    pub retained_description: Option<String>,
    #[serde(default)]
    pub import_id: Option<i64>,
}
#[derive(Clone, Debug, Serialize)]
pub(crate) struct Entry {
    pub source: SourceBinding,
    pub world_ref: String,
    pub path: PathBuf,
    pub kind: String,
    pub revision: String,
    pub title: String,
    pub tags: Vec<String>,
    pub native_import: bool,
}
pub(crate) fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message.into())
}
pub(crate) fn conflict(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::AlreadyExists, message.into())
}
pub(crate) fn text<'a>(input: &'a Value, key: &str) -> io::Result<&'a str> {
    input[key]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| invalid(format!("Required string: {key}")))
}
pub(crate) fn relative(raw: &str) -> io::Result<&Path> {
    let path = Path::new(raw);
    if raw.is_empty()
        || path.is_absolute()
        || !path.components().all(|c| matches!(c, Component::Normal(_)))
    {
        return Err(invalid("Expected a non-empty contained relative path"));
    }
    Ok(path)
}
pub(crate) fn safe_member(root: &Path, raw: &str, exists: bool) -> io::Result<PathBuf> {
    let relative = relative(raw)?;
    reject_symlink_components(root, relative)?;
    let path = root.join(relative);
    if exists && path.canonicalize()? != path {
        return Err(invalid("Filesystem path redirected"));
    }
    Ok(path)
}
pub(crate) fn safe_directory(root: &Path, path: &Path) -> io::Result<()> {
    let mut relative = PathBuf::new();
    for part in path.components() {
        if !matches!(part, Component::Normal(_)) {
            return Err(invalid("Invalid directory path"));
        }
        relative.push(part);
        reject_symlink_components(root, &relative)?;
        match fs::create_dir(root.join(&relative)) {
            Ok(()) => fs::set_permissions(root.join(&relative), fs::Permissions::from_mode(0o700))?,
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists && root.join(&relative).is_dir() => {
            }
            Err(e) => return Err(e),
        }
    }
    Ok(())
}
pub(crate) fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    crate::file_mutation::atomic_record(path, bytes)
}
pub(crate) fn read_json(path: &Path) -> io::Result<Value> {
    if fs::metadata(path)?.len() > 8 * 1024 * 1024 {
        return Err(invalid("Map JSON exceeds size bound"));
    }
    serde_json::from_slice(&fs::read(path)?)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}
impl Scope {
    pub fn project(root: PathBuf, project: Option<String>) -> io::Result<Self> {
        let manifest = crate::projectcentral::read_project_manifest(&root)?;
        if !manifest.validate().valid {
            return Err(invalid("Invalid ProjectCentral manifest"));
        }
        Ok(Self {
            root,
            world: format!("project:{}", manifest.project_id),
            project,
        })
    }
    pub fn relations_path(&self) -> &'static str {
        if self.world == "control:root" {
            source_horizon::CONTROL_GROUND_RELATIONS_SOURCE
        } else {
            source_horizon::GROUND_RELATIONS_SOURCE
        }
    }
    pub fn document(&self) -> io::Result<Value> {
        let path = safe_member(&self.root, self.relations_path(), false)?;
        if !path.exists() {
            return Ok(Value::Null);
        }
        let doc = read_json(&path)?;
        let schema = if self.world == "control:root" {
            source_horizon::CONTROL_GROUND_RELATIONS_SCHEMA
        } else {
            source_horizon::GROUND_RELATIONS_SCHEMA
        };
        let id = if self.world == "control:root" {
            self.world.as_str()
        } else {
            self.world.strip_prefix("project:").unwrap()
        };
        if doc["schema"] != schema || doc["project_id"] != id || !doc["relations"].is_array() {
            return Err(invalid("Invalid source-relations document"));
        }
        Ok(doc)
    }
    pub fn ground(&self) -> io::Result<MapGround> {
        let doc = self.document()?;
        if doc["file_map"].is_null() {
            return Ok(MapGround::default());
        }
        serde_json::from_value(doc["file_map"].clone())
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }
    pub fn basis(&self) -> io::Result<String> {
        let path = safe_member(&self.root, self.relations_path(), false)?;
        Ok(if path.exists() {
            content_revision_bytes(&fs::read(path)?)
        } else {
            "absent".into()
        })
    }
    pub fn save(&self, ground: &MapGround, mut doc: Value) -> io::Result<()> {
        if doc.is_null() {
            let (schema, id) = if self.world == "control:root" {
                (
                    source_horizon::CONTROL_GROUND_RELATIONS_SCHEMA,
                    self.world.as_str(),
                )
            } else {
                (
                    source_horizon::GROUND_RELATIONS_SCHEMA,
                    self.world.strip_prefix("project:").unwrap(),
                )
            };
            doc = json!({"schema":schema,"project_id":id,"relations":[]});
        }
        doc["file_map"] = serde_json::to_value(ground)?;
        let path = Path::new(self.relations_path());
        safe_directory(&self.root, path.parent().unwrap())?;
        write_atomic(&self.root.join(path), &serde_json::to_vec_pretty(&doc)?)
    }
    pub fn index(&self) -> io::Result<Index> {
        let path = safe_member(&self.root, INDEX, false)?;
        if !path.exists() {
            return Ok(Index {
                schema: SCHEMA.into(),
                world_ref: self.world.clone(),
                ..Default::default()
            });
        }
        let index: Index = serde_json::from_value(read_json(&path)?).map_err(io::Error::other)?;
        if index.schema != SCHEMA || index.world_ref != self.world {
            return Err(invalid("bkmr bindings belong to another World or schema"));
        }
        Ok(index)
    }
    pub fn save_index(&self, index: &Index) -> io::Result<()> {
        write_atomic(&self.root.join(INDEX), &serde_json::to_vec_pretty(index)?)
    }
}
pub(crate) fn scopes(root: &Path) -> io::Result<Vec<Scope>> {
    if root.join("ProjectCentral/project.json").is_file() && !root.join("Control").is_dir() {
        return Ok(vec![Scope::project(root.into(), None)?]);
    }
    let base = Scope {
        root: root.into(),
        world: "control:root".into(),
        project: None,
    };
    let mut candidates = base.ground()?.scopes;
    if root.join("Work").is_dir() {
        safe_member(root, "Work", true)?;
        for child in fs::read_dir(root.join("Work"))? {
            let child = child?;
            if child.file_type()?.is_dir()
                && child.path().join("ProjectCentral/project.json").is_file()
            {
                let name = child
                    .file_name()
                    .into_string()
                    .map_err(|_| invalid("Non-UTF8 Project path"))?;
                candidates
                    .entry(name.clone())
                    .or_insert(format!("Work/{name}"));
            }
        }
    }
    if candidates.len() > 512 {
        return Err(invalid("More than 512 participating scopes"));
    }
    let mut result = vec![base];
    let mut seen = BTreeSet::new();
    for (name, path) in candidates {
        let target = if Path::new(&path).is_absolute() {
            safe_member(Path::new("/"), path.trim_start_matches('/'), true)?
        } else {
            safe_member(root, &path, true)?
        };
        let scope = Scope::project(target, Some(name))?;
        if !seen.insert(scope.world.clone()) {
            return Err(invalid("Duplicate participating Project World identity"));
        }
        result.push(scope);
    }
    Ok(result)
}
pub(crate) fn selected<'a>(scopes: &'a [Scope], input: &Value) -> io::Result<&'a Scope> {
    match input.get("project").and_then(Value::as_str) {
        Some(project) => scopes
            .iter()
            .find(|s| s.project.as_deref() == Some(project) || s.world == project)
            .ok_or_else(|| invalid("Unknown Project map")),
        None => scopes.first().ok_or_else(|| invalid("No scope")),
    }
}
pub(crate) fn entries(scope: &Scope) -> io::Result<Vec<Entry>> {
    let ground = scope.ground()?;
    let native = if scope.world == "control:root" {
        source_horizon::control_source_bindings(&scope.root)?
    } else {
        source_horizon::project_source_bindings(&scope.root)?
    };
    let mut sources: BTreeMap<String, SourceBinding> = native
        .into_iter()
        .map(|s| (s.source_ref.clone(), s))
        .collect();
    for (reference, resource) in &ground.resources {
        sources.entry(reference.clone()).or_insert(SourceBinding {
            source_ref: reference.clone(),
            path: resource.path.clone(),
            roles: vec!["registered-file-map-source".into()],
            provenance: "unresolved".into(),
            standing: "unspecified".into(),
            treatment: "retain-native-in-place".into(),
            agent_retrieval_allowed: true,
        });
    }
    if sources.len() > MAX_ENTRIES {
        return Err(invalid("File map exceeds 10000-source bound"));
    }
    let mut result = Vec::new();
    for (reference, mut source) in sources {
        let spec = ground.resources.get(&reference);
        let external = spec.is_some_and(|s| s.external);
        if let Some(spec) = spec {
            source.path.clone_from(&spec.path);
        }
        let path = if external {
            if !Path::new(&source.path).is_absolute() {
                return Err(invalid("External resource requires absolute path"));
            }
            safe_member(Path::new("/"), source.path.trim_start_matches('/'), false)?
        } else {
            safe_member(&scope.root, &source.path, false)?
        };
        if !path.exists() {
            continue;
        }
        let owner_root = if external {
            Path::new("/")
        } else {
            scope.root.as_path()
        };
        source.agent_retrieval_allowed &= source_horizon::retrieval_allowed(owner_root, &path)
            && !(path.is_dir() && path.join(".no-agent-retrieval").exists());
        if !source.agent_retrieval_allowed {
            continue;
        }
        let metadata = fs::symlink_metadata(&path)?;
        if !(metadata.is_file() || metadata.is_dir()) {
            return Err(invalid("Map supports regular files and directories only"));
        }
        // Enumeration is metadata-only. Hash only refresh inputs and selected
        // search/read candidates, not every source on each query.
        let revision = format!(
            "metadata:{}:{}:{}:{}:{}",
            metadata.dev(),
            metadata.ino(),
            metadata.len(),
            metadata.mtime(),
            metadata.mtime_nsec()
        );
        let title = spec
            .map(|s| s.title.clone())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| source.path.clone());
        result.push(Entry {
            source,
            world_ref: scope.world.clone(),
            path,
            kind: if metadata.is_dir() {
                "directory"
            } else {
                "file"
            }
            .into(),
            revision,
            title,
            tags: spec.map(|s| s.tags.clone()).unwrap_or_default(),
            native_import: spec.is_some_and(|s| s.native_import),
        });
    }
    Ok(result)
}
pub(crate) fn lookup(all: &[Scope], reference: &str) -> io::Result<(Scope, Entry)> {
    for scope in all {
        if let Some(entry) = entries(scope)?
            .into_iter()
            .find(|e| e.source.source_ref == reference)
        {
            return Ok((scope.clone(), with_revision(entry)?));
        }
    }
    Err(io::Error::new(
        io::ErrorKind::NotFound,
        "Source is missing, excluded or outside participating maps",
    ))
}
pub(crate) fn with_revision(mut entry: Entry) -> io::Result<Entry> {
    if entry.kind == "file"
        && fs::metadata(&entry.path)?.len() <= crate::source_safety::MAX_SOURCE as u64
    {
        let relative = entry
            .path
            .strip_prefix("/")
            .map_err(io::Error::other)?
            .to_str()
            .ok_or_else(|| invalid("Non-UTF8 source"))?;
        let mut file = crate::file_mutation::open_native_file(Path::new("/"), relative)?;
        let mut bytes = Vec::new();
        use std::io::Read;
        (&mut file)
            .take((crate::source_safety::MAX_SOURCE + 1) as u64)
            .read_to_end(&mut bytes)?;
        if bytes.len() > crate::source_safety::MAX_SOURCE {
            return Err(conflict("Source grew while reading"));
        }
        entry.revision = content_revision_bytes(&bytes);
    }
    Ok(entry)
}
pub(crate) fn payload(entry: &Entry) -> io::Result<Vec<u8>> {
    if entry.kind != "file" {
        return Err(invalid("A directory has no byte payload"));
    }
    let allowed = || source_horizon::retrieval_allowed(Path::new("/"), &entry.path);
    if !allowed() {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Source retrieval has been revoked",
        ));
    }
    let relative = entry
        .path
        .strip_prefix("/")
        .map_err(io::Error::other)?
        .to_str()
        .ok_or_else(|| invalid("Non-UTF8 resource"))?;
    use std::io::Read;
    let file = crate::file_mutation::open_native_file(Path::new("/"), relative)?;
    let mut bytes = Vec::new();
    file.take((crate::source_safety::MAX_SOURCE + 1) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() > crate::source_safety::MAX_SOURCE {
        return Err(invalid("Source exceeds bounded payload size"));
    }
    if !allowed() {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Source retrieval has been revoked",
        ));
    }
    if content_revision_bytes(&bytes) != entry.revision {
        return Err(conflict("Source changed during read"));
    }
    Ok(bytes)
}
pub(crate) fn content(entry: &Entry) -> io::Result<String> {
    let bytes = payload(entry)?;
    if bytes.contains(&0) {
        return Err(invalid("Source is not text; use content_encoding=base64"));
    }
    String::from_utf8(bytes).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

pub(crate) fn uri(path: &Path) -> String {
    let escaped: String = path
        .to_string_lossy()
        .bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"/-_.~".contains(&b) {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect();
    format!("file://{escaped}")
}
pub(crate) fn marker(reference: &str) -> String {
    format!("central-source-ref:{reference}\n")
}
pub(crate) fn description(entry: &Entry) -> String {
    let body = content(entry).unwrap_or_default();
    let mut end = body.len().min(TEXT_LIMIT);
    while !body.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}{}", marker(&entry.source.source_ref), &body[..end])
}
pub(crate) fn expect_basis(scope: &Scope, input: &Value) -> io::Result<()> {
    let current = scope.basis()?;
    let expected = input["expected_revision"].as_str().unwrap_or("absent");
    if current != expected {
        return Err(conflict(format!(
            "Source-relations revision changed: expected {expected}, current {current}"
        )));
    }
    Ok(())
}

/// Reuse the native World relation store and graph. A malformed declaration is
/// an error, never treated as absence. This is disclosure filtering, not an
/// authentication claim for the calling process.
pub(crate) fn context_exclusions(all: &[Scope], scope: &Scope) -> io::Result<BTreeSet<String>> {
    use crate::agent_set_store::RelationRecordStore;
    use crate::world::{EffectiveSourceState, WorldGraph, WorldRecord, WorldRef};
    let mut graph = WorldGraph::default();
    if let Some(root) = all.iter().find(|s| s.world == "control:root") {
        for record in RelationRecordStore::worlds_at_root(&root.root)
            .load_typed::<WorldRecord>()
            .map_err(io::Error::other)?
        {
            graph.insert(record).map_err(io::Error::other)?;
        }
    }
    if scope.world != "control:root" {
        for record in RelationRecordStore::worlds_in_project(&scope.root)
            .load_typed::<WorldRecord>()
            .map_err(io::Error::other)?
        {
            graph.insert(record).map_err(io::Error::other)?;
        }
    }
    // The World register names the authored project_id, while the source
    // horizon qualifies that id as project:<id>. Prefer the authored name.
    let authored = WorldRef::new(scope.world.strip_prefix("project:").unwrap_or(&scope.world))
        .map_err(io::Error::other)?;
    let qualified = WorldRef::new(scope.world.clone()).map_err(io::Error::other)?;
    let root = WorldRef::new("control:root").map_err(io::Error::other)?;
    let target = if graph.get(&authored).is_some() {
        authored
    } else if graph.get(&qualified).is_some() {
        qualified
    } else if graph.get(&root).is_some() {
        root
    } else {
        return Ok(BTreeSet::new());
    };
    let effective = graph.effective_sources(&target).map_err(io::Error::other)?;
    Ok(effective
        .into_iter()
        .filter(|entry| entry.state == EffectiveSourceState::Excluded)
        .map(|entry| entry.source_ref)
        .collect())
}
pub(crate) fn policy_allows(excluded: &BTreeSet<String>, source_ref: &str) -> bool {
    !excluded.iter().any(|reference| {
        reference == source_ref
            || source_ref
                .strip_prefix(reference)
                .is_some_and(|rest| rest.starts_with('/'))
    })
}
pub(crate) fn context_allows(all: &[Scope], scope: &Scope, source_ref: &str) -> io::Result<bool> {
    Ok(policy_allows(&context_exclusions(all, scope)?, source_ref))
}
