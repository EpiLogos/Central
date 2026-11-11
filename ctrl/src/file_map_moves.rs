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
    // The journal names the destination as the disk spells it: the folders above it in the disk's own case and form, and its own
    // new name as asked (DLW build ruling 2026-11-11, round 54j.1). On a volume that folds case a person may spell an existing
    // folder otherwise (`control/user/new` for `Control/user/new`); a scope inside the moved folder is read after the rename by
    // the journal's own spelling of where it went, and a journal's scope must be the disk's spelling of its folder
    // (`in_ground_spelling`). So the plan writes that spelling, and the checks above are made of it again.
    let to = disk_spelling(&owner.root, &to).unwrap_or(to);
    if Path::new(&to).starts_with(&from) || Path::new(&to).starts_with(".central") {
        return Err(invalid(
            "Move would include its state or owning relation document",
        ));
    }
    safe_member(&owner.root, &to, false)?;
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
/// A destination under `owner_root` as the disk spells it: the folders above it in the disk's own case and form, read through no
/// link that `safe_member` would refuse, and its own name as given (the name the rename makes, or made). None where the folder
/// above it does not read or the place is not under the root.
fn disk_spelling(owner_root: &Path, to: &str) -> Option<String> {
    let place = owner_root.join(to);
    let spelled = fs::canonicalize(place.parent()?)
        .ok()?
        .join(place.file_name()?);
    let root = fs::canonicalize(owner_root).ok()?;
    Some(spelled.strip_prefix(root).ok()?.to_str()?.to_owned())
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
    resume_scopes_as(root, input, true, false)
}
/// The scopes a move's apply (`rollback` false) or rollback runs from, for the act and for the person's gate, which stands a replay
/// or a rollback on the scopes it runs from and makes nothing (`make_state` false; file_map.rs `grant_scope`; DLW build ruling
/// 2026-11-09, round 54f item 1). They are [`resume_scopes`], but for one journal (DLW build ruling 2026-11-11, round 54j item 2; the review of #346's third commit, C2).
/// A journal whose rename was carried and nothing after it (`rename_stands_alone`), and whose scopes do not read while its Source
/// stands where the rename put it, can be rolled back so far as the rename: the rollback then runs from the journal's scopes that
/// do read, its owner among them, and makes the rename back and nothing else (`apply`). The gate judges that rename's two places as
/// it judges any move's. Every other journal whose scopes do not read is refused, as before.
pub(crate) fn carried_scopes(
    root: &Path,
    input: &Value,
    rollback: bool,
    make_state: bool,
) -> io::Result<Vec<Scope>> {
    let unread = match resume_scopes_as(root, input, make_state, false) {
        Ok(all) => return Ok(all),
        Err(unread) => unread,
    };
    let alone = rollback
        && text(input, "plan_id")
            .and_then(|id| path(root, id))
            .and_then(|journal| read_json(&journal))
            .and_then(|journal| serde_json::from_value::<Plan>(journal).map_err(io::Error::other))
            .is_ok_and(|plan| rename_stands_alone(root, &plan));
    if !alone {
        return Err(unread);
    }
    resume_scopes_as(root, input, make_state, true).map_err(|_| unread)
}
/// Whether a journal's rename was carried and stands alone to be made back: its state says "renamed", as apply saves it once the
/// Source has moved, and the Source stands at its `to`. Where its owner is spelled is the journal's word, as in `resume_scopes_as`;
/// the rename itself is made only by `apply`, under the owner's root as the scopes give it.
fn rename_stands_alone(root: &Path, plan: &Plan) -> bool {
    plan.state == "renamed"
        && plan
            .worlds
            .iter()
            .find(|world| world.world == plan.world)
            .is_some_and(|owner| {
                let owner_root = if owner.external {
                    PathBuf::from(&owner.path)
                } else {
                    root.join(&owner.path)
                };
                identity(&owner_root.join(&plan.to), plan.device, plan.inode)
            })
}
/// An in-ground scope of a journal stands at the folder its spelling names, in the disk's own spelling of it (DLW build ruling
/// 2026-11-11, round 54j item 1; the review of #346's third commit, C1): no link in any name of it, and each name in the case and
/// form the disk keeps, as `plan` writes it (a scope is opened that way, `safe_member`) and as an external scope is already held
/// (`plain_world_path`). So no link step can lead a scope elsewhere by replacing a link its spelling runs through. `current` is
/// where the scope stands now, shifted with the moved folder. A project that does not stand there is named by its own refusal; any
/// other scope that does not is refused here.
fn in_ground_spelling(root: &Path, current: &Path, world: &PlanWorld) -> io::Result<()> {
    let refused = || {
        invalid(format!(
            "This move's journal spells the scope {} as {}, which is not that folder's own spelling on the disk now: a link, or \
             another case or form of a name, stands in it",
            world.world, world.path
        ))
    };
    let expected = match current.strip_prefix(root) {
        Ok(below) => root.canonicalize()?.join(below),
        Err(_) => current.to_path_buf(),
    };
    match fs::canonicalize(current) {
        Ok(canonical) if canonical == expected => Ok(()),
        Ok(_) => Err(refused()),
        Err(_) if world.world.starts_with("project:") => Ok(()),
        Err(_) => Err(refused()),
    }
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
/// `alone` leaves out a project whose declaration does not read, for a rollback that makes the rename back and nothing else
/// (`carried_scopes`); every other refusal stands.
fn resume_scopes_as(
    root: &Path,
    input: &Value,
    make_state: bool,
    alone: bool,
) -> io::Result<Vec<Scope>> {
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
    // Where the Source went, as the disk spells the folders above it: a journal planned before round 54j.1, or by hand, may spell
    // them in another case, and a scope inside the moved folder is read below by this spelling. The Source's own name is the
    // journal's: the rename made it so, and it is not read through if it is a link.
    let to = disk_spelling(&owner_root, &plan.to).map_or(to, |spelled| owner_root.join(spelled));
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
        if !world.external {
            in_ground_spelling(root, &current, world)?;
        }
        // A folder scope has no Project declaration to check: its own is the ground's.
        if world.world != "control:root"
            && !world
                .world
                .starts_with(super::file_map_catalog::FOLDER_WORLD_PREFIX)
        {
            // A project the journal runs from whose declaration does not read is named, as the gate names one the move reaches
            // (round 54g item 12): the error keeps its kind, so a link loop is a failure and no permission is a denial.
            let checked = match Scope::project(current.clone(), world.project.clone()) {
                Ok(checked) => checked,
                Err(_) if alone && world.world != plan.world => continue,
                Err(error) => {
                    return Err(io::Error::new(
                        error.kind(),
                        format!(
                            "its journal runs from the project {}, which does not read: {error}",
                            world.project.as_deref().unwrap_or(&world.world)
                        ),
                    ))
                }
            };
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
/// One link step carried at its scope's root, or taken back (`back`): the staged link put in the link's place with the old link kept
/// aside, or the old link put back. The link's place is its path before the move for a rollback and after it for an apply
/// (`rollback`), whichever way the step then goes. Answers whether anything was changed: a step already as asked is left.
fn carry_link(scope_root: &Path, step: &LinkStep, rollback: bool, back: bool) -> io::Result<bool> {
    let link = step_member(
        scope_root,
        if rollback {
            &step.path
        } else {
            &step.after_path
        },
    )?;
    let backup = step_member(scope_root, &step.backup)?;
    if back {
        if identity(&link, step.before.device, step.before.inode) {
            return Ok(false);
        }
        if identity(&link, step.new_device, step.new_inode) {
            fs::remove_file(&link)?;
        }
        move_exclusive(&backup, &link)?;
    } else {
        if identity(&link, step.new_device, step.new_inode) {
            return Ok(false);
        }
        if identity(&link, step.before.device, step.before.inode) {
            move_exclusive(&link, &backup)?;
        }
        // link(2) with a symlink source retains its inode and creates the
        // destination exclusively. The staged inode is already journalled.
        fs::hard_link(step_member(scope_root, &step.staged)?, &link)?;
    }
    Ok(true)
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
    // A journal whose destination is spelled otherwise than the disk spells it was planned before `plan` wrote the disk's
    // spelling (round 54j.1; the review of #346's fourth commit, D1). Its relations documents name every scope and Source under
    // the moved folder by that spelling, and the map does not read them: carried out, it leaves a ground that only a rollback
    // mends, as it did before this rule. So it is not carried out, and the words say what to do. A rollback of it goes on, from
    // any state: it writes the relations as they were.
    if !rollback {
        if let Some(spelled) =
            disk_spelling(&owner.root, &plan.to).filter(|spelled| *spelled != plan.to)
        {
            return Err(conflict(format!(
                "This move was planned with its destination spelled {}, where the disk spells it {spelled}: carried out, the \
                 relations it writes would name that spelling and not read. Roll the move back{}, then plan it again; nothing \
                 was written",
                plan.to,
                if at_to { "" } else { " if it was begun" }
            )));
        }
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
    // A journal whose rename was carried and whose scopes do not read while the Source stands there is rolled back so far as the
    // rename, and says so (round 54j item 2; C2). Its link steps and documents need scopes that read: once the Source is back
    // they may, and a second rollback then undoes whatever the move had carried of them.
    if rollback && at_to && rename_stands_alone(root, &plan) {
        if let Err(unread) = resume_scopes_as(root, input, false, false) {
            return match move_exclusive(&to, &from) {
                Ok(()) => {
                    plan.state = "rolling-back".into();
                    save(
                        root,
                        &plan,
                        crate::file_mutation::RecordDisposition::ReplaceOrCreate,
                    )?;
                    Ok(json!({"plan_id":id,"source_ref":plan.source_ref,"state":plan.state,"indexes":[],
                        "source_bytes_changed":false,
                        "note":format!(
                            "the Source is back at {}, and nothing else was written: this journal's scopes did not read while it \
                             stood at {} ({unread}). Roll back again to undo any link or relations step the move had carried",
                            plan.from, plan.to
                        )}))
                }
                Err(error) => Err(conflict(format!(
                    "Move journal's scopes do not read ({unread}) while its Source stands at {}, and it could not be moved back \
                     ({error}): move it back to {} by hand, then roll back again; nothing was written",
                    to.display(),
                    from.display()
                ))),
            };
        }
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
        // A link step replaces a link and nothing else (DLW build ruling 2026-11-11, round 54i item 1; the review of #346's second
        // commit, B1): whatever stands at its path, before or after, at its kept name or its staged one, is a symbolic link, or
        // the journal stops here, before anything is moved. And its path is no scope's own folder, nor a folder above one, which
        // the refusal says in its own words (round 54j item 1).
        for (spelled, place) in [
            (&link.path, step_member(&world.root, &link.path)?),
            (
                &link.after_path,
                step_member(&world.root, &link.after_path)?,
            ),
            (&link.backup, kept.clone()),
            (&link.staged, staged.clone()),
        ] {
            // A scope's folder is spelled as the disk spells it (`in_ground_spelling`, `plain_world_path`), so this reading of the
            // spellings finds every step at one, or above one.
            if all.iter().any(|scope| scope.root.starts_with(&place)) {
                return Err(invalid(format!(
                    "Invalid move journal: its link step at {} names {spelled}, which is a scope's own folder or a folder above \
                     one: a link step replaces a link and nothing else",
                    link.path
                )));
            }
            let standing = fs::symlink_metadata(&place);
            if standing.is_ok_and(|entry| !entry.file_type().is_symlink()) {
                return Err(invalid(format!(
                    "Invalid move journal: its link step at {} names {spelled}, which is not a link: a link step replaces a \
                     link and nothing else",
                    link.path
                )));
            }
        }
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
    // its documents and links would be written in a tree the gate never judged. So was one that does not read at all once the
    // Source has moved (round 54i item 2; B2). Either way the scopes are not the ones the gate saw: the rename this call made is
    // put back and nothing else is written. A call that made no rename (a journal already carried that far) goes on to its own
    // refusal below. Since round 54j a scope spelled through a link is refused before the rename (`in_ground_spelling`), so no
    // journal reaches the another-folder half by itself: it is kept against a second writer that swaps a scope's folder while
    // the move runs, and no test reaches it. The does-not-read half is reached by a journal that moves a scope's own declaration
    // away.
    let strayed = match &moved_scopes {
        Ok(scopes) => folders_of(scopes)
            .into_iter()
            .find(|now| !stood.contains(now))
            .map(|(world, _)| format!("scope {world} is another folder")),
        Err(error) if renamed.is_some() => Some(format!("scopes do not read ({error})")),
        Err(_) => None,
    };
    if let Some(strayed) = strayed {
        if let Some((now, before)) = renamed {
            if let Err(error) = move_exclusive(now, before) {
                // The Source could not be put back: the journal says where it stands, in the state a carried rename has. A
                // rollback reads that state when the scopes still do not read, and makes the rename back alone
                // (`rename_stands_alone`; round 54j item 2). A rollback's own failed put-back leaves the Source where the move
                // took it from, and the refusal says what a hand must do.
                plan.state = if rollback { "rolling-back" } else { "renamed" }.into();
                save(
                    root,
                    &plan,
                    crate::file_mutation::RecordDisposition::ReplaceOrCreate,
                )?;
                return Err(conflict(format!(
                    "Move journal's {strayed} once its Source has moved, and the Source could not be put back ({error}): it \
                     stands at {}, and must be moved back to {} by hand, or with a rollback once that name is free; nothing else \
                     was written",
                    now.display(),
                    before.display()
                )));
            }
        }
        return Err(conflict(format!(
            "Move journal's {strayed} once its Source has moved; the Source is back where it stood and nothing else was written"
        )));
    }
    let was = plan.state.clone();
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
        // The steps this call carried, to be taken back if a scope strays (round 54j item 1).
        let mut carried: Vec<&LinkStep> = Vec::new();
        for step in &plan.links {
            let world = scope(all, &step.world)?;
            if carry_link(&world.root, step, rollback, rollback)? {
                carried.push(step);
            }
            // Each scope is held to its folder again after every link step, so before the next one and before any document is
            // written (DLW build ruling 2026-11-11, round 54j item 1; C1). The spelling rule leaves no link in a scope's path for a
            // step to replace; this holds the scopes should one stray all the same, as a second writer could make one. What this
            // call carried is then taken back, newest first, each step only while its own scope still stands where it stood, and
            // the refusal says what could not be.
            let Some((strayed, _)) = folders_of(all).into_iter().find(|now| !stood.contains(now))
            else {
                continue;
            };
            let mut left = Vec::new();
            for taken in carried.iter().rev() {
                let world = scope(all, &taken.world)?;
                let holds = folders_of(std::slice::from_ref(world))
                    .iter()
                    .all(|now| stood.contains(now));
                match holds.then(|| carry_link(&world.root, taken, rollback, !rollback)) {
                    Some(Ok(_)) => {}
                    Some(Err(error)) => left.push(format!("{} ({error})", taken.path)),
                    None => left.push(format!("{} (its scope strayed)", taken.path)),
                }
            }
            let source = match renamed {
                _ if !left.is_empty() => None,
                Some((now, before)) => move_exclusive(now, before).ok().map(|()| before),
                None => None,
            };
            if source.is_some() {
                plan.state = was.clone();
                save(
                    root,
                    &plan,
                    crate::file_mutation::RecordDisposition::ReplaceOrCreate,
                )?;
            }
            let steps = if left.is_empty() {
                "the link steps this call carried are undone".to_owned()
            } else {
                format!(
                    "these link steps could not be undone and stand as carried: {}",
                    left.join(", ")
                )
            };
            return Err(conflict(format!(
                "Move journal's scope {strayed} is another folder once a link step has run; {steps}; the Source {}; no document \
                 was written",
                match source {
                    Some(back) => format!("is back at {}", back.display()),
                    None => "stands where this call found or left it".to_owned(),
                }
            )));
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
