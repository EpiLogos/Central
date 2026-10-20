use crate::action::{
    ActionAvailability, ActionDescriptor, ActionExecutionContext, ActionInputDefinition,
    ActionOutputDefinition, ActionRegistry, MutationClass,
};
use crate::control::AGENT_RETRIEVAL_DENY_MARKER;
use crate::projectcentral::{
    read_project_manifest, AGENT_GOVERNANCE_DIR, ROOT_AGENT_GOVERNANCE_DIR, ROOT_HUMAN_SOURCE_DIR,
    ROOT_WIKI_DIR, ROOT_WIKI_SOURCE, WIKI_DIR, WIKI_SOURCE,
};
use crate::result::{ActionResult, ResultStatus};
use crate::root::resolve_central_root;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

pub const SOURCE_HORIZON_SCHEMA: &str = "central.source-change-horizon/v1";
pub const SOURCE_CHANGE_SCHEMA: &str = "central.source-change/v1";
pub const SOURCE_HORIZON_PROVIDER: &str = "central.filesystem-reconcile/v1";
pub const PROJECT_HORIZON_STATE: &str = ".central/source-change-horizon.json";
pub const CONTROL_HORIZON_STATE: &str = ".central/source-change-control.json";
pub const GROUND_RELATIONS_SOURCE: &str = "ProjectCentral/relations/source-relations.json";
pub const GROUND_RELATIONS_SCHEMA: &str = "central.project.ground-relations/v1";
pub const CONTROL_GROUND_RELATIONS_SOURCE: &str = "Control/relations/source-relations.json";
pub const CONTROL_GROUND_RELATIONS_SCHEMA: &str = "central.control.ground-relations/v1";
pub const CONTROL_WORLD_REF: &str = "control:root";

const MAX_SCAN_DEPTH: usize = 24;
const SOURCE_HORIZON_LOCK: &str = "source-horizon.lock";
static NEXT_STATE_TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// The Control trees that participate in the root horizon by tree stamp:
/// `(directory, role, provenance, treatment)`. Declared Control relations
/// override the stamp for their exact paths. The owner's personal ground,
/// the Agent governance and the root Wiki are the original participants; the
/// root agent ground (AgentProfiles, agent expressions, AgentSets) is the
/// material an agent host needs, and participates so a root source transfer
/// can carry it through the same revisioned seam as every other source. It
/// is agent ground, not authored human ground: its roles are not in the
/// authored-human set, so a declared agent may carry it.
pub(crate) const CONTROL_TREE_BINDINGS: [(&str, &str, &str, &str); 6] = [
    (
        ROOT_HUMAN_SOURCE_DIR,
        "personal-human-source-aperture",
        "unresolved",
        "control-user",
    ),
    (
        ROOT_AGENT_GOVERNANCE_DIR,
        "agent-governance-source",
        "unresolved",
        "control-agent-governance",
    ),
    (
        ROOT_WIKI_DIR,
        "agent-wiki-source",
        "agent-maintained",
        "control-agent-wiki",
    ),
    (
        crate::agent_profile_store::ROOT_AGENT_PROFILE_DIR,
        "agent-profile-source",
        "unresolved",
        "control-agent-profiles",
    ),
    (
        crate::world_map::ROOT_AGENT_EXPRESSIONS_DIR,
        "agent-expression-source",
        "unresolved",
        "control-agent-expressions",
    ),
    (
        crate::agent_set_store::ROOT_AGENT_SET_DIR,
        "agent-set-source",
        "unresolved",
        "control-agent-sets",
    ),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SourceChangeKind {
    Added,
    Modified,
    Removed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceBinding {
    #[serde(rename = "ref")]
    pub source_ref: String,
    pub path: String,
    #[serde(default)]
    pub roles: Vec<String>,
    pub provenance: String,
    pub standing: String,
    pub treatment: String,
    pub agent_retrieval_allowed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceRevision {
    pub revision: String,
    pub byte_len: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObservedSource {
    pub binding: SourceBinding,
    pub revision: SourceRevision,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceChange {
    pub schema: String,
    pub change_ref: String,
    pub cursor: u64,
    pub world_ref: String,
    pub source_ref: String,
    pub source_path: String,
    pub source_roles: Vec<String>,
    pub provenance: String,
    pub standing: String,
    pub treatment: String,
    pub agent_retrieval_allowed: bool,
    pub before_revision: Option<String>,
    pub after_revision: Option<String>,
    pub kind: SourceChangeKind,
    pub observed_at_unix_seconds: u64,
    pub provider: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actor: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actor_kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_session_ref: Option<String>,
}

/// Caller-supplied attribution carried onto the horizon change that one owner
/// Action write produces. Attribution is declared by the caller and recorded
/// verbatim; it is never inferred and never substituted for the human's own
/// authorship.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceWriteAttribution {
    pub actor: String,
    pub actor_kind: String,
    pub agent_session_ref: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct SourceHorizonState {
    schema: String,
    world_ref: String,
    cursor: u64,
    #[serde(default)]
    sources: BTreeMap<String, ObservedSource>,
    #[serde(default)]
    changes: Vec<SourceChange>,
    #[serde(default)]
    consumer_cursors: BTreeMap<String, u64>,
    provider: String,
    reconciled_at_unix_seconds: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SourceHorizon {
    pub schema: String,
    pub world_ref: String,
    pub cursor: u64,
    pub sources: Vec<ObservedSource>,
    pub changes: Vec<SourceChange>,
    pub consumer_cursors: BTreeMap<String, u64>,
    pub provider: String,
    pub reconciled_at_unix_seconds: u64,
    pub source_payloads_exposed: bool,
    pub automatic_agent_or_model_invocation: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReconcileReport {
    pub initialized: bool,
    pub new_changes: Vec<SourceChange>,
    pub horizon: SourceHorizon,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CompactionReport {
    pub before_changes: usize,
    pub after_changes: usize,
    pub minimum_active_cursor: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
struct GroundRelationsFile {
    schema: String,
    project_id: String,
    #[serde(default)]
    relations: Vec<GroundRelation>,
    /// The bounded-entity subject ref declared at this register, in the
    /// central.pasu/v1 grammar; present only where the ground names its
    /// subject. Absence is data.
    #[serde(default)]
    subject_ref: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct GroundRelation {
    #[serde(rename = "ref")]
    source_ref: String,
    path: String,
    provenance: String,
    standing: String,
    #[serde(default)]
    roles: Vec<String>,
    treatment: String,
}

/// A validated relations document, indexed by member, for a caller that
/// resolves many Sources against the same observed declaration. Validation
/// rejects duplicate refs and paths, so each member names at most one row.
#[derive(Debug, Clone)]
pub(crate) struct ParsedRelations {
    file: GroundRelationsFile,
    by_member: BTreeMap<PathBuf, usize>,
}

impl ParsedRelations {
    fn new(file: GroundRelationsFile) -> io::Result<Self> {
        let by_member = file
            .relations
            .iter()
            .enumerate()
            .map(|(index, relation)| {
                Ok((
                    crate::source_safety::normal_member_key(&relation.path)?,
                    index,
                ))
            })
            .collect::<io::Result<_>>()?;
        Ok(Self { file, by_member })
    }

    /// Whether an accepted relation declares this normal member.
    pub(crate) fn declares(&self, member_key: &Path) -> bool {
        self.by_member.contains_key(member_key)
    }

    fn relation(&self, member_key: &Path) -> Option<&GroundRelation> {
        self.by_member
            .get(member_key)
            .map(|&index| &self.file.relations[index])
    }
}

/// The same validation as `validate_relations_value`, kept for reuse: a
/// batch parses each observed document once instead of once per Source.
pub(crate) fn parse_declared_relations(
    value: &Value,
    schema: &str,
    expected_id: &str,
) -> io::Result<ParsedRelations> {
    ParsedRelations::new(parse_relations_value(value, schema, expected_id)?)
}

fn unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub(crate) fn normalize_relative(path: &Path) -> String {
    path.components()
        .filter_map(|component| match component {
            Component::Normal(value) => Some(value.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

fn validate_project_member(raw: &str) -> io::Result<()> {
    if raw.trim().is_empty() || raw != raw.trim() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "source path must be non-empty and have no surrounding whitespace",
        ));
    }
    let path = Path::new(raw);
    if path.is_absolute()
        || !path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("source path must remain inside its world: {raw}"),
        ));
    }
    Ok(())
}

pub(crate) fn source_ref(world_ref: &str, path: &str) -> String {
    let escaped = path
        .replace('%', "%25")
        .replace(':', "%3A")
        .replace(' ', "%20");
    format!("central:source:{world_ref}:{escaped}")
}

pub fn content_revision(path: &Path) -> io::Result<SourceRevision> {
    let bytes = fs::read(path)?;
    // Versioned FNV-1a is deliberately implemented in-tree: a change horizon needs a stable
    // content revision, not a new crypto/package dependency or a platform-specific metadata id.
    let mut hash = 0xcbf29ce484222325u64;
    for byte in &bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    Ok(SourceRevision {
        revision: format!("central.content-fnv1a64/v1:{}:{hash:016x}", bytes.len()),
        byte_len: bytes.len() as u64,
    })
}

/// Current stock retrieval admission within the supplied owner's boundary.
/// This observes treatment and form; it grants neither authorship nor mutation.
/// An absent final member is permitted for existing creation/dummy-member
/// callers, without claiming that the material exists or can be delivered.
pub(crate) fn retrieval_admission(world_root: &Path, source: &Path) -> io::Result<bool> {
    retrieval_path_admission(world_root, source, false)
}

/// Creation observes the same existing ancestor treatments and physical form,
/// but a genuinely absent suffix can be prepared by the native creator. This
/// is no evidence of material, Source identity, delivery or write authority.
pub(crate) fn retrieval_creation_admission(world_root: &Path, destination: &Path) -> io::Result<bool> {
    retrieval_path_admission(world_root, destination, true)
}

#[cfg(test)]
type CreationRootCheckpoint = Box<dyn FnOnce()>;
#[cfg(test)]
thread_local! {
    static AFTER_CREATION_ROOT_CAPTURE: std::cell::RefCell<Option<CreationRootCheckpoint>> = const { std::cell::RefCell::new(None) };
}

fn retrieval_path_admission(world_root: &Path, source: &Path, allow_absent_suffix: bool) -> io::Result<bool> {
    use std::os::unix::fs::MetadataExt;

    let relative = source.strip_prefix(world_root).map_err(|_| {
        io::Error::new(io::ErrorKind::InvalidInput, "Retrieval source is outside its owner boundary")
    })?;
    if !relative.components().all(|part| matches!(part, Component::Normal(_))) {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "Retrieval source requires a normal member path"));
    }
    let canonical = fs::canonicalize(world_root)?;
    let root_metadata = fs::symlink_metadata(&canonical)?;
    if !root_metadata.is_dir() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "Retrieval owner root is not a directory"));
    }
    let marker = |directory: &Path| -> io::Result<bool> {
        match fs::symlink_metadata(directory.join(AGENT_RETRIEVAL_DENY_MARKER)) {
            Ok(metadata) if metadata.is_file() => Ok(true),
            Ok(_) => Err(io::Error::new(io::ErrorKind::PermissionDenied,
                "Retrieval marker must be an unredirected regular file")),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(error),
        }
    };
    let check_root = || -> io::Result<()> {
        let current = fs::canonicalize(world_root)?;
        let metadata = fs::symlink_metadata(&current)?;
        if current != canonical || metadata.dev() != root_metadata.dev() || metadata.ino() != root_metadata.ino() {
            return Err(io::Error::other("Retrieval owner root affiliation changed"));
        }
        Ok(())
    };
    #[cfg(test)]
    if allow_absent_suffix {
        let checkpoint = AFTER_CREATION_ROOT_CAPTURE.with(|checkpoint| checkpoint.borrow_mut().take());
        if let Some(checkpoint) = checkpoint { checkpoint(); }
    }
    let mut current = canonical.clone();
    if marker(&current)? {
        check_root()?;
        return Ok(false);
    }
    let parts: Vec<_> = relative.components().collect();
    for (index, part) in parts.iter().enumerate() {
        current.push(part.as_os_str());
        let metadata = match fs::symlink_metadata(&current) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound
                && (allow_absent_suffix || index + 1 == parts.len()) => {
                check_root()?;
                return Ok(true);
            }
            Err(error) => return Err(error),
        };
        if metadata.file_type().is_symlink() || (!metadata.is_dir() && !metadata.is_file()) {
            return Err(io::Error::new(io::ErrorKind::PermissionDenied, "Retrieval member is redirected or not ordinary material"));
        }
        if index + 1 != parts.len() && !metadata.is_dir() {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "Retrieval parent is not a directory"));
        }
        if metadata.is_dir() && marker(&current)? {
            check_root()?;
            return Ok(false);
        }
    }
    check_root()?;
    Ok(true)
}

/// Compatibility projection for existing binding metadata. A failed
/// observation cannot grant readability; actual delivery uses the fallible API.
pub(crate) fn retrieval_allowed(world_root: &Path, source: &Path) -> bool {
    retrieval_admission(world_root, source).unwrap_or(false)
}

fn safe_regular_file(world_root: &Path, path: &Path) -> io::Result<bool> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error),
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Ok(false);
    }
    let canonical_root = fs::canonicalize(world_root)?;
    let canonical_file = fs::canonicalize(path)?;
    Ok(canonical_file.starts_with(canonical_root))
}

/// A refusal a scan has already met, kept so a directory is judged once and every file below it hears the same answer.
#[derive(Clone)]
struct Refusal(io::ErrorKind, String);

