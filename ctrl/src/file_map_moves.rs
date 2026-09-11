//! Reversible, no-overwrite file-map moves. A durable plan owns both before and
//! after links; replay/rollback never identify a link by its spelling alone.
use super::file_map::*;
use crate::projectcentral_flow::content_revision_bytes;
use crate::source_horizon;
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
fn save(root: &Path, plan: &Plan) -> io::Result<()> {
    write_atomic(&path(root, &plan.id)?, &serde_json::to_vec_pretty(plan)?)
}
fn document_text(scope: &Scope) -> io::Result<Option<String>> {
    document_at(scope, scope.relations_path())
}
fn document_at(scope: &Scope, relative: &str) -> io::Result<Option<String>> {
    if relative != scope.relations_path() && relative != crate::projectcentral_flow::FLOW_REGISTRY {
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
            let (_, target) = lookup(all, &before_link.source_ref)?;
            let original = current.root.join(&link_path);
            let after_path = if current.world == owner.world {
                transform(&link_path, &from, &to)
            } else {
                link_path.clone()
            };
            let after_target = if target.path.starts_with(&entry.path) {
                {
                    let rest = target.path.strip_prefix(&entry.path).unwrap();
                    if rest.as_os_str().is_empty() {
                        owner.root.join(&to)
                    } else {
                        owner.root.join(&to).join(rest)
                    }
                }
            } else {
                target.path
            };
            let new_scope_root = if let Ok(rest) = current.root.strip_prefix(&entry.path) {
                if rest.as_os_str().is_empty() {
                    owner.root.join(&to)
                } else {
                    owner.root.join(&to).join(rest)
                }
            } else {
                current.root.clone()
            };
            let new_target = relative_between(
                new_scope_root.join(&after_path).parent().unwrap(),
                &after_target,
            );
            if after_path == link_path && Path::new(&before_link.target) == new_target {
                continue;
            }
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
    if let Some((before, after)) = crate::projectcentral_flow::plan_file_relocation(
        &owner.root,
        Path::new(&from),
        Path::new(&to),
    )? {
        plan.documents.push(Document {
            world: owner.world.clone(),
            path: Some(crate::projectcentral_flow::FLOW_REGISTRY.into()),
            before: Some(before),
            after,
        });
    }
    save(root, &plan)?;
    Ok(
        json!({"plan_id":id,"source_ref":entry.source.source_ref,"from":from,"destination":to,"state":"prepared","links":plan.links.len(),"requires_quiesced":true,"source_bytes_changed":false}),
    )
}
pub(crate) fn resume_scopes(root: &Path, input: &Value) -> io::Result<Vec<Scope>> {
    let id = text(input, "plan_id")?;
    safe_member(root, ".central/bkmr/moves", true)?;
    let plan: Plan =
        serde_json::from_value(read_json(&path(root, id)?)?).map_err(io::Error::other)?;
    if plan.schema != "central.file-map-move/v1" || plan.id != id {
        return Err(invalid("Invalid move journal"));
    }
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
        if world.world != "control:root" {
            let checked = Scope::project(current.clone(), world.project.clone())?;
            if checked.world != world.world {
                return Err(conflict("Project identity changed during move"));
            }
        }
        result.push(Scope {
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
        let link_path = world.root.join(if at_from {
            &link.path
        } else {
            &link.after_path
        });
        let old = identity(&link_path, link.before.device, link.before.inode);
        let new = identity(&link_path, link.new_device, link.new_inode);
        let backed = identity(
            &world.root.join(&link.backup),
            link.before.device,
            link.before.inode,
        );
        if !(old || new || (backed && fs::symlink_metadata(&link_path).is_err())) {
            return Err(conflict(
                "Managed link was replaced outside this transaction",
            ));
        }
        if !identity(
            &world.root.join(&link.staged),
            link.new_device,
            link.new_inode,
        ) {
            return Err(conflict("Move's staged link is missing or replaced"));
        }
    }
    if !rollback && at_from {
        move_exclusive(&from, &to)?;
    } else if rollback && at_to {
        move_exclusive(&to, &from)?;
    }
    let moved_scopes = resume_scopes(root, input)?;
    let all = moved_scopes.as_slice();
    plan.state = if rollback { "rolling-back" } else { "renamed" }.into();
    save(root, &plan)?;
    // This environment variable is confined to deterministic interruption tests.
    if std::env::var_os("CENTRAL_FILE_MAP_TEST_INTERRUPT_AFTER_RENAME").is_some() {
        return Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "Injected interruption after durable rename",
        ));
    }
    for step in &plan.links {
        let world = scope(all, &step.world)?;
        let link = world.root.join(if rollback {
            &step.path
        } else {
            &step.after_path
        });
        let backup = world.root.join(&step.backup);
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
            fs::hard_link(world.root.join(&step.staged), &link)?;
        }
    }
    for doc in &plan.documents {
        let world = scope(all, &doc.world)?;
        let relative = doc.path.as_deref().unwrap_or(world.relations_path());
        document_at(world, relative)?;
        let target = safe_member(&world.root, relative, false)?;
        if rollback {
            if let Some(before) = &doc.before {
                write_atomic(&target, before.as_bytes())?;
            } else if target.exists() {
                fs::remove_file(target)?;
            }
        } else {
            safe_directory(&world.root, Path::new(relative).parent().unwrap())?;
            write_atomic(&target, doc.after.as_bytes())?;
        }
    }
    plan.state = if rollback { "rolled-back" } else { "applied" }.into();
    save(root, &plan)?;
    let mut indexes = Vec::new();
    let mut refresh: std::collections::BTreeSet<_> = plan.refresh_worlds.iter().cloned().collect();
    refresh.extend(plan.documents.iter().map(|d| d.world.clone()));
    for reference in refresh {
        let world = scope(all, &reference)?;
        if super::file_map_backend::Backend::new(&world.root).present() {
            indexes.push(super::file_map::refresh(world, world.index()?.embeddings)?);
        }
    }
    Ok(
        json!({"plan_id":id,"source_ref":plan.source_ref,"state":plan.state,"indexes":indexes,"source_bytes_changed":false}),
    )
}
