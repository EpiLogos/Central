//! Persistent Central/ProjectCentral file maps. bkmr owns its records and search;
//! existing source relations own source identity, locations and managed links.
use crate::action::{
    ActionAvailability, ActionDescriptor, ActionExecutionContext, ActionInputDefinition,
    ActionOutputDefinition, ActionRegistry, MutationClass,
};
use crate::file_map_backend::{self as native, Backend};
use crate::projectcentral_flow::reject_symlink_components;
use crate::result::{ActionResult, ResultStatus};
use crate::source_horizon;
use serde_json::{json, Value};
use std::os::unix::fs::MetadataExt;
use std::{
    collections::BTreeSet,
    fs, io,
    path::{Path, PathBuf},
};

pub use crate::file_map_catalog::SCHEMA;
pub(crate) use crate::file_map_catalog::*;
pub(crate) use crate::file_map_index::refresh;
use crate::file_map_index::search;
fn register_source(all: &[Scope], scope: &Scope, input: &Value) -> io::Result<Value> {
    expect_basis(scope, input)?;
    let raw = text(input, "path")?;
    let external = Path::new(raw).is_absolute();
    if external && input["allow_external"] != true {
        return Err(invalid(
            "External registration requires allow_external=true",
        ));
    }
    let path = if external {
        safe_member(Path::new("/"), raw.trim_start_matches('/'), true)?
    } else {
        safe_member(&scope.root, raw, true)?
    };
    if !(path.is_file() || path.is_dir()) {
        return Err(invalid("Register a regular file or directory"));
    }
    let policy_root = if external {
        Path::new("/")
    } else {
        scope.root.as_path()
    };
    if !source_horizon::retrieval_allowed(policy_root, &path)
        || (path.is_dir() && path.join(".no-agent-retrieval").exists())
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Resource is excluded by source policy",
        ));
    }
    // A root encounter must not mint another identity for Project-owned ground.
    for owner in all {
        if owner.world != scope.world {
            if let Some(entry) = entries(owner)?.into_iter().find(|e| e.path == path) {
                return Ok(
                    json!({"source_ref":entry.source.source_ref,"world_ref":owner.world,"revision":owner.basis()?,"source_bytes_changed":false,"already_registered":true}),
                );
            }
        }
    }
    let existing = entries(scope)?.into_iter().find(|e| e.path == path);
    let reference = existing
        .as_ref()
        .map(|e| e.source.source_ref.clone())
        .unwrap_or_else(|| source_horizon::source_ref(&scope.world, raw));
    if let Some(requested) = input["source_ref"].as_str() {
        if requested != reference {
            return Err(invalid(
                "Use the source's existing canonical ref; registration does not reidentify it",
            ));
        }
    }
    let mut ground = scope.ground()?;
    let doc = scope.document()?;
    let tags: Vec<String> = serde_json::from_value(input.get("tags").cloned().unwrap_or(json!([])))
        .map_err(io::Error::other)?;
    if tags
        .iter()
        .any(|t| t.is_empty() || t.contains([',', '\n', '\r']))
    {
        return Err(invalid(
            "Tags must be non-empty and cannot contain commas or newlines",
        ));
    }
    ground.resources.insert(
        reference.clone(),
        Resource {
            path: raw.into(),
            external,
            native_import: input["native_import"] == true,
            title: input["title"].as_str().unwrap_or(raw).into(),
            tags,
        },
    );
    scope.save(&ground, doc)?;
    Ok(
        json!({"source_ref":reference,"world_ref":scope.world,"revision":scope.basis()?,"source_bytes_changed":false}),
    )
}
pub(crate) fn verified_links(all: &[Scope], scope: &Scope) -> io::Result<Vec<(PathBuf, Entry)>> {
    let mut result = Vec::new();
    for (relative, link) in scope.ground()?.links {
        let member = relative_member(&scope.root, &relative)?;
        let metadata = fs::symlink_metadata(&member)?;
        if !metadata.file_type().is_symlink()
            || metadata.dev() != link.device
            || metadata.ino() != link.inode
            || fs::read_link(&member)? != Path::new(&link.target)
        {
            return Err(conflict("Registered link was replaced or redirected"));
        }
        let (_, source) = lookup(all, &link.source_ref)?;
        if source.world_ref != link.world_ref || member.canonicalize()? != source.path {
            return Err(conflict("Registered link no longer reaches its source"));
        }
        result.push((member, source));
    }
    Ok(result)
}
fn relative_member(root: &Path, raw: &str) -> io::Result<PathBuf> {
    let member = relative(raw)?;
    if let Some(parent) = member.parent().filter(|p| !p.as_os_str().is_empty()) {
        reject_symlink_components(root, parent)?;
    }
    Ok(root.join(member))
}
fn locate(all: &[Scope], input: &Value) -> io::Result<Value> {
    let raw = text(input, "path")?;
    let lexical = if Path::new(raw).is_absolute() {
        PathBuf::from(raw)
    } else {
        selected(all, input)?.root.join(relative(raw)?)
    };
    for scope in all {
        if input["project"].is_string() && selected(all, input)?.world != scope.world {
            continue;
        }
        let candidate = lexical
            .strip_prefix(&scope.root)
            .ok()
            .and_then(Path::to_str);
        if candidate.is_some_and(|path| scope.ground().is_ok_and(|g| g.links.contains_key(path))) {
            for (link_path, entry) in verified_links(all, scope)? {
                if link_path == lexical {
                    let mut request = input.clone();
                    request.as_object_mut().unwrap().remove("project");
                    request["source_ref"] = json!(entry.source.source_ref);
                    let mut result = resolve(all, &request)?;
                    result["encountered_link"] = json!({"path":link_path,"world_ref":scope.world});
                    return Ok(result);
                }
            }
        }
    }
    let target = if Path::new(raw).is_absolute() {
        safe_member(Path::new("/"), raw.trim_start_matches('/'), true)?
    } else {
        safe_member(&selected(all, input)?.root, raw, true)?
    };
    for scope in all {
        if input["project"].is_string() && selected(all, input)?.world != scope.world {
            continue;
        }
        if let Some(entry) = entries(scope)?.into_iter().find(|e| e.path == target) {
            let mut request = input.clone();
            request["source_ref"] = json!(entry.source.source_ref);
            return resolve(all, &request);
        }
    }
    if !source_horizon::retrieval_allowed(Path::new("/"), &target)
        || (target.is_dir() && target.join(".no-agent-retrieval").exists())
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Location is excluded by source policy",
        ));
    }
    Err(io::Error::new(
        io::ErrorKind::NotFound,
        "Location is unregistered",
    ))
}
pub(crate) fn resolve(all: &[Scope], input: &Value) -> io::Result<Value> {
    let (scope, entry) = lookup(all, text(input, "source_ref")?)?;
    if input["project"].is_string()
        && selected(all, input)?.world != scope.world
        && !verified_links(all, selected(all, input)?)?
            .iter()
            .any(|(_, linked)| linked.source.source_ref == entry.source.source_ref)
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Source belongs to another Project scope and has no declared link here",
        ));
    }
    if !context_allows(all, selected(all, input)?, &entry.source.source_ref)? {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Source excluded by the requesting World's effective source relations",
        ));
    }
    if let Some(revision) = input["expected_revision"].as_str() {
        if revision != entry.revision {
            return Err(conflict("Source revision changed"));
        }
    }
    let mut value = serde_json::to_value(&entry)?;
    value["project"] = json!(scope.project);
    if input["content"] == true {
        if input["content_encoding"] == "base64" {
            use base64::Engine;
            value["content"] =
                json!(base64::engine::general_purpose::STANDARD.encode(payload(&entry)?));
            value["content_encoding"] = json!("base64");
        } else {
            value["content"] = json!(content(&entry)?);
            value["content_encoding"] = json!("utf-8");
        }
    }
    if entry.path.file_name().is_some_and(|n| n == "SKILL.md") {
        let skill_dir = entry.path.parent().unwrap();
        let manifest_path = skill_dir.join("skill.json");
        if manifest_path.exists() {
            safe_member(
                Path::new("/"),
                manifest_path
                    .strip_prefix("/")
                    .unwrap()
                    .to_str()
                    .ok_or_else(|| invalid("Non-UTF8 manifest"))?,
                true,
            )?;
        }
        let manifest = crate::control_skills::read_skill_manifest(skill_dir)?;
        let mut faults = Vec::<String>::new();
        if let Some(manifest) = &manifest {
            if skill_dir.file_name().and_then(|s| s.to_str()) != Some(&manifest.name) {
                faults.push("Manifest name differs from source directory".into());
            }
            if manifest.standing == crate::control_skills::SkillStanding::Retired
                && manifest
                    .retirement
                    .as_ref()
                    .is_none_or(|r| r.retirement_reason.trim().is_empty())
            {
                faults.push("Retirement lacks its reason".into());
            }
        }
        let allowed = manifest
            .as_ref()
            .is_some_and(|m| m.standing == crate::control_skills::SkillStanding::Active)
            && faults.is_empty();
        value["projection"] =
            json!({"schema":"central.skill-projection/v1","allowed":allowed,"faults":faults});
        value["skill_manifest"] = serde_json::to_value(manifest)?;
        value["skill_directory"] = json!(skill_dir);
    }
    if !context_allows(all, selected(all, input)?, &entry.source.source_ref)? {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Source disclosure changed during read",
        ));
    }
    let (_, current) = lookup(all, &entry.source.source_ref)?;
    if current.path != entry.path || current.revision != entry.revision {
        return Err(conflict("Source binding changed during read"));
    }
    Ok(value)
}
fn inspect(all: &[Scope], input: &Value) -> io::Result<Value> {
    let scope = selected(all, input)?;
    let backend = Backend::new(&scope.root);
    let version = backend.version();
    let mut choices = vec![scope];
    let mut linked_refs = BTreeSet::new();
    if input["federated"] == true {
        choices = all.iter().collect();
    } else {
        for (_, entry) in verified_links(all, scope)? {
            linked_refs.insert(entry.source.source_ref.clone());
            if let Some(owner) = all.iter().find(|owner| owner.world == entry.world_ref) {
                if !choices.iter().any(|chosen| chosen.world == owner.world) {
                    choices.push(owner);
                }
            }
        }
    }
    let initialized: Vec<_> = choices
        .iter()
        .filter(|s| Backend::new(&s.root).present())
        .collect();
    let available = version.is_ok() && !initialized.is_empty();
    // A federated hybrid query must not silently drop maps without embeddings.
    let hybrid = available
        && initialized
            .iter()
            .all(|s| s.index().is_ok_and(|i| i.embeddings));
    let excluded = context_exclusions(all, scope)?;
    let mut resources = Vec::new();
    for chosen in &choices {
        for entry in entries(chosen)? {
            if input["federated"] != true
                && chosen.world != scope.world
                && !linked_refs.contains(&entry.source.source_ref)
            {
                continue;
            }
            if policy_allows(&excluded, &entry.source.source_ref) {
                resources.push(entry);
            }
        }
    }
    let native_record = if let Some(id) = input["record_id"].as_i64() {
        let row = backend
            .records()?
            .into_iter()
            .find(|row| native::id(row).ok() == Some(id))
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Native bookmark not found"))?;
        Some(json!({"record":row,"revision":native::record_revision(&row)?}))
    } else {
        None
    };
    Ok(
        json!({"native_record":native_record,"world_ref":scope.world,"revision":scope.basis()?,
        "scopes":all.iter().map(|s|json!({"world_ref":s.world,"project":s.project,"path":s.root})).collect::<Vec<_>>(),
        "resources":resources,"links":scope.ground()?.links,"database":backend.db(),
        "provider":{"available":available,"version":version.as_ref().ok().map(|v|v.trim()),
        "tested_version":native::VERSION,"fulltext":available,"hybrid":hybrid,"semantic":false,
        "reason":version.err().map(|e|e.to_string())}}),
    )
}

