//! Persistent Central/ProjectCentral file maps. bkmr owns its records and search;
//! existing source relations own source identity, locations and managed links.
use crate::source_horizon::{self, SourceBinding};
use crate::source_safety::{content_revision_bytes, reject_symlink_components};
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
pub(crate) const MAX_ENTRIES: usize = 1_000_000;
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
    #[serde(default)]
    pub content_pool: ContentPool,
}
/// A scope that pools its readable content: everything under the root that
/// passes the retrieval membrane becomes a source, git or not. The project
/// wiki sits inside the walk, so every constellation in it is pooled with the
/// same content-hash freshness as any other file.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub(crate) struct ContentPool {
    #[serde(default)]
    pub enabled: bool,
    /// Relative-path subtrees kept out of the pool, matched at component
    /// boundaries ("Seeds" excludes "Seeds/x" but not "Seeds-2/x").
    #[serde(default)]
    pub exclude: Vec<String>,
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
    /// The embedder that produced the stored vectors. A configured model
    /// other than this one means the vectors are wrong-shaped and must be
    /// regenerated before hybrid search can be trusted.
    #[serde(default)]
    pub embedding_model: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Indexed {
    pub id: i64,
    pub revision: String,
    pub url: String,
    #[serde(default)]
    pub source_path: String,
    pub generated_title: String,
    /// Digest of the description last written to bkmr (marker + content).
    /// The text itself lives in bkmr; storing the digest keeps the derived
    /// index compact enough to pool whole trees.
    #[serde(default)]
    pub description_hash: String,
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

/// Operation-local physical evidence, never semantic World identity.
#[derive(Clone, Debug)]
pub(crate) struct ReadRoot {
    requested: PathBuf,
    canonical: PathBuf,
    identity: (u64, u64),
}
impl ReadRoot {
    pub(crate) fn capture(root: &Path) -> io::Result<Self> {
        let requested = if root.is_absolute() {
            root.to_path_buf()
        } else {
            std::env::current_dir()?.join(root)
        };
        let canonical = fs::canonicalize(&requested)?;
        let metadata = fs::symlink_metadata(&canonical)?;
        if !metadata.is_dir() {
            return Err(invalid("Read owner root is not a directory"));
        }
        Ok(Self {
            requested,
            canonical,
            identity: (metadata.dev(), metadata.ino()),
        })
    }
    pub(crate) fn validate(&self) -> io::Result<()> {
        let current = fs::canonicalize(&self.requested)?;
        let metadata = fs::symlink_metadata(&current)?;
        if current != self.canonical
            || !metadata.is_dir()
            || (metadata.dev(), metadata.ino()) != self.identity
        {
            return Err(conflict("Read owner root affiliation changed"));
        }
        Ok(())
    }
}
#[derive(Debug)]
pub(crate) struct ReadFailure {
    ownership: &'static str,
    stage: &'static str,
    material: &'static str,
    message: String,
    cause: Option<io::Error>,
}
impl std::fmt::Display for ReadFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}
impl std::error::Error for ReadFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.cause
            .as_ref()
            .map(|cause| cause as &(dyn std::error::Error + 'static))
    }
}
pub(crate) fn read_failure(
    error: io::Error,
    ownership: &'static str,
    stage: &'static str,
    material: &'static str,
) -> io::Error {
    if error
        .get_ref()
        .is_some_and(|cause| cause.is::<ReadFailure>())
    {
        return error;
    }
    let kind = error.kind();
    let message = if kind == io::ErrorKind::PermissionDenied {
        "Native source observation is unavailable".into()
    } else {
        error.to_string()
    };
    io::Error::new(
        kind,
        ReadFailure {
            ownership,
            stage,
            material,
            message,
            cause: Some(error),
        },
    )
}
pub(crate) fn read_refusal(
    kind: io::ErrorKind,
    message: &str,
    ownership: &'static str,
    stage: &'static str,
    material: &'static str,
) -> io::Error {
    io::Error::new(
        kind,
        ReadFailure {
            ownership,
            stage,
            material,
            message: message.into(),
            cause: None,
        },
    )
}
pub(crate) fn read_failure_details(error: &io::Error) -> Value {
    let failure = error
        .get_ref()
        .and_then(|cause| cause.downcast_ref::<ReadFailure>());
    let cause = failure
        .and_then(|failure| failure.cause.as_ref())
        .unwrap_or(error);
    // The native relation store retains its actual error. Project that typed
    // cause rather than reporting the intermediary io::Error's generic kind.
    let store_error = cause
        .get_ref()
        .and_then(|error| error.downcast_ref::<crate::agent_set_store::RelationRecordStoreError>());
    let capacity = store_error.and_then(|error| match error {
        crate::agent_set_store::RelationRecordStoreError::RecordBudget { byte_len, limit } => {
            Some(json!({"byte_len":byte_len,"limit":limit,"profile":"eager-relation-metadata"}))
        }
        _ => None,
    });
    let semantic_store_refusal = store_error.is_some_and(|error| error.io_error().is_none());
    let cause = store_error
        .and_then(crate::agent_set_store::RelationRecordStoreError::io_error)
        .unwrap_or(cause);
    json!({"ownership":failure.map(|failure| failure.ownership).unwrap_or("unknown"),
    "failure_stage":failure.map(|failure| failure.stage).unwrap_or("owner_metadata"),
    "material_state":failure.map(|failure| failure.material).unwrap_or("unavailable"), "effects":"none",
    "capacity":capacity,
    "io_error":if semantic_store_refusal || failure.is_some_and(|failure| failure.cause.is_none()) { Value::Null } else {
        json!({"kind":format!("{:?}",cause.kind()),"raw_os_error":cause.raw_os_error(),"message":cause.to_string()})
    }})
}
fn metadata_document(root: &Path, member: &str) -> io::Result<(Value, String)> {
    let owner = ReadRoot::capture(root)?;
    let path = safe_member(&owner.canonical, member, false)?;
    match fs::symlink_metadata(&path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            owner.validate()?;
            return Ok((Value::Null, "absent".into()));
        }
        Err(error) => return Err(error),
        Ok(metadata) if !metadata.is_file() => {
            return Err(invalid("Owner metadata must be a regular file"))
        }
        Ok(_) => {}
    }
    let mut reader = crate::file_mutation::NativeFileRead::open(
        &owner.canonical,
        owner.identity,
        relative(member)?,
    )?;
    let bytes = reader.read_metadata_bytes(8 * 1024 * 1024)?;
    owner.validate()?;
    let doc: Value = serde_json::from_slice(&bytes)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    if doc.is_null() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Present native declaration cannot be JSON null",
        ));
    }
    Ok((doc, content_revision_bytes(&bytes)))
}
fn observed_project_manifest(
    root: &Path,
) -> io::Result<(crate::projectcentral::ProjectCentralManifest, String)> {
    let owner = ReadRoot::capture(root)?;
    let mut reader = crate::file_mutation::NativeFileRead::open(
        &owner.canonical,
        owner.identity,
        Path::new("ProjectCentral/project.json"),
    )?;
    let bytes = reader.read_bytes(crate::source_safety::MAX_SOURCE)?;
    owner.validate()?;
    let manifest = crate::projectcentral::parse_project_manifest(
        &bytes,
        &root.join("ProjectCentral/project.json"),
    )?;
    let validation = manifest.validate();
    if !validation.valid {
        return Err(invalid(validation.errors.join("; ")));
    }
    Ok((manifest, content_revision_bytes(&bytes)))
}
fn project_manifest(root: &Path) -> io::Result<crate::projectcentral::ProjectCentralManifest> {
    observed_project_manifest(root).map(|(manifest, _)| manifest)
}
/// Full current native declarations, never a payload or semantic identity.
pub(crate) fn ownership_bases(all: &[Scope]) -> io::Result<Vec<(String, String, Option<String>)>> {
    all.iter()
        .map(|scope| {
            let relation = scope.basis()?;
            let manifest = if scope.world == "control:root" {
                None
            } else {
                let (manifest, basis) = observed_project_manifest(&scope.root)?;
                if format!("project:{}", manifest.project_id) != scope.world {
                    return Err(conflict("Project declaration identity changed"));
                }
                Some(basis)
            };
            Ok((scope.world.clone(), relation, manifest))
        })
        .collect()
}
fn optional_directory(path: &Path) -> io::Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() => Ok(true),
        Ok(_) => Err(invalid(
            "Native scope aperture is not an ordinary directory",
        )),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
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
pub(crate) fn write_atomic(
    root: &Path,
    path: &Path,
    bytes: &[u8],
    disposition: crate::file_mutation::RecordDisposition,
) -> io::Result<()> {
    crate::file_mutation::atomic_record(
        root,
        path.strip_prefix(root).map_err(io::Error::other)?,
        bytes,
        disposition,
    )
}
pub(crate) fn read_json(path: &Path) -> io::Result<Value> {
    if fs::metadata(path)?.len() > 8 * 1024 * 1024 {
        return Err(invalid("Map JSON exceeds size bound"));
    }
    serde_json::from_slice(&fs::read(path)?)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}