impl Refusal {
    fn of(error: io::Error) -> Self {
        Self(error.kind(), error.to_string())
    }
    fn new(kind: io::ErrorKind, message: &str) -> Self {
        Self(kind, message.to_owned())
    }
    fn error(&self) -> io::Error {
        io::Error::new(self.0, self.1.clone())
    }
}

/// Whether a directory, and everything above it up to the world's root, lets retrieval through.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Chain {
    Open,
    Withheld,
}

/// What one scan of a world's tree learns once and reuses. The scan asked the same questions of the same directories for every
/// file below them: five canonicalisations of the world's root and a symlink check and a deny-marker check for each ancestor, per
/// file — about twenty-five syscalls for each of the 625 Control sources, most of the 24 ms a `central.time.policy` read still spent
/// after its quadratic key scan was gone (20 Oct measurement). Here each directory is judged once for the scan and each file costs its
/// own `lstat`. The answers are those of `safe_regular_file` and of `retrieval_allowed`, with one difference the compatibility
/// projection allows: the closing re-check that the root did not change (`check_root`) is not repeated for every file, because
/// `retrieval_allowed` only records a flag on a binding — "a failed observation cannot grant readability; actual delivery uses the
/// fallible API", which keeps its own check.
pub(crate) struct ScanMemo<'a> {
    world_root: &'a Path,
    canonical_root: Option<Result<PathBuf, Refusal>>,
    root_is_directory: Option<Result<(), Refusal>>,
    chains: std::collections::HashMap<PathBuf, Result<Chain, Refusal>>,
    canonical_dirs: std::collections::HashMap<PathBuf, Result<PathBuf, Refusal>>,
}

impl<'a> ScanMemo<'a> {
    pub(crate) fn new(world_root: &'a Path) -> Self {
        Self {
            world_root,
            canonical_root: None,
            root_is_directory: None,
            chains: std::collections::HashMap::new(),
            canonical_dirs: std::collections::HashMap::new(),
        }
    }

    fn canonical_root(&mut self) -> Result<PathBuf, Refusal> {
        self.canonical_root
            .get_or_insert_with(|| fs::canonicalize(self.world_root).map_err(Refusal::of))
            .clone()
    }

    fn canonical_dir(&mut self, dir: &Path) -> Result<PathBuf, Refusal> {
        if let Some(known) = self.canonical_dirs.get(dir) {
            return known.clone();
        }
        let canonical = fs::canonicalize(dir).map_err(Refusal::of);
        self.canonical_dirs.insert(dir.to_path_buf(), canonical.clone());
        canonical
    }

    /// `safe_regular_file`, with the directory's canonical path read once. A file that is not a symlink canonicalises to its
    /// directory's canonical path and its own name, so the containment test needs only the directory's.
    pub(crate) fn safe_regular_file(&mut self, path: &Path) -> io::Result<bool> {
        let metadata = match fs::symlink_metadata(path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(error),
        };
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Ok(false);
        }
        let canonical_root = self.canonical_root().map_err(|refusal| refusal.error())?;
        let canonical_file = match (path.parent(), path.file_name()) {
            (Some(parent), Some(name)) if name != "." && name != ".." => {
                self.canonical_dir(parent).map_err(|refusal| refusal.error())?.join(name)
            }
            _ => fs::canonicalize(path)?,
        };
        Ok(canonical_file.starts_with(canonical_root))
    }

    fn marker(directory: &Path) -> Result<bool, Refusal> {
        match fs::symlink_metadata(directory.join(AGENT_RETRIEVAL_DENY_MARKER)) {
            Ok(metadata) if metadata.is_file() => Ok(true),
            Ok(_) => Err(Refusal::new(
                io::ErrorKind::PermissionDenied,
                "Retrieval marker must be an unredirected regular file",
            )),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(Refusal::of(error)),
        }
    }

    /// The verdict on one directory below the root, given the verdict on its parent (the loop of `retrieval_path_admission` for a
    /// component that is not the last): redirected, absent or unordinary material refuses; a deny marker withholds.
    fn judge_directory(directory: &Path) -> Result<Chain, Refusal> {
        let metadata = fs::symlink_metadata(directory).map_err(Refusal::of)?;
        if metadata.file_type().is_symlink() || (!metadata.is_dir() && !metadata.is_file()) {
            return Err(Refusal::new(io::ErrorKind::PermissionDenied, "Retrieval member is redirected or not ordinary material"));
        }
        if !metadata.is_dir() {
            return Err(Refusal::new(io::ErrorKind::InvalidInput, "Retrieval parent is not a directory"));
        }
        Ok(if Self::marker(directory)? { Chain::Withheld } else { Chain::Open })
    }

    fn chain(&mut self, directory: &Path, canonical_root: &Path) -> Result<Chain, Refusal> {
        if let Some(known) = self.chains.get(directory) {
            return known.clone();
        }
        let verdict = if directory == canonical_root {
            Self::marker(directory).map(|withheld| if withheld { Chain::Withheld } else { Chain::Open })
        } else {
            match directory.parent() {
                Some(parent) => match self.chain(parent, canonical_root) {
                    Ok(Chain::Open) => Self::judge_directory(directory),
                    other => other,
                },
                None => Err(Refusal::new(io::ErrorKind::InvalidInput, "Retrieval source is outside its owner boundary")),
            }
        };
        self.chains.insert(directory.to_path_buf(), verdict.clone());
        verdict
    }

    /// `retrieval_admission` for a member of the world (never an absent suffix), each ancestor judged once for the scan.
    fn admission(&mut self, source: &Path) -> io::Result<bool> {
        let relative = source.strip_prefix(self.world_root).map_err(|_| {
            io::Error::new(io::ErrorKind::InvalidInput, "Retrieval source is outside its owner boundary")
        })?;
        if !relative.components().all(|part| matches!(part, Component::Normal(_))) {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "Retrieval source requires a normal member path"));
        }
        let canonical = self.canonical_root().map_err(|refusal| refusal.error())?;
        let root_is_directory = self
            .root_is_directory
            .get_or_insert_with(|| match fs::symlink_metadata(&canonical) {
                Ok(metadata) if metadata.is_dir() => Ok(()),
                Ok(_) => Err(Refusal::new(io::ErrorKind::InvalidInput, "Retrieval owner root is not a directory")),
                Err(error) => Err(Refusal::of(error)),
            })
            .clone();
        root_is_directory.map_err(|refusal| refusal.error())?;
        let Some(name) = relative.file_name() else {
            // The root itself: only its own marker speaks.
            return match self.chain(&canonical, &canonical).map_err(|refusal| refusal.error())? {
                Chain::Open => Ok(true),
                Chain::Withheld => Ok(false),
            };
        };
        let directory = canonical.join(relative.parent().unwrap_or(Path::new("")));
        if self.chain(&directory, &canonical).map_err(|refusal| refusal.error())? == Chain::Withheld {
            return Ok(false);
        }
        let member = directory.join(name);
        let metadata = match fs::symlink_metadata(&member) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(true),
            Err(error) => return Err(error),
        };
        if metadata.file_type().is_symlink() || (!metadata.is_dir() && !metadata.is_file()) {
            return Err(io::Error::new(io::ErrorKind::PermissionDenied, "Retrieval member is redirected or not ordinary material"));
        }
        if metadata.is_dir() && Self::marker(&member).map_err(|refusal| refusal.error())? {
            return Ok(false);
        }
        Ok(true)
    }

    /// The compatibility projection (`retrieval_allowed`): a failed observation cannot grant readability.
    pub(crate) fn retrieval_allowed(&mut self, source: &Path) -> bool {
        self.admission(source).unwrap_or(false)
    }

    /// `collect_files` over this scan's memo.
    pub(crate) fn collect_files(&mut self, root: &Path, depth: usize, files: &mut Vec<PathBuf>) -> io::Result<()> {
        if depth > MAX_SCAN_DEPTH || !root.is_dir() {
            return Ok(());
        }
        let mut entries = fs::read_dir(root)?.collect::<Result<Vec<_>, _>>()?;
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let path = entry.path();
            let file_type = entry.file_type()?;
            if file_type.is_symlink() {
                continue;
            }
            if file_type.is_dir() {
                let name = entry.file_name().to_string_lossy().into_owned();
                if !should_skip_dir(&name) {
                    self.collect_files(&path, depth + 1, files)?;
                }
            } else if file_type.is_file()
                && entry.file_name() != AGENT_RETRIEVAL_DENY_MARKER
                && !is_system_artifact(&entry.file_name())
                && self.safe_regular_file(&path)?
            {
                files.push(path);
            }
        }
        Ok(())
    }
}

/// Directories no scan descends into: version control, build output, and the
/// state or vendored bytes a tool keeps for itself. None of it is something a
/// person wrote or an agent should retrieve, and all of it churns: a GitNexus
/// parse cache or a package store rewrites itself on every index or install,
/// which left the pooled maps permanently "stale index; refresh required".
fn should_skip_dir(name: &str) -> bool {
    matches!(
        name,
        ".git"
            | ".central"
            | "target"
            | "node_modules"
            | ".next"
            | "dist"
            | "build"
            | ".gitnexus"
            | ".pnpm-store"
            | "__pycache__"
            | ".venv"
            | "venv"
            | ".cache"
            | ".mypy_cache"
            | ".pytest_cache"
            | ".ruff_cache"
            | ".tox"
            | ".obsidian"
            | ".idea"
            | ".turbo"
            | ".svelte-kit"
            | "DerivedData"
    )
}

/// Extensions of files that hold bytes, not text: media, archives, compiled
/// output, office containers, databases, fonts. A Source is read as text, so
/// a pooled file with one of these can only fail admission when a search hit
/// on it is read (`Source is not text`): 7 of 176 candidate hits did on
/// 19 Oct 2026, and AIKit reported them as an unreadable origin. The pool
/// leaves them out; reading one by path (`content_encoding: base64`) is
/// unchanged.
const NON_TEXT_EXTENSIONS: &[&str] = &[
    "png", "jpg", "jpeg", "gif", "webp", "tif", "tiff", "bmp", "heic", "ico", "icns", "psd", "raw",
    "mp3", "wav", "flac", "aac", "m4a", "ogg", "mp4", "mov", "m4v", "mkv", "avi", "webm", "zip",
    "gz", "tgz", "bz2", "xz", "7z", "rar", "tar", "dmg", "iso", "pyc", "pyo", "o", "a", "so",
    "dylib", "dll", "exe", "class", "jar", "wasm", "node", "pdf", "doc", "docx", "ppt", "pptx",
    "xls", "xlsx", "key", "pages", "numbers", "db", "sqlite", "sqlite3", "lbug", "ttf", "otf",
    "woff", "woff2",
];

/// Whether a pooled file can be read as text, from its name alone: the pool is
/// enumerated on every roster build, so it cannot afford to open each file.
pub(crate) fn holds_text(relative: &str) -> bool {
    Path::new(relative)
        .extension()
        .and_then(|ext| ext.to_str())
        .is_none_or(|ext| !NON_TEXT_EXTENSIONS.contains(&ext.to_ascii_lowercase().as_str()))
}

/// Files an operating system writes beside a person's own: never Source.
/// macOS Finder leaves a binary `.DS_Store` in every folder it opens; on
/// 18 Oct 2026 hybrid file-map search ranked `Control/user/.DS_Store` first
/// for an unrelated query, and such files only fail admission as not text.
fn is_system_artifact(name: &std::ffi::OsStr) -> bool {
    name == ".DS_Store"
}

#[cfg(test)]
pub(crate) fn collect_files(
    root: &Path,
    world_root: &Path,
    depth: usize,
    files: &mut Vec<PathBuf>,
) -> io::Result<()> {
    ScanMemo::new(world_root).collect_files(root, depth, files)
}

// Collects a subtree's source bindings under one provenance/standing/treatment;
// the descriptor fields are passed positionally into the shared accumulator
// rather than bundled into a struct that exists only for this call.
/// Does a normalized relative path sit inside an excluded subtree? An
/// exclusion matches at component boundaries: "Seeds" excludes "Seeds/x"
/// but not "Seeds-2/x".
fn excluded_relative(relative: &str, excluded: &str) -> bool {
    let excluded = excluded.trim_matches('/');
    if excluded.is_empty() {
        return false;
    }
    relative == excluded || relative.starts_with(&format!("{excluded}/"))
}

