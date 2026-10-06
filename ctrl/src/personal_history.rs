//! central.personal-collection/v1 — personal-history intake through the
//! existing adoption machinery (Central #242).
//!
//! A person brings earlier writing into their Central world through the same
//! source identity, relations and placement law every other adoption uses:
//! entries become ordinary registered sources (`central.file-map`), relations
//! are written only by an explicitly human-accepted apply, and the durable
//! intake facts live in one ordinary collection record
//! (`central.personal-collection/v1`). Nothing here invents a second
//! migration controller, a profile database, or present-day activity out of
//! historical material.
//!
//! Time is kept separate at every seam: `event_date` (with basis and
//! approximation) is the entry's own time; file mtimes stay writing/revision
//! time; import receipts carry import time; interpretation time belongs to
//! the downstream knowledge stage.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::action::{
    ActionAvailability, ActionDescriptor, ActionExecutionContext, ActionInputDefinition,
    ActionOutputDefinition, ActionRegistry, MutationClass,
};
use crate::file_map_catalog::{write_atomic, Resource, Scope};
use crate::file_map_index;
use crate::pasu::{PasuIdentityState, PasuRef};
use crate::result::{ActionResult, ResultStatus};
use crate::root::resolve_central_root;
use crate::source_horizon::{self, SourceRevision};
use crate::source_safety;

pub const PERSONAL_COLLECTION_SCHEMA: &str = "central.personal-collection/v1";
pub const COLLECTIONS_DIR: &str = "Control/user/collections";
pub const PROJECT_COLLECTIONS_DIR: &str = "ProjectCentral/user/collections";
pub const ROLE_COLLECTION: &str = "personal-history-collection";
pub const ROLE_ENTRY: &str = "personal-history-entry";
pub const ROLE_RECORD: &str = "personal-collection-record";
pub const MARKDOWN_ADAPTER: &str = "markdown-frontmatter@1";
/// Walk and journal bounds: intake must stay useful on real archives without
/// pretending unbounded work fit in one apply.
pub(crate) const MAX_MEMBERS: usize = 100_000;
pub(crate) const MAX_DEPTH: usize = 32;

// ---------------------------------------------------------------------------
// Record model
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Placement {
    /// `copy` | `retain-in-place` | `registered`
    pub mode: String,
    /// World-relative home of the retained material (copy/registered) or of
    /// the collection record (retain-in-place).
    pub home: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accepted_plan_revision: Option<String>,
}

/// A superseded retained revision: which import replaced it, and what the
/// previous content revision was. Bytes of the previous revision remain at
/// the origin archive; the alternatives area under `.central` keeps a restore
/// point until that import is rolled back.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AlternativeRevision {
    pub content_revision: String,
    pub replaced_by_import: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CollectionEntry {
    /// Collection-relative path of the member — the stable native identity.
    pub entry_id: String,
    /// `entry` | `attachment` | `record`
    pub role: String,
    /// World-relative path of the retained bytes (absolute for external
    /// retained-in-place origins).
    pub path: String,
    /// Derived from the path; never stored truth.
    pub source_ref: String,
    pub entry_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event_date: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date_basis: Option<String>,
    #[serde(default)]
    pub date_approximate: bool,
    pub content_revision: String,
    pub bytes: u64,
    /// `retained` | `excluded-by-selection` | `unreadable`
    pub disposition: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub readable: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub entry_meta: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub alternatives: Vec<AlternativeRevision>,
    pub first_import: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportReceipt {
    pub sequence: u64,
    pub applied_at_unix_seconds: u64,
    pub adapter: String,
    pub entries_added: u64,
    pub entries_changed: u64,
    pub entries_unchanged: u64,
    pub entries_unchanged_at_origin_absent: u64,
    pub accepted_plan_revision: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersonalCollectionRecord {
    pub schema: String,
    pub collection_id: String,
    pub title: String,
    pub world_ref: String,
    /// The `about`/subject binding: whose history this is.
    pub person_ref: String,
    /// The narrator. Importer, author and subject may all differ.
    pub author_ref: String,
    pub retained_by: String,
    pub placement: Placement,
    pub entries: Vec<CollectionEntry>,
    pub imports: Vec<ImportReceipt>,
}

/// The collections home of a scope: root register keeps them under
/// `Control/user/collections`, a Project under its own ProjectCentral/user.
pub fn collections_dir(world_ref: &str) -> &'static str {
    if world_ref == "control:root" {
        COLLECTIONS_DIR
    } else {
        PROJECT_COLLECTIONS_DIR
    }
}

impl PersonalCollectionRecord {
    pub fn record_path(collection_id: &str) -> String {
        format!("{COLLECTIONS_DIR}/{collection_id}/collection.json")
    }

    pub fn record_path_for(world_ref: &str, collection_id: &str) -> String {
        format!(
            "{}/{collection_id}/collection.json",
            collections_dir(world_ref)
        )
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.schema != PERSONAL_COLLECTION_SCHEMA {
            return Err(format!("schema must be {PERSONAL_COLLECTION_SCHEMA}"));
        }
        if self.collection_id.is_empty()
            || !self
                .collection_id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
        {
            return Err("collection_id must be non-empty [a-zA-Z0-9_-]".to_owned());
        }
        PasuRef::parse(&self.person_ref)
            .map_err(|error| format!("person_ref is invalid: {error}"))?;
        if !self.person_ref.starts_with("central:pasu:nara:") {
            return Err("person_ref must name a nara-form subject".to_owned());
        }
        PasuRef::parse(&self.author_ref)
            .map_err(|error| format!("author_ref is invalid: {error}"))?;
        if self.placement.home.is_empty() {
            return Err("placement.home is required".to_owned());
        }
        let mut seen = std::collections::BTreeSet::new();
        for entry in &self.entries {
            if !seen.insert(entry.entry_id.as_str()) {
                return Err(format!("duplicate entry_id {}", entry.entry_id));
            }
        }
        Ok(())
    }

    pub fn record_revision(&self, root: &Path) -> io::Result<String> {
        Ok(content_revision(
            &root.join(Self::record_path_for(&self.world_ref, &self.collection_id)),
        )?
        .revision)
    }

    pub fn load(root: &Path, collection_id: &str) -> io::Result<Option<PersonalCollectionRecord>> {
        Self::load_scoped(root, "control:root", collection_id)
    }

    pub fn load_scoped(
        root: &Path,
        world_ref: &str,
        collection_id: &str,
    ) -> io::Result<Option<PersonalCollectionRecord>> {
        let path = root.join(Self::record_path_for(world_ref, collection_id));
        if !path.is_file() {
            return Ok(None);
        }
        let record: PersonalCollectionRecord = serde_json::from_slice(&fs::read(&path)?)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        record
            .validate()
            .map_err(|message| io::Error::new(io::ErrorKind::InvalidData, message))?;
        Ok(Some(record))
    }

    pub fn persist(&self, root: &Path) -> io::Result<PathBuf> {
        self.validate()
            .map_err(|message| io::Error::new(io::ErrorKind::InvalidData, message))?;
        let path = root.join(Self::record_path_for(&self.world_ref, &self.collection_id));
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut body = serde_json::to_vec_pretty(self).map_err(io::Error::other)?;
        body.push(b'\n');
        write_atomic(&path, &body)?;
        Ok(path)
    }
}

// ---------------------------------------------------------------------------
// Intake adapter: member enumeration and classification
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MemberReading {
    pub entry_id: String,
    pub role: String,
    pub entry_type: String,
    pub disposition: String,
    pub bytes: u64,
    pub content_revision: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date_basis: Option<String>,
    pub date_approximate: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub readable: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    pub entry_meta: BTreeMap<String, String>,
}

fn fnv(bytes: &[u8]) -> SourceRevision {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    SourceRevision {
        revision: format!("central.content-fnv1a64/v1:{}:{hash:016x}", bytes.len()),
        byte_len: bytes.len() as u64,
    }
}

fn content_revision(path: &Path) -> io::Result<SourceRevision> {
    Ok(fnv(&fs::read(path)?))
}

struct Frontmatter {
    meta: BTreeMap<String, String>,
}

/// Minimal frontmatter reader: `key: value` lines between `---` markers.
/// Values are kept verbatim; nothing is interpreted into authority.
fn read_frontmatter(bytes: &[u8]) -> Option<Frontmatter> {
    let text = std::str::from_utf8(bytes).ok()?;
    let mut lines = text.lines();
    if lines.next()?.trim_end() != "---" {
        return None;
    }
    let mut meta = BTreeMap::new();
    for line in lines {
        let trimmed = line.trim_end();
        if trimmed == "---" {
            return Some(Frontmatter { meta });
        }
        if let Some((key, value)) = trimmed.split_once(':') {
            let key = key.trim();
            let value = value.trim().trim_matches('"').trim_matches('\'');
            if !key.is_empty() && !key.contains(' ') {
                meta.insert(key.to_owned(), value.to_owned());
            }
        }
    }
    None
}

fn looks_textual(path: &Path) -> bool {
    const TEXTUAL: &[&str] = &[
        "md", "markdown", "txt", "text", "org", "rst", "adoc", "html", "htm", "json", "yaml",
        "yml", "toml", "csv", "canvas", "vtt", "srt", "py", "sh", "rb", "js", "ts", "css", "svg",
    ];
    match path.extension().and_then(|e| e.to_str()) {
        Some(ext) => TEXTUAL.contains(&ext.to_ascii_lowercase().as_str()),
        None => false,
    }
}

fn classification(entry_type: &str) -> &'static str {
    match entry_type {
        "dream" | "quotation" | "poem" | "letter" | "essay" | "correction" | "journal" | "note" => {
            "entry"
        }
        "canvas" | "board" => "record",
        _ => "attachment",
    }
}