/// The derived bindings index is not authored ground: pooled scopes carry a
/// digest per source, so its read bound is sized for a fully pooled world
/// rather than the authored-document bound above.
const INDEX_JSON_BOUND: u64 = 512 * 1024 * 1024;
fn read_index_json(path: &Path) -> io::Result<Value> {
    if fs::metadata(path)?.len() > INDEX_JSON_BOUND {
        return Err(invalid("Bindings index exceeds size bound"));
    }
    serde_json::from_slice(&fs::read(path)?)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}
impl Scope {
    pub fn project(root: PathBuf, project: Option<String>) -> io::Result<Self> {
        let manifest = project_manifest(&root)
            .map_err(|error| read_failure(error, "known", "project_declaration", "unavailable"))?;
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
    pub(crate) fn observed_document(&self) -> io::Result<(Value, String)> {
        let (doc, basis) = metadata_document(&self.root, self.relations_path())?;
        if !doc.is_null() {
            let (schema, id) = if self.world == "control:root" {
                (
                    source_horizon::CONTROL_GROUND_RELATIONS_SCHEMA,
                    self.world.as_str(),
                )
            } else {
                (
                    source_horizon::GROUND_RELATIONS_SCHEMA,
                    self.world
                        .strip_prefix("project:")
                        .ok_or_else(|| invalid("Invalid Project World"))?,
                )
            };
            source_horizon::validate_relations_value(&doc, schema, id)?;
        }
        Ok((doc, basis))
    }
    pub fn document(&self) -> io::Result<Value> {
        self.observed_document().map(|(doc, _)| doc)
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
        self.observed_document().map(|(_, basis)| basis)
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
        write_atomic(
            &self.root,
            &self.root.join(path),
            &serde_json::to_vec_pretty(&doc)?,
            crate::file_mutation::RecordDisposition::ReplaceOrCreate,
        )
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
        let index: Index =
            serde_json::from_value(read_index_json(&path)?).map_err(io::Error::other)?;
        if index.schema != SCHEMA || index.world_ref != self.world {
            return Err(invalid("bkmr bindings belong to another World or schema"));
        }
        Ok(index)
    }
    pub fn save_index(&self, index: &Index) -> io::Result<()> {
        write_atomic(
            &self.root,
            &self.root.join(INDEX),
            &serde_json::to_vec_pretty(index)?,
            crate::file_mutation::RecordDisposition::ReplaceOrCreate,
        )
    }
}
pub(crate) fn scopes(root: &Path) -> io::Result<Vec<Scope>> {
    let manifest = match fs::symlink_metadata(root.join("ProjectCentral/project.json")) {
        Ok(metadata) if metadata.is_file() => true,
        Ok(_) => return Err(invalid("Project manifest is not an ordinary file")),
        Err(error) if error.kind() == io::ErrorKind::NotFound => false,
        Err(error) => return Err(error),
    };
    let control = optional_directory(&root.join("Control"))?;
    if manifest && !control {
        return Ok(vec![Scope::project(root.into(), None)?]);
    }
    if !control {
        return Err(invalid(
            "No native Control or ProjectCentral owner aperture",
        ));
    }
    let base = Scope {
        root: root.into(),
        world: "control:root".into(),
        project: None,
    };
    let mut candidates = base.ground()?.scopes;
    if optional_directory(&root.join("Work"))? {
        safe_member(root, "Work", true)?;
        for child in fs::read_dir(root.join("Work"))? {
            let child = child?;
            if !child.file_type()?.is_dir() {
                continue;
            }
            match fs::symlink_metadata(child.path().join("ProjectCentral/project.json")) {
                Ok(metadata) if metadata.is_file() => {}
                Ok(_) => {
                    return Err(invalid(
                        "Participating Project manifest is not an ordinary file",
                    ))
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
                Err(error) => return Err(error),
            }
            let name = child
                .file_name()
                .into_string()
                .map_err(|_| invalid("Non-UTF8 Project path"))?;
            candidates
                .entry(name.clone())
                .or_insert(format!("Work/{name}"));
        }
    }
    if candidates.len() > 512 {
        return Err(invalid("More than 512 participating scopes"));
    }
    let mut result = vec![base];
    let mut seen = BTreeSet::new();
    for (name, path) in candidates {
        let target = if Path::new(&path).is_absolute() {
            safe_member(Path::new("/"), path.trim_start_matches('/'), true)
        } else {
            safe_member(root, &path, true)
        }
        .map_err(|error| read_failure(error, "known", "owner_root", "unavailable"))?;
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
/// Turn a scope's content pool on or off. An enabled pool means the scope's
/// readable content — the whole tree, wiki and constellations included — is
/// source, without per-file declarations.
pub(crate) fn pool(all: &[Scope], input: &Value) -> io::Result<Value> {
    let scope = selected(all, input)?;
    let enable = input["enable"]
        .as_bool()
        .ok_or_else(|| invalid("pool requires enable: true|false"))?;
    let mut ground = scope.ground()?;
    if let Some(exclude) = input.get("exclude").and_then(Value::as_array) {
        ground.content_pool.exclude = exclude
            .iter()
            .filter_map(|value| value.as_str().map(str::to_owned))
            .collect();
    }
    let mut pooled = 0usize;
    if enable {
        let mut sources: BTreeMap<String, SourceBinding> = BTreeMap::new();
        pooled = source_horizon::insert_tree_bindings(
            &scope.root,
            &scope.root,
            &scope.world,
            &["content-pool"],
            "pooled",
            "scope-content",
            "pooled-content",
            &ground.content_pool.exclude,
            &mut sources,
        )?;
        // Refuse at enable time, with the count and the remedy, rather than
        // poison every later enumeration with an over-bound pool.
        if pooled > MAX_ENTRIES {
            return Err(invalid(format!(
                "pool would add {pooled} sources, over the {MAX_ENTRIES}-source bound; exclude subtrees and retry"
            )));
        }
    }
    ground.content_pool.enabled = enable;
    let doc = scope.document()?;
    scope.save(&ground, doc)?;
    Ok(
        json!({"world_ref": scope.world, "content_pool": {"enabled": enable}, "exclude": ground.content_pool.exclude, "pooled_sources": pooled}),
    )
}

#[derive(Clone, Debug)]
pub(crate) struct BindingCandidate {
    pub scope: Scope,
    pub source: SourceBinding,
    pub resource: Option<Resource>,
    pub relation_revision: String,
    pub selection_metadata_basis: Option<String>,
}
impl BindingCandidate {
    pub(crate) fn path(&self) -> io::Result<PathBuf> {
        let external = self
            .resource
            .as_ref()
            .is_some_and(|resource| resource.external);
        let raw = self
            .resource
            .as_ref()
            .map(|resource| resource.path.as_str())
            .unwrap_or(&self.source.path);
        if external {
            if !Path::new(raw).is_absolute() {
                return Err(invalid("External resource requires absolute path"));
            }
            relative(raw.trim_start_matches('/'))?;
            Ok(PathBuf::from(raw))
        } else {
            relative(raw)?;
            Ok(self.scope.root.join(raw))
        }
    }
    pub(crate) fn observe(&self) -> io::Result<Entry> {
        let path = self
            .path()
            .map_err(|error| read_failure(error, "known", "binding_metadata", "unavailable"))?;
        let root = if self
            .resource
            .as_ref()
            .is_some_and(|resource| resource.external)
        {
            Path::new("/")
        } else {
            self.scope.root.as_path()
        };
        let admitted = source_horizon::retrieval_admission(root, &path)
            .map_err(|error| read_failure(error, "known", "source_admission", "unavailable"))?;
        if !admitted {
            return Err(read_refusal(
                io::ErrorKind::PermissionDenied,
                "Source is excluded by current retrieval treatment",
                "known",
                "source_admission",
                "withheld",
            ));
        }
        let metadata = fs::symlink_metadata(&path).map_err(|error| {
            let state = if error.kind() == io::ErrorKind::NotFound {
                "missing"
            } else {
                "unavailable"
            };
            read_failure(error, "known", "source_metadata", state)
        })?;
        if !(metadata.is_file() || metadata.is_dir()) || metadata.file_type().is_symlink() {
            return Err(read_refusal(
                io::ErrorKind::InvalidInput,
                "Native source form is unsupported",
                "known",
                "source_metadata",
                "unavailable",
            ));
        }
        let mut source = self.source.clone();
        source.agent_retrieval_allowed = true;
        let title = self
            .resource
            .as_ref()
            .map(|resource| resource.title.clone())
            .filter(|title| !title.is_empty())
            .unwrap_or_else(|| source.path.clone());
        Ok(Entry {
            source,
            world_ref: self.scope.world.clone(),
            path,
            kind: if metadata.is_dir() {
                "directory"
            } else {
                "file"
            }
            .into(),
            revision: format!(
                "metadata:{}:{}:{}:{}:{}",
                metadata.dev(),
                metadata.ino(),
                metadata.len(),
                metadata.mtime(),
                metadata.mtime_nsec()
            ),
            title,
            tags: self
                .resource
                .as_ref()
                .map(|resource| resource.tags.clone())
                .unwrap_or_default(),
            native_import: self
                .resource
                .as_ref()
                .is_some_and(|resource| resource.native_import),
        })
    }
}
/// Complete explicit/adopted/resource metadata stays independent of bulk
/// fallback discovery. The selected native owner operation supplies semantics
/// only for a member capable of matching this request.
fn source_candidates(
    scope: &Scope,
    reference: Option<&str>,
    location: Option<&Path>,
) -> io::Result<Vec<BindingCandidate>> {
    let (doc, basis) = scope
        .observed_document()
        .map_err(|error| read_failure(error, "unknown", "declaration_metadata", "unavailable"))?;
    let ground: MapGround = if doc["file_map"].is_null() {
        MapGround::default()
    } else {
        serde_json::from_value(doc["file_map"].clone()).map_err(|error| {
            read_failure(
                io::Error::new(io::ErrorKind::InvalidData, error),
                "unknown",
                "declaration_metadata",
                "unavailable",
            )
        })?
    };
    let manifest = if scope.world == "control:root" {
        None
    } else {
        let manifest = project_manifest(&scope.root)?;
        if format!("project:{}", manifest.project_id) != scope.world {
            return Err(conflict("Project declaration identity changed"));
        }
        Some(manifest)
    };
    let mut declarations = BTreeMap::<String, SourceBinding>::new();
    if let Some(manifest) = &manifest {
        for path in &manifest.wiki.adopted_sources {
            relative(path)?;
            let native = source_horizon::project_binding_for_observed_path(
                manifest,
                path,
                (!doc.is_null()).then_some(&doc),
                None,
                true,
            )?
            .ok_or_else(|| invalid("Adopted source lacks native ownership"))?;
            declarations
                .entry(native.source_ref.clone())
                .or_insert(native);
        }
    }
    if let Some(rows) = doc["relations"].as_array() {
        for row in rows {
            let mut value = row.clone();
            value["agent_retrieval_allowed"] = json!(true);
            let binding: SourceBinding = serde_json::from_value(value)
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
            let key = crate::source_safety::normal_member_key(&binding.path)?;
            let replaced = declarations
                .iter()
                .filter_map(|(reference, current)| {
                    match crate::source_safety::normal_member_key(&current.path) {
                        Ok(current) if current == key => Some(Ok(reference.clone())),
                        Ok(_) => None,
                        Err(error) => Some(Err(error)),
                    }
                })
                .collect::<io::Result<Vec<_>>>()?;
            for reference in replaced {
                declarations.remove(&reference);
            }
            declarations.insert(binding.source_ref.clone(), binding);
        }
    }
    for (reference, resource) in &ground.resources {
        if reference.trim().is_empty() || reference.len() > 4096 {
            return Err(invalid(
                "Registered SourceRef requires bounded non-empty text",
            ));
        }
        let resource_path = if resource.external {
            if !Path::new(&resource.path).is_absolute() {
                return Err(invalid("External resource requires absolute path"));
            }
            relative(resource.path.trim_start_matches('/'))?;
            PathBuf::from(&resource.path)
        } else {
            scope.root.join(relative(&resource.path)?)
        };
        if let Some(declared) = declarations.get(reference) {
            if scope.root.join(&declared.path) != resource_path {
                return Err(conflict(
                    "Registered resource competes with accepted native location",
                ));
            }
        } else {
            declarations.insert(
                reference.clone(),
                SourceBinding {
                    source_ref: reference.clone(),
                    path: resource.path.clone(),
                    roles: vec!["registered-file-map-source".into()],
                    provenance: "unresolved".into(),
                    standing: "unspecified".into(),
                    treatment: "retain-native-in-place".into(),
                    agent_retrieval_allowed: true,
                },
            );
        }
    }
    if declarations.len() > MAX_ENTRIES {
        return Err(invalid("File map exceeds the 1000000-source bound"));
    }
    let mut selected = BTreeMap::<String, BindingCandidate>::new();
    for source in declarations.into_values() {
        let mut candidate = BindingCandidate {
            scope: scope.clone(),
            resource: ground.resources.get(&source.source_ref).cloned(),
            source,
            relation_revision: basis.clone(),
            selection_metadata_basis: None,
        };
        let path = candidate.path()?; // Every declared route must be well-formed.
        if !reference.is_some_and(|reference| reference == candidate.source.source_ref)
            && !location.is_some_and(|location| location == path.as_path())
        {
            continue;
        }
        if let Ok(member) = path.strip_prefix(&scope.root) {
            let member = member
                .to_str()
                .ok_or_else(|| invalid("Native source member is not UTF-8"))?;
            let (native, metadata_basis) =
                selected_native_binding(scope, member, &doc, manifest.as_ref())?;
            if let Some(native) = native {
                if native.source_ref != candidate.source.source_ref {
                    return Err(read_refusal(
                        io::ErrorKind::AlreadyExists,
                        "Registered location competes with native Source identity",
                        "known",
                        "binding_metadata",
                        "unavailable",
                    ));
                }
                candidate.source = native;
                candidate.selection_metadata_basis = metadata_basis;
            }
        }
        selected.insert(candidate.source.source_ref.clone(), candidate);
    }
    let nominated = if let Some(location) = location {
        location
            .strip_prefix(&scope.root)
            .ok()
            .and_then(Path::to_str)
            .map(str::to_owned)
    } else {
        reference.and_then(|reference| nominated_member(scope, reference))
    };
    if let Some(member) = nominated {
        if relative(&member).is_ok() {
            let (native, metadata_basis) =
                selected_native_binding(scope, &member, &doc, manifest.as_ref())?;
            let native = native.or_else(|| {
                if !ground.content_pool.enabled
                    || ground
                        .content_pool
                        .exclude
                        .iter()
                        .any(|excluded| Path::new(&member).starts_with(Path::new(excluded)))
                {
                    return None;
                }
                let normal = crate::source_safety::normal_member_key(&member).ok()?;
                let path = normal.to_str()?.to_owned();
                Some(SourceBinding {
                    source_ref: source_horizon::source_ref(&scope.world, &path),
                    path,
                    roles: vec!["content-pool".into()],
                    provenance: "pooled".into(),
                    standing: "scope-content".into(),
                    treatment: "pooled-content".into(),
                    agent_retrieval_allowed: true,
                })
            });
            if let Some(source) = native {
                if reference.is_none_or(|reference| reference == source.source_ref) {
                    let candidate = BindingCandidate {
                        scope: scope.clone(),
                        resource: ground.resources.get(&source.source_ref).cloned(),
                        source,
                        relation_revision: basis.clone(),
                        selection_metadata_basis: metadata_basis,
                    };
                    if location
                        .is_none_or(|location| candidate.path().is_ok_and(|path| path == location))
                    {
                        selected
                            .entry(candidate.source.source_ref.clone())
                            .or_insert(candidate);
                    }
                }
            }
        }
    }
    if scope.basis()? != basis {
        return Err(conflict(
            "Source declarations changed during ownership observation",
        ));
    }
    Ok(selected.into_values().collect())
}
fn nominated_member(scope: &Scope, reference: &str) -> Option<String> {
    let prefix = format!("central:source:{}:", scope.world);
    reference
        .strip_prefix(&prefix)
        .map(|suffix| {
            suffix
                .replace("%20", " ")
                .replace("%3A", ":")
                .replace("%25", "%")
        })
        .filter(|member| source_horizon::source_ref(&scope.world, member) == reference)
}
fn selected_native_binding(
    scope: &Scope,
    member: &str,
    doc: &Value,
    manifest: Option<&crate::projectcentral::ProjectCentralManifest>,
) -> io::Result<(Option<SourceBinding>, Option<String>)> {
    let member = crate::source_safety::normal_member_key(member)?;
    let member = member
        .to_str()
        .ok_or_else(|| invalid("Native member is not UTF-8"))?;
    let explicitly_bound = doc["relations"].as_array().is_some_and(|rows| {
        rows.iter().any(|row| {
            row["path"].as_str().is_some_and(|path| {
                crate::source_safety::normal_member_key(path)
                    .is_ok_and(|key| key == Path::new(member))
            })
        })
    });
    let skill_path = if explicitly_bound {
        None
    } else if let Some(manifest) = manifest {
        crate::control_skills::project_skill_manifest_path(&manifest.human_source, member)?
    } else {
        crate::control_skills::control_skill_manifest_path(member)?
    };
    let mut selection_basis = None;
    let skill = if let Some(path) = skill_path {
        if !source_horizon::retrieval_admission(&scope.root, &scope.root.join(member))
            .map_err(|error| read_failure(error, "known", "source_admission", "unavailable"))?
        {
            return Err(read_refusal(
                io::ErrorKind::PermissionDenied,
                "Source is excluded by current retrieval treatment",
                "known",
                "source_admission",
                "withheld",
            ));
        }
        let (value, basis) = metadata_document(&scope.root, &path)
            .map_err(|error| read_failure(error, "known", "binding_metadata", "unavailable"))?;
        selection_basis = Some(basis);
        if value.is_null() {
            None
        } else {
            Some(crate::control_skills::parse_skill_manifest(
                &serde_json::to_vec(&value)?,
                &scope.root.join(path),
            )?)
        }
    } else {
        None
    };
    let relations = (!doc.is_null()).then_some(doc);
    let source = if let Some(manifest) = manifest {
        source_horizon::project_binding_for_observed_path(
            manifest,
            member,
            relations,
            skill.as_ref(),
            true,
        )?
    } else {
        source_horizon::control_binding_for_observed_path(member, relations, skill.as_ref(), true)?
    };
    Ok((source, selection_basis))
}
fn unique_candidates(
    all: &[Scope],
    reference: Option<&str>,
    path: Option<&Path>,
) -> io::Result<Option<BindingCandidate>> {
    let mut found = None;
    for scope in all {
        for candidate in source_candidates(scope, reference, path)
            .map_err(|error| read_failure(error, "unknown", "binding_metadata", "unavailable"))?
        {
            if found.is_some() {
                return Err(read_refusal(
                    io::ErrorKind::AlreadyExists,
                    "Source has ambiguous native owners",
                    "known",
                    "binding_metadata",
                    "unavailable",
                ));
            }
            found = Some(candidate);
        }
    }
    Ok(found)
}
pub(crate) fn binding_by_ref(
    all: &[Scope],
    reference: &str,
) -> io::Result<Option<BindingCandidate>> {
    let found = unique_candidates(all, Some(reference), None)?;
    if let Some(candidate) = &found {
        let path = candidate.path()?;
        let location = unique_candidates(all, None, Some(&path))?
            .ok_or_else(|| conflict("Selected native owner disappeared"))?;
        if location.scope.world != candidate.scope.world
            || location.source != candidate.source
            || location.selection_metadata_basis != candidate.selection_metadata_basis
        {
            return Err(conflict("Selected native owner changed during observation"));
        }
    }
    Ok(found)
}
pub(crate) fn binding_by_path(all: &[Scope], path: &Path) -> io::Result<Option<BindingCandidate>> {
    unique_candidates(all, None, Some(path))
}

/// A native spelling nominates a route; only current native participation can
/// classify it. This never emits or registers a new Source binding.
pub(crate) fn require_no_unobserved_route(
    all: &[Scope],
    reference: Option<&str>,
    path: Option<&Path>,
) -> io::Result<()> {
    for scope in all {
        let nominated = if let Some(path) = path {
            path.strip_prefix(&scope.root)
                .ok()
                .and_then(Path::to_str)
                .map(str::to_owned)
        } else if let Some(reference) = reference {
            let prefix = format!("central:source:{}:", scope.world);
            reference
                .strip_prefix(&prefix)
                .map(|suffix| {
                    suffix
                        .replace("%20", " ")
                        .replace("%3A", ":")
                        .replace("%25", "%")
                })
                .filter(|member| source_horizon::source_ref(&scope.world, member) == reference)
        } else {
            None
        };
        let Some(member) = nominated else {
            continue;
        };
        if relative(&member).is_err() {
            continue;
        }
        let (doc, _) = scope.observed_document()?;
        let ground: MapGround = if doc["file_map"].is_null() {
            MapGround::default()
        } else {
            serde_json::from_value(doc["file_map"].clone()).map_err(io::Error::other)?
        };
        let native_home = if scope.world == "control:root" {
            source_horizon::control_binding_for_observed_path(
                &member,
                (!doc.is_null()).then_some(&doc),
                None,
                true,
            )?
            .is_some()
        } else {
            let manifest = project_manifest(&scope.root)?;
            let selected = Path::new(&member);
            [
                Path::new(&manifest.human_source),
                Path::new(crate::projectcentral::AGENT_GOVERNANCE_DIR),
                Path::new(crate::projectcentral::WIKI_DIR),
            ]
            .iter()
            .any(|aperture| selected.starts_with(aperture) && selected != *aperture)
                || manifest
                    .wiki
                    .adopted_sources
                    .iter()
                    .any(|source| Path::new(source) == selected)
        };
        if !native_home && !ground.content_pool.enabled {
            continue;
        }
        let location = scope.root.join(&member);
        for boundary in std::iter::once(scope.root.as_path()).chain(
            all.iter()
                .filter(|owner| owner.world == "control:root" && location.starts_with(&owner.root))
                .map(|owner| owner.root.as_path()),
        ) {
            if !source_horizon::retrieval_admission(boundary, &location)
                .map_err(|error| read_failure(error, "known", "source_admission", "unavailable"))?
            {
                return Err(read_refusal(
                    io::ErrorKind::PermissionDenied,
                    "Native route is excluded by current retrieval treatment",
                    "known",
                    "source_admission",
                    "withheld",
                ));
            }
        }
        let actual = fs::symlink_metadata(&location);
        return match actual {
            Err(error) => {
                let state = if error.kind() == io::ErrorKind::NotFound {
                    "missing"
                } else {
                    "unavailable"
                };
                Err(read_failure(error, "known", "binding_metadata", state))
            }
            Ok(_) => Err(read_refusal(
                io::ErrorKind::InvalidInput,
                "Native participation cannot be completely observed by this ownership route",
                "known",
                "binding_metadata",
                "unavailable",
            )),
        };
    }
    Ok(())
}
fn held_source_bytes(entry: &Entry) -> io::Result<Vec<u8>> {
    let root = ReadRoot::capture(Path::new("/"))?;
    let member = entry.path.strip_prefix("/").map_err(io::Error::other)?;
    let mut reader =
        crate::file_mutation::NativeFileRead::open(&root.canonical, root.identity, member)?;
    let bytes = reader.read_bytes(crate::source_safety::MAX_SOURCE)?;
    root.validate()?;
    Ok(bytes)
}

pub(crate) fn metadata_basis(entry: &Entry) -> io::Result<Value> {
    let metadata = fs::symlink_metadata(&entry.path)?;
    let current = format!(
        "metadata:{}:{}:{}:{}:{}",
        metadata.dev(),
        metadata.ino(),
        metadata.len(),
        metadata.mtime(),
        metadata.mtime_nsec()
    );
    if current != entry.revision {
        return Err(conflict("Source metadata changed before acknowledgement"));
    }
    Ok(
        json!({"device":metadata.dev(),"inode":metadata.ino(),"byte_len":metadata.len(),"mtime_seconds":metadata.mtime(),"mtime_nanoseconds":metadata.mtime_nsec()}),
    )
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
    if ground.content_pool.enabled {
        // Declared sources keep their identity; the pool fills the rest of
        // the scope's readable content under one pooled provenance.
        source_horizon::insert_tree_bindings(
            &scope.root,
            &scope.root,
            &scope.world,
            &["content-pool"],
            "pooled",
            "scope-content",
            "pooled-content",
            &ground.content_pool.exclude,
            &mut sources,
        )?;
    }
    if sources.len() > MAX_ENTRIES {
        return Err(invalid("File map exceeds the 1000000-source bound"));
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
        let bytes = held_source_bytes(&entry)?;
        entry.revision = content_revision_bytes(&bytes);
    }
    Ok(entry)
}
pub(crate) fn payload(entry: &Entry) -> io::Result<Vec<u8>> {
    if entry.kind != "file" {
        return Err(invalid("A directory has no byte payload"));
    }
    let allowed = || source_horizon::retrieval_admission(Path::new("/"), &entry.path);
    if !allowed()? {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Source retrieval has been revoked",
        ));
    }
    let bytes = held_source_bytes(entry)?;
    if !allowed()? {
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
            .map_err(|error| {
                let kind = error
                    .io_error()
                    .map(io::Error::kind)
                    .unwrap_or(io::ErrorKind::Other);
                io::Error::new(kind, error)
            })?
        {
            graph.insert(record).map_err(io::Error::other)?;
        }
    }
    if scope.world != "control:root" {
        for record in RelationRecordStore::worlds_in_project(&scope.root)
            .load_typed::<WorldRecord>()
            .map_err(|error| {
                let kind = error
                    .io_error()
                    .map(io::Error::kind)
                    .unwrap_or(io::ErrorKind::Other);
                io::Error::new(kind, error)
            })?
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
