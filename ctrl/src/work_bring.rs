//! Folders of work that stand outside the ground, and bringing one into Work (O-I #738; manual §10.11, Setup's Projects step).
//!
//! Central keeps every project at `Work/<Name>`: the world's shape says so (`Control/agents/governance/world/shape-of-the-world.md`),
//! every project action resolves `<root>/Work/<project>`, Work discovery lists real directories only, a path that leads out of
//! Work through a link is refused (`work.open`), and a source's identity is its ground-relative path. A folder of work elsewhere
//! under the person's home therefore comes into the ground by copy, never by a reference to where it stands:
//!
//! - `work.candidates` (read only) finds the folders of work under the home: git repositories, folders with their own agent
//!   instructions (AGENTS.md, CLAUDE.md), an OKF wiki (`wiki.json`) and Obsidian vaults. It never enters the ground, a hidden
//!   folder (the products' homes among them), `Library` or `node_modules`, never follows a link, and goes four folders deep.
//! - `work.bring.preview` (read only) lists the copy and its digest; `work.bring` makes it on that digest: the folder with its
//!   `.git`, file modes and modification times, copied beside Work and moved in once it reads back equal to the original.
//!   The original stays. A link that would lead out of the copy, a folder that shares another repository (a git worktree, borrowed
//!   objects), a folder in or over the ground, and a name Work already holds are refused.
use crate::action::{
    ActionAvailability, ActionDescriptor, ActionExecutionContext, ActionInputDefinition,
    ActionOutputDefinition, ActionRegistry, MutationClass,
};
use crate::projectcentral_ops::compatible_wiki;
use crate::result::{ActionResult, ResultStatus};
use crate::root::resolve_central_root;
use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::ffi::CString;
use std::fs;
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{symlink, MetadataExt, PermissionsExt};
use std::path::{Component, Path, PathBuf};

const CANDIDATES: &str = "work.candidates";
const PREVIEW: &str = "work.bring.preview";
const BRING: &str = "work.bring";

/// How many folders below the home a folder of work may stand (`~/Documents/GitHub/org/repo` is four).
pub const CANDIDATE_DEPTH: usize = 4;
/// How many folders one reading enters before it stops and says so.
pub const CANDIDATE_VISITS: usize = 20_000;
/// Folders a reading never enters, beside every hidden one: the system's own and dependency trees.
const NEVER_ENTERED: [&str; 2] = ["Library", "node_modules"];

#[derive(Debug, Clone, Serialize)]
struct Candidate {
    name: String,
    path: PathBuf,
    relative: String,
    signals: Vec<&'static str>,
}

/// What marks a folder as a folder of work, in this order: a git repository, its own agent instructions, an OKF wiki, a vault.
fn signals_of(folder: &Path) -> Vec<&'static str> {
    let mut signals = Vec::new();
    if fs::symlink_metadata(folder.join(".git")).is_ok() {
        signals.push("git");
    }
    if folder.join("AGENTS.md").is_file() {
        signals.push("agents");
    }
    if folder.join("CLAUDE.md").is_file() {
        signals.push("claude");
    }
    if matches!(compatible_wiki(&folder.join("wiki.json")), Ok(Some(_))) {
        signals.push("wiki");
    }
    if folder.join(".obsidian").is_dir() {
        signals.push("obsidian");
    }
    signals
}

struct Scan<'a> {
    home: &'a Path,
    ground: &'a Path,
    visits: usize,
    truncated: bool,
    found: Vec<Candidate>,
}

impl Scan<'_> {
    fn enter(&mut self, folder: &Path, depth: usize) {
        let Ok(entries) = fs::read_dir(folder) else {
            return;
        };
        let mut children: Vec<(String, PathBuf)> = entries
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
            .map(|entry| {
                (
                    entry.file_name().to_string_lossy().into_owned(),
                    entry.path(),
                )
            })
            .filter(|(name, _)| !name.starts_with('.') && !NEVER_ENTERED.contains(&name.as_str()))
            .collect();
        children.sort();
        for (name, path) in children {
            if path == self.ground {
                continue;
            }
            if self.visits >= CANDIDATE_VISITS {
                self.truncated = true;
                return;
            }
            self.visits += 1;
            let signals = signals_of(&path);
            if !signals.is_empty() {
                let relative = path
                    .strip_prefix(self.home)
                    .map(|relative| relative.to_string_lossy().into_owned())
                    .unwrap_or_default();
                self.found.push(Candidate {
                    name,
                    path,
                    relative,
                    signals,
                });
            } else if depth < CANDIDATE_DEPTH {
                self.enter(&path, depth + 1);
            }
        }
    }
}