/// Enumerate one collection directory. The adapter keeps whole files intact —
/// extraction, not intake, is where spans and summaries belong.
pub fn read_members(origin: &Path, adapter: &str) -> io::Result<Vec<MemberReading>> {
    if adapter != MARKDOWN_ADAPTER {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("unsupported collection adapter: {adapter}"),
        ));
    }
    if !origin.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("collection origin is not a directory: {}", origin.display()),
        ));
    }
    let mut members = Vec::new();
    let mut stack = vec![(origin.to_path_buf(), 0usize)];
    while let Some((dir, depth)) = stack.pop() {
        if depth > MAX_DEPTH {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "collection nesting exceeds the intake depth bound",
            ));
        }
        let mut entries: Vec<_> = fs::read_dir(&dir)?.collect::<Result<Vec<_>, _>>()?;
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue;
            }
            if path.is_dir() {
                stack.push((path, depth + 1));
                continue;
            }
            if !path.is_file() {
                continue;
            }
            if members.len() >= MAX_MEMBERS {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "collection exceeds the intake member bound",
                ));
            }
            let relative = path
                .strip_prefix(origin)
                .map_err(|_| io::Error::other("member escapes origin"))?
                .to_string_lossy()
                .to_string();
            members.push(classify_member(&relative, &path)?);
        }
    }
    members.sort_by(|a, b| a.entry_id.cmp(&b.entry_id));
    Ok(members)
}

fn classify_member(relative: &str, path: &Path) -> io::Result<MemberReading> {
    let revision = content_revision(path)?;
    let lower = relative.to_ascii_lowercase();
    // A `.no-agent-retrieval` marker excludes its subtree from agent retrieval;
    // intake honours the policy instead of registering past it.
    if source_retrieval_excluded(path)? {
        return Ok(MemberReading {
            entry_id: relative.to_owned(),
            role: "attachment".to_owned(),
            entry_type: "excluded".to_owned(),
            disposition: "excluded-by-selection".to_owned(),
            bytes: revision.byte_len,
            content_revision: revision.revision,
            event_date: None,
            date_basis: None,
            date_approximate: false,
            readable: None,
            reason: Some("excluded by a .no-agent-retrieval marker above this member".to_owned()),
            entry_meta: BTreeMap::new(),
        });
    }
    let bytes = fs::read(path)?;
    let is_markdown = lower.ends_with(".md") || lower.ends_with(".markdown");
    let front = if is_markdown {
        read_frontmatter(&bytes)
    } else {
        None
    };
    let mut meta = BTreeMap::new();
    let mut entry_type = if lower.ends_with(".canvas") {
        "canvas".to_owned()
    } else if is_markdown {
        "journal".to_owned()
    } else if looks_textual(path) {
        "note".to_owned()
    } else {
        "attachment".to_owned()
    };
    let mut event_date = None;
    let mut date_basis = None;
    let mut approximate = false;
    if let Some(front) = front {
        for (key, value) in front.meta {
            match key.as_str() {
                "date" | "created" => {
                    let (date, is_approximate) = parse_date(&value);
                    if date.is_some() {
                        event_date = date;
                        date_basis = Some(format!("frontmatter {key}"));
                        approximate = is_approximate;
                    }
                }
                "type" => {
                    if !value.is_empty() {
                        meta.insert(key.clone(), value.clone());
                        entry_type = value.to_ascii_lowercase();
                    }
                }
                _ => {
                    if !value.is_empty() {
                        meta.insert(key, value);
                    }
                }
            }
        }
    }
    let (readable, reason) = match std::str::from_utf8(&bytes) {
        Ok(_) => (Some(true), None),
        Err(_) => (
            Some(false),
            Some("member is not valid UTF-8; retained as bytes, not readable as text".to_owned()),
        ),
    };
    let disposition = if readable == Some(false) {
        "unreadable".to_owned()
    } else {
        "retained".to_owned()
    };
    Ok(MemberReading {
        entry_id: relative.to_owned(),
        role: classification(&entry_type).to_owned(),
        entry_type,
        disposition,
        bytes: revision.byte_len,
        content_revision: revision.revision,
        event_date,
        date_basis,
        date_approximate: approximate,
        readable,
        reason,
        entry_meta: meta,
    })
}

fn source_retrieval_excluded(path: &Path) -> io::Result<bool> {
    let mut dir = path.parent();
    while let Some(current) = dir {
        if current.join(".no-agent-retrieval").exists() {
            return Ok(true);
        }
        dir = current.parent();
    }
    Ok(false)
}

/// `2026-03-14`, `2026-03-14T…`, or `~2026`. Unknown shapes stay approximate
/// rather than being invented into precision.
fn parse_date(raw: &str) -> (Option<String>, bool) {
    let value = raw.trim();
    if value.is_empty() {
        return (None, false);
    }
    let approximate =
        value.starts_with('~') || value.starts_with("ca ") || value.starts_with("c. ");
    let bare = value
        .trim_start_matches('~')
        .trim_start_matches("ca ")
        .trim_start_matches("c. ")
        .trim_start();
    // Take the date part only; a time suffix never changes the day.
    let date_part: String = bare
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '-')
        .collect();
    let mut parts = date_part.split('-').filter(|part| !part.is_empty());
    let year = parts.next().and_then(|part| part.parse::<u32>().ok());
    let Some(year) = year.filter(|year| (1000..=9999).contains(year)) else {
        return (None, true);
    };
    let month = parts.next().and_then(|part| part.parse::<u32>().ok());
    let day = parts.next().and_then(|part| part.parse::<u32>().ok());
    match (month, day) {
        (Some(m), Some(d)) if (1..=12).contains(&m) && (1..=31).contains(&d) => {
            (Some(format!("{year:04}-{m:02}-{d:02}")), approximate)
        }
        (Some(m), _) if (1..=12).contains(&m) => (Some(format!("{year:04}-{m:02}")), true),
        _ => (Some(format!("{year:04}")), true),
    }
}

// ---------------------------------------------------------------------------
// Placement plan
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanEntry {
    /// `copy-register` (new member) | `update` (changed member) |
    /// `register` (retain-in-place) | `none` (identical or excluded)
    pub action: String,
    pub entry_id: String,
    pub role: String,
    pub entry_type: String,
    pub disposition: String,
    pub bytes: u64,
    pub content_revision: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub origin: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub destination: Option<String>,
    pub source_ref: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prior_revision: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date_basis: Option<String>,
    pub date_approximate: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub readable: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    pub entry_meta: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CollectionPlan {
    pub schema: String,
    pub collection_id: String,
    pub title: String,
    pub world_ref: String,
    pub project: Option<String>,
    pub person_ref: String,
    pub author_ref: String,
    pub adapter: String,
    pub placement: Placement,
    pub entries: Vec<PlanEntry>,
    /// Members of an existing record whose origin has disappeared. Reported,
    /// never deleted by intake.
    pub origin_absent: Vec<String>,
    pub conflicts: Vec<String>,
    /// Retained copies edited after import while the origin stood still. Not
    /// a blocker — later edits win — but the person should see them.
    #[serde(default)]
    pub divergences: Vec<String>,
    pub counts: BTreeMap<String, u64>,
    pub plan_revision: String,
    pub undo_summary: String,
}

fn slug(raw: &str) -> String {
    let dashed: String = raw
        .trim()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let compact = dashed
        .split('-')
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("-")
        .to_ascii_lowercase();
    if compact.is_empty() {
        "collection".to_owned()
    } else {
        compact.chars().take(64).collect()
    }
}

fn scope_for(root: &Path, project: Option<&str>) -> io::Result<Scope> {
    match project {
        None => Ok(Scope {
            root: root.canonicalize()?,
            world: "control:root".to_owned(),
            project: None,
        }),
        Some(project) => {
            let project_root = root.join("Work").join(project);
            if !project_root.is_dir() {
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("project is not a Work member: {project}"),
                ));
            }
            if !project_root.join("ProjectCentral/project.json").is_file() {
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    format!(
                        "project {project} has no ProjectCentral; run projectcentral.init first"
                    ),
                ));
            }
            Scope::project(project_root.canonicalize()?, Some(project.to_owned()))
        }
    }
}

/// Resolve a caller path against a scope: absolute stays absolute; otherwise
/// scope-relative first, with a `Work/<project>/…` world-relative fallback so
/// one spelling works from both directions.
fn resolve_origin(scope: &Scope, raw: &str) -> PathBuf {
    let candidate = Path::new(raw);
    if candidate.is_absolute() {
        return candidate.to_path_buf();
    }
    let scope_relative = scope.root.join(candidate);
    if scope_relative.exists() {
        return scope_relative;
    }
    if let Some(project) = &scope.project {
        let world_relative = scope
            .root
            .parent()
            .map(|work| work.join("Work"))
            .unwrap_or_else(|| scope.root.clone());
        let _ = world_relative;
        if let Some(central_root) = scope
            .root
            .ancestors()
            .find(|ancestor| ancestor.join("Control").is_dir() && ancestor.join("Work").is_dir())
        {
            let from_world = central_root.join("Work").join(project).join(candidate);
            if from_world.exists() {
                return from_world;
            }
            let stripped = candidate.strip_prefix(format!("Work/{project}")).ok();
            if let Some(stripped) = stripped {
                let from_project = scope.root.join(stripped);
                if from_project.exists() {
                    return from_project;
                }
            }
        }
    }
    scope_relative
}

fn world_relative(scope: &Scope, path: &Path) -> String {
    path.strip_prefix(&scope.root)
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| path.to_string_lossy().to_string())
}

/// Resolve the person anchor for this world. The manifest is the anchor; a
/// caller-named person must be exactly the manifest subject — a different
/// person with the same display name is a different `pasu` id and is refused
/// here rather than silently merged.
fn resolve_person(root: &Path, requested: Option<&str>) -> Result<String, String> {
    let state = PasuIdentityState::read(root, None);
    if !state.present {
        return Err(
            "this world has no person anchor: Control/user/identity/manifest.json is absent"
                .to_owned(),
        );
    }
    let subject = state
        .subject_ref
        .clone()
        .ok_or_else(|| "person anchor is present but names no subject".to_owned())?;
    if let Some(requested) = requested {
        if requested != subject {
            return Err(format!(
                "requested person {requested} is not this world's anchored subject {subject}; \
                 importing another person's material keeps the anchored person as subject and \
                 names the narrator through author_ref"
            ));
        }
    }
    Ok(subject)
}

