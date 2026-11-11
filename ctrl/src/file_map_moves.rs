//! Reversible, no-overwrite file-map moves. A durable plan owns both before and
//! after links; replay/rollback never identify a link by its spelling alone.
use super::file_map::*;
use crate::source_horizon;
use crate::source_safety::content_revision_bytes;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::os::unix::fs::MetadataExt;
use std::{
    fs, io,
    path::{Path, PathBuf},
};

#[derive(Clone, Serialize, Deserialize)]
struct Document {
    world: String,
    #[serde(default)]
    path: Option<String>,
    before: Option<String>,
    after: String,
}
#[derive(Clone, Serialize, Deserialize)]
struct LinkStep {
    world: String,
    path: String,
    after_path: String,
    before: Link,
    staged: String,
    backup: String,
    new_device: u64,
    new_inode: u64,
}
#[derive(Clone, Serialize, Deserialize)]
struct PlanWorld {
    world: String,
    path: String,
    external: bool,
    project: Option<String>,
}
#[derive(Clone, Serialize, Deserialize)]
struct Plan {
    worlds: Vec<PlanWorld>,
    refresh_worlds: Vec<String>,
    schema: String,
    id: String,
    world: String,
    source_ref: String,
    from: String,
    to: String,
    device: u64,
    inode: u64,
    revision: String,
    documents: Vec<Document>,
    links: Vec<LinkStep>,
    state: String,
}
fn scope<'a>(all: &'a [Scope], world: &str) -> io::Result<&'a Scope> {
    all.iter()
        .find(|s| s.world == world)
        .ok_or_else(|| invalid("Move's owning World is unavailable"))
}
fn path(root: &Path, id: &str) -> io::Result<PathBuf> {
    if id.is_empty() || !id.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(invalid("Invalid move id"));
    }
    Ok(root.join(".central/bkmr/moves").join(format!("{id}.json")))
}
/// One place a move's journal writes (`journal_places`): how the refusal names it, where it stands, and whether it is a link's own
/// place, which is judged where the link stands and never where it points.
pub(crate) struct JournalPlace {
    pub named: String,
    pub path: PathBuf,
    pub link: bool,
}
/// The places a move's journal writes, read without changing anything, for the person's gate (file_map.rs `act_places`; round 54d
/// F1): where it moves its Source between (its `from` and its `to`, each under its owner's root), the relations document of each
/// scope it writes one in, and each link step's own link, before and after, under its scope's root, as that scope stands among
/// `all`, the scopes the move runs from (round 54g item 4; DLW build ruling 2026-11-11, round 54h item 1). A link step in a Work
/// project's tree so needs that project's grant, whichever scope the journal says keeps the link. A step's staged and kept links
/// stand in its scope's own moves folder (`checked`). A journal that does not read names none: the act itself then refuses it.
pub(crate) fn journal_places(root: &Path, all: &[Scope], input: &Value) -> Vec<JournalPlace> {
    let Some(plan) = input
        .get("plan_id")
        .and_then(Value::as_str)
        .and_then(|id| path(root, id).ok())
        .and_then(|journal| fs::read(journal).ok())
        .and_then(|bytes| serde_json::from_slice::<Plan>(&bytes).ok())
    else {
        return Vec::new();
    };
    let Some(owner) = plan.worlds.iter().find(|world| world.world == plan.world) else {
        return Vec::new();
    };
    let base = if owner.external {
        PathBuf::from(&owner.path)
    } else {
        root.join(&owner.path)
    };
    let place = |named: String, path: PathBuf, link: bool| JournalPlace { named, path, link };
    let mut places = vec![
        place(plan.from.clone(), base.join(&plan.from), false),
        place(plan.to.clone(), base.join(&plan.to), false),
    ];
    let written = plan
        .documents
        .iter()
        .map(|document| &document.world)
        .collect::<std::collections::BTreeSet<_>>();
    for world in written {
        if let Some(scope) = all.iter().find(|scope| &scope.world == world) {
            places.push(place(
                format!("the relations of {world}"),
                scope.root.join(scope.relations_path()),
                false,
            ));
        }
    }
    for step in &plan.links {
        let Some(scope) = all.iter().find(|scope| scope.world == step.world) else {
            continue;
        };
        places.push(place(
            link_named(scope, &step.path),
            scope.root.join(&step.path),
            true,
        ));
        if step.after_path != step.path {
            places.push(place(
                link_named(scope, &step.after_path),
                scope.root.join(&step.after_path),
                true,
            ));
        }
    }
    places
}
/// How a refusal names a managed link: its path in its scope, and the scope.
fn link_named(scope: &Scope, link_path: &str) -> String {
    format!("the link {link_path} of {}", scope.world)
}
/// Where a link step's link, its staged link or its kept link stands under its scope's root: a contained path, with no link in any
/// folder above it (round 54h item 1). The last name is the link itself and is not followed.
fn step_member(root: &Path, raw: &str) -> io::Result<PathBuf> {
    let contained = relative(raw)?;
    if let Some(folder) = contained.parent() {
        crate::source_safety::reject_symlink_components(root, folder)?;
    }
    Ok(root.join(contained))
}
fn save(
    root: &Path,
    plan: &Plan,
    disposition: crate::file_mutation::RecordDisposition,
) -> io::Result<()> {
    write_atomic(
        root,
        &path(root, &plan.id)?,
        &serde_json::to_vec_pretty(plan)?,
        disposition,
    )
}
fn document_text(scope: &Scope) -> io::Result<Option<String>> {
    document_at(scope, scope.relations_path())
}
fn document_at(scope: &Scope, relative: &str) -> io::Result<Option<String>> {
    const RETIRED_FLOW_REGISTRY: &str = ".central/flows.json";
    if relative != scope.relations_path() && relative != RETIRED_FLOW_REGISTRY {
        return Err(invalid("Move journal names an unsupported owner document"));
    }
    let p = safe_member(&scope.root, relative, false)?;
    if p.exists() {
        Ok(Some(fs::read_to_string(p)?))
    } else {
        Ok(None)
    }
}
fn transform(raw: &str, from: &str, to: &str) -> String {
    match Path::new(raw).strip_prefix(from) {
        Ok(rest) if rest.as_os_str().is_empty() => to.into(),
        Ok(rest) => Path::new(to).join(rest).to_string_lossy().into(),
        Err(_) => raw.into(),
    }
}
fn identity(path: &Path, device: u64, inode: u64) -> bool {
    fs::symlink_metadata(path).is_ok_and(|m| m.dev() == device && m.ino() == inode)
}
fn move_exclusive(from: &Path, to: &Path) -> io::Result<()> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    let from = CString::new(from.as_os_str().as_bytes()).map_err(io::Error::other)?;
    let to = CString::new(to.as_os_str().as_bytes()).map_err(io::Error::other)?;
    #[cfg(target_os = "linux")]
    let result = unsafe {
        libc::renameat2(
            libc::AT_FDCWD,
            from.as_ptr(),
            libc::AT_FDCWD,
            to.as_ptr(),
            libc::RENAME_NOREPLACE,
        )
    };
    #[cfg(target_os = "macos")]
    let result = unsafe { libc::renamex_np(from.as_ptr(), to.as_ptr(), libc::RENAME_EXCL) };
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    return Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "No atomic no-replace rename on this platform",
    ));
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    if result != 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}
pub(crate) fn plan(root: &Path, all: &[Scope], input: &Value) -> io::Result<Value> {
    let (owner, entry) = lookup(all, text(input, "source_ref")?)?;
    // A folder scope's file is the person's, where it stands: Central writes nothing inside the folder, so it moves nothing there.
    if owner.is_folder() {
        return Err(folder_writes_nothing(&owner));
    }
    if owner.is_external() {
        return Err(invalid(
            "External scopes are read-only (placement.json grants no writes there); move the file with the owner's own tools and refresh",
        ));
    }
    let from = entry
        .path
        .strip_prefix(&owner.root)
        .map_err(|_| invalid("External resource moves require its native owner"))?
        .to_str()
        .ok_or_else(|| invalid("Non-UTF8 move path"))?
        .to_owned();
    let to = text(input, "destination")?.to_owned();
    relative(&to)?;
    if Path::new(&to).starts_with(&from)
        || Path::new(&from).starts_with(".central")
        || Path::new(&to).starts_with(".central")
        || Path::new(owner.relations_path()).starts_with(&from)
    {
        return Err(invalid(
            "Move would include its state or owning relation document",
        ));
    }
    let dest = safe_member(&owner.root, &to, false)?;
    if fs::symlink_metadata(&dest).is_ok() {
        return Err(conflict("Move destination already exists"));
    }
    if !dest.parent().is_some_and(Path::is_dir) {
        return Err(invalid("Move destination parent must already exist"));
    }
    if std::env::current_dir()?.starts_with(&entry.path) {
        return Err(conflict(
            "Cannot move the invoking process's working directory",
        ));
    }
    let metadata = fs::symlink_metadata(&entry.path)?;
    let id = format!(
        "{:x}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(io::Error::other)?
            .as_nanos()
    );
    safe_directory(root, Path::new(".central/bkmr/moves"))?;
    let worlds = all
        .iter()
        .map(|s| PlanWorld {
            world: s.world.clone(),
            path: s
                .root
                .strip_prefix(root)
                .unwrap_or(&s.root)
                .to_string_lossy()
                .into(),
            external: !s.root.starts_with(root),
            project: s.project.clone(),
        })
        .collect();
    let refresh_worlds = all
        .iter()
        .filter(|s| s.root.starts_with(&entry.path))
        .map(|s| s.world.clone())
        .collect();
    let mut plan = Plan {
        worlds,
        refresh_worlds,
        schema: "central.file-map-move/v1".into(),
        id: id.clone(),
        world: owner.world.clone(),
        source_ref: entry.source.source_ref.clone(),
        from: from.clone(),
        to: to.clone(),
        device: metadata.dev(),
        inode: metadata.ino(),
        revision: entry.revision.clone(),
        documents: vec![],
        links: vec![],
        state: "prepared".into(),
    };
    // Pin every participating source under the move to its existing ref, rather
    // than letting the pathname fallback mint a second identity after rename.
    for current in all {
        // A folder scope stands outside the ground and keeps no relation a move could change.
        if current.is_folder() {
            continue;
        }
        let before = document_text(current)?;
        let mut doc = current.document()?;
        let mut ground = current.ground()?;
        let mut changed = false;
        if current.world == owner.world {
            for path in ground.scopes.values_mut() {
                *path = transform(path, &from, &to);
                changed = true;
            }
            for resource in ground.resources.values_mut() {
                if !resource.external {
                    let path = transform(&resource.path, &from, &to);
                    changed |= path != resource.path;
                    resource.path = path;
                }
            }
            if doc.is_null() {
                let schema = if current.world == "control:root" {
                    source_horizon::CONTROL_GROUND_RELATIONS_SCHEMA
                } else {
                    source_horizon::GROUND_RELATIONS_SCHEMA
                };
                let pid = current
                    .world
                    .strip_prefix("project:")
                    .unwrap_or(&current.world);
                doc = json!({"schema":schema,"project_id":pid,"relations":[]});
            }
            for source in entries(current)?
                .into_iter()
                .filter(|e| e.path.starts_with(&entry.path) && e.kind == "file")
            {
                let mut binding = serde_json::to_value(&source.source)?;
                let new_path = transform(&source.source.path, &from, &to);
                binding["path"] = json!(new_path);
                let relations = doc["relations"]
                    .as_array_mut()
                    .ok_or_else(|| invalid("Invalid source relations"))?;
                relations.retain(|r| {
                    r["ref"] != source.source.source_ref && r["path"] != source.source.path
                });
                relations.push(binding);
                changed = true;
            }
        }
        let old_links = ground.links.clone();
        for (link_path, before_link) in old_links {
            let original = current.root.join(&link_path);
            let Some((after_path, new_target)) = link_after(
                all,
                (&owner, &entry.path, &from, &to),
                current,
                &link_path,
                &before_link,
            )?
            else {
                continue;
            };
            if !identity(&original, before_link.device, before_link.inode)
                || fs::read_link(&original)? != Path::new(&before_link.target)
            {
                return Err(conflict("Managed link changed before move planning"));
            }
            let slot = format!(".central/bkmr/moves/{id}-{}", plan.links.len());
            safe_directory(&current.root, Path::new(".central/bkmr/moves"))?;
            let staged = format!("{slot}-new");
            let backup = format!("{slot}-old");
            std::os::unix::fs::symlink(&new_target, current.root.join(&staged))?;
            let meta = fs::symlink_metadata(current.root.join(&staged))?;
            let mut updated = before_link.clone();
            updated.target = new_target.to_string_lossy().into();
            updated.device = meta.dev();
            updated.inode = meta.ino();
            ground.links.remove(&link_path);
            ground.links.insert(after_path.clone(), updated);
            plan.links.push(LinkStep {
                world: current.world.clone(),
                path: link_path,
                after_path,
                before: before_link,
                staged,
                backup,
                new_device: meta.dev(),
                new_inode: meta.ino(),
            });
            changed = true;
        }
        if changed {
            doc["file_map"] = serde_json::to_value(ground)?;
            plan.documents.push(Document {
                world: current.world.clone(),
                path: None,
                before,
                after: serde_json::to_string_pretty(&doc)?,
            });
        }
    }
    // The retired Flow registry needs no relocation planning: a leftover
    // registry file, if any, simply stays where it is and stays refused as a
    // move target.
    save(
        root,
        &plan,
        crate::file_mutation::RecordDisposition::CreateNew,
    )?;
    Ok(
        json!({"plan_id":id,"source_ref":entry.source.source_ref,"from":from,"destination":to,"state":"prepared","links":plan.links.len(),"requires_quiesced":true,"source_bytes_changed":false}),
    )
}
/// A move of the Source at `moved` (`from` to `to`, under `owner`'s root).
type Moving<'a> = (&'a Scope, &'a Path, &'a str, &'a str);
/// Where `current`'s managed link at `link_path` stands and points once the move is carried out, or None when the move leaves it as
/// it is. One reading for `plan`, which rewrites the link, and for the person's gate, which asks the grant of each scope the move
/// rewrites a link in (`rewritten_scopes`).
fn link_after(
    all: &[Scope],
    (owner, moved, from, to): Moving<'_>,
    current: &Scope,
    link_path: &str,
    before: &Link,
) -> io::Result<Option<(String, PathBuf)>> {
    let (_, target) =
        lookup(all, &before.source_ref).map_err(|error| link_unread(current, link_path, &error))?;
    let after_path = if current.world == owner.world {
        transform(link_path, from, to)
    } else {
        link_path.to_owned()
    };
    let moved_to = |rest: &Path| {
        if rest.as_os_str().is_empty() {
            owner.root.join(to)
        } else {
            owner.root.join(to).join(rest)
        }
    };
    let after_target = match target.path.strip_prefix(moved) {
        Ok(rest) => moved_to(rest),
        Err(_) => target.path,
    };
    let new_scope_root = match current.root.strip_prefix(moved) {
        Ok(rest) => moved_to(rest),
        Err(_) => current.root.clone(),
    };
    let new_target = relative_between(
        new_scope_root.join(&after_path).parent().unwrap(),
        &after_target,
    );
    if after_path == link_path && Path::new(&before.target) == new_target {
        return Ok(None);
    }
    Ok(Some((after_path, new_target)))
}
/// A managed link whose Source does not read stops every move's plan: no grant mends it, so the refusal names the link and the
/// scope it stands in, and says what does (round 54h item 6).
fn link_unread(current: &Scope, link_path: &str, error: &io::Error) -> io::Error {
    let stands = match current.project.as_deref() {
        Some(project) if current.world != "control:root" => format!("the project {project}"),
        _ => "the root".to_owned(),
    };
    io::Error::new(
        error.kind(),
        format!(
            "the link {link_path} in {stands} names a Source that does not read ({error}); no grant mends this: remove the link or \
             restore its Source, then plan the move again"
        ),
    )
}
/// A scope a move rewrites in (`rewritten_scopes`), and each link it rewrites there: how a refusal names it, and where it stands
/// before and after the move.
pub(crate) struct Rewritten<'a> {
    pub scope: &'a Scope,
    pub links: Vec<(String, PathBuf)>,
}
/// The scopes whose managed links a move of the Source at `moved` to `to`, under `owner`, rewrites, as `plan` rewrites them: in each,
/// the move stages the new link in its `.central/bkmr/moves`, replaces the link and rewrites its relations document, so the
/// person's gate asks each one's grant (DLW build ruling 2026-11-09, round 54g item 4). Each rewritten link is named with where it
/// stands, before the move and after it, so the gate judges the link by its own place and not by the scope that keeps it (round
/// 54h item 3). A scope whose relations do not read is counted, with no link named: `plan` refuses it, and the gate asks no less
/// than the act would write. A link whose Source does not read is refused here in the words `plan` refuses it in (`link_unread`).
pub(crate) fn rewritten_scopes<'a>(
    all: &'a [Scope],
    owner: &Scope,
    moved: &Path,
    to: &str,
) -> io::Result<Vec<Rewritten<'a>>> {
    let Some(from) = moved.strip_prefix(&owner.root).ok().and_then(Path::to_str) else {
        return Ok(Vec::new());
    };
    let mut rewritten = Vec::new();
    for current in all.iter().filter(|current| !current.is_folder()) {
        let Ok(ground) = current.ground() else {
            rewritten.push(Rewritten {
                scope: current,
                links: Vec::new(),
            });
            continue;
        };
        let mut links = Vec::new();
        for (link_path, before) in &ground.links {
            let Some((after_path, _)) =
                link_after(all, (owner, moved, from, to), current, link_path, before)?
            else {
                continue;
            };
            links.push((link_named(current, link_path), current.root.join(link_path)));
            let after_root = match current.root.strip_prefix(moved) {
                Ok(rest) => owner.root.join(to).join(rest),
                Err(_) => current.root.clone(),
            };
            links.push((
                link_named(current, &after_path),
                after_root.join(&after_path),
            ));
        }
        if !links.is_empty() {
            rewritten.push(Rewritten {
                scope: current,
                links,
            });
        }
    }
    Ok(rewritten)
}
/// What a move's journal may write, as `plan` writes it (round 54g item 4): a relations document only at the move's own scope or at
/// one it rewrites a link in, and each link step only at a path contained in its scope, with its own staged and kept links in that
/// scope's moves folder. A journal edited to name anything else is refused here. A contained path in the root's scope can still
/// stand in a Work project's tree: that is the gate's to judge, by the step's own place (`journal_places`; round 54h item 1).
fn checked(plan: &Plan) -> io::Result<()> {
    for (index, step) in plan.links.iter().enumerate() {
        let slot = format!(".central/bkmr/moves/{}-{index}", plan.id);
        if relative(&step.path).is_err()
            || relative(&step.after_path).is_err()
            || step.staged != format!("{slot}-new")
            || step.backup != format!("{slot}-old")
        {
            return Err(invalid(format!(
                "Invalid move journal: its link step at {} names a place its move does not write",
                step.path
            )));
        }
    }
    if let Some(document) = plan.documents.iter().find(|document| {
        document.world != plan.world && !plan.links.iter().any(|step| step.world == document.world)
    }) {
        return Err(invalid(format!(
            "Invalid move journal: it rewrites the relations of {}, where its move changes no link",
            document.world
        )));
    }
    Ok(())
}
/// The scopes a move carried out from its journal runs from: the journal's own, each where the move has left it, so a scope inside
/// the moved folder is read where it now stands though its declaration still names where it stood. An external scope's state folder
/// is made where missing.
pub(crate) fn resume_scopes(root: &Path, input: &Value) -> io::Result<Vec<Scope>> {
    resume_scopes_as(root, input, true)
}
/// [`resume_scopes`] that makes nothing, for the person's gate, which stands a replay or a rollback on the scopes it runs from
/// (file_map.rs `grant_scope`; DLW build ruling 2026-11-09, round 54f item 1).
pub(crate) fn resume_scopes_unmade(root: &Path, input: &Value) -> io::Result<Vec<Scope>> {
    resume_scopes_as(root, input, false)
}
/// A journal's scope is spelled as `plan` writes it (round 54h item 2): in the ground, plain names below it (none for the ground
/// itself), so no `..` or `.` reads one folder before the rename and another after it; outside the ground, the folder's own
/// absolute spelling, as an external root is opened (`open_external_root`). An external folder that is not there now is taken as
/// spelled: nothing can be read or written through it.
fn plain_world_path(world: &PlanWorld) -> io::Result<()> {
    let spelled = Path::new(&world.path);
    let refused = || {
        invalid(format!(
            "Invalid move journal: it spells the scope {} as {}, which is not the spelling its plan writes",
            world.world, world.path
        ))
    };
    let plain = |path: &Path| {
        path.components()
            .all(|part| matches!(part, std::path::Component::Normal(_)))
    };
    if !world.external {
        return if spelled.is_absolute() || !plain(spelled) {
            Err(refused())
        } else {
            Ok(())
        };
    }
    let Ok(below) = spelled.strip_prefix("/") else {
        return Err(refused());
    };
    if !plain(below) || Path::new("/").join(below).as_os_str() != spelled.as_os_str() {
        return Err(refused());
    }
    match fs::canonicalize(spelled) {
        Ok(canonical) if canonical != spelled => Err(refused()),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}
/// The folder each scope stands at, by its device and inode, or none where it does not read.
fn folders_of(all: &[Scope]) -> Vec<(String, Option<(u64, u64)>)> {
    all.iter()
        .map(|scope| {
            (
                scope.world.clone(),
                fs::metadata(&scope.root)
                    .ok()
                    .map(|held| (held.dev(), held.ino())),
            )
        })
        .collect()
}
fn resume_scopes_as(root: &Path, input: &Value, make_state: bool) -> io::Result<Vec<Scope>> {
    let id = text(input, "plan_id")?;
    safe_member(root, ".central/bkmr/moves", true)?;
    let plan: Plan =
        serde_json::from_value(read_json(&path(root, id)?)?).map_err(io::Error::other)?;
    if plan.schema != "central.file-map-move/v1" || plan.id != id {
        return Err(invalid("Invalid move journal"));
    }
    // The gate reads the journal here too, so an edited one is refused before any grant is asked of it (round 54g item 4).
    checked(&plan)?;
    let owner = plan
        .worlds
        .iter()
        .find(|s| s.world == plan.world)
        .ok_or_else(|| invalid("Move's owning scope is missing"))?;
    let owner_root = if owner.external {
        PathBuf::from(&owner.path)
    } else {
        root.join(&owner.path)
    };
    let from = owner_root.join(&plan.from);
    let to = owner_root.join(&plan.to);
    let moved = identity(&to, plan.device, plan.inode);
    let mut result = Vec::new();
    for world in &plan.worlds {
        // The root's scope is the ground itself, as `plan` writes it: a journal that sets it anywhere else would carry the move out
        // in another tree than the one its gate stands on (round 54f.2).
        if world.world == "control:root" && (world.external || !world.path.is_empty()) {
            return Err(invalid(
                "Invalid move journal: it sets the root's scope away from the ground",
            ));
        }
        plain_world_path(world)?;
        let mut current = if world.external {
            PathBuf::from(&world.path)
        } else {
            root.join(&world.path)
        };
        if moved {
            if let Ok(rest) = current.strip_prefix(&from) {
                current = if rest.as_os_str().is_empty() {
                    to.clone()
                } else {
                    to.join(rest)
                };
            }
        }
        // A folder scope has no Project declaration to check: its own is the ground's.
        if world.world != "control:root"
            && !world
                .world
                .starts_with(super::file_map_catalog::FOLDER_WORLD_PREFIX)
        {
            // A project the journal runs from whose declaration does not read is named, as the gate names one the move reaches
            // (round 54g item 12): the error keeps its kind, so a link loop is a failure and no permission is a denial.
            let checked =
                Scope::project(current.clone(), world.project.clone()).map_err(|error| {
                    io::Error::new(
                        error.kind(),
                        format!(
                            "its journal runs from the project {}, which does not read: {error}",
                            world.project.as_deref().unwrap_or(&world.world)
                        ),
                    )
                })?;
            if checked.world != world.world {
                return Err(conflict("Project identity changed during move"));
            }
        }
        result.push(Scope {
            state: if world.external {
                let name = world
                    .project
                    .as_deref()
                    .ok_or_else(|| invalid("External scope without a name"))?;
                if make_state {
                    super::file_map_catalog::external_state(root, name)?
                } else {
                    super::file_map_catalog::external_state_unmade(root, name)?
                }
            } else {
                current.clone()
            },
            root: current,
            world: world.world.clone(),
            project: world.project.clone(),
        });
    }
    Ok(result)
}
pub(crate) fn apply(
    root: &Path,
    all: &[Scope],
    input: &Value,
    rollback: bool,
) -> io::Result<Value> {
    if input["quiesced"] != true {
        return Err(invalid(
            "A move requires explicit quiesced=true: caller has stopped affected sessions/processes",
        ));
    }
    let id = text(input, "plan_id")?;
    safe_member(root, ".central/bkmr/moves", true)?;
    let mut plan: Plan =
        serde_json::from_value(read_json(&path(root, id)?)?).map_err(io::Error::other)?;
    if plan.schema != "central.file-map-move/v1" || plan.id != id {
        return Err(invalid("Invalid move journal"));
    }
    checked(&plan)?;
    let owner = scope(all, &plan.world)?;
    let from = safe_member(&owner.root, &plan.from, false)?;
    let to = safe_member(&owner.root, &plan.to, false)?;
    let at_from = identity(&from, plan.device, plan.inode);
    let at_to = identity(&to, plan.device, plan.inode);
    if at_from == at_to {
        return Err(conflict("Move source identity missing or ambiguous"));
    }
    let present = if at_from { &from } else { &to };
    if present.is_file() && content_revision_bytes(&fs::read(present)?) != plan.revision {
        return Err(conflict("Source content changed since move plan"));
    }
    if std::env::current_dir()?.starts_with(present) {
        return Err(conflict(
            "Cannot relocate a live invoking working directory",
        ));
    }
    for doc in &plan.documents {
        let world = scope(all, &doc.world)?;
        let current = document_at(world, doc.path.as_deref().unwrap_or(world.relations_path()))?;
        if current != doc.before && current.as_deref() != Some(&doc.after) {
            return Err(conflict("Source relations changed since move plan"));
        }
    }
    for link in &plan.links {
        let world = scope(all, &link.world)?;
        // Each of a step's three places is contained in its scope, with no link in a folder above it (round 54h item 1).
        let link_path = step_member(
            &world.root,
            if at_from {
                &link.path
            } else {
                &link.after_path
            },
        )?;
        let kept = step_member(&world.root, &link.backup)?;
        let staged = step_member(&world.root, &link.staged)?;
        let old = identity(&link_path, link.before.device, link.before.inode);
        let new = identity(&link_path, link.new_device, link.new_inode);
        let backed = identity(&kept, link.before.device, link.before.inode);
        if !(old || new || (backed && fs::symlink_metadata(&link_path).is_err())) {
            return Err(conflict(
                "Managed link was replaced outside this transaction",
            ));
        }
        if !identity(&staged, link.new_device, link.new_inode) {
            return Err(conflict("Move's staged link is missing or replaced"));
        }
    }
    // Each scope's folder as the checks above and the gate read it. The rename moves folders and makes none, so each scope is the
    // same folder after it, wherever it then stands (round 54h item 2).
    let stood = folders_of(all);
    let renamed = if !rollback && at_from {
        move_exclusive(&from, &to)?;
        Some((&to, &from))
    } else if rollback && at_to {
        move_exclusive(&to, &from)?;
        Some((&from, &to))
    } else {
        None
    };
    let moved_scopes = resume_scopes(root, input);
    // A scope that reads as another folder once the Source has moved was spelled through the moved folder (a link inside it, say):
    // its documents and links would be written in a tree the gate never judged. The rename is put back and nothing else is written.
    if let Some((world, _)) = moved_scopes.as_ref().ok().and_then(|scopes| {
        folders_of(scopes)
            .into_iter()
            .find(|now| !stood.contains(now))
    }) {
        if let Some((now, before)) = renamed {
            move_exclusive(now, before)?;
        }
        return Err(conflict(format!(
            "Move journal's scope {world} is another folder once its Source has moved; the Source is back where it stood and \
             nothing else was written"
        )));
    }
    let completion = (|| {
        let moved_scopes = moved_scopes?;
        let all = moved_scopes.as_slice();
        plan.state = if rollback { "rolling-back" } else { "renamed" }.into();
        save(
            root,
            &plan,
            crate::file_mutation::RecordDisposition::ReplaceOrCreate,
        )?;
        // This environment variable is confined to deterministic interruption tests.
        if std::env::var_os("CENTRAL_FILE_MAP_TEST_INTERRUPT_AFTER_RENAME").is_some() {
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "Injected interruption after durable rename",
            ));
        }
        for step in &plan.links {
            let world = scope(all, &step.world)?;
            let link = step_member(
                &world.root,
                if rollback {
                    &step.path
                } else {
                    &step.after_path
                },
            )?;
            let backup = step_member(&world.root, &step.backup)?;
            if rollback {
                if identity(&link, step.before.device, step.before.inode) {
                    continue;
                }
                if identity(&link, step.new_device, step.new_inode) {
                    fs::remove_file(&link)?;
                }
                move_exclusive(&backup, &link)?;
            } else {
                if identity(&link, step.new_device, step.new_inode) {
                    continue;
                }
                if identity(&link, step.before.device, step.before.inode) {
                    move_exclusive(&link, &backup)?;
                }
                // link(2) with a symlink source retains its inode and creates the
                // destination exclusively. The staged inode is already journalled.
                fs::hard_link(step_member(&world.root, &step.staged)?, &link)?;
            }
        }
        for doc in &plan.documents {
            let world = scope(all, &doc.world)?;
            let relative = doc.path.as_deref().unwrap_or(world.relations_path());
            document_at(world, relative)?;
            let target = safe_member(&world.root, relative, false)?;
            if rollback {
                if let Some(before) = &doc.before {
                    write_atomic(
                        &world.root,
                        &target,
                        before.as_bytes(),
                        crate::file_mutation::RecordDisposition::ReplaceOrCreate,
                    )?;
                } else if target.exists() {
                    fs::remove_file(target)?;
                }
            } else {
                safe_directory(&world.root, Path::new(relative).parent().unwrap())?;
                write_atomic(
                    &world.root,
                    &target,
                    doc.after.as_bytes(),
                    crate::file_mutation::RecordDisposition::ReplaceOrCreate,
                )?;
            }
        }
        plan.state = if rollback { "rolled-back" } else { "applied" }.into();
        save(
            root,
            &plan,
            crate::file_mutation::RecordDisposition::ReplaceOrCreate,
        )?;
        let mut indexes = Vec::new();
        let mut refresh: std::collections::BTreeSet<_> =
            plan.refresh_worlds.iter().cloned().collect();
        refresh.extend(plan.documents.iter().map(|d| d.world.clone()));
        for reference in refresh {
            let world = scope(all, &reference)?;
            if super::file_map_backend::Backend::for_scope(world).present() {
                indexes.push(super::file_map::refresh(world, world.index()?.embeddings)?);
            }
        }
        Ok(
            json!({"plan_id":id,"source_ref":plan.source_ref,"state":plan.state,"indexes":indexes,"source_bytes_changed":false}),
        )
    })();
    completion.map_err(|error| {
        crate::file_mutation::record_owner_error(
            error,
            "file_map.move_after_path_observation",
            Some(&plan.source_ref),
            Some(&plan.revision),
            "path_move_observed_relation_steps_may_be_partial",
        )
    })
}
