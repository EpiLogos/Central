//! Reversible, no-overwrite file-map moves. A durable plan owns both before and
//! after links; replay/rollback never identify a link by its spelling alone.
use super::file_map::*;
use crate::source_horizon;
use crate::source_safety::content_revision_bytes;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
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
/// A journal as `move-plan` wrote it before plans were sealed (round 54k), and one written since.
const JOURNAL_UNSEALED: &str = "central.file-map-move/v1";
const JOURNAL_SEALED: &str = "central.file-map-move/v2";
const SEAL_SCHEMA: &str = "central.file-map-move-seal/v1";
fn journal_schema(schema: &str) -> bool {
    schema == JOURNAL_UNSEALED || schema == JOURNAL_SEALED
}
/// A Work project's own copy of a move's plan (DLW build ruling 2026-11-11, round 54k): the digest of the whole journal as
/// `move-plan` wrote it, kept in that project's `.central/bkmr/moves/<id>.seal.json`, beside the lock files a plan makes there.
/// `move-plan` is gated by that project's grant, so the seal stands only for a plan the project's grant admitted.
#[derive(Serialize, Deserialize)]
struct Seal {
    schema: String,
    plan_id: String,
    project: String,
    digest: String,
}
fn seal_member(id: &str) -> String {
    format!(".central/bkmr/moves/{id}.seal.json")
}
/// The digest a seal holds: SHA-256 over the journal written again from its parsed form, with `state` blanked, since apply and
/// rollback change it as the move goes and nothing else of a journal changes after its plan. So every field that steers a write
/// is under it (the scopes and their spellings, the owner, `from`, `to`, the Source's identity and revision, each link step, each
/// document's world, place and bytes), and the spacing and key order of the file on the disk are not.
fn digest(plan: &Plan) -> io::Result<String> {
    let mut sealed = plan.clone();
    sealed.state = String::new();
    Ok(format!(
        "sha256:{:x}",
        Sha256::digest(serde_json::to_vec(&sealed)?)
    ))
}
/// `move-plan` seals its plan in every Work project the journal's places reach, by the gate's own reading of them
/// (`journal_reach`): the folder is made 0700 where it was not there, the file is 0600, and a seal already standing for this id
/// with another digest is never written over. If one cannot be written, those written are taken away and the plan fails.
fn seal(root: &Path, all: &[Scope], plan: &Plan) -> io::Result<()> {
    let member = seal_member(&plan.id);
    let digest = digest(plan)?;
    let mut written: Vec<PathBuf> = Vec::new();
    let result = (|| {
        for (name, project) in journal_reach(root, all, &json!({"plan_id":plan.id}))? {
            let file = project.join(&member);
            let bytes = serde_json::to_vec_pretty(&Seal {
                schema: SEAL_SCHEMA.into(),
                plan_id: plan.id.clone(),
                project: name.clone(),
                digest: digest.clone(),
            })?;
            safe_directory(&project, Path::new(".central/bkmr/moves"))?;
            if fs::symlink_metadata(&file).is_ok() {
                return Err(conflict(format!(
                    "The Work project {name} already holds a plan for the move {} (Work/{name}/{member}); nothing was planned",
                    plan.id
                )));
            }
            write_atomic(
                &project,
                &file,
                &bytes,
                crate::file_mutation::RecordDisposition::CreateNew,
            )?;
            written.push(file.clone());
            fs::set_permissions(
                &file,
                <fs::Permissions as std::os::unix::fs::PermissionsExt>::from_mode(0o600),
            )?;
        }
        Ok(())
    })();
    if result.is_err() {
        for file in written {
            let _ = fs::remove_file(file);
        }
    }
    result
}
/// Whether the Work project at `project` holds the seal of the journal `id` whose digest is `digest`. The seal is read as the
/// project's own ordinary file, with no link in the folders above it.
fn seal_answers(project: &Path, id: &str, digest: &str) -> bool {
    let file = project.join(seal_member(id));
    crate::source_safety::reject_symlink_components(project, Path::new(".central/bkmr/moves"))
        .ok()
        .and_then(|()| fs::symlink_metadata(&file).ok())
        .filter(|entry| entry.is_file())
        .and_then(|_| fs::read(&file).ok())
        .and_then(|bytes| serde_json::from_slice::<Seal>(&bytes).ok())
        .is_some_and(|seal| seal.plan_id == id && seal.digest == digest)
}
/// Whether the Work project at `project` holds the seal of the journal `input` names, as it stands at the ground now: what a
/// carried move asks before it makes a lock folder in that project's tree (file_map.rs `locks_only_standing`), and before it
/// refreshes an index there (`apply`). A journal from before seals, or one that does not read, is held by no project.
pub(crate) fn plan_held_by(root: &Path, input: &Value, project: &Path) -> bool {
    input
        .get("plan_id")
        .and_then(Value::as_str)
        .and_then(|id| path(root, id).ok())
        .and_then(|journal| fs::read(journal).ok())
        .and_then(|bytes| serde_json::from_slice::<Plan>(&bytes).ok())
        .filter(|plan| plan.schema == JOURNAL_SEALED)
        .is_some_and(|plan| {
            digest(&plan).is_ok_and(|digest| seal_answers(project, &plan.id, &digest))
        })
}
/// What a carried move holds every write to: a write that lands in a Work project is made only when that project's own seal is
/// the digest of the journal in hand (DLW build ruling 2026-11-11). The project is found on the disk at the moment of the write,
/// from the folder actually written in (`projects_written_at`), never from what the journal says of its scopes; so a journal
/// edited in the root's `.central` matches no project's seal and writes in none, whatever the edit is. A journal planned before
/// plans were sealed has no digest and writes in no Work project: `unsealed` is what its refusal goes on to say (`old_journal`).
struct Sealed {
    ground: PathBuf,
    id: String,
    digest: Option<String>,
    unsealed: String,
}
impl Sealed {
    fn of(root: &Path, plan: &Plan, unsealed: String) -> io::Result<Self> {
        let sealed = plan.schema == JOURNAL_SEALED;
        Ok(Self {
            ground: root.to_path_buf(),
            id: plan.id.clone(),
            digest: sealed.then(|| digest(plan)).transpose()?,
            unsealed,
        })
    }
    /// Whether the Work project at `project` holds this journal's seal.
    fn holds(&self, project: &Path) -> bool {
        self.digest
            .as_deref()
            .is_some_and(|digest| seal_answers(project, &self.id, digest))
    }
    /// The Work project a journal from before seals would write in at `path`, by its folder's name; none for a sealed journal,
    /// whose writes ask each project's seal (`place`).
    fn unsealed_in(&self, path: &Path, link: bool) -> Option<String> {
        if self.digest.is_some() {
            return None;
        }
        projects_written_at(&self.ground, path, link)
            .into_iter()
            .next()
            .map(|(name, _)| name)
    }
    /// Refuses a write at `path` (a link's own place where `link`) unless every Work project it lies in, or holds, holds this
    /// journal's seal. `named` is how the refusal names the place.
    fn place(&self, path: &Path, link: bool, named: &str) -> io::Result<()> {
        let member = seal_member(&self.id);
        for (name, project) in projects_written_at(&self.ground, path, link) {
            let Some(digest) = &self.digest else {
                return Err(conflict(format!(
                    "This move was planned before a plan was kept in each Work project it writes in, and {named} is in the Work \
                     project {name}: `ctrl` writes nothing of this move there. {}; nothing was written",
                    self.unsealed
                )));
            };
            if !seal_answers(&project, &self.id, digest) {
                return Err(conflict(format!(
                    "This move's journal is not the plan the Work project {name} holds (Work/{name}/{member}): {named} is in \
                     {name}, and nothing is written there; plan the move again"
                )));
            }
        }
        Ok(())
    }
}
/// A seal lives as long as its journal, at every state: a rolled-back move can be applied again, and that needs it (DLW build
/// ruling 2026-11-11). Nothing removes a journal today. This is the one place a seal goes: whatever comes to remove a journal
/// calls it first, with the journal still standing, and the seals in the projects its places reach go with it.
#[allow(dead_code)]
pub(crate) fn remove_seals(root: &Path, all: &[Scope], id: &str) -> io::Result<()> {
    for (_, project) in journal_reach(root, all, &json!({"plan_id":id}))? {
        let _ = fs::remove_file(project.join(seal_member(id)));
    }
    Ok(())
}
/// What the refusal of a journal from before seals goes on to say (DLW build ruling 2026-11-12): where its Source stands, and
/// what goes on from there, which is never the press being refused. An apply is refused whole: where nothing of the move stands
/// the person plans it again, and where something does a rollback is the press, since a rollback of such a journal puts back
/// the Source and every step outside Work's projects and names each step left in one (`apply`). A rollback is refused only
/// where its own rename back would write in a Work project: then a hand moves the Source back, and the rollback goes on.
fn old_journal(plan: &Plan, rollback: bool, at_from: bool, from: &Path, to: &Path) -> String {
    let stands = if at_from {
        format!(
            "Its Source stands at {}, where it stood before the move",
            from.display()
        )
    } else {
        format!(
            "Its Source stands at {}, where the move took it from {}",
            to.display(),
            from.display()
        )
    };
    if rollback {
        return format!(
            "{stands}. Moving it back writes in that project, so a hand does it: move {} to {}. A rollback then puts back the \
             steps outside Work's projects and names each one left in a project",
            to.display(),
            from.display()
        );
    }
    if at_from && matches!(plan.state.as_str(), "prepared" | "rolled-back") {
        return format!("{stands}, and nothing of the move stands: plan the move again");
    }
    format!(
        "{stands}. To undo what was carried, roll the move back: a rollback puts back the Source and each link and relations \
         document outside Work's projects, and names each step left in a project for a hand; then plan the move again"
    )
}
/// One step a rollback of a journal from before seals leaves in a Work project, in words a hand can follow.
fn left_link(
    ground: &Path,
    project: &str,
    link: &Path,
    kept: &Path,
    carried_stands: bool,
) -> String {
    let named = |path: &Path| {
        path.strip_prefix(ground)
            .unwrap_or(path)
            .display()
            .to_string()
    };
    format!(
        "in {project}, {}move {} to {}",
        if carried_stands {
            format!("remove the link {}, then ", named(link))
        } else {
            String::new()
        },
        named(kept),
        named(link)
    )
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
    // `Work` itself, or a folder that holds it, is not moved (DLW build ruling 2026-11-12): every project of Work stands under it,
    // and no act of Central's reads a ground whose `Work` is away, so such a move could be neither finished nor undone by a press.
    if within(&root.join("Work"), &entry.path) {
        return Err(invalid(format!(
            "{from} is Work itself, or a folder that holds it, and Central does not move it: every project of Work stands under \
             it, and no act reads a ground whose Work is away. Move a project of Work, or a folder inside one"
        )));
    }
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
        schema: JOURNAL_SEALED.into(),
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
    // Each Work project the move writes in keeps its own copy of the plan (round 54k). A plan that cannot be sealed is no plan.
    if let Err(error) = seal(root, all, &plan) {
        let _ = fs::remove_file(path(root, &id)?);
        return Err(error);
    }
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
/// A journal's scopes are the ones its plan wrote, and a plan writes the scopes the ground holds (DLW build ruling 2026-11-12;
/// the review of #349, F1): the root; a project of Work, which stands at `Work/<its folder's name>`; a project registered at the
/// root, at the place the root's relations keep for its name (or where this move takes that place, since the plan rewrites it);
/// and a folder scope, which stands outside the ground and is declared in `.central/scopes/<name>`. A journal that names any
/// other scope is refused here, where the gate reads it, before a lock is taken or a state folder made: so no scope made up in
/// a journal (a folder of a Work project's among them) has a lock folder made in it. `registered` is the root's own register,
/// read once.
fn held_by_the_ground(
    root: &Path,
    plan: &Plan,
    world: &PlanWorld,
    registered: &mut Option<std::collections::BTreeMap<String, String>>,
) -> io::Result<()> {
    // The ground's own scope: the root's, or the one project of a ground that is a project.
    if !world.external && world.path.is_empty() {
        return Ok(());
    }
    let refused = || {
        invalid(format!(
            "Invalid move journal: it names the scope {} at {}, and the ground holds no such scope there: a move runs from the \
             scopes its plan wrote, which are the root, the projects of Work, the projects registered at the root and the folder \
             scopes declared in it",
            world.world, world.path
        ))
    };
    let Some(name) = world.project.as_deref() else {
        return Err(refused());
    };
    if world
        .world
        .starts_with(super::file_map_catalog::FOLDER_WORLD_PREFIX)
    {
        let declared = world.external
            && super::file_map_catalog::folder_declarations(root)?
                .into_iter()
                .flatten()
                .any(|folder| {
                    folder.name == name
                        && world.world
                            == format!("{}{name}", super::file_map_catalog::FOLDER_WORLD_PREFIX)
                        && folder.root == world.path
                });
        return if declared { Ok(()) } else { Err(refused()) };
    }
    if !world.external && Path::new(&world.path) == Path::new("Work").join(name) {
        return Ok(());
    }
    if registered.is_none() {
        let base = Scope {
            root: root.into(),
            state: root.into(),
            world: "control:root".into(),
            project: None,
        };
        *registered = Some(base.ground()?.scopes);
    }
    let Some(kept) = registered.as_ref().and_then(|scopes| scopes.get(name)) else {
        return Err(refused());
    };
    // The register's own spelling may be absolute, and a place in the ground kept so is still in the ground: it is read as the
    // journal spells it, below the ground.
    let (kept, spelled) = (Path::new(kept), Path::new(&world.path));
    let canonical = kept
        .is_absolute()
        .then(|| fs::canonicalize(kept).ok())
        .flatten();
    let held = if world.external {
        kept.is_absolute() && (kept == spelled || canonical.as_deref() == Some(spelled))
    } else {
        let below = [Some(kept), canonical.as_deref()]
            .into_iter()
            .flatten()
            .find_map(|place| match place.strip_prefix(root) {
                Ok(below) => Some(below.to_path_buf()),
                Err(_) => (!place.is_absolute()).then(|| place.to_path_buf()),
            });
        below.is_some_and(|below| {
            below == spelled
                || (plan.world == "control:root"
                    && below == Path::new(&transform(&world.path, &plan.from, &plan.to)))
        })
    };
    if held {
        Ok(())
    } else {
        Err(refused())
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
    if !journal_schema(&plan.schema) || plan.id != id {
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
    let mut registered = None;
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
        if world.world != "control:root" {
            held_by_the_ground(root, &plan, world, &mut registered)?;
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
fn carry_link(
    scope_root: &Path,
    step: &LinkStep,
    rollback: bool,
    back: bool,
    sealed: Option<&Sealed>,
) -> io::Result<bool> {
    let link = step_member(
        scope_root,
        if rollback {
            &step.path
        } else {
            &step.after_path
        },
    )?;
    let backup = step_member(scope_root, &step.backup)?;
    // Each of the step's three places is asked for its project's seal as it stands now (round 54k).
    if let Some(sealed) = sealed {
        sealed.place(&link, true, &step.path)?;
        sealed.place(&backup, true, &step.backup)?;
        sealed.place(&step_member(scope_root, &step.staged)?, true, &step.staged)?;
    }
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
    if !journal_schema(&plan.schema) || plan.id != id {
        return Err(invalid("Invalid move journal"));
    }
    checked(&plan)?;
    let owner = scope(all, &plan.world)?;
    // `move-plan` plans no move in a folder scope or in a project outside the ground, and a journal that names one as its owner
    // is carried out no more than it would be planned (round 54k; the review of #346's fourth commit, D2).
    if owner.is_folder() || owner.is_external() {
        return Err(invalid(format!(
            "A move's journal names {} as its owner, and Central moves nothing in a folder scope or a project outside the \
             ground: such a scope is read-only; nothing was moved",
            owner.world
        )));
    }
    let from = safe_member(&owner.root, &plan.from, false)?;
    let to = safe_member(&owner.root, &plan.to, false)?;
    let at_from = identity(&from, plan.device, plan.inode);
    let at_to = identity(&to, plan.device, plan.inode);
    if at_from == at_to {
        return Err(conflict("Move source identity missing or ambiguous"));
    }
    // A move of `Work` itself, or of a folder that holds it, is carried out no more than it is planned (DLW build ruling
    // 2026-11-12; the review of #352, F2): a journal that a `ctrl` from before that rule planned and sealed is refused here,
    // whatever seals it holds, since carried out it leaves a ground no act reads. A rollback goes on: it closes the journal, or
    // undoes what was carried once a hand has put `Work` back.
    if !rollback && within(&root.join("Work"), &from) {
        return Err(invalid(format!(
            "This move's journal moves {}, which is Work itself, or a folder that holds it, and Central does not carry such a \
             move out: every project of Work stands under it, and no act reads a ground whose Work is away. Roll the move back \
             to close its journal; nothing was moved",
            plan.from
        )));
    }
    let sealed = Sealed::of(
        root,
        &plan,
        old_journal(&plan, rollback, at_from, &from, &to),
    )?;
    // A rollback of a journal from before seals is carried out so far as it writes in no Work project (DLW build ruling
    // 2026-11-12; the review of #349, F2): the rename back, and each link step and relations document outside Work's projects.
    // Each step in a project is left, and the answer names it for a hand. So such a journal, begun, always has a press.
    let old = rollback && sealed.digest.is_none();
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
            sealed.place(&to, false, &plan.to)?;
            sealed.place(&from, false, &plan.from)?;
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
    // Every place the journal writes at is asked for its project's seal now, so a journal that cannot be carried out whole is
    // refused before anything moves (round 54k). Each write asks again as it is made: that asking is the boundary.
    // A rollback of a journal from before seals asks at its rename, and leaves each step in a project (`old`).
    if !old {
        for place in journal_places(root, all, input) {
            sealed.place(&place.path, place.link, &place.named)?;
        }
    }
    // Each scope's folder as the checks above and the gate read it. The rename moves folders and makes none, so each scope is the
    // same folder after it, wherever it then stands (round 54h item 2).
    let stood = folders_of(all);
    let renamed = if !rollback && at_from {
        sealed.place(&from, false, &plan.from)?;
        sealed.place(&to, false, &plan.to)?;
        move_exclusive(&from, &to)?;
        Some((&to, &from))
    } else if rollback && at_to {
        sealed.place(&to, false, &plan.to)?;
        sealed.place(&from, false, &plan.from)?;
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
    // Set where a refusal inside the completion left everything as this call found it: such a refusal is passed on as it is,
    // and not as a completion that may be partial (round 54k; the review of #346's fourth commit, D3).
    let whole = std::cell::Cell::new(false);
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
        // What a rollback of a journal from before seals leaves in Work's projects, each as a step for a hand, in order.
        let mut by_hand: Vec<String> = Vec::new();
        for step in &plan.links {
            let world = scope(all, &step.world)?;
            if old {
                let link = step_member(&world.root, &step.path)?;
                let kept = step_member(&world.root, &step.backup)?;
                let staged = step_member(&world.root, &step.staged)?;
                let in_project = [&link, &kept, &staged]
                    .into_iter()
                    .find_map(|place| sealed.unsealed_in(place, true));
                if let Some(project) = in_project {
                    // Left only where the step stands carried: a link still the one the plan found needs no hand.
                    if !identity(&link, step.before.device, step.before.inode) {
                        by_hand.push(left_link(
                            root,
                            &project,
                            &link,
                            &kept,
                            identity(&link, step.new_device, step.new_inode),
                        ));
                    }
                    continue;
                }
            }
            if carry_link(&world.root, step, rollback, rollback, Some(&sealed))? {
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
                // Taking back what this call carried asks no seal: each of those writes asked it a moment ago.
                match holds.then(|| carry_link(&world.root, taken, rollback, !rollback, None)) {
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
            whole.set(left.is_empty() && (renamed.is_none() || source.is_some()));
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
        for (index, doc) in plan.documents.iter().enumerate() {
            let world = scope(all, &doc.world)?;
            let relative = doc.path.as_deref().unwrap_or(world.relations_path());
            let current = document_at(world, relative)?;
            let target = safe_member(&world.root, relative, false)?;
            if let Some(project) = sealed.unsealed_in(&target, false).filter(|_| old) {
                if current != doc.before {
                    let named = target.strip_prefix(root).unwrap_or(&target).display();
                    by_hand.push(match &doc.before {
                        Some(_) => format!(
                            "in {project}, write {named} back as this move's journal keeps it (the text at \
                             `documents[{index}].before` in .central/bkmr/moves/{id}.json)"
                        ),
                        None => format!("in {project}, remove {named}"),
                    });
                }
                continue;
            }
            sealed.place(&target, false, &format!("the relations of {}", world.world))?;
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
        // A rollback that left steps in a project is not whole: its journal stays "rolling-back", and a rollback after a hand
        // has made them finds nothing left and closes it.
        plan.state = match (rollback, by_hand.is_empty()) {
            (true, true) => "rolled-back",
            (true, false) => "rolling-back",
            (false, _) => "applied",
        }
        .into();
        save(
            root,
            &plan,
            crate::file_mutation::RecordDisposition::ReplaceOrCreate,
        )?;
        let mut indexes = Vec::new();
        // An index kept in a Work project's tree is that project's, and is refreshed here only where the project holds this
        // journal's seal (DLW build ruling 2026-11-12; the review of #349, F4): a journal edited to list a project among its
        // refresh scopes writes no index there. The scope is named in the answer; `refresh` is a press of its own.
        let mut not_refreshed = Vec::new();
        let mut refresh: std::collections::BTreeSet<_> =
            plan.refresh_worlds.iter().cloned().collect();
        refresh.extend(plan.documents.iter().map(|d| d.world.clone()));
        for reference in refresh {
            let world = scope(all, &reference)?;
            if !super::file_map_backend::Backend::for_scope(world).present() {
                continue;
            }
            let unsealed = projects_written_at(root, &world.state, false)
                .into_iter()
                .any(|(_, project)| within(&world.state, &project) && !sealed.holds(&project));
            if unsealed {
                not_refreshed.push(reference);
                continue;
            }
            indexes.push(super::file_map::refresh(world, world.index()?.embeddings)?);
        }
        let mut answer = json!({"plan_id":id,"source_ref":plan.source_ref,"state":plan.state,"indexes":indexes,"source_bytes_changed":false});
        if !not_refreshed.is_empty() {
            answer["indexes_not_refreshed"] = json!(not_refreshed);
        }
        if !by_hand.is_empty() {
            answer["left_for_a_hand"] = json!(by_hand);
            answer["note"] = json!(format!(
                "the Source is back at {} and each step outside Work's projects is undone. This move was planned before a plan \
                 was kept in each Work project it writes in, so `ctrl` writes nothing of it there. Left for a hand, in this \
                 order: {}. A rollback after that finds nothing left and closes the journal",
                plan.from,
                by_hand.join("; then ")
            ));
        }
        Ok(answer)
    })();
    completion.map_err(|error| {
        if whole.get() {
            return error;
        }
        crate::file_mutation::record_owner_error(
            error,
            "file_map.move_after_path_observation",
            Some(&plan.source_ref),
            Some(&plan.revision),
            "path_move_observed_relation_steps_may_be_partial",
        )
    })
}