/// The plan's own identity: the revision of its body with the identity field
/// zeroed, so a round-tripped plan document can be re-verified exactly.
pub(crate) fn plan_identity(plan: &CollectionPlan) -> Result<String, String> {
    let mut body = plan.clone();
    body.plan_revision = String::new();
    Ok(fnv(serde_json::to_vec(&body)
        .map_err(|e| e.to_string())?
        .as_slice())
    .revision)
}

#[derive(Debug, Clone, Default)]
pub struct PlanRequest {
    pub path: Option<String>,
    pub project: Option<String>,
    pub collection_id: Option<String>,
    pub title: Option<String>,
    pub person_ref: Option<String>,
    pub author_ref: Option<String>,
    pub mode: Option<String>,
    pub home: Option<String>,
    pub adapter: Option<String>,
    pub exclude: Vec<String>,
}

pub fn build_plan(root: &Path, input: &PlanRequest) -> Result<CollectionPlan, String> {
    let scope = scope_for(root, input.project.as_deref()).map_err(|e| e.to_string())?;
    let origin_input = input
        .path
        .as_ref()
        .ok_or_else(|| "collection path is required".to_owned())?;
    let origin = resolve_origin(&scope, origin_input);
    let external = origin.is_absolute() && !origin.starts_with(&scope.root);
    let canonical = origin
        .canonicalize()
        .map_err(|error| format!("collection path is unreadable: {error}"))?;
    if !canonical.is_dir() {
        return Err(format!(
            "collection path is not a directory: {}",
            canonical.display()
        ));
    }
    let collection_id = match input.collection_id.as_deref() {
        Some(id) => {
            if id.is_empty()
                || !id
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
            {
                return Err("collection_id must be non-empty [a-zA-Z0-9_-]".to_owned());
            }
            id.to_owned()
        }
        None => slug(
            &canonical
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default(),
        ),
    };
    let existing = PersonalCollectionRecord::load_scoped(root, &scope.world, &collection_id)
        .map_err(|e| e.to_string())?;
    // The person anchor belongs to the Central root at every scope.
    let person_ref = resolve_person(root, input.person_ref.as_deref())?;
    if let Some(record) = &existing {
        if record.person_ref != person_ref {
            return Err(format!(
                "collection {collection_id} belongs to {}; re-import by a different person \
                 would merge two people's history",
                record.person_ref
            ));
        }
    }
    let author_ref = input
        .author_ref
        .clone()
        .unwrap_or_else(|| person_ref.clone());
    if author_ref != person_ref {
        PasuRef::parse(&author_ref).map_err(|error| format!("author_ref is invalid: {error}"))?;
    }
    let adapter = input
        .adapter
        .clone()
        .unwrap_or_else(|| MARKDOWN_ADAPTER.to_owned());
    let members = read_members(&canonical, &adapter).map_err(|e| e.to_string())?;

    let inside_world = !external;
    let inside_home = world_relative(&scope, &canonical)
        .starts_with(&format!("{}/", collections_dir(&scope.world)));
    let mode = input.mode.clone().unwrap_or_else(|| {
        if inside_home {
            "registered".to_owned()
        } else if inside_world {
            "retain-in-place".to_owned()
        } else {
            "copy".to_owned()
        }
    });
    if !matches!(mode.as_str(), "copy" | "retain-in-place" | "registered") {
        return Err(format!("unsupported placement mode: {mode}"));
    }
    if mode == "copy" && inside_home {
        return Err(
            "the collection is already inside the collections home; use the registered mode"
                .to_owned(),
        );
    }
    let _ = COLLECTIONS_DIR;
    if let Some(record) = &existing {
        if record.placement.mode != mode {
            return Err(format!(
                "collection {collection_id} was placed with mode {}; a re-import cannot change \
                 placement implicitly — roll the import back and re-plan instead",
                record.placement.mode
            ));
        }
    }
    let dir = collections_dir(&scope.world);
    let home = match mode.as_str() {
        "copy" => input
            .home
            .clone()
            .unwrap_or_else(|| format!("{dir}/{collection_id}")),
        _ => input.home.clone().unwrap_or_else(|| {
            if inside_home {
                world_relative(&scope, &canonical)
            } else {
                format!("{dir}/{collection_id}")
            }
        }),
    };
    if home.starts_with('/') || home.contains("..") {
        return Err("home must be a contained world-relative path".to_owned());
    }

    let mut conflicts = Vec::new();
    let mut divergences = Vec::new();
    let mut origin_absent = Vec::new();
    let mut entries = Vec::new();
    let mut counts = BTreeMap::new();
    let bump = |counts: &mut BTreeMap<String, u64>, key: &str| {
        *counts.entry(key.to_owned()).or_insert(0) += 1;
    };

    let by_id: BTreeMap<String, CollectionEntry> = existing
        .as_ref()
        .map(|record| {
            record
                .entries
                .iter()
                .map(|entry| (entry.entry_id.clone(), entry.clone()))
                .collect()
        })
        .unwrap_or_default();

    for member in &members {
        if input
            .exclude
            .iter()
            .any(|prefix| member.entry_id.starts_with(prefix.as_str()))
        {
            continue;
        }
        let destination = match mode.as_str() {
            "copy" | "registered" => Some(format!("{home}/{}", member.entry_id)),
            _ => None,
        };
        let previous = by_id.get(&member.entry_id);
        let action = if member.disposition == "excluded-by-selection" {
            "none"
        } else if mode == "retain-in-place" {
            // The origin is the source: re-reading it just refreshes record
            // state; registration is idempotent by SourceRef.
            if previous.is_some() {
                "none"
            } else {
                "register"
            }
        } else {
            match previous {
                None => "copy-register",
                Some(prior) => {
                    if prior.content_revision == member.content_revision {
                        // Origin stood still; surface a retained copy edited
                        // after import as a divergence, never as a target.
                        let retained_path = scope.root.join(prior.path.trim_start_matches('/'));
                        let retained_current = if prior.path.starts_with('/') {
                            content_revision(Path::new(&prior.path)).ok()
                        } else {
                            content_revision(&retained_path).ok()
                        };
                        if let Some(current) = retained_current {
                            if current.revision != prior.content_revision {
                                divergences.push(format!(
                                    "{}: retained copy was edited after import (recorded {}, \
                                     retained {}); the later edit stands",
                                    member.entry_id, prior.content_revision, current.revision
                                ));
                            }
                        }
                        "none"
                    } else {
                        // Changed at origin: an update is allowed only when the
                        // retained copy still matches what was imported. A
                        // divergent destination is later human editing — it is
                        // reported, never overwritten.
                        let retained_path = scope.root.join(prior.path.trim_start_matches('/'));
                        let retained_current = if prior.path.starts_with('/') {
                            content_revision(Path::new(&prior.path)).ok()
                        } else {
                            content_revision(&retained_path).ok()
                        };
                        match retained_current {
                            Some(current) if current.revision == prior.content_revision => "update",
                            Some(current) => {
                                conflicts.push(format!(
                                    "{}: retained copy was edited after import (recorded {}, \
                                     retained {}); reconcile it before importing this change",
                                    member.entry_id, prior.content_revision, current.revision
                                ));
                                "none"
                            }
                            None => {
                                conflicts.push(format!(
                                    "{}: retained copy is missing; re-plan after recovering it",
                                    member.entry_id
                                ));
                                "none"
                            }
                        }
                    }
                }
            }
        };
        if let Some(destination) = &destination {
            if matches!(action, "copy-register") {
                let destination_path = scope.root.join(destination.trim_start_matches('/'));
                if destination_path.exists() {
                    conflicts.push(format!(
                        "{}: destination already exists: {destination}",
                        member.entry_id
                    ));
                }
            }
        }
        let world_path = match mode.as_str() {
            "retain-in-place" => {
                if external {
                    canonical
                        .join(&member.entry_id)
                        .to_string_lossy()
                        .to_string()
                } else {
                    format!("{}/{}", world_relative(&scope, &canonical), member.entry_id)
                }
            }
            _ => destination.clone().unwrap_or_default(),
        };
        let source_ref = if mode == "retain-in-place" && external {
            source_horizon::source_ref(
                scope.world.as_str(),
                &canonical.join(&member.entry_id).to_string_lossy(),
            )
        } else {
            source_horizon::source_ref(scope.world.as_str(), &world_path)
        };
        if matches!(action, "copy-register" | "update" | "register") {
            bump(&mut counts, action);
        } else if member.disposition == "excluded-by-selection" {
            bump(&mut counts, "excluded-by-selection");
        } else {
            bump(&mut counts, "unchanged");
        }
        entries.push(PlanEntry {
            action: action.to_owned(),
            entry_id: member.entry_id.clone(),
            role: member.role.clone(),
            entry_type: member.entry_type.clone(),
            disposition: member.disposition.clone(),
            bytes: member.bytes,
            content_revision: member.content_revision.clone(),
            origin: Some(
                canonical
                    .join(&member.entry_id)
                    .to_string_lossy()
                    .to_string(),
            ),
            destination,
            source_ref,
            prior_revision: previous
                .filter(|_| action == "update")
                .map(|prior| prior.content_revision.clone()),
            event_date: member.event_date.clone(),
            date_basis: member.date_basis.clone(),
            date_approximate: member.date_approximate,
            readable: member.readable,
            reason: member.reason.clone(),
            entry_meta: member.entry_meta.clone(),
        });
    }

    for entry_id in by_id.keys() {
        if !members.iter().any(|member| &member.entry_id == entry_id) {
            origin_absent.push(entry_id.clone());
        }
    }

    let undo_summary = format!(
        "{} new, {} update(s), {} unchanged, {} excluded; {} origin-absent member(s) reported, \
         never deleted; rollback removes this import's copies/updates and preserves later edits",
        counts.get("copy-register").unwrap_or(&0),
        counts.get("update").unwrap_or(&0),
        counts.get("unchanged").unwrap_or(&0),
        counts.get("excluded-by-selection").unwrap_or(&0),
        origin_absent.len(),
    );

    let mut plan = CollectionPlan {
        schema: "central.personal-collection-plan/v1".to_owned(),
        collection_id,
        title: input.title.clone().unwrap_or_else(|| {
            existing
                .as_ref()
                .map(|record| record.title.clone())
                .unwrap_or_else(|| {
                    canonical
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| "collection".to_owned())
                })
        }),
        world_ref: scope.world.clone(),
        project: input.project.clone(),
        person_ref: person_ref.clone(),
        author_ref,
        adapter,
        placement: Placement {
            mode,
            home,
            origin: Some(format!("{}", canonical.display())),
            accepted_plan_revision: None,
        },
        entries,
        origin_absent,
        conflicts,
        divergences,
        counts,
        plan_revision: String::new(),
        undo_summary,
    };
    plan.plan_revision = plan_identity(&plan)?;
    Ok(plan)
}

