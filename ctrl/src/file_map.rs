//! Persistent Central/ProjectCentral file maps. bkmr owns its records and search;
//! existing source relations own source identity, locations and managed links.
use crate::action::{
    ActionAvailability, ActionDescriptor, ActionExecutionContext, ActionInputDefinition,
    ActionOutputDefinition, ActionRegistry, MutationClass,
};
use crate::file_map_backend::{self as native, Backend};
use crate::result::{ActionResult, ResultStatus};
use crate::source_horizon;
use crate::source_safety::reject_symlink_components;
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
#[cfg(test)]
type BeforeReadAcknowledgement = Box<dyn FnOnce()>;
#[cfg(test)]
thread_local! {
    static BEFORE_READ_ACKNOWLEDGEMENT: std::cell::RefCell<Option<BeforeReadAcknowledgement>> = const { std::cell::RefCell::new(None) };
}
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
fn link_descriptors(
    all: &[Scope],
    scope: &Scope,
    requested: Option<&str>,
) -> io::Result<Vec<(PathBuf, Entry)>> {
    let mut result = Vec::new();
    for (relative, link) in scope.ground()?.links {
        if requested.is_some_and(|reference| reference != link.source_ref) {
            continue;
        }
        let member = relative_member(&scope.root, &relative)?;
        let metadata = fs::symlink_metadata(&member)?;
        if !metadata.file_type().is_symlink()
            || metadata.dev() != link.device
            || metadata.ino() != link.inode
            || fs::read_link(&member)? != Path::new(&link.target)
        {
            return Err(conflict("Registered link was replaced or redirected"));
        }
        let candidate = binding_by_ref(all, &link.source_ref)?.ok_or_else(|| {
            read_refusal(
                io::ErrorKind::NotFound,
                "Registered link owner is unavailable",
                "known",
                "link_metadata",
                "unavailable",
            )
        })?;
        let source = candidate.observe()?;
        if source.world_ref != link.world_ref || member.canonicalize()? != source.path {
            return Err(conflict("Registered link no longer reaches its source"));
        }
        result.push((member, source));
    }
    Ok(result)
}
pub(crate) fn verified_links(all: &[Scope], scope: &Scope) -> io::Result<Vec<(PathBuf, Entry)>> {
    link_descriptors(all, scope, None)?
        .into_iter()
        .map(|(path, source)| Ok((path, with_revision(source)?)))
        .collect()
}
fn binding_only(input: &Value) -> io::Result<bool> {
    let selected = match input.get("binding_only") {
        None => false,
        Some(value) => value
            .as_bool()
            .ok_or_else(|| invalid("binding_only must be a boolean"))?,
    };
    if selected && (input["content"] == true || input.get("expected_revision").is_some()) {
        return Err(invalid(
            "binding_only cannot request content or a payload expected_revision",
        ));
    }
    Ok(selected)
}
fn read_admission(all: &[Scope], candidate: &BindingCandidate, input: &Value) -> io::Result<()> {
    let scope = selected(all, input)?;
    if input["project"].is_string()
        && scope.world != candidate.scope.world
        && !link_descriptors(all, scope, Some(&candidate.source.source_ref))?
            .iter()
            .any(|(_, entry)| entry.source.source_ref == candidate.source.source_ref)
    {
        return Err(read_refusal(
            io::ErrorKind::PermissionDenied,
            "Source belongs to another Project scope and has no declared link here",
            "known",
            "project_admission",
            "withheld",
        ));
    }
    if !context_allows(all, scope, &candidate.source.source_ref)
        .map_err(|error| read_failure(error, "known", "context_admission", "unavailable"))?
    {
        return Err(read_refusal(
            io::ErrorKind::PermissionDenied,
            "Source excluded by requesting World's effective source relations",
            "known",
            "context_admission",
            "withheld",
        ));
    }
    if let Some(world) = all.iter().find(|scope| scope.world == "control:root") {
        let path = candidate.path()?;
        if path.starts_with(&world.root)
            && !source_horizon::retrieval_admission(&world.root, &path).map_err(|error| {
                read_failure(error, "known", "world_source_admission", "unavailable")
            })?
        {
            return Err(read_refusal(
                io::ErrorKind::PermissionDenied,
                "Source excluded by current enclosing World treatment",
                "known",
                "world_source_admission",
                "withheld",
            ));
        }
    }
    Ok(())
}
fn relative_member(root: &Path, raw: &str) -> io::Result<PathBuf> {
    let member = relative(raw)?;
    if let Some(parent) = member.parent().filter(|p| !p.as_os_str().is_empty()) {
        reject_symlink_components(root, parent)?;
    }
    Ok(root.join(member))
}
fn locate(all: &[Scope], input: &Value) -> io::Result<Value> {
    let metadata_only = binding_only(input)?;
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
        let Some(member) = lexical
            .strip_prefix(&scope.root)
            .ok()
            .and_then(Path::to_str)
        else {
            continue;
        };
        if let Some(link) = scope.ground()?.links.get(member) {
            let actual = link_descriptors(all, scope, Some(&link.source_ref))?;
            if !actual.iter().any(|(path, _)| *path == lexical) {
                return Err(conflict("Selected registered link changed"));
            }
            let mut request = input.clone();
            request
                .as_object_mut()
                .ok_or_else(|| invalid("Expected object input"))?
                .remove("project");
            request["source_ref"] = json!(link.source_ref);
            let mut result = resolve(all, &request)?;
            result["encountered_link"] = json!({"path":lexical,"world_ref":scope.world});
            return Ok(result);
        }
    }
    if let Some(candidate) = binding_by_path(all, &lexical)? {
        let mut request = input.clone();
        request["source_ref"] = json!(candidate.source.source_ref);
        return resolve(all, &request);
    }
    require_no_unobserved_route(all, None, Some(&lexical))?;
    // An ordinary unregistered location is not a retrieval grant. Validate its
    // actual route, current treatment and form without opening its byte body.
    let target = if Path::new(raw).is_absolute() {
        safe_member(Path::new("/"), raw.trim_start_matches('/'), true)?
    } else {
        safe_member(&selected(all, input)?.root, raw, true)?
    };
    if !source_horizon::retrieval_admission(Path::new("/"), &target)? {
        return Err(read_refusal(
            io::ErrorKind::PermissionDenied,
            "Location is excluded by current source treatment",
            "unknown",
            "source_admission",
            "withheld",
        ));
    }
    let form = fs::symlink_metadata(&target)?;
    if !(form.is_file() || form.is_dir()) {
        return Err(invalid("Unregistered material form is unsupported"));
    }
    if binding_by_path(all, &target)?.is_some() {
        return Err(conflict("Location ownership changed during observation"));
    }
    if metadata_only {
        target
            .to_str()
            .ok_or_else(|| invalid("Location cannot be represented by native JSON"))?;
        return Ok(json!({"ownership":"unregistered","binding_only":true,"requested_path":raw}));
    }
    Err(io::Error::new(
        io::ErrorKind::NotFound,
        "Location is unregistered",
    ))
}
pub(crate) fn resolve(all: &[Scope], input: &Value) -> io::Result<Value> {
    let metadata_only = binding_only(input)?;
    let reference = text(input, "source_ref")?;
    let candidate = match binding_by_ref(all, reference)? {
        Some(candidate) => candidate,
        None => {
            require_no_unobserved_route(all, Some(reference), None)?;
            if binding_by_ref(all, reference)?.is_some() {
                return Err(conflict("Source ownership changed during observation"));
            }
            return if metadata_only {
                Ok(json!({"ownership":"unregistered","binding_only":true,"source_ref":reference}))
            } else {
                Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    "Source is outside participating maps",
                ))
            };
        }
    };
    read_admission(all, &candidate, input)?;
    let mut entry = candidate.observe()?;
    let original_metadata = entry.clone();
    let mut value = if metadata_only {
        let mut descriptor = serde_json::to_value(&entry)?;
        descriptor
            .as_object_mut()
            .ok_or_else(|| invalid("Invalid source descriptor"))?
            .remove("revision");
        descriptor["ownership"] = json!("owned");
        descriptor["binding_only"] = json!(true);
        descriptor["relation_revision"] = json!(candidate.relation_revision);
        descriptor["material_metadata_basis"] = metadata_basis(&entry)?;
        descriptor
    } else {
        entry = with_revision(entry)
            .map_err(|error| read_failure(error, "known", "payload_revision", "unavailable"))?;
        if input["expected_revision"]
            .as_str()
            .is_some_and(|expected| expected != entry.revision)
        {
            return Err(conflict("Source revision changed"));
        }
        serde_json::to_value(&entry)?
    };
    value["project"] = json!(candidate.scope.project);
    if !metadata_only && input["content"] == true {
        if input["content_encoding"] == "base64" {
            use base64::Engine;
            value["content"] =
                json!(
                    base64::engine::general_purpose::STANDARD.encode(payload(&entry).map_err(
                        |error| read_failure(error, "known", "payload_read", "unavailable")
                    )?)
                );
            value["content_encoding"] = json!("base64");
        } else {
            value["content"] = json!(content(&entry).map_err(|error| read_failure(
                error,
                "known",
                "payload_read",
                "unavailable"
            ))?);
            value["content_encoding"] = json!("utf-8");
        }
    }
    if !metadata_only && entry.path.file_name().is_some_and(|n| n == "SKILL.md") {
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
        let manifest = crate::control_skills::read_skill_manifest(skill_dir)
            .map_err(|error| read_failure(error, "known", "skill_projection", "unavailable"))?;
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
    let current = binding_by_ref(all, reference)?.ok_or_else(|| {
        read_refusal(
            io::ErrorKind::AlreadyExists,
            "Source ownership disappeared during read",
            "known",
            "final_binding",
            "unavailable",
        )
    })?;
    read_admission(all, &current, input)?;
    let mut observed = current.observe()?;
    if current.scope.world != candidate.scope.world
        || current.scope.root != candidate.scope.root
        || current.relation_revision != candidate.relation_revision
        || current.selection_metadata_basis != candidate.selection_metadata_basis
        || observed.path != entry.path
        || observed.source != entry.source
        || observed.kind != entry.kind
    {
        return Err(read_refusal(
            io::ErrorKind::AlreadyExists,
            "Source binding changed during read",
            "known",
            "final_binding",
            "unavailable",
        ));
    }
    if metadata_only {
        if observed.revision != original_metadata.revision {
            return Err(conflict("Source metadata changed during read"));
        }
        metadata_basis(&observed)?;
    } else {
        observed = with_revision(observed).map_err(|error| {
            read_failure(error, "known", "final_payload_revision", "unavailable")
        })?;
        if observed.revision != entry.revision {
            return Err(conflict("Source binding changed during read"));
        }
    }
    Ok(value)
}
fn inspect(all: &[Scope], input: &Value) -> io::Result<Value> {
    let scope = selected(all, input)?;
    let backend = Backend::for_scope(scope);
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
        .filter(|s| Backend::for_scope(s).present())
        .collect();
    let available = version.is_ok() && !initialized.is_empty();
    // A federated hybrid query must not silently drop maps without embeddings.
    let hybrid = available
        && initialized
            .iter()
            .all(|s| s.index().is_ok_and(|i| i.embeddings));
    let excluded = context_exclusions(all, scope)?;
    let mut resources = Vec::new();
    // `{"resources": false}` turns inspect into a cheap capability and scope
    // probe: attachments that only need provider state skip the full walk.
    let want_resources = input["resources"] != false;
    if want_resources {
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
        "scopes":all.iter().map(|s| {
            let pooled = s.ground().map(|g| g.content_pool.enabled).unwrap_or(false);
            json!({"world_ref":s.world,"project":s.project,"path":s.root,"content_pool":pooled})
        }).collect::<Vec<_>>(),
        "resources":resources,"links":scope.ground()?.links,"database":backend.db(),
        "provider":{"available":available,"version":version.as_ref().ok().map(|v|v.trim()),
        "tested_version":native::VERSION,"fulltext":available,"hybrid":hybrid,"semantic":hybrid,
        "embedding_model":native::embedding_model(),
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
    if let Err(error) = scope
        .document()
        .and_then(|document| scope.save(&ground, document))
    {
        if crate::file_mutation::record_publication_observation(&error).is_some() {
            return Err(crate::file_mutation::record_owner_error(
                error,
                "file_map.link_relation_published",
                Some(&target.source.source_ref),
                None,
                "source_not_changed_link_created",
            ));
        }
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
fn selected_metadata_basis(
    all: &[Scope],
    op: &str,
    input: &Value,
) -> io::Result<Option<(String, String, Option<String>)>> {
    let candidate = if op == "resolve" {
        binding_by_ref(all, text(input, "source_ref")?)?
    } else {
        let raw = text(input, "path")?;
        let path = if Path::new(raw).is_absolute() {
            PathBuf::from(raw)
        } else {
            selected(all, input)?.root.join(relative(raw)?)
        };
        let mut linked = None;
        for scope in all {
            if input["project"].is_string() && selected(all, input)?.world != scope.world {
                continue;
            }
            let Some(member) = path.strip_prefix(&scope.root).ok().and_then(Path::to_str) else {
                continue;
            };
            if let Some(link) = scope.ground()?.links.get(member) {
                linked = Some(link.source_ref.clone());
                break;
            }
        }
        if let Some(reference) = linked {
            binding_by_ref(all, &reference)?
        } else {
            binding_by_path(all, &path)?
        }
    };
    Ok(candidate.map(|candidate| {
        (
            candidate.scope.world,
            candidate.source.source_ref,
            candidate.selection_metadata_basis,
        )
    }))
}

pub fn execute(root: &Path, op: &str, input: &Value) -> io::Result<Value> {
    let read_root = if matches!(op, "resolve" | "locate") {
        Some(ReadRoot::capture(root)?)
    } else {
        None
    };
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
    let read_roots = if read_root.is_some() {
        all.iter()
            .map(|scope| ReadRoot::capture(&scope.root))
            .collect::<io::Result<Vec<_>>>()?
    } else {
        Vec::new()
    };
    let retained_bases = if read_root.is_some() {
        ownership_bases(&all)?
    } else {
        Vec::new()
    };

    let mut locks = Vec::new();
    if !matches!(
        op,
        "inspect" | "search" | "resolve" | "locate" | "projection-read" | "skill-tree"
    ) {
        let mut roots: Vec<_> = if op.starts_with("move-") {
            all.iter().map(|s| s.state.clone()).collect()
        } else {
            vec![scope.state.clone()]
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
            all.iter().map(|s| s.state.clone()).collect()
        } else {
            vec![scope.state.clone()]
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
    if let Some(original) = &read_root {
        original.validate()?;
        for owner in &read_roots {
            owner.validate()?;
        }
        if ownership_bases(&all)? != retained_bases {
            return Err(conflict("Native declarations changed before selected read"));
        }
    }
    let retained_selected_metadata = if read_root.is_some() {
        selected_metadata_basis(&all, op, input)?
    } else {
        None
    };
    let data = match op {
        "skill-tree" => super::file_map_skills::tree(&all, input)?,
        "projection-record" => super::file_map_projection::execute(&all, input, true)?,
        "projection-read" => super::file_map_projection::execute(&all, input, false)?,
        "adopt-db" => super::file_map_adoption::adopt(scope, input)?,
        "record-adopt" => super::file_map_adoption::record_adopt(scope, input)?,
        "scope-register" => super::file_map_adoption::scope_register(&all, input)?,
        "pool" => super::file_map_catalog::pool(&all, input)?,
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
    if let Some(original) = &read_root {
        #[cfg(test)]
        {
            let checkpoint =
                BEFORE_READ_ACKNOWLEDGEMENT.with(|checkpoint| checkpoint.borrow_mut().take());
            if let Some(checkpoint) = checkpoint {
                checkpoint();
            }
        }
        original.validate()?;
        for owner in &read_roots {
            owner.validate()?;
        }
        let current = scopes(&root)?;
        if current.len() != all.len()
            || current.iter().zip(&all).any(|(current, retained)| {
                current.world != retained.world
                    || current.root != retained.root
                    || current.project != retained.project
            })
        {
            return Err(conflict("Participating owner scopes changed during read"));
        }
        if ownership_bases(&current)? != retained_bases {
            return Err(conflict(
                "Native ownership declarations changed before acknowledgement",
            ));
        }
        // Reuse the same read after all operation checkpoints. No descriptor
        // remains open for each item in a World; only the selected read is
        // repeated. Even an unregistered response requires a current route.
        if selected_metadata_basis(&current, op, input)? != retained_selected_metadata {
            return Err(conflict(
                "Selected native metadata changed before acknowledgement",
            ));
        }
        let qualified = if op == "resolve" {
            resolve(&current, input)?
        } else {
            locate(&current, input)?
        };
        if selected_metadata_basis(&current, op, input)? != retained_selected_metadata {
            return Err(conflict(
                "Selected native metadata changed during final qualification",
            ));
        }
        if qualified != data {
            return Err(conflict(
                "Native read result changed before acknowledgement",
            ));
        }
        for owner in &read_roots {
            owner.validate()?;
        }
        if ownership_bases(&current)? != retained_bases {
            return Err(conflict(
                "Native ownership declarations changed during final qualification",
            ));
        }
        original.validate()?;
    }
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
        Err(e) if crate::file_mutation::record_failure_result(&id, &e).is_some() => {
            crate::file_mutation::record_failure_result(&id, &e)
                .expect("matched native record failure")
        }
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
            if matches!(op, "resolve" | "locate") {
                Some(read_failure_details(&e))
            } else {
                None
            },
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
        "pool",
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
            "pool" => |_, i, c| action("pool", i, c),
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
        let mut inputs: Vec<_> = [
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
        if matches!(op, "resolve" | "locate") {
            inputs.push(ActionInputDefinition {
                name: "binding_only".into(),
                input_type: "boolean".into(),
                required: false,
                choices: None,
                selection: None,
            });
        }
        registry.register(ActionDescriptor{id:format!("central.file-map.{op}"),title:format!("File map {op}"),description:"Persistent bkmr file map; source and placement meaning remain in Central. Reads never rebuild the owner's database or execute openers. Writes require current source-relations revision.".into(),inputs,output:ActionOutputDefinition{output_type:SCHEMA.into()},mutation_class:if matches!(op,"inspect"|"search"|"resolve"|"locate"|"projection-read"|"skill-tree"){MutationClass::ReadOnly}else{MutationClass::LocallyMutating},preview_supported:false,required_ports:vec![],availability:ActionAvailability{available:true,reason:None}},handler).expect("unique file map Action");
    }
}

#[cfg(test)]
mod binding_read_tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static NEXT: AtomicU64 = AtomicU64::new(0);
    struct Fixture {
        root: PathBuf,
        source: PathBuf,
        relations: PathBuf,
    }
    impl Fixture {
        fn new() -> Self {
            let scratch = Path::new(env!("CARGO_MANIFEST_DIR")).join("../ProjectCentral/now/tmp");
            fs::create_dir_all(&scratch).unwrap();
            let root = scratch.join(format!(
                "file-map-binding-{}-{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&root).unwrap();
            fs::create_dir_all(root.join("Control/user")).unwrap();
            let source = root.join("Control/user/note.md");
            fs::write(&source, "retained actual body\n").unwrap();
            let relations = root.join(source_horizon::CONTROL_GROUND_RELATIONS_SOURCE);
            fs::create_dir_all(relations.parent().unwrap()).unwrap();
            fs::write(&relations, serde_json::to_vec(&json!({
                "schema":source_horizon::CONTROL_GROUND_RELATIONS_SCHEMA,"project_id":"control:root",
                "extension":{"retained":true},"relations":[{"ref":"opaque:fixture:note","path":"Control/user/note.md",
                "roles":["agent-governance-source"],"provenance":"unresolved","standing":"unspecified",
                "treatment":"retain-native-in-place"}]
            })).unwrap()).unwrap();
            Self {
                root,
                source,
                relations,
            }
        }
        fn request(&self, metadata_only: bool) -> Value {
            json!({"source_ref":"opaque:fixture:note","binding_only":metadata_only,"content":!metadata_only})
        }
        fn checkpoint(&self, callback: impl FnOnce() + 'static) {
            BEFORE_READ_ACKNOWLEDGEMENT.with(|slot| {
                assert!(slot.borrow().is_none());
                *slot.borrow_mut() = Some(Box::new(callback));
            });
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            BEFORE_READ_ACKNOWLEDGEMENT.with(|slot| slot.borrow_mut().take());
            if let Err(error) = fs::remove_dir_all(&self.root) {
                let message = format!(
                    "Owned file-map fixture cleanup failed at {}: {:?} errno {:?}: {}",
                    self.root.display(),
                    error.kind(),
                    error.raw_os_error(),
                    error
                );
                if std::thread::panicking() {
                    eprintln!("{message}");
                } else {
                    panic!("{message}");
                }
            }
        }
    }

    #[test]
    fn captured_payload_removal_before_acknowledgement_is_not_delivered() {
        let fixture = Fixture::new();
        let source = fixture.source.clone();
        fixture.checkpoint(move || fs::remove_file(source).unwrap());
        let error = execute(&fixture.root, "resolve", &fixture.request(false)).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::NotFound);
        assert_eq!(read_failure_details(&error)["ownership"], "known");
        assert!(fixture.relations.is_file());
    }
    #[test]
    fn captured_payload_same_inode_change_before_acknowledgement_conflicts() {
        let fixture = Fixture::new();
        let source = fixture.source.clone();
        let inode = fs::metadata(&source).unwrap().ino();
        fixture.checkpoint(move || fs::write(source, "changed actual body\n").unwrap());
        assert_eq!(
            execute(&fixture.root, "resolve", &fixture.request(false))
                .unwrap_err()
                .kind(),
            io::ErrorKind::AlreadyExists
        );
        assert_eq!(fs::metadata(&fixture.source).unwrap().ino(), inode);
        assert_eq!(fs::read(&fixture.source).unwrap(), b"changed actual body\n");
    }
    #[test]
    fn metadata_only_replacement_before_acknowledgement_conflicts_without_body_delivery() {
        let fixture = Fixture::new();
        let source = fixture.source.clone();
        let replacement = fixture.root.join("replacement");
        fs::write(&replacement, "retained actual body\n").unwrap();
        let inode = fs::metadata(&source).unwrap().ino();
        fixture.checkpoint(move || fs::rename(replacement, source).unwrap());
        assert_eq!(
            execute(&fixture.root, "resolve", &fixture.request(true))
                .unwrap_err()
                .kind(),
            io::ErrorKind::AlreadyExists
        );
        assert_ne!(fs::metadata(&fixture.source).unwrap().ino(), inode);
        assert_eq!(
            fs::read(&fixture.source).unwrap(),
            b"retained actual body\n"
        );
    }
    #[test]
    fn accepted_relation_change_before_acknowledgement_conflicts_and_preserves_source() {
        let fixture = Fixture::new();
        let path = fixture.relations.clone();
        fixture.checkpoint(move || {
            let mut value: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
            value["relations"][0]["standing"] = json!("changed-observed-standing");
            fs::write(path, serde_json::to_vec(&value).unwrap()).unwrap();
        });
        assert_eq!(
            execute(&fixture.root, "resolve", &fixture.request(true))
                .unwrap_err()
                .kind(),
            io::ErrorKind::AlreadyExists
        );
        assert_eq!(
            fs::read(&fixture.source).unwrap(),
            b"retained actual body\n"
        );
    }
    #[test]
    fn late_native_withdrawal_is_denied_and_unchanged_reopen_remains_useful() {
        let fixture = Fixture::new();
        let marker = fixture.root.join("Control/user/.no-agent-retrieval");
        let create = marker.clone();
        fixture.checkpoint(move || fs::write(create, b"").unwrap());
        let error = execute(&fixture.root, "resolve", &fixture.request(true)).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
        assert_eq!(read_failure_details(&error)["material_state"], "withheld");
        fs::remove_file(marker).unwrap();
        let result = execute(&fixture.root, "resolve", &fixture.request(true)).unwrap();
        assert_eq!(result["result"]["source"]["ref"], "opaque:fixture:note");
        assert!(result["result"].get("revision").is_none());
        assert_eq!(
            fs::read(&fixture.source).unwrap(),
            b"retained actual body\n"
        );
    }
    #[test]
    fn healthy_no_owner_does_not_survive_new_accepted_owner_before_acknowledgement() {
        let fixture = Fixture::new();
        let relations = fixture.relations.clone();
        fixture.checkpoint(move || {
            let mut value: Value = serde_json::from_slice(&fs::read(&relations).unwrap()).unwrap();
            value["relations"][0]["ref"] = json!("opaque:fixture:new");
            fs::write(relations, serde_json::to_vec(&value).unwrap()).unwrap();
        });
        assert_eq!(
            execute(
                &fixture.root,
                "resolve",
                &json!({"source_ref":"opaque:fixture:new","binding_only":true})
            )
            .unwrap_err()
            .kind(),
            io::ErrorKind::AlreadyExists
        );
    }
    #[test]
    fn original_root_alias_retarget_is_not_a_new_owner_acknowledgement() {
        let fixture = Fixture::new();
        let other = Fixture::new();
        let alias = fixture.root.join("owner-alias");
        std::os::unix::fs::symlink(&fixture.root, &alias).unwrap();
        let retarget = alias.clone();
        let destination = other.root.clone();
        fixture.checkpoint(move || {
            fs::remove_file(&retarget).unwrap();
            std::os::unix::fs::symlink(destination, retarget).unwrap();
        });
        assert_eq!(
            execute(&alias, "resolve", &fixture.request(true))
                .unwrap_err()
                .kind(),
            io::ErrorKind::AlreadyExists
        );
        assert_eq!(
            fs::read(&fixture.source).unwrap(),
            b"retained actual body\n"
        );
        assert_eq!(fs::read(&other.source).unwrap(), b"retained actual body\n");
    }

    #[test]
    fn registered_native_file_keeps_actual_owner_roles_and_standing() {
        let fixture = Fixture::new();
        fs::remove_file(&fixture.relations).unwrap();
        let before = fs::read(&fixture.source).unwrap();
        let inode = fs::metadata(&fixture.source).unwrap().ino();
        let native = source_horizon::control_source_bindings(&fixture.root)
            .unwrap()
            .into_iter()
            .find(|binding| binding.path == "Control/user/note.md")
            .unwrap();
        let registered = execute(&fixture.root, "register", &json!({"path":"Control/user/note.md", "expected_revision":"absent", "title":"retained useful title"})).unwrap();
        assert_eq!(registered["result"]["source_ref"], native.source_ref);
        let declaration = fs::read(&fixture.relations).unwrap();
        for metadata_only in [true, false] {
            let resolved = execute(
                &fixture.root,
                "resolve",
                &json!({"source_ref":native.source_ref,"binding_only":metadata_only}),
            )
            .unwrap();
            assert_eq!(
                resolved["result"]["source"],
                serde_json::to_value(&native).unwrap()
            );
            assert_eq!(resolved["result"]["title"], "retained useful title");
        }
        assert_eq!(fs::read(&fixture.source).unwrap(), before);
        assert_eq!(fs::metadata(&fixture.source).unwrap().ino(), inode);
        assert_eq!(fs::read(&fixture.relations).unwrap(), declaration);
    }

    #[test]
    fn registered_native_skill_keeps_manifest_authored_binding_without_body_revision_in_metadata_mode(
    ) {
        let fixture = Fixture::new();
        fs::remove_file(&fixture.relations).unwrap();
        let skill = fixture.root.join("Control/user/skills/actual");
        fs::create_dir_all(&skill).unwrap();
        fs::write(skill.join("SKILL.md"), "---\nname: actual\ndescription: Actual native test source\n---\nRead this retained source.\n").unwrap();
        fs::write(skill.join("skill.json"), serde_json::to_vec(&json!({"schema":"central.skill/v1","name":"actual","scope":"control-user","standing":"active","provenance":"human-authored"})).unwrap()).unwrap();
        let native = source_horizon::control_source_bindings(&fixture.root)
            .unwrap()
            .into_iter()
            .find(|binding| binding.path == "Control/user/skills/actual/SKILL.md")
            .unwrap();
        assert_eq!(native.provenance, "human-authored");
        assert_eq!(native.standing, "active");
        execute(
            &fixture.root,
            "register",
            &json!({"path":"Control/user/skills/actual/SKILL.md","expected_revision":"absent"}),
        )
        .unwrap();
        let before = fs::read(skill.join("SKILL.md")).unwrap();
        let result = execute(
            &fixture.root,
            "resolve",
            &json!({"source_ref":native.source_ref,"binding_only":true}),
        )
        .unwrap();
        assert_eq!(
            result["result"]["source"],
            serde_json::to_value(&native).unwrap()
        );
        assert!(result["result"].get("revision").is_none());
        assert!(result["result"].get("content").is_none());
        assert_eq!(fs::read(skill.join("SKILL.md")).unwrap(), before);
    }

    #[test]
    fn cross_scope_explicit_ref_cannot_hide_another_native_owner() {
        let fixture = Fixture::new();
        let project = fixture.root.join("Work/alpha");
        fs::create_dir_all(project.join("ProjectCentral/user")).unwrap();
        fs::write(project.join("ProjectCentral/project.json"), serde_json::to_vec(&json!({"schema":"central.project/v1","project_id":"alpha","human_source":"ProjectCentral/user","wiki":{"profile":"okf-wiki/v1","source":"ProjectCentral/agents/wiki/wiki.json"}})).unwrap()).unwrap();
        let body = project.join("ProjectCentral/user/native.md");
        fs::write(&body, b"actual foreign owner body\n").unwrap();
        let native = source_horizon::project_source_bindings(&project)
            .unwrap()
            .into_iter()
            .find(|binding| binding.path == "ProjectCentral/user/native.md")
            .unwrap();
        let mut root_doc: Value =
            serde_json::from_slice(&fs::read(&fixture.relations).unwrap()).unwrap();
        root_doc["relations"][0]["ref"] = json!(native.source_ref);
        fs::write(&fixture.relations, serde_json::to_vec(&root_doc).unwrap()).unwrap();
        let root_before = fs::read(&fixture.relations).unwrap();
        let project_before = fs::read(&body).unwrap();
        let error = execute(
            &fixture.root,
            "resolve",
            &json!({"source_ref":native.source_ref,"binding_only":true}),
        )
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(read_failure_details(&error)["ownership"], "known");
        assert_eq!(fs::read(&fixture.relations).unwrap(), root_before);
        assert_eq!(fs::read(&body).unwrap(), project_before);
    }

    #[test]
    fn selected_explicit_skill_source_does_not_parse_unrelated_fallback_manifest() {
        let fixture = Fixture::new();
        let skill = fixture.root.join("Control/user/skills/selected");
        fs::create_dir_all(&skill).unwrap();
        let body = skill.join("SKILL.md");
        let manifest = skill.join("skill.json");
        fs::write(&body, b"retained selected native Source body").unwrap();
        fs::write(&manifest, b"actual malformed unoverridden Skill metadata").unwrap();
        let mut doc: Value =
            serde_json::from_slice(&fs::read(&fixture.relations).unwrap()).unwrap();
        doc["relations"][0]["path"] = json!("Control/user/skills/selected/SKILL.md");
        fs::write(&fixture.relations, serde_json::to_vec(&doc).unwrap()).unwrap();
        let before = fs::read(&body).unwrap();
        let before_manifest = fs::read(&manifest).unwrap();
        let inode = fs::metadata(&body).unwrap().ino();
        let selected = execute(&fixture.root, "resolve", &fixture.request(true)).unwrap();
        assert_eq!(selected["result"]["source"]["ref"], "opaque:fixture:note");
        assert_eq!(
            selected["result"]["source"]["roles"],
            json!(["agent-governance-source"])
        );
        assert!(selected["result"].get("revision").is_none());
        assert!(
            source_horizon::control_source_bindings(&fixture.root).is_err(),
            "Bulk fallback must still reject the genuinely unoverridden malformed manifest member"
        );
        assert!(
            execute(&fixture.root, "resolve", &fixture.request(false)).is_err(),
            "Default Skill projection still needs its own native manifest"
        );
        assert_eq!(fs::read(&body).unwrap(), before);
        assert_eq!(fs::read(&manifest).unwrap(), before_manifest);
        assert_eq!(fs::metadata(&body).unwrap().ino(), inode);
    }

    #[test]
    fn selected_project_skill_uses_same_native_metadata_and_accepted_source_override() {
        let fixture = Fixture::new();
        let project = fixture.root.join("Work/alpha");
        let skill = project.join("ProjectCentral/user/skills/selected");
        fs::create_dir_all(&skill).unwrap();
        fs::write(
            project.join("ProjectCentral/project.json"),
            serde_json::to_vec(&json!({
            "schema":"central.project/v1","project_id":"alpha","human_source":"ProjectCentral/user",
            "wiki":{"profile":"okf-wiki/v1","source":"ProjectCentral/agents/wiki/wiki.json"}}))
            .unwrap(),
        )
        .unwrap();
        fs::write(skill.join("SKILL.md"), b"actual Project Skill Source body").unwrap();
        fs::write(skill.join("skill.json"),serde_json::to_vec(&json!({"schema":"central.skill/v1",
            "name":"selected","scope":"projectcentral-user","standing":"active","provenance":"adopted"})).unwrap()).unwrap();
        let native = source_horizon::project_source_bindings(&project)
            .unwrap()
            .into_iter()
            .find(|binding| binding.path == "ProjectCentral/user/skills/selected/SKILL.md")
            .unwrap();
        let reading = execute(
            &fixture.root,
            "resolve",
            &json!({"project":"alpha","source_ref":native.source_ref,"binding_only":true}),
        )
        .unwrap();
        assert_eq!(
            reading["result"]["source"],
            serde_json::to_value(&native).unwrap()
        );
        let relations = project.join(source_horizon::GROUND_RELATIONS_SOURCE);
        fs::create_dir_all(relations.parent().unwrap()).unwrap();
        fs::write(&relations,serde_json::to_vec(&json!({"schema":source_horizon::GROUND_RELATIONS_SCHEMA,"project_id":"alpha",
            "relations":[{"ref":"opaque:accepted-project-skill","path":"ProjectCentral/user/skills/selected/SKILL.md",
                "roles":["project-human-authored-source"],"provenance":"human-authored","standing":"accepted","treatment":"retain-native-in-place"}]})).unwrap()).unwrap();
        fs::write(
            skill.join("skill.json"),
            b"actual malformed other fallback metadata",
        )
        .unwrap();
        let source_before = fs::read(skill.join("SKILL.md")).unwrap();
        let reading = execute(&fixture.root,"resolve",&json!({"project":"alpha","source_ref":"opaque:accepted-project-skill","binding_only":true})).unwrap();
        assert_eq!(
            reading["result"]["source"]["ref"],
            "opaque:accepted-project-skill"
        );
        assert_eq!(reading["result"]["source"]["standing"], "accepted");
        assert!(source_horizon::project_source_bindings(&project).is_err());
        assert_eq!(fs::read(skill.join("SKILL.md")).unwrap(), source_before);
    }

    #[test]
    fn selected_skill_metadata_revision_is_reobserved_before_acknowledgement() {
        let fixture = Fixture::new();
        let skill = fixture.root.join("Control/user/skills/actual");
        fs::create_dir_all(&skill).unwrap();
        fs::write(skill.join("SKILL.md"), b"actual selected Skill body").unwrap();
        let metadata = skill.join("skill.json");
        fs::write(&metadata,serde_json::to_vec(&json!({"schema":"central.skill/v1","name":"actual",
            "scope":"control-user","standing":"active","provenance":"human-authored","retained":{"value":"before"}})).unwrap()).unwrap();
        let body_before = fs::read(skill.join("SKILL.md")).unwrap();
        fixture.checkpoint(move || {
            let mut doc: Value = serde_json::from_slice(&fs::read(&metadata).unwrap()).unwrap();
            doc["retained"]["value"] = json!("after");
            fs::write(&metadata, serde_json::to_vec(&doc).unwrap()).unwrap();
        });
        let reference =
            source_horizon::source_ref("control:root", "Control/user/skills/actual/SKILL.md");
        let error = execute(
            &fixture.root,
            "resolve",
            &json!({"source_ref":reference,"binding_only":true}),
        )
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(fs::read(skill.join("SKILL.md")).unwrap(), body_before);
        assert_eq!(
            execute(
                &fixture.root,
                "resolve",
                &json!({"source_ref":reference,"binding_only":true})
            )
            .unwrap()["result"]["source"]["standing"],
            "active"
        );
    }
}

#[cfg(test)]
mod external_scope_tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT: AtomicU64 = AtomicU64::new(0);
    /// A ground plus an external project folder beside it (outside the ground),
    /// registered the way ~/Documents/epi was: an absolute path in the root
    /// register's file_map.scopes.
    struct World {
        base: PathBuf,
        root: PathBuf,
        external: PathBuf,
    }
    impl World {
        fn new() -> Self {
            let scratch = Path::new(env!("CARGO_MANIFEST_DIR")).join("../ProjectCentral/now/tmp");
            fs::create_dir_all(&scratch).unwrap();
            let base = scratch.canonicalize().unwrap().join(format!(
                "file-map-external-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            let root = base.join("ground");
            let external = base.join("documents-epi");
            fs::create_dir_all(root.join("Control/user")).unwrap();
            fs::create_dir_all(external.join("ProjectCentral")).unwrap();
            fs::write(external.join("notes.md"), "owner material\n").unwrap();
            fs::write(
                external.join("ProjectCentral/project.json"),
                serde_json::to_vec(&json!({"schema":"central.project/v1","project_id":"epi",
                    "human_source":"ProjectCentral/user",
                    "wiki":{"profile":"okf-wiki/v1","source":"ProjectCentral/agents/wiki/wiki.json"}}))
                .unwrap(),
            )
            .unwrap();
            let relations = root.join(source_horizon::CONTROL_GROUND_RELATIONS_SOURCE);
            fs::create_dir_all(relations.parent().unwrap()).unwrap();
            fs::write(
                &relations,
                serde_json::to_vec(&json!({
                    "schema":source_horizon::CONTROL_GROUND_RELATIONS_SCHEMA,
                    "project_id":"control:root","relations":[],
                    "file_map":{"scopes":{"epi":external.to_str().unwrap()}}
                }))
                .unwrap(),
            )
            .unwrap();
            Self {
                base,
                root,
                external,
            }
        }
        /// Every path under the external tree, so a test can prove it unchanged.
        fn external_listing(&self) -> Vec<PathBuf> {
            let mut out = Vec::new();
            let mut stack = vec![self.external.clone()];
            while let Some(dir) = stack.pop() {
                for entry in fs::read_dir(&dir).unwrap() {
                    let path = entry.unwrap().path();
                    if path.is_dir() {
                        stack.push(path.clone());
                    }
                    out.push(path);
                }
            }
            out.sort();
            out
        }
    }
    impl Drop for World {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.base);
        }
    }

    #[test]
    fn external_scope_state_lives_in_the_ground() {
        let world = World::new();
        let all = scopes(&world.root).unwrap();
        let epi = all.iter().find(|s| s.world == "project:epi").unwrap();
        assert!(epi.is_external());
        assert_eq!(epi.state, world.root.join(".central/scopes/epi"));
        let inside = all.iter().find(|s| s.world == "control:root").unwrap();
        assert!(!inside.is_external());
    }

    #[test]
    fn writes_for_an_external_scope_never_touch_its_tree() {
        let world = World::new();
        let before = world.external_listing();
        execute(
            &world.root,
            "pool",
            &json!({"project":"epi","enable":false}),
        )
        .unwrap();
        let all = scopes(&world.root).unwrap();
        let epi = all.iter().find(|s| s.world == "project:epi").unwrap();
        epi.save_index(&epi.index().unwrap()).unwrap();
        assert_eq!(
            world.external_listing(),
            before,
            "external tree was written"
        );
        assert!(epi
            .state
            .join(source_horizon::GROUND_RELATIONS_SOURCE)
            .is_file());
        assert!(epi.state.join(".central/bkmr/bindings.json").is_file());
        assert!(!epi.ground().unwrap().content_pool.enabled);
        assert_eq!(
            super::super::file_map_backend::Backend::for_scope(epi).area,
            epi.state.join(".central/bkmr")
        );
    }

    #[test]
    fn external_relations_read_from_the_tree_until_the_ground_edits_them() {
        let world = World::new();
        let own = world.external.join(source_horizon::GROUND_RELATIONS_SOURCE);
        fs::create_dir_all(own.parent().unwrap()).unwrap();
        fs::write(
            &own,
            serde_json::to_vec(&json!({"schema":source_horizon::GROUND_RELATIONS_SCHEMA,
                "project_id":"epi","relations":[],"file_map":{"content_pool":{"enabled":true}}}))
            .unwrap(),
        )
        .unwrap();
        let all = scopes(&world.root).unwrap();
        let epi = all.iter().find(|s| s.world == "project:epi").unwrap();
        assert!(epi.ground().unwrap().content_pool.enabled);
        let mut ground = epi.ground().unwrap();
        ground.content_pool.enabled = false;
        epi.save(&ground, epi.document().unwrap()).unwrap();
        assert!(!epi.ground().unwrap().content_pool.enabled);
        let untouched: Value = serde_json::from_slice(&fs::read(&own).unwrap()).unwrap();
        assert_eq!(untouched["file_map"]["content_pool"]["enabled"], true);
    }

    #[test]
    fn external_scope_name_must_be_one_component() {
        assert!(super::super::file_map_catalog::external_state(Path::new("/tmp"), "a/b").is_err());
    }
}