/// Only the caller's actual canonical Wiki source gives its stable physical
/// lock this role. An identical relative path in another register is ordinary
/// source material; explicitly declared source relations remain unchanged.
pub(crate) fn is_canonical_wiki_publication_lock(relative: &Path, wiki_source: &str) -> bool {
    matches!(
        (wiki_source, relative.to_str()),
        (
            ROOT_WIKI_SOURCE,
            Some("Control/agents/wiki/.wiki.json.publication.lock")
        ) | (
            WIKI_SOURCE,
            Some("ProjectCentral/agents/wiki/.wiki.json.publication.lock")
        )
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn insert_tree_bindings(
    world_root: &Path,
    scan_root: &Path,
    world_ref: &str,
    roles: &[&str],
    provenance: &str,
    standing: &str,
    treatment: &str,
    exclude: &[String],
    bindings: &mut BTreeMap<String, SourceBinding>,
) -> io::Result<usize> {
    insert_admitted_bindings(
        world_root, scan_root, world_ref, roles, provenance, standing, treatment, exclude,
        bindings, |_| true,
    )
}

/// A scope's content pool: everything readable under the scope's root that is
/// not declared, excluded or a file that holds bytes rather than text.
pub(crate) fn insert_pool_bindings(
    world_root: &Path,
    world_ref: &str,
    exclude: &[String],
    bindings: &mut BTreeMap<String, SourceBinding>,
) -> io::Result<usize> {
    insert_admitted_bindings(
        world_root,
        world_root,
        world_ref,
        &["content-pool"],
        "pooled",
        "scope-content",
        "pooled-content",
        exclude,
        bindings,
        holds_text,
    )
}

#[allow(clippy::too_many_arguments)]
fn insert_admitted_bindings(
    world_root: &Path,
    scan_root: &Path,
    world_ref: &str,
    roles: &[&str],
    provenance: &str,
    standing: &str,
    treatment: &str,
    exclude: &[String],
    bindings: &mut BTreeMap<String, SourceBinding>,
    admit: impl Fn(&str) -> bool,
) -> io::Result<usize> {
    let mut scan = ScanMemo::new(world_root);
    let mut files = Vec::new();
    scan.collect_files(scan_root, 0, &mut files)?;
    let wiki_source = if world_ref == CONTROL_WORLD_REF
        && scan_root == world_root.join(ROOT_WIKI_DIR).as_path()
    {
        Some(ROOT_WIKI_SOURCE)
    } else if world_ref.starts_with("project:")
        && scan_root == world_root.join(WIKI_DIR).as_path()
    {
        Some(WIKI_SOURCE)
    } else {
        None
    };
    let mut seen = 0usize;
    for file in files {
        let relative = normalize_relative(file.strip_prefix(world_root).map_err(|_| {
            io::Error::new(io::ErrorKind::InvalidData, "source escaped its world root")
        })?);
        let publication_lock = wiki_source.is_some_and(|source| {
            is_canonical_wiki_publication_lock(Path::new(&relative), source)
        });
        if publication_lock
            || !admit(&relative)
            || exclude
                .iter()
                .any(|excluded| excluded_relative(&relative, excluded))
        {
            continue;
        }
        seen += 1;
        let reference = source_ref(world_ref, &relative);
        // A source an earlier pass bound (a skill's files, before the Control/user aperture fallback walks them again) keeps its
        // binding, so its admission is not observed a second time.
        if let std::collections::btree_map::Entry::Vacant(slot) = bindings.entry(reference.clone()) {
            slot.insert(SourceBinding {
                source_ref: reference,
                path: relative,
                roles: roles.iter().map(|role| (*role).to_owned()).collect(),
                provenance: provenance.to_owned(),
                standing: standing.to_owned(),
                treatment: treatment.to_owned(),
                agent_retrieval_allowed: scan.retrieval_allowed(&file),
            });
        }
    }
    Ok(seen)
}

/// The NOW field of each register, relative to its world root.
pub(crate) const ROOT_NOW_DIR: &str = "Control/agents/now";
pub(crate) const PROJECT_NOW_DIR: &str = "ProjectCentral/now";

/// Record families of a NOW field, each a directory whose shallow files are
/// records. Clearings are the one nested family: `clearings/<id>/now.json`.
const NOW_RECORD_FAMILIES: [&str; 3] = ["agents", "day", "flows"];
const NOW_RECORD_EXTENSIONS: [&str; 4] = ["json", "md", "txt", "html"];

/// Is `member` (a normalised path relative to the world root) a NOW record of
/// that register? The one definition behind the roster, resolve-by-ref and the
/// listing: a file at its family's record depth, `agents/<file>`, `day/<file>`,
/// `flows/<file>` or `clearings/<id>/now.json`. Anything deeper is evidence
/// (`day/<date>.sources/`, a clearing's `T/`) and `tmp/` is scratch; none of it
/// is a record, so none of it is a Source.
pub(crate) fn is_now_record_member(world_ref: &str, member: &str) -> bool {
    let base = if world_ref == CONTROL_WORLD_REF {
        ROOT_NOW_DIR
    } else {
        PROJECT_NOW_DIR
    };
    let Some(rest) = member
        .strip_prefix(base)
        .and_then(|rest| rest.strip_prefix('/'))
    else {
        return false;
    };
    match rest.split('/').collect::<Vec<_>>().as_slice() {
        [family, file] if NOW_RECORD_FAMILIES.contains(family) => record_typed_name(file),
        ["clearings", id, "now.json"] => !id.is_empty() && !id.starts_with('.'),
        _ => false,
    }
}

fn record_typed_name(name: &str) -> bool {
    !name.starts_with('.')
        && name != AGENT_RETRIEVAL_DENY_MARKER
        && Path::new(name)
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| NOW_RECORD_EXTENSIONS.contains(&ext))
}

/// The retrieval-only binding of one NOW record.
pub(crate) fn now_record_binding(
    world_ref: &str,
    relative: &str,
    agent_retrieval_allowed: bool,
) -> SourceBinding {
    SourceBinding {
        source_ref: source_ref(world_ref, relative),
        path: relative.to_owned(),
        roles: vec!["now-record".to_owned()],
        provenance: "agent-maintained".to_owned(),
        standing: "unspecified".to_owned(),
        treatment: "retain-native-in-place".to_owned(),
        agent_retrieval_allowed,
    }
}

/// Retrieval-only bindings for the NOW records of one register.
///
/// Placement is the authorisation, so there is no registration step: the
/// bindings are computed from the filesystem each time, and a record that is
/// distilled, pruned or archived leaves the roster by leaving its folder.
///
/// These are not ground relations, carry no native-ownership role of their own
/// and take no part in the source change horizon, so NOW churn raises no
/// source-change events. A registered relation for the same path keeps its own
/// identity and roles.
pub(crate) fn now_record_bindings(
    world_root: &Path,
    world_ref: &str,
) -> io::Result<Vec<SourceBinding>> {
    let now_rel = if world_ref == CONTROL_WORLD_REF {
        ROOT_NOW_DIR
    } else {
        PROJECT_NOW_DIR
    };
    let now_dir = world_root.join(now_rel);
    let mut files: Vec<PathBuf> = Vec::new();
    for family in NOW_RECORD_FAMILIES {
        shallow_record_files(&now_dir.join(family), world_root, &mut files)?;
    }
    if let Ok(clearings) = fs::read_dir(now_dir.join("clearings")) {
        let mut ids = clearings.collect::<Result<Vec<_>, _>>()?;
        ids.sort_by_key(|entry| entry.file_name());
        for id in ids {
            if id.file_type()?.is_dir() && !id.file_name().to_string_lossy().starts_with('.') {
                let record = id.path().join("now.json");
                if fs::symlink_metadata(&record).is_ok_and(|m| m.file_type().is_file())
                    && safe_regular_file(world_root, &record)?
                {
                    files.push(record);
                }
            }
        }
    }
    let mut bindings = Vec::with_capacity(files.len());
    for file in files {
        let relative = normalize_relative(file.strip_prefix(world_root).map_err(|_| {
            io::Error::new(io::ErrorKind::InvalidData, "source escaped its world root")
        })?);
        debug_assert!(is_now_record_member(world_ref, &relative));
        bindings.push(now_record_binding(
            world_ref,
            &relative,
            retrieval_allowed(world_root, &file),
        ));
    }
    Ok(bindings)
}

/// The regular, record-typed files directly inside `dir`. An absent directory
/// has none. Symlinks, subdirectories, dotfiles, locks and the retrieval-deny
/// marker are never records.
fn shallow_record_files(dir: &Path, world_root: &Path, out: &mut Vec<PathBuf>) -> io::Result<()> {
    let Ok(read) = fs::read_dir(dir) else {
        return Ok(());
    };
    let mut entries = read.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        if !record_typed_name(&entry.file_name().to_string_lossy()) || !entry.file_type()?.is_file()
        {
            continue;
        }
        let path = entry.path();
        if safe_regular_file(world_root, &path)? {
            out.push(path);
        }
    }
    Ok(())
}

fn read_relations_file(
    path: &Path,
    schema: &str,
    expected_id: &str,
) -> io::Result<Vec<GroundRelation>> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    let value: Value = serde_json::from_slice(&bytes)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    Ok(parse_relations_value(&value, schema, expected_id)?.relations)
}

/// The same accepted relation grammar governs mutation, bulk bindings and
/// selected observations. A duplicated ref or path cannot acquire a different
/// meaning merely because a consumer happens to choose the first or last row.
pub(crate) fn validate_relations_value(
    value: &Value,
    schema: &str,
    expected_id: &str,
) -> io::Result<()> {
    parse_relations_value(value, schema, expected_id).map(|_| ())
}

fn parse_relations_value(
    value: &Value,
    schema: &str,
    expected_id: &str,
) -> io::Result<GroundRelationsFile> {
    if !value.get("relations").is_some_and(Value::is_array) {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "relations must be an array"));
    }
    let relations: GroundRelationsFile = serde_json::from_value(value.clone())
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    if relations.schema != schema || relations.project_id != expected_id {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "ground relations have an unsupported schema or world id",
        ));
    }
    if let Some(subject_ref) = &relations.subject_ref {
        crate::pasu::PasuRef::parse(subject_ref).map_err(|error| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("ground relations subject ref is invalid: {error}"),
            )
        })?;
    }
    let mut refs = BTreeSet::new();
    let mut paths = BTreeSet::new();
    for relation in &relations.relations {
        validate_project_member(&relation.path)?;
        let member_key = crate::source_safety::normal_member_key(&relation.path)?;
        if relation.source_ref.trim().is_empty() || relation.source_ref.len() > 4096 {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "source ref requires non-empty text of at most 4096 bytes"));
        }
        if !refs.insert(&relation.source_ref) || !paths.insert(member_key) {
            return Err(io::Error::new(io::ErrorKind::InvalidData,
                "ambiguous duplicate source relation; reconcile before mutation"));
        }
    }
    Ok(relations)
}

fn read_ground_relations(
    project_root: &Path,
    expected_project_id: &str,
) -> io::Result<Vec<GroundRelation>> {
    let relations = read_relations_file(
        &project_root.join(GROUND_RELATIONS_SOURCE),
        GROUND_RELATIONS_SCHEMA,
        expected_project_id,
    )?;
    for relation in &relations {
        validate_project_member(&relation.path)?;
    }
    Ok(relations)
}

fn read_control_ground_relations(central_root: &Path) -> io::Result<Vec<GroundRelation>> {
    read_relations_file(
        &central_root.join(CONTROL_GROUND_RELATIONS_SOURCE),
        CONTROL_GROUND_RELATIONS_SCHEMA,
        CONTROL_WORLD_REF,
    )
}

/// The subject ref declared by the Control ground relations, in the
/// central.pasu/v1 grammar. Read for the `central.world` identity exposure;
/// a malformed ref is an error, an absent one is None.
pub fn control_ground_relations_subject_ref(central_root: &Path) -> io::Result<Option<String>> {
    let path = central_root.join(CONTROL_GROUND_RELATIONS_SOURCE);
    if !path.is_file() {
        return Ok(None);
    }
    let file: GroundRelationsFile = serde_json::from_slice(&fs::read(&path)?)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    if file.schema != CONTROL_GROUND_RELATIONS_SCHEMA || file.project_id != CONTROL_WORLD_REF {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "ground relations have an unsupported schema or world id",
        ));
    }
    if let Some(subject_ref) = &file.subject_ref {
        crate::pasu::PasuRef::parse(subject_ref).map_err(|error| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("ground relations subject ref is invalid: {error}"),
            )
        })?;
    }
    Ok(file.subject_ref)
}

fn project_tree_bindings(manifest: &crate::projectcentral::ProjectCentralManifest) -> [(&str, &str, &str, &str); 3] {
    [(&manifest.human_source, "project-human-source-aperture", "unresolved", "projectcentral-user"),
        (AGENT_GOVERNANCE_DIR, "agent-governance-source", "unresolved", "projectcentral-agent-governance"),
        (WIKI_DIR, "agent-wiki-source", "agent-maintained", "projectcentral-agent-wiki")]
}

/// Resolve one Project member from current already-observed native metadata.
/// Accepted exact normal-member declarations take precedence over fallbacks;
/// no other Skill is traversed, and this observation never grants retrieval.
pub(crate) fn project_binding_for_observed_path(
    manifest: &crate::projectcentral::ProjectCentralManifest, path: &str,
    relations: Option<&Value>, skill_manifest: Option<&crate::control_skills::SkillManifest>,
    agent_retrieval_allowed: bool,
) -> io::Result<Option<SourceBinding>> {
    let declared = relations
        .map(|value| parse_declared_relations(value, GROUND_RELATIONS_SCHEMA, &manifest.project_id))
        .transpose()?;
    project_binding_for_parsed_path(
        manifest,
        path,
        declared.as_ref(),
        skill_manifest,
        agent_retrieval_allowed,
    )
}