fn canonical_ground(
    context: &ActionExecutionContext<'_>,
    action: &str,
) -> Result<PathBuf, ActionResult> {
    let root = resolve_central_root(context.root_options)
        .map_err(|message| {
            ActionResult::failure(Some(action), ResultStatus::InvalidInput, message, None)
        })?
        .path;
    Ok(root.canonicalize().unwrap_or(root))
}

fn candidates_action(
    _registry: &ActionRegistry,
    _input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let ground = match canonical_ground(context, CANDIDATES) {
        Ok(ground) => ground,
        Err(result) => return result,
    };
    let Some(home) = context
        .root_options
        .home
        .as_deref()
        .and_then(|home| home.canonicalize().ok())
    else {
        return ActionResult::failure(
            Some(CANDIDATES),
            ResultStatus::InvalidInput,
            "work.candidates reads the folders of work under your home, and no home is known (HOME is unset or missing).",
            None,
        );
    };
    let mut scan = Scan {
        home: &home,
        ground: &ground,
        visits: 0,
        truncated: false,
        found: Vec::new(),
    };
    // A home inside the ground holds nothing that stands outside it.
    if !home.starts_with(&ground) {
        scan.enter(&home, 1);
    }
    let (visits, truncated, mut items) = (scan.visits, scan.truncated, scan.found);
    items.sort_by(|left, right| left.relative.cmp(&right.relative));
    ActionResult::success(
        CANDIDATES,
        json!({
            "home": home,
            "ground": ground,
            "max_depth": CANDIDATE_DEPTH,
            "visited": visits,
            "truncated": truncated,
            "items": items,
        }),
    )
}

// ------------------------------------------------------------------------------------------------------------------ the copy

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum Kind {
    Folder,
    File,
    Link,
}

/// One entry of the folder as the copy must reproduce it: its kind, ground-relative path, bytes, mode and modification time
/// (files and folders), and a link's target.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct Entry {
    path: String,
    kind: Kind,
    #[serde(skip_serializing_if = "Option::is_none")]
    bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    mode: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    modified: Option<(i64, i64)>,
    #[serde(skip_serializing_if = "Option::is_none")]
    link: Option<String>,
}

fn refusal(message: String) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}

/// Where a relative link at `entry` (folder-relative) leads, folder-relative, or None when it leads out of the folder.
fn link_within(entry: &Path, target: &Path) -> Option<PathBuf> {
    if target.is_absolute() {
        return None;
    }
    let mut resolved: Vec<std::ffi::OsString> = entry
        .parent()
        .map(|parent| parent.iter().map(ToOwned::to_owned).collect())
        .unwrap_or_default();
    for component in target.components() {
        match component {
            Component::Normal(part) => resolved.push(part.to_owned()),
            Component::CurDir => {}
            Component::ParentDir => {
                resolved.pop()?;
            }
            Component::RootDir | Component::Prefix(_) => return None,
        }
    }
    Some(resolved.iter().collect())
}

