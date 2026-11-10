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
    rc::Rc,
};

pub const SCHEMA: &str = "central.file-map/v1";
const INDEX: &str = ".central/bkmr/bindings.json";
const ENTRIES_CACHE: &str = ".central/bkmr/entries-cache.json";
/// Bytes of a source's text written into its bkmr description. bkmr accepts a
/// description only as a command-line argument, and an argument has a ceiling
/// that is the OS's, not ours: one argument may not exceed 128 KiB on Linux and
/// the whole argv 1 MiB on macOS. So the description cannot simply grow with the
/// file; text past this point is not searchable until sources are indexed as
/// passages. The bound is never silent: refresh names every source it cuts
/// (`truncated`) and a search hit carries `indexed_bytes` of `total_bytes`.
pub(crate) const INDEXED_TEXT_BYTES: usize = 32768;
pub(crate) const MAX_ENTRIES: usize = 1_000_000;
/// A plain folder registered as a scope (`scope-register` with `kind: "folder"`): a folder outside the
/// ground with no ProjectCentral, such as a world's Music folder. Central reads it and writes nothing
/// inside it. Its declaration is the ground's, at `<ground>/.central/scopes/<name>/scope.json`, beside
/// the scope's derived state; its World is `folder:<name>`.
pub(crate) const FOLDER_SCOPE_SCHEMA: &str = "central.file-map.folder-scope/v1";
pub(crate) const FOLDER_SCOPE_DECLARATION: &str = "scope.json";
pub(crate) const FOLDER_WORLD_PREFIX: &str = "folder:";
/// The media a folder scope can admit, by extension, with the kind its entries carry. A media entry is
/// listed by its metadata (path, size, mtime, kind): no byte of it enters a text index or a whole-file
/// digest, and a read of its content is refused in words.
pub(crate) const MEDIA_KINDS: [(&str, &str); 6] = [
    ("wav", "audio"),
    ("aif", "audio"),
    ("aiff", "audio"),
    ("flac", "audio"),
    ("mp3", "audio"),
    ("m4a", "audio"),
];
/// A folder scope's declaration, as `scope-register` wrote it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct FolderScope {
    pub schema: String,
    pub name: String,
    /// The folder's absolute, canonical path.
    pub root: String,
    pub kind: String,
    /// The media extensions it admits, lower case and sorted.
    pub media: Vec<String>,
    /// Folder names never walked, at any depth (a tool's own folders inside the person's, such as an app's backups), sorted.
    #[serde(default)]
    pub exclude: Vec<String>,
    pub registered_at: String,
}
/// Folders a folder scope never walks besides the ones it excludes: macOS packages, which Finder shows as one file (an app, an app
/// extension, a plug-in, a Logic, GarageBand or Final Cut project, a media library, a sparse disk image, an iWork document). A folder
/// that holds a bundle's `Contents/Info.plist` is a package whatever its name. A hidden folder and a symbolic link are never walked
/// either, nor a folder mounted from another volume.
pub(crate) const FOLDER_PACKAGES: [&str; 24] = [
    "aaxplugin",
    "app",
    "appex",
    "band",
    "bundle",
    "clap",
    "component",
    "fcpbundle",
    "framework",
    "kext",
    "key",
    "logic",
    "logicx",
    "lv2",
    "musiclibrary",
    "numbers",
    "pages",
    "photoslibrary",
    "plugin",
    "sparsebundle",
    "tvlibrary",
    "vst",
    "vst3",
    "xpc",
];
/// How deep a folder scope is walked: as deep as Central's own scans go.
const FOLDER_DEPTH: usize = 24;
/// The most entries one page of a folder scope's listing holds.
pub(crate) const FOLDER_PAGE_MOST: usize = 5000;
/// How many entries a page of a folder scope's listing holds when its read names no limit: a folder scope is never read whole.
pub(crate) const FOLDER_PAGE_DEFAULT: usize = 2000;
/// How many folders one page's walk reads at most: twice the entries of the largest page, so only a tree of far more folders than
/// media cuts a page short by this bound.
const FOLDER_WALK_DIRECTORIES: usize = 10_000;
/// How long one page's walk may take. Past it the page holds what the walk listed by then: a File Provider or a network volume
/// answering slowly, or not at all, costs the reader this long and no longer.
pub(crate) const FOLDER_WALK_TIME: std::time::Duration = std::time::Duration::from_secs(5);
/// How many of the folders and entries a walk skipped its answer names, with why; past this they are counted only.
const FOLDER_SKIPPED_NAMED: usize = 32;
/// The flag Finder hides a file or a folder by (`chflags hidden`): `UF_HIDDEN` in <sys/stat.h>.
#[cfg(target_os = "macos")]
const UF_HIDDEN: u32 = 0x0000_8000;
/// Whether `name`, with its `lstat` `metadata`, is hidden as macOS hides it: a leading dot (macOS's AppleDouble companions, `._name`,
/// among them) or Finder's hidden flag.
pub(crate) fn folder_hidden(name: &str, metadata: &fs::Metadata) -> bool {
    name.starts_with('.') || hidden_flag(metadata)
}
#[cfg(target_os = "macos")]
fn hidden_flag(metadata: &fs::Metadata) -> bool {
    use std::os::macos::fs::MetadataExt as _;
    metadata.st_flags() & UF_HIDDEN != 0
}
#[cfg(not(target_os = "macos"))]
fn hidden_flag(_: &fs::Metadata) -> bool {
    false
}
/// Whether a folder named `name` is a package by its extension, whatever its case.
fn package_by_name(name: &str) -> bool {
    name.rsplit_once('.').is_some_and(|(_, extension)| {
        FOLDER_PACKAGES.contains(&extension.to_ascii_lowercase().as_str())
    })
}
/// Whether the folder at `path` holds a bundle's `Contents/Info.plist`, seen by `lstat` alone: a package whatever its name.
fn holds_bundle(path: &Path) -> bool {
    fs::symlink_metadata(path.join("Contents")).is_ok_and(|contents| contents.is_dir())
        && fs::symlink_metadata(path.join("Contents/Info.plist")).is_ok_and(|plist| plist.is_file())
}
/// Whether a folder scope's walk enters a folder it meets.
enum Enters {
    Yes,
    /// Never, by the scope's rules: a symbolic link, a hidden folder, a package or a folder it excludes.
    No,
    /// Not, and the answer names it with why: another volume is mounted there.
    Skipped(&'static str),
}
/// The rules a folder scope's walk keeps, the same for a page's walk and for one path's resolution: what the walk never enters or
/// lists, a resolve or a locate answers as outside the scope.
pub(crate) struct FolderRules {
    folder: FolderScope,
    /// The device of the scope's root: the walk never enters a folder on another.
    device: u64,
    #[cfg(test)]
    other_volume: Option<PathBuf>,
}
impl FolderRules {
    pub(crate) fn of(scope: &Scope) -> io::Result<Self> {
        let (folder, _) = scope.folder()?;
        Ok(Self {
            folder,
            device: fs::symlink_metadata(&scope.root)?.dev(),
            #[cfg(test)]
            other_volume: FOLDER_WALK_SEAMS.with(|seams| seams.borrow().other_volume.clone()),
        })
    }
    fn device_of(&self, path: &Path, metadata: &fs::Metadata) -> u64 {
        #[cfg(test)]
        if self.other_volume.as_deref() == Some(path) {
            return self.device.wrapping_add(1);
        }
        let _ = path;
        metadata.dev()
    }
    /// Whether the walk enters the folder at `path`, named `name`, by its `lstat` `metadata`. An exclusion matches the folder's name
    /// whatever its case, as macOS's file system does.
    fn enters(&self, path: &Path, name: &str, metadata: &fs::Metadata) -> Enters {
        self.enters_unless(path, name, metadata, || false)
            .unwrap_or(Enters::No)
    }
    /// [`Self::enters`], asking `stop` before the one read it makes (the bundle probe): `None` when `stop` answers true, and then
    /// the probe is not read.
    fn enters_unless(
        &self,
        path: &Path,
        name: &str,
        metadata: &fs::Metadata,
        stop: impl FnOnce() -> bool,
    ) -> Option<Enters> {
        if !metadata.is_dir() || folder_hidden(name, metadata) {
            return Some(Enters::No);
        }
        let lower = name.to_lowercase();
        if self
            .folder
            .exclude
            .iter()
            .any(|excluded| excluded.to_lowercase() == lower)
            || package_by_name(name)
        {
            return Some(Enters::No);
        }
        if stop() {
            return None;
        }
        if holds_bundle(path) {
            return Some(Enters::No);
        }
        if self.device_of(path, metadata) != self.device {
            return Some(Enters::Skipped("another volume is mounted here"));
        }
        Some(Enters::Yes)
    }
    /// The media kind of the file at `path`, named `name`, when the walk lists it: an ordinary file, not hidden, of a kind the scope
    /// admits.
    fn lists(&self, path: &Path, name: &str, metadata: &fs::Metadata) -> Option<&'static str> {
        if !metadata.is_file() || folder_hidden(name, metadata) {
            return None;
        }
        media_kind(&self.folder.media, path)
    }
}
/// What a test sets for a folder scope's walk on this thread: smaller bounds, a folder taken for another volume, a folder whose
/// reading stalls on a FIFO with no writer, and a path taken away just before the walk reads it.
#[cfg(test)]
#[derive(Clone, Debug, Default)]
pub(crate) struct FolderWalkSeams {
    pub directories: Option<usize>,
    pub time: Option<std::time::Duration>,
    pub other_volume: Option<PathBuf>,
    pub stall: Option<(PathBuf, PathBuf)>,
    pub vanish: Option<PathBuf>,
    /// Paths the walk is handed under another name, these bytes: a name that is not UTF-8, as a Linux disk or a network share hands
    /// one (APFS keeps only UTF-8 names). The walk reads each where it stands.
    pub not_utf8: Vec<(PathBuf, Vec<u8>)>,
    /// The walk's deadline as a logical clock: it passes once this many folders are read, so a page is the same on any machine.
    pub deadline_after: Option<usize>,
    /// Every path the walk reads (a folder listed, a file's metadata), in order.
    pub read: Option<std::sync::Arc<std::sync::Mutex<Vec<PathBuf>>>>,
    /// Every folder whose listing the walk opens, in order: as it opens it, after every seam of the read has run.
    pub listed: Option<std::sync::Arc<std::sync::Mutex<Vec<PathBuf>>>>,
    /// The page a read that names no limit gets.
    pub page_default: Option<usize>,
    /// The stall says nothing to the reader: the reader leaves at its own time bound, as a read that never answers leaves it.
    pub silent: bool,
    /// The walk's deadline passes as it reads this path (a folder listed, a folder's metadata or bundle probe, a file's metadata),
    /// whatever its time bound.
    pub pass_deadline_at: Option<PathBuf>,
}
/// How many entries a page holds when its read names no limit ([`FOLDER_PAGE_DEFAULT`]; a test's seam may set fewer).
pub(crate) fn folder_page_default() -> usize {
    #[cfg(test)]
    if let Some(page) = FOLDER_WALK_SEAMS.with(|seams| seams.borrow().page_default) {
        return page;
    }
    FOLDER_PAGE_DEFAULT
}
/// The folders (their canonical paths) a walk is running in now: one walk of a folder at a time, so a folder that stops answering
/// holds one helper thread, never one for every read of it.
static FOLDERS_WALKING: std::sync::Mutex<BTreeSet<PathBuf>> =
    std::sync::Mutex::new(BTreeSet::new());