/// `project_binding_for_observed_path` over a declaration the caller parsed once.
pub(crate) fn project_binding_for_parsed_path(
    manifest: &crate::projectcentral::ProjectCentralManifest,
    path: &str,
    declared: Option<&ParsedRelations>,
    skill_manifest: Option<&crate::control_skills::SkillManifest>,
    agent_retrieval_allowed: bool,
) -> io::Result<Option<SourceBinding>> {
    let validation = manifest.validate();
    if !validation.valid { return Err(io::Error::new(io::ErrorKind::InvalidData, validation.errors.join("; "))); }
    let member = crate::source_safety::normal_member_key(path)?;
    if let Some(relation) = declared.and_then(|declared| declared.relation(&member)) {
        return Ok(Some(SourceBinding { source_ref:relation.source_ref.clone(), path:relation.path.clone(),
            roles:relation.roles.clone(), provenance:relation.provenance.clone(), standing:relation.standing.clone(),
            treatment:relation.treatment.clone(), agent_retrieval_allowed }));
    }
    let world = format!("project:{}", manifest.project_id);
    if let Some(binding) = crate::control_skills::project_skill_binding(&world, &manifest.human_source,
        path, skill_manifest, agent_retrieval_allowed)? { return Ok(Some(binding)); }
    for (directory, role, provenance, treatment) in project_tree_bindings(manifest) {
        let aperture = crate::source_safety::normal_member_key(directory)?;
        if member.starts_with(&aperture) && member != aperture {
            return Ok(Some(SourceBinding { source_ref:source_ref(&world,path), path:path.to_owned(),
                roles:vec![role.to_owned()], provenance:provenance.to_owned(), standing:"unspecified".to_owned(),
                treatment:treatment.to_owned(), agent_retrieval_allowed }));
        }
    }
    for adopted in &manifest.wiki.adopted_sources {
        if crate::source_safety::normal_member_key(adopted)? == member {
            return Ok(Some(SourceBinding { source_ref:source_ref(&world,adopted), path:adopted.clone(),
                roles:vec!["adopted-agent-wiki-source".to_owned()], provenance:"unresolved".to_owned(),
                standing:"unspecified".to_owned(), treatment:"retain-native-in-place".to_owned(), agent_retrieval_allowed }));
        }
    }
    Ok(None)
}

/// The bound members by comparison key (`normal_member_key` of a binding's path), so an explicit relation finds the bindings its
/// path replaces by a lookup. The scan this stands in for re-keyed every binding for every relation — quadratic, and almost half of
/// every Central action's time (a `central.time.policy` read spent 25 of its 55 ms there; 20 Oct measurement). The index is built when
/// the first relation needs it, so a ground with no relations never keys a binding, and a binding whose path will not key still fails
/// the walk at the same point, on the same first binding. A hit is checked against the binding standing at that ref now: a later
/// relation may have overwritten a ref with another path, and a stale hit must not remove it.
#[derive(Default)]
struct MemberIndex(Option<HashMap<PathBuf, Vec<String>>>);

impl MemberIndex {
    /// Remove from `bindings` every binding whose path names the member `key`.
    fn remove_members(&mut self, bindings: &mut BTreeMap<String, SourceBinding>, key: &PathBuf) -> io::Result<()> {
        if self.0.is_none() {
            let mut by_key = HashMap::<PathBuf, Vec<String>>::new();
            for (reference, binding) in bindings.iter() {
                by_key.entry(crate::source_safety::normal_member_key(&binding.path)?).or_default().push(reference.clone());
            }
            self.0 = Some(by_key);
        }
        let Some(references) = self.0.as_mut().and_then(|index| index.remove(key)) else { return Ok(()) };
        for reference in references {
            let standing = bindings.get(&reference)
                .map(|binding| crate::source_safety::normal_member_key(&binding.path))
                .transpose()?;
            if standing.as_ref() == Some(key) {
                bindings.remove(&reference);
            }
        }
        Ok(())
    }

    /// Note a binding the caller put at `reference` for the member `key`.
    fn inserted(&mut self, key: PathBuf, reference: &str) {
        if let Some(index) = self.0.as_mut() {
            index.entry(key).or_default().push(reference.to_owned());
        }
    }
}

pub fn project_source_bindings(project_root: &Path) -> io::Result<Vec<SourceBinding>> {
    let manifest = read_project_manifest(project_root)?;
    let validation = manifest.validate();
    if !validation.valid {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            validation.errors.join("; "),
        ));
    }
    let world_ref = format!("project:{}", manifest.project_id);
    let relations = read_ground_relations(project_root, &manifest.project_id)?;
    let explicit_members = relations.iter()
        .map(|relation| crate::source_safety::normal_member_key(&relation.path))
        .collect::<io::Result<BTreeSet<_>>>()?;
    let mut bindings = BTreeMap::<String, SourceBinding>::new();

    // Skill ground participates first so a skill source keeps one logical
    // binding under the control-skill treatment; the human-source aperture
    // fallback below uses or_insert and therefore never overrides it.
    crate::control_skills::insert_skill_bindings(
        project_root,
        &project_root
            .join(&manifest.human_source)
            .join(crate::control_skills::SKILLS_SEGMENT),
        &world_ref,
        &mut bindings,
        &explicit_members,
    )?;

    for (directory, role, provenance, treatment) in project_tree_bindings(&manifest) {
        insert_tree_bindings(project_root, &project_root.join(directory), &world_ref,
            &[role], provenance, "unspecified", treatment, &[], &mut bindings)?;
    }

    // Current accepted ProjectCentral already supports Wiki sources retained in place. They are
    // participants, not generic Project truth, and therefore retain an explicit Wiki role.
    for adopted in &manifest.wiki.adopted_sources {
        validate_project_member(adopted)?;
        let path = project_root.join(adopted);
        if safe_regular_file(project_root, &path)? {
            let reference = source_ref(&world_ref, adopted);
            bindings.entry(reference.clone()).or_insert(SourceBinding {
                source_ref: reference,
                path: adopted.clone(),
                roles: vec!["adopted-agent-wiki-source".to_owned()],
                provenance: "unresolved".to_owned(),
                standing: "unspecified".to_owned(),
                treatment: "retain-native-in-place".to_owned(),
                agent_retrieval_allowed: retrieval_allowed(project_root, &path),
            });
        }
    }

    let mut members = MemberIndex::default();
    let mut scan = ScanMemo::new(project_root);
    for relation in relations {
        let relative = relation.path.clone();
        let path = project_root.join(&relative);
        if !scan.safe_regular_file(&path)? {
            continue;
        }
        // An explicit recognised relation is the identity/standing authority for its path.
        // Remove the aperture fallback first so one physical source produces one logical change.
        let member_key = crate::source_safety::normal_member_key(&relative)?;
        members.remove_members(&mut bindings, &member_key)?;
        let reference = relation.source_ref.clone();
        members.inserted(member_key, &reference);
        bindings.insert(
            reference,
            SourceBinding {
                source_ref: relation.source_ref,
                path: relative,
                roles: relation.roles,
                provenance: relation.provenance,
                standing: relation.standing,
                treatment: relation.treatment,
                agent_retrieval_allowed: scan.retrieval_allowed(&path),
            },
        );
    }


    Ok(bindings.into_values().collect())
}

pub fn control_source_bindings(central_root: &Path) -> io::Result<Vec<SourceBinding>> {
    let world_ref = CONTROL_WORLD_REF;
    let relations = read_control_ground_relations(central_root)?;
    let explicit_members = relations.iter()
        .map(|relation| crate::source_safety::normal_member_key(&relation.path))
        .collect::<io::Result<BTreeSet<_>>>()?;
    let mut bindings = BTreeMap::<String, SourceBinding>::new();
    // Skill ground participates first (same law as the project side): skill
    // sources bind under the control-skill treatment with standing/provenance
    // from their manifests, and the Control/user aperture fallback below never
    // overrides them. Machine skill scopes are Control ground the tree pass
    // does not otherwise walk.
    crate::control_skills::insert_skill_bindings(
        central_root,
        &central_root.join(crate::control_skills::PERSONAL_SKILL_DIR),
        world_ref,
        &mut bindings,
        &explicit_members,
    )?;
    if let Ok(machines) =
        crate::control_skills::child_directories(&central_root.join("Control/machines"))
    {
        for machine in machines {
            crate::control_skills::insert_skill_bindings(
                central_root,
                &central_root
                    .join("Control/machines")
                    .join(&machine)
                    .join(crate::control_skills::SKILLS_SEGMENT),
                world_ref,
                &mut bindings,
                &explicit_members,
            )?;
        }
    }
    for (dir, role, provenance, treatment) in CONTROL_TREE_BINDINGS {
        insert_tree_bindings(
            central_root,
            &central_root.join(dir),
            world_ref,
            &[role],
            provenance,
            "unspecified",
            treatment,
            &[],
            &mut bindings,
        )?;
    }

    let mut members = MemberIndex::default();
    let mut scan = ScanMemo::new(central_root);
    for relation in relations {
        let relative = relation.path.clone();
        let path = central_root.join(&relative);
        if !scan.safe_regular_file(&path)? {
            continue;
        }
        // Same law as the project flow: an explicit recognised relation is the
        // identity/standing authority for its path; the tree fallback is replaced.
        let member_key = crate::source_safety::normal_member_key(&relative)?;
        members.remove_members(&mut bindings, &member_key)?;
        let reference = relation.source_ref.clone();
        members.inserted(member_key, &reference);
        bindings.insert(
            reference,
            SourceBinding {
                source_ref: relation.source_ref,
                path: relative,
                roles: relation.roles,
                provenance: relation.provenance,
                standing: relation.standing,
                treatment: relation.treatment,
                agent_retrieval_allowed: scan.retrieval_allowed(&path),
            },
        );
    }

    Ok(bindings.into_values().collect())
}

fn observe_bindings(
    world_root: &Path,
    bindings: Vec<SourceBinding>,
) -> io::Result<BTreeMap<String, ObservedSource>> {
    let mut observed = BTreeMap::new();
    for binding in bindings {
        validate_project_member(&binding.path)?;
        let path = world_root.join(&binding.path);
        if !safe_regular_file(world_root, &path)? {
            continue;
        }
        let revision = content_revision(&path)?;
        observed.insert(
            binding.source_ref.clone(),
            ObservedSource { binding, revision },
        );
    }
    Ok(observed)
}

fn load_state(path: &Path) -> io::Result<Option<SourceHorizonState>> {
    if !path.is_file() {
        return Ok(None);
    }
    let state: SourceHorizonState = serde_json::from_slice(&fs::read(path)?)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    if state.schema != SOURCE_HORIZON_SCHEMA || state.provider != SOURCE_HORIZON_PROVIDER {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "source horizon state has an unsupported schema/provider",
        ));
    }
    Ok(Some(state))
}

fn write_state(path: &Path, state: &SourceHorizonState) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut bytes = serde_json::to_vec_pretty(state)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    bytes.push(b'\n');
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = path.file_name().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "source horizon state path has no file name",
        )
    })?;
    let (tmp, mut file) = loop {
        let sequence = NEXT_STATE_TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let tmp = parent.join(format!(
            ".{}.{}.{}.tmp",
            file_name.to_string_lossy(),
            std::process::id(),
            sequence
        ));
        match OpenOptions::new().write(true).create_new(true).open(&tmp) {
            Ok(file) => break (tmp, file),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    };
    if let Err(error) = file.write_all(&bytes) {
        drop(file);
        let _ = fs::remove_file(&tmp);
        return Err(error);
    }
    drop(file);
    if let Err(error) = fs::rename(&tmp, path) {
        let _ = fs::remove_file(&tmp);
        return Err(error);
    }
    Ok(())
}

fn public_horizon(state: &SourceHorizonState, since: Option<u64>) -> SourceHorizon {
    let cutoff = since.unwrap_or(0);
    SourceHorizon {
        schema: state.schema.clone(),
        world_ref: state.world_ref.clone(),
        cursor: state.cursor,
        sources: state.sources.values().cloned().collect(),
        changes: state
            .changes
            .iter()
            .filter(|change| change.cursor > cutoff)
            .cloned()
            .collect(),
        consumer_cursors: state.consumer_cursors.clone(),
        provider: state.provider.clone(),
        reconciled_at_unix_seconds: state.reconciled_at_unix_seconds,
        source_payloads_exposed: false,
        automatic_agent_or_model_invocation: false,
    }
}

/// A horizon written before Project identities were bare manifest ids stored
/// `project:project:<id lowercased>` (the manifest then read
/// `"project_id": "project:<id>"`). That is the same World as `project:<id>`.
fn is_legacy_project_identity(stored: &str, current: &str) -> bool {
    let Some(id) = current.strip_prefix("project:") else {
        return false;
    };
    stored == format!("project:project:{}", id.to_lowercase())
}

fn reconcile(
    world_root: &Path,
    state_path: &Path,
    world_ref: &str,
    bindings: Vec<SourceBinding>,
    attributions: &BTreeMap<String, SourceWriteAttribution>,
) -> io::Result<ReconcileReport> {
    reconcile_state(world_root, state_path, world_ref, bindings, attributions)
        .map(|(report, _)| report)
}