/// Refuses a file through which the copy would share another repository rather than carry its own: a `.git` file naming a
/// repository outside the folder (a git worktree, or a submodule of another repository), or a repository's borrowed objects
/// (`objects/info/alternates`). A submodule whose repository lies inside the folder comes with it.
fn shares_no_repository(relative: &Path, path: &Path, meta: &fs::Metadata) -> io::Result<()> {
    let shown = relative.to_string_lossy();
    let in_git = relative.iter().any(|part| part == ".git");
    if in_git && relative.ends_with("objects/info/alternates") && meta.len() > 0 {
        return Err(refusal(format!(
            "{shown} borrows its objects from another store: its copy shares another repository's history rather than carrying its own. Nothing was copied."
        )));
    }
    if relative.file_name().is_some_and(|name| name == ".git") {
        let pointer = fs::read_to_string(path).unwrap_or_default();
        let gitdir = pointer
            .lines()
            .find_map(|line| line.strip_prefix("gitdir:"))
            .map(str::trim)
            .unwrap_or_default();
        if gitdir.is_empty() || link_within(relative, Path::new(gitdir)).is_none() {
            return Err(refusal(format!(
                "{shown} names the repository {gitdir}, outside the folder (a git worktree, or a submodule of another repository): its copy shares another repository rather than carrying its own. Bring that repository in instead. Nothing was copied."
            )));
        }
    }
    Ok(())
}

/// Every entry below `root`, sorted by path, without following a link. Refuses what the copy cannot reproduce faithfully.
fn listing(root: &Path) -> io::Result<Vec<Entry>> {
    fn walk(root: &Path, folder: &Path, out: &mut Vec<Entry>) -> io::Result<()> {
        let mut entries: Vec<_> = fs::read_dir(folder)?.collect::<Result<_, _>>()?;
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let path = entry.path();
            let relative = path
                .strip_prefix(root)
                .expect("an entry stands below its root");
            let shown = relative.to_string_lossy().into_owned();
            let meta = fs::symlink_metadata(&path)?;
            let kind = meta.file_type();
            if kind.is_symlink() {
                let target = fs::read_link(&path)?;
                if link_within(relative, &target).is_none() {
                    return Err(refusal(format!(
                        "{shown} links to {}, which leads out of the folder: its copy would lead out of your ground. Nothing was copied.",
                        target.display()
                    )));
                }
                out.push(Entry {
                    path: shown,
                    kind: Kind::Link,
                    bytes: None,
                    mode: None,
                    modified: None,
                    link: Some(target.to_string_lossy().into_owned()),
                });
            } else if kind.is_dir() {
                out.push(Entry {
                    path: shown,
                    kind: Kind::Folder,
                    bytes: None,
                    mode: Some(meta.mode() & 0o7777),
                    modified: Some((meta.mtime(), meta.mtime_nsec())),
                    link: None,
                });
                walk(root, &path, out)?;
            } else if kind.is_file() {
                shares_no_repository(relative, &path, &meta)?;
                out.push(Entry {
                    path: shown,
                    kind: Kind::File,
                    bytes: Some(meta.len()),
                    mode: Some(meta.mode() & 0o7777),
                    modified: Some((meta.mtime(), meta.mtime_nsec())),
                    link: None,
                });
            } else {
                return Err(refusal(format!(
                    "{shown} is neither a file, a folder nor a link (a socket, pipe or device): Central copies none of it. Nothing was copied."
                )));
            }
        }
        Ok(())
    }
    let mut out = Vec::new();
    walk(root, root, &mut out)?;
    Ok(out)
}

#[derive(Debug, Clone, Serialize)]
struct BringPlan {
    source_path: PathBuf,
    name: String,
    target: PathBuf,
    target_ref: String,
    git: bool,
    entries: Value,
    operations: Vec<String>,
    digest: String,
    #[serde(skip)]
    mode: u32,
    #[serde(skip)]
    listing: Vec<Entry>,
    #[serde(skip)]
    work: PathBuf,
}

fn work_name(raw: &str) -> io::Result<String> {
    let path = Path::new(raw);
    let single = matches!(
        path.components().collect::<Vec<_>>().as_slice(),
        [Component::Normal(_)]
    );
    if raw.trim() != raw || raw.is_empty() || raw.starts_with('.') || !single || raw.contains('/') {
        return Err(refusal(format!(
            "{raw:?} is not a name a folder of Work can take: one name, without `/`, not hidden."
        )));
    }
    Ok(raw.to_owned())
}