static FOLDER_WALK_ENDED: std::sync::Condvar = std::sync::Condvar::new();
/// The folders a read has found walked by another and waited for (a test releases a parked walk once a read waits for it).
#[cfg(test)]
static FOLDERS_WAITED: std::sync::Mutex<BTreeSet<PathBuf>> = std::sync::Mutex::new(BTreeSet::new());
/// Whether a read has waited for another walk of the folder at `root`.
#[cfg(test)]
pub(crate) fn folder_waited(root: &Path) -> bool {
    FOLDERS_WAITED
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .contains(root)
}
/// What a page answers when a walk of its folder is still running past its own bound.
const FOLDER_STILL_WALKING: &str =
    "a walk of this folder is still running, so this page lists none of it: ask again when it ends";
fn folders_walking() -> std::sync::MutexGuard<'static, BTreeSet<PathBuf>> {
    FOLDERS_WALKING
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}
/// A walk running in its folder: the folder is free again when its walk ends, however it ends.
struct FolderWalking(PathBuf);
impl Drop for FolderWalking {
    fn drop(&mut self) {
        folders_walking().remove(&self.0);
        FOLDER_WALK_ENDED.notify_all();
    }
}
/// Whether a walk of the folder at `root` (its canonical path) is running now (a test waits on it for a parked walk to end).
#[cfg(test)]
pub(crate) fn folder_walk_running(root: &Path) -> bool {
    folders_walking().contains(root)
}
#[cfg(test)]
thread_local! {
    pub(crate) static FOLDER_WALK_SEAMS: std::cell::RefCell<FolderWalkSeams> =
        std::cell::RefCell::new(FolderWalkSeams::default());
}
/// The kind of media `path` is, when its extension is one the scope admits.
pub(crate) fn media_kind(media: &[String], path: &Path) -> Option<&'static str> {
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    if !media.contains(&extension) {
        return None;
    }
    MEDIA_KINDS
        .iter()
        .find(|(known, _)| *known == extension)
        .map(|(_, kind)| *kind)
}
/// The media extensions Central admits, in words ("wav, aif, aiff, flac, mp3 and m4a").
pub(crate) fn media_words() -> String {
    let known: Vec<&str> = MEDIA_KINDS
        .iter()
        .map(|(extension, _)| *extension)
        .collect();
    match known.split_last() {
        Some((last, rest)) if !rest.is_empty() => format!("{} and {last}", rest.join(", ")),
        _ => known.join(""),
    }
}
/// The folder scopes this ground declares: each `.central/scopes/<name>/scope.json`. A folder under `.central/scopes` without one
/// is an external project's state. A declaration that does not read is that one scope's failure, never taken for absence: it comes
/// back as its folder's name with the words why, and the other declarations stand.
pub(crate) fn folder_declarations(
    root: &Path,
) -> io::Result<Vec<Result<FolderScope, (String, String)>>> {
    let scopes = root.join(".central/scopes");
    match fs::symlink_metadata(&scopes) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
        Ok(metadata) if !metadata.is_dir() => {
            return Err(invalid("The ground's .central/scopes is not a folder"))
        }
        Ok(_) => {}
    }
    reject_symlink_components(root, Path::new(".central/scopes"))?;
    let mut names = fs::read_dir(&scopes)?
        .map(|entry| entry.map(|entry| entry.file_name()))
        .collect::<io::Result<Vec<_>>>()?;
    names.sort();
    let mut result = Vec::new();
    for name in names {
        let Some(name) = name.to_str() else {
            continue;
        };
        let dir = scopes.join(name);
        let declared = (|| -> io::Result<Option<FolderScope>> {
            if !fs::symlink_metadata(&dir)?.is_dir() {
                return Ok(None);
            }
            match fs::symlink_metadata(dir.join(FOLDER_SCOPE_DECLARATION)) {
                Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
                Err(error) => return Err(error),
                Ok(metadata) if !metadata.is_file() => {
                    return Err(invalid(format!(
                        "The folder scope declaration in .central/scopes/{name} is not an ordinary file"
                    )))
                }
                Ok(_) => {}
            }
            let (folder, _) = read_folder_declaration(&dir)?;
            if folder.name != name {
                return Err(invalid(format!(
                    "The folder scope declared in .central/scopes/{name} names itself {}",
                    folder.name
                )));
            }
            Ok(Some(folder))
        })();
        match declared {
            Ok(Some(folder)) => result.push(Ok(folder)),
            Ok(None) => {}
            Err(error) => result.push(Err((name.to_owned(), error.to_string()))),
        }
    }
    Ok(result)
}
/// A folder scope's declaration in `dir` (`.central/scopes/<name>` of the ground), and its revision. Its words name the
/// declaration by its place in the ground, never by the ground's absolute path: they reach a client as an unreachable folder's
/// cause (DLW build ruling 2026-11-09, 47f item 1).
pub(crate) fn read_folder_declaration(dir: &Path) -> io::Result<(FolderScope, String)> {
    let path = dir.join(FOLDER_SCOPE_DECLARATION);
    let named = format!(
        ".central/scopes/{}/{FOLDER_SCOPE_DECLARATION}",
        dir.file_name().unwrap_or_default().to_string_lossy()
    );
    if fs::symlink_metadata(&path)?.len() > 64 * 1024 {
        return Err(invalid(format!(
            "The folder scope declaration {named} is larger than a declaration is"
        )));
    }
    let bytes = fs::read(&path)?;
    let folder: FolderScope = serde_json::from_slice(&bytes).map_err(|error| {
        invalid(format!(
            "The folder scope declaration {named} does not read as one: {error}"
        ))
    })?;
    if folder.schema != FOLDER_SCOPE_SCHEMA
        || folder.kind != "folder"
        || !Path::new(&folder.root).is_absolute()
    {
        return Err(invalid(format!(
            "The folder scope declaration {named} is not one Central reads: it needs schema \
             {FOLDER_SCOPE_SCHEMA}, kind folder and an absolute root"
        )));
    }
    Ok((folder, content_revision_bytes(&bytes)))
}
#[derive(Clone, Debug)]
pub(crate) struct Scope {
    pub root: PathBuf,
    /// Where this scope's derived state lives: its bkmr database, bindings
    /// index, caches, locks and relation edits. A scope inside the ground keeps
    /// them under `root`. An external scope — an absolute path outside the
    /// ground, such as a folder in ~/Documents — is read-only: placement.json
    /// grants no writes there, so its state lives under
    /// `<ground>/.central/scopes/<name>` instead.
    pub state: PathBuf,
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
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Entry {
    pub source: SourceBinding,
    pub world_ref: String,
    pub path: PathBuf,
    pub kind: String,
    pub revision: String,
    pub title: String,
    pub tags: Vec<String>,
    pub native_import: bool,
    /// A media entry's length and modification time (seconds), from its metadata alone: Central reads
    /// none of a media file's content (see [`MEDIA_KINDS`]). Absent for every other entry.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mtime: Option<i64>,
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
/// The words of what made a read fail, beneath Central's own words for it: a folder scope's reader names its folder's own error
/// plainly, as the folder's walk names it in `skipped_named`.
pub(crate) fn failure_cause(error: &io::Error) -> String {
    error
        .get_ref()
        .and_then(|inner| inner.downcast_ref::<ReadFailure>())
        .and_then(|failure| failure.cause.as_ref())
        .map_or_else(|| error.to_string(), ToString::to_string)
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
/// One batch phase's observations of per-scope declarations: each scope's
/// relations document and Project manifest, observed once for the phase.
#[derive(Default)]
struct ObservationPhase {
    documents: BTreeMap<(PathBuf, String), Rc<(Value, String)>>,
    manifests: BTreeMap<PathBuf, (crate::projectcentral::ProjectCentralManifest, String)>,
    declarations: BTreeMap<(String, PathBuf, PathBuf), Rc<ScopeDeclarations>>,
}
thread_local! {
    static OBSERVATION_PHASE: std::cell::RefCell<Option<ObservationPhase>> =
        const { std::cell::RefCell::new(None) };
}

/// Run `phase` with every scope's declarations observed at most once.
///
/// A single `resolve` re-observes every participating scope's relations
/// document and Project manifest on each lookup — five or more lookups per
/// Source, each across every scope — and its in-phase cross-checks compare
/// those repeated observations. A batch resolving many Sources would repeat
/// that World observation per Source (0.3 s each on the 18 Oct 2026 ground).
/// Inside a phase these declarations are observed once; the guarantee moves
/// to the phase's edges: the batch brackets each phase with
/// `ownership_bases`, which observes exactly these documents fresh, so a
/// declaration that changes during a phase refuses the batch. Per-Source
/// observations — skill metadata, retrieval markers, payloads — are never
/// held. Outside a phase nothing is held, so every other read is unchanged.
pub(crate) fn within_observation_phase<T>(phase: impl FnOnce() -> T) -> T {
    struct Restore(Option<ObservationPhase>);
    impl Drop for Restore {
        fn drop(&mut self) {
            let outer = self.0.take();
            OBSERVATION_PHASE.with(|held| {
                held.replace(outer);
            });
        }
    }
    let _restore =
        Restore(OBSERVATION_PHASE.with(|held| held.replace(Some(ObservationPhase::default()))));
    phase()
}

fn phase_document(
    root: &Path,
    member: &str,
    observe: impl FnOnce() -> io::Result<(Value, String)>,
) -> io::Result<Rc<(Value, String)>> {
    let key = (root.to_path_buf(), member.to_owned());
    let held = OBSERVATION_PHASE.with(|held| {
        held.borrow()
            .as_ref()
            .and_then(|phase| phase.documents.get(&key).cloned())
    });
    if let Some(held) = held {
        return Ok(held);
    }
    let observed = Rc::new(observe()?);
    OBSERVATION_PHASE.with(|held| {
        if let Some(phase) = held.borrow_mut().as_mut() {
            phase.documents.insert(key, Rc::clone(&observed));
        }
    });
    Ok(observed)
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
    let held = OBSERVATION_PHASE.with(|held| {
        held.borrow()
            .as_ref()
            .and_then(|phase| phase.manifests.get(root).cloned())
    });
    if let Some(held) = held {
        return Ok(held);
    }
    let observed = observe_project_manifest(root)?;
    OBSERVATION_PHASE.with(|held| {
        if let Some(phase) = held.borrow_mut().as_mut() {
            phase.manifests.insert(root.to_path_buf(), observed.clone());
        }
    });
    Ok(observed)
}
fn observe_project_manifest(
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
            } else if scope.is_folder() {
                Some(scope.folder()?.1)
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
/// Why nothing is registered, linked, pooled or moved in a folder scope.
pub(crate) fn folder_writes_nothing(scope: &Scope) -> io::Error {
    invalid(format!(
        "{} is a folder scope: Central reads {} and writes nothing inside it, so nothing is \
         registered, linked, pooled or moved there",
        scope.project.as_deref().unwrap_or(&scope.world),
        scope.root.display()
    ))
}
/// What every act that would keep an index or a projection receipt for a folder scope answers instead.
pub(crate) fn folder_keeps_no_index(scope: &Scope) -> io::Error {
    invalid(format!(
        "{} is a folder scope: Central reads {} where it stands and keeps no index or projection receipt for it",
        scope.project.as_deref().unwrap_or(&scope.world),
        scope.root.display()
    ))
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
/// digest per source and it grows with the pool, so it is read whole.
fn read_index_json(path: &Path) -> io::Result<Value> {
    serde_json::from_slice(&fs::read(path)?)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}
impl Scope {
    pub fn project(root: PathBuf, project: Option<String>) -> io::Result<Self> {
        let manifest = project_manifest(&root)
            .map_err(|error| read_failure(error, "known", "project_declaration", "unavailable"))?;
        Ok(Self::declared(root, project, &manifest))
    }
    fn declared(
        root: PathBuf,
        project: Option<String>,
        manifest: &crate::projectcentral::ProjectCentralManifest,
    ) -> Self {
        Self {
            state: root.clone(),
            root,
            world: format!("project:{}", manifest.project_id),
            project,
        }
    }
    pub fn is_external(&self) -> bool {
        self.state != self.root
    }
    /// A plain folder registered as a scope ([`FOLDER_SCOPE_SCHEMA`]): no ProjectCentral, its
    /// declaration in the ground, nothing written inside it.
    pub fn is_folder(&self) -> bool {
        self.world.starts_with(FOLDER_WORLD_PREFIX)
    }
    /// What kind of scope this is: the root's own ("control"), a project's, or a folder's.
    pub fn kind(&self) -> &'static str {
        if self.world == "control:root" {
            "control"
        } else if self.is_folder() {
            "folder"
        } else {
            "project"
        }
    }
    /// A folder scope's declaration and its revision.
    pub(crate) fn folder(&self) -> io::Result<(FolderScope, String)> {
        if !self.is_folder() {
            return Err(invalid("Only a folder scope has a folder declaration"));
        }
        read_folder_declaration(&self.state)
    }
    /// The relations document an external scope is read from: its own
    /// ProjectCentral copy until the ground has edited it, then the ground's.
    /// A folder scope has no ProjectCentral, so nothing of it is read there.
    fn relations_root(&self) -> &Path {
        if self.is_folder()
            || self.is_external() && self.state.join(self.relations_path()).is_file()
        {
            &self.state
        } else {
            &self.root
        }
    }
    pub fn relations_path(&self) -> &'static str {
        if self.world == "control:root" {
            source_horizon::CONTROL_GROUND_RELATIONS_SOURCE
        } else {
            source_horizon::GROUND_RELATIONS_SOURCE
        }
    }
    pub(crate) fn observed_document(&self) -> io::Result<(Value, String)> {
        self.shared_document()
            .map(|observed| (observed.0.clone(), observed.1.clone()))
    }
    /// The validated relations document, held for a batch phase.
    fn shared_document(&self) -> io::Result<Rc<(Value, String)>> {
        let (root, member) = (self.relations_root(), self.relations_path());
        phase_document(root, member, || self.observe_document())
    }
    fn observe_document(&self) -> io::Result<(Value, String)> {
        let (doc, basis) = metadata_document(self.relations_root(), self.relations_path())?;
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
        self.shared_document().map(|observed| observed.1.clone())
    }
    pub fn save(&self, ground: &MapGround, mut doc: Value) -> io::Result<()> {
        if self.is_folder() {
            return Err(folder_writes_nothing(self));
        }
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
        safe_directory(&self.state, path.parent().unwrap())?;
        write_atomic(
            &self.state,
            &self.state.join(path),
            &serde_json::to_vec_pretty(&doc)?,
            crate::file_mutation::RecordDisposition::ReplaceOrCreate,
        )
    }
    pub fn index(&self) -> io::Result<Index> {
        let path = safe_member(&self.state, INDEX, false)?;
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
        safe_directory(&self.state, Path::new(INDEX).parent().unwrap())?;
        write_atomic(
            &self.state,
            &self.state.join(INDEX),
            &serde_json::to_vec_pretty(index)?,
            crate::file_mutation::RecordDisposition::ReplaceOrCreate,
        )
    }
    /// The enumeration snapshot federated search serves from. `entries()` walks
    /// the whole pooled tree with per-file metadata, which is refresh-scale
    /// work; a query must load the last refresh's walk instead of redoing it.
    /// Staleness stays safe: matched files are content-hashed at query time and
    /// a stale binding is disclosed, not served.
    ///
    /// The snapshot is derived state, so it lives with the rest of it under `state`: an external
    /// scope's tree is never written. A folder scope keeps none: its listing reads metadata only, and
    /// a file put in the folder is listed at once.
    pub fn save_entries_cache(&self, entries: &[Entry]) -> io::Result<()> {
        if self.is_folder() {
            return Ok(());
        }
        write_atomic(
            &self.state,
            &self.state.join(ENTRIES_CACHE),
            &serde_json::to_vec(entries)?,
            crate::file_mutation::RecordDisposition::ReplaceOrCreate,
        )
    }
    pub fn load_entries_cache(&self) -> io::Result<Option<Vec<Entry>>> {
        if self.is_folder() {
            return Ok(None);
        }
        match fs::read(self.state.join(ENTRIES_CACHE)) {
            Ok(bytes) => Ok(serde_json::from_slice(&bytes)
                .map(Some)
                .map_err(io::Error::other)?),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error),
        }
    }
}
/// How long an external scope's root may take to open before the scope is
/// reported unreachable. On macOS ~/Documents is a File Provider domain: while
/// its provider stalls, a plain open() of it blocks every process (5 Nov 2026:
/// a world refresh waited 17 minutes opening ~/Documents/epi, and the gateway
/// stop that waited on it was held as long).
pub(crate) const EXTERNAL_ROOT_DEADLINE: std::time::Duration = std::time::Duration::from_secs(3);

/// External roots whose opening thread has not returned. Asked again, such a
/// root is unreachable at once instead of another thread blocking the same
/// way; the thread clears it when its open finally returns.
static OPENING_EXTERNAL_ROOTS: std::sync::Mutex<BTreeSet<PathBuf>> =
    std::sync::Mutex::new(BTreeSet::new());

#[cfg(test)]
thread_local! {
    /// A root whose opening first opens this FIFO for reading: with no writer
    /// the open never returns, standing in for a stalled provider.
    pub(crate) static STALLED_EXTERNAL_ROOT: std::cell::RefCell<Option<(PathBuf, PathBuf)>> =
        const { std::cell::RefCell::new(None) };
}

/// A participating external scope whose root did not open within
/// [`EXTERNAL_ROOT_DEADLINE`]. It is left out of the scopes and named.
#[derive(Clone, Debug)]
pub(crate) struct UnreachableScope {
    pub project: String,
    /// What kind of failure it is, so a read that asks for it fails as its cause does: a folder gone is not found, a root that does
    /// not answer timed out.
    pub kind: io::ErrorKind,
    /// A folder scope's World (`folder:<name>`): its name alone never selects it.
    pub world: Option<String>,
    /// Where it stands, where that is known: none for a folder whose declaration does not read, which so holds no location
    /// (DLW build ruling 2026-11-09, 47f item 1).
    pub root: Option<PathBuf>,
    pub message: String,
}

/// An unreachable scope's root, from the path it was named by: none where it was named by none.
fn unreachable_root(path: String) -> Option<PathBuf> {
    (!path.is_empty()).then(|| PathBuf::from(path))
}

type ExternalOpened = (
    PathBuf,
    (crate::projectcentral::ProjectCentralManifest, String),
);

enum ExternalOpening<T = ExternalOpened> {
    Held(Box<T>),
    Waiting(std::sync::mpsc::Receiver<io::Result<T>>),
    StillOpening,
    Failed(io::Error),
}

/// Start opening an external scope's root on a helper thread. An abandoned
/// thread (one whose open has not returned by the deadline) ends with the
/// process.
fn begin_external_open(raw: &str) -> ExternalOpening {
    let key = PathBuf::from(raw);
    let held = OBSERVATION_PHASE.with(|held| {
        held.borrow()
            .as_ref()
            .and_then(|phase| phase.manifests.get(&key).cloned())
    });
    if let Some(held) = held {
        return ExternalOpening::Held(Box::new((key, held)));
    }
    begin_open(raw, open_external_root)
}

/// Open an external root with `open` on a helper thread, unless an earlier opening of it has not
/// returned yet.
fn begin_open<T: Send + 'static>(
    raw: &str,
    open: impl FnOnce(&str) -> io::Result<T> + Send + 'static,
) -> ExternalOpening<T> {
    let key = PathBuf::from(raw);
    if !OPENING_EXTERNAL_ROOTS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .insert(key.clone())
    {
        return ExternalOpening::StillOpening;
    }
    #[cfg(test)]
    let stall = STALLED_EXTERNAL_ROOT.with(|slot| {
        slot.borrow()
            .as_ref()
            .filter(|(root, _)| *root == key)
            .map(|(_, fifo)| fifo.clone())
    });
    let (send, receive) = std::sync::mpsc::channel();
    let owned = raw.to_owned();
    let spawned = std::thread::Builder::new()
        .name("file-map-external-root".into())
        .spawn(move || {
            #[cfg(test)]
            if let Some(fifo) = stall {
                let _ = fs::File::open(fifo);
            }
            let opened = open(&owned);
            OPENING_EXTERNAL_ROOTS
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .remove(Path::new(&owned));
            let _ = send.send(opened);
        });
    match spawned {
        Ok(_) => ExternalOpening::Waiting(receive),
        Err(error) => {
            OPENING_EXTERNAL_ROOTS
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .remove(&key);
            ExternalOpening::Failed(error)
        }
    }
}

/// What an external scope's first touch reads: its root, contained, and its
/// Project declaration. These are the opens a stalled provider blocks.
fn open_external_root(raw: &str) -> io::Result<ExternalOpened> {
    let target = safe_member(Path::new("/"), raw.trim_start_matches('/'), true)
        .map_err(|error| read_failure(error, "known", "owner_root", "unavailable"))?;
    let observed = observe_project_manifest(&target)
        .map_err(|error| read_failure(error, "known", "project_declaration", "unavailable"))?;
    Ok((target, observed))
}

/// What a folder scope's first touch reads: its root, contained, a folder it may open, and still where registration would have it,
/// so a declaration edited by hand is refused as its registration would be: outside the ground (`ground`, canonical), never around
/// it, and holding no ProjectCentral. Each is asked of the folder's canonical path. Nothing in it is read beyond its own listing's
/// opening.
fn open_folder_root(raw: &str, ground: &Path) -> io::Result<PathBuf> {
    let target = match raw.trim_start_matches('/') {
        "" => PathBuf::from("/"),
        inside => safe_member(Path::new("/"), inside, true)?,
    };
    if !fs::metadata(&target)?.is_dir() {
        return Err(invalid(format!("{raw} is not a folder")));
    }
    let canonical = target.canonicalize()?;
    // Registration writes the folder's canonical path: any other spelling (a trailing slash, `.` or `..`, a link, another case) is a
    // declaration edited by hand, refused as registration would refuse it.
    if canonical.as_os_str() != std::ffi::OsStr::new(raw) {
        return Err(invalid(format!(
            "{raw} is not the folder's own spelling ({}), the one registration writes",
            canonical.display()
        )));
    }
    // Said of the folder's own declared root alone: these words ride on every answer while the folder is unreachable, and the
    // ground's own path is not theirs to show (DLW build ruling 2026-11-09, 47g item 5).
    if canonical.starts_with(ground) {
        return Err(invalid(format!("{raw} is inside this ground")));
    }
    if ground.starts_with(&canonical) {
        return Err(invalid(format!("{raw} holds this ground")));
    }
    match fs::symlink_metadata(canonical.join("ProjectCentral/project.json")) {
        Ok(_) => return Err(invalid(format!("{raw} holds a ProjectCentral"))),
        Err(error)
            if error.kind() == io::ErrorKind::NotFound
                || error.raw_os_error() == Some(libc::ENOTDIR) => {}
        Err(error) => return Err(error),
    }
    // Opened as a listing, as a reader will: a folder macOS's privacy or the folder's own mode keeps closed is refused here.
    drop(fs::read_dir(&canonical)?);
    Ok(target)
}

impl<T> ExternalOpening<T> {
    /// The opened root, `Ok(None)` when it did not open by `deadline`.
    fn wait(self, deadline: std::time::Instant) -> io::Result<Option<T>> {
        match self {
            Self::Held(opened) => Ok(Some(*opened)),
            Self::StillOpening => Ok(None),
            Self::Failed(error) => Err(error),
            Self::Waiting(receive) => {
                let left = deadline.saturating_duration_since(std::time::Instant::now());
                match receive.recv_timeout(left) {
                    Ok(opened) => opened.map(Some),
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => Ok(None),
                    Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => Err(io::Error::other(
                        "the external root's opening thread ended without an answer",
                    )),
                }
            }
        }
    }
}

pub(crate) fn scopes(root: &Path) -> io::Result<Vec<Scope>> {
    participating(root).map(|(scopes, _)| scopes)
}

/// Every participating scope, and the external ones whose roots did not open
/// within [`EXTERNAL_ROOT_DEADLINE`]: those are left out and named, and the
/// others stand. Each external root opens on its own helper thread, all under
/// one deadline.
pub(crate) fn participating(root: &Path) -> io::Result<(Vec<Scope>, Vec<UnreachableScope>)> {
    participating_as(root, true)
}

/// [`participating`] that makes nothing: an external scope's state folder (`.central/scopes/<name>`) is named, and not made. The
/// person's acts of the file map read where their grant stands on this (file_map.rs `grant_scope`), so an act refused there
/// leaves no folder behind (DLW build ruling 2026-11-09, round 54b item 1).
///
/// A folder scope's state is still taken through `external_state`, which makes a missing folder, and it makes nothing here: a folder
/// scope is read from its declaration, `.central/scopes/<name>/scope.json`, which is in that same folder, so the folder stands
/// whenever the scope does. "Makes nothing" holds for a folder scope by that, not by this function (round 54d F7).
pub(crate) fn participating_unmade(root: &Path) -> io::Result<(Vec<Scope>, Vec<UnreachableScope>)> {
    participating_as(root, false)
}

fn participating_as(
    root: &Path,
    make_state: bool,
) -> io::Result<(Vec<Scope>, Vec<UnreachableScope>)> {
    let manifest = match fs::symlink_metadata(root.join("ProjectCentral/project.json")) {
        Ok(metadata) if metadata.is_file() => true,
        Ok(_) => return Err(invalid("Project manifest is not an ordinary file")),
        Err(error) if error.kind() == io::ErrorKind::NotFound => false,
        Err(error) => return Err(error),
    };
    let control = optional_directory(&root.join("Control"))?;
    if manifest && !control {
        return Ok((vec![Scope::project(root.into(), None)?], Vec::new()));
    }
    if !control {
        return Err(invalid(
            "No native Control or ProjectCentral owner aperture",
        ));
    }
    let base = Scope {
        root: root.into(),
        state: root.into(),
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
    // A folder scope's name is a scope name like any other: one namespace. A folder scope that cannot stand (its declaration does
    // not read, or a project holds its name) is unreachable, in words, and every other scope stands.
    let mut unreachable = Vec::new();
    // `folder:` names a folder scope's World: a project spelled so (a folder under Work, or a hand-edited register) would shadow
    // one, so it does not stand, in words.
    let shadowing: Vec<String> = candidates
        .keys()
        .filter(|name| name.starts_with(FOLDER_WORLD_PREFIX))
        .cloned()
        .collect();
    for name in shadowing {
        let path = candidates.remove(&name).unwrap_or_default();
        unreachable.push(UnreachableScope {
            kind: io::ErrorKind::InvalidInput,
            world: None,
            message: format!(
                "unreachable: a project named {name} ({path}) does not stand: {FOLDER_WORLD_PREFIX} names a folder scope, so the \
                 project is not read"
            ),
            project: name,
            root: unreachable_root(path),
        });
    }
    let mut folders = Vec::new();
    for declared in folder_declarations(root)? {
        let folder = match declared {
            Ok(folder) => folder,
            Err((name, words)) => {
                unreachable.push(UnreachableScope {
                    kind: io::ErrorKind::InvalidData,
                    world: Some(format!("{FOLDER_WORLD_PREFIX}{name}")),
                    message: format!(
                        "unreachable: the folder scope declared in .central/scopes/{name} does not read: {words}"
                    ),
                    project: name,
                    root: None,
                });
                continue;
            }
        };
        if let Some(path) = candidates.get(&folder.name) {
            unreachable.push(UnreachableScope {
                kind: io::ErrorKind::AlreadyExists,
                world: Some(format!("{FOLDER_WORLD_PREFIX}{}", folder.name)),
                message: format!(
                    "unreachable: the folder scope {} shares its name with a project of this ground ({path}): the project \
                     stands, and the folder is not read until one of them is renamed",
                    folder.name
                ),
                project: folder.name,
                root: unreachable_root(folder.root),
            });
            continue;
        }
        folders.push(folder);
    }
    if candidates.len() + folders.len() > 512 {
        return Err(invalid("More than 512 participating scopes"));
    }
    let mut result = vec![base];
    let mut seen = BTreeSet::new();
    let deadline = std::time::Instant::now() + EXTERNAL_ROOT_DEADLINE;
    let mut openings = candidates
        .iter()
        .filter(|(_, path)| Path::new(path).is_absolute())
        .map(|(name, path)| (name.clone(), begin_external_open(path)))
        .collect::<BTreeMap<_, _>>();
    let ground = root.canonicalize()?;
    let folder_openings = folders
        .iter()
        .map(|folder| {
            let ground = ground.clone();
            begin_open(&folder.root, move |raw| open_folder_root(raw, &ground))
        })
        .collect::<Vec<_>>();
    for (name, path) in candidates {
        let (target, scope) = match openings.remove(&name) {
            Some(opening) => match opening.wait(deadline)? {
                Some((target, observed)) => {
                    OBSERVATION_PHASE.with(|held| {
                        if let Some(phase) = held.borrow_mut().as_mut() {
                            phase.manifests.insert(target.clone(), observed.clone());
                        }
                    });
                    let scope = Scope::declared(target.clone(), Some(name.clone()), &observed.0);
                    (target, scope)
                }
                None => {
                    unreachable.push(UnreachableScope {
                        kind: io::ErrorKind::TimedOut,
                        world: None,
                        message: format!(
                            "unreachable: {path} did not open within {}s (a File Provider or \
                             network volume not answering)",
                            EXTERNAL_ROOT_DEADLINE.as_secs()
                        ),
                        project: name,
                        root: unreachable_root(path),
                    });
                    continue;
                }
            },
            None => {
                let target = safe_member(root, &path, true)
                    .map_err(|error| read_failure(error, "known", "owner_root", "unavailable"))?;
                let scope = Scope::project(target.clone(), Some(name.clone()))?;
                (target, scope)
            }
        };
        let external = !target.starts_with(root);
        let mut scope = scope;
        if external {
            scope.state = if make_state {
                external_state(root, &name)?
            } else {
                external_state_unmade(root, &name)?
            };
        }
        if !seen.insert(scope.world.clone()) {
            return Err(invalid("Duplicate participating Project World identity"));
        }
        result.push(scope);
    }
    for (folder, opening) in folders.into_iter().zip(folder_openings) {
        let world = Some(format!("{FOLDER_WORLD_PREFIX}{}", folder.name));
        let target = match opening.wait(deadline) {
            Ok(Some(target)) => target,
            Ok(None) => {
                unreachable.push(UnreachableScope {
                    kind: io::ErrorKind::TimedOut,
                    message: format!(
                        "unreachable: {} did not open within {}s (a File Provider or network \
                         volume not answering)",
                        folder.root,
                        EXTERNAL_ROOT_DEADLINE.as_secs()
                    ),
                    world,
                    project: folder.name,
                    root: unreachable_root(folder.root),
                });
                continue;
            }
            // A folder taken away (a drive not mounted, a folder moved) leaves the other scopes standing.
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                unreachable.push(UnreachableScope {
                    kind: io::ErrorKind::NotFound,
                    message: format!(
                        "unreachable: the folder scope {} stands over {}, and no folder is there now",
                        folder.name, folder.root
                    ),
                    world,
                    project: folder.name,
                    root: unreachable_root(folder.root),
                });
                continue;
            }
            // Any other failure to open it (a folder it may not enter, macOS's privacy, a component turned into a file or a symbolic
            // link, a declaration edited to name what registration refuses) is the same: this scope is unreachable, in its words.
            Err(error) => {
                unreachable.push(UnreachableScope {
                    kind: error.kind(),
                    message: format!(
                        "unreachable: the folder scope {} stands over {}, and it did not open: {error}",
                        folder.name, folder.root
                    ),
                    world,
                    project: folder.name,
                    root: unreachable_root(folder.root),
                });
                continue;
            }
        };
        let scope = Scope {
            root: target,
            state: external_state(root, &folder.name)?,
            world: format!("{FOLDER_WORLD_PREFIX}{}", folder.name),
            project: Some(folder.name),
        };
        if !seen.insert(scope.world.clone()) {
            return Err(invalid("Duplicate participating folder scope"));
        }
        result.push(scope);
    }
    Ok((result, unreachable))
}
/// The ground-side home of an external scope's derived state, created on
/// first sight so locks and indexes never fall back into the external tree.
pub(crate) fn external_state(root: &Path, name: &str) -> io::Result<PathBuf> {
    let member = external_member(name)?;
    safe_directory(root, &member)?;
    Ok(root.join(member))
}
/// [`external_state`] named, and not made: where the state folder stands, for a read that makes nothing.
pub(crate) fn external_state_unmade(root: &Path, name: &str) -> io::Result<PathBuf> {
    Ok(root.join(external_member(name)?))
}
/// Where an external scope's state stands in the ground, relative to it: `.central/scopes/<name>`.
fn external_member(name: &str) -> io::Result<PathBuf> {
    let relative = relative(name)?;
    if relative.components().count() != 1 {
        return Err(invalid("External scope name must be one path component"));
    }
    Ok(Path::new(".central/scopes").join(relative))
}
pub(crate) fn selected<'a>(scopes: &'a [Scope], input: &Value) -> io::Result<&'a Scope> {
    match input.get("project").and_then(Value::as_str) {
        // A folder scope is asked for by its World (`folder:<name>`), and a project by its name or its World: neither name ever
        // selects the other.
        Some(project) => scopes
            .iter()
            .find(|s| s.world == project || !s.is_folder() && s.project.as_deref() == Some(project))
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
        pooled = source_horizon::insert_pool_bindings(
            &scope.root,
            &scope.world,
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
        // A folder scope's Source is one of its admitted media, and stays media: never a file whose
        // bytes a read may take.
        let media = if self.scope.is_folder() {
            let (folder, _) = self.scope.folder()?;
            Some(media_kind(&folder.media, &path).ok_or_else(|| {
                invalid(format!(
                    "{} is not media the folder scope {} admits",
                    source.path, self.scope.world
                ))
            })?)
        } else {
            None
        };
        Ok(Entry {
            source,
            world_ref: self.scope.world.clone(),
            path,
            kind: if metadata.is_dir() {
                "directory"
            } else {
                media.unwrap_or("file")
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
            size: media.map(|_| metadata.len()),
            mtime: media.map(|_| metadata.mtime()),
        })
    }
}
/// Complete explicit/adopted/resource metadata stays independent of bulk
/// fallback discovery. The selected native owner operation supplies semantics
/// only for a member capable of matching this request.
/// One scope's declared Sources as unselected candidates, with what selection
/// reads beside them: the relations document and its basis, the map ground
/// and the Project manifest.
pub(crate) struct ScopeDeclarations {
    doc: Value,
    basis: String,
    ground: MapGround,
    manifest: Option<crate::projectcentral::ProjectCentralManifest>,
    /// Each declaration with its route; None where the route is malformed, so
    /// selection re-derives the route's own error in declaration order.
    declared: Vec<(BindingCandidate, Option<PathBuf>)>,
    /// `doc` parsed on first selection and kept with the observation, so each
    /// Source of a held phase binds against one parse of the same document.
    parsed: std::cell::OnceCell<Option<source_horizon::ParsedRelations>>,
}

impl ScopeDeclarations {
    /// The observed relations document, parsed as its scope's grammar requires.
    fn parsed_relations(&self) -> io::Result<Option<&source_horizon::ParsedRelations>> {
        if self.parsed.get().is_none() {
            let parsed = if self.doc.is_null() {
                None
            } else if let Some(manifest) = &self.manifest {
                Some(source_horizon::parse_declared_relations(
                    &self.doc,
                    source_horizon::GROUND_RELATIONS_SCHEMA,
                    &manifest.project_id,
                )?)
            } else {
                Some(source_horizon::parse_declared_relations(
                    &self.doc,
                    source_horizon::CONTROL_GROUND_RELATIONS_SCHEMA,
                    source_horizon::CONTROL_WORLD_REF,
                )?)
            };
            // Only this call fills the cell, and only once: the check above holds.
            let _ = self.parsed.set(parsed);
        }
        Ok(self.parsed.get().and_then(Option::as_ref))
    }
}

/// Observe a scope's declarations — held for a batch phase, fresh otherwise.
fn scope_declarations(scope: &Scope) -> io::Result<Rc<ScopeDeclarations>> {
    let key = (scope.world.clone(), scope.root.clone(), scope.state.clone());
    let held = OBSERVATION_PHASE.with(|held| {
        held.borrow()
            .as_ref()
            .and_then(|phase| phase.declarations.get(&key).cloned())
    });
    if let Some(held) = held {
        return Ok(held);
    }
    let observed = Rc::new(observe_scope_declarations(scope)?);
    OBSERVATION_PHASE.with(|held| {
        if let Some(phase) = held.borrow_mut().as_mut() {
            phase.declarations.insert(key, Rc::clone(&observed));
        }
    });
    Ok(observed)
}

fn observe_scope_declarations(scope: &Scope) -> io::Result<ScopeDeclarations> {
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
        // Each declaration's SourceRefs by normal member key, so a relation
        // row replaces whatever is declared at its path without re-keying
        // every declaration per row (quadratic in the relations document).
        let mut at_key: Option<BTreeMap<PathBuf, BTreeSet<String>>> = None;
        for row in rows {
            let mut value = row.clone();
            value["agent_retrieval_allowed"] = json!(true);
            let binding: SourceBinding = serde_json::from_value(value)
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
            let key = crate::source_safety::normal_member_key(&binding.path)?;
            let at_key = match &mut at_key {
                Some(at_key) => at_key,
                None => {
                    let mut keyed = BTreeMap::<PathBuf, BTreeSet<String>>::new();
                    for (reference, current) in &declarations {
                        keyed
                            .entry(crate::source_safety::normal_member_key(&current.path)?)
                            .or_default()
                            .insert(reference.clone());
                    }
                    at_key.insert(keyed)
                }
            };
            for reference in at_key.remove(&key).unwrap_or_default() {
                declarations.remove(&reference);
            }
            let reference = binding.source_ref.clone();
            if let Some(previous) = declarations.insert(reference.clone(), binding) {
                let previous = crate::source_safety::normal_member_key(&previous.path)?;
                if let Some(references) = at_key.get_mut(&previous) {
                    references.remove(&reference);
                    if references.is_empty() {
                        at_key.remove(&previous);
                    }
                }
            }
            at_key.entry(key).or_default().insert(reference);
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
    let declared = declarations
        .into_values()
        .map(|source| {
            let candidate = BindingCandidate {
                scope: scope.clone(),
                resource: ground.resources.get(&source.source_ref).cloned(),
                source,
                relation_revision: basis.clone(),
                selection_metadata_basis: None,
            };
            let path = candidate.path().ok();
            (candidate, path)
        })
        .collect();
    Ok(ScopeDeclarations {
        doc,
        basis,
        ground,
        manifest,
        declared,
        parsed: std::cell::OnceCell::new(),
    })
}

fn source_candidates(
    scope: &Scope,
    reference: Option<&str>,
    location: Option<&Path>,
) -> io::Result<Vec<BindingCandidate>> {
    if scope.is_folder() {
        return folder_candidates(scope, reference, location);
    }
    let declarations = scope_declarations(scope)?;
    let ScopeDeclarations {
        basis,
        ground,
        declared,
        ..
    } = &*declarations;
    let mut selected = BTreeMap::<String, BindingCandidate>::new();
    for (declared, path) in declared {
        // Every declared route must be well-formed.
        let path = match path {
            Some(path) => path.clone(),
            None => declared.path()?,
        };
        if !reference.is_some_and(|reference| reference == declared.source.source_ref)
            && !location.is_some_and(|location| location == path.as_path())
        {
            continue;
        }
        let mut candidate = declared.clone();
        if let Ok(member) = path.strip_prefix(&scope.root) {
            let member = member
                .to_str()
                .ok_or_else(|| invalid("Native source member is not UTF-8"))?;
            let (native, metadata_basis) = selected_native_binding(scope, member, &declarations)?;
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
            let (native, metadata_basis) = selected_native_binding(scope, &member, &declarations)?;
            // NOW records are retrievable by placement: a member with the record
            // shape of a NOW family resolves by its path-derived identity with no
            // registration, exactly as `entries()` lists it.
            let native = native.or_else(|| {
                let normal = crate::source_safety::normal_member_key(&member).ok()?;
                let path = normal.to_str()?;
                let on_disk = scope.root.join(path);
                (source_horizon::is_now_record_member(&scope.world, path)
                    && fs::symlink_metadata(&on_disk).is_ok_and(|m| m.file_type().is_file()))
                .then(|| {
                    source_horizon::now_record_binding(
                        &scope.world,
                        path,
                        source_horizon::retrieval_allowed(&scope.root, &on_disk),
                    )
                })
            });
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
    if scope.basis()? != *basis {
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
    declarations: &ScopeDeclarations,
) -> io::Result<(Option<SourceBinding>, Option<String>)> {
    let member_key = crate::source_safety::normal_member_key(member)?;
    let member = member_key
        .to_str()
        .ok_or_else(|| invalid("Native member is not UTF-8"))?;
    let relations = declarations.parsed_relations()?;
    let explicitly_bound = relations.is_some_and(|relations| relations.declares(&member_key));
    let skill_path = if explicitly_bound {
        None
    } else if let Some(manifest) = &declarations.manifest {
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
    let source = if let Some(manifest) = &declarations.manifest {
        source_horizon::project_binding_for_parsed_path(
            manifest,
            member,
            relations,
            skill.as_ref(),
            true,
        )?
    } else {
        source_horizon::control_binding_for_parsed_path(member, relations, skill.as_ref(), true)?
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
        // A folder scope's Sources are its admitted media, which its candidates already observed.
        if scope.is_folder() {
            continue;
        }
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
    if scope.is_folder() {
        return folder_entries(scope);
    }
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
    // NOW records are retrievable by placement: live, shallow, scratch and
    // evidence excluded, never part of the change horizon. A registered
    // relation for the same path keeps its own identity.
    for binding in source_horizon::now_record_bindings(&scope.root, &scope.world)? {
        sources.entry(binding.source_ref.clone()).or_insert(binding);
    }
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
        source_horizon::insert_pool_bindings(
            &scope.root,
            &scope.world,
            &ground.content_pool.exclude,
            &mut sources,
        )?;
    }
    finish_entries(scope, &ground, sources)
}

/// The NOW records of one scope as roster entries, computed from the filesystem
/// right now. They change constantly, so they are never served from the
/// refresh-time snapshot: a reader overlays these on whatever the snapshot holds.
pub(crate) fn now_entries(scope: &Scope) -> io::Result<Vec<Entry>> {
    if scope.is_folder() {
        return Ok(Vec::new());
    }
    let ground = scope.ground()?;
    let sources = source_horizon::now_record_bindings(&scope.root, &scope.world)?
        .into_iter()
        .map(|binding| (binding.source_ref.clone(), binding))
        .collect();
    finish_entries(scope, &ground, sources)
}

/// True for an entry that is a live-only NOW record (never part of a snapshot).
pub(crate) fn is_now_record_entry(entry: &Entry) -> bool {
    entry.source.roles.iter().any(|role| role == "now-record")
}

fn finish_entries(
    scope: &Scope,
    ground: &MapGround,
    sources: BTreeMap<String, SourceBinding>,
) -> io::Result<Vec<Entry>> {
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
            size: None,
            mtime: None,
        });
    }
    Ok(result)
}

/// A folder scope is never enumerated whole: its entries are listed page by page, through `inspect`, and one path of it is
/// resolved where it stands ([`folder_member`]).
fn folder_entries(scope: &Scope) -> io::Result<Vec<Entry>> {
    Err(invalid(format!(
        "{} is a folder scope: Central lists it page by page and resolves one path of it where it stands, never the whole folder at once",
        scope.project.as_deref().unwrap_or(&scope.world)
    )))
}
/// One page of a folder scope's listing, as far as its walk went.
#[derive(Debug, Default)]
pub(crate) struct FolderPage {
    /// The entries the page keeps, in path order.
    pub found: Vec<Entry>,
    /// The last position the walk finished past the page's start: an entry it read, or a folder it walked whole or left out. A page
    /// that starts after it misses nothing and repeats nothing.
    pub finished: Option<PathBuf>,
    /// The bound that stopped the walk before the page filled, in words.
    pub bound: Option<String>,
    /// How many folders and entries the walk skipped (one it could not read, one gone while it read it, another volume), and the
    /// first [`FOLDER_SKIPPED_NAMED`] of them, each with why.
    pub skipped: usize,
    pub skipped_named: Vec<(PathBuf, String)>,
}
/// What a page of a folder scope asks of its walk.
pub(crate) struct FolderRead {
    /// The position the page starts after (the last entry of the page before, or where a bound stopped it).
    pub after: Option<PathBuf>,
    /// How many kept entries fill the page: a reader asks for one more than its limit, to know whether more stand.
    pub want: usize,
    /// When there are any, the walk enters only the folders on the way to one of these, or under one.
    pub prefixes: Vec<PathBuf>,
    /// Which entries the page keeps (the reader's prefixes and its policy), applied as the walk goes, so a page fills with entries
    /// that match.
    pub keep: Box<dyn Fn(&Entry) -> bool + Send>,
}
/// Whether the position `position` lies past `after`: after it in path order, or a folder walked whole that holds it.
pub(crate) fn beyond(position: &Path, after: &Path) -> bool {
    position > after || position != after && after.starts_with(position)
}
#[derive(Default)]
struct FolderProgress {
    page: FolderPage,
    /// The reader has gone with what was listed by its time bound: the walk records nothing more.
    left: bool,
}
struct FolderWalk {
    scope: Scope,
    rules: FolderRules,
    read: FolderRead,
    /// Folders read, and the most a page's walk reads.
    directories: usize,
    most: usize,
    /// The walk's time bound, and when it passes: past it the walk reads nothing more.
    time: std::time::Duration,
    deadline: std::cell::Cell<std::time::Instant>,
    progress: std::sync::Arc<std::sync::Mutex<FolderProgress>>,
    #[cfg(test)]
    seams: FolderWalkSeams,
    /// Where a walk parked by a test's stall says so, as a read that never answers leaves its reader at the bound.
    #[cfg(test)]
    parked: std::sync::mpsc::Sender<Walked>,
}
/// What a page's walk hands its reader.
enum Walked {
    /// The walk ended: the page as far as it went, or the folder's own failure to be read.
    Done(io::Result<()>),
    /// The walk is parked in a read that does not answer (a test's stall): the reader goes with what it has, at the time bound.
    #[cfg(test)]
    Parked,
}
/// The time bound's words for a walk bounded at `time`.
fn time_bound_words(time: std::time::Duration) -> String {
    format!(
        "the time bound stopped the walk: {}s passed before the page filled",
        time.as_secs_f64()
    )
}
impl FolderWalk {
    fn progress(&self) -> std::sync::MutexGuard<'_, FolderProgress> {
        self.progress
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
    fn left(&self) -> bool {
        self.progress().left
    }
    /// Whether the walk's deadline has passed, before reading a folder (`folder`: its listing, or its metadata or bundle probe on the
    /// way into it) or a file. Under a test's logical clock it passes once that many folders are read, and only a folder's reading
    /// is held to it.
    fn past(&self, folder: bool) -> bool {
        #[cfg(test)]
        if let Some(after) = self.seams.deadline_after {
            return folder && self.directories >= after;
        }
        let _ = folder;
        std::time::Instant::now() >= self.deadline.get()
    }
    /// Stop at the deadline: the page holds what the walk listed by then.
    fn stop_at_deadline(&self) -> bool {
        self.bound(time_bound_words(self.time));
        false
    }
    /// The walk is past `path`. A name that is not UTF-8 is never a position: no page starts after it.
    fn finish(&self, path: &Path) {
        if path.to_str().is_none() {
            return;
        }
        let mut progress = self.progress();
        if !progress.left {
            progress.page.finished = Some(path.to_path_buf());
        }
    }
    fn skip(&self, path: &Path, why: String) {
        let mut progress = self.progress();
        if progress.left {
            return;
        }
        progress.page.skipped += 1;
        if progress.page.skipped_named.len() < FOLDER_SKIPPED_NAMED {
            progress.page.skipped_named.push((path.to_path_buf(), why));
        }
    }
    fn bound(&self, words: String) {
        let mut progress = self.progress();
        if !progress.left {
            progress.page.bound = Some(words);
        }
    }
    /// Keep `entry` if the page does, at `path`: false once the page is full.
    fn found(&self, path: &Path, entry: Entry) -> bool {
        let keep = (self.read.keep)(&entry);
        let mut progress = self.progress();
        if progress.left {
            return false;
        }
        if keep {
            progress.page.found.push(entry);
        }
        progress.page.finished = Some(path.to_path_buf());
        progress.page.found.len() < self.read.want
    }
    /// Whether the walk goes into `dir`, by the reader's prefixes: only on the way to one, or under one.
    fn on_the_way(&self, dir: &Path) -> bool {
        self.read.prefixes.is_empty()
            || self
                .read
                .prefixes
                .iter()
                .any(|prefix| prefix.starts_with(dir) || dir.starts_with(prefix))
    }
    #[cfg(test)]
    fn before_reading(&self, path: &Path) {
        if let Some(read) = &self.seams.read {
            read.lock().unwrap().push(path.to_path_buf());
        }
        if self.seams.vanish.as_deref() == Some(path) {
            let _ = fs::remove_dir_all(path).or_else(|_| fs::remove_file(path));
        }
        if self.seams.pass_deadline_at.as_deref() == Some(path) {
            self.deadline.set(std::time::Instant::now());
        }
        if let Some((_, fifo)) = self.seams.stall.as_ref().filter(|(at, _)| at == path) {
            if !self.seams.silent {
                let _ = self.parked.send(Walked::Parked);
            }
            let _ = fs::File::open(fifo);
        }
    }
    /// The `lstat` of `path`. Under a test's seam, a row handed under a name that is not UTF-8 is read where it stands.
    fn lstat(&self, path: &Path) -> io::Result<fs::Metadata> {
        #[cfg(test)]
        {
            use std::os::unix::ffi::OsStrExt;
            if let Some((at, _)) = self.seams.not_utf8.iter().find(|(at, bytes)| {
                at.parent()
                    .is_some_and(|parent| parent.join(std::ffi::OsStr::from_bytes(bytes)) == path)
            }) {
                return fs::symlink_metadata(at);
            }
        }
        fs::symlink_metadata(path)
    }
    /// Walk `dir` (`depth` folders under the root). Only the root's own failure to be read fails the page; every other folder or
    /// entry that cannot be read is skipped and named. `Ok(false)` once the walk stops: the page is full, a bound stopped it or the
    /// reader has gone.
    fn dir(
        &mut self,
        scan: &mut source_horizon::ScanMemo<'_>,
        dir: &Path,
        depth: usize,
    ) -> io::Result<bool> {
        if self.past(true) {
            return Ok(self.stop_at_deadline());
        }
        if self.directories >= self.most {
            self.bound(format!(
                "the directory bound stopped the walk: {} folders were read before the page filled",
                self.most
            ));
            return Ok(false);
        }
        #[cfg(test)]
        self.before_reading(dir);
        if self.left() {
            return Ok(false);
        }
        self.directories += 1;
        #[cfg(test)]
        if let Some(listed) = &self.seams.listed {
            listed.lock().unwrap().push(dir.to_path_buf());
        }
        let listing = match fs::read_dir(dir) {
            Ok(listing) => listing,
            Err(error) if depth == 0 => return Err(error),
            Err(error) => {
                self.skip(dir, format!("it could not be read: {error}"));
                return Ok(true);
            }
        };
        let mut rows = Vec::new();
        for row in listing {
            if self.left() {
                return Ok(false);
            }
            match row {
                Ok(row) => rows.push((row.file_name(), row.path(), row.file_type())),
                Err(error) => self.skip(dir, format!("an entry in it could not be read: {error}")),
            }
        }
        #[cfg(test)]
        for (at, bytes) in &self.seams.not_utf8 {
            use std::os::unix::ffi::OsStringExt;
            for row in rows.iter_mut().filter(|row| row.1 == *at) {
                row.0 = std::ffi::OsString::from_vec(bytes.clone());
                row.1 = dir.join(&row.0);
            }
        }
        rows.sort_by(|a, b| a.0.cmp(&b.0));
        let after = self.read.after.clone();
        for (name, path, kind) in rows {
            if self.left() {
                return Ok(false);
            }
            // A Source is named by its path, and a name that is not UTF-8 cannot be one. A folder of such a name the walk would enter,
            // or a media file it would list, is skipped and named (lossily) by the page whose start lies before it, and the rest of
            // the folder answers; anything else of such a name is passed by as it would be under any name.
            let utf8 = path.to_str().is_some();
            let name = name.to_string_lossy();
            let kind = match kind {
                Ok(kind) => kind,
                Err(error) => {
                    self.skip(&path, format!("it could not be read: {error}"));
                    continue;
                }
            };
            if kind.is_dir() {
                // A folder at or wholly before the page's start holds nothing the page lists.
                if after.as_deref().is_some_and(|after| {
                    path == after || path.as_path() < after && !after.starts_with(&path)
                }) {
                    continue;
                }
                // Past the deadline, not even a folder's metadata is read, nor its bundle probe.
                if self.past(true) {
                    return Ok(self.stop_at_deadline());
                }
                #[cfg(test)]
                self.before_reading(&path);
                let metadata = match self.lstat(&path) {
                    Ok(metadata) => metadata,
                    Err(error) if error.kind() == io::ErrorKind::NotFound => {
                        self.skip(&path, "it was gone when it was read".into());
                        continue;
                    }
                    Err(error) => {
                        self.skip(&path, format!("it could not be read: {error}"));
                        continue;
                    }
                };
                // A read that did not answer at once may have outlasted its reader: past that, nothing more is read.
                if self.left() {
                    return Ok(false);
                }
                let walk = &*self;
                let enters = walk.rules.enters_unless(&path, &name, &metadata, || {
                    if walk.past(true) {
                        return true;
                    }
                    #[cfg(test)]
                    walk.before_reading(&path.join("Contents"));
                    false
                });
                let Some(enters) = enters else {
                    return Ok(self.stop_at_deadline());
                };
                // Nor past a bundle probe that did not answer at once (DLW build ruling 2026-11-09, 47f item 5). No read lies between
                // here and `dir`'s own check, which holds the folder's listing; this one keeps the test seam (`before_reading`) quiet
                // once the reader has left (47g item 5).
                if self.left() {
                    return Ok(false);
                }
                match enters {
                    Enters::No => {}
                    Enters::Skipped(why) => self.skip(&path, why.into()),
                    Enters::Yes if !self.on_the_way(&path) => {}
                    Enters::Yes if depth + 1 > FOLDER_DEPTH => {
                        self.skip(&path, format!("it is deeper than {FOLDER_DEPTH} folders"));
                    }
                    Enters::Yes if !utf8 => {
                        self.skip(&path, "its name is not UTF-8".into());
                        continue;
                    }
                    Enters::Yes => {
                        if !self.dir(scan, &path, depth + 1)? {
                            return Ok(false);
                        }
                    }
                }
                self.finish(&path);
                continue;
            }
            // A symbolic link, a socket or another special file is never the person's media.
            if !kind.is_file()
                || after
                    .as_deref()
                    .is_some_and(|after| path.as_path() <= after)
            {
                continue;
            }
            if name.starts_with('.') || media_kind(&self.rules.folder.media, &path).is_none() {
                self.finish(&path);
                continue;
            }
            if self.past(false) {
                return Ok(self.stop_at_deadline());
            }
            #[cfg(test)]
            self.before_reading(&path);
            let metadata = match self.lstat(&path) {
                Ok(metadata) => metadata,
                Err(error) if error.kind() == io::ErrorKind::NotFound => {
                    self.skip(&path, "it was gone when it was read".into());
                    continue;
                }
                Err(error) => {
                    self.skip(&path, format!("it could not be read: {error}"));
                    continue;
                }
            };
            let Some(media) = self.rules.lists(&path, &name, &metadata) else {
                self.finish(&path);
                continue;
            };
            if !utf8 {
                self.skip(&path, "its name is not UTF-8".into());
                continue;
            }
            if !scan.retrieval_allowed(&path) {
                self.finish(&path);
                continue;
            }
            let Ok(inside) = path.strip_prefix(&self.scope.root) else {
                continue;
            };
            let relative = source_horizon::normalize_relative(inside);
            let entry = media_entry(
                &self.scope,
                media_binding(&self.scope, &relative),
                path.clone(),
                media,
                &metadata,
            );
            if !self.found(&path, entry) {
                return Ok(false);
            }
        }
        Ok(true)
    }
}
/// One page of a folder scope's media entries, in path order (component by component, as the walk meets them): the first
/// `read.want` that the page keeps past `read.after`. The walk never enters a hidden folder, a package, a folder the scope excludes,
/// a symbolic link or another volume, leaves unopened every folder that lies at or wholly before the page's start, and stops at
/// whichever comes first: the page full, [`FOLDER_WALK_DIRECTORIES`] folders read, or [`FOLDER_WALK_TIME`] passed. It runs on a
/// helper thread, so a folder that stops answering costs the time bound and no more; the thread, left, records nothing further and
/// ends with the process. A folder or entry that cannot be read is skipped and named; only the scope's own folder unread fails the
/// page.
pub(crate) fn folder_page(scope: &Scope, read: FolderRead) -> io::Result<FolderPage> {
    let unread = |error: io::Error| {
        io::Error::new(
            error.kind(),
            format!(
                "{} is a folder scope over {}, and its folder cannot be read: {error}",
                scope.project.as_deref().unwrap_or(&scope.world),
                scope.root.display()
            ),
        )
    };
    let rules = FolderRules::of(scope).map_err(unread)?;
    #[cfg(test)]
    let seams = FOLDER_WALK_SEAMS.with(|seams| seams.borrow().clone());
    #[cfg(test)]
    let (most, time) = (
        seams.directories.unwrap_or(FOLDER_WALK_DIRECTORIES),
        seams.time.unwrap_or(FOLDER_WALK_TIME),
    );
    #[cfg(not(test))]
    let (most, time) = (FOLDER_WALK_DIRECTORIES, FOLDER_WALK_TIME);
    // One walk of a folder at a time: a read that finds one running waits for it within its own bound, and answers in words while it
    // still runs. The wait and the walk each have a bound of their own: a read that waited still walks its whole bound (DLW build
    // ruling 2026-11-09, 47f item 4).
    let walking = {
        let mut walking = folders_walking();
        if walking.contains(&scope.root) {
            #[cfg(test)]
            FOLDERS_WAITED
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .insert(scope.root.clone());
            walking = FOLDER_WALK_ENDED
                .wait_timeout_while(walking, time, |walking| walking.contains(&scope.root))
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .0;
        }
        if walking.contains(&scope.root) {
            return Ok(FolderPage {
                bound: Some(FOLDER_STILL_WALKING.into()),
                ..FolderPage::default()
            });
        }
        walking.insert(scope.root.clone());
        FolderWalking(scope.root.clone())
    };
    let began = std::time::Instant::now();
    let progress = std::sync::Arc::new(std::sync::Mutex::new(FolderProgress::default()));
    let (send, receive) = std::sync::mpsc::channel();
    let mut walk = FolderWalk {
        scope: scope.clone(),
        rules,
        read,
        directories: 0,
        most,
        time,
        deadline: std::cell::Cell::new(began + time),
        progress: progress.clone(),
        #[cfg(test)]
        seams,
        #[cfg(test)]
        parked: send.clone(),
    };
    std::thread::Builder::new()
        .name("file-map-folder-walk".into())
        .spawn(move || {
            let root = walk.scope.root.clone();
            let mut scan = source_horizon::ScanMemo::new(&root);
            let walked = walk.dir(&mut scan, &root, 0);
            drop(walking);
            let _ = send.send(Walked::Done(walked.map(|_| ())));
        })?;
    let take = |bound: Option<String>| {
        let mut progress = progress
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        progress.left = true;
        let mut page = std::mem::take(&mut progress.page);
        if bound.is_some() {
            page.bound = bound;
        }
        page
    };
    let page = match receive.recv_timeout(time.saturating_sub(began.elapsed())) {
        Ok(Walked::Done(walked)) => walked.map(|()| take(None)).map_err(unread)?,
        #[cfg(test)]
        Ok(Walked::Parked) => take(Some(time_bound_words(time))),
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => take(Some(time_bound_words(time))),
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
            return Err(io::Error::other(format!(
                "the walk of the folder scope over {} ended without an answer",
                scope.root.display()
            )))
        }
    };
    Ok(page)
}
/// The entry a folder scope lists at `member` (a path relative to its root), resolved by `lstat` of each folder on its way and of the
/// file, under the walk's own rules, never by walking: what the walk leaves out (a hidden file or folder, a package, an excluded
/// folder, a symbolic link, another volume, a file too deep) is outside the scope here too.
pub(crate) fn folder_member(scope: &Scope, member: &str) -> io::Result<Option<Entry>> {
    let Ok(inside) = relative(member) else {
        return Ok(None);
    };
    let parts: Vec<_> = inside.components().collect();
    if parts.len() > FOLDER_DEPTH + 1 {
        return Ok(None);
    }
    let rules = FolderRules::of(scope)?;
    let mut path = scope.root.clone();
    for (index, part) in parts.iter().enumerate() {
        path.push(part);
        let name = part.as_os_str().to_string_lossy();
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error)
                if error.kind() == io::ErrorKind::NotFound
                    || error.raw_os_error() == Some(libc::ENOTDIR) =>
            {
                return Ok(None)
            }
            Err(error) => return Err(error),
        };
        if index + 1 < parts.len() {
            if !matches!(rules.enters(&path, &name, &metadata), Enters::Yes) {
                return Ok(None);
            }
            continue;
        }
        let Some(media) = rules.lists(&path, &name, &metadata) else {
            return Ok(None);
        };
        if !source_horizon::ScanMemo::new(&scope.root).retrieval_allowed(&path) {
            return Ok(None);
        }
        let relative = source_horizon::normalize_relative(inside);
        return Ok(Some(media_entry(
            scope,
            media_binding(scope, &relative),
            path,
            media,
            &metadata,
        )));
    }
    Ok(None)
}
/// A folder scope's media file as a Source: its identity is its path in the folder.
fn media_binding(scope: &Scope, relative: &str) -> SourceBinding {
    SourceBinding {
        source_ref: source_horizon::source_ref(&scope.world, relative),
        path: relative.to_owned(),
        roles: vec!["folder-media".into()],
        provenance: "folder-scope".into(),
        standing: "media-metadata".into(),
        treatment: "retain-native-in-place".into(),
        agent_retrieval_allowed: true,
    }
}
fn media_entry(
    scope: &Scope,
    source: SourceBinding,
    path: PathBuf,
    kind: &str,
    metadata: &fs::Metadata,
) -> Entry {
    Entry {
        title: source.path.clone(),
        source,
        world_ref: scope.world.clone(),
        path,
        kind: kind.into(),
        revision: format!(
            "metadata:{}:{}:{}:{}:{}",
            metadata.dev(),
            metadata.ino(),
            metadata.len(),
            metadata.mtime(),
            metadata.mtime_nsec()
        ),
        tags: Vec::new(),
        native_import: false,
        size: Some(metadata.len()),
        mtime: Some(metadata.mtime()),
    }
}
/// The member a SourceRef names in `world`, when it is that world's spelling of one.
fn member_of_ref(world: &str, reference: &str) -> Option<String> {
    let prefix = format!("central:source:{world}:");
    reference
        .strip_prefix(&prefix)
        .map(|suffix| {
            suffix
                .replace("%20", " ")
                .replace("%3A", ":")
                .replace("%25", "%")
        })
        .filter(|member| source_horizon::source_ref(world, member) == reference)
}
/// A folder scope's candidate for a selected read: the admitted media file the reference or
/// location names, if it stands.
fn folder_candidates(
    scope: &Scope,
    reference: Option<&str>,
    location: Option<&Path>,
) -> io::Result<Vec<BindingCandidate>> {
    let (_, basis) = scope.folder()?;
    let member = match (location, reference) {
        (Some(location), _) => location
            .strip_prefix(&scope.root)
            .ok()
            .and_then(Path::to_str)
            .map(str::to_owned),
        (None, Some(reference)) => member_of_ref(&scope.world, reference),
        (None, None) => None,
    };
    let Some(member) = member.filter(|member| relative(member).is_ok()) else {
        return Ok(Vec::new());
    };
    // Resolved where it stands, under the walk's rules: what a page never lists is outside the scope here too.
    let Some(entry) = folder_member(scope, &member)? else {
        return Ok(Vec::new());
    };
    let source = entry.source;
    if reference.is_some_and(|reference| reference != source.source_ref) {
        return Ok(Vec::new());
    }
    Ok(vec![BindingCandidate {
        scope: scope.clone(),
        source,
        resource: None,
        relation_revision: scope.basis()?,
        selection_metadata_basis: Some(basis),
    }])
}
pub(crate) fn lookup(all: &[Scope], reference: &str) -> io::Result<(Scope, Entry)> {
    for scope in all {
        // A folder scope's Source is resolved where it stands, never by walking the folder.
        if scope.is_folder() {
            if let Some(entry) = member_of_ref(&scope.world, reference)
                .map(|member| folder_member(scope, &member))
                .transpose()?
                .flatten()
                .filter(|entry| entry.source.source_ref == reference)
            {
                return Ok((scope.clone(), entry));
            }
            continue;
        }
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
/// Why an entry that is not a file has no content to give.
fn no_payload(entry: &Entry) -> io::Error {
    if entry.kind == "directory" {
        invalid("A directory has no byte payload")
    } else {
        invalid(format!(
            "{} is a media entry ({}), and a media entry is not text: Central lists it by its \
             path, size and time and reads none of its content",
            entry.source.path, entry.kind
        ))
    }
}
pub(crate) fn payload(entry: &Entry) -> io::Result<Vec<u8>> {
    if entry.kind != "file" {
        return Err(no_payload(entry));
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
/// What the index holds of one source's text, and what it does not.
pub(crate) struct Extent {
    /// Bytes of text written into the description.
    pub indexed: u64,
    /// The file's whole length, when it could be observed.
    pub total: Option<u64>,
    /// Why no text was read, when none was. A source that is not text is the
    /// ordinary case and says so; anything else is a fault to report.
    pub gap: Option<String>,
}
impl Extent {
    pub(crate) fn is_not_text(&self) -> bool {
        self.gap
            .as_deref()
            .is_some_and(|gap| gap.contains("not text"))
    }
}

/// The start of a source's text, at most `max` bytes, cut at a character
/// boundary, with the file's whole length. Retrieval admission is checked on
/// both sides of the read, as `payload` does; unlike `payload` the file is not
/// read past `max`, so a source of any size has a start to index.
pub(crate) fn text_prefix(entry: &Entry, max: usize) -> io::Result<(String, u64)> {
    if entry.kind != "file" {
        return Err(no_payload(entry));
    }
    let allowed = || source_horizon::retrieval_admission(Path::new("/"), &entry.path);
    let revoked = || {
        io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Source retrieval has been revoked",
        )
    };
    if !allowed()? {
        return Err(revoked());
    }
    let root = ReadRoot::capture(Path::new("/"))?;
    let member = entry.path.strip_prefix("/").map_err(io::Error::other)?;
    let mut reader =
        crate::file_mutation::NativeFileRead::open(&root.canonical, root.identity, member)?;
    let (bytes, total) = reader.read_prefix(max)?;
    root.validate()?;
    if !allowed()? {
        return Err(revoked());
    }
    if bytes.contains(&0) {
        return Err(invalid("Source is not text"));
    }
    let valid = match std::str::from_utf8(&bytes) {
        Ok(text) => text.len(),
        // A character split by the cut is the cut's doing, not the file's.
        Err(error) if error.error_len().is_none() && (bytes.len() as u64) < total => {
            error.valid_up_to()
        }
        Err(_) => return Err(invalid("Source is not UTF-8 text")),
    };
    let text = String::from_utf8(bytes[..valid].to_vec()).map_err(io::Error::other)?;
    Ok((text, total))
}

/// The description bkmr is given for a source, and the extent of the text in it.
pub(crate) fn describe(entry: &Entry) -> (String, Extent) {
    let marker = marker(&entry.source.source_ref);
    match text_prefix(entry, INDEXED_TEXT_BYTES) {
        Ok((text, total)) => {
            let extent = Extent {
                indexed: text.len() as u64,
                total: Some(total),
                gap: None,
            };
            (format!("{marker}{text}"), extent)
        }
        Err(error) => (
            marker,
            Extent {
                indexed: 0,
                total: None,
                gap: Some(error.to_string()),
            },
        ),
    }
}
pub(crate) fn description(entry: &Entry) -> String {
    describe(entry).0
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
    if scope.world != "control:root" && !scope.is_folder() {
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
