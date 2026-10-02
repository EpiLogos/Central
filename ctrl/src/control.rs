use serde::Serialize;
use std::fs;
use std::io;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

pub const CONTROL_ROOTS: [&str; 3] = ["user", "agents", "machines"];
pub const AGENT_RETRIEVAL_DENY_MARKER: &str = ".no-agent-retrieval";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceClass {
    Authored,
    Mixed,
    Unresolved,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ControlSourceRoot {
    pub target: String,
    pub path: PathBuf,
    pub source_class: SourceClass,
    pub exists: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ControlSearchMatch {
    pub target: String,
    pub source_path: PathBuf,
    pub line: usize,
    pub text: String,
    pub source_class: SourceClass,
    pub source_binding: Option<crate::source_horizon::SourceBinding>,
    pub source_revision: crate::source_horizon::SourceRevision,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ControlSkippedSource {
    pub target: String,
    pub source_path: PathBuf,
    pub source_class: SourceClass,
    pub reason: String,
    pub source_binding: Option<crate::source_horizon::SourceBinding>,
    pub source_revision: Option<crate::source_horizon::SourceRevision>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ControlSearchResult {
    pub query: String,
    pub roots: Vec<ControlSourceRoot>,
    pub files_scanned: usize,
    pub skipped_sources: Vec<ControlSkippedSource>,
    pub matches: Vec<ControlSearchMatch>,
}

pub fn locate_control_root(central_root: &Path, target: &str) -> io::Result<ControlSourceRoot> {
    if !CONTROL_ROOTS.contains(&target) {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, format!(
            "Control root must be one of: {}.",
            CONTROL_ROOTS.join(", ")
        )));
    }
    let path = central_root.join("Control").join(target);
    let read = ControlRead::new(central_root)?;
    let relative = Path::new("Control").join(target);
    read.require(&relative)?;
    let exists = match fs::symlink_metadata(read.canonical_root.join(&relative)) {
        Ok(metadata) => {
            if !metadata.is_dir() || metadata.file_type().is_symlink() {
                return Err(io::Error::new(io::ErrorKind::InvalidInput, "Control aperture must be an ordinary directory"));
            }
            let held = crate::file_mutation::directory(&read.canonical_root, &relative)?;
            let original = held.metadata()?;
            checkpoint("after_lookup", &read.canonical_root.join(&relative));
            read.require(&relative)?;
            let current = crate::file_mutation::directory(&read.canonical_root, &relative)?.metadata()?;
            if (original.dev(), original.ino()) != (current.dev(), current.ino()) {
                return Err(io::Error::other("Control root aperture changed during lookup"));
            }
            true
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            // The final aperture may be absent; a missing/changed owner or
            // parent is not evidence of that final member's absence.
            crate::file_mutation::directory(&read.canonical_root, Path::new("Control"))?;
            checkpoint("after_lookup", &read.canonical_root.join(&relative));
            read.require(&relative)?;
            match fs::symlink_metadata(read.canonical_root.join(&relative)) {
                Err(error) if error.kind() == io::ErrorKind::NotFound => {},
                Err(error) => return Err(error),
                Ok(_) => return Err(io::Error::other("Control aperture changed from absent to present during lookup")),
            }
            false
        }
        Err(error) => return Err(error),
    };
    read.validate_root()?;
    Ok(ControlSourceRoot {
        target: target.to_owned(),
        path,
        // Control/agents is intentionally a container of two authorities:
        // human-authored governance (plus preserved pre-split authored files) and
        // Agent-maintained Wiki knowledge. Human Control search below excludes wiki/.
        source_class: if target == "agents" {
            SourceClass::Mixed
        } else {
            SourceClass::Authored
        },
        exists,
    })
}

struct ControlRead {
    requested_root: PathBuf,
    canonical_root: PathBuf,
    affiliation: (u64, u64),
}

#[derive(Debug, PartialEq, Eq)]
struct ControlMaterialBasis {
    revision: String,
    device: u64,
    inode: u64,
    length: u64,
    modified_seconds: i64,
    modified_nanoseconds: i64,
}

struct ControlMaterial {
    bytes: Vec<u8>,
    basis: ControlMaterialBasis,
}

struct ControlStanding {
    value: serde_json::Value,
    basis: ControlMaterialBasis,
}

impl ControlRead {
    fn new(root: &Path) -> io::Result<Self> {
        let canonical_root = fs::canonicalize(root)?;
        let metadata = fs::symlink_metadata(&canonical_root)?;
        if !metadata.is_dir() {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "Control owner root is not a directory"));
        }
        let read = Self { requested_root: root.to_path_buf(), canonical_root, affiliation: (metadata.dev(), metadata.ino()) };
        read.validate_root()?;
        Ok(read)
    }

    fn validate_root(&self) -> io::Result<()> {
        let canonical = fs::canonicalize(&self.requested_root)?;
        let metadata = fs::symlink_metadata(&canonical)?;
        if canonical != self.canonical_root || (metadata.dev(), metadata.ino()) != self.affiliation {
            return Err(io::Error::other("Control owner root affiliation changed during reading"));
        }
        Ok(())
    }

    fn allowed(&self, relative: &Path) -> io::Result<bool> {
        self.validate_root()?;
        let allowed = crate::source_horizon::retrieval_admission(
            &self.requested_root, &self.requested_root.join(relative))?;
        self.validate_root()?;
        Ok(allowed)
    }

    fn require(&self, relative: &Path) -> io::Result<()> {
        if !self.allowed(relative)? {
            return Err(io::Error::new(io::ErrorKind::PermissionDenied, "Control retrieval is withheld by a current .no-agent-retrieval marker"));
        }
        Ok(())
    }

    fn entries(&self, relative: &Path) -> io::Result<Option<Vec<fs::DirEntry>>> {
        if !self.allowed(relative)? { return Ok(None); }
        let held = crate::file_mutation::directory(&self.canonical_root, relative)?;
        let admitted = held.metadata()?;
        let mut entries = fs::read_dir(self.canonical_root.join(relative))?.collect::<io::Result<Vec<_>>>()?;
        checkpoint("after_enumeration", &self.canonical_root.join(relative));
        if !self.allowed(relative)? { return Ok(None); }
        let current = crate::file_mutation::directory(&self.canonical_root, relative)?.metadata()?;
        if current.dev() != admitted.dev() || current.ino() != admitted.ino() {
            return Err(io::Error::other("Control directory affiliation changed during enumeration"));
        }
        entries.sort_by_key(|entry| entry.file_name());
        Ok(Some(entries))
    }

    fn bytes(&self, relative: &Path) -> io::Result<Option<ControlMaterial>> {
        self.material(relative, true)
    }

    fn material(&self, relative: &Path, observe_read: bool) -> io::Result<Option<ControlMaterial>> {
        if !self.allowed(relative)? { return Ok(None); }
        if observe_read { checkpoint("before_body", &self.canonical_root.join(relative)); }
        if !self.allowed(relative)? { return Ok(None); }
        let mut reader = crate::file_mutation::NativeFileRead::open(&self.canonical_root, self.affiliation, relative)?;
        let bytes = reader.read_bytes(crate::source_safety::MAX_SOURCE)?;
        if observe_read { checkpoint("after_body", &self.canonical_root.join(relative)); }
        if !self.allowed(relative)? { return Ok(None); }
        let metadata = fs::symlink_metadata(self.canonical_root.join(relative))?;
        reader.validate()?;
        let basis = ControlMaterialBasis {
            revision: crate::source_safety::content_revision_bytes(&bytes),
            device: metadata.dev(), inode: metadata.ino(), length: metadata.len(),
            modified_seconds: metadata.mtime(), modified_nanoseconds: metadata.mtime_nsec(),
        };
        Ok(Some(ControlMaterial { bytes, basis }))
    }

    fn qualify_material(&self, relative: &Path, expected: &ControlMaterialBasis) -> io::Result<bool> {
        // Aperture admission permits an absent final creation member. Delivery
        // instead reopens actual material and qualifies its captured basis.
        // Keep no descriptors proportional to the number of visited sources.
        let Some(current) = self.material(relative, false)? else { return Ok(false); };
        if &current.basis != expected {
            return Err(io::Error::other(format!(
                "Control source material changed before emission: expected revision {}, observed revision {}",
                expected.revision, current.basis.revision,
            )));
        }
        Ok(true)
    }

    fn metadata_source(&self, relations: &Path, observe_read: bool) -> io::Result<Option<ControlStanding>> {
        let parent = relations.parent().ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Metadata source has no parent"))?;
        self.require(Path::new("Control"))?;
        let admission = match self.allowed(relations) {
            Ok(false) => return Err(io::Error::new(io::ErrorKind::PermissionDenied,
                "Governance standing source is currently withheld")),
            Ok(true) => true,
            Err(error) if error.kind() == io::ErrorKind::NotFound => false,
            Err(error) => return Err(error),
        };
        match fs::symlink_metadata(self.canonical_root.join(relations)) {
            Ok(_) => {
                if !admission {
                    return Err(io::Error::other("Governance standing source appeared after its parent observation"));
                }
                self.require(relations)?;
                let material = self.material(relations, observe_read)?.ok_or_else(|| io::Error::new(
                    io::ErrorKind::PermissionDenied, "Governance standing source became withheld"))?;
                let value = serde_json::from_slice(&material.bytes)
                    .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
                Ok(Some(ControlStanding { value, basis: material.basis }))
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                self.require(Path::new("Control"))?;
                // Native relation parents are optional, but a missing parent
                // must be reobserved on qualification rather than erasing IO.
                match fs::symlink_metadata(self.canonical_root.join(parent)) {
                    Ok(metadata) if !metadata.is_dir() || metadata.file_type().is_symlink() => {
                        return Err(io::Error::new(io::ErrorKind::InvalidData, "Metadata parent is not an ordinary directory"));
                    }
                    Ok(_) => {},
                    Err(error) if error.kind() == io::ErrorKind::NotFound => {},
                    Err(error) => return Err(error),
                }
                Ok(None)
            }
            Err(error) => Err(error),
        }
    }

    fn standing_source(&self, observe_read: bool) -> io::Result<Option<ControlStanding>> {
        let source = self.metadata_source(Path::new(SOURCE_RELATIONS), observe_read)?;
        if let Some(source) = &source {
            crate::source_horizon::validate_relations_value(&source.value,
                crate::source_horizon::CONTROL_GROUND_RELATIONS_SCHEMA,
                crate::source_horizon::CONTROL_WORLD_REF)?;
        }
        Ok(source)
    }

    fn qualify_standing(&self, expected: &Option<ControlStanding>) -> io::Result<()> {
        self.qualify_metadata(Path::new(SOURCE_RELATIONS), expected)
    }

    fn qualify_metadata(&self, relative: &Path, expected: &Option<ControlStanding>) -> io::Result<()> {
        if let Some(expected) = expected {
            // A formerly present source disappearing must keep the actual
            // reader's IO cause, rather than becoming a new absence/default.
            if !self.qualify_material(relative, &expected.basis)? {
                return Err(io::Error::new(io::ErrorKind::PermissionDenied,
                    "Governance standing source became withheld before emission"));
            }
            return Ok(());
        }
        if self.metadata_source(relative, false)?.is_some() {
            return Err(io::Error::other("Governance standing source changed from absent to present before emission"));
        }
        Ok(())
    }
}