fn plan(
    input: &Value,
    context: &ActionExecutionContext<'_>,
    action: &str,
) -> Result<BringPlan, ActionResult> {
    let invalid = |error: io::Error| {
        let status = if error.kind() == io::ErrorKind::InvalidInput {
            ResultStatus::InvalidInput
        } else {
            ResultStatus::InternalFailure
        };
        ActionResult::failure(Some(action), status, error.to_string(), None)
    };
    let raw = input
        .get("source_path")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|raw| !raw.is_empty())
        .ok_or_else(|| invalid(refusal(format!("{action} requires source_path."))))?;
    let source = Path::new(raw);
    if !source.is_absolute() {
        return Err(invalid(refusal(format!(
            "{raw} is not a full path: name the folder by its full path."
        ))));
    }
    let meta = fs::symlink_metadata(source)
        .map_err(|_| invalid(refusal(format!("Nothing stands at {raw}."))))?;
    if meta.file_type().is_symlink() {
        return Err(invalid(refusal(format!(
            "{raw} is a link: name the folder it leads to."
        ))));
    }
    if !meta.is_dir() {
        return Err(invalid(refusal(format!("{raw} is not a folder."))));
    }
    let source = source.canonicalize().map_err(invalid)?;
    let ground = canonical_ground(context, action)?;
    if source.starts_with(&ground) {
        return Err(invalid(refusal(format!(
            "{} stands in your ground already: adopt it where it stands in Work.",
            source.display()
        ))));
    }
    if ground.starts_with(&source) {
        return Err(invalid(refusal(format!(
            "{} holds your ground: bring in a folder of work inside it instead.",
            source.display()
        ))));
    }
    let work = ground.join("Work");
    if !work.is_dir() {
        return Err(ActionResult::failure(
            Some(action),
            ResultStatus::InvalidCentralStructure,
            format!(
                "Your ground has no Work folder at {}: make your ground first.",
                work.display()
            ),
            None,
        ));
    }
    let name = match input.get("name").and_then(Value::as_str) {
        Some(name) => work_name(name).map_err(invalid)?,
        None => {
            work_name(&source.file_name().unwrap_or_default().to_string_lossy()).map_err(invalid)?
        }
    };
    let target = work.join(&name);
    let target_ref = format!("Work/{name}");
    if fs::symlink_metadata(&target).is_ok() {
        return Err(invalid(refusal(format!(
            "{target_ref} is there already: bring the folder in under another name, or adopt {target_ref} where it stands."
        ))));
    }
    let listing = listing(&source).map_err(invalid)?;
    let count = |kind: Kind| listing.iter().filter(|entry| entry.kind == kind).count();
    let (files, folders, links) = (count(Kind::File), count(Kind::Folder), count(Kind::Link));
    let bytes: u64 = listing.iter().filter_map(|entry| entry.bytes).sum();
    let git = fs::symlink_metadata(source.join(".git")).is_ok();
    let mode = meta.mode() & 0o7777;
    let mut digest = Sha256::new();
    digest.update(
        serde_json::to_vec(&json!({
            "schema": "central.work-bring-plan/v1",
            "source_path": source,
            "target": target,
            "mode": mode,
            "listing": listing,
        }))
        .expect("the plan serializes"),
    );
    let digest = digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let mut operations = vec![format!(
        "Copy {} into your ground's Work as {target_ref}: {files} files, {folders} folders and {links} links, {bytes} bytes, each file's mode and modification time kept.",
        source.display()
    )];
    if git {
        operations.push("Its git repository (.git) comes with it: the history, the branches and the working tree as they stand.".to_owned());
    }
    operations.push(format!(
        "Central reads the copy back against the folder before {target_ref} stands in Work."
    ));
    operations.push(format!(
        "The original stays at {}; nothing in it moves or changes.",
        source.display()
    ));
    Ok(BringPlan {
        source_path: source,
        name,
        target,
        target_ref,
        git,
        entries: json!({"files": files, "folders": folders, "links": links, "bytes": bytes}),
        operations,
        digest,
        mode,
        listing,
        work,
    })
}

fn preview_action(
    _registry: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    match plan(input, context, PREVIEW) {
        Ok(plan) => ActionResult::success(
            PREVIEW,
            serde_json::to_value(plan).expect("the plan serializes"),
        ),
        Err(result) => result,
    }
}