/// Reconcile a horizon and keep the reconciled state: the report every caller
/// receives, and the horizon exactly as this reconciliation left it (its
/// `reconciled_at_unix_seconds` is this reconciliation's).
///
/// The state file is rewritten only when what it records changed — a new
/// horizon, a recorded change, a changed observed source or a migrated World
/// identity. A reconciliation that finds the ground as recorded leaves the
/// file untouched, so the file's `reconciled_at_unix_seconds` is when the
/// stored horizon was last written; every report still carries the time of
/// its own reconciliation.
fn reconcile_state(
    world_root: &Path,
    state_path: &Path,
    world_ref: &str,
    bindings: Vec<SourceBinding>,
    attributions: &BTreeMap<String, SourceWriteAttribution>,
) -> io::Result<(ReconcileReport, SourceHorizonState)> {
    let _horizon = crate::source_safety::lock(world_root, SOURCE_HORIZON_LOCK)?;
    let current = observe_bindings(world_root, bindings)?;
    let now = unix_seconds();
    let existing = load_state(state_path)?;
    let initialized = existing.is_none();
    let mut state = existing.unwrap_or(SourceHorizonState {
        schema: SOURCE_HORIZON_SCHEMA.to_owned(),
        world_ref: world_ref.to_owned(),
        cursor: 0,
        sources: BTreeMap::new(),
        changes: Vec::new(),
        consumer_cursors: BTreeMap::new(),
        provider: SOURCE_HORIZON_PROVIDER.to_owned(),
        reconciled_at_unix_seconds: now,
    });
    let mut migrated = false;
    if state.world_ref != world_ref {
        if is_legacy_project_identity(&state.world_ref, world_ref) {
            // The same Project under the pre-2026-09-14 identity grammar
            // (`project_id: "project:o-i"` rendered `project:project:o-i`).
            // The manifest's rename is authored; the derived horizon follows
            // it, keeping its cursors, consumer acknowledgements and changes.
            state.world_ref = world_ref.to_owned();
            migrated = true;
        } else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "source horizon world identity changed: {} holds a horizon for {} but this \
                     World is {world_ref}. Nothing was reconciled or written. If the World was \
                     deliberately renamed, move that state file aside (it is derived, never \
                     source) and rerun `ctrl --json action run projectcentral.change.horizon` \
                     to found a fresh horizon; otherwise restore the manifest identity.",
                    state_path.display(),
                    state.world_ref
                ),
            ));
        }
    }

    let mut new_changes = Vec::new();
    if !initialized {
        let previous_refs = state.sources.keys().cloned().collect::<BTreeSet<_>>();
        let current_refs = current.keys().cloned().collect::<BTreeSet<_>>();
        let all_refs = previous_refs
            .union(&current_refs)
            .cloned()
            .collect::<Vec<_>>();
        for reference in all_refs {
            let before = state.sources.get(&reference);
            let after = current.get(&reference);
            let kind = match (before, after) {
                (None, Some(_)) => Some(SourceChangeKind::Added),
                (Some(_), None) => Some(SourceChangeKind::Removed),
                (Some(left), Some(right)) if left.revision.revision != right.revision.revision => {
                    Some(SourceChangeKind::Modified)
                }
                _ => None,
            };
            let Some(kind) = kind else { continue };
            state.cursor = state.cursor.saturating_add(1);
            let basis = after
                .or(before)
                .expect("change has a before or after source");
            let attribution = attributions.get(&reference);
            let change = SourceChange {
                schema: SOURCE_CHANGE_SCHEMA.to_owned(),
                change_ref: format!("central:change:{world_ref}:{}", state.cursor),
                cursor: state.cursor,
                world_ref: world_ref.to_owned(),
                source_ref: reference,
                source_path: basis.binding.path.clone(),
                source_roles: basis.binding.roles.clone(),
                provenance: basis.binding.provenance.clone(),
                standing: basis.binding.standing.clone(),
                treatment: basis.binding.treatment.clone(),
                agent_retrieval_allowed: basis.binding.agent_retrieval_allowed,
                before_revision: before.map(|value| value.revision.revision.clone()),
                after_revision: after.map(|value| value.revision.revision.clone()),
                kind,
                observed_at_unix_seconds: now,
                provider: SOURCE_HORIZON_PROVIDER.to_owned(),
                actor: attribution.map(|value| value.actor.clone()),
                actor_kind: attribution.map(|value| value.actor_kind.clone()),
                agent_session_ref: attribution.and_then(|value| value.agent_session_ref.clone()),
            };
            state.changes.push(change.clone());
            new_changes.push(change);
        }
    }
    let recorded =
        initialized || migrated || !new_changes.is_empty() || state.sources != current;
    state.sources = current;
    state.reconciled_at_unix_seconds = now;
    if recorded {
        write_state(state_path, &state)?;
    }
    let horizon = public_horizon(&state, None);
    Ok((
        ReconcileReport {
            initialized,
            new_changes,
            horizon,
        },
        state,
    ))
}

pub fn reconcile_project_sources(project_root: &Path) -> io::Result<ReconcileReport> {
    reconcile_project_source_writes(project_root, &BTreeMap::new())
}

/// Reconcile the Project Source Change Horizon while attributing the emitted
/// change of each named source to the owner-Action caller that produced it.
/// Attribution only attaches to the change this reconciliation observes; it is
/// never back-filled onto changes that already exist.
pub fn reconcile_project_source_writes(
    project_root: &Path,
    attributions: &BTreeMap<String, SourceWriteAttribution>,
) -> io::Result<ReconcileReport> {
    reconcile_project_state(project_root, attributions).map(|(report, _)| report)
}

fn reconcile_project_state(
    project_root: &Path,
    attributions: &BTreeMap<String, SourceWriteAttribution>,
) -> io::Result<(ReconcileReport, SourceHorizonState)> {
    let manifest = read_project_manifest(project_root)?;
    let world_ref = format!("project:{}", manifest.project_id);
    reconcile_state(
        project_root,
        &project_root.join(PROJECT_HORIZON_STATE),
        &world_ref,
        project_source_bindings(project_root)?,
        attributions,
    )
}

pub fn reconcile_control_sources(central_root: &Path) -> io::Result<ReconcileReport> {
    reconcile(
        central_root,
        &central_root.join(CONTROL_HORIZON_STATE),
        "control:root",
        control_source_bindings(central_root)?,
        &BTreeMap::new(),
    )
}

pub fn read_project_change_horizon(
    project_root: &Path,
    since: Option<u64>,
) -> io::Result<SourceHorizon> {
    // Reading the current horizon is also the correctness reconciliation path. This makes direct
    // external edits available without a manual sync command while keeping all mutation under
    // derived .central state and never invoking an Agent/model. The horizon read is the one
    // this reconciliation produced, whether or not it had anything to write.
    let (_, state) = reconcile_project_state(project_root, &BTreeMap::new())?;
    Ok(public_horizon(&state, since))
}

pub fn acknowledge_project_cursor(
    project_root: &Path,
    consumer: &str,
    cursor: u64,
) -> io::Result<SourceHorizon> {
    if consumer.trim().is_empty() || consumer != consumer.trim() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "consumer must be non-empty",
        ));
    }
    let path = project_root.join(PROJECT_HORIZON_STATE);
    let _horizon = crate::source_safety::lock(project_root, SOURCE_HORIZON_LOCK)?;
    let mut state = load_state(&path)?.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "source horizon must be reconciled before acknowledgement",
        )
    })?;
    if cursor > state.cursor {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "cannot acknowledge cursor {cursor} beyond current cursor {}",
                state.cursor
            ),
        ));
    }
    let entry = state
        .consumer_cursors
        .entry(consumer.to_owned())
        .or_insert(0);
    *entry = (*entry).max(cursor);
    write_state(&path, &state)?;
    Ok(public_horizon(&state, None))
}

pub fn compact_project_changes(project_root: &Path) -> io::Result<CompactionReport> {
    let path = project_root.join(PROJECT_HORIZON_STATE);
    let _horizon = crate::source_safety::lock(project_root, SOURCE_HORIZON_LOCK)?;
    let mut state = load_state(&path)?.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "source horizon must be reconciled before compaction",
        )
    })?;
    let before_changes = state.changes.len();
    let minimum_active_cursor = state.consumer_cursors.values().copied().min();
    if let Some(cursor) = minimum_active_cursor {
        state.changes.retain(|change| change.cursor > cursor);
        write_state(&path, &state)?;
    }
    Ok(CompactionReport {
        before_changes,
        after_changes: state.changes.len(),
        minimum_active_cursor,
    })
}