// ---------------------------------------------------------------------------
// Apply journal and per-import restore points (recovery state under .central)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JournalStep {
    pub entry_id: String,
    pub kind: String,
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApplyJournal {
    pub schema: String,
    pub collection_id: String,
    pub plan_revision: String,
    pub sequence: u64,
    pub started_at_unix_seconds: u64,
    pub steps: Vec<JournalStep>,
}

fn journal_area(root: &Path, collection_id: &str) -> PathBuf {
    root.join(".central/personal-history").join(collection_id)
}

impl ApplyJournal {
    fn path(root: &Path, collection_id: &str) -> PathBuf {
        journal_area(root, collection_id).join("journal.json")
    }
    fn load(root: &Path, collection_id: &str) -> io::Result<Option<ApplyJournal>> {
        let path = Self::path(root, collection_id);
        if !path.is_file() {
            return Ok(None);
        }
        Ok(Some(serde_json::from_slice(&fs::read(&path)?).map_err(
            |error| io::Error::new(io::ErrorKind::InvalidData, error),
        )?))
    }
    fn persist(&self, root: &Path) -> io::Result<()> {
        let area = journal_area(root, &self.collection_id);
        fs::create_dir_all(&area)?;
        write_atomic(
            &Self::path(root, &self.collection_id),
            &serde_json::to_vec_pretty(self).map_err(io::Error::other)?,
        )
    }
    /// After a successful apply the journal becomes the import's durable
    /// receipt-of-work: restore points for updated members stay until that
    /// import is rolled back or explicitly pruned.
    fn complete(self, root: &Path) -> io::Result<PathBuf> {
        let path = journal_area(root, &self.collection_id)
            .join(format!("import-{}.receipt.json", self.sequence));
        write_atomic(
            &path,
            &serde_json::to_vec_pretty(&self).map_err(io::Error::other)?,
        )?;
        let live = Self::path(root, &self.collection_id);
        if live.exists() {
            fs::remove_file(&live)?;
        }
        Ok(path)
    }
    fn remove(root: &Path, collection_id: &str) -> io::Result<()> {
        let path = Self::path(root, collection_id);
        if path.exists() {
            fs::remove_file(path)?;
        }
        Ok(())
    }
}

fn alternatives_area(root: &Path, collection_id: &str, sequence: u64) -> PathBuf {
    journal_area(root, collection_id)
        .join("alternatives")
        .join(format!("import-{sequence}"))
}

fn unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

// ---------------------------------------------------------------------------
// Apply
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct ApplyOutcome {
    pub record: PersonalCollectionRecord,
    pub receipt: ImportReceipt,
    pub steps_refused: Vec<String>,
}

/// An empty relations document for a scope, matching what `Scope::save`
/// synthesises — apply may be the first writer in a fresh world.
fn empty_relations_doc(scope: &Scope) -> Value {
    if scope.world == "control:root" {
        json!({
            "schema": source_horizon::CONTROL_GROUND_RELATIONS_SCHEMA,
            "project_id": scope.world,
            "relations": [],
        })
    } else {
        json!({
            "schema": source_horizon::GROUND_RELATIONS_SCHEMA,
            "project_id": scope.world.strip_prefix("project:").unwrap_or(""),
            "relations": [],
        })
    }
}