/// Sets a path's access and modification times, the link itself when it is one.
fn set_times(path: &Path, accessed: (i64, i64), modified: (i64, i64)) -> io::Result<()> {
    let name = CString::new(path.as_os_str().as_bytes())
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
    let times = [
        libc::timespec {
            tv_sec: accessed.0 as libc::time_t,
            tv_nsec: accessed.1 as _,
        },
        libc::timespec {
            tv_sec: modified.0 as libc::time_t,
            tv_nsec: modified.1 as _,
        },
    ];
    // SAFETY: `name` is a valid NUL-terminated path and `times` holds the two timespecs utimensat reads.
    if unsafe {
        libc::utimensat(
            libc::AT_FDCWD,
            name.as_ptr(),
            times.as_ptr(),
            libc::AT_SYMLINK_NOFOLLOW,
        )
    } == 0
    {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

/// Copies the planned entries from `source` into `staging` (an empty folder Central made), as the listing names them.
fn copy_into(plan: &BringPlan, staging: &Path) -> io::Result<()> {
    for entry in &plan.listing {
        let from = plan.source_path.join(&entry.path);
        let to = staging.join(&entry.path);
        match entry.kind {
            Kind::Folder => fs::create_dir(&to)?,
            Kind::File => {
                fs::copy(&from, &to)?;
                fs::set_permissions(&to, fs::Permissions::from_mode(entry.mode.unwrap_or(0o644)))?;
                let meta = fs::symlink_metadata(&from)?;
                set_times(
                    &to,
                    (meta.atime(), meta.atime_nsec()),
                    entry.modified.unwrap_or_default(),
                )?;
            }
            Kind::Link => symlink(entry.link.as_deref().unwrap_or_default(), &to)?,
        }
    }
    // Folders last, deepest first: their modes may close them to writing, and filling them moved their times.
    for entry in plan
        .listing
        .iter()
        .rev()
        .filter(|entry| entry.kind == Kind::Folder)
    {
        let to = staging.join(&entry.path);
        fs::set_permissions(&to, fs::Permissions::from_mode(entry.mode.unwrap_or(0o755)))?;
        let modified = entry.modified.unwrap_or_default();
        set_times(&to, modified, modified)?;
    }
    fs::set_permissions(staging, fs::Permissions::from_mode(plan.mode))
}

/// Removes a staging copy Central made, opening its folders to writing first.
fn remove_staging(staging: &Path) {
    fn open(folder: &Path) {
        let _ = fs::set_permissions(folder, fs::Permissions::from_mode(0o700));
        if let Ok(entries) = fs::read_dir(folder) {
            for entry in entries.flatten() {
                if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                    open(&entry.path());
                }
            }
        }
    }
    open(staging);
    let _ = fs::remove_dir_all(staging);
}

fn bring_action(
    _registry: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let expected = input
        .get("expected_digest")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let plan = match plan(input, context, BRING) {
        Ok(plan) => plan,
        Err(result) => return result,
    };
    if plan.digest != expected {
        return ActionResult::failure(
            Some(BRING),
            ResultStatus::VerificationFailure,
            format!(
                "{} changed since its preview (or no preview was named): preview it again. Nothing was copied.",
                plan.source_path.display()
            ),
            Some(json!({"expected_digest": expected, "digest": plan.digest})),
        );
    }
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos())
        .unwrap_or_default();
    let staging = plan.work.join(format!(
        ".{}.bringing-{}-{nonce}",
        plan.name,
        std::process::id()
    ));
    if let Err(error) = fs::create_dir(&staging) {
        return ActionResult::failure(
            Some(BRING),
            ResultStatus::InternalFailure,
            format!(
                "Central could not make its copy beside Work ({}): {error}. Nothing was copied.",
                staging.display()
            ),
            None,
        );
    }
    let fail = |status: ResultStatus, message: String| {
        remove_staging(&staging);
        ActionResult::failure(Some(BRING), status, message, None)
    };
    if let Err(error) = copy_into(&plan, &staging) {
        return fail(
            ResultStatus::InternalFailure,
            format!(
                "Copying {} failed: {error}. Nothing stands in Work; the original is as it was.",
                plan.source_path.display()
            ),
        );
    }
    match listing(&staging) {
        Ok(copied) if copied == plan.listing => {}
        Ok(_) => {
            return fail(
                ResultStatus::VerificationFailure,
                format!(
                    "{} changed while it was copied: the copy does not read back equal to it. Nothing stands in Work; preview it again.",
                    plan.source_path.display()
                ),
            )
        }
        Err(error) => return fail(ResultStatus::InternalFailure, format!("Central could not read its copy back: {error}. Nothing stands in Work.")),
    }
    // Nothing may have taken the name meanwhile: a rename never replaces what stands there.
    if fs::symlink_metadata(&plan.target).is_ok() {
        return fail(
            ResultStatus::InvalidInput,
            format!("{} appeared while the folder was copied. Nothing was replaced; the copy was removed.", plan.target_ref),
        );
    }
    if let Err(error) = fs::rename(&staging, &plan.target) {
        return fail(
            ResultStatus::InternalFailure,
            format!(
                "Central could not move its copy into {}: {error}. Nothing stands there.",
                plan.target_ref
            ),
        );
    }
    let verified = listing(&plan.target).is_ok_and(|copied| copied == plan.listing);
    if !verified {
        return ActionResult::failure(
            Some(BRING),
            ResultStatus::VerificationFailure,
            format!(
                "{} stands in Work but does not read back equal to {}: compare them before you rely on it.",
                plan.target_ref,
                plan.source_path.display()
            ),
            Some(json!({"target": plan.target})),
        );
    }
    ActionResult::success(
        BRING,
        json!({
            "source_path": plan.source_path,
            "name": plan.name,
            "target": plan.target,
            "target_ref": plan.target_ref,
            "git": plan.git,
            "entries": plan.entries,
            "digest": plan.digest,
            "verified": true,
            "original": "kept",
        }),
    )
}