fn action_input(name: &str, required: bool) -> ActionInputDefinition {
    ActionInputDefinition {
        name: name.to_owned(),
        input_type: "string".to_owned(),
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
        mutation_class,
        preview_supported: false,
        required_ports: Vec::new(),
        availability: ActionAvailability {
            available: true,
            reason: None,
        },
    }
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

fn project_root(
    action: &str,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> Result<PathBuf, ActionResult> {
    let project = required(input, "project", action)?;
    validate_project_member(&project).map_err(|error| {
        ActionResult::failure(
            Some(action),
            ResultStatus::InvalidInput,
            error.to_string(),
            None,
        )
    })?;
    let central = resolve_central_root(context.root_options)
        .map_err(|message| {
            ActionResult::failure(Some(action), ResultStatus::InvalidInput, message, None)
        })?
        .path;
    let root = central.join("Work").join(project);
    if !root.is_dir() {
        return Err(ActionResult::failure(
            Some(action),
            ResultStatus::InvalidInput,
            format!("Project root does not exist: {}", root.display()),
            None,
        ));
    }
    Ok(root)
}

fn io_failure(action: &str, error: io::Error) -> ActionResult {
    let status = match error.kind() {
        io::ErrorKind::InvalidInput | io::ErrorKind::NotFound => ResultStatus::InvalidInput,
        io::ErrorKind::InvalidData => ResultStatus::VerificationFailure,
        _ => ResultStatus::InternalFailure,
    };
    ActionResult::failure(Some(action), status, error.to_string(), None)
}

fn horizon_action(
    _: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let action = "projectcentral.change.horizon";
    let since = input.get("cursor").and_then(Value::as_u64);
    let result = if input.get("project").is_none_or(Value::is_null) {
        let root = match resolve_central_root(context.root_options) {
            Ok(root) => root.path,
            Err(message) => {
                return ActionResult::failure(
                    Some(action),
                    ResultStatus::InvalidInput,
                    message,
                    None,
                )
            }
        };
        crate::continuous_work::source::Scope::resolve(&root, None)
            .and_then(|scope| read_control_change_horizon(&scope.root, since))
    } else {
        let root = match project_root(action, input, context) {
            Ok(root) => root,
            Err(result) => return result,
        };
        read_project_change_horizon(&root, since)
    };
    result
        .map(|value| {
            ActionResult::success(
                action,
                serde_json::to_value(value).expect("horizon serializes"),
            )
        })
        .unwrap_or_else(|error| io_failure(action, error))
}

fn reconcile_action(
    _: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let action = "projectcentral.change.reconcile";
    let root = match project_root(action, input, context) {
        Ok(root) => root,
        Err(result) => return result,
    };
    reconcile_project_sources(&root)
        .map(|value| {
            ActionResult::success(
                action,
                serde_json::to_value(value).expect("reconcile serializes"),
            )
        })
        .unwrap_or_else(|error| io_failure(action, error))
}

fn acknowledge_action(
    _: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let action = "projectcentral.change.ack";
    let root = match project_root(action, input, context) {
        Ok(root) => root,
        Err(result) => return result,
    };
    let consumer = match required(input, "consumer", action) {
        Ok(value) => value,
        Err(result) => return result,
    };
    let cursor = match input.get("cursor").and_then(Value::as_u64) {
        Some(value) => value,
        None => {
            return ActionResult::failure(
                Some(action),
                ResultStatus::InvalidInput,
                format!("{action} requires numeric cursor."),
                None,
            );
        }
    };
    acknowledge_project_cursor(&root, &consumer, cursor)
        .map(|value| {
            ActionResult::success(
                action,
                serde_json::to_value(value).expect("horizon serializes"),
            )
        })
        .unwrap_or_else(|error| io_failure(action, error))
}

pub fn register_source_horizon_actions(registry: &mut ActionRegistry) {
    let actions = [
        (
            descriptor(
                "projectcentral.change.horizon",
                "Read current Source Change Horizon",
                "Reconcile participating sources (omit project for the Central root meta-Project) into deterministic revisions and return the current change horizon. This updates derived .central state only and never invokes an Agent/model.",
                MutationClass::LocallyMutating,
                vec![action_input("project", false), action_input("cursor", false)],
                "central-source-change-horizon",
            ),
            horizon_action
                as fn(&ActionRegistry, &Value, &ActionExecutionContext<'_>) -> ActionResult,
        ),
        (
            descriptor(
                "projectcentral.change.reconcile",
                "Reconcile Project source revisions",
                "Correct watcher/startup hints by rescanning the authoritative participating source set and emitting logical changes only for revision differences.",
                MutationClass::LocallyMutating,
                vec![action_input("project", true)],
                "central-source-change-reconcile",
            ),
            reconcile_action,
        ),
        (
            descriptor(
                "projectcentral.change.ack",
                "Acknowledge Source Change cursor",
                "Advance one named consumer cursor without changing any source. Compaction may only remove changes older than every active cursor.",
                MutationClass::LocallyMutating,
                vec![
                    action_input("project", true),
                    action_input("consumer", true),
                    action_input("cursor", true),
                ],
                "central-source-change-horizon",
            ),
            acknowledge_action,
        ),
    ];
    for (descriptor, handler) in actions {
        registry
            .register(descriptor, handler)
            .expect("Source Change Horizon Action ids are valid");
    }
}

#[cfg(test)]
mod retrieval_tests {
    use super::*;

    struct Ground(PathBuf);
    impl Ground {
        fn new() -> Self {
            let scratch = Path::new(env!("CARGO_MANIFEST_DIR")).join("../ProjectCentral/now/tmp");
            fs::create_dir_all(&scratch).unwrap();
            let path = scratch.join(format!("retrieval-{}-{}-{}", std::process::id(), unix_seconds(), NEXT_STATE_TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Ground {
        fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); }
    }

    #[test]
    fn a_finder_ds_store_is_never_collected_as_source() {
        let ground = Ground::new();
        let root = &ground.0;
        let user = root.join("Control/user/notes");
        fs::create_dir_all(&user).unwrap();
        fs::write(user.join("source.md"), b"retained source").unwrap();
        fs::write(user.join(".DS_Store"), b"\0\0\0\x01Bud1").unwrap();
        fs::write(root.join("Control/user/.DS_Store"), b"\0\0\0\x01Bud1").unwrap();
        let mut files = Vec::new();
        collect_files(&root.join("Control/user"), root, 0, &mut files).unwrap();
        assert_eq!(files, vec![user.join("source.md")]);
    }

    fn paths(bindings: &BTreeMap<String, SourceBinding>) -> Vec<String> {
        bindings.values().map(|binding| binding.path.clone()).collect()
    }

    #[test]
    fn the_pool_admits_text_and_leaves_out_bytes_and_tool_state() {
        let ground = Ground::new();
        let root = &ground.0;
        for (relative, bytes) in [
            ("notes/idea.md", &b"text"[..]),
            ("src/tool.py", b"print()"),
            ("Makefile", b"all:"),
            ("notes/photo.png", b"\x89PNG"),
            ("notes/SHOUT.PNG", b"\x89PNG"),
            ("src/__pycache__/tool.cpython-313.pyc", b"\0"),
            ("src/tool.pyc", b"\0"),
            ("deck/pitch.pptx", b"PK"),
            ("deck/brief.pdf", b"%PDF"),
            ("media/take.mp4", b"\0"),
            (".gitnexus/lbug", b"\0"),
            (".gitnexus/parse-cache/a.json", b"{}"),
            (".pnpm-store/v11/files/ab/exec", b"\0"),
            ("vault/.obsidian/plugins/x/main.js", b"//"),
            ("app/.svelte-kit/output/a.js", b"//"),
            ("app/DerivedData/Build/log.txt", b"x"),
        ] {
            let path = root.join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, bytes).unwrap();
        }
        let mut pooled = BTreeMap::new();
        let counted = insert_pool_bindings(root, "project:p", &[], &mut pooled).unwrap();
        let mut kept = paths(&pooled);
        kept.sort();
        assert_eq!(kept, vec!["Makefile", "notes/idea.md", "src/tool.py"]);
        assert_eq!(counted, 3, "the count a pool reports is the roster it builds");
    }

    #[test]
    fn the_text_rule_is_the_pool_s_alone_an_authored_tree_keeps_what_it_holds() {
        let ground = Ground::new();
        let root = &ground.0;
        let tree = root.join("docs");
        fs::create_dir_all(&tree).unwrap();
        fs::write(tree.join("spec.md"), b"text").unwrap();
        fs::write(tree.join("figure.png"), b"\x89PNG").unwrap();
        let mut bindings = BTreeMap::new();
        insert_tree_bindings(
            root, &tree, "project:p", &["docs"], "authored", "unspecified", "retain",
            &[], &mut bindings,
        )
        .unwrap();
        let mut kept = paths(&bindings);
        kept.sort();
        assert_eq!(kept, vec!["docs/figure.png", "docs/spec.md"]);
    }

    #[test]
    fn a_declared_exclusion_still_narrows_the_pool_further() {
        let ground = Ground::new();
        let root = &ground.0;
        fs::create_dir_all(root.join("a")).unwrap();
        fs::create_dir_all(root.join("b")).unwrap();
        fs::write(root.join("a/one.md"), b"x").unwrap();
        fs::write(root.join("b/two.md"), b"x").unwrap();
        let mut pooled = BTreeMap::new();
        insert_pool_bindings(root, "project:p", &["b".to_owned()], &mut pooled).unwrap();
        assert_eq!(paths(&pooled), vec!["a/one.md"]);
    }

    #[test]
    fn actual_root_ancestor_directory_self_and_dummy_member_share_one_admission() {
        let ground = Ground::new();
        let root = &ground.0;
        let private = root.join("Control/user/private");
        fs::create_dir_all(&private).unwrap();
        fs::write(private.join("source.md"), b"retained source").unwrap();
        for directory in [root.to_path_buf(), root.join("Control"), root.join("Control/user"), private.clone()] {
            let marker = directory.join(AGENT_RETRIEVAL_DENY_MARKER);
            fs::write(&marker, b"").unwrap();
            assert!(!retrieval_admission(root, &private).unwrap());
            assert!(!retrieval_admission(root, &private.join("source.md")).unwrap());
            assert!(!retrieval_admission(root, &private.join("new-member")).unwrap());
            fs::remove_file(marker).unwrap();
            assert!(retrieval_admission(root, &private).unwrap());
            assert!(retrieval_admission(root, &private.join("new-member")).unwrap());
        }
        assert_eq!(fs::read(private.join("source.md")).unwrap(), b"retained source");
    }

    #[test]
    fn actual_root_alias_is_supported_but_member_and_marker_redirection_refuse() {
        use std::os::unix::fs::symlink;
        let ground = Ground::new();
        let root = ground.0.join("root");
        fs::create_dir(&root).unwrap();
        fs::write(root.join("source.md"), b"unchanged").unwrap();
        let alias = ground.0.join("alias");
        symlink(&root, &alias).unwrap();
        assert!(retrieval_admission(&alias, &alias.join("source.md")).unwrap());
        symlink(root.join("source.md"), root.join("redirected")).unwrap();
        assert_eq!(retrieval_admission(&root, &root.join("redirected")).unwrap_err().kind(), io::ErrorKind::PermissionDenied);
        symlink(root.join("source.md"), root.join(AGENT_RETRIEVAL_DENY_MARKER)).unwrap();
        assert_eq!(retrieval_admission(&root, &root.join("source.md")).unwrap_err().kind(), io::ErrorKind::PermissionDenied);
        assert!(!retrieval_allowed(&root, &root.join("source.md")));
        assert_eq!(fs::read(root.join("source.md")).unwrap(), b"unchanged");
    }

    #[test]
    fn actual_marker_stat_permission_error_is_not_an_absent_marker() {
        use std::os::unix::fs::PermissionsExt;
        if unsafe { libc::geteuid() } == 0 {
            eprintln!("qualification unavailable: marker-stat EACCES requires a nonroot OS user");
            return;
        }
        let ground = Ground::new();
        let directory = ground.0.join("blocked");
        fs::create_dir(&directory).unwrap();
        struct Restore(PathBuf);
        impl Drop for Restore {
            fn drop(&mut self) { let _ = fs::set_permissions(&self.0, fs::Permissions::from_mode(0o700)); }
        }
        let _restore = Restore(directory.clone());
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o000)).unwrap();
        let actual = fs::symlink_metadata(directory.join(AGENT_RETRIEVAL_DENY_MARKER)).unwrap_err();
        let error = retrieval_admission(&ground.0, &directory).unwrap_err();
        assert_eq!(actual.kind(), io::ErrorKind::PermissionDenied);
        assert_eq!(error.kind(), actual.kind());
        assert_eq!(error.raw_os_error(), actual.raw_os_error());
        assert!(!retrieval_allowed(&ground.0, &directory));
    }


    fn declared_fixture_member(reference: &str, path: &str, role: &str) -> SourceBinding {
        SourceBinding {
            source_ref: reference.to_owned(), path: path.to_owned(), roles: vec![role.to_owned()],
            provenance: "agent-maintained".to_owned(), standing: "durable-source".to_owned(),
            treatment: "retain-native-fixture".to_owned(), agent_retrieval_allowed: true,
        }
    }

    fn bind_fixture_member(scope: &crate::continuous_work::source::Scope, binding: &SourceBinding) {
        let _mutation = crate::source_safety::lock(&scope.root, "source-mutation.lock").unwrap();
        scope.bind(binding, 1).unwrap();
    }

    fn retained_fixture_file(path: &Path) -> (Vec<u8>, u64, u64, SystemTime) {
        use std::os::unix::fs::MetadataExt;
        let metadata = fs::symlink_metadata(path).unwrap();
        (fs::read(path).unwrap(), metadata.dev(), metadata.ino(), metadata.modified().unwrap())
    }

    fn one_fixture_member<'a>(bindings: &'a [SourceBinding], path: &str) -> &'a SourceBinding {
        let key = crate::source_safety::normal_member_key(path).unwrap();
        let members = bindings.iter().filter(|binding|
            crate::source_safety::normal_member_key(&binding.path).unwrap() == key).collect::<Vec<_>>();
        assert_eq!(members.len(), 1, "one current Source identity per declared member: {path}");
        members[0]
    }

    #[test]
    fn explicit_personal_and_machine_sources_do_not_parse_superseded_malformed_skill_manifests() {
        let ground = Ground::new();
        crate::initialize_central(&ground.0).unwrap();
        let scope = crate::continuous_work::source::Scope::resolve(&ground.0, None).unwrap();
        let mut expected = Vec::new();
        let mut retained = Vec::new();
        for (index, base) in ["Control/user/skills/Owned", "Control/machines/native-fixture/skills/Owned"].iter().enumerate() {
            fs::create_dir_all(ground.0.join(base)).unwrap();
            for (member, content) in [("SKILL.md", "explicit native body"), ("skill.json", "{not-json")] {
                let actual = format!("{base}/{member}");
                let literal = format!("{base}//./{member}");
                fs::write(ground.0.join(&actual), content).unwrap();
                let binding = declared_fixture_member(&format!("fixture:explicit:{index}:{member}"), &literal, "selected-native-source");
                bind_fixture_member(&scope, &binding);
                retained.push((actual, retained_fixture_file(&ground.0.join(&literal))));
                expected.push((binding, content));
            }
        }
        let sibling = "Control/user/skills/Open/SKILL.md";
        fs::create_dir_all(ground.0.join("Control/user/skills/Open")).unwrap();
        fs::write(ground.0.join(sibling), "unbound useful sibling").unwrap();
        let relation_before = retained_fixture_file(&ground.0.join(CONTROL_GROUND_RELATIONS_SOURCE));
        let bindings = control_source_bindings(&ground.0).unwrap();
        for (binding, content) in expected {
            assert_eq!(one_fixture_member(&bindings, &binding.path), &binding);
            let reading = scope.read(&binding.source_ref).unwrap();
            assert_eq!(reading.source, binding);
            assert_eq!(reading.content, content);
            assert_eq!(reading.revision.revision, crate::source_safety::content_revision_bytes(content.as_bytes()));
        }
        let open = one_fixture_member(&bindings, sibling);
        assert_eq!(open.roles, vec!["skill-source"]);
        assert_eq!(open.treatment, crate::control_skills::CONTROL_SKILL_TREATMENT);
        assert_eq!(open.provenance, "unresolved");
        assert_eq!(open.standing, "unspecified");
        assert_eq!(scope.read(&open.source_ref).unwrap().content, "unbound useful sibling");
        let report = reconcile_control_sources(&ground.0).unwrap();
        assert!(report.horizon.sources.iter().any(|source| source.binding == *open));
        for (path, before) in retained {
            assert_eq!(retained_fixture_file(&ground.0.join(path)), before);
        }
        assert_eq!(retained_fixture_file(&ground.0.join(CONTROL_GROUND_RELATIONS_SOURCE)), relation_before);
    }

    #[test]
    fn partial_and_unoverridden_skill_members_keep_native_manifest_failures() {
        let ground = Ground::new();
        crate::initialize_central(&ground.0).unwrap();
        let scope = crate::continuous_work::source::Scope::resolve(&ground.0, None).unwrap();
        let base = "Control/user/skills/Partial";
        fs::create_dir_all(ground.0.join(base)).unwrap();
        for (member, content) in [("SKILL.md", "selected body"), ("skill.json", "{malformed"), ("resource.txt", "remaining fallback")] {
            let path = format!("{base}/{member}");
            fs::write(ground.0.join(&path), content).unwrap();
            if member != "resource.txt" {
                bind_fixture_member(&scope, &declared_fixture_member(&format!("fixture:partial:{member}"), &path, "selected-native-source"));
            }
        }
        let malformed_before = retained_fixture_file(&ground.0.join(format!("{base}/skill.json")));
        assert_eq!(control_source_bindings(&ground.0).unwrap_err().kind(), io::ErrorKind::InvalidData);
        let resource = declared_fixture_member("fixture:partial:resource", &format!("{base}//resource.txt"), "selected-native-source");
        bind_fixture_member(&scope, &resource);
        assert_eq!(one_fixture_member(&control_source_bindings(&ground.0).unwrap(), &resource.path), &resource);
        let unrelated = ground.0.join("Control/user/skills/Unrelated");
        fs::create_dir(&unrelated).unwrap();
        fs::write(unrelated.join("SKILL.md"), "genuine unoverridden body").unwrap();
        fs::write(unrelated.join("skill.json"), "{another malformed manifest").unwrap();
        assert_eq!(control_source_bindings(&ground.0).unwrap_err().kind(), io::ErrorKind::InvalidData);
        assert_eq!(retained_fixture_file(&ground.0.join(format!("{base}/skill.json"))), malformed_before);
        assert_eq!(fs::read_to_string(unrelated.join("skill.json")).unwrap(), "{another malformed manifest");
    }

    #[test]
    fn explicit_project_skill_sources_preserve_literal_identity_and_current_read() {
        let ground = Ground::new();
        crate::initialize_central(&ground.0).unwrap();
        let project = ground.0.join("Work/Owned");
        fs::create_dir(&project).unwrap();
        crate::initialize_projectcentral(&ground.0, &project, "opaque/project").unwrap();
        let scope = crate::continuous_work::source::Scope::resolve(&ground.0, Some("Owned")).unwrap();
        assert_eq!(scope.world_ref, "project:opaque/project");
        let base = "ProjectCentral/user/skills/Owned";
        fs::create_dir_all(project.join(base)).unwrap();
        let mut selected = Vec::new();
        for (member, content) in [("SKILL.md", "project selected body"), ("skill.json", "{native malformed fixture")] {
            let literal = format!("{base}//./{member}");
            fs::write(project.join(&literal), content).unwrap();
            let binding = declared_fixture_member(&format!("fixture:project:{member}"), &literal, "selected-project-source");
            bind_fixture_member(&scope, &binding);
            selected.push((binding, retained_fixture_file(&project.join(&literal))));
        }
        let manifest_before = retained_fixture_file(&project.join("ProjectCentral/project.json"));
        let wiki_before = retained_fixture_file(&project.join(WIKI_SOURCE));
        let relations_before = retained_fixture_file(&project.join(GROUND_RELATIONS_SOURCE));
        let report = reconcile_project_sources(&project).unwrap();
        let bindings = scope.bindings().unwrap();
        for (binding, before) in selected {
            assert_eq!(one_fixture_member(&bindings, &binding.path), &binding);
            let reading = scope.read(&binding.source_ref).unwrap();
            assert_eq!(reading.source, binding);
            assert_eq!(reading.content.as_bytes(), before.0);
            assert!(report.horizon.sources.iter().any(|source| source.binding == binding));
            assert_eq!(retained_fixture_file(&project.join(&binding.path)), before);
        }
        assert_eq!(retained_fixture_file(&project.join("ProjectCentral/project.json")), manifest_before);
        assert_eq!(retained_fixture_file(&project.join(WIKI_SOURCE)), wiki_before);
        assert_eq!(retained_fixture_file(&project.join(GROUND_RELATIONS_SOURCE)), relations_before);
    }

    #[test]
    fn native_adopted_wiki_fallback_yields_to_complete_literal_source_relation() {
        for (index, literal) in ["docs/wiki.json", "docs//./wiki.json"].iter().enumerate() {
            let ground = Ground::new();
            crate::initialize_central(&ground.0).unwrap();
            let project = ground.0.join("Work/Adopted");
            fs::create_dir(&project).unwrap();
            fs::create_dir(project.join("docs")).unwrap();
            let source = project.join("docs/wiki.json");
            let wiki = serde_json::to_vec(&serde_json::json!({"objects":[{
                "profile":crate::projectcentral::WIKI_PROFILE,"object":"space","ref":"fixture:space:adopted",
                "revision":1,"provenance":[],"parent_space_refs":[],"child_space_refs":[],"node_refs":[]
            }]})).unwrap();
            fs::write(&source, &wiki).unwrap();
            let adopted_before = retained_fixture_file(&source);
            let receipt = crate::adopt_in_place(&ground.0, &project, "opaque/adopted", "docs/wiki.json").unwrap();
            assert_eq!(receipt.adopted_sources, vec!["docs/wiki.json"]);
            assert_eq!(retained_fixture_file(&source), adopted_before);
            let initial = reconcile_project_sources(&project).unwrap();
            let generated = source_ref("project:opaque/adopted", "docs/wiki.json");
            let initial_bindings = project_source_bindings(&project).unwrap();
            let fallback = one_fixture_member(&initial_bindings, literal);
            assert_eq!(fallback.source_ref, generated);
            assert_eq!(fallback.roles, vec!["adopted-agent-wiki-source"]);
            assert_eq!(fallback.provenance, "unresolved");
            assert_eq!(fallback.standing, "unspecified");
            assert_eq!(fallback.treatment, "retain-native-in-place");
            assert!(initial.horizon.sources.iter().any(|source| source.binding == *fallback));
            let manifest_before = retained_fixture_file(&project.join("ProjectCentral/project.json"));
            let canonical_before = retained_fixture_file(&project.join(WIKI_SOURCE));
            let scope = crate::continuous_work::source::Scope::resolve(&ground.0, Some("Adopted")).unwrap();
            let declaration = declared_fixture_member(&format!("fixture:accepted:adopted:{index}"), literal, "adopted-agent-wiki-source");
            bind_fixture_member(&scope, &declaration);
            let relations_before = retained_fixture_file(&project.join(GROUND_RELATIONS_SOURCE));
            let bindings = project_source_bindings(&project).unwrap();
            assert_eq!(one_fixture_member(&bindings, literal), &declaration);
            assert!(!bindings.iter().any(|binding| binding.source_ref == generated));
            let reading = scope.read(&declaration.source_ref).unwrap();
            assert_eq!(reading.source, declaration);
            assert_eq!(reading.content.as_bytes(), wiki);
            let reconciled = reconcile_project_sources(&project).unwrap();
            assert_eq!(one_fixture_member(&reconciled.horizon.sources.iter().map(|source| source.binding.clone()).collect::<Vec<_>>(), literal), &declaration);
            assert!(reconciled.new_changes.iter().any(|change| change.source_ref == generated && change.kind == SourceChangeKind::Removed));
            assert!(reconciled.new_changes.iter().any(|change| change.source_ref == declaration.source_ref && change.kind == SourceChangeKind::Added));
            fs::write(&source, format!("{}\n", String::from_utf8(wiki).unwrap())).unwrap();
            let changed = reconcile_project_sources(&project).unwrap();
            assert_eq!(changed.new_changes.len(), 1);
            assert_eq!(changed.new_changes[0].source_ref, declaration.source_ref);
            assert_eq!(changed.new_changes[0].source_path, *literal);
            assert_eq!(changed.new_changes[0].kind, SourceChangeKind::Modified);
            assert!(changed.horizon.changes.iter().any(|change| change.source_ref == generated && change.kind == SourceChangeKind::Removed));
            assert!(!changed.horizon.sources.iter().any(|source| source.binding.source_ref == generated));
            fs::write(&source, &adopted_before.0).unwrap();
            assert_eq!(retained_fixture_file(&project.join("ProjectCentral/project.json")), manifest_before);
            assert_eq!(retained_fixture_file(&project.join(WIKI_SOURCE)), canonical_before);
            assert_eq!(retained_fixture_file(&project.join(GROUND_RELATIONS_SOURCE)), relations_before);
            assert_eq!(fs::read(&source).unwrap(), adopted_before.0);
            use std::os::unix::fs::MetadataExt;
            assert_eq!((fs::metadata(&source).unwrap().dev(), fs::metadata(&source).unwrap().ino()), (adopted_before.1, adopted_before.2));
        }
    }


    #[test]
    fn creation_aperture_admits_only_missing_suffix_without_claiming_current_material() {
        use std::os::unix::fs::symlink;
        let ground = Ground::new();
        let root = ground.0.join("owner");
        fs::create_dir(&root).unwrap();
        fs::create_dir(root.join("existing")).unwrap();
        let destination = root.join("existing/new/child/source.md");
        assert!(retrieval_creation_admission(&root, &destination).unwrap());
        assert_eq!(retrieval_admission(&root, &destination).unwrap_err().kind(), io::ErrorKind::NotFound);
        assert!(!root.join("existing/new").exists());
        assert!(retrieval_admission(&root, &root.join("existing/source.md")).unwrap());
        let alias = ground.0.join("owner-alias");
        symlink(&root, &alias).unwrap();
        assert!(retrieval_creation_admission(&alias, &alias.join("existing/new/child/source.md")).unwrap());
        fs::write(root.join("existing/.no-agent-retrieval"), b"").unwrap();
        assert!(!retrieval_creation_admission(&root, &destination).unwrap());
        assert!(!retrieval_creation_admission(&alias, &alias.join("existing/new/child/source.md")).unwrap());
        fs::remove_file(root.join("existing/.no-agent-retrieval")).unwrap();
        assert!(retrieval_creation_admission(&root, &destination).unwrap());
        assert!(!destination.exists());
    }

    #[test]
    fn creation_aperture_retains_current_marker_and_physical_form_refusals() {
        use std::os::unix::fs::symlink;
        let ground = Ground::new();
        let root = &ground.0;
        let original = root.join("original.md");
        fs::write(&original, b"retained original source").unwrap();
        let retained = retained_fixture_file(&original);
        fs::create_dir(root.join("ordinary")).unwrap();
        symlink(root.join("ordinary"), root.join("redirected-parent")).unwrap();
        symlink(&original, root.join("redirected-source")).unwrap();
        for destination in [root.join("redirected-parent/new/source.md"), root.join("redirected-source")] {
            assert_eq!(retrieval_creation_admission(root, &destination).unwrap_err().kind(), io::ErrorKind::PermissionDenied);
        }
        assert_eq!(retrieval_creation_admission(root, &root.join("original.md/child")).unwrap_err().kind(), io::ErrorKind::InvalidInput);
        symlink(&original, root.join(AGENT_RETRIEVAL_DENY_MARKER)).unwrap();
        assert_eq!(retrieval_creation_admission(root, &root.join("missing/child")).unwrap_err().kind(), io::ErrorKind::PermissionDenied);
        assert_eq!(retained_fixture_file(&original), retained);
        assert!(!root.join("missing").exists());
    }


    #[test]
    fn actual_creation_root_alias_retarget_cannot_acknowledge_old_owner_aperture() {
        use std::os::unix::fs::symlink;
        let ground = Ground::new();
        let first = ground.0.join("first");
        let other = ground.0.join("other");
        fs::create_dir(&first).unwrap();
        fs::create_dir(&other).unwrap();
        fs::write(first.join("retained.md"), b"first retained source").unwrap();
        fs::write(other.join("retained.md"), b"other retained source").unwrap();
        let first_before = retained_fixture_file(&first.join("retained.md"));
        let other_before = retained_fixture_file(&other.join("retained.md"));
        let alias = ground.0.join("alias");
        symlink(&first, &alias).unwrap();
        let requested = alias.join("missing/child/source.md");
        let moved_alias = alias.clone();
        let moved_target = other.clone();
        AFTER_CREATION_ROOT_CAPTURE.with(|checkpoint| *checkpoint.borrow_mut() = Some(Box::new(move || {
            fs::remove_file(&moved_alias).unwrap();
            symlink(&moved_target, &moved_alias).unwrap();
        })));
        assert_eq!(retrieval_creation_admission(&alias, &requested).unwrap_err().kind(), io::ErrorKind::Other);
        assert_eq!(retained_fixture_file(&first.join("retained.md")), first_before);
        assert_eq!(retained_fixture_file(&other.join("retained.md")), other_before);
        assert!(!first.join("missing").exists());
        assert!(!other.join("missing").exists());
        // A fresh physical observation has its own admitted root basis; this
        // does not adopt a Source identity or grant a writer authority.
        assert!(retrieval_creation_admission(&alias, &requested).unwrap());
    }
}