pub fn apply_plan(
    root: &Path,
    plan: &CollectionPlan,
    acceptance: &str,
) -> Result<ApplyOutcome, String> {
    if acceptance != "human-accepted" {
        return Err(
            "personal-history apply requires acceptance:\"human-accepted\"; an agent's plan \
             alone is not authorship"
                .to_owned(),
        );
    }
    let recorded = plan_identity(plan)?;
    if recorded != plan.plan_revision {
        return Err(
            "the plan does not carry its own identity; re-run central.personal.collection.plan"
                .to_owned(),
        );
    }
    let scope = scope_for(root, plan.project.as_deref()).map_err(|e| e.to_string())?;
    // The world binding is checked, not assumed: the collection binds the
    // anchored person of the Central root, at every scope.
    let person = resolve_person(root, Some(&plan.person_ref))
        .map_err(|error| format!("person binding refused: {error}"))?;
    if person != plan.person_ref {
        return Err("plan person does not match the anchored subject".to_owned());
    }
    let existing = PersonalCollectionRecord::load_scoped(root, &scope.world, &plan.collection_id)
        .map_err(|e| e.to_string())?;
    let next_sequence = existing
        .as_ref()
        .map(|record| record.imports.iter().map(|i| i.sequence).max().unwrap_or(0) + 1)
        .unwrap_or(1);
    if existing
        .as_ref()
        .map(|record| {
            record
                .imports
                .iter()
                .any(|import| import.sequence == next_sequence)
        })
        .unwrap_or(false)
    {
        return Err(format!(
            "import {} of {} is already recorded as applied; read its status and receipts",
            next_sequence, plan.collection_id
        ));
    }
    if !plan.conflicts.is_empty() {
        return Err(format!(
            "plan carries {} unresolved conflict(s); re-plan after resolving them",
            plan.conflicts.len()
        ));
    }
    let has_work = plan.entries.iter().any(|entry| {
        matches!(
            entry.action.as_str(),
            "copy-register" | "update" | "register"
        )
    });
    if !has_work && existing.is_some() {
        return Err(
            "every member is already present at the same revision; nothing to apply".to_owned(),
        );
    }

    // One owner lock for the whole apply; the journal makes it resumable.
    let _file_map_lock =
        source_safety::lock(scope.root.as_path(), "file-map.lock").map_err(|e| e.to_string())?;
    let _source_lock = source_safety::lock(scope.root.as_path(), "source-mutation.lock")
        .map_err(|e| e.to_string())?;

    let mut journal = ApplyJournal {
        schema: "central.personal-collection-journal/v1".to_owned(),
        collection_id: plan.collection_id.clone(),
        plan_revision: plan.plan_revision.clone(),
        sequence: next_sequence,
        started_at_unix_seconds: unix_seconds(),
        steps: Vec::new(),
    };
    if let Some(resumed) =
        ApplyJournal::load(scope.root.as_path(), &plan.collection_id).map_err(|e| e.to_string())?
    {
        if resumed.plan_revision == plan.plan_revision && resumed.sequence == next_sequence {
            journal = resumed;
        }
    }

    let mut record = match existing {
        Some(record) => record,
        None => PersonalCollectionRecord {
            schema: PERSONAL_COLLECTION_SCHEMA.to_owned(),
            collection_id: plan.collection_id.clone(),
            title: plan.title.clone(),
            world_ref: scope.world.clone(),
            person_ref: plan.person_ref.clone(),
            author_ref: plan.author_ref.clone(),
            retained_by: "ctrl central.personal.collection.apply".to_owned(),
            placement: Placement {
                mode: plan.placement.mode.clone(),
                home: plan.placement.home.clone(),
                origin: plan.placement.origin.clone(),
                accepted_plan_revision: Some(plan.plan_revision.clone()),
            },
            entries: Vec::new(),
            imports: Vec::new(),
        },
    };

    let mut refused: Vec<String> = Vec::new();
    let mut ground = scope.ground().map_err(|e| e.to_string())?;
    let mut doc = scope.document().map_err(|e| e.to_string())?;
    if doc.is_null() {
        doc = empty_relations_doc(&scope);
    }
    let sequence = next_sequence;

    for entry in &plan.entries {
        match entry.action.as_str() {
            "none" => continue,
            "copy-register" | "update" | "register" => {}
            _ => continue,
        }
        // A resumed run replays idempotently: bytes for steps the journal
        // marks applied are already durable, so only registration and record
        // state are recomputed. A journal step whose bytes did not survive is
        // replayed, never trusted.
        let mut already_applied = journal
            .steps
            .iter()
            .any(|step| step.entry_id == entry.entry_id && step.status == "applied");
        if already_applied {
            let check = entry
                .destination
                .as_ref()
                .map(|destination| scope.root.join(destination.trim_start_matches('/')))
                .or_else(|| entry.origin.as_ref().map(PathBuf::from));
            let holds_planned_bytes = check
                .map(|path| {
                    content_revision(&path)
                        .map(|revision| revision.revision == entry.content_revision)
                        .unwrap_or(false)
                })
                .unwrap_or(true);
            if !holds_planned_bytes {
                journal
                    .steps
                    .retain(|step| !(step.entry_id == entry.entry_id && step.status == "applied"));
                already_applied = false;
            }
        }
        if !already_applied {
            // Recheck the origin basis before every write: an external edit
            // during processing is a change of identity, not an overwrite
            // opportunity.
            let origin_path = Path::new(entry.origin.as_deref().unwrap_or_default());
            let bytes = match fs::read(origin_path) {
                Ok(bytes) => bytes,
                Err(error) => {
                    refused.push(format!("{}: origin unreadable: {error}", entry.entry_id));
                    journal.steps.push(JournalStep {
                        entry_id: entry.entry_id.clone(),
                        kind: "place".to_owned(),
                        status: "failed".to_owned(),
                        detail: Some("origin unreadable".to_owned()),
                    });
                    continue;
                }
            };
            let current = fnv(&bytes);
            if current.revision != entry.content_revision {
                refused.push(format!(
                    "{}: origin changed since the plan ({} -> {}); re-plan to include it",
                    entry.entry_id, entry.content_revision, current.revision
                ));
                continue;
            }
            let mut step_ok = true;
            match entry.action.as_str() {
                "copy-register" | "update" => {
                    let destination = match entry.destination.clone() {
                        Some(destination) => destination,
                        None => {
                            refused.push(format!("{}: no destination for a copy", entry.entry_id));
                            step_ok = false;
                            String::new()
                        }
                    };
                    let destination_path = scope.root.join(destination.trim_start_matches('/'));
                    if step_ok && entry.action == "update" {
                        // Update law: the retained copy must still match the
                        // recorded revision; a moved base is re-surfaced,
                        // never overwritten. The previous bytes become a
                        // restore point before anything moves.
                        let retained_current = content_revision(&destination_path).ok();
                        let matches_basis = retained_current
                            .as_ref()
                            .map(|revision| {
                                Some(&revision.revision) == entry.prior_revision.as_ref()
                            })
                            .unwrap_or(false);
                        if !matches_basis {
                            refused.push(format!(
                                "{}: retained copy moved since the plan; re-plan to include it",
                                entry.entry_id
                            ));
                            step_ok = false;
                        } else if let Some(prior_revision) = &entry.prior_revision {
                            match fs::read(&destination_path) {
                                Ok(prior_bytes) => {
                                    if fnv(&prior_bytes).revision != *prior_revision {
                                        refused.push(format!(
                                            "{}: restore basis moved; update withheld",
                                            entry.entry_id
                                        ));
                                        step_ok = false;
                                    } else {
                                        let restore =
                                            alternatives_area(root, &plan.collection_id, sequence)
                                                .join(
                                                    entry.entry_id.to_string().replace('/', "__"),
                                                );
                                        fs::create_dir_all(
                                            restore.parent().unwrap_or(Path::new(".")),
                                        )
                                        .map_err(|e| e.to_string())?;
                                        if let Err(error) = write_atomic(&restore, &prior_bytes) {
                                            refused.push(format!(
                                                "{}: restore point failed, update withheld: {error}",
                                                entry.entry_id
                                            ));
                                            step_ok = false;
                                        }
                                    }
                                }
                                Err(error) => {
                                    refused.push(format!(
                                        "{}: retained copy unreadable: {error}",
                                        entry.entry_id
                                    ));
                                    step_ok = false;
                                }
                            }
                        }
                    }
                    if step_ok {
                        // Copy only when the destination does not already
                        // hold exactly these bytes: an interrupted earlier
                        // attempt is absorbed, a divergent destination
                        // refuses, an update overwrites its verified basis.
                        let existing = content_revision(&destination_path)
                            .ok()
                            .map(|revision| revision.revision);
                        let needs_copy =
                            existing.as_deref() != Some(entry.content_revision.as_str());
                        if needs_copy
                            && entry.action == "copy-register"
                            && destination_path.exists()
                        {
                            refused.push(format!(
                                "{}: destination exists with different bytes: {destination}",
                                entry.entry_id
                            ));
                            step_ok = false;
                        } else if needs_copy {
                            if let Some(parent) = destination_path.parent() {
                                if let Err(error) = fs::create_dir_all(parent) {
                                    refused.push(format!(
                                        "{}: destination directory failed: {error}",
                                        entry.entry_id
                                    ));
                                    step_ok = false;
                                }
                            }
                            let temp = destination_path.with_file_name(format!(
                                ".{}.import-tmp",
                                destination_path
                                    .file_name()
                                    .map(|n| n.to_string_lossy().to_string())
                                    .unwrap_or_default()
                            ));
                            if let Err(error) = fs::write(&temp, &bytes)
                                .and_then(|_| fs::rename(&temp, &destination_path))
                            {
                                let _ = fs::remove_file(&temp);
                                refused.push(format!("{}: copy failed: {error}", entry.entry_id));
                                step_ok = false;
                            }
                        }
                    }
                }
                "register" => {
                    if let Some(destination) = &entry.destination {
                        if scope
                            .root
                            .join(destination.trim_start_matches('/'))
                            .exists()
                        {
                            refused.push(format!(
                                "{}: destination appeared during apply: {destination}",
                                entry.entry_id
                            ));
                            step_ok = false;
                        }
                    }
                }
                _ => {}
            }
            if !step_ok {
                journal.steps.push(JournalStep {
                    entry_id: entry.entry_id.clone(),
                    kind: "place".to_owned(),
                    status: "failed".to_owned(),
                    detail: None,
                });
                continue;
            }
            journal.steps.push(JournalStep {
                entry_id: entry.entry_id.clone(),
                kind: "place".to_owned(),
                status: "applied".to_owned(),
                detail: None,
            });
            // Persist progress as it happens so an interrupted apply resumes
            // instead of duplicating.
            if let Err(error) = journal.persist(scope.root.as_path()) {
                refused.push(format!(
                    "{}: placed but the journal could not be written: {error}",
                    entry.entry_id
                ));
            }
        }

        // Register the retained source in this scope's file map.
        let external_registration = plan.placement.mode == "retain-in-place"
            && entry
                .origin
                .as_deref()
                .map(|p| Path::new(p).is_absolute())
                .unwrap_or(false);
        let retained_path = entry
            .destination
            .clone()
            .unwrap_or_else(|| entry.origin.clone().unwrap_or_default());
        let resource_path = if external_registration {
            retained_path.clone()
        } else {
            retained_path.trim_start_matches('/').to_owned()
        };
        if !ground.resources.contains_key(&entry.source_ref) {
            ground.resources.insert(
                entry.source_ref.clone(),
                Resource {
                    path: resource_path.clone(),
                    external: external_registration,
                    native_import: false,
                    title: format!("{} — {}", plan.title, entry.entry_id),
                    tags: vec![
                        "personal-history".to_owned(),
                        format!("collection:{}", plan.collection_id),
                    ],
                },
            );
        }
        // The relation row: human-adopted provenance, recorded through this
        // human-accepted apply.
        upsert_relation(&mut doc, &entry.source_ref, &resource_path, entry);

        // Record state, deduplicated by entry_id so a resumed run rebuilds
        // rather than duplicating.
        let retained_world_path = retained_path.clone();
        let new_entry = CollectionEntry {
            entry_id: entry.entry_id.clone(),
            role: entry.role.clone(),
            path: retained_world_path,
            source_ref: entry.source_ref.clone(),
            entry_type: entry.entry_type.clone(),
            event_date: entry.event_date.clone(),
            date_basis: entry.date_basis.clone(),
            date_approximate: entry.date_approximate,
            content_revision: entry.content_revision.clone(),
            bytes: entry.bytes,
            disposition: entry.disposition.clone(),
            readable: entry.readable,
            reason: entry.reason.clone(),
            entry_meta: entry.entry_meta.clone(),
            alternatives: Vec::new(),
            first_import: sequence,
        };
        match record
            .entries
            .iter()
            .position(|recorded| recorded.entry_id == entry.entry_id)
        {
            Some(index) => {
                let mut updated = new_entry;
                updated.first_import = record.entries[index].first_import;
                if let Some(prior_revision) = &entry.prior_revision {
                    updated.alternatives = record.entries[index].alternatives.clone();
                    updated.alternatives.push(AlternativeRevision {
                        content_revision: prior_revision.clone(),
                        replaced_by_import: sequence,
                    });
                }
                record.entries[index] = updated;
            }
            None => record.entries.push(new_entry),
        }
    }

    if record.entries.is_empty() && !plan.entries.is_empty() {
        return Err(format!(
            "no member could be applied; first refusal: {}",
            refused.first().map(String::as_str).unwrap_or("unknown")
        ));
    }

    // The collection record itself, registered like any source.
    let record_path = PersonalCollectionRecord::record_path_for(&scope.world, &plan.collection_id);
    record
        .persist(scope.root.as_path())
        .map_err(|e| e.to_string())?;
    let record_ref = source_horizon::source_ref(scope.world.as_str(), &record_path);
    if !ground.resources.contains_key(&record_ref) {
        ground.resources.insert(
            record_ref.clone(),
            Resource {
                path: record_path.trim_start_matches('/').to_owned(),
                external: false,
                native_import: false,
                title: format!("{} — collection record", plan.title),
                tags: vec![
                    ROLE_COLLECTION.to_owned(),
                    format!("collection:{}", plan.collection_id),
                ],
            },
        );
        upsert_record_relation(&mut doc, &record_ref, &record_path);
    }

    scope.save(&ground, doc).map_err(|e| e.to_string())?;

    // Receipt counts come from the journal's applied steps, so a resumed run
    // reports exactly what landed once.
    let applied_ids: std::collections::BTreeSet<String> = journal
        .steps
        .iter()
        .filter(|step| step.status == "applied")
        .map(|step| step.entry_id.clone())
        .collect();
    let receipt = ImportReceipt {
        sequence,
        applied_at_unix_seconds: unix_seconds(),
        adapter: plan.adapter.clone(),
        entries_added: plan
            .entries
            .iter()
            .filter(|entry| {
                applied_ids.contains(&entry.entry_id) && entry.action == "copy-register"
            })
            .count() as u64,
        entries_changed: plan
            .entries
            .iter()
            .filter(|entry| applied_ids.contains(&entry.entry_id) && entry.action == "update")
            .count() as u64,
        entries_unchanged: plan
            .entries
            .iter()
            .filter(|entry| entry.action == "none")
            .count() as u64,
        entries_unchanged_at_origin_absent: plan.origin_absent.len() as u64,
        accepted_plan_revision: plan.plan_revision.clone(),
    };
    record.imports.push(receipt.clone());
    record
        .persist(scope.root.as_path())
        .map_err(|e| e.to_string())?;
    journal
        .complete(scope.root.as_path())
        .map_err(|e| e.to_string())?;
    // Index what arrived so search is useful immediately.
    let _ = file_map_index::refresh(&scope, false);
    Ok(ApplyOutcome {
        record,
        receipt,
        steps_refused: refused,
    })
}

fn upsert_relation(doc: &mut Value, source_ref: &str, resource_path: &str, entry: &PlanEntry) {
    // Relations carry world-relative paths without a leading slash, exactly
    // like every other ground relation; external retained origins keep their
    // absolute path.
    let relation_path = resource_path
        .trim_start_matches('/')
        .to_owned();
    let relations = relations_array(doc);
    if let Some(existing) = relations
        .iter_mut()
        .find(|relation| relation["ref"] == json!(source_ref))
    {
        existing["roles"] = json!(roles_for(entry));
        return;
    }
    relations.push(json!({
        "ref": source_ref,
        "path": relation_path,
        "provenance": "human-adopted",
        "standing": "authored-human-position",
        "roles": roles_for(entry),
        "treatment": "retain-native-in-place",
        "recognition": "human-accepted personal-history intake",
        "recorded_at_unix_seconds": unix_seconds(),
    }));
    relations.sort_by(|a, b| {
        a["path"]
            .as_str()
            .unwrap_or_default()
            .cmp(b["path"].as_str().unwrap_or_default())
    });
}