fn input(name: &str, required: bool) -> ActionInputDefinition {
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
    output_type: &str,
    inputs: Vec<ActionInputDefinition>,
    preview_supported: bool,
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
        preview_supported,
        required_ports: Vec::new(),
        availability: ActionAvailability {
            available: true,
            reason: None,
        },
    }
}

pub(crate) fn register_work_bring_actions(registry: &mut ActionRegistry) {
    let actions = [
        (
            descriptor(
                CANDIDATES,
                "Find folders of work outside the ground",
                "Read the folders of work under the person's home that stand outside the ground: git repositories, folders with AGENTS.md or CLAUDE.md, an OKF wiki.json, Obsidian vaults. Never enters the ground, a hidden folder (the products' homes), Library or node_modules, never follows a link, and goes four folders deep.",
                MutationClass::ReadOnly,
                "work-candidate-list",
                Vec::new(),
                false,
            ),
            candidates_action as fn(&ActionRegistry, &Value, &ActionExecutionContext<'_>) -> ActionResult,
        ),
        (
            descriptor(
                PREVIEW,
                "Preview bringing a folder into Work",
                "List the copy work.bring would make of a folder outside the ground into Work/<name> (its own name unless one is given): what it holds, its git repository, and the digest work.bring is pressed on. Refuses a folder in or over the ground, a link leading out of it, a folder that shares another repository (a git worktree, borrowed objects), and a name Work already holds.",
                MutationClass::ReadOnly,
                "work-bring-plan",
                vec![input("source_path", true), input("name", false)],
                false,
            ),
            preview_action,
        ),
        (
            descriptor(
                BRING,
                "Bring a folder into Work",
                "Copy a folder outside the ground into Work/<name> as previewed (expected_digest): its .git, file modes and modification times kept, read back equal before it stands in Work. The original stays where it was.",
                MutationClass::LocallyMutating,
                "work-bring-receipt",
                vec![input("source_path", true), input("name", false), input("expected_digest", true)],
                true,
            ),
            bring_action,
        ),
    ];
    for (descriptor, handler) in actions {
        registry
            .register(descriptor, handler)
            .expect("work bring Action ids are valid");
    }
}