fn source_class(binding: Option<&crate::source_horizon::SourceBinding>) -> SourceClass {
    if binding.is_some_and(|source| matches!(source.provenance.as_str(), "human-authored" | "human-adopted")) {
        SourceClass::Authored
    } else { SourceClass::Unresolved }
}

fn source_revision(basis: &ControlMaterialBasis) -> crate::source_horizon::SourceRevision {
    crate::source_horizon::SourceRevision { revision: basis.revision.clone(), byte_len: basis.length }
}

#[cfg(test)]
type ReadCheckpoint = (&'static str, PathBuf, Box<dyn FnOnce(&Path)>);
#[cfg(test)]
thread_local! {
    static READ_CHECKPOINT: std::cell::RefCell<Option<ReadCheckpoint>> = std::cell::RefCell::new(None);
}
fn checkpoint(_stage: &str, _path: &Path) {
    #[cfg(test)]
    READ_CHECKPOINT.with(|hook| {
        let selected = hook.borrow().as_ref().is_some_and(|(stage, path, _)|
            *stage == _stage && path.as_path() == _path);
        if selected {
            let (_, _, observer) = hook.borrow_mut().take().expect("selected actual read checkpoint");
            observer(_path);
        }
    });
}

fn readable_files(
    read: &ControlRead,
    target: &str,
    relative: &Path,
    files: &mut Vec<PathBuf>,
    skipped_sources: &mut Vec<ControlSkippedSource>,
) -> io::Result<()> {
    // `control.search` reads eligible native Control material. Agent Wiki
    // knowledge has its own SemanticWiki path and must not be relabelled authored
    // merely because it is nested under Control/agents.
    if target == "agents" && relative == Path::new("Control/agents/wiki") {
        return Ok(());
    }

    let Some(entries) = read.entries(relative)? else {
        skipped_sources.push(ControlSkippedSource {
            target: target.to_owned(),
            source_path: relative.to_path_buf(),
            source_class: SourceClass::Unresolved,
            reason: "not_agent_readable".to_owned(),
            source_binding: None, source_revision: None,
        });
        return Ok(());
    };
    for entry in entries {
        let path = relative.join(entry.file_name());
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            readable_files(read, target, &path, files, skipped_sources)?;
        } else if file_type.is_file() && entry.file_name() != AGENT_RETRIEVAL_DENY_MARKER && read.allowed(&path)? {
            files.push(path);
        }
    }
    Ok(())
}