fn roles_for(entry: &PlanEntry) -> Vec<String> {
    let mut roles = vec![ROLE_ENTRY.to_owned()];
    if entry.role == "attachment" {
        roles.push("attachment".to_owned());
    }
    if entry.disposition == "unreadable" {
        roles.push("unreadable-member".to_owned());
    }
    roles
}

fn upsert_record_relation(doc: &mut Value, source_ref: &str, path: &str) {
    let relations = relations_array(doc);
    if relations
        .iter()
        .any(|relation| relation["ref"] == json!(source_ref))
    {
        return;
    }
    relations.push(json!({
        "ref": source_ref,
        "path": path,
        "provenance": "generated-derived",
        "standing": "implementation-fact",
        "roles": [ROLE_RECORD],
        "treatment": "retain-native-in-place",
        "recognition": "human-accepted personal-history intake",
        "recorded_at_unix_seconds": unix_seconds(),
    }));
    relations.sort_by(|a, b| {
        a["path"]
            .as_str()
            .unwrap_or_default()
            .cmp(b["path"].as_str().unwrap_or_default())
    });
}

fn relations_array(doc: &mut Value) -> &mut Vec<Value> {
    if !doc["relations"].is_array() {
        doc["relations"] = json!([]);
    }
    doc["relations"].as_array_mut().expect("just ensured array")
}

// ---------------------------------------------------------------------------
// Verify / status / rollback / listing
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EntryVerification {
    pub entry_id: String,
    pub state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VerificationReading {
    pub collection_id: String,
    pub entries_total: usize,
    pub verified: usize,
    pub problems: Vec<EntryVerification>,
    pub record_revision: String,
}

pub fn verify_collection(
    root: &Path,
    project: Option<&str>,
    collection_id: &str,
) -> Result<VerificationReading, String> {
    let scope = scope_for(root, project).map_err(|e| e.to_string())?;
    let record = PersonalCollectionRecord::load_scoped(&scope.root, &scope.world, collection_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("no collection record for {collection_id}"))?;
    let record_revision = record
        .record_revision(&scope.root)
        .map_err(|e| e.to_string())?;
    let mut problems = Vec::new();
    let mut verified = 0usize;
    for entry in &record.entries {
        if entry.disposition == "excluded-by-selection" {
            verified += 1;
            continue;
        }
        let check_path = if entry.path.starts_with('/') {
            PathBuf::from(&entry.path)
        } else {
            scope.root.join(entry.path.trim_start_matches('/'))
        };
        if !check_path.exists() {
            problems.push(EntryVerification {
                entry_id: entry.entry_id.clone(),
                state: "missing".to_owned(),
                detail: Some(entry.path.clone()),
            });
            continue;
        }
        match content_revision(&check_path) {
            Ok(current) if current.revision == entry.content_revision => verified += 1,
            Ok(current) => problems.push(EntryVerification {
                entry_id: entry.entry_id.clone(),
                state: "changed".to_owned(),
                detail: Some(format!(
                    "recorded {} current {} — a later edit preserved, not an error",
                    entry.content_revision, current.revision
                )),
            }),
            Err(error) => problems.push(EntryVerification {
                entry_id: entry.entry_id.clone(),
                state: "unreadable".to_owned(),
                detail: Some(error.to_string()),
            }),
        }
    }
    Ok(VerificationReading {
        collection_id: collection_id.to_owned(),
        entries_total: record.entries.len(),
        verified,
        problems,
        record_revision,
    })
}

pub fn collection_status(
    root: &Path,
    project: Option<&str>,
    collection_id: &str,
) -> Result<Value, String> {
    let scope = scope_for(root, project).map_err(|e| e.to_string())?;
    let record = PersonalCollectionRecord::load_scoped(&scope.root, &scope.world, collection_id)
        .map_err(|e| e.to_string())?;
    let journal = ApplyJournal::load(&scope.root, collection_id).map_err(|e| e.to_string())?;
    let mut prior_imports = Vec::new();
    if let Ok(area) = fs::read_dir(journal_area(root, collection_id)) {
        for entry in area.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with("import-") && name.ends_with(".receipt.json") {
                prior_imports.push(name);
            }
        }
    }
    prior_imports.sort();
    Ok(json!({
        "record": record,
        "resumable_apply": journal,
        "import_work_receipts": prior_imports,
    }))
}

pub fn list_collections(root: &Path) -> io::Result<Vec<Value>> {
    let mut found = Vec::new();
    let mut scan = |base: PathBuf, world_ref: &str, project: Option<&str>| -> io::Result<()> {
        if !base.is_dir() {
            return Ok(());
        }
        let mut entries: Vec<_> = fs::read_dir(&base)?.collect::<Result<Vec<_>, _>>()?;
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let path = entry.path().join("collection.json");
            if !path.is_file() {
                continue;
            }
            if let Ok(bytes) = fs::read(&path) {
                if let Ok(value) = serde_json::from_slice::<Value>(&bytes) {
                    found.push(json!({
                        "collection_id": value["collection_id"],
                        "title": value["title"],
                        "world_ref": world_ref,
                        "project": project,
                        "person_ref": value["person_ref"],
                        "author_ref": value["author_ref"],
                        "mode": value["placement"]["mode"],
                        "origin": value["placement"]["origin"],
                        "entries": value["entries"].as_array().map(|a| a.len()).unwrap_or(0),
                        "imports": value["imports"].as_array().map(|a| a.len()).unwrap_or(0),
                    }));
                }
            }
        }
        Ok(())
    };
    scan(root.join(COLLECTIONS_DIR), "control:root", None)?;
    let work = root.join("Work");
    if work.is_dir() {
        let mut projects: Vec<_> = fs::read_dir(&work)?.collect::<Result<Vec<_>, _>>()?;
        projects.sort_by_key(|entry| entry.file_name());
        for project in projects {
            let name = project.file_name().to_string_lossy().to_string();
            let base = project.path().join("ProjectCentral/user/collections");
            scan(base, &format!("project:{name}"), Some(name.as_str()))?;
        }
    }
    Ok(found)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RollbackReport {
    pub collection_id: String,
    pub import_sequence: u64,
    pub removed_entries: Vec<String>,
    pub restored_entries: Vec<String>,
    pub preserved_entries: Vec<String>,
    pub removed_registrations: Vec<String>,
    pub record_removed: bool,
    pub notes: Vec<String>,
}