fn link(all: &[Scope], input: &Value) -> io::Result<Value> {
    let scope = selected(all, input)?;
    expect_basis(scope, input)?;
    let (_, target) = lookup(all, text(input, "source_ref")?)?;
    let raw = text(input, "path")?;
    let member = relative(raw)?;
    if let Some(parent) = member.parent().filter(|p| !p.as_os_str().is_empty()) {
        reject_symlink_components(&scope.root, parent)?;
    }
    let dest = scope.root.join(member);
    if target.kind == "directory" && dest.starts_with(&target.path) {
        return Err(invalid(
            "A directory link cannot be placed inside its own target",
        ));
    }
    if target.kind == "directory" {
        let mut queue = vec![target.path.clone()];
        let mut seen = BTreeSet::new();
        while let Some(directory) = queue.pop() {
            if !seen.insert(directory.clone()) {
                continue;
            }
            if dest.starts_with(&directory) {
                return Err(invalid(
                    "Managed directory links would form a traversal cycle",
                ));
            }
            for world in all {
                for (path, existing) in world.ground()?.links {
                    if world.root.join(&path).starts_with(&directory) {
                        let (_, next) = lookup(all, &existing.source_ref)?;
                        if next.kind == "directory" {
                            queue.push(next.path);
                        }
                    }
                }
            }
            if seen.len() > MAX_ENTRIES {
                return Err(invalid("Link traversal exceeds cycle-check bound"));
            }
        }
    }
    let owner = text(input, "owner")?;
    let mut ground = scope.ground()?;
    if let Some(existing) = ground.links.get(raw) {
        let metadata = fs::symlink_metadata(&dest)?;
        if existing.source_ref != target.source.source_ref
            || existing.owner != owner
            || metadata.dev() != existing.device
            || metadata.ino() != existing.inode
            || fs::read_link(&dest)? != Path::new(&existing.target)
        {
            return Err(conflict("Managed link changed or belongs to another owner"));
        }
        return Ok(json!({"source_ref":existing.source_ref,"path":dest,"changed":false}));
    }
    if fs::symlink_metadata(&dest).is_ok() {
        return Err(conflict(
            "Link destination already exists; it is not replaced",
        ));
    }
    let parent = relative(raw)?.parent().unwrap();
    safe_directory(&scope.root, parent)?;
    let relative_target = relative_between(dest.parent().unwrap(), &target.path);
    std::os::unix::fs::symlink(&relative_target, &dest)?;
    let metadata = fs::symlink_metadata(&dest)?;
    ground.links.insert(
        raw.into(),
        Link {
            source_ref: target.source.source_ref.clone(),
            world_ref: target.world_ref.clone(),
            owner: owner.into(),
            target: relative_target.to_string_lossy().into(),
            device: metadata.dev(),
            inode: metadata.ino(),
        },
    );
    if let Err(error) = scope.save(&ground, scope.document()?) {
        // Undo only the inode just created; never remove a foreign replacement.
        if fs::symlink_metadata(&dest)
            .is_ok_and(|m| m.dev() == metadata.dev() && m.ino() == metadata.ino())
        {
            let _ = fs::remove_file(&dest);
        }
        return Err(error);
    }
    Ok(
        json!({"source_ref":target.source.source_ref,"world_ref":target.world_ref,"path":dest,"changed":true,"revision":scope.basis()?}),
    )
}
pub(crate) fn relative_between(from: &Path, to: &Path) -> PathBuf {
    let a: Vec<_> = from.components().collect();
    let b: Vec<_> = to.components().collect();
    let common = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
    let mut result = PathBuf::new();
    for _ in common..a.len() {
        result.push("..");
    }
    for p in &b[common..] {
        result.push(p);
    }
    result
}
pub fn execute(root: &Path, op: &str, input: &Value) -> io::Result<Value> {
    let root = root.canonicalize()?;
    // Reads do not acquire a filesystem lock, create a DB or reconcile state.
    // Mutation is serialized in one existing owner lock namespace.
    let _lock = if matches!(
        op,
        "inspect" | "search" | "resolve" | "locate" | "projection-read" | "skill-tree"
    ) {
        None
    } else {
        Some(crate::source_safety::lock(&root, "file-map.lock")?)
    };
    let all = if matches!(op, "move-apply" | "move-rollback") {
        super::file_map_moves::resume_scopes(&root, input)?
    } else {
        scopes(&root)?
    };
    let scope = selected(&all, input)?;
    let mut locks = Vec::new();
    if !matches!(
        op,
        "inspect" | "search" | "resolve" | "locate" | "projection-read" | "skill-tree"
    ) {
        let mut roots: Vec<_> = if op.starts_with("move-") {
            all.iter().map(|s| s.root.clone()).collect()
        } else {
            vec![scope.root.clone()]
        };
        roots.sort();
        roots.dedup();
        for owner_root in roots {
            if owner_root != root {
                locks.push(crate::source_safety::lock(&owner_root, "file-map.lock")?);
            }
        }
    }
    // Ground edits share the source owner's lock with its other native writers.
    // A file-map-only lock must not race document/source relation publication.
    let mut source_locks = Vec::new();
    if matches!(
        op,
        "register" | "link" | "scope-register" | "move-plan" | "move-apply" | "move-rollback"
    ) {
        let mut roots: Vec<_> = if op.starts_with("move-") {
            all.iter().map(|s| s.root.clone()).collect()
        } else {
            vec![scope.root.clone()]
        };
        roots.sort();
        roots.dedup();
        for owner_root in roots {
            source_locks.push(crate::source_safety::lock(
                &owner_root,
                "source-mutation.lock",
            )?);
        }
    }
    let data = match op {
        "skill-tree" => super::file_map_skills::tree(&all, input)?,
        "projection-record" => super::file_map_projection::execute(&all, input, true)?,
        "projection-read" => super::file_map_projection::execute(&all, input, false)?,
        "adopt-db" => super::file_map_adoption::adopt(scope, input)?,
        "record-adopt" => super::file_map_adoption::record_adopt(scope, input)?,
        "scope-register" => super::file_map_adoption::scope_register(&all, input)?,
        "locate" => locate(&all, input)?,
        "inspect" => inspect(&all, input)?,
        "search" => search(&all, input)?,
        "resolve" => resolve(&all, input)?,
        "move-plan" => super::file_map_moves::plan(&root, &all, input)?,
        "move-apply" => super::file_map_moves::apply(&root, &all, input, false)?,
        "move-rollback" => super::file_map_moves::apply(&root, &all, input, true)?,
        "register" => register_source(&all, scope, input)?,
        "refresh" => refresh(scope, input["embeddings"] == true)?,
        "link" => link(&all, input)?,
        _ => return Err(invalid("Unknown file-map operation")),
    };
    Ok(
        json!({"schema":SCHEMA,"operation":op,"result":data,"automatic_agent_or_model_invocation":false}),
    )
}
fn action(op: &str, input: &Value, context: &ActionExecutionContext<'_>) -> ActionResult {
    let id = format!("central.file-map.{op}");
    let result = crate::root::resolve_central_root(context.root_options)
        .map_err(io::Error::other)
        .and_then(|r| execute(&r.path, op, input));
    match result {
        Ok(data) => ActionResult::success(&id, data),
        Err(e) => ActionResult::failure_coded(
            Some(&id),
            ResultStatus::InvalidInput,
            match e.kind() {
                io::ErrorKind::NotFound => "central.file_map_not_found",
                io::ErrorKind::PermissionDenied => "central.file_map_denied",
                io::ErrorKind::AlreadyExists => "central.file_map_conflict",
                _ => "central.file_map_failure",
            },
            e.to_string(),
            None,
        ),
    }
}
pub fn register(registry: &mut ActionRegistry) {
    for op in [
        "skill-tree",
        "projection-record",
        "projection-read",
        "adopt-db",
        "record-adopt",
        "scope-register",
        "inspect",
        "register",
        "refresh",
        "search",
        "resolve",
        "link",
        "move-plan",
        "move-apply",
        "move-rollback",
        "locate",
    ] {
        let handler: crate::action::ActionHandler = match op {
            "skill-tree" => |_, i, c| action("skill-tree", i, c),
            "projection-record" => |_, i, c| action("projection-record", i, c),
            "projection-read" => |_, i, c| action("projection-read", i, c),
            "adopt-db" => |_, i, c| action("adopt-db", i, c),
            "record-adopt" => |_, i, c| action("record-adopt", i, c),
            "scope-register" => |_, i, c| action("scope-register", i, c),
            "locate" => |_, i, c| action("locate", i, c),
            "move-plan" => |_, i, c| action("move-plan", i, c),
            "move-apply" => |_, i, c| action("move-apply", i, c),
            "move-rollback" => |_, i, c| action("move-rollback", i, c),
            "inspect" => |_, i, c| action("inspect", i, c),
            "register" => |_, i, c| action("register", i, c),
            "refresh" => |_, i, c| action("refresh", i, c),
            "search" => |_, i, c| action("search", i, c),
            "resolve" => |_, i, c| action("resolve", i, c),
            _ => |_, i, c| action("link", i, c),
        };
        let inputs = [
            "selected_capsules",
            "source_revision",
            "tree_revision",
            "generation",
            "projection_kind",
            "content_encoding",
            "database",
            "record_id",
            "record_revision",
            "name",
            "project",
            "path",
            "source_ref",
            "expected_revision",
            "title",
            "owner",
            "query",
            "mode",
            "limit",
            "tags",
            "content",
            "federated",
            "allow_external",
            "native_import",
            "embeddings",
            "allowed_sources",
            "destination",
            "plan_id",
            "quiesced",
        ]
        .iter()
        .map(|name| ActionInputDefinition {
            name: (*name).into(),
            input_type: match *name {
                "limit" | "record_id" => "number",
                "tags" | "allowed_sources" | "selected_capsules" => "array",
                "content" | "federated" | "allow_external" | "native_import" | "embeddings"
                | "quiesced" => "boolean",
                _ => "string",
            }
            .into(),
            required: false,
            choices: None,
            selection: None,
        })
        .collect();
        registry.register(ActionDescriptor{id:format!("central.file-map.{op}"),title:format!("File map {op}"),description:"Persistent bkmr file map; source and placement meaning remain in Central. Reads never rebuild the owner's database or execute openers. Writes require current source-relations revision.".into(),inputs,output:ActionOutputDefinition{output_type:SCHEMA.into()},mutation_class:if matches!(op,"inspect"|"search"|"resolve"|"locate"|"projection-read"|"skill-tree"){MutationClass::ReadOnly}else{MutationClass::LocallyMutating},preview_supported:false,required_ports:vec![],availability:ActionAvailability{available:true,reason:None}},handler).expect("unique file map Action");
    }
}