pub fn search_control(central_root: &Path, query: &str) -> io::Result<ControlSearchResult> {
    let query = query.trim();
    if query.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Control search requires a non-empty query.",
        ));
    }

    let read = ControlRead::new(central_root)?;
    let mut roots = Vec::new();
    for target in CONTROL_ROOTS {
        let mut root = locate_control_root(central_root, target)?;
        let relative = Path::new("Control").join(target);
        read.require(&relative)?;
        let metadata = fs::symlink_metadata(read.canonical_root.join(&relative))?;
        if !metadata.is_dir() {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, format!("Control/{target} is not a directory.")));
        }
        root.exists = true;
        roots.push(root);
    }

    let needle = query.to_lowercase();
    let mut pending_matches = Vec::new();
    let mut pending_binary = Vec::new();
    let mut skipped_sources = Vec::new();
    let standing_source = read.standing_source(true)?;
    let mut skill_sources = std::collections::BTreeMap::<PathBuf, Option<ControlStanding>>::new();

    for root in &roots {
        let mut files = Vec::new();
        readable_files(
            &read,
            &root.target,
            &Path::new("Control").join(&root.target),
            &mut files,
            &mut skipped_sources,
        )?;
        for path in files {
            let source_path = path.clone();
            let Some(material) = read.bytes(&path)? else { continue; };
            let relative = path.to_str().ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "Control source path is not UTF-8"))?;
            let member_key = crate::source_safety::normal_member_key(relative)?;
            let mut declared = false;
            if let Some(source) = &standing_source {
                for relation in source.value["relations"].as_array().ok_or_else(|| io::Error::new(
                    io::ErrorKind::InvalidData, "relations must be an array"))? {
                    let declared_path = relation["path"].as_str().ok_or_else(|| io::Error::new(
                        io::ErrorKind::InvalidData, "source relation path must be text"))?;
                    if crate::source_safety::normal_member_key(declared_path)? == member_key {
                        declared = true;
                        break;
                    }
                }
            }
            let manifest = if !declared {
                if let Some(manifest_path) = crate::control_skills::control_skill_manifest_path(relative)? {
                    let manifest_path = PathBuf::from(manifest_path);
                    if !skill_sources.contains_key(&manifest_path) {
                        skill_sources.insert(manifest_path.clone(), read.metadata_source(&manifest_path, true)?);
                    }
                    skill_sources.get(&manifest_path).and_then(Option::as_ref).map(|source|
                        crate::control_skills::parse_skill_manifest(
                            &serde_json::to_vec(&source.value).map_err(io::Error::other)?, &manifest_path))
                        .transpose()?
                } else { None }
            } else { None };
            let binding = crate::source_horizon::control_binding_for_observed_path(
                relative, standing_source.as_ref().map(|source| &source.value), manifest.as_ref(), true)?;
            let class = source_class(binding.as_ref());
            let revision = source_revision(&material.basis);
            let text = match if material.bytes.contains(&0) { None } else { String::from_utf8(material.bytes).ok() } {
                Some(text) => text,
                None => {
                    pending_binary.push((ControlSkippedSource {
                        target: root.target.clone(),
                        source_path,
                        source_class: class,
                        reason: "unsupported_non_text_source".to_owned(),
                        source_binding: binding, source_revision: Some(revision),
                    }, material.basis));
                    continue;
                }
            };
            let mut file_matches = Vec::new();
            for (index, line) in text.lines().enumerate() {
                if line.to_lowercase().contains(&needle) {
                    file_matches.push(ControlSearchMatch {
                        target: root.target.clone(),
                        source_path: source_path.clone(),
                        line: index + 1,
                        text: line.to_owned(),
                        source_class: class,
                        source_binding: binding.clone(), source_revision: revision.clone(),
                    });
                }
            }
            pending_matches.push((path, file_matches, material.basis));
        }
    }

    // Run all emission checkpoints before qualifying any returned material.
    // A later source's checkpoint may withdraw or replace an earlier source.
    for (path, _, _) in &pending_matches {
        checkpoint("before_emit", &read.canonical_root.join(path));
    }
    for (skipped, _) in &pending_binary {
        checkpoint("before_emit", &read.canonical_root.join(&skipped.source_path));
    }
    let mut matches = Vec::new();
    let mut files_scanned = 0;
    for (path, file_matches, basis) in pending_matches {
        if read.qualify_material(&path, &basis)? {
            files_scanned += 1;
            matches.extend(file_matches);
        }
    }
    let mut current_skipped = Vec::new();
    for skipped in skipped_sources {
        if read.allowed(skipped.source_path.parent().unwrap_or(Path::new("")))?
            && !read.allowed(&skipped.source_path)?
        {
            current_skipped.push(skipped);
        }
    }
    for (skipped, basis) in pending_binary {
        if read.qualify_material(&skipped.source_path, &basis)? {
            current_skipped.push(skipped);
        }
    }
    let skipped_sources = current_skipped;
    for root in &roots { read.require(&Path::new("Control").join(&root.target))?; }
    read.qualify_standing(&standing_source)?;
    for (path, source) in &skill_sources { read.qualify_metadata(path, source)?; }
    read.validate_root()?;

    Ok(ControlSearchResult {
        query: query.to_owned(),
        roots,
        files_scanned,
        skipped_sources,
        matches,
    })
}

/// One governance statement in the session-start index: file name, topic and
/// standing. Standing is resolved from the source relations so a session can
/// tell adopted law from a generated suggestion awaiting adoption.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GovernanceIndexEntry {
    pub file: String,
    pub topic: String,
    pub standing: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GovernanceIndex {
    pub statements: Vec<GovernanceIndexEntry>,
    pub drafts: usize,
}

const GOVERNANCE_DIR: &str = "Control/agents/governance";
const SOURCE_RELATIONS: &str = "Control/relations/source-relations.json";

fn governance_standing(value: &serde_json::Value, rel: &str) -> io::Result<String> {
    let binding = crate::source_horizon::control_binding_for_observed_path(rel, Some(value), None, true)?;
    Ok(match binding.as_ref().map(|source| source.standing.as_str()) {
        Some("durable-source") => "durable", Some("draft-source") => "draft", _ => "undeclared",
    }.to_owned())
}