/// Undo one import's owned effects. Only effects whose current bases still
/// match the import are undone; a member edited after import is preserved and
/// reported — later human writing is not reverted by a rollback. Updated
/// members are restored from their recorded restore point when that point
/// still matches; otherwise they are preserved and reported.
pub fn rollback_import(
    root: &Path,
    project: Option<&str>,
    collection_id: &str,
    import_sequence: u64,
    expected_record_revision: &str,
) -> Result<RollbackReport, String> {
    let scope = scope_for(root, project).map_err(|e| e.to_string())?;
    let record_path = scope.root.join(PersonalCollectionRecord::record_path_for(
        &scope.world,
        collection_id,
    ));
    if !record_path.is_file() {
        return Err(format!("no collection record for {collection_id}"));
    }
    let current = content_revision(&record_path)
        .map_err(|e| e.to_string())?
        .revision;
    if current != expected_record_revision {
        return Err(format!(
            "record moved since it was read ({} -> {current}); reload status and retry",
            expected_record_revision
        ));
    }
    let mut record =
        PersonalCollectionRecord::load_scoped(&scope.root, &scope.world, collection_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("no collection record for {collection_id}"))?;
    if !record
        .imports
        .iter()
        .any(|import| import.sequence == import_sequence)
    {
        return Err(format!(
            "import sequence {import_sequence} is not part of collection {collection_id}"
        ));
    }
    let scope = scope_for(root, project).map_err(|e| e.to_string())?;
    let _file_map_lock =
        source_safety::lock(scope.root.as_path(), "file-map.lock").map_err(|e| e.to_string())?;
    let _source_lock = source_safety::lock(scope.root.as_path(), "source-mutation.lock")
        .map_err(|e| e.to_string())?;

    let mode = record.placement.mode.clone();
    let mut removed_entries = Vec::new();
    let mut restored_entries = Vec::new();
    let mut preserved = Vec::new();
    let mut removed_registrations = Vec::new();
    let mut notes = Vec::new();

    let owned: Vec<CollectionEntry> = record
        .entries
        .iter()
        .filter(|entry| entry.first_import == import_sequence)
        .cloned()
        .collect();
    let updated_here: Vec<String> = record
        .entries
        .iter()
        .filter(|entry| {
            entry
                .alternatives
                .iter()
                .any(|alt| alt.replaced_by_import == import_sequence)
        })
        .map(|entry| entry.entry_id.clone())
        .collect();

    // Remove copies this import added (copy/registered modes).
    for entry in &owned {
        let check_path = if entry.path.starts_with('/') {
            PathBuf::from(&entry.path)
        } else {
            scope.root.join(entry.path.trim_start_matches('/'))
        };
        if !check_path.exists() {
            removed_entries.push(entry.entry_id.clone());
        } else {
            match content_revision(&check_path) {
                Ok(at) if at.revision == entry.content_revision => {
                    if mode != "retain-in-place" && !entry.path.starts_with('/') {
                        if let Err(error) = fs::remove_file(&check_path) {
                            notes.push(format!("{}: copy removal failed: {error}", entry.entry_id));
                            continue;
                        }
                    }
                    removed_entries.push(entry.entry_id.clone());
                }
                Ok(_) => {
                    // Later edits win: the file stays; knowledge made from it
                    // needs reconciliation, which is reported, not assumed.
                    preserved.push(entry.entry_id.clone());
                    notes.push(format!(
                        "{}: changed after import; retained with its later edits",
                        entry.entry_id
                    ));
                }
                Err(error) => {
                    preserved.push(entry.entry_id.clone());
                    notes.push(format!(
                        "{}: could not recheck before removal: {error}",
                        entry.entry_id
                    ));
                }
            }
        }
        removed_registrations.push(entry.source_ref.clone());
    }

    // Members this import did not own but which no longer match the record
    // are drifted later edits: untouched, disclosed for reconciliation.
    for entry in record.entries.iter() {
        if entry.first_import == import_sequence || updated_here.contains(&entry.entry_id) {
            continue;
        }
        let check = if entry.path.starts_with('/') {
            PathBuf::from(&entry.path)
        } else {
            root.join(entry.path.trim_start_matches('/'))
        };
        if let Ok(current) = content_revision(&check) {
            if current.revision != entry.content_revision {
                preserved.push(entry.entry_id.clone());
                notes.push(format!(
                    "{}: edited after import (not owned by this rollback); \
                     knowledge made from it may need reconciliation",
                    entry.entry_id
                ));
            }
        }
    }

    // Restore members this import updated, from their restore points.
    let restore_area = alternatives_area(root, collection_id, import_sequence);
    for entry_id in &updated_here {
        let index = record
            .entries
            .iter()
            .position(|entry| &entry.entry_id == entry_id)
            .expect("updated_here comes from record entries");
        let entry = record.entries[index].clone();
        let Some(alternative) = entry
            .alternatives
            .iter()
            .rev()
            .find(|alt| alt.replaced_by_import == import_sequence)
            .cloned()
        else {
            continue;
        };
        let live = if entry.path.starts_with('/') {
            PathBuf::from(&entry.path)
        } else {
            scope.root.join(entry.path.trim_start_matches('/'))
        };
        let restore_point = restore_area.join(entry.entry_id.to_string().replace('/', "__"));
        match (content_revision(&live), fs::read(&restore_point)) {
            (Ok(live_revision), Ok(prior_bytes)) => {
                if live_revision.revision == entry.content_revision {
                    let prior_revision = fnv(&prior_bytes).revision;
                    if prior_revision == alternative.content_revision {
                        if write_atomic(&live, &prior_bytes).is_ok() {
                            restored_entries.push(entry_id.clone());
                            let mut restored_entry = entry.clone();
                            restored_entry.content_revision = prior_revision;
                            restored_entry.bytes = prior_bytes.len() as u64;
                            restored_entry
                                .alternatives
                                .retain(|alt| alt.replaced_by_import != import_sequence);
                            record.entries[index] = restored_entry;
                        } else {
                            preserved.push(entry_id.clone());
                            notes.push(format!("{entry_id}: restore write failed; preserved"));
                        }
                    } else {
                        preserved.push(entry_id.clone());
                        notes.push(format!("{entry_id}: restore point basis moved; preserved"));
                    }
                } else {
                    preserved.push(entry_id.clone());
                    notes.push(format!(
                        "{entry_id}: edited after the update; preserved with its later edits"
                    ));
                }
            }
            (Err(error), _) | (_, Err(error)) => {
                preserved.push(entry_id.clone());
                notes.push(format!("{entry_id}: restore recheck failed: {error}"));
            }
        }
        removed_registrations.push(entry.source_ref.clone());
    }

    // Registration + relation withdrawal happens in one ground save.
    let mut ground = scope.ground().map_err(|e| e.to_string())?;
    let mut doc = scope.document().map_err(|e| e.to_string())?;
    if doc.is_null() {
        doc = empty_relations_doc(&scope);
    }
    let record_ref = source_horizon::source_ref(
        scope.world.as_str(),
        &PersonalCollectionRecord::record_path(collection_id),
    );
    let mut withdrawals = removed_registrations.clone();
    let last_sequence = record
        .imports
        .iter()
        .map(|import| import.sequence)
        .max()
        .unwrap_or(0);
    let removing_record = import_sequence == last_sequence;
    if removing_record {
        withdrawals.push(record_ref.clone());
    }
    for source_ref in &withdrawals {
        ground.resources.remove(source_ref);
    }
    if let Some(relations) = doc["relations"].as_array_mut() {
        relations.retain(|relation| {
            let reference = relation["ref"].as_str().unwrap_or_default();
            !withdrawals.contains(&reference.to_owned())
        });
    }
    scope.save(&ground, doc).map_err(|e| e.to_string())?;

    // Entries still owned by other imports stay; an emptied record goes.
    record
        .entries
        .retain(|entry| entry.first_import != import_sequence);
    record
        .imports
        .retain(|import| import.sequence != import_sequence);
    let record_removed = record.imports.is_empty() && record.entries.is_empty();
    if record_removed {
        fs::remove_file(&record_path).map_err(|e| e.to_string())?;
        if removing_record {
            let parent = record_path.parent().unwrap_or(Path::new("."));
            let _ = fs::remove_dir(parent);
        }
    } else {
        record.persist(&scope.root).map_err(|e| e.to_string())?;
    }
    let _ = fs::remove_dir_all(&restore_area);
    let _ = ApplyJournal::remove(&scope.root, collection_id);
    let _ = file_map_index::refresh(&scope, false);
    Ok(RollbackReport {
        collection_id: collection_id.to_owned(),
        import_sequence,
        removed_entries,
        restored_entries,
        preserved_entries: preserved,
        removed_registrations: withdrawals,
        record_removed,
        notes,
    })
}

// ---------------------------------------------------------------------------
// Anchor readback
// ---------------------------------------------------------------------------

pub fn anchor_inspect(root: &Path) -> Value {
    let state = PasuIdentityState::read(root, None);
    let declared = {
        let path = root.join("Control/relations/source-relations.json");
        fs::read_to_string(path)
            .ok()
            .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
            .and_then(|doc| {
                doc["subject_ref"]
                    .as_str()
                    .map(|subject| subject.to_owned())
            })
    };
    let subject_consistent = match (&declared, &state.subject_ref) {
        (Some(declared), Some(subject)) => Some(declared == subject),
        _ => None,
    };
    let workcell = fs::read_to_string(root.join("Control/machines/current.json"))
        .ok()
        .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
        .map(|doc| {
            json!({
                "workcell_ref": doc.get("workcell_ref").cloned().unwrap_or(Value::Null),
                "machine_ref": doc.get("machine_ref").cloned().unwrap_or(Value::Null),
            })
        });
    let collections = list_collections(root).unwrap_or_default();
    json!({
        "schema": "central.personal-anchor/v1",
        "person": {
            "subject_ref": state.subject_ref,
            "form": state.form,
            "manifest_revision": state.manifest_revision,
            "manifest_present": state.present,
            "identity_source_path": state.identity_source_path,
            "sourced_files": state.sourced_files,
            "error": state.error,
        },
        "world": {
            "subject_ref_declared": declared,
            "subject_ref_consistent": subject_consistent,
        },
        "installation": workcell,
        "collections": collections,
    })
}

// ---------------------------------------------------------------------------
// Action surface
// ---------------------------------------------------------------------------

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

fn text_input(name: &str, required: bool) -> ActionInputDefinition {
    ActionInputDefinition {
        name: name.to_owned(),
        input_type: "text".to_owned(),
        required,
        choices: None,
        selection: None,
    }
}

fn array_input(name: &str) -> ActionInputDefinition {
    ActionInputDefinition {
        name: name.to_owned(),
        input_type: "array".to_owned(),
        required: false,
        choices: None,
        selection: None,
    }
}

fn object_input(name: &str, required: bool) -> ActionInputDefinition {
    ActionInputDefinition {
        name: name.to_owned(),
        input_type: "object".to_owned(),
        required,
        choices: None,
        selection: None,
    }
}