#[cfg(test)]
mod now_record_tests {
    use super::*;

    struct Ground(PathBuf);
    impl Ground {
        fn new() -> Self {
            let scratch = Path::new(env!("CARGO_MANIFEST_DIR")).join("../ProjectCentral/now/tmp");
            fs::create_dir_all(&scratch).unwrap();
            let path = scratch.join(format!(
                "now-records-{}-{}-{}",
                std::process::id(),
                unix_seconds(),
                NEXT_STATE_TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
        fn file(&self, relative: &str) {
            let path = self.0.join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, b"{}").unwrap();
        }
        fn paths(&self, world_ref: &str) -> Vec<String> {
            let mut paths: Vec<String> = now_record_bindings(&self.0, world_ref)
                .unwrap()
                .into_iter()
                .map(|binding| binding.path)
                .collect();
            paths.sort();
            paths
        }
    }
    impl Drop for Ground {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn root_records_are_the_shallow_files_of_each_family_and_nothing_deeper() {
        let ground = Ground::new();
        for record in [
            "agents/handoff.json",
            "agents/note.md",
            "day/2026-10-01.md",
            "flows/flow.md",
            "flows/flow.html",
            "clearings/c1/now.json",
            "clearings/c2/now.json",
        ] {
            ground.file(&format!("Control/agents/now/{record}"));
        }
        for not_record in [
            "agents/.hidden.json",
            "agents/session.lock",
            "agents/binary.bin",
            "agents/sub/deep.json",
            "day/2026-10-01.sources/evidence.json",
            "flows/evidence/dir/x.json",
            "clearings/c1/T/plan.md",
            "clearings/c3/notes.md",
            "tmp/scratch.json",
            "preserved/old.json",
            "user/u.md",
            "README.md",
        ] {
            ground.file(&format!("Control/agents/now/{not_record}"));
        }
        let expected: Vec<String> = [
            "agents/handoff.json",
            "agents/note.md",
            "clearings/c1/now.json",
            "clearings/c2/now.json",
            "day/2026-10-01.md",
            "flows/flow.html",
            "flows/flow.md",
        ]
        .iter()
        .map(|record| format!("Control/agents/now/{record}"))
        .collect();
        assert_eq!(ground.paths(CONTROL_WORLD_REF), expected);
    }

    #[test]
    fn a_project_register_uses_its_own_now_directory_and_world() {
        let ground = Ground::new();
        ground.file("ProjectCentral/now/agents/h.json");
        ground.file("ProjectCentral/now/day/d.md");
        ground.file("ProjectCentral/now/tmp/t.json");
        ground.file("ProjectCentral/now/day/d.sources/e.json");
        ground.file("Control/agents/now/agents/root-only.json");
        assert_eq!(
            ground.paths("project:Demo"),
            vec![
                "ProjectCentral/now/agents/h.json".to_owned(),
                "ProjectCentral/now/day/d.md".to_owned()
            ]
        );
        let bindings = now_record_bindings(&ground.0, "project:Demo").unwrap();
        assert!(bindings
            .iter()
            .all(|b| b.source_ref.starts_with("central:source:project:Demo:")));
        assert!(bindings
            .iter()
            .all(|b| b.roles == ["now-record"] && b.agent_retrieval_allowed));
    }

    #[test]
    fn a_pruned_record_leaves_the_roster_and_no_register_is_needed() {
        let ground = Ground::new();
        ground.file("ProjectCentral/now/agents/a.json");
        ground.file("ProjectCentral/now/agents/b.json");
        assert_eq!(ground.paths("project:Demo").len(), 2);
        fs::remove_file(ground.0.join("ProjectCentral/now/agents/a.json")).unwrap();
        assert_eq!(
            ground.paths("project:Demo"),
            vec!["ProjectCentral/now/agents/b.json".to_owned()]
        );
        fs::remove_dir_all(ground.0.join("ProjectCentral/now")).unwrap();
        assert!(ground.paths("project:Demo").is_empty());
    }

    #[test]
    fn symlinks_never_become_records_and_the_deny_marker_withholds_a_family() {
        use std::os::unix::fs::symlink;
        let ground = Ground::new();
        ground.file("ProjectCentral/now/agents/real.json");
        ground.file("elsewhere/target.json");
        symlink(
            ground.0.join("elsewhere/target.json"),
            ground.0.join("ProjectCentral/now/agents/link.json"),
        )
        .unwrap();
        assert_eq!(
            ground.paths("project:Demo"),
            vec!["ProjectCentral/now/agents/real.json".to_owned()]
        );
        fs::write(
            ground.0.join("ProjectCentral/now/agents").join(AGENT_RETRIEVAL_DENY_MARKER),
            b"",
        )
        .unwrap();
        let bindings = now_record_bindings(&ground.0, "project:Demo").unwrap();
        assert_eq!(bindings.len(), 1, "the marker file itself is not a record");
        assert!(
            !bindings[0].agent_retrieval_allowed,
            "the owner's deny marker withholds the whole family"
        );
    }
}

#[cfg(test)]
mod member_index_tests {
    use super::*;

    fn binding(reference: &str, path: &str) -> SourceBinding {
        SourceBinding {
            source_ref: reference.to_owned(),
            path: path.to_owned(),
            roles: vec![],
            provenance: "unresolved".into(),
            standing: "unspecified".into(),
            treatment: "test".into(),
            agent_retrieval_allowed: true,
        }
    }

    /// The scan the index replaced, verbatim: every relation re-keys every binding.
    fn replace_by_scan(bindings: &mut BTreeMap<String, SourceBinding>, relative: &str, reference: &str) -> io::Result<()> {
        let member_key = crate::source_safety::normal_member_key(relative)?;
        let mut replaced = Vec::new();
        for (existing, binding) in bindings.iter() {
            if crate::source_safety::normal_member_key(&binding.path)? == member_key {
                replaced.push(existing.clone());
            }
        }
        for existing in replaced { bindings.remove(&existing); }
        bindings.insert(reference.to_owned(), binding(reference, relative));
        Ok(())
    }

    fn replace_by_index(index: &mut MemberIndex, bindings: &mut BTreeMap<String, SourceBinding>, relative: &str, reference: &str) -> io::Result<()> {
        let member_key = crate::source_safety::normal_member_key(relative)?;
        index.remove_members(bindings, &member_key)?;
        index.inserted(member_key, reference);
        bindings.insert(reference.to_owned(), binding(reference, relative));
        Ok(())
    }

    fn tree(paths: &[(&str, &str)]) -> BTreeMap<String, SourceBinding> {
        paths.iter().map(|(reference, path)| ((*reference).to_owned(), binding(reference, path))).collect()
    }

    /// The same relations, in the same order, over the same tree: the same bindings, or the same first error.
    fn both(start: &[(&str, &str)], relations: &[(&str, &str)]) {
        let (mut scanned, mut indexed) = (tree(start), tree(start));
        let mut index = MemberIndex::default();
        for (relative, reference) in relations {
            let by_scan = replace_by_scan(&mut scanned, relative, reference);
            let by_index = replace_by_index(&mut index, &mut indexed, relative, reference);
            assert_eq!(by_scan.is_ok(), by_index.is_ok(), "{relative} -> {reference}: one refused, the other did not");
            if let (Err(a), Err(b)) = (&by_scan, &by_index) { assert_eq!(a.kind(), b.kind()); return; }
            assert_eq!(scanned, indexed, "after {relative} -> {reference}");
        }
    }

    #[test]
    fn the_index_replaces_exactly_what_the_scan_replaced() {
        let fallback = [("c:a", "Control/user/a.md"), ("c:b", "Control/user/b.md"), ("c:b2", "Control/user/./b.md"), ("c:z", "Control/z.md")];
        // A relation names a path the tree already binds (once, and twice through a spelling of it).
        both(&fallback, &[("Control/user/a.md", "r:a")]);
        both(&fallback, &[("Control/user/b.md", "r:b")]);
        // A relation for a path nothing binds; then one for it again under another ref (the second replaces the first).
        both(&fallback, &[("Control/new.md", "r:n1"), ("Control/new.md", "r:n2")]);
        // A later relation overwrites a ref that stood for another path: the stale hit must not remove the new binding.
        both(&fallback, &[("Control/user/a.md", "c:z"), ("Control/z.md", "r:z"), ("Control/user/a.md", "r:a2")]);
        // The ref a relation takes is one the tree bound to the same member.
        both(&fallback, &[("Control/user/a.md", "c:a"), ("Control/user/a.md", "c:a")]);
        // Several relations in one pass, mixing all of it.
        both(&fallback, &[("Control/user/b.md", "r:b"), ("Control/user/a.md", "c:b"), ("Control/user/b.md", "r:b3"), ("Control/z.md", "r:z")]);
        // No relations, and no bindings.
        both(&fallback, &[]);
        both(&[], &[("Control/x.md", "r:x"), ("Control/x.md", "r:y")]);
    }

    #[test]
    fn a_binding_whose_path_will_not_key_fails_the_walk_where_the_scan_did() {
        // The scan never keyed a binding when there was no relation to compare; the index is built only when the first one needs it.
        let bad = [("c:ok", "Control/ok.md"), ("c:bad", "../outside.md")];
        let mut index = MemberIndex::default();
        let mut bindings = tree(&bad);
        let before = bindings.clone();
        assert!(index.0.is_none() && bindings == before, "nothing is keyed before a relation asks");
        assert!(replace_by_scan(&mut tree(&bad), "Control/ok.md", "r:ok").is_err());
        assert!(replace_by_index(&mut index, &mut bindings, "Control/ok.md", "r:ok").is_err());
        assert_eq!(bindings, before, "a refusal leaves the bindings as they stood");
    }
}

#[cfg(test)]
mod scan_memo_tests {
    use super::*;
    use std::os::unix::fs::symlink;

    /// A scratch ground outside the repository, removed on drop. Its path goes through a symlink where the platform has one
    /// (macOS `/var` → `/private/var`), so the world root is not its own canonical path.
    struct Ground(PathBuf);
    impl Ground {
        fn new(tag: &str) -> Self {
            let path = std::env::temp_dir().join(format!("central-scan-memo-{tag}-{}-{}", std::process::id(), unix_seconds()));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
        fn write(&self, relative: &str, content: &str) {
            let path = self.0.join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, content).unwrap();
        }
    }
    impl Drop for Ground {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    /// Every kind of thing a scan can meet.
    fn anomalous(tag: &str) -> (Ground, PathBuf) {
        let ground = Ground::new(tag);
        let world = ground.0.join("world");
        fs::create_dir_all(&world).unwrap();
        let put = |relative: &str| {
            let path = world.join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, relative).unwrap();
        };
        put("Control/user/a.md");
        put("Control/user/sub/b.md");
        put("Control/user/sub/deeper/c.md");
        put("Control/user/.DS_Store");
        // A deny marker in the middle of a branch: it withholds that directory and everything below it.
        put("Control/user/denied/.no-agent-retrieval");
        put("Control/user/denied/d.md");
        put("Control/user/denied/below/e.md");
        // A marker that is a directory, not a regular file: a refusal, never silently open.
        fs::create_dir_all(world.join("Control/user/oddmarker/.no-agent-retrieval")).unwrap();
        put("Control/user/oddmarker/f.md");
        // Redirections: a symlinked file, a symlinked directory inside the world, one that leaves it.
        symlink(world.join("Control/user/a.md"), world.join("Control/user/link.md")).unwrap();
        symlink(world.join("Control/user/sub"), world.join("Control/user/linkdir")).unwrap();
        fs::create_dir_all(ground.0.join("outside")).unwrap();
        fs::write(ground.0.join("outside/out.md"), "outside").unwrap();
        symlink(ground.0.join("outside"), world.join("Control/escape")).unwrap();
        // A marker at the root of a second world, to be checked on its own below.
        put("Control/agents/now/x.json");
        (ground, world)
    }

    /// The members the scan may ask about: everything under the world, symlinks and directories included.
    fn everything(world: &Path) -> Vec<PathBuf> {
        fn walk(directory: &Path, out: &mut Vec<PathBuf>) {
            let mut entries: Vec<_> = fs::read_dir(directory).unwrap().map(|entry| entry.unwrap().path()).collect();
            entries.sort();
            for path in entries {
                out.push(path.clone());
                if path.is_dir() && !fs::symlink_metadata(&path).unwrap().file_type().is_symlink() {
                    walk(&path, out);
                }
            }
        }
        let mut out = vec![world.to_path_buf()];
        walk(world, &mut out);
        out
    }

    #[test]
    fn the_memo_answers_what_the_unmemoised_functions_answer() {
        let (_ground, world) = anomalous("answers");
        let mut memo = ScanMemo::new(&world);
        for path in everything(&world) {
            // Twice: the second answer comes from what the first learned, and must not change.
            for round in 0..2 {
                assert_eq!(
                    memo.retrieval_allowed(&path),
                    retrieval_allowed(&world, &path),
                    "retrieval_allowed {} (round {round})",
                    path.display()
                );
                let by_memo = memo.safe_regular_file(&path);
                let by_function = safe_regular_file(&world, &path);
                assert_eq!(by_memo.is_ok(), by_function.is_ok(), "safe_regular_file {} (round {round})", path.display());
                if let (Ok(a), Ok(b)) = (&by_memo, &by_function) {
                    assert_eq!(a, b, "safe_regular_file {} (round {round})", path.display());
                }
            }
        }
        // Spot-check that the fixture means what it says, so the equality above is not two copies of one wrong answer.
        assert!(memo.retrieval_allowed(&world.join("Control/user/a.md")));
        assert!(memo.retrieval_allowed(&world.join("Control/user/sub/deeper/c.md")));
        assert!(!memo.retrieval_allowed(&world.join("Control/user/denied/d.md")), "a deny marker withholds its directory");
        assert!(!memo.retrieval_allowed(&world.join("Control/user/denied/below/e.md")), "and everything below it");
        assert!(!memo.retrieval_allowed(&world.join("Control/user/oddmarker/f.md")), "a marker that is not a regular file refuses");
        assert!(!memo.retrieval_allowed(&world.join("Control/user/link.md")), "a symlinked file is redirected material");
        assert!(!memo.safe_regular_file(&world.join("Control/user/link.md")).unwrap());
        assert!(!memo.safe_regular_file(&world.join("Control/escape/out.md")).unwrap_or(false), "a file reached through a link out of the world is not safe");
    }

    #[test]
    fn the_memo_collects_the_files_the_original_scan_collected() {
        let (_ground, world) = anomalous("collect");
        for scan_root in [world.join("Control/user"), world.join("Control"), world.join("Control/agents"), world.clone()] {
            let mut by_memo = Vec::new();
            ScanMemo::new(&world).collect_files(&scan_root, 0, &mut by_memo).unwrap();
            let mut by_original = Vec::new();
            collect_files_reference(&scan_root, &world, 0, &mut by_original).unwrap();
            assert_eq!(by_memo, by_original, "{}", scan_root.display());
            assert!(!by_memo.is_empty());
        }
    }

    /// The scan as it stood before the memo, verbatim.
    fn collect_files_reference(root: &Path, world_root: &Path, depth: usize, files: &mut Vec<PathBuf>) -> io::Result<()> {
        if depth > MAX_SCAN_DEPTH || !root.is_dir() {
            return Ok(());
        }
        let mut entries = fs::read_dir(root)?.collect::<Result<Vec<_>, _>>()?;
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let path = entry.path();
            let file_type = entry.file_type()?;
            if file_type.is_symlink() {
                continue;
            }
            if file_type.is_dir() {
                let name = entry.file_name().to_string_lossy().into_owned();
                if !should_skip_dir(&name) {
                    collect_files_reference(&path, world_root, depth + 1, files)?;
                }
            } else if file_type.is_file()
                && entry.file_name() != AGENT_RETRIEVAL_DENY_MARKER
                && !is_system_artifact(&entry.file_name())
                && safe_regular_file(world_root, &path)?
            {
                files.push(path);
            }
        }
        Ok(())
    }

    #[test]
    fn a_root_that_is_not_a_directory_or_a_path_outside_the_world_refuses_as_before() {
        let ground = Ground::new("refuses");
        ground.write("file.md", "x");
        let not_a_directory = ground.0.join("file.md");
        let mut memo = ScanMemo::new(&not_a_directory);
        assert_eq!(memo.admission(&not_a_directory.join("anything")).is_err(), retrieval_admission(&not_a_directory, &not_a_directory.join("anything")).is_err());
        let world = ground.0.join("world");
        fs::create_dir_all(&world).unwrap();
        let mut memo = ScanMemo::new(&world);
        let outside = ground.0.join("file.md");
        assert!(memo.admission(&outside).is_err(), "outside its owner boundary");
        assert!(retrieval_admission(&world, &outside).is_err());
        assert!(!memo.retrieval_allowed(&outside));
        // The root itself: only its own marker speaks.
        assert_eq!(memo.admission(&world).unwrap(), retrieval_admission(&world, &world).unwrap());
        fs::write(world.join(AGENT_RETRIEVAL_DENY_MARKER), "").unwrap();
        let mut fresh = ScanMemo::new(&world);
        assert_eq!(fresh.admission(&world).unwrap(), retrieval_admission(&world, &world).unwrap());
        assert!(!fresh.retrieval_allowed(&world.join("x.md")), "a marker at the root withholds the whole world");
    }
}