/// The session-start flash of the governance field: one entry per statement
/// file — name, topic, standing — never the content. Derived compilations and
/// folder readers are skipped.
pub fn index_governance(central_root: &Path) -> io::Result<GovernanceIndex> {
    let read = ControlRead::new(central_root)?;
    let governance = Path::new(GOVERNANCE_DIR);
    read.require(governance)?;
    let standing_source = read.standing_source(true)?;
    let mut files = Vec::new();
    let mut stack = vec![governance.to_path_buf()];
    while let Some(directory) = stack.pop() {
        let Some(entries) = read.entries(&directory)? else { continue; };
        for entry in entries {
            let path = directory.join(entry.file_name());
            let kind = entry.file_type()?;
            if kind.is_dir() {
                stack.push(path);
            } else if kind.is_file() && path.extension().is_some_and(|ext| ext == "md")
                && path
                    .file_name()
                    .is_some_and(|name| name != "README.md" && name != "foundational-prompt.md")
                && read.allowed(&path)?
            {
                files.push(path);
            }
        }
    }
    files.sort();
    let mut statements = Vec::new();
    for path in files {
        let rel = path.to_str().ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "Governance path is not UTF-8"))?;
        let Some(material) = read.bytes(&path)? else { continue; };
        if material.bytes.contains(&0) {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "Governance source is not UTF-8 text material"));
        }
        let text = String::from_utf8(material.bytes).map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        let topic = text
            .lines()
            .find_map(|line| line.strip_prefix("# "))
            .unwrap_or("")
            .trim()
            .to_owned();
        let entry = GovernanceIndexEntry {
            file: rel
                .strip_prefix(&format!("{GOVERNANCE_DIR}/"))
                .unwrap_or(&rel)
                .to_owned(),
            topic: if topic.is_empty() {
                path.file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string()
            } else {
                topic
            },
            standing: standing_source.as_ref().map(|source| governance_standing(&source.value, rel)).transpose()?.unwrap_or_else(|| "undeclared".to_owned()),
        };
        statements.push((path, entry, material.basis));
    }
    read.require(governance)?;
    for (path, _, _) in &statements {
        checkpoint("before_emit", &read.canonical_root.join(path));
    }
    let mut current_statements = Vec::new();
    for (path, entry, basis) in statements {
        if read.qualify_material(&path, &basis)? { current_statements.push(entry); }
    }
    read.require(governance)?;
    read.qualify_standing(&standing_source)?;
    read.validate_root()?;
    let statements = current_statements;
    let drafts = statements
        .iter()
        .filter(|entry| entry.standing == "draft")
        .count();
    Ok(GovernanceIndex { statements, drafts })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);

    struct Ground(PathBuf);
    impl Ground {
        fn new() -> Self {
            let scratch = Path::new(env!("CARGO_MANIFEST_DIR")).join("../ProjectCentral/now/tmp");
            fs::create_dir_all(&scratch).unwrap();
            let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
            let path = scratch.join(format!("current-control-{}-{nonce}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
            fs::create_dir(&path).unwrap();
            crate::root::initialize_central(&path).unwrap();
            Self(fs::canonicalize(path).unwrap())
        }
        fn write(&self, relative: &str, bytes: &[u8]) -> PathBuf {
            let path = self.0.join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, bytes).unwrap();
            path
        }
    }
    impl Drop for Ground {
        fn drop(&mut self) {
            READ_CHECKPOINT.with(|hook| { hook.borrow_mut().take(); });
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn observe(stage: &'static str, path: &Path, observer: impl FnOnce(&Path) + 'static) {
        READ_CHECKPOINT.with(|hook| *hook.borrow_mut() = Some((stage, path.to_path_buf(), Box::new(observer))));
    }

    #[test]
    fn actual_late_marker_at_descent_body_and_emission_withholds_only_its_subtree() {
        for stage in ["after_enumeration", "before_body", "after_body", "before_emit"] {
            for index in [false, true] {
                let ground = Ground::new();
                let private = ground.write("Control/agents/governance/private/secret.md", b"# Late private title\nprivate-needle\n");
                let original = fs::metadata(&private).unwrap();
                ground.write("Control/agents/governance/open.md", b"# Open sibling\npublic-needle\n");
                let directory = private.parent().unwrap().to_path_buf();
                let target = if stage == "after_enumeration" { directory.clone() } else { private.clone() };
                observe(stage, &target, move |_| { fs::write(directory.join(AGENT_RETRIEVAL_DENY_MARKER), b"").unwrap(); });
                if index {
                    let reading = index_governance(&ground.0).unwrap();
                    assert_eq!(reading.statements.len(), 1);
                    assert_eq!(reading.statements[0].topic, "Open sibling");
                    assert!(!serde_json::to_string(&reading).unwrap().contains("secret.md"));
                } else {
                    let reading = search_control(&ground.0, "needle").unwrap();
                    assert_eq!(reading.matches.len(), 1);
                    assert_eq!(reading.matches[0].text, "public-needle");
                    assert!(!serde_json::to_string(&reading.matches).unwrap().contains("secret.md"));
                }
                assert!(private.parent().unwrap().join(AGENT_RETRIEVAL_DENY_MARKER).is_file());
                assert_eq!(fs::read(&private).unwrap(), b"# Late private title\nprivate-needle\n");
                assert_eq!(fs::metadata(&private).unwrap().ino(), original.ino());
                fs::remove_file(private.parent().unwrap().join(AGENT_RETRIEVAL_DENY_MARKER)).unwrap();
                assert_eq!(index_governance(&ground.0).unwrap().statements.len(), 2);
                assert_eq!(search_control(&ground.0, "needle").unwrap().matches.len(), 2);
            }
        }
    }

    #[test]
    fn actual_root_alias_retarget_after_body_never_acknowledges_other_world_bytes() {
        use std::os::unix::fs::symlink;
        let ground = Ground::new();
        let other = Ground::new();
        let source = ground.write("Control/user/source.md", b"original-needle");
        other.write("Control/user/source.md", b"other-needle");
        let alias = ground.0.join("root-alias");
        symlink(&ground.0, &alias).unwrap();
        assert_eq!(search_control(&alias, "needle").unwrap().matches[0].text, "original-needle");
        let requested = alias.clone();
        let replacement = other.0.clone();
        observe("after_body", &source, move |_| {
            fs::remove_file(&requested).unwrap();
            symlink(&replacement, &requested).unwrap();
        });
        assert_eq!(search_control(&alias, "needle").unwrap_err().kind(), io::ErrorKind::Other);
        assert_eq!(fs::read(&source).unwrap(), b"original-needle");
        assert_eq!(search_control(&alias, "needle").unwrap().matches[0].text, "other-needle");
    }

    #[test]
    fn actual_selected_root_withdrawal_after_body_is_a_refusal_not_empty_success() {
        for index in [false, true] {
            let ground = Ground::new();
            let source = ground.write("Control/agents/governance/source.md", b"# Retained title\nretained-needle\n");
            let marker = ground.0.join("Control/agents/.no-agent-retrieval");
            observe("before_emit", &source, move |_| { fs::write(&marker, b"").unwrap(); });
            let error = if index { index_governance(&ground.0).unwrap_err() }
                else { search_control(&ground.0, "needle").unwrap_err() };
            assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
            assert_eq!(fs::read(source).unwrap(), b"# Retained title\nretained-needle\n");
        }
    }

    #[test]
    fn actual_final_symlink_or_parent_substitution_before_body_refuses_redirected_content() {
        use std::os::unix::fs::symlink;
        for parent in [false, true] {
            let ground = Ground::new();
            let source = ground.write("Control/user/folder/source.md", b"original-needle");
            let outside = ground.write("outside.md", b"redirected-needle");
            let original = source.clone();
            let retained = ground.0.join("retained");
            observe("before_body", &source, move |_| {
                if parent {
                    fs::rename(original.parent().unwrap(), &retained).unwrap();
                    symlink(&retained, original.parent().unwrap()).unwrap();
                } else {
                    fs::rename(&original, &retained).unwrap();
                    symlink(&outside, &original).unwrap();
                }
            });
            assert_eq!(search_control(&ground.0, "needle").unwrap_err().kind(), io::ErrorKind::PermissionDenied);
            let saved = if parent { ground.0.join("retained/source.md") } else { ground.0.join("retained") };
            assert_eq!(fs::read(saved).unwrap(), b"original-needle");
        }
    }

    #[test]
    fn actual_body_permission_error_retains_errno_instead_of_an_empty_topic() {
        use std::os::unix::fs::PermissionsExt;
        if unsafe { libc::geteuid() } == 0 {
            eprintln!("qualification unavailable: actual body EACCES requires a nonroot OS user");
            return;
        }
        let ground = Ground::new();
        let source = ground.write("Control/agents/governance/blocked.md", b"# Retained topic\n");
        struct Restore(PathBuf);
        impl Drop for Restore {
            fn drop(&mut self) { let _ = fs::set_permissions(&self.0, fs::Permissions::from_mode(0o600)); }
        }
        let _restore = Restore(source.clone());
        fs::set_permissions(&source, fs::Permissions::from_mode(0o000)).unwrap();
        let actual = fs::read(&source).unwrap_err();
        let error = index_governance(&ground.0).unwrap_err();
        assert_eq!(actual.kind(), io::ErrorKind::PermissionDenied);
        assert_eq!(error.kind(), actual.kind());
        assert_eq!(error.raw_os_error(), actual.raw_os_error());
    }

    #[test]
    fn actual_deleted_material_before_emission_cannot_deliver_stale_hits_or_topics() {
        for index in [false, true] {
            let ground = Ground::new();
            let source = ground.write("Control/agents/governance/source.md", b"# Previous title\nprevious-needle\n");
            ground.write("Control/agents/governance/sibling.md", b"# Open sibling\nsibling-needle\n");
            observe("before_emit", &source, move |path| { fs::remove_file(path).unwrap(); });
            let error = if index { index_governance(&ground.0).unwrap_err() }
                else { search_control(&ground.0, "needle").unwrap_err() };
            let actual = fs::symlink_metadata(&source).unwrap_err();
            assert_eq!(error.kind(), actual.kind());
            assert_eq!(error.raw_os_error(), actual.raw_os_error());
            assert_eq!(index_governance(&ground.0).unwrap().statements.len(), 1);
            assert_eq!(search_control(&ground.0, "needle").unwrap().matches[0].text, "sibling-needle");
            fs::write(&source, b"# Fresh title\nfresh-needle\n").unwrap();
            assert_eq!(index_governance(&ground.0).unwrap().statements.len(), 2);
            assert!(search_control(&ground.0, "needle").unwrap().matches.iter()
                .any(|found| found.text == "fresh-needle"));
        }
    }

    #[test]
    fn actual_replacement_and_same_inode_edit_require_a_fresh_emission_basis() {
        for replace in [false, true] {
            for index in [false, true] {
                let ground = Ground::new();
                let original = b"# Previous title\nprevious-needle\n";
                let source = ground.write("Control/agents/governance/source.md", original);
                let metadata = fs::metadata(&source).unwrap();
                let retained = ground.0.join("retained-source.md");
                let saved = retained.clone();
                observe("before_emit", &source, move |path| {
                    if replace { fs::rename(path, &saved).unwrap(); }
                    fs::write(path, b"# Current title\ncurrent-needle\n").unwrap();
                });
                let error = if index { index_governance(&ground.0).unwrap_err() }
                    else { search_control(&ground.0, "needle").unwrap_err() };
                assert_eq!(error.kind(), io::ErrorKind::Other);
                assert!(error.to_string().contains("material changed before emission"));
                assert!(error.to_string().contains(&crate::source_safety::content_revision_bytes(original)));
                if replace {
                    assert_eq!(fs::read(retained).unwrap(), original);
                    assert_ne!(fs::metadata(&source).unwrap().ino(), metadata.ino());
                } else {
                    assert_eq!(fs::metadata(&source).unwrap().ino(), metadata.ino());
                }
                let fresh = index_governance(&ground.0).unwrap();
                assert_eq!(fresh.statements[0].topic, "Current title");
                assert_eq!(search_control(&ground.0, "needle").unwrap().matches[0].text, "current-needle");
                let unchanged = fs::metadata(&source).unwrap();
                assert_eq!(index_governance(&ground.0).unwrap(), fresh);
                assert_eq!(fs::metadata(&source).unwrap().ino(), unchanged.ino());
                assert_eq!(fs::metadata(&source).unwrap().modified().unwrap(), unchanged.modified().unwrap());
            }
        }
    }

    #[test]
    fn actual_binary_skip_metadata_requires_existing_unchanged_material_at_emission() {
        let ground = Ground::new();
        let source = ground.write("Control/user/binary.bin", &[0xff, 0]);
        let unchanged = search_control(&ground.0, "needle").unwrap();
        assert_eq!(unchanged.skipped_sources.len(), 1);
        observe("before_emit", &source, move |path| { fs::remove_file(path).unwrap(); });
        let error = search_control(&ground.0, "needle").unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::NotFound);
        assert!(search_control(&ground.0, "needle").unwrap().skipped_sources.is_empty());
    }

    #[test]
    fn actual_standing_deletion_change_or_new_source_cannot_emit_a_previous_standing() {
        for change in ["delete", "change", "appear"] {
            let ground = Ground::new();
            let relative = "Control/agents/governance/source.md";
            let source = ground.write(relative, b"# Actual title\n");
            let relations = ground.0.join(SOURCE_RELATIONS);
            let source_ref = crate::source_horizon::source_ref(crate::source_horizon::CONTROL_WORLD_REF, relative);
            let relation = |standing: &str| serde_json::to_vec(&serde_json::json!({
                "schema": crate::source_horizon::CONTROL_GROUND_RELATIONS_SCHEMA,
                "project_id": crate::source_horizon::CONTROL_WORLD_REF,
                "relations": [{"ref": source_ref, "path": relative, "standing": standing,
                    "provenance":"unresolved", "roles":["agent-governance-source"], "treatment":"control-agent-governance"}]
            })).unwrap();
            let draft = relation("draft-source");
            let durable = relation("durable-source");
            if change != "appear" { ground.write(SOURCE_RELATIONS, &draft); }
            let original = index_governance(&ground.0).unwrap();
            assert_eq!(original.statements[0].standing, if change == "appear" { "undeclared" } else { "draft" });
            let target = relations.clone();
            let next = durable.clone();
            observe("before_emit", &source, move |_| {
                if change == "delete" { fs::remove_file(&target).unwrap(); }
                else {
                    fs::create_dir_all(target.parent().unwrap()).unwrap();
                    fs::write(&target, next).unwrap();
                }
            });
            let error = index_governance(&ground.0).unwrap_err();
            if change == "delete" {
                let actual = fs::symlink_metadata(&relations).unwrap_err();
                assert_eq!(error.kind(), actual.kind());
                assert_eq!(error.raw_os_error(), actual.raw_os_error());
            } else { assert_eq!(error.kind(), io::ErrorKind::Other); }
            let fresh = index_governance(&ground.0).unwrap();
            assert_eq!(fresh.statements[0].standing, if change == "delete" { "undeclared" } else { "durable" });
            assert_eq!(index_governance(&ground.0).unwrap(), fresh);
            assert_eq!(fs::read(source).unwrap(), b"# Actual title\n");
        }
    }

    #[test]
    fn actual_unavailable_absent_standing_recheck_is_not_fabricated_absence() {
        use std::os::unix::fs::PermissionsExt;
        if unsafe { libc::geteuid() } == 0 {
            eprintln!("qualification unavailable: actual absent-standing EACCES requires a nonroot OS user");
            return;
        }
        let ground = Ground::new();
        let source = ground.write("Control/agents/governance/source.md", b"# Actual title\n");
        let directory = ground.0.join("Control/relations");
        fs::create_dir(&directory).unwrap();
        let relations = directory.join("source-relations.json");
        assert_eq!(fs::symlink_metadata(&relations).unwrap_err().kind(), io::ErrorKind::NotFound);
        assert_eq!(index_governance(&ground.0).unwrap().statements[0].standing, "undeclared");
        struct Restore(PathBuf);
        impl Drop for Restore {
            fn drop(&mut self) { let _ = fs::set_permissions(&self.0, fs::Permissions::from_mode(0o700)); }
        }
        let _restore = Restore(directory.clone());
        let restricted = directory.clone();
        observe("before_emit", &source, move |_| {
            fs::set_permissions(&restricted, fs::Permissions::from_mode(0o000)).unwrap();
        });
        let error = index_governance(&ground.0).unwrap_err();
        let actual = fs::symlink_metadata(&relations).unwrap_err();
        assert_eq!(actual.kind(), io::ErrorKind::PermissionDenied);
        assert_eq!(error.kind(), actual.kind());
        assert_eq!(error.raw_os_error(), actual.raw_os_error());
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).unwrap();
        assert_eq!(index_governance(&ground.0).unwrap().statements[0].standing, "undeclared");
    }

    #[test]
    fn actual_later_emission_checkpoint_cannot_leave_an_earlier_source_in_output() {
        for change in ["withdraw", "remove", "replace", "edit"] {
            for index in [false, true] {
                let ground = Ground::new();
                let earlier = ground.write("Control/agents/governance/a-room/source.md", b"# Earlier title\nearlier-needle\n");
                let later = ground.write("Control/agents/governance/z-later.md", b"# Later title\nlater-needle\n");
                let selected = earlier.clone();
                let retained = ground.0.join("retained-earlier.md");
                observe("before_emit", &later, move |_| {
                    match change {
                        "withdraw" => { fs::write(selected.parent().unwrap().join(AGENT_RETRIEVAL_DENY_MARKER), b"").unwrap(); }
                        "remove" => { fs::remove_file(&selected).unwrap(); }
                        "replace" => {
                            fs::rename(&selected, retained).unwrap();
                            fs::write(&selected, b"# Replacement title\nreplacement-needle\n").unwrap();
                        }
                        _ => { fs::write(&selected, b"# Edited title\nedited-needle\n").unwrap(); }
                    }
                });
                if change == "withdraw" {
                    if index {
                        let reading = index_governance(&ground.0).unwrap();
                        assert_eq!(reading.statements.len(), 1);
                        assert_eq!(reading.statements[0].topic, "Later title");
                    } else {
                        let reading = search_control(&ground.0, "needle").unwrap();
                        assert_eq!(reading.matches.len(), 1);
                        assert_eq!(reading.matches[0].text, "later-needle");
                        assert_eq!(reading.files_scanned, 1);
                    }
                    assert_eq!(fs::read(&earlier).unwrap(), b"# Earlier title\nearlier-needle\n");
                    fs::remove_file(earlier.parent().unwrap().join(AGENT_RETRIEVAL_DENY_MARKER)).unwrap();
                    assert_eq!(index_governance(&ground.0).unwrap().statements.len(), 2);
                } else {
                    let error = if index { index_governance(&ground.0).unwrap_err() }
                        else { search_control(&ground.0, "needle").unwrap_err() };
                    assert_eq!(error.kind(), if change == "remove" { io::ErrorKind::NotFound } else { io::ErrorKind::Other });
                    assert_eq!(fs::read(&later).unwrap(), b"# Later title\nlater-needle\n");
                    assert_eq!(index_governance(&ground.0).unwrap().statements.len(), if change == "remove" { 1 } else { 2 });
                }
            }
        }
    }

    #[test]
    fn actual_binary_emission_checkpoint_cannot_delete_an_earlier_text_source_silently() {
        let ground = Ground::new();
        let text = ground.write("Control/user/a-source.md", b"earlier-needle");
        let binary = ground.write("Control/user/z-binary.bin", &[0xff, 0]);
        observe("before_emit", &binary, move |_| { fs::remove_file(text).unwrap(); });
        assert_eq!(search_control(&ground.0, "needle").unwrap_err().kind(), io::ErrorKind::NotFound);
        let fresh = search_control(&ground.0, "needle").unwrap();
        assert!(fresh.matches.is_empty());
        assert_eq!(fresh.skipped_sources.len(), 1);
        assert_eq!(fs::read(binary).unwrap(), [0xff, 0]);
    }
    #[test]
    fn actual_metadata_change_delete_or_appearance_refuses_old_disclosure_and_reopens_fresh() {
        for skill in [false, true] {
            for change in ["change", "delete", "appear"] {
                let ground = Ground::new();
                let relative = if skill { "Control/user/skills/example/SKILL.md" }
                    else { "Control/agents/governance/source.md" };
                let source = ground.write(relative, b"classification-needle");
                let metadata = ground.0.join(if skill { "Control/user/skills/example/skill.json" } else { SOURCE_RELATIONS });
                let value = |human: bool| if skill { serde_json::json!({
                    "schema":"central.skill/v1","name":"example","scope":"control-user",
                    "provenance": if human { "human-authored" } else { "adopted" }, "standing":"active"
                }) } else { serde_json::json!({
                    "schema":crate::source_horizon::CONTROL_GROUND_RELATIONS_SCHEMA,
                    "project_id":crate::source_horizon::CONTROL_WORLD_REF,
                    "relations":[{"ref":"central:source:control:root:observed", "path":relative,
                        "provenance":if human { "human-authored" } else { "generated-derived" },
                        "standing":"durable-source","roles":["agent-governance-source"],"treatment":"retain-native-in-place"}]
                }) };
                if change != "appear" {
                    fs::create_dir_all(metadata.parent().unwrap()).unwrap();
                    fs::write(&metadata, serde_json::to_vec(&value(true)).unwrap()).unwrap();
                }
                let previous = search_control(&ground.0, "classification-needle").unwrap();
                assert_eq!(previous.matches[0].source_class,
                    if change == "appear" { SourceClass::Unresolved } else { SourceClass::Authored });
                let target = metadata.clone();
                let next = serde_json::to_vec(&value(false)).unwrap();
                observe("before_emit", &source, move |_| {
                    if change == "delete" { fs::remove_file(&target).unwrap(); }
                    else {
                        fs::create_dir_all(target.parent().unwrap()).unwrap();
                        fs::write(&target, next).unwrap();
                    }
                });
                let error = search_control(&ground.0, "classification-needle").unwrap_err();
                assert_eq!(error.kind(), if change == "delete" { io::ErrorKind::NotFound } else { io::ErrorKind::Other });
                let fresh = search_control(&ground.0, "classification-needle").unwrap();
                assert_eq!(fresh.matches.len(), 1);
                assert_eq!(fresh.matches[0].source_class,
                    if skill && change != "delete" { SourceClass::Authored } else { SourceClass::Unresolved });
                assert_eq!(fs::read(&source).unwrap(), b"classification-needle");
            }
        }
    }

    #[test]
    fn actual_native_creation_bulk_and_observed_binding_share_the_same_ambiguity_rule() {
        let ground = Ground::new();
        let path = "Control/agents/governance/source.md";
        ground.write(path, b"source");
        let relation = |reference: &str| serde_json::json!({
            "ref":reference,"path":path,"provenance":"human-authored","standing":"durable-source",
            "roles":["agent-governance-source"],"treatment":"retain-native-in-place"
        });
        let value = serde_json::json!({
            "schema":crate::source_horizon::CONTROL_GROUND_RELATIONS_SCHEMA,
            "project_id":crate::source_horizon::CONTROL_WORLD_REF,
            "relations":[relation("central:source:control:root:one"),relation("central:source:control:root:two")]
        });
        let bytes = serde_json::to_vec(&value).unwrap();
        let metadata = ground.write(SOURCE_RELATIONS, &bytes);
        for error in [
            crate::source_horizon::control_binding_for_path(&ground.0, path).unwrap_err(),
            crate::source_horizon::control_source_bindings(&ground.0).unwrap_err(),
            search_control(&ground.0, "source").unwrap_err(),
        ] {
            assert_eq!(error.kind(), io::ErrorKind::InvalidData);
            assert!(error.to_string().contains("ambiguous duplicate source relation"));
        }
        let scope = crate::continuous_work::source::Scope::resolve(&ground.0, None).unwrap();
        assert_eq!(scope.relations().unwrap_err().kind(), io::ErrorKind::InvalidData);
        assert_eq!(fs::read(metadata).unwrap(), bytes);
    }

    #[test]
    fn actual_metadata_permission_error_retains_errno_and_never_downgrades_to_unbound() {
        use std::os::unix::fs::PermissionsExt;
        if unsafe { libc::geteuid() } == 0 {
            eprintln!("qualification unavailable: actual metadata EACCES requires a nonroot OS user");
            return;
        }
        let ground = Ground::new();
        let source = ground.write("Control/user/skills/example/SKILL.md", b"metadata-needle");
        let manifest = ground.write("Control/user/skills/example/skill.json",
            br#"{"schema":"central.skill/v1","name":"example","scope":"control-user","provenance":"human-authored","standing":"active"}"#);
        struct Restore(PathBuf);
        impl Drop for Restore { fn drop(&mut self) { let _ = fs::set_permissions(&self.0, fs::Permissions::from_mode(0o600)); } }
        let _restore = Restore(manifest.clone());
        assert_eq!(search_control(&ground.0, "metadata-needle").unwrap().matches[0].source_class, SourceClass::Authored);
        let restricted = manifest.clone();
        observe("before_emit", &source, move |_| { fs::set_permissions(restricted, fs::Permissions::from_mode(0o000)).unwrap(); });
        let error = search_control(&ground.0, "metadata-needle").unwrap_err();
        let actual = fs::read(&manifest).unwrap_err();
        assert_eq!(error.kind(), actual.kind());
        assert_eq!(error.raw_os_error(), actual.raw_os_error());
        fs::set_permissions(&manifest, fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(search_control(&ground.0, "metadata-needle").unwrap().matches[0].source_class, SourceClass::Authored);
    }

    #[test]
    fn actual_lookup_checkpoint_withdrawal_and_retarget_keep_owner_cause_and_form() {
        use std::os::unix::fs::symlink;
        let ground = Ground::new();
        let directory = ground.0.join("Control/user");
        observe("after_lookup", &directory, move |path| {
            fs::write(path.join(AGENT_RETRIEVAL_DENY_MARKER), b"").unwrap();
        });
        assert_eq!(locate_control_root(&ground.0, "user").unwrap_err().kind(), io::ErrorKind::PermissionDenied);
        fs::remove_file(directory.join(AGENT_RETRIEVAL_DENY_MARKER)).unwrap();
        let other = Ground::new();
        let alias = ground.0.join("world-alias");
        symlink(&ground.0, &alias).unwrap();
        let requested = alias.clone();
        let replacement = other.0.clone();
        observe("after_lookup", &directory, move |_| {
            fs::remove_file(&requested).unwrap();
            symlink(&replacement, &requested).unwrap();
        });
        assert_eq!(locate_control_root(&alias, "user").unwrap_err().kind(), io::ErrorKind::Other);
        assert!(locate_control_root(&alias, "user").unwrap().exists);
    }

    #[test]
    fn actual_lookup_final_absence_must_still_be_absent_at_acknowledgement() {
        let ground = Ground::new();
        let selected = ground.0.join("Control/user");
        fs::remove_dir(&selected).unwrap();
        observe("after_lookup", &selected, move |path| { fs::create_dir(path).unwrap(); });
        let error = locate_control_root(&ground.0, "user").unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::Other);
        assert!(locate_control_root(&ground.0, "user").unwrap().exists);
    }

    #[test]
    fn actual_lookup_permission_error_is_not_a_missing_aperture() {
        use std::os::unix::fs::PermissionsExt;
        if unsafe { libc::geteuid() } == 0 {
            eprintln!("qualification unavailable: actual aperture EACCES requires a nonroot OS user");
            return;
        }
        let ground = Ground::new();
        let directory = ground.0.join("Control/user");
        struct Restore(PathBuf);
        impl Drop for Restore { fn drop(&mut self) { let _ = fs::set_permissions(&self.0, fs::Permissions::from_mode(0o700)); } }
        let _restore = Restore(directory.clone());
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o000)).unwrap();
        let actual = fs::symlink_metadata(directory.join(AGENT_RETRIEVAL_DENY_MARKER)).unwrap_err();
        let error = locate_control_root(&ground.0, "user").unwrap_err();
        assert_eq!(error.kind(), actual.kind());
        assert_eq!(error.raw_os_error(), actual.raw_os_error());
        fs::set_permissions(directory, fs::Permissions::from_mode(0o700)).unwrap();
        assert!(locate_control_root(&ground.0, "user").unwrap().exists);
    }


    #[derive(Debug, PartialEq, Eq)]
    struct RetainedSource {
        bytes: Vec<u8>,
        device: u64,
        inode: u64,
        modified: i64,
        modified_nanos: i64,
    }
    fn retained_source(path: &Path) -> RetainedSource {
        let metadata = fs::metadata(path).unwrap();
        RetainedSource {
            bytes: fs::read(path).unwrap(), device: metadata.dev(), inode: metadata.ino(),
            modified: metadata.mtime(), modified_nanos: metadata.mtime_nsec(),
        }
    }
    fn fixture_relation(reference: &str, path: &str) -> serde_json::Value {
        serde_json::json!({
            "ref":reference, "path":path, "roles":["controlled-source-fixture"],
            "provenance":"agent-maintained", "standing":"durable-source", "treatment":"retain-native-in-place",
            "recognition":"controlled-test-fixture-not-personal-adoption", "extension":{"retained":true}
        })
    }
    fn fixture_relations(ground: &Ground, relations: Vec<serde_json::Value>) -> PathBuf {
        ground.write(SOURCE_RELATIONS, &serde_json::to_vec(&serde_json::json!({
            "schema":crate::source_horizon::CONTROL_GROUND_RELATIONS_SCHEMA,
            "project_id":crate::source_horizon::CONTROL_WORLD_REF, "relations":relations,
            "unknown_header":{"retained":"verbatim"}
        })).unwrap())
    }

    #[test]
    fn native_duplicate_component_spellings_refuse_without_effect() {
        let canonical = "Control/agents/governance/source.md";
        for alias in ["Control/agents/governance//source.md", "Control/agents/governance/./source.md",
            "Control/agents/governance/source.md/"] {
            let ground = Ground::new();
            let source = ground.write(canonical, b"# Native source\ncomponent-needle\n");
            let ledger = fixture_relations(&ground, vec![
                fixture_relation("opaque:fixture:first", canonical), fixture_relation("opaque:fixture:second", alias)]);
            let source_before = retained_source(&source);
            let ledger_before = retained_source(&ledger);
            let scope = crate::continuous_work::source::Scope::resolve(&ground.0, None).unwrap();
            let value: serde_json::Value = serde_json::from_slice(&ledger_before.bytes).unwrap();
            for error in [
                crate::source_horizon::control_source_bindings(&ground.0).unwrap_err(),
                crate::source_horizon::control_binding_for_path(&ground.0, canonical).unwrap_err(),
                crate::source_horizon::control_binding_for_observed_path(canonical, Some(&value), None, true).unwrap_err(),
                scope.relations().unwrap_err(), search_control(&ground.0, "component-needle").unwrap_err(),
                index_governance(&ground.0).unwrap_err(),
            ] {
                assert_eq!(error.kind(), io::ErrorKind::InvalidData);
                assert!(error.to_string().contains("ambiguous duplicate source relation"));
            }
            assert_eq!(retained_source(&source), source_before);
            assert_eq!(retained_source(&ledger), ledger_before);
        }
        // This is an actual raw file-form observation, separate from the pure
        // lexical comparison above. No native reader silently removes a slash.
        let ground = Ground::new();
        let source = ground.write(canonical, b"retained raw file bytes");
        let before = retained_source(&source);
        let trailing = format!("{canonical}/");
        let actual = fs::symlink_metadata(ground.0.join(&trailing)).unwrap_err();
        assert_eq!(actual.raw_os_error(), Some(libc::ENOTDIR));
        fixture_relations(&ground, vec![fixture_relation("opaque:fixture:trailing", &trailing)]);
        let native = crate::source_horizon::control_source_bindings(&ground.0).unwrap_err();
        assert_eq!(native.kind(), actual.kind());
        assert_eq!(native.raw_os_error(), actual.raw_os_error());
        assert_eq!(retained_source(&source), before);
    }

    #[test]
    fn single_legacy_member_spelling_preserves_identity_and_binding() {
        let canonical = "Control/agents/governance/source.md";
        for alias in ["Control/agents/governance//source.md", "Control/agents/governance/./source.md"] {
            let ground = Ground::new();
            let source = ground.write(canonical, b"# Legacy singleton\nlegacy-member-needle\n");
            let reference = "opaque:source:legacy-singleton";
            let ledger = fixture_relations(&ground, vec![fixture_relation(reference, alias)]);
            let before = (retained_source(&source), retained_source(&ledger));
            let key = crate::source_safety::normal_member_key(canonical).unwrap();
            let bindings = crate::source_horizon::control_source_bindings(&ground.0).unwrap();
            let same: Vec<_> = bindings.iter().filter(|binding|
                crate::source_safety::normal_member_key(&binding.path).unwrap() == key).collect();
            assert_eq!(same.len(), 1);
            assert_eq!(same[0].source_ref, reference);
            assert_eq!(same[0].path, alias);
            let selected = crate::source_horizon::control_binding_for_path(&ground.0, canonical).unwrap().unwrap();
            assert_eq!(&selected, same[0]);
            let scope = crate::continuous_work::source::Scope::resolve(&ground.0, None).unwrap();
            let reading = scope.read(reference).unwrap();
            assert_eq!(reading.source, selected);
            assert_eq!(reading.content, "# Legacy singleton\nlegacy-member-needle\n");
            let search = search_control(&ground.0, "legacy-member-needle").unwrap();
            assert_eq!(search.matches.len(), 1);
            assert_eq!(search.matches[0].source_binding.as_ref(), Some(&selected));
            assert_eq!(search.matches[0].source_revision, reading.revision);
            assert_eq!(search.matches[0].source_class, SourceClass::Unresolved);
            assert_eq!(index_governance(&ground.0).unwrap().statements[0].standing, "durable");
            assert_eq!(retained_source(&source), before.0);
            assert_eq!(retained_source(&ledger), before.1);
        }
    }

    #[test]
    fn opaque_refs_and_noncolliding_members_remain_independent() {
        let ground = Ground::new();
        let paths = ["Control/agents/governance/a space.md", "Control/agents/governance/a:colon.md"];
        let references = ["opaque:source:not-derived-from-first-path", "opaque:source:not-derived-from-second-path"];
        let first = ground.write(paths[0], b"first opaque-needle\n");
        let second = ground.write(paths[1], b"second opaque-needle\n");
        let ledger = fixture_relations(&ground, vec![fixture_relation(references[0], paths[0]), fixture_relation(references[1], paths[1])]);
        let before = (retained_source(&first), retained_source(&second), retained_source(&ledger));
        let scope = crate::continuous_work::source::Scope::resolve(&ground.0, None).unwrap();
        for (index, (path, reference)) in paths.iter().zip(references).enumerate() {
            let reading = scope.read(reference).unwrap();
            assert_eq!(reading.source.path, *path);
            assert_eq!(reading.source.source_ref, reference);
            assert_eq!(reading.content, if index == 0 { "first opaque-needle\n" } else { "second opaque-needle\n" });
        }
        let search = search_control(&ground.0, "opaque-needle").unwrap();
        assert_eq!(search.matches.len(), 2);
        for reference in references {
            assert_eq!(search.matches.iter().filter(|hit|
                hit.source_binding.as_ref().is_some_and(|binding| binding.source_ref == reference)).count(), 1);
        }
        assert_eq!(retained_source(&first), before.0);
        assert_eq!(retained_source(&second), before.1);
        assert_eq!(retained_source(&ledger), before.2);
    }

    #[test]
    fn native_bind_same_member_is_idempotent_or_conflict_before_write() {
        let ground = Ground::new();
        let source = ground.write("Control/agents/governance/source.md", b"retained binding bytes\n");
        let alias = "Control/agents/governance//source.md";
        let reference = "opaque:source:existing-binding";
        let ledger = fixture_relations(&ground, vec![fixture_relation(reference, alias)]);
        let before = (retained_source(&source), retained_source(&ledger));
        let scope = crate::continuous_work::source::Scope::resolve(&ground.0, None).unwrap();
        let mut binding = crate::source_horizon::control_binding_for_path(&ground.0, alias).unwrap().unwrap();
        let _lock = crate::source_safety::lock(&ground.0, "source-mutation.lock").unwrap();
        binding.path = "Control/agents/governance/./source.md".into();
        scope.bind(&binding, 101).unwrap();
        assert_eq!(retained_source(&ledger), before.1);
        binding.source_ref = "opaque:source:competing-binding".into();
        assert_eq!(scope.bind(&binding, 102).unwrap_err().kind(), io::ErrorKind::AlreadyExists);
        binding.source_ref = reference.into();
        binding.path = "Control/agents/governance/another.md".into();
        assert_eq!(scope.bind(&binding, 103).unwrap_err().kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(retained_source(&source), before.0);
        assert_eq!(retained_source(&ledger), before.1);
    }

    #[test]
    fn native_project_and_control_override_remove_only_matching_fallback() {
        let ground = Ground::new();
        ground.write("Control/agents/governance/source.md", b"root retained bytes\n");
        ground.write("Control/agents/governance/sibling.md", b"root sibling bytes\n");
        fixture_relations(&ground, vec![fixture_relation("opaque:root:declared", "Control/agents/governance//source.md")]);
        let project = ground.0.join("Work/native-member");
        fs::create_dir(&project).unwrap();
        crate::initialize_projectcentral(&ground.0, &project, "opaque-project:member").unwrap();
        ground.write("Work/native-member/ProjectCentral/user/source.md", b"project retained bytes\n");
        ground.write("Work/native-member/ProjectCentral/user/sibling.md", b"project sibling bytes\n");
        let project_ledger = ground.write("Work/native-member/ProjectCentral/relations/source-relations.json",
            &serde_json::to_vec(&serde_json::json!({
                "schema":crate::source_horizon::GROUND_RELATIONS_SCHEMA, "project_id":"opaque-project:member",
                "relations":[fixture_relation("opaque:project:declared", "ProjectCentral/user/./source.md")],
                "unknown_header":{"retain":true}
            })).unwrap());
        let root_ledger = ground.0.join(SOURCE_RELATIONS);
        let before = (retained_source(&root_ledger), retained_source(&project_ledger));
        for (bindings, path, sibling, reference) in [
            (crate::source_horizon::control_source_bindings(&ground.0).unwrap(),
                "Control/agents/governance/source.md", "Control/agents/governance/sibling.md", "opaque:root:declared"),
            (crate::source_horizon::project_source_bindings(&project).unwrap(),
                "ProjectCentral/user/source.md", "ProjectCentral/user/sibling.md", "opaque:project:declared"),
        ] {
            let key = crate::source_safety::normal_member_key(path).unwrap();
            let selected: Vec<_> = bindings.iter().filter(|binding|
                crate::source_safety::normal_member_key(&binding.path).unwrap() == key).collect();
            assert_eq!(selected.len(), 1);
            assert_eq!(selected[0].source_ref, reference);
            assert_eq!(bindings.iter().filter(|binding| binding.path == sibling).count(), 1);
        }
        let scope = crate::continuous_work::source::Scope::resolve(&ground.0, Some("native-member")).unwrap();
        assert_eq!(scope.read("opaque:project:declared").unwrap().content, "project retained bytes\n");
        assert_eq!(retained_source(&root_ledger), before.0);
        assert_eq!(retained_source(&project_ledger), before.1);
    }

    #[test]
    fn selected_skill_override_avoids_unselected_manifest_authority() {
        let ground = Ground::new();
        let source = ground.write("Control/user/skills/example/SKILL.md", b"selected-skill-needle\n");
        let manifest = ground.write("Control/user/skills/example/skill.json", b"{not valid native Skill JSON");
        let ledger = fixture_relations(&ground, vec![
            fixture_relation("opaque:source:declared-skill", "Control/user/skills/example//SKILL.md"),
            // Search legitimately visits the manifest as another source. Its
            // own explicit relation also prevents it from becoming authority
            // for the already-declared Skill body.
            fixture_relation("opaque:source:declared-manifest", "Control/user/skills/example/skill.json"),
        ]);
        let before = (retained_source(&source), retained_source(&manifest), retained_source(&ledger));
        let selected = crate::source_horizon::control_binding_for_path(&ground.0,
            "Control/user/skills/example/SKILL.md").unwrap().unwrap();
        assert_eq!(selected.source_ref, "opaque:source:declared-skill");
        assert_eq!(selected.path, "Control/user/skills/example//SKILL.md");
        let search = search_control(&ground.0, "selected-skill-needle").unwrap();
        assert_eq!(search.matches.len(), 1);
        assert_eq!(search.matches[0].source_binding.as_ref(), Some(&selected));
        ground.write("Control/user/skills/nonoverridden/SKILL.md", b"unselected-skill-needle\n");
        let unrelated = ground.write("Control/user/skills/nonoverridden/skill.json", b"{genuine malformed manifest");
        let native = crate::source_horizon::control_binding_for_path(&ground.0,
            "Control/user/skills/nonoverridden/SKILL.md").unwrap_err();
        let actual = crate::control_skills::read_skill_manifest(unrelated.parent().unwrap()).unwrap_err();
        assert_eq!(native.kind(), actual.kind());
        assert_eq!(native.to_string(), actual.to_string());
        assert_eq!(search_control(&ground.0, "selected-skill-needle").unwrap_err().kind(), actual.kind());
        assert_eq!(retained_source(&source), before.0);
        assert_eq!(retained_source(&manifest), before.1);
        assert_eq!(retained_source(&ledger), before.2);
    }

    #[test]
    fn native_migration_destination_identity_uses_same_member_key_before_plan_effects() {
        use crate::continuous_work::{placement, source, execute_with_token_at};
        let ground = Ground::new();
        let original = ground.write("Control/user/old-day.md", b"retained migration source\r\n");
        let policy_path = "Control/user/test-placement.json";
        let authority_path = "Control/user/test-authority.json";
        let token = "native-member-migration-controlled-fixture-credential-0001";
        ground.write(policy_path, &serde_json::to_vec(&serde_json::json!({
            "schema":placement::POLICY_SCHEMA,"scope_ref":"control:root","authority_refs":[],
            "writable":[],"protected":[],"enforcement":"harness-interception",
            "required_coverage":["filesystem"],"lease_seconds":300,"expires_at_unix_seconds":9999
        })).unwrap());
        ground.write(authority_path, &serde_json::to_vec(&serde_json::json!({
            "schema":"central.native-action-authority/v1","scope_ref":"control:root","grants":[{
                "principal_ref":"human:controlled-fixture","actor_kind":"human","token_sha256":source::key(token),
                "scope_refs":["control:root"],"actions":["central.migration.plan"],"expires_at_unix_seconds":9999
            }]
        })).unwrap());
        let mut policy_relation = fixture_relation("opaque:fixture:placement", policy_path);
        policy_relation["roles"] = serde_json::json!([placement::POLICY_ROLE]);
        policy_relation["provenance"] = serde_json::json!("human-adopted");
        policy_relation["standing"] = serde_json::json!("architecture-contract");
        let mut authority_relation = fixture_relation("opaque:fixture:authority", authority_path);
        authority_relation["roles"] = serde_json::json!(["native-action-authority"]);
        authority_relation["provenance"] = serde_json::json!("human-adopted");
        authority_relation["standing"] = serde_json::json!("architecture-contract");
        let ledger = fixture_relations(&ground, vec![policy_relation, authority_relation,
            fixture_relation("opaque:fixture:moving-source", "Control/user/old-day.md"),
            fixture_relation("opaque:fixture:already-reserved-destination", "Control/user/day//retained/day.md"),
        ]);
        let scope = source::Scope::resolve(&ground.0, None).unwrap();
        let revision = scope.read("opaque:fixture:moving-source").unwrap().revision.revision;
        let policy = placement::effective_policy(&scope, 100).unwrap();
        let before = (retained_source(&original), retained_source(&ledger));
        let input = serde_json::json!({"request_id":"native-member-conflict","expected_policy_revision":policy.revision,
            "moves":[{"source_ref":"opaque:fixture:moving-source","expected_revision":revision,
                "to":"Control/user/day/retained/day.md"}]});
        let error = execute_with_token_at(&ground.0, "migration_plan", &input, Some(token), 100).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
        assert!(error.to_string().contains("destination is already another SourceRef's identity"));
        assert!(!ground.0.join("Control/user/day/retained/day.md").exists());
        assert!(!ground.0.join(".central/temporal-migrations").exists());
        assert_eq!(retained_source(&original), before.0);
        assert_eq!(retained_source(&ledger), before.1);
    }
}