fn required_text(input: &Value, field: &str, action: &str) -> Result<String, ActionResult> {
    input[field]
        .as_str()
        .filter(|s| !s.is_empty())
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
    input[field]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

fn invalid_input(action: &str, message: impl std::fmt::Display) -> ActionResult {
    ActionResult::failure(
        Some(action),
        ResultStatus::InvalidInput,
        message.to_string(),
        None,
    )
}

fn root_of(context: &ActionExecutionContext<'_>) -> Result<PathBuf, ActionResult> {
    resolve_central_root(context.root_options)
        .map(|resolved| resolved.path)
        .map_err(|message| ActionResult::failure(None, ResultStatus::InvalidInput, message, None))
}

fn plan_request_from(action: &str, input: &Value) -> Result<PlanRequest, ActionResult> {
    if input.get("path").and_then(Value::as_str).is_none() {
        return Err(invalid_input(action, "personal plan requires path."));
    }
    let exclude = input
        .get("exclude")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    Ok(PlanRequest {
        path: optional_text(input, "path"),
        project: optional_text(input, "project"),
        collection_id: optional_text(input, "collection_id"),
        title: optional_text(input, "title"),
        person_ref: optional_text(input, "person_ref"),
        author_ref: optional_text(input, "author_ref"),
        mode: optional_text(input, "mode"),
        home: optional_text(input, "home"),
        adapter: optional_text(input, "adapter"),
        exclude,
    })
}

fn anchor_action(
    _: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let action = "central.personal.anchor.inspect";
    let root = match root_of(context) {
        Ok(value) => value,
        Err(result) => return result,
    };
    let mut reading = anchor_inspect(&root);
    if let Some(project) = optional_text(input, "project") {
        reading["project"] = json!(project);
    }
    ActionResult::success(action, reading)
}

fn inspect_action(
    _: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let action = "central.personal.collection.inspect";
    let root = match root_of(context) {
        Ok(value) => value,
        Err(result) => return result,
    };
    let request = match plan_request_from(action, input) {
        Ok(value) => value,
        Err(result) => return result,
    };
    let scope = match scope_for(&root, request.project.as_deref()) {
        Ok(value) => value,
        Err(error) => return invalid_input(action, error),
    };
    let raw_path = match request.path.as_deref() {
        Some(path) => path,
        None => return invalid_input(action, "personal inspect requires path."),
    };
    let origin = resolve_origin(&scope, raw_path);
    let canonical = match origin.canonicalize() {
        Ok(value) => value,
        Err(error) => {
            return invalid_input(action, format!("collection path is unreadable: {error}"))
        }
    };
    let adapter = request
        .adapter
        .clone()
        .unwrap_or_else(|| MARKDOWN_ADAPTER.to_owned());
    let members = match read_members(&canonical, &adapter) {
        Ok(value) => value,
        Err(error) => return invalid_input(action, error),
    };
    let mut retained = 0usize;
    let mut unreadable = 0usize;
    let mut excluded = 0usize;
    for member in &members {
        match member.disposition.as_str() {
            "retained" => retained += 1,
            "unreadable" => unreadable += 1,
            "excluded-by-selection" => excluded += 1,
            _ => {}
        }
    }
    ActionResult::success(
        action,
        json!({
            "schema": "central.personal-collection-inspection/v1",
            "origin": canonical.display().to_string(),
            "world_ref": scope.world,
            "adapter": adapter,
            "members": members,
            "counts": {"retained": retained, "unreadable": unreadable, "excluded-by-selection": excluded, "total": members.len()},
        }),
    )
}

fn plan_action(
    _: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let action = "central.personal.collection.plan";
    let root = match root_of(context) {
        Ok(value) => value,
        Err(result) => return result,
    };
    let request = match plan_request_from(action, input) {
        Ok(value) => value,
        Err(result) => return result,
    };
    match build_plan(&root, &request) {
        Ok(plan) => ActionResult::success(
            action,
            serde_json::to_value(&plan).unwrap_or_else(|_| json!({})),
        ),
        Err(message) => invalid_input(action, message),
    }
}

fn apply_action(
    _: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let action = "central.personal.collection.apply";
    let root = match root_of(context) {
        Ok(value) => value,
        Err(result) => return result,
    };
    let plan_value = match input.get("plan") {
        Some(value) if value.is_object() => value.clone(),
        _ => return invalid_input(action, "personal apply requires the accepted plan object."),
    };
    let plan: CollectionPlan = match serde_json::from_value(plan_value) {
        Ok(value) => value,
        Err(error) => {
            return invalid_input(
                action,
                format!("plan is not a valid plan document: {error}"),
            )
        }
    };
    let acceptance = match input.get("acceptance").and_then(Value::as_str) {
        Some(value) => value,
        None => return invalid_input(action, "personal apply requires acceptance."),
    };
    match apply_plan(&root, &plan, acceptance) {
        Ok(outcome) => {
            let data = json!({
                "schema": "central.personal-collection-apply/v1",
                "collection_id": outcome.record.collection_id,
                "receipt": outcome.receipt,
                "record_ref": source_horizon::source_ref(
                    outcome.record.world_ref.as_str(),
                    &PersonalCollectionRecord::record_path_for(
                        &outcome.record.world_ref,
                        &outcome.record.collection_id,
                    ),
                ),
                "entries": outcome.record.entries.len(),
                "refused": outcome.steps_refused,
            });
            if outcome.steps_refused.is_empty() {
                ActionResult::success(action, data)
            } else {
                // Some members landed; the refusals are the honest remainder.
                ActionResult {
                    ok: true,
                    action: Some(action.to_owned()),
                    status: ResultStatus::PartialCompletion,
                    data: Some(data),
                    error: None,
                }
            }
        }
        Err(message) => invalid_input(action, message),
    }
}

fn status_action(
    _: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let action = "central.personal.collection.status";
    let root = match root_of(context) {
        Ok(value) => value,
        Err(result) => return result,
    };
    let collection_id = match required_text(input, "collection_id", action) {
        Ok(value) => value,
        Err(result) => return result,
    };
    match collection_status(
        &root,
        optional_text(input, "project").as_deref(),
        &collection_id,
    ) {
        Ok(value) => ActionResult::success(action, value),
        Err(message) => invalid_input(action, message),
    }
}

fn verify_action(
    _: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let action = "central.personal.collection.verify";
    let root = match root_of(context) {
        Ok(value) => value,
        Err(result) => return result,
    };
    let collection_id = match required_text(input, "collection_id", action) {
        Ok(value) => value,
        Err(result) => return result,
    };
    match verify_collection(
        &root,
        optional_text(input, "project").as_deref(),
        &collection_id,
    ) {
        Ok(reading) => ActionResult::success(
            action,
            serde_json::to_value(&reading).unwrap_or_else(|_| json!({})),
        ),
        Err(message) => invalid_input(action, message),
    }
}

fn rollback_action(
    _: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let action = "central.personal.collection.rollback";
    let root = match root_of(context) {
        Ok(value) => value,
        Err(result) => return result,
    };
    let collection_id = match required_text(input, "collection_id", action) {
        Ok(value) => value,
        Err(result) => return result,
    };
    let import_sequence = match input.get("import_sequence").and_then(Value::as_u64) {
        Some(value) => value,
        None => return invalid_input(action, "personal rollback requires import_sequence."),
    };
    let expected = match required_text(input, "expected_record_revision", action) {
        Ok(value) => value,
        Err(result) => return result,
    };
    match rollback_import(
        &root,
        optional_text(input, "project").as_deref(),
        &collection_id,
        import_sequence,
        &expected,
    ) {
        Ok(report) => ActionResult::success(
            action,
            serde_json::to_value(&report).unwrap_or_else(|_| json!({})),
        ),
        Err(message) => invalid_input(action, message),
    }
}

fn list_action(
    _: &ActionRegistry,
    _input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let action = "central.personal.collection.list";
    let root = match root_of(context) {
        Ok(value) => value,
        Err(result) => return result,
    };
    match list_collections(&root) {
        Ok(collections) => ActionResult::success(
            action,
            json!({"schema": "central.personal-collection-listing/v1", "collections": collections}),
        ),
        Err(error) => invalid_input(action, error),
    }
}

pub fn register_personal_history_actions(registry: &mut ActionRegistry) {
    let actions: Vec<(ActionDescriptor, crate::action::ActionHandler)> = vec![
        (
            descriptor(
                "central.personal.anchor.inspect",
                "Inspect the personal anchor",
                "Read the person/world/installation anchor of this world without mutating anything: the pasu identity manifest state, the ground-relations subject agreement, the machine Workcell binding and the registered personal collections. The anchor is the navigable personal node, never a generated biography. Read-only; never invokes an Agent or model.",
                MutationClass::ReadOnly,
                vec![text_input("project", false)],
                "central-personal-anchor",
            ),
            anchor_action,
        ),
        (
            descriptor(
                "central.personal.collection.inspect",
                "Inspect a personal-history collection",
                "Enumerate a candidate collection directory through an intake adapter: entries with types, event dates and their bases, dispositions (retained / unreadable / excluded-by-selection), byte sizes and content revisions. Retention is whole-file; extraction belongs to later stages. Read-only; never invokes an Agent or model.",
                MutationClass::ReadOnly,
                vec![
                    text_input("path", true),
                    text_input("project", false),
                    text_input("adapter", false),
                    array_input("exclude"),
                ],
                "central-personal-collection-inspection",
            ),
            inspect_action,
        ),
        (
            descriptor(
                "central.personal.collection.plan",
                "Plan personal-history placement",
                "Build the placement plan for bringing a collection to a chosen person: per-entry operations (new copy / update / register / unchanged), source identities, destination paths, divergences (retained copies edited after import are reported, never overwritten), origin-absent members and an undo summary. Modes: copy into Control/user/collections, retain-in-place, or register an already-world member. Read-only; nothing is written.",
                MutationClass::ReadOnly,
                vec![
                    text_input("path", true),
                    text_input("project", false),
                    text_input("collection_id", false),
                    text_input("title", false),
                    text_input("person_ref", false),
                    text_input("author_ref", false),
                    text_input("mode", false),
                    text_input("home", false),
                    text_input("adapter", false),
                    array_input("exclude"),
                ],
                "central-personal-collection-plan",
            ),
            plan_action,
        ),
        (
            descriptor(
                "central.personal.collection.apply",
                "Apply an accepted personal-history plan",
                "Execute a human-accepted placement plan: copy bytes without overwriting, register every retained member as an ordinary source through the file map, write human-adopted relations, persist the collection record, and index what arrived. Journal-based: an interrupted apply resumes instead of duplicating. Origins and retained copies that moved since the plan are refused, never overwritten.",
                MutationClass::LocallyMutating,
                vec![object_input("plan", true), text_input("acceptance", true)],
                "central-personal-collection-apply",
            ),
            apply_action,
        ),
        (
            descriptor(
                "central.personal.collection.status",
                "Read collection status",
                "Read one collection's record, any resumable apply journal and prior import work receipts (project selects a Project scope). Read-only.",
                MutationClass::ReadOnly,
                vec![text_input("collection_id", true), text_input("project", false)],
                "central-personal-collection-status",
            ),
            status_action,
        ),
        (
            descriptor(
                "central.personal.collection.verify",
                "Verify retained material",
                "Recheck every retained entry of a collection: presence, exact content revision, reachability (project selects a Project scope). Changed members are reported as preserved later edits, not errors. Read-only.",
                MutationClass::ReadOnly,
                vec![text_input("collection_id", true), text_input("project", false)],
                "central-personal-collection-verification",
            ),
            verify_action,
        ),
        (
            descriptor(
                "central.personal.collection.rollback",
                "Roll back one import",
                "Undo one import's owned effects whose bases still match the record: imported copies are withdrawn, updated members are restored from their restore points, registrations and relations are withdrawn; members edited after import are preserved and reported. Requires the exact expected record revision. Locally mutating.",
                MutationClass::LocallyMutating,
                vec![
                    text_input("collection_id", true),
                    text_input("import_sequence", true),
                    text_input("expected_record_revision", true),
                    text_input("project", false),
                ],
                "central-personal-collection-rollback",
            ),
            rollback_action,
        ),
        (
            descriptor(
                "central.personal.collection.list",
                "List personal collections",
                "List the registered personal collections of this world with their person/author bindings and placement. Read-only.",
                MutationClass::ReadOnly,
                vec![],
                "central-personal-collection-listing",
            ),
            list_action,
        ),
    ];
    for (descriptor, handler) in actions {
        registry
            .register(descriptor, handler)
            .expect("personal-history Action ids are valid");
    }
}

#[cfg(test)]
mod tests;
