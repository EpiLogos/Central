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
use crate::file_map_index::{refresh_walked, walk, Walk};
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
            // A folder scope's file is found where it stands, never by walking the folder.
            let found = if owner.is_folder() {
                match path.strip_prefix(&owner.root).ok().and_then(Path::to_str) {
                    Some(member) => folder_member(owner, member)?,
                    None => None,
                }
            } else {
                entries(owner)?.into_iter().find(|e| e.path == path)
            };
            if let Some(entry) = found {
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
/// The words for a project's link into a folder scope whose file the folder does not answer for now: the folder errs as it is asked
/// (`error`), it is not reachable, or it no longer lists the file (gone, or outside its rules).
fn broken_folder_link(
    all: &[Scope],
    relative: &str,
    link: &Link,
    error: Option<&io::Error>,
) -> Value {
    let words = if let Some(error) = error {
        format!(
            "{relative} links to a file of {}, which cannot be read now ({}), so it is not listed",
            link.world_ref,
            failure_cause(error)
        )
    } else if all.iter().any(|owner| owner.world == link.world_ref) {
        format!(
            "{relative} links to a file {} no longer lists (gone, or outside its rules), so it is not listed",
            link.world_ref
        )
    } else {
        format!(
            "{relative} links to a file of {}, which is not reachable now, so it is not listed",
            link.world_ref
        )
    };
    json!({"path": relative, "source_ref": link.source_ref, "world_ref": link.world_ref, "words": words})
}
/// The project's registered links that reach their Source now. With `broken`, a link into a folder scope that does not answer for
/// its file now is named there and left out, so the rest of the read stands; without it, as for every other link, the read refuses.
fn link_descriptors(
    all: &[Scope],
    scope: &Scope,
    requested: Option<&str>,
    mut broken: Option<&mut Vec<Value>>,
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
        let into_folder = link.world_ref.starts_with("folder:");
        // A link into a folder is looked up in that folder's scope alone (47f item 2): another scope's error never reads as the
        // folder's cause. A folder that is not among the scopes now (unplugged, its declaration unread) finds none.
        let found = if into_folder {
            let folder = all
                .iter()
                .find(|candidate| candidate.world == link.world_ref);
            match folder
                .map(|folder| binding_by_ref(std::slice::from_ref(folder), &link.source_ref))
            {
                // The folder errs as it is asked (a folder on the way it may not read, a disk's error, its root gone since it
                // opened): that is the link's cause, named in words, and the rest of the read stands.
                Some(Err(error)) if broken.is_some() => {
                    if let Some(broken) = broken.as_deref_mut() {
                        broken.push(broken_folder_link(all, &relative, &link, Some(&error)));
                    }
                    continue;
                }
                Some(found) => found?,
                None => None,
            }
        } else {
            binding_by_ref(all, &link.source_ref)?
        };
        if found.is_none() && into_folder {
            if let Some(broken) = broken.as_deref_mut() {
                broken.push(broken_folder_link(all, &relative, &link, None));
                continue;
            }
        }
        let candidate = found.ok_or_else(|| {
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
/// A project's links that reach their Source (where each stands, and its Source), and those into a folder scope that does not answer
/// for its file now, each named in words.
pub(crate) type VerifiedLinks = (Vec<(PathBuf, Entry)>, Vec<Value>);
/// The project's links that reach their Source, each with its revision, and the links into a folder scope that does not answer for
/// its file now, each named in words (a read lists them as `broken_links`).
pub(crate) fn verified_links(all: &[Scope], scope: &Scope) -> io::Result<VerifiedLinks> {
    let mut broken = Vec::new();
    let links = link_descriptors(all, scope, None, Some(&mut broken))?
        .into_iter()
        .map(|(path, source)| Ok((path, with_revision(source)?)))
        .collect::<io::Result<_>>()?;
    Ok((links, broken))
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
/// `excluded` is the requesting World's exclusion set when the caller holds
/// it for the read; None reads it for this Source, as a single resolve does.
fn read_admission(
    all: &[Scope],
    candidate: &BindingCandidate,
    input: &Value,
    excluded: Option<&BTreeSet<String>>,
) -> io::Result<()> {
    let scope = selected(all, input)?;
    if input["project"].is_string()
        && scope.world != candidate.scope.world
        && !link_descriptors(all, scope, Some(&candidate.source.source_ref), None)?
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
    let admitted = match excluded {
        Some(excluded) => policy_allows(excluded, &candidate.source.source_ref),
        None => context_allows(all, scope, &candidate.source.source_ref)
            .map_err(|error| read_failure(error, "known", "context_admission", "unavailable"))?,
    };
    if !admitted {
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
fn locate(all: &[Scope], unreachable: &[UnreachableScope], input: &Value) -> io::Result<Value> {
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
            let actual = link_descriptors(all, scope, Some(&link.source_ref), None)?;
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
    // A location no scope holds keeps this answer while a folder whose declaration does not read stands unreachable, and the
    // folder's words say it may be there (DLW build ruling 2026-11-09, 47g item 1).
    Err(io::Error::new(
        io::ErrorKind::NotFound,
        match may_be_in(all, unreachable, &lexical) {
            Some(words) => format!("Location is unregistered; {words}"),
            None => "Location is unregistered".to_owned(),
        },
    ))
}
pub(crate) fn resolve(all: &[Scope], input: &Value) -> io::Result<Value> {
    resolve_within(all, input, None).map(|(_, value)| value)
}
/// The selected native metadata one read was taken under, as
/// `selected_metadata_basis` names it: owning World, Source and selection basis.
type SelectedBasis = Option<(String, String, Option<String>)>;
/// `resolve` with the requesting World's exclusion set held by the caller. The
/// selection basis is the one its first binding observed, before any read.
fn resolve_within(
    all: &[Scope],
    input: &Value,
    excluded: Option<&BTreeSet<String>>,
) -> io::Result<(SelectedBasis, Value)> {
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
                Ok((
                    None,
                    json!({"ownership":"unregistered","binding_only":true,"source_ref":reference}),
                ))
            } else {
                Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    "Source is outside participating maps",
                ))
            };
        }
    };
    let retained = (
        candidate.scope.world.clone(),
        candidate.source.source_ref.clone(),
        candidate.selection_metadata_basis.clone(),
    );
    read_admission(all, &candidate, input, excluded)?;
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
    read_admission(all, &current, input, excluded)?;
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
    Ok((Some(retained), value))
}
/// Why a federated read lists no entry of a folder scope it did not ask for.
const FOLDER_NOT_ASKED: &str =
    "a folder scope is listed only when a read names it or a path_prefixes entry reaches into it";
/// Why a read names a folder scope and lists none of it: none of its path prefixes reaches into the folder.
const FOLDER_NOT_REACHED: &str = "no path_prefixes entry reaches into it";
/// A read's `path_prefixes`, each normalised once, as every use of them reads them: an absolute path with its `.` components and
/// trailing slash dropped. One that is empty, not absolute, or climbs with `..` is refused in words: compared by its spelling, it
/// would reach where it does not lead.
fn path_prefixes(input: &Value) -> io::Result<Vec<PathBuf>> {
    let Some(list) = input.get("path_prefixes").and_then(Value::as_array) else {
        return Ok(Vec::new());
    };
    list.iter()
        .filter_map(Value::as_str)
        .map(|prefix| {
            let path = Path::new(prefix);
            if prefix.is_empty() {
                return Err(invalid(
                    "A path_prefixes entry is empty: each is an absolute path",
                ));
            }
            if !path.is_absolute() {
                return Err(invalid(format!(
                    "The path_prefixes entry {prefix} is not an absolute path"
                )));
            }
            if path
                .components()
                .any(|part| part == std::path::Component::ParentDir)
            {
                return Err(invalid(format!(
                    "The path_prefixes entry {prefix} climbs with ..: name the folder it means"
                )));
            }
            // `components` already leaves out every `.` and a trailing slash of an absolute path, so its components are the prefix
            // normalised (47f item 5).
            Ok(path.components().collect())
        })
        .collect()
}
/// Whether `path` is under one of `prefixes` (normalised by [`path_prefixes`], compared component by component), or there are
/// none.
fn under_prefixes(prefixes: &[PathBuf], path: &Path) -> bool {
    prefixes.is_empty() || prefixes.iter().any(|prefix| path.starts_with(prefix))
}
/// Whether one of `prefixes` reaches into the folder at `root`: under it, at it, or around it.
fn prefixes_reach(prefixes: &[PathBuf], root: &Path) -> bool {
    prefixes
        .iter()
        .any(|prefix| prefix.starts_with(root) || root.starts_with(prefix))
}
fn inspect(all: &[Scope], input: &Value) -> io::Result<Value> {
    let scope = selected(all, input)?;
    let backend = Backend::for_scope(scope);
    let version = backend.version();
    let federated = input["federated"] == true;
    // Optional scoping: only sources under these absolute path prefixes
    // enter the response. Consumers that need one subtree (the NOW field,
    // a project's records) must not materialize the whole pooled world.
    let prefixes = path_prefixes(input)?;
    let mut choices = vec![scope];
    let mut linked_refs = BTreeSet::new();
    // A folder scope's entries a read reaches through its links, each resolved where it stands: the folder is never walked for them.
    let mut linked_media = Vec::new();
    // A project's links into a folder scope that does not answer for their file now, each named: the rest of the read stands.
    let mut broken_links = Vec::new();
    if federated {
        choices = all.iter().collect();
    } else {
        let (links, broken) = verified_links(all, scope)?;
        broken_links = broken;
        for (_, entry) in links {
            linked_refs.insert(entry.source.source_ref.clone());
            if let Some(owner) = all.iter().find(|owner| owner.world == entry.world_ref) {
                if owner.is_folder() && owner.world != scope.world {
                    linked_media.push(entry);
                } else if !choices.iter().any(|chosen| chosen.world == owner.world) {
                    choices.push(owner);
                }
            }
        }
    }
    // A folder scope is walked only when the read names it or a path prefix reaches into it.
    let mut folders = Vec::new();
    choices.retain(|chosen| {
        if !chosen.is_folder() || prefixes.is_empty() && chosen.world == scope.world {
            return true;
        }
        // Named or not, a folder no path prefix reaches into is not walked: the read answers at once, reading nothing of it.
        let reached = prefixes_reach(&prefixes, &chosen.root);
        if !reached {
            let words = if chosen.world == scope.world {
                FOLDER_NOT_REACHED
            } else {
                FOLDER_NOT_ASKED
            };
            folders.push(json!({"world_ref": chosen.world, "listed": false, "words": words}));
        }
        reached
    });
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
    // `{"resources": false}` turns inspect into a cheap capability and scope
    // probe: attachments that only need provider state skip the full walk.
    let want_resources = input["resources"] != false;
    // A page of the roster: with `limit`, at most that many entries in path order after `after` (the last path of the page before),
    // and the answer says whether more stand (`page.truncated`) and where the next page starts (`page.next`). A read that walks a
    // folder scope, or names one, is always a page: of `limit`, or of FOLDER_PAGE_DEFAULT when it names none.
    let walks_folder =
        want_resources && (scope.is_folder() || choices.iter().any(|chosen| chosen.is_folder()));
    let asked_page = input.get("limit").is_some_and(|value| !value.is_null());
    // A read that carries `after` continues a page even where no folder is walked now (the folder unplugged between its pages), so it
    // lists none of the entries the first page listed whole (DLW build ruling 2026-11-09, 47f item 3).
    let continued = want_resources && input.get("after").is_some_and(|value| !value.is_null());
    let page = match input.get("limit").filter(|value| !value.is_null()) {
        None if !walks_folder && !continued => None,
        limit => {
            let limit = match limit {
                None => crate::file_map_catalog::folder_page_default(),
                Some(limit) => limit
                    .as_u64()
                    .map(|n| n as usize)
                    .filter(|n| (1..=crate::file_map_catalog::FOLDER_PAGE_MOST).contains(n))
                    .ok_or_else(|| {
                        invalid(format!(
                            "limit is how many entries one page holds, from 1 to {}",
                            crate::file_map_catalog::FOLDER_PAGE_MOST
                        ))
                    })?,
            };
            let after = match input.get("after").filter(|value| !value.is_null()) {
                None => None,
                Some(after) => Some(
                    after
                        .as_str()
                        .map(std::path::PathBuf::from)
                        .filter(|path| path.is_absolute())
                        .ok_or_else(|| {
                            invalid(
                                "after is the absolute path of the last entry of the page before",
                            )
                        })?,
                ),
            };
            Some((limit, after))
        }
    };
    let mut resources = Vec::new();
    let mut absences = Vec::new();
    // Where a bound stopped a folder scope's walk: (the position, whether it is past what the walk finished (1) or before the
    // folder's root because the walk finished nothing (0), the bound's words).
    let mut cuts: Vec<(PathBuf, u8, String)> = Vec::new();
    if want_resources {
        for chosen in &choices {
            if chosen.is_folder() {
                let Some((limit, after)) = page.as_ref() else {
                    continue;
                };
                let keep = {
                    let (prefixes, excluded) = (prefixes.clone(), excluded.clone());
                    move |entry: &crate::file_map_catalog::Entry| {
                        under_prefixes(&prefixes, &entry.path)
                            && policy_allows(&excluded, &entry.source.source_ref)
                    }
                };
                let read = crate::file_map_catalog::FolderRead {
                    after: after.clone(),
                    want: limit + 1,
                    prefixes: prefixes.clone(),
                    keep: Box::new(keep),
                };
                match crate::file_map_catalog::folder_page(chosen, read) {
                    Ok(walked) => {
                        folders.push(json!({"world_ref": chosen.world, "listed": true,
                            "entries": walked.found.len(), "skipped": walked.skipped,
                            "skipped_named": walked.skipped_named.iter()
                                .map(|(path, why)| json!({"path": path.to_string_lossy(), "why": why}))
                                .collect::<Vec<_>>(),
                            "bound": walked.bound}));
                        if let Some(words) = walked.bound {
                            cuts.push(match walked.finished {
                                Some(finished) => (finished, 1, words),
                                None => (chosen.root.clone(), 0, words),
                            });
                        }
                        resources.extend(walked.found);
                    }
                    // One folder scope's failure is its own: in a read across scopes it is that scope's absence, in its words.
                    Err(error) if federated => {
                        absences
                            .push(json!({"world_ref": chosen.world, "words": error.to_string()}));
                    }
                    Err(error) => return Err(error),
                }
                continue;
            }
            // Serve from the refresh-time enumeration snapshot when present;
            // the live walk is refresh-scale work (see entries cache).
            let cached = chosen.load_entries_cache().ok().flatten();
            let mut scoped: Vec<crate::file_map_catalog::Entry> =
                match cached.filter(|items| !items.is_empty()) {
                    Some(items) => items,
                    None => {
                        let mut items = entries(chosen)?;
                        // NOW records change constantly: the snapshot never holds them.
                        items.retain(|entry| !crate::file_map_catalog::is_now_record_entry(entry));
                        let _ = chosen.save_entries_cache(&items);
                        items
                    }
                };
            // NOW records are always read live and overlaid on the snapshot. A
            // registered relation for the same path keeps its own entry.
            scoped.retain(|entry| !crate::file_map_catalog::is_now_record_entry(entry));
            let known: BTreeSet<String> = scoped
                .iter()
                .map(|entry| entry.source.source_ref.clone())
                .collect();
            scoped.extend(
                crate::file_map_catalog::now_entries(chosen)?
                    .into_iter()
                    .filter(|entry| !known.contains(&entry.source.source_ref)),
            );
            for entry in scoped {
                if !federated
                    && chosen.world != scope.world
                    && !linked_refs.contains(&entry.source.source_ref)
                {
                    continue;
                }
                if !under_prefixes(&prefixes, &entry.path) {
                    continue;
                }
                if policy_allows(&excluded, &entry.source.source_ref) {
                    resources.push(entry);
                }
            }
        }
        resources.extend(linked_media.into_iter().filter(|entry| {
            under_prefixes(&prefixes, &entry.path)
                && policy_allows(&excluded, &entry.source.source_ref)
        }));
    }
    let mut paged = Value::Null;
    if let Some((limit, after)) = &page {
        // A page no read asked for (one that walks a folder scope and names no limit) cuts only the folder scopes' entries: every
        // other entry stands whole on its first page, as a read naming no limit has always had it, and on none after it (a page
        // that goes on from `after`), so each entry is listed once.
        let mut whole = Vec::new();
        if !asked_page {
            let walked: BTreeSet<_> = choices
                .iter()
                .filter(|chosen| chosen.is_folder())
                .map(|chosen| chosen.world.clone())
                .collect();
            (resources, whole) = std::mem::take(&mut resources)
                .into_iter()
                .partition(|entry| walked.contains(&entry.world_ref));
            if after.is_some() {
                whole.clear();
            }
        }
        resources.sort_by(|a, b| a.path.cmp(&b.path));
        if let Some(after) = after {
            resources.retain(|entry| entry.path.as_path() > after.as_path());
        }
        // A folder scope whose walk a bound stopped is known only as far as it went: the page ends there, so nothing past it is
        // skipped by the next page and `truncated` says more may stand.
        let cut = cuts
            .into_iter()
            .min_by(|a, b| (&a.0, a.1).cmp(&(&b.0, b.1)));
        if let Some((position, finished, _)) = &cut {
            resources.retain(|entry| {
                if *finished == 1 {
                    entry.path <= *position || entry.path.starts_with(position)
                } else {
                    entry.path < *position && !entry.path.starts_with(position)
                }
            });
        }
        let full = resources.len() > *limit;
        resources.truncate(*limit);
        let next = match &cut {
            _ if full => resources.last().map(|entry| entry.path.clone()),
            Some((position, 1, _)) => Some(position.clone()),
            Some(_) => resources.last().map(|entry| entry.path.clone()),
            None => None,
        }
        // A page never starts where the one before it did: a walk stopped before it finished anything names no next.
        .filter(|next| {
            after
                .as_deref()
                .is_none_or(|after| crate::file_map_catalog::beyond(next, after))
        });
        paged = json!({"limit": limit, "after": after, "truncated": full || cut.is_some(), "next": next});
        if let Some((_, _, words)) = cut {
            paged["bound"] = json!(words);
        }
        if !whole.is_empty() {
            resources.extend(whole);
            resources.sort_by(|a, b| a.path.cmp(&b.path));
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
    let mut provider = json!({"available":available,"version":version.as_ref().ok().map(|v|v.trim()),
        "tested_version":native::VERSION,"fulltext":available,"hybrid":hybrid,"semantic":hybrid,
        "embedding_model":native::embedding_model(),
        "reason":version.err().map(|e|e.to_string())});
    // Opt-in: `semantic` says the embedder is configured, not that vectors exist.
    // `embedding` reads what the selected scope's meaning index actually holds.
    if input["embedding_coverage"] == true && backend.present() {
        provider["embedding"] = match backend.embedding_coverage() {
            Ok(coverage) => coverage.to_json(&native::embedding_model()),
            Err(error) => json!({"model":native::embedding_model(),"state":"unknown",
                "reason":error.to_string()}),
        };
    }
    let mut answer = json!({"native_record":native_record,"world_ref":scope.world,"revision":scope.basis()?,
        "scopes":all.iter().map(|s| {
            let pooled = s.ground().map(|g| g.content_pool.enabled).unwrap_or(false);
            let mut listed = json!({"world_ref":s.world,"project":s.project,"path":s.root,
                "content_pool":pooled,"kind":s.kind()});
            if let Ok((folder, _)) = s.folder() {
                listed["media"] = json!(folder.media);
                listed["exclude"] = json!(folder.exclude);
            }
            listed
        }).collect::<Vec<_>>(),
        "resources":resources,"links":scope.ground()?.links,"database":backend.db(),
        "provider":provider});
    if !paged.is_null() {
        answer["page"] = paged;
    }
    if !folders.is_empty() {
        answer["folders"] = json!(folders);
    }
    if !absences.is_empty() {
        answer["absences"] = json!(absences);
    }
    if !broken_links.is_empty() {
        answer["broken_links"] = json!(broken_links);
    }
    Ok(answer)
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

/// The unreachable scope a read asked for by `asked`: a folder scope by its World first, then a project by its name, so a project
/// that only shares a folder scope's spelling never answers for the folder.
fn unreachable_asked<'a>(
    unreachable: &'a [UnreachableScope],
    asked: &str,
) -> Option<&'a UnreachableScope> {
    unreachable
        .iter()
        .find(|scope| scope.world.as_deref() == Some(asked))
        .or_else(|| {
            unreachable
                .iter()
                .find(|scope| scope.world.is_none() && scope.project == asked)
        })
}
/// The scope a read selects or, when it asked by name for one that did not open, that scope's own words.
fn selected_or_unreachable<'a>(
    all: &'a [Scope],
    unreachable: &[UnreachableScope],
    input: &Value,
) -> io::Result<&'a Scope> {
    selected(all, input).map_err(|error| {
        input
            .get("project")
            .and_then(Value::as_str)
            .and_then(|asked| unreachable_asked(unreachable, asked))
            .map_or(error, |scope| {
                io::Error::new(scope.kind, scope.message.clone())
            })
    })
}
/// A Source of a folder scope that is unreachable now, in the folder's own words.
fn unreachable_source(unreachable: &[UnreachableScope], reference: &str) -> Option<io::Error> {
    unreachable.iter().find_map(|gone| {
        gone.world
            .as_deref()
            .filter(|world| reference.starts_with(&format!("central:source:{world}:")))
            .map(|world| {
                io::Error::new(
                    gone.kind,
                    format!("Source is in {world}, which is {}", gone.message),
                )
            })
    })
}
/// A location (an absolute path) in a folder scope that is unreachable now, in the folder's own words. A folder whose declaration
/// does not read names no root, so it never claims a location (DLW build ruling 2026-11-09, 47f item 1): see [`may_be_in`].
fn unreachable_location(unreachable: &[UnreachableScope], location: &Path) -> Option<io::Error> {
    unreachable.iter().find_map(|gone| {
        gone.world
            .as_deref()
            .filter(|_| {
                gone.root
                    .as_deref()
                    .is_some_and(|root| location.starts_with(root))
            })
            .map(|world| {
                io::Error::new(
                    gone.kind,
                    format!("Location is in {world}, which is {}", gone.message),
                )
            })
    })
}
/// Where a location (an absolute path) that no scope holds may be, while a folder whose declaration does not read stands unreachable:
/// that folder's words. The location keeps its normal answer, for one broken folder never turns another answer into an error: with
/// `binding_only` it is unregistered, and the folder's words ride on it as on every answer (`unreachable`); without, it is not found,
/// in these words appended (DLW build ruling 2026-11-09, 47g item 1).
fn may_be_in(all: &[Scope], unreachable: &[UnreachableScope], location: &Path) -> Option<String> {
    if all.iter().any(|scope| location.starts_with(&scope.root)) {
        return None;
    }
    let rootless = unreachable
        .iter()
        .filter(|gone| gone.root.is_none())
        .filter_map(|gone| {
            gone.world
                .as_deref()
                .map(|world| format!("{world}, which is {}", gone.message))
        })
        .collect::<Vec<_>>();
    (!rootless.is_empty()).then(|| format!("it may be in {}", rootless.join("; or in ")))
}

/// The file map's acts that are the person's own. Each changes the person's ground or what they have adopted, so each needs the
/// person's own credential (DLW build ruling 2026-11-09: round 53 item 2 for the moves, round 54's Central follow-up for the rest).
/// Each entry is the act, the words its refusal opens with, and the word for what the refusal did not do.
/// - `move-plan`, `move-apply`, `move-rollback`: moving a file of the ground, and moving it back. `move-plan` writes the journal
///   that names where the file goes.
/// - `register`: writes a Source into the scope's relations document: `Control/relations/source-relations.json` at the root,
///   `ProjectCentral/relations/source-relations.json` at a project.
/// - `link`: makes a link inside the scope's own folder, and keeps it in that relations document.
/// - `scope-register`: chooses which of the person's folders the map reads. A project scope is kept in Control's relations
///   document. A folder scope's declaration is kept in the ground (`.central/scopes/<name>/scope.json`), and nothing is written
///   inside the folder.
/// - `adopt-db`: adopts the person's bookmark database into the map, as a copy under `.central/bkmr` with the original untouched
///   beside it. The database is only read, but what the person has adopted changes.
/// - `record-adopt`: binds one of the person's bookmarks to a Source, and rewrites its description with the person's own words
///   kept inside.
/// - `pool`: keeps, in the scope's relations document, whether the scope's whole readable tree is Source. The ruling's list does
///   not name it. It stays gated because it writes the person's Source relations (DLW build ruling 2026-11-09, round 54b item 4).
///
/// The file map's other writes need no credential: each writes only Central's own index, made again from what the ground already
/// declares. `refresh` rebuilds the bindings, the entry cache and the bkmr database under `.central/bkmr`. `projection-record`
/// keeps Central's own receipts of AIKit's copies (`.central/bkmr/projections.json`). Neither changes the ground or what is
/// adopted.
const PERSON_ACTS: [(&str, &str, &str); 9] = [
    (
        "move-plan",
        "Planning a move is the person's own press",
        "planned",
    ),
    (
        "move-apply",
        "Moving a file is the person's own press",
        "moved",
    ),
    (
        "move-rollback",
        "Moving a file back is the person's own press",
        "moved",
    ),
    (
        "register",
        "Registering a Source is the person's own press",
        "registered",
    ),
    (
        "link",
        "Linking a Source into a folder is the person's own press",
        "linked",
    ),
    (
        "scope-register",
        "Registering a scope is the person's own press",
        "registered",
    ),
    (
        "adopt-db",
        "Adopting a bookmark database is the person's own press",
        "adopted",
    ),
    (
        "record-adopt",
        "Adopting a bookmark is the person's own press",
        "adopted",
    ),
    (
        "pool",
        "Making a scope's whole tree Source is the person's own press",
        "changed",
    ),
];
/// What a move's Action says of its authority, in its description.
const PERSON_MOVE_WORDS: &str = " A move is the person's own act: it authenticates on their native authority \
    (CENTRAL_NATIVE_TOKEN) at the root, and without it is refused in words, and nothing is planned or moved. move-plan writes the \
    move's journal, which names where the file goes; move-apply and move-rollback carry it out by its plan_id alone.";
/// What the file map's other person acts say of their authority, in their descriptions, before what each writes.
const PERSON_GROUND_WORDS: &str = " It is the person's own act, since it changes their ground or what they have adopted: it \
    authenticates on their native authority (CENTRAL_NATIVE_TOKEN) where it stands (at the Work project it names, else at the root), \
    and without it is refused in words, and nothing is written.";
/// What `scope-register` says of its authority, in its description: it stands at the root.
const PERSON_SCOPE_WORDS: &str = " It is the person's own act, since it changes which of their folders the map reads: it \
    authenticates on their native authority (CENTRAL_NATIVE_TOKEN) at the root, and without it is refused in words, and nothing is \
    written.";
/// What each of those writes, in its description.
fn person_act_writes(op: &str) -> &'static str {
    match op {
        "register" => " It writes the Source into the scope's relations document.",
        "link" => " It makes the link inside the scope's folder and keeps it in the scope's relations document.",
        "scope-register" => {
            " It keeps a project scope in Control's relations document, or a folder scope's declaration in the ground; nothing is \
            written inside the folder."
        }
        "adopt-db" => {
            " It adopts the bookmark database into the map: a copy, and the untouched original beside it, under .central/bkmr; the \
            database named is only read."
        }
        "record-adopt" => {
            " It binds the bookmark to the Source and rewrites its description, keeping the bookmark's own words inside it."
        }
        "pool" => " It keeps in the scope's relations document whether the scope's whole readable tree is Source.",
        _ => "",
    }
}

/// The gate of the file map's acts of the person's own (`PERSON_ACTS`). Each authenticates on the host's native credential
/// (`CENTRAL_NATIVE_TOKEN`, never a request field) for a human grant that names that exact Action where the act stands
/// (`grant_scope`), as Central's other owner-authority acts do (`authority::authenticate`). A move is no read here: `move-plan`
/// writes the journal under `.central/bkmr/moves` that names where the file goes, which `move-apply` and `move-rollback` carry out
/// by its id alone.
///
/// The credential is asked first, before any scope is read (`authority::authenticate_before_scope`). Then where the act stands is
/// read, making nothing, and the grant is asked there (DLW build ruling 2026-11-09, round 54b item 1). So an act refused for its
/// credential reads no scope, and no refused act makes anything. A refusal opens with whose act it is, then its cause. Every other
/// operation reads no credential.
fn the_persons_act(root: &Path, op: &str, input: &Value, token: Option<&str>) -> io::Result<()> {
    let Some((_, opening, nothing)) = PERSON_ACTS.iter().find(|(asked, _, _)| *asked == op) else {
        return Ok(());
    };
    let words = |error: io::Error| {
        io::Error::new(
            error.kind(),
            format!("{opening}: {error}; nothing was {nothing}"),
        )
    };
    let action = format!("central.file-map.{op}");
    let expected = input
        .get("expected_authority_revision")
        .and_then(Value::as_str);
    let now = crate::civil_time_adoption::now();
    crate::continuous_work::authority::authenticate_before_scope(
        root, token, &action, expected, now,
    )
    .map_err(words)?;
    let scope = grant_scope(root, op, input).map_err(words)?;
    crate::continuous_work::authority::authenticate(&scope, token, &action, expected, now)
        .and_then(|principal| principal.require_human())
        .map_err(words)
}

/// Where the grant for one of the person's acts must stand: at a project of Work the act names (`project`, by its name or its World),
/// whose grant extended to it holds the act there, as Central's other acts that name a project are held
/// (`authority::PROJECT_AUTHENTICATED_ACTIONS`); at the root for anything else: no project, the root, a project scope registered from
/// outside Work or a folder scope, which the root registered. A move and `scope-register` stand at the root whatever they name: a move
/// can bring a folder into Work, and the root registers every scope. Which scope is named is read without making anything
/// (`participating_unmade`), so a refusal here leaves no folder behind (round 54b item 1).
fn grant_scope(
    root: &Path,
    op: &str,
    input: &Value,
) -> io::Result<crate::continuous_work::source::Scope> {
    let at_root = || crate::continuous_work::source::Scope::resolve(root, None);
    let Some(asked) = input
        .get("project")
        .and_then(Value::as_str)
        .filter(|_| !op.starts_with("move-") && op != "scope-register")
    else {
        return at_root();
    };
    let ground = root.canonicalize()?;
    let (all, _) = participating_unmade(&ground)?;
    let member = all
        .iter()
        .find(|scope| {
            scope.world == asked || !scope.is_folder() && scope.project.as_deref() == Some(asked)
        })
        .filter(|scope| !scope.is_external() && !scope.is_folder())
        .and_then(|scope| scope.root.strip_prefix(ground.join("Work")).ok())
        .filter(|member| member.components().count() == 1)
        .and_then(Path::to_str);
    match member {
        Some(member) => crate::continuous_work::source::Scope::resolve(root, Some(member)),
        None => at_root(),
    }
}

pub fn execute(root: &Path, op: &str, input: &Value) -> io::Result<Value> {
    execute_with_token(root, op, input, None)
}

/// [`execute`] with the host's native credential, which the person's acts authenticate on (`the_persons_act`).
pub fn execute_with_token(
    root: &Path,
    op: &str,
    input: &Value,
    token: Option<&str>,
) -> io::Result<Value> {
    the_persons_act(root, op, input, token)?;
    admitted(root, op, input)
}

/// An operation once admitted: past the person's own gate for their acts (`the_persons_act`), and with nothing to admit for any other.
/// The module's own tests of what an act does run it from here; the gate is pinned against the built `ctrl`
/// (tests/file_map_moves_person.rs).
fn admitted(root: &Path, op: &str, input: &Value) -> io::Result<Value> {
    if op == "resolve-batch" {
        return execute_resolve_batch(root, input);
    }
    if op == "refresh" && input["federated"] == true && input["project"].is_null() {
        return refresh_world(root, input);
    }
    // A folder scope's refresh builds nothing and takes no lock, the ground's least of all.
    if op == "refresh"
        && input["project"]
            .as_str()
            .is_some_and(|asked| asked.starts_with(crate::file_map_catalog::FOLDER_WORLD_PREFIX))
    {
        let ground = root.canonicalize()?;
        let (all, unreachable) = participating(&ground)?;
        let asked = input["project"].as_str().unwrap_or_default();
        let scope = all
            .iter()
            .find(|scope| scope.is_folder() && scope.world == asked)
            .ok_or_else(|| {
                unreachable_asked(&unreachable, asked)
                    .map_or(invalid("Unknown Project map"), |scope| {
                        io::Error::new(scope.kind, scope.message.clone())
                    })
            })?;
        let mut envelope = json!({"schema":SCHEMA,"operation":op,"result":folder_not_indexed(scope),
            "automatic_agent_or_model_invocation":false});
        disclose_unreachable(&mut envelope, &unreachable);
        return Ok(envelope);
    }
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
    let (all, unreachable) = if matches!(op, "move-apply" | "move-rollback") {
        (
            super::file_map_moves::resume_scopes(&root, input)?,
            Vec::new(),
        )
    } else {
        participating(&root)?
    };
    // A scope's register stands at the root, where its gate (`grant_scope`) and its write (`scope_register`) stand: a `project` it
    // is given names none of its scopes and chooses none of its locks (DLW build ruling 2026-11-09, round 54b item 2). A scope
    // asked for by name whose root did not open says so, rather than being an unknown map.
    let scope = if op == "scope-register" {
        all.first().ok_or_else(|| invalid("No scope"))?
    } else {
        selected_or_unreachable(&all, &unreachable, input)?
    };
    // A Source of a folder scope that is unreachable now is named with the folder's own words, whatever act asks for it, and so is
    // a location in it.
    if let Some(reference) = input.get("source_ref").and_then(Value::as_str) {
        if let Some(error) = unreachable_source(&unreachable, reference) {
            return Err(error);
        }
    }
    if op == "locate" {
        if let Some(error) = input["path"]
            .as_str()
            .map(Path::new)
            .filter(|location| location.is_absolute())
            .and_then(|location| unreachable_location(&unreachable, location))
        {
            return Err(error);
        }
    }
    // A folder scope is read where it stands: nothing is registered, linked or pooled inside it.
    if scope.is_folder() && matches!(op, "register" | "link" | "pool") {
        return Err(folder_writes_nothing(scope));
    }
    // Nor is an index kept for it: adoption makes one, as a refresh would.
    if scope.is_folder() && matches!(op, "adopt-db" | "record-adopt") {
        return Err(folder_keeps_no_index(scope));
    }
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
        "locate" => locate(&all, &unreachable, input)?,
        "inspect" => inspect(&all, input)?,
        "search" => search(&all, input)?,
        "resolve" => resolve(&all, input)?,
        "move-plan" => super::file_map_moves::plan(&root, &all, input)?,
        "move-apply" => super::file_map_moves::apply(&root, &all, input, false)?,
        "move-rollback" => super::file_map_moves::apply(&root, &all, input, true)?,
        "register" => register_source(&all, scope, input)?,
        // An external scope's walk is bounded as the world refresh bounds it.
        "refresh" if scope.is_external() => refresh_walked(
            scope,
            input["embeddings"] == true,
            walk_within(scope, EXTERNAL_ROOT_DEADLINE)?,
        )?,
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
            locate(&current, &unreachable, input)?
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
    let mut envelope = json!({"schema":SCHEMA,"operation":op,"result":data,"automatic_agent_or_model_invocation":false});
    disclose_unreachable(&mut envelope, &unreachable);
    Ok(envelope)
}

/// Any answer read across the World while a participating external scope did
/// not open names that scope beside its result, so a search or an inspection
/// that could not include it never reads as the whole World.
fn disclose_unreachable(envelope: &mut Value, unreachable: &[UnreachableScope]) {
    if unreachable.is_empty() {
        return;
    }
    envelope["unreachable"] = unreachable
        .iter()
        .map(|scope| {
            let mut named =
                json!({"project": scope.project, "root": scope.root, "message": scope.message});
            if let Some(world) = &scope.world {
                named["world_ref"] = json!(world);
            }
            named
        })
        .collect();
}
/// `refresh` for every participating scope: the world's upkeep as one call.
/// Each scope is refreshed by the same locked, validated `refresh` a single
/// scope gets, so one that cannot refresh (a bkmr row edited by hand, a
/// vanished external root) is reported in its own entry and the others still
/// are. Central knows its scopes, including the external ones no `central.world`
/// listing names, so a caller never has to enumerate or spell them.
///
/// No scope holds the others: a lock another writer keeps past
/// [`EXTERNAL_ROOT_DEADLINE`] is that scope's `held` entry, and an external
/// scope whose walk stops answering for as long is its `stalled` entry; the
/// world refresh goes on to the next.
fn refresh_world(root: &Path, input: &Value) -> io::Result<Value> {
    let ground = root.canonicalize()?;
    let (all, unreachable) = participating(&ground)?;
    let embeddings = input["embeddings"] == true;
    let mut ground_wait = EXTERNAL_ROOT_DEADLINE;
    let reports = world_refresh_reports(&all, &unreachable, |scope| {
        let refreshed = refresh_scope_within(
            &ground,
            scope,
            ground_wait,
            EXTERNAL_ROOT_DEADLINE,
            |scope, walked| match walked {
                Some(walked) => refresh_walked(scope, embeddings, walked),
                None => refresh(scope, embeddings),
            },
        );
        // The ground's lock held once is not waited on again by each scope.
        if matches!(
            refreshed.as_ref().err().and_then(scope_wait),
            Some(ScopeWait::GroundHeld(_))
        ) {
            ground_wait = std::time::Duration::ZERO;
        }
        refreshed
    });
    let failed = reports.iter().filter(|report| report["ok"] != true).count();
    Ok(json!({
        "schema": SCHEMA,
        "operation": "refresh",
        "result": {
            "federated": true,
            "scopes": reports,
            "refreshed": reports.len() - failed,
            "failed": failed,
        },
        "automatic_agent_or_model_invocation": false,
    }))
}

/// The world refresh's reports: one per participating scope, in scope order,
/// then one per external scope whose root did not open in time, named
/// unreachable. A stalled volume costs its own entry, never the others.
fn world_refresh_reports(
    all: &[Scope],
    unreachable: &[UnreachableScope],
    refresh_one: impl FnMut(&Scope) -> io::Result<Value>,
) -> Vec<Value> {
    let mut reports = refresh_each(all, refresh_one);
    reports.extend(unreachable.iter().map(|scope| {
        with_no_counts(json!({
            "world_ref": scope.world,
            "project": scope.project,
            "root": scope.root,
            "ok": false,
            "unreachable": true,
            "error": scope.message,
        }))
    }));
    reports
}

/// Why a scope was not refreshed this time, though nothing failed: a lock
/// another writer keeps, or an external tree that stopped answering.
#[derive(Debug)]
enum ScopeWait {
    GroundHeld(String),
    Held(String),
    Stalled(String),
}
impl std::fmt::Display for ScopeWait {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::GroundHeld(message) | Self::Held(message) | Self::Stalled(message) => {
                f.write_str(message)
            }
        }
    }
}
impl std::error::Error for ScopeWait {}

fn scope_wait(error: &io::Error) -> Option<&ScopeWait> {
    error
        .get_ref()
        .and_then(|inner| inner.downcast_ref::<ScopeWait>())
}

#[cfg(test)]
thread_local! {
    /// An external scope whose walk first opens this FIFO for reading: with no
    /// writer the open never returns, standing in for a provider that stalls
    /// after the root opened.
    static STALLED_EXTERNAL_WALK: std::cell::RefCell<Option<(PathBuf, PathBuf)>> =
        const { std::cell::RefCell::new(None) };
}

/// An external scope's walk on a helper thread. It must answer — list the
/// tree, read the next entry — within `quiet` of its last answer, the
/// deadline the scope's root had to open in; a walk that keeps answering
/// takes as long as it needs. One that stops is left: it holds no lock and
/// writes nothing, and it ends with the process.
fn walk_within(scope: &Scope, quiet: std::time::Duration) -> io::Result<Walk> {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let answered = std::sync::Arc::new(AtomicUsize::new(0));
    let (send, receive) = std::sync::mpsc::channel();
    let owned = scope.clone();
    let counting = answered.clone();
    #[cfg(test)]
    let stall = STALLED_EXTERNAL_WALK.with(|slot| {
        slot.borrow()
            .as_ref()
            .filter(|(root, _)| *root == scope.root)
            .map(|(_, fifo)| fifo.clone())
    });
    std::thread::Builder::new()
        .name("file-map-external-walk".into())
        .spawn(move || {
            #[cfg(test)]
            if let Some(fifo) = stall {
                let _ = fs::File::open(fifo);
            }
            let _ = send.send(walk(&owned, &counting));
        })?;
    let mut seen = 0;
    let mut since = std::time::Instant::now();
    loop {
        match receive.recv_timeout(std::time::Duration::from_millis(50)) {
            Ok(walked) => return walked,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                let now = answered.load(Ordering::Relaxed);
                if now != seen {
                    seen = now;
                    since = std::time::Instant::now();
                } else if since.elapsed() >= quiet {
                    return Err(io::Error::other(ScopeWait::Stalled(format!(
                        "stalled: {} stopped answering its refresh walk for {quiet:?} after \
                         {seen} answers (a File Provider or network volume not answering); \
                         nothing of it was written",
                        scope.root.display()
                    ))));
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                return Err(io::Error::other(
                    "the external scope's walk ended without an answer",
                ))
            }
        }
    }
}

/// One scope's refresh in the world's upkeep, under the locks a single
/// `refresh` takes — the ground's, then the scope's own state's — each
/// waited on for no longer than its allowance. An external scope's walk runs
/// on a helper thread ([`walk_within`]); the bkmr maintenance runs here,
/// after it, under the locks.
fn refresh_scope_within(
    ground: &Path,
    scope: &Scope,
    ground_wait: std::time::Duration,
    quiet: std::time::Duration,
    apply: impl FnOnce(&Scope, Option<Walk>) -> io::Result<Value>,
) -> io::Result<Value> {
    let held = |owner: &Path, wait: std::time::Duration| {
        let kept = if wait.is_zero() {
            "is kept by another writer".to_owned()
        } else {
            format!("was kept by another writer for {wait:?}")
        };
        format!(
            "held: {}'s file-map lock {kept}; {} is not refreshed this time",
            owner.display(),
            scope.world
        )
    };
    let Some(_ground) = crate::source_safety::lock_within(ground, "file-map.lock", ground_wait)?
    else {
        return Err(io::Error::other(ScopeWait::GroundHeld(held(
            ground,
            ground_wait,
        ))));
    };
    let _own = if scope.state != ground {
        match crate::source_safety::lock_within(&scope.state, "file-map.lock", quiet)? {
            Some(lock) => Some(lock),
            None => return Err(io::Error::other(ScopeWait::Held(held(&scope.state, quiet)))),
        }
    } else {
        None
    };
    if scope.is_external() {
        let walked = walk_within(scope, quiet)?;
        apply(scope, Some(walked))
    } else {
        apply(scope, None)
    }
}

/// What a refresh of a folder scope answers: it is listed by metadata, page by page, and Central builds no search index for it (no
/// bkmr database, no record per file).
fn folder_not_indexed(scope: &Scope) -> Value {
    json!({
        "world_ref": scope.world,
        "kind": "folder",
        "indexed": false,
        "words": format!(
            "{} is a folder scope: Central lists {} by metadata, page by page, and builds no search index for it",
            scope.project.as_deref().unwrap_or(&scope.world),
            scope.root.display()
        ),
        "changes": [],
        "diagnostics": [],
        "truncated": [],
    })
}

/// A world refresh's row for a scope that refreshed nothing (a folder scope, one that failed, one unreachable): the same numbers as
/// every other row, each none, so a reader of the rows reads one shape.
fn with_no_counts(mut report: Value) -> Value {
    for key in ["entries", "changes", "truncated"] {
        report[key] = json!(0);
    }
    report["diagnostics"] = json!([]);
    if !report["seconds"].is_number() {
        report["seconds"] = json!(0.0);
    }
    report
}
/// One report per scope, in scope order, whatever each refresh answers.
fn refresh_each(
    all: &[Scope],
    mut refresh_one: impl FnMut(&Scope) -> io::Result<Value>,
) -> Vec<Value> {
    all.iter()
        .map(|scope| {
            // A folder scope is listed by metadata, never indexed: nothing of it is refreshed, and no lock is taken for it.
            if scope.is_folder() {
                let mut report = with_no_counts(folder_not_indexed(scope));
                report["project"] = json!(scope.project);
                report["ok"] = json!(true);
                return report;
            }
            let began = std::time::Instant::now();
            let outcome = refresh_one(scope);
            let seconds = (began.elapsed().as_secs_f64() * 10.0).round() / 10.0;
            match outcome {
                Ok(done) => json!({
                    "world_ref": scope.world,
                    "project": scope.project,
                    "ok": true,
                    "entries": done["entries"].as_u64().unwrap_or(0),
                    "changes": done["changes"].as_array().map_or(0, Vec::len),
                    "diagnostics": done["diagnostics"],
                    "truncated": done["truncated"].as_array().map_or(0, Vec::len),
                    "seconds": seconds,
                }),
                Err(error) => {
                    let mut report = with_no_counts(json!({
                        "world_ref": scope.world,
                        "project": scope.project,
                        "ok": false,
                        "error": error.to_string(),
                        "seconds": seconds,
                    }));
                    match scope_wait(&error) {
                        Some(ScopeWait::GroundHeld(_) | ScopeWait::Held(_)) => {
                            report["held"] = json!(true)
                        }
                        Some(ScopeWait::Stalled(_)) => report["stalled"] = json!(true),
                        None => {}
                    }
                    report
                }
            }
        })
        .collect()
}

/// At most this many Sources are resolved by one batch.
const MAX_BATCH_SOURCES: usize = 256;
/// Serialized readings one batch answers before deferring the rest. A
/// single Source is read eagerly up to 4 MiB, so a batch answers at most this
/// plus one Source — inside a caller's capture (AIKit captures 16 MiB a
/// stream). The first reading is always answered.
const MAX_BATCH_BYTES: usize = 6 * 1024 * 1024;

/// `resolve` for many Sources in one owner read. A search answers with up to
/// a whole candidate pool, and each single `resolve` composes every
/// participating scope twice and re-reads its Source: on 18 Oct 2026 AIKit
/// admitted 177 hybrid hits for one root search through 177 `resolve`
/// processes, 40 s. Here the World checks — participating scopes, read
/// roots, ownership declarations, and the requesting World's exclusions — are
/// made once for the batch, each checked again at acknowledgement, and each
/// Source keeps its own: selected metadata retained, read, re-qualified and
/// compared exactly as a single `resolve` does, its admission checked against
/// the exclusion set. A failure that belongs to one Source is that entry's
/// refusal, in the codes `resolve` answers with; a World change refuses the
/// batch. Entries answer in the order asked; past
/// the byte budget the rest are `deferred`, unread, for the caller to ask
/// again.
fn execute_resolve_batch(root: &Path, input: &Value) -> io::Result<Value> {
    if input.get("source_ref").is_some() {
        return Err(invalid(
            "resolve-batch takes source_refs; source_ref is a single resolve",
        ));
    }
    let references = input["source_refs"]
        .as_array()
        .ok_or_else(|| invalid("source_refs must be a list of SourceRefs"))?;
    if references.is_empty() || references.len() > MAX_BATCH_SOURCES {
        return Err(invalid(format!(
            "source_refs names 1 to {MAX_BATCH_SOURCES} Sources"
        )));
    }
    let mut seen = BTreeSet::new();
    let mut requests = Vec::with_capacity(references.len());
    for reference in references {
        let reference = reference
            .as_str()
            .ok_or_else(|| invalid("source_refs must be a list of SourceRefs"))?;
        if !seen.insert(reference) {
            return Err(invalid(format!("{reference} is named twice")));
        }
        let mut single = input.clone();
        let fields = single
            .as_object_mut()
            .ok_or_else(|| invalid("resolve-batch input must be an object"))?;
        fields.remove("source_refs");
        fields.insert("source_ref".into(), json!(reference));
        binding_only(&single)?;
        requests.push((reference.to_owned(), single));
    }

    let read_root = ReadRoot::capture(root)?;
    let root = root.canonicalize()?;
    let (all, unreachable) = participating(&root)?;
    selected_or_unreachable(&all, &unreachable, input)?;
    let read_roots = all
        .iter()
        .map(|scope| ReadRoot::capture(&scope.root))
        .collect::<io::Result<Vec<_>>>()?;
    let retained_bases = ownership_bases(&all)?;
    read_root.validate()?;
    for owner in &read_roots {
        owner.validate()?;
    }
    if ownership_bases(&all)? != retained_bases {
        return Err(conflict("Native declarations changed before selected read"));
    }
    // The requesting World's exclusions depend on no Source, so the batch reads
    // them once, here, and once more at acknowledgement. Each Source still
    // checks its own admission against the set.
    let held_exclusions = context_exclusions(&all, selected(&all, input)?).ok();

    enum Entry {
        Read(Option<(String, String, Option<String>)>, Value),
        Refused(io::Error),
        Deferred,
    }
    let entries = within_observation_phase(|| -> io::Result<Vec<Entry>> {
        let mut entries = Vec::with_capacity(requests.len());
        let mut answered_bytes = 0usize;
        for (reference, single) in &requests {
            if answered_bytes >= MAX_BATCH_BYTES {
                entries.push(Entry::Deferred);
                continue;
            }
            // A Source of a folder scope that is unreachable now is refused in the folder's own words, as a single resolve is.
            if let Some(error) = unreachable_source(&unreachable, reference) {
                entries.push(Entry::Refused(error));
                continue;
            }
            let read = resolve_within(&all, single, held_exclusions.as_ref());
            entries.push(match read {
                Ok((retained, data)) => {
                    answered_bytes += serde_json::to_vec(&data)?.len();
                    Entry::Read(retained, data)
                }
                Err(error) => Entry::Refused(error),
            });
        }
        Ok(entries)
    })?;

    #[cfg(test)]
    {
        let checkpoint =
            BEFORE_READ_ACKNOWLEDGEMENT.with(|checkpoint| checkpoint.borrow_mut().take());
        if let Some(checkpoint) = checkpoint {
            checkpoint();
        }
    }
    read_root.validate()?;
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
    // Acknowledged against the set the readings were admitted under. If the
    // World changed between the two readings, the batch is refused; if either
    // reading failed, each Source reads its own, so it is refused as a single
    // resolve would refuse it.
    let acknowledged_exclusions = match &held_exclusions {
        Some(held) => match context_exclusions(&current, selected(&current, input)?) {
            Ok(acknowledged) if acknowledged == *held => Some(held),
            Ok(_) => return Err(conflict("World source relations changed during read")),
            Err(_) => None,
        },
        None => None,
    };
    let qualify = |single: &Value,
                   retained: &Option<(String, String, Option<String>)>,
                   data: &Value|
     -> io::Result<()> {
        if &selected_metadata_basis(&current, "resolve", single)? != retained {
            return Err(conflict(
                "Selected native metadata changed before acknowledgement",
            ));
        }
        let (_, qualified) = resolve_within(&current, single, acknowledged_exclusions)?;
        if &selected_metadata_basis(&current, "resolve", single)? != retained {
            return Err(conflict(
                "Selected native metadata changed during final qualification",
            ));
        }
        if &qualified != data {
            return Err(conflict(
                "Native read result changed before acknowledgement",
            ));
        }
        Ok(())
    };
    let readings: Vec<Value> = within_observation_phase(|| {
        requests
            .iter()
            .zip(entries)
            .map(|((reference, single), entry)| match entry {
                Entry::Read(retained, data) => match qualify(single, &retained, &data) {
                    Ok(()) => json!({"source_ref": reference, "reading": data}),
                    Err(error) => refusal(reference, &error),
                },
                Entry::Refused(error) => refusal(reference, &error),
                Entry::Deferred => json!({"source_ref": reference, "deferred": true}),
            })
            .collect()
    });
    for owner in &read_roots {
        owner.validate()?;
    }
    if ownership_bases(&current)? != retained_bases {
        return Err(conflict(
            "Native ownership declarations changed during final qualification",
        ));
    }
    read_root.validate()?;
    let mut envelope = json!({
        "schema": SCHEMA,
        "operation": "resolve-batch",
        "result": {"readings": readings},
        "automatic_agent_or_model_invocation": false
    });
    disclose_unreachable(&mut envelope, &unreachable);
    Ok(envelope)
}

/// One Source's refusal inside a batch, in the code and details a single
/// `resolve` refuses with.
fn refusal(reference: &str, error: &io::Error) -> Value {
    json!({
        "source_ref": reference,
        "refusal": {
            "code": failure_code(error),
            "message": error.to_string(),
            "details": read_failure_details(error),
        }
    })
}

fn failure_code(error: &io::Error) -> &'static str {
    match error.kind() {
        io::ErrorKind::NotFound => "central.file_map_not_found",
        io::ErrorKind::PermissionDenied => "central.file_map_denied",
        io::ErrorKind::AlreadyExists => "central.file_map_conflict",
        _ => "central.file_map_failure",
    }
}

fn action(op: &str, input: &Value, context: &ActionExecutionContext<'_>) -> ActionResult {
    let token = std::env::var("CENTRAL_NATIVE_TOKEN").ok();
    action_on(op, input, context, token.as_deref())
}

/// [`action`] on the credential given, as the host's environment gives it there: for the crate's tests of an act of the person's own
/// through its Action's answer, which give their own test credential and never set the process's environment.
pub(crate) fn action_on(
    op: &str,
    input: &Value,
    context: &ActionExecutionContext<'_>,
    token: Option<&str>,
) -> ActionResult {
    let id = format!("central.file-map.{op}");
    let result = crate::root::resolve_central_root(context.root_options)
        .map_err(io::Error::other)
        .and_then(|r| execute_with_token(&r.path, op, input, token));
    match result {
        Ok(data) => ActionResult::success(&id, data),
        Err(e) if crate::file_mutation::record_failure_result(&id, &e).is_some() => {
            crate::file_mutation::record_failure_result(&id, &e)
                .expect("matched native record failure")
        }
        Err(e) => ActionResult::failure_coded(
            Some(&id),
            ResultStatus::InvalidInput,
            failure_code(&e),
            e.to_string(),
            if matches!(op, "resolve" | "locate" | "resolve-batch") {
                Some(read_failure_details(&e))
            } else {
                None
            },
        ),
    }
}
/// What `inspect` says of a folder scope's pages, in its Action's description.
const FOLDER_INSPECT_WORDS: &str = " A folder scope is listed page by page: a page's walk stops at its time bound or its count of \
    folders, and says which. With no `limit`, only the folders' entries are paged: a read's first page lists every project entry \
    whole, and `after` (the `next` of the page before) continues a page and lists none of them; a `limit` pages all entries. One walk of a folder runs at a time, so after a call into the folder that never returns (a stalled \
    share or volume), every read of that folder costs a full time bound and lists none of it, until the call returns. A read that \
    waits for another walk of the folder to end can cost up to two time bounds: the wait, then its own walk.";
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
        "resolve-batch",
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
            "resolve-batch" => |_, i, c| action("resolve-batch", i, c),
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
            "snippet_chars",
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
            "kind",
            "media",
            "exclude",
        ]
        .iter()
        // A move is the root's to grant (`grant_scope`), and its scope is its Source's, so it names no project; nor does a scope's
        // register, which the root makes.
        .filter(|name| {
            !((op.starts_with("move-") || op == "scope-register") && **name == "project")
        })
        .map(|name| ActionInputDefinition {
            name: (*name).into(),
            input_type: match *name {
                "limit" | "snippet_chars" | "record_id" => "number",
                "tags" | "allowed_sources" | "selected_capsules" | "media" | "exclude" => "array",
                "content" | "federated" | "allow_external" | "native_import" | "embeddings"
                | "quiesced" => "boolean",
                _ => "string",
            }
            .into(),
            required: false,
            // scope-register says which kinds of scope it registers, so a reader learns from the Action itself whether this
            // Central keeps folder scopes.
            choices: (op == "scope-register" && *name == "kind")
                .then(|| vec!["project".to_owned(), "folder".to_owned()]),
            selection: None,
        })
        .collect();
        if op == "inspect" {
            // Read by `inspect` but never declared, so they only worked because
            // undeclared inputs pass through: `resources` asks for the Source
            // roster, and `path_prefixes` (absolute paths) keeps the roster to
            // the subtrees a consumer needs instead of the whole pooled world.
            for (name, input_type) in [
                ("resources", "boolean"),
                ("path_prefixes", "array"),
                ("embedding_coverage", "boolean"),
                ("after", "string"),
            ] {
                inputs.push(ActionInputDefinition {
                    name: name.into(),
                    input_type: input_type.into(),
                    required: false,
                    choices: None,
                    selection: None,
                });
            }
        }
        if op == "resolve-batch" {
            inputs.push(ActionInputDefinition {
                name: "source_refs".into(),
                input_type: "array".into(),
                required: true,
                choices: None,
                selection: None,
            });
        }
        if matches!(op, "resolve" | "locate" | "resolve-batch") {
            inputs.push(ActionInputDefinition {
                name: "binding_only".into(),
                input_type: "boolean".into(),
                required: false,
                choices: None,
                selection: None,
            });
        }
        if PERSON_ACTS.iter().any(|(act, _, _)| *act == op) {
            inputs.push(ActionInputDefinition {
                name: "expected_authority_revision".into(),
                input_type: "string".into(),
                required: false,
                choices: None,
                selection: None,
            });
        }
        let mut description = "Persistent bkmr file map; source and placement meaning remain in Central. Reads never rebuild the owner's database or execute openers. Writes require current source-relations revision.".to_owned();
        if op == "inspect" {
            description.push_str(FOLDER_INSPECT_WORDS);
        }
        if op.starts_with("move-") {
            description.push_str(PERSON_MOVE_WORDS);
        } else if op == "scope-register" {
            description.push_str(PERSON_SCOPE_WORDS);
            description.push_str(person_act_writes(op));
        } else if PERSON_ACTS.iter().any(|(act, _, _)| *act == op) {
            description.push_str(PERSON_GROUND_WORDS);
            description.push_str(person_act_writes(op));
        }
        registry
            .register(
                ActionDescriptor {
                    id: format!("central.file-map.{op}"),
                    title: format!("File map {op}"),
                    description,
                    inputs,
                    output: ActionOutputDefinition {
                        output_type: SCHEMA.into(),
                    },
                    mutation_class: if matches!(
                        op,
                        "inspect"
                            | "search"
                            | "resolve"
                            | "resolve-batch"
                            | "locate"
                            | "projection-read"
                            | "skill-tree"
                    ) {
                        MutationClass::ReadOnly
                    } else {
                        MutationClass::LocallyMutating
                    },
                    preview_supported: false,
                    required_ports: vec![],
                    availability: ActionAvailability {
                        available: true,
                        reason: None,
                    },
                },
                handler,
            )
            .expect("unique file map Action");
    }
}

#[cfg(test)]
mod binding_read_tests {
    use super::*;
    use crate::agent_set_store::RelationRecordStore;
    use crate::world::WORLD_RELATION_SCHEMA;
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

    impl Fixture {
        /// Declare one more retained Source beside the fixture's note.
        fn add_source(&self, name: &str, body: &[u8]) -> String {
            let path = format!("Control/user/{name}");
            fs::write(self.root.join(&path), body).unwrap();
            let reference = format!("opaque:fixture:{name}");
            let mut value: Value =
                serde_json::from_slice(&fs::read(&self.relations).unwrap()).unwrap();
            value["relations"].as_array_mut().unwrap().push(json!({
                "ref":reference,"path":path,"roles":["agent-governance-source"],
                "provenance":"unresolved","standing":"unspecified",
                "treatment":"retain-native-in-place"
            }));
            fs::write(&self.relations, serde_json::to_vec(&value).unwrap()).unwrap();
            reference
        }

        /// Project `alpha` beside the ground, with a kept and an excluded Source
        /// in its human aperture. The ground's World declares the excluded one;
        /// the Project's World excludes it when `exclude` is set.
        fn project_alpha(&self, exclude: bool) -> (PathBuf, String, String) {
            let project = self.root.join("Work/alpha");
            fs::create_dir_all(project.join("ProjectCentral/user")).unwrap();
            fs::write(project.join("ProjectCentral/project.json"), serde_json::to_vec(&json!({
                "schema":"central.project/v1","project_id":"alpha","human_source":"ProjectCentral/user",
                "wiki":{"profile":"okf-wiki/v1","source":"ProjectCentral/agents/wiki/wiki.json"}
            })).unwrap()).unwrap();
            fs::write(
                project.join("ProjectCentral/user/kept.md"),
                b"kept project body\n",
            )
            .unwrap();
            fs::write(
                project.join("ProjectCentral/user/excluded.md"),
                b"excluded project body\n",
            )
            .unwrap();
            let bindings = source_horizon::project_source_bindings(&project).unwrap();
            let reference = |path: &str| {
                bindings
                    .iter()
                    .find(|binding| binding.path == path)
                    .unwrap()
                    .source_ref
                    .clone()
            };
            let (kept, excluded) = (
                reference("ProjectCentral/user/kept.md"),
                reference("ProjectCentral/user/excluded.md"),
            );
            RelationRecordStore::worlds_at_root(&self.root)
                .save(
                    &json!({"schema":WORLD_RELATION_SCHEMA,"ref":"control:root","revision":"r1","parent":null,
                        "sources":[{"ref":excluded,"revision":"e1","authority":"human-authored","treatment":"canonical"}]}),
                    None,
                )
                .unwrap();
            if exclude {
                project_world_excluding(&project, &excluded);
            }
            (project, kept, excluded)
        }
    }

    /// The Project World `alpha`, a child of the ground, excluding one Source.
    fn project_world_excluding(project: &Path, excluded: &str) {
        RelationRecordStore::worlds_in_project(project)
            .save(
                &json!({"schema":WORLD_RELATION_SCHEMA,"ref":"project:alpha","revision":"p1","parent":"control:root",
                    "sources":[],"excluded_sources":[excluded]}),
                None,
            )
            .unwrap();
    }

    #[test]
    fn a_batch_answers_each_source_as_its_single_resolve_does_in_the_order_asked() {
        let fixture = Fixture::new();
        let other = fixture.add_source("other.md", b"other body\n");
        let batch = execute(
            &fixture.root,
            "resolve-batch",
            &json!({"source_refs":[&other,"opaque:fixture:absent","opaque:fixture:note"],"content":true}),
        )
        .unwrap();
        assert_eq!(batch["operation"], "resolve-batch");
        let readings = batch["result"]["readings"].as_array().unwrap();
        assert_eq!(readings.len(), 3);
        for (index, reference) in [(0, other.as_str()), (2, "opaque:fixture:note")] {
            let single = execute(
                &fixture.root,
                "resolve",
                &json!({"source_ref":reference,"content":true}),
            )
            .unwrap();
            assert_eq!(readings[index]["source_ref"], reference);
            assert_eq!(readings[index]["reading"], single["result"]);
        }
        assert_eq!(readings[0]["reading"]["content"], "other body\n");
        assert_eq!(readings[1]["source_ref"], "opaque:fixture:absent");
        assert_eq!(readings[1]["refusal"]["code"], "central.file_map_not_found");
        assert!(readings[1].get("reading").is_none());
        assert_eq!(readings[2]["reading"]["content"], "retained actual body\n");
    }

    #[test]
    fn a_source_changed_before_acknowledgement_refuses_only_its_own_entry() {
        let fixture = Fixture::new();
        let other = fixture.add_source("other.md", b"other body\n");
        let source = fixture.source.clone();
        fixture.checkpoint(move || fs::write(source, "changed actual body\n").unwrap());
        let batch = execute(
            &fixture.root,
            "resolve-batch",
            &json!({"source_refs":["opaque:fixture:note",&other],"content":true}),
        )
        .unwrap();
        let readings = batch["result"]["readings"].as_array().unwrap();
        assert_eq!(readings[0]["refusal"]["code"], "central.file_map_conflict");
        assert!(readings[0].get("reading").is_none());
        assert_eq!(readings[1]["reading"]["content"], "other body\n");
    }

    #[test]
    fn a_world_change_before_acknowledgement_refuses_the_whole_batch() {
        let fixture = Fixture::new();
        let path = fixture.relations.clone();
        fixture.checkpoint(move || {
            let mut value: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
            value["relations"][0]["standing"] = json!("changed-observed-standing");
            fs::write(path, serde_json::to_vec(&value).unwrap()).unwrap();
        });
        assert_eq!(
            execute(
                &fixture.root,
                "resolve-batch",
                &json!({"source_refs":["opaque:fixture:note"],"content":true}),
            )
            .unwrap_err()
            .kind(),
            io::ErrorKind::AlreadyExists
        );
    }

    #[test]
    fn a_batch_past_its_byte_budget_defers_the_rest_unread() {
        let fixture = Fixture::new();
        let half = vec![b'a'; MAX_BATCH_BYTES / 2 + 1];
        let (first, second) = (
            fixture.add_source("first.txt", &half),
            fixture.add_source("second.txt", &half),
        );
        let batch = execute(
            &fixture.root,
            "resolve-batch",
            &json!({"source_refs":[&first,&second,"opaque:fixture:note"],"content":true}),
        )
        .unwrap();
        let readings = batch["result"]["readings"].as_array().unwrap();
        assert!(readings[0]["reading"]["content"].is_string());
        assert!(readings[1]["reading"]["content"].is_string());
        assert_eq!(
            readings[2],
            json!({"source_ref":"opaque:fixture:note","deferred":true})
        );
    }

    #[test]
    fn a_batch_names_each_source_once_and_never_a_single_source_ref() {
        let fixture = Fixture::new();
        for input in [
            json!({"source_refs":[]}),
            json!({"source_refs":"opaque:fixture:note"}),
            json!({"source_refs":["opaque:fixture:note","opaque:fixture:note"]}),
            json!({"source_refs":["opaque:fixture:note"],"source_ref":"opaque:fixture:note"}),
            json!({"source_refs":["opaque:fixture:note"],"binding_only":true,"content":true}),
        ] {
            assert_eq!(
                execute(&fixture.root, "resolve-batch", &input)
                    .unwrap_err()
                    .kind(),
                io::ErrorKind::InvalidInput,
                "{input}"
            );
        }
    }

    #[test]
    fn a_source_the_requesting_world_excludes_is_refused_as_a_single_resolve_refuses_it() {
        let fixture = Fixture::new();
        let (_, kept, excluded) = fixture.project_alpha(true);
        let batch = execute(
            &fixture.root,
            "resolve-batch",
            &json!({"project":"alpha","source_refs":[&excluded,&kept],"content":true}),
        )
        .unwrap();
        let readings = batch["result"]["readings"].as_array().unwrap();
        let single = execute(
            &fixture.root,
            "resolve",
            &json!({"project":"alpha","source_ref":excluded,"content":true}),
        )
        .unwrap_err();
        assert_eq!(single.kind(), io::ErrorKind::PermissionDenied);
        assert_eq!(readings[0]["refusal"]["code"], "central.file_map_denied");
        assert_eq!(
            readings[0]["refusal"]["details"],
            read_failure_details(&single)
        );
        assert!(readings[0].get("reading").is_none());
        assert_eq!(readings[1]["reading"]["content"], "kept project body\n");
    }

    #[test]
    fn a_world_exclusion_change_before_acknowledgement_refuses_the_whole_batch() {
        let fixture = Fixture::new();
        let (project, kept, excluded) = fixture.project_alpha(false);
        let excluded_at_checkpoint = excluded.clone();
        fixture.checkpoint(move || project_world_excluding(&project, &excluded_at_checkpoint));
        assert_eq!(
            execute(
                &fixture.root,
                "resolve-batch",
                &json!({"project":"alpha","source_refs":[&kept,&excluded],"content":true}),
            )
            .unwrap_err()
            .kind(),
            io::ErrorKind::AlreadyExists
        );
    }

    #[test]
    fn an_unreadable_world_record_refuses_each_source_as_a_single_resolve_does() {
        let fixture = Fixture::new();
        let (project, kept, _) = fixture.project_alpha(false);
        let store = RelationRecordStore::worlds_in_project(&project);
        fs::create_dir_all(store.source_dir()).unwrap();
        fs::write(
            store.source_path("project:alpha").unwrap(),
            b"{ not a world record",
        )
        .unwrap();
        let single = execute(
            &fixture.root,
            "resolve",
            &json!({"project":"alpha","source_ref":kept,"content":true}),
        )
        .unwrap_err();
        let batch = execute(
            &fixture.root,
            "resolve-batch",
            &json!({"project":"alpha","source_refs":[&kept],"content":true}),
        )
        .unwrap();
        let refusal = &batch["result"]["readings"][0]["refusal"];
        assert_eq!(refusal["code"], failure_code(&single));
        assert_eq!(refusal["details"], read_failure_details(&single));
        assert!(batch["result"]["readings"][0].get("reading").is_none());
    }

    #[test]
    fn a_selected_skill_whose_metadata_changes_before_acknowledgement_refuses_only_its_entry() {
        let fixture = Fixture::new();
        let skill = fixture.root.join("Control/user/skills/actual");
        fs::create_dir_all(&skill).unwrap();
        fs::write(skill.join("SKILL.md"), b"actual selected Skill body").unwrap();
        let metadata = skill.join("skill.json");
        fs::write(
            &metadata,
            serde_json::to_vec(&json!({"schema":"central.skill/v1","name":"actual",
                "scope":"control-user","standing":"active","provenance":"human-authored",
                "retained":{"value":"before"}}))
            .unwrap(),
        )
        .unwrap();
        let reference =
            source_horizon::source_ref("control:root", "Control/user/skills/actual/SKILL.md");
        fixture.checkpoint(move || {
            let mut doc: Value = serde_json::from_slice(&fs::read(&metadata).unwrap()).unwrap();
            doc["retained"]["value"] = json!("after");
            fs::write(&metadata, serde_json::to_vec(&doc).unwrap()).unwrap();
        });
        let batch = execute(
            &fixture.root,
            "resolve-batch",
            &json!({"source_refs":[&reference,"opaque:fixture:note"],"content":true}),
        )
        .unwrap();
        let readings = batch["result"]["readings"].as_array().unwrap();
        assert_eq!(readings[0]["refusal"]["code"], "central.file_map_conflict");
        assert!(readings[0].get("reading").is_none());
        assert_eq!(readings[1]["reading"]["content"], "retained actual body\n");
    }

    #[test]
    fn an_unchanged_selected_skill_reads_in_a_batch_as_its_single_resolve_does() {
        let fixture = Fixture::new();
        let skill = fixture.root.join("Control/user/skills/actual");
        fs::create_dir_all(&skill).unwrap();
        fs::write(skill.join("SKILL.md"), b"actual selected Skill body").unwrap();
        fs::write(
            skill.join("skill.json"),
            serde_json::to_vec(&json!({"schema":"central.skill/v1","name":"actual",
                "scope":"control-user","standing":"active","provenance":"human-authored"}))
            .unwrap(),
        )
        .unwrap();
        let reference =
            source_horizon::source_ref("control:root", "Control/user/skills/actual/SKILL.md");
        let single = execute(
            &fixture.root,
            "resolve",
            &json!({"source_ref":reference,"content":true}),
        )
        .unwrap();
        let batch = execute(
            &fixture.root,
            "resolve-batch",
            &json!({"source_refs":[&reference],"content":true}),
        )
        .unwrap();
        assert_eq!(batch["result"]["readings"][0]["reading"], single["result"]);
        assert_eq!(
            batch["result"]["readings"][0]["reading"]["content"],
            "actual selected Skill body"
        );
    }

    #[test]
    fn a_batch_binds_a_declared_skill_by_its_declaration_not_its_fallback_manifest() {
        let fixture = Fixture::new();
        let skill = fixture.root.join("Control/user/skills/selected");
        fs::create_dir_all(&skill).unwrap();
        fs::write(
            skill.join("SKILL.md"),
            b"retained selected native Source body",
        )
        .unwrap();
        fs::write(
            skill.join("skill.json"),
            b"actual malformed unoverridden Skill metadata",
        )
        .unwrap();
        let mut doc: Value =
            serde_json::from_slice(&fs::read(&fixture.relations).unwrap()).unwrap();
        doc["relations"][0]["path"] = json!("Control/user/skills/selected/SKILL.md");
        fs::write(&fixture.relations, serde_json::to_vec(&doc).unwrap()).unwrap();
        let single = execute(&fixture.root, "resolve", &fixture.request(true)).unwrap();
        let batch = execute(
            &fixture.root,
            "resolve-batch",
            &json!({"source_refs":["opaque:fixture:note"],"binding_only":true}),
        )
        .unwrap();
        let reading = &batch["result"]["readings"][0]["reading"];
        assert_eq!(reading, &single["result"]);
        assert_eq!(
            reading["source"]["roles"],
            json!(["agent-governance-source"])
        );
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
        let registered = admitted(&fixture.root, "register", &json!({"path":"Control/user/note.md", "expected_revision":"absent", "title":"retained useful title"})).unwrap();
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
    fn inspect_declares_the_inputs_it_reads() {
        let mut registry = ActionRegistry::default();
        register(&mut registry);
        let inspect = registry.get("central.file-map.inspect").unwrap();
        let declared = |name: &str| {
            inspect
                .inputs
                .iter()
                .find(|input| input.name == name)
                .map(|input| (input.input_type.clone(), input.required))
        };
        assert_eq!(declared("resources"), Some(("boolean".into(), false)));
        assert_eq!(declared("path_prefixes"), Some(("array".into(), false)));
        assert_eq!(
            declared("embedding_coverage"),
            Some(("boolean".into(), false))
        );
        // They belong to inspect alone.
        let search = registry.get("central.file-map.search").unwrap();
        assert!(search
            .inputs
            .iter()
            .all(|input| input.name != "path_prefixes"));
    }

    /// O-I #939's review 1 (DLW build ruling 2026-11-09, 47c item 1): a reader learns from the Action's own disclosure whether
    /// this Central keeps folder scopes: scope-register declares the kinds of scope it registers.
    #[test]
    fn scope_register_declares_the_kinds_of_scope_it_registers() {
        let mut registry = ActionRegistry::default();
        register(&mut registry);
        let kind = |action: &str| {
            registry
                .get(action)
                .unwrap()
                .inputs
                .iter()
                .find(|input| input.name == "kind")
                .map(|input| (input.input_type.clone(), input.choices.clone()))
        };
        assert_eq!(
            kind("central.file-map.scope-register"),
            Some((
                "string".into(),
                Some(vec!["project".to_owned(), "folder".to_owned()])
            ))
        );
        // Another act's `kind` names no kinds of scope.
        assert_eq!(
            kind("central.file-map.register"),
            Some(("string".into(), None))
        );
    }

    #[test]
    fn a_pooled_scope_rosters_text_and_not_bytes_or_tool_state() {
        let fixture = Fixture::new();
        for (relative, bytes) in [
            ("scratch/idea.md", &b"text"[..]),
            ("scratch/figure.png", b"\x89PNG"),
            ("scratch/tool.pyc", b"\0"),
            (".gitnexus/parse-cache/a.json", b"{}"),
        ] {
            let path = fixture.root.join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, bytes).unwrap();
        }
        let pooled = admitted(&fixture.root, "pool", &json!({"enable":true})).unwrap();
        let paths: Vec<String> = execute(&fixture.root, "inspect", &json!({"resources":true}))
            .unwrap()["result"]["resources"]
            .as_array()
            .unwrap()
            .iter()
            .map(|entry| entry["source"]["path"].as_str().unwrap().to_owned())
            .collect();
        assert!(paths.contains(&"scratch/idea.md".to_owned()), "{paths:?}");
        for left_out in [
            "scratch/figure.png",
            "scratch/tool.pyc",
            ".gitnexus/parse-cache/a.json",
        ] {
            assert!(
                !paths.contains(&left_out.to_owned()),
                "{left_out} in {paths:?}"
            );
        }
        // The count `pool` reports is the pooled roster, not the files on disk.
        let pooled_in_roster = execute(&fixture.root, "inspect", &json!({"resources":true}))
            .unwrap()["result"]["resources"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|entry| {
                entry["source"]["roles"]
                    .as_array()
                    .is_some_and(|roles| roles.iter().any(|role| role == "content-pool"))
            })
            .count();
        assert_eq!(pooled["result"]["pooled_sources"], json!(pooled_in_roster));
    }

    #[test]
    fn a_world_refresh_reports_every_scope_and_one_failure_stops_none() {
        let fixture = Fixture::new();
        let all = scopes(&fixture.root.canonicalize().unwrap()).unwrap();
        assert!(!all.is_empty());
        // A second scope makes the order and the independence observable.
        let mut two = all.clone();
        let mut other = all[0].clone();
        other.world = "project:Other".into();
        other.project = Some("Other".into());
        two.push(other);
        let mut asked = Vec::new();
        let reports = refresh_each(&two, |scope| {
            asked.push(scope.world.clone());
            if scope.world == "project:Other" {
                Err(io::Error::other(
                    "Managed indexed description was edited in bkmr",
                ))
            } else {
                Ok(json!({"entries": 3, "changes": ["added:a", "added:b"], "diagnostics": []}))
            }
        });
        assert_eq!(
            asked,
            vec![all[0].world.clone(), "project:Other".to_owned()]
        );
        assert_eq!(reports[0]["ok"], true);
        assert_eq!(reports[0]["entries"], 3);
        assert_eq!(reports[0]["changes"], 2);
        assert_eq!(reports[1]["ok"], false);
        assert_eq!(
            reports[1]["error"],
            "Managed indexed description was edited in bkmr"
        );
        assert_eq!(reports[1]["project"], "Other");
    }

    /// The whole call against the real bkmr (`CENTRAL_FILE_MAP_BKMR_CONFORMANCE=real`;
    /// CI runners carry no bkmr): every scope of the disposable root is
    /// refreshed by one `federated` call, and a second call changes nothing.
    #[test]
    fn a_federated_refresh_initialises_the_world_and_a_second_one_is_quiet() {
        if std::env::var("CENTRAL_FILE_MAP_BKMR_CONFORMANCE").as_deref() != Ok("real") {
            eprintln!("set CENTRAL_FILE_MAP_BKMR_CONFORMANCE=real to run the real-bkmr regression");
            return;
        }
        let fixture = Fixture::new();
        let first = execute(
            &fixture.root,
            "refresh",
            &json!({"federated": true, "embeddings": false}),
        )
        .unwrap();
        let result = &first["result"];
        assert_eq!(result["federated"], true, "{first}");
        assert_eq!(result["failed"], 0, "{first}");
        let scopes = result["scopes"].as_array().unwrap();
        assert!(!scopes.is_empty());
        assert!(scopes.iter().all(|scope| scope["ok"] == true), "{first}");
        assert!(scopes[0]["entries"].as_u64().unwrap() > 0, "{first}");
        let second = execute(
            &fixture.root,
            "refresh",
            &json!({"federated": true, "embeddings": false}),
        )
        .unwrap();
        for scope in second["result"]["scopes"].as_array().unwrap() {
            assert_eq!(scope["changes"], 0, "{second}");
        }
    }

    /// Against the real bkmr: a source longer than the indexed start is named by
    /// refresh as cut, and a search hit carries the passage that matched, with
    /// where it sits, instead of the opening of the file.
    #[test]
    fn a_long_source_is_reported_cut_and_its_hit_carries_the_matching_passage() {
        if std::env::var("CENTRAL_FILE_MAP_BKMR_CONFORMANCE").as_deref() != Ok("real") {
            eprintln!("set CENTRAL_FILE_MAP_BKMR_CONFORMANCE=real to run the real-bkmr regression");
            return;
        }
        let fixture = Fixture::new();
        let opening = "The quick brown fox jumps over a lazy dog. ".repeat(250);
        let body = format!(
            "{opening}Zyxwvu is the rare word this test looks for, and it sits well past the first thousand characters.\n{}",
            "Lorem ipsum dolor sit amet, consectetur. ".repeat(2500)
        );
        assert!(
            body.len() > INDEXED_TEXT_BYTES,
            "the fixture must exceed the indexed start"
        );
        fs::write(&fixture.source, &body).unwrap();
        let refreshed = execute(&fixture.root, "refresh", &json!({"embeddings": false})).unwrap();
        let cut = refreshed["result"]["truncated"].as_array().unwrap();
        let named = cut
            .iter()
            .find(|entry| entry["source_ref"] == "opaque:fixture:note")
            .unwrap_or_else(|| panic!("the cut source is named: {refreshed}"));
        assert_eq!(named["indexed_bytes"], INDEXED_TEXT_BYTES as u64);
        assert_eq!(named["total_bytes"], body.len() as u64);
        let found = execute(
            &fixture.root,
            "search",
            &json!({"query": "zyxwvu", "mode": "fulltext"}),
        )
        .unwrap();
        let hits = found["result"]["hits"].as_array().unwrap();
        assert_eq!(hits.len(), 1, "{found}");
        let hit = &hits[0];
        assert!(
            hit["snippet"]
                .as_str()
                .unwrap()
                .contains("Zyxwvu is the rare word"),
            "{hit}"
        );
        assert_eq!(hit["matched"], true);
        assert_eq!(hit["span"]["unit"], "text_span");
        assert!(hit["span"]["start"].as_u64().unwrap() > 1000, "{hit}");
        assert_eq!(hit["indexed_bytes"], INDEXED_TEXT_BYTES as u64);
        assert_eq!(hit["total_bytes"], body.len() as u64);
    }

    #[test]
    fn a_world_refresh_is_asked_for_by_federated_without_a_project() {
        let mut registry = ActionRegistry::default();
        register(&mut registry);
        let refresh = registry.get("central.file-map.refresh").unwrap();
        for name in ["federated", "embeddings"] {
            assert!(
                refresh
                    .inputs
                    .iter()
                    .any(|input| input.name == name && input.input_type == "boolean"),
                "{name} is a declared input of refresh"
            );
        }
    }

    #[test]
    fn embedding_coverage_is_read_only_when_asked_and_only_from_an_initialised_map() {
        let fixture = Fixture::new();
        let plain = execute(&fixture.root, "inspect", &json!({"resources":false})).unwrap();
        assert!(plain["result"]["provider"].get("embedding").is_none());
        // Asked, but no map is initialised here: nothing to read, nothing invented.
        let asked = execute(
            &fixture.root,
            "inspect",
            &json!({"resources":false,"embedding_coverage":true}),
        )
        .unwrap();
        assert!(asked["result"]["provider"].get("embedding").is_none());
    }

    #[test]
    fn now_records_are_in_the_roster_by_placement_and_leave_it_when_pruned() {
        let fixture = Fixture::new();
        let record = fixture.root.join("Control/agents/now/agents/handoff.json");
        let scratch = fixture.root.join("Control/agents/now/tmp/scratch.json");
        let evidence = fixture.root.join("Control/agents/now/day/d.sources/e.json");
        for path in [&record, &scratch, &evidence] {
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, b"{\"kind\":\"handoff\"}").unwrap();
        }
        let roster = |fixture: &Fixture| -> Vec<String> {
            let inspected = execute(&fixture.root, "inspect", &json!({"resources":true})).unwrap();
            inspected["result"]["resources"]
                .as_array()
                .unwrap()
                .iter()
                .map(|entry| entry["source"]["path"].as_str().unwrap().to_owned())
                .collect()
        };
        let paths = roster(&fixture);
        assert!(paths.contains(&"Control/agents/now/agents/handoff.json".to_owned()));
        assert!(
            !paths.iter().any(|p| p.contains("now/tmp/")),
            "scratch is not a record"
        );
        assert!(
            !paths.iter().any(|p| p.contains(".sources/")),
            "evidence is not a record"
        );
        assert!(
            paths.contains(&"Control/user/note.md".to_owned()),
            "registered sources stay"
        );
        // The record resolves by its path-derived identity with no registration.
        let reference =
            source_horizon::source_ref("control:root", "Control/agents/now/agents/handoff.json");
        let resolved = execute(
            &fixture.root,
            "resolve",
            &json!({"source_ref":reference,"binding_only":true}),
        )
        .unwrap();
        assert_eq!(
            resolved["result"]["source"]["roles"],
            json!(["now-record"]),
            "resolved: {resolved}"
        );
        // A record written after the first inspect (which saved the snapshot) is
        // visible at once: NOW records are read live, never from the snapshot.
        let later = fixture.root.join("Control/agents/now/agents/later.json");
        fs::write(&later, b"{\"kind\":\"handoff\"}").unwrap();
        assert!(roster(&fixture).contains(&"Control/agents/now/agents/later.json".to_owned()));
        let snapshot = fs::read_to_string(fixture.root.join(".central/bkmr/entries-cache.json"))
            .unwrap_or_default();
        assert!(
            !snapshot.contains("now/agents/"),
            "the snapshot never holds NOW records"
        );
        // Pruning a record releases it, snapshot or not.
        fs::remove_file(&record).unwrap();
        let after = roster(&fixture);
        assert!(!after.contains(&"Control/agents/now/agents/handoff.json".to_owned()));
        assert!(after.contains(&"Control/agents/now/agents/later.json".to_owned()));
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
        admitted(
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
        admitted(
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

    /// The listing snapshot `inspect` keeps is derived state, so it is kept with the scope's other
    /// state in the ground. Before, it was written into the external tree wherever that tree held a
    /// `.central/bkmr` folder of its own.
    #[test]
    fn an_external_scopes_listing_snapshot_is_kept_in_the_ground_not_its_tree() {
        let world = World::new();
        fs::create_dir_all(world.external.join(".central/bkmr")).unwrap();
        let all = scopes(&world.root).unwrap();
        let epi = all.iter().find(|s| s.world == "project:epi").unwrap();
        safe_directory(&epi.state, Path::new(".central/bkmr")).unwrap();
        let before = world.external_listing();
        execute(
            &world.root,
            "inspect",
            &json!({"project":"epi","resources":true}),
        )
        .unwrap();
        assert_eq!(
            world.external_listing(),
            before,
            "external tree was written"
        );
        assert!(epi.state.join(".central/bkmr/entries-cache.json").is_file());
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

    /// Round 22, residual (a): the root opened, then the provider stalled
    /// inside the walk. The world refresh names that scope `stalled` and goes
    /// on; a scope whose lock another writer keeps is `held`, and the ground's
    /// lock held is every scope's `held`, waited on once. Nothing waits past
    /// its deadline, and nothing is applied for a scope that did not answer.
    #[test]
    fn a_walk_that_stops_answering_is_stalled_a_kept_lock_is_held_and_the_world_goes_on() {
        let world = World::new();
        let ground = world.root.canonicalize().unwrap();
        let (all, unreachable) = participating(&ground).unwrap();
        assert!(unreachable.is_empty());
        let epi = all
            .iter()
            .find(|scope| scope.world == "project:epi")
            .unwrap()
            .clone();
        assert!(epi.is_external());
        let fine = |_: &Scope| Ok(json!({"entries": 1, "changes": [], "diagnostics": []}));
        let quick = std::time::Duration::from_millis(400);
        let round = |quiet: std::time::Duration| {
            let mut applied = Vec::new();
            let began = std::time::Instant::now();
            let reports = world_refresh_reports(&all, &unreachable, |scope| {
                refresh_scope_within(&ground, scope, quiet, quiet, |scope, walked| {
                    applied.push((scope.world.clone(), walked.is_some()));
                    fine(scope)
                })
            });
            (reports, applied, began.elapsed())
        };

        // A walk that keeps answering is applied after it, the others as before.
        let (reports, applied, _) = round(quick);
        assert!(
            reports.iter().all(|report| report["ok"] == true),
            "{reports:?}"
        );
        assert!(applied.contains(&("project:epi".to_owned(), true)));
        assert!(applied.contains(&("control:root".to_owned(), false)));

        // The provider stalls inside the walk: that scope is stalled, the world goes on.
        let fifo = world.base.join("walk.fifo");
        let named = std::ffi::CString::new(fifo.as_os_str().as_encoded_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(named.as_ptr(), 0o600) }, 0);
        STALLED_EXTERNAL_WALK
            .with(|slot| *slot.borrow_mut() = Some((epi.root.clone(), fifo.clone())));
        let (reports, applied, took) = round(quick);
        assert!(took < quick + std::time::Duration::from_secs(2), "{took:?}");
        let stalled = reports
            .iter()
            .find(|report| report["world_ref"] == "project:epi")
            .unwrap();
        assert_eq!(stalled["ok"], false);
        assert_eq!(stalled["stalled"], true);
        assert!(stalled.get("held").is_none());
        let message = stalled["error"].as_str().unwrap();
        assert!(
            message.starts_with(&format!(
                "stalled: {} stopped answering its refresh walk for 400ms after 0 answers",
                epi.root.display()
            )),
            "{message}"
        );
        assert!(reports
            .iter()
            .filter(|report| report["world_ref"] != "project:epi")
            .all(|report| report["ok"] == true));
        assert!(!applied.iter().any(|(world, _)| world == "project:epi"));
        // Asked for by name, the scope's refresh says so in the same words.
        let refused = execute(&world.root, "refresh", &json!({"project": "epi"})).unwrap_err();
        assert!(refused.to_string().starts_with("stalled: "), "{refused}");
        STALLED_EXTERNAL_WALK.with(|slot| *slot.borrow_mut() = None);
        let _writer = fs::OpenOptions::new().write(true).open(&fifo).unwrap();

        // Another writer keeps the scope's own lock: held, and the world goes on.
        let kept = crate::source_safety::lock(&epi.state, "file-map.lock").unwrap();
        let (reports, applied, took) = round(quick);
        assert!(took < quick + std::time::Duration::from_secs(2), "{took:?}");
        let held = reports
            .iter()
            .find(|report| report["world_ref"] == "project:epi")
            .unwrap();
        assert_eq!(held["held"], true);
        assert!(held["error"]
            .as_str()
            .unwrap()
            .starts_with(&format!("held: {}'s file-map lock", epi.state.display())));
        assert!(!applied.iter().any(|(world, _)| world == "project:epi"));
        assert!(applied.contains(&("control:root".to_owned(), false)));
        drop(kept);

        // The ground's lock kept: every scope is held, waited on once.
        let kept = crate::source_safety::lock(&ground, "file-map.lock").unwrap();
        let began = std::time::Instant::now();
        let answered = execute(&world.root, "refresh", &json!({"federated": true})).unwrap();
        assert!(
            began.elapsed() < EXTERNAL_ROOT_DEADLINE + std::time::Duration::from_secs(2),
            "{:?}",
            began.elapsed()
        );
        let scopes = answered["result"]["scopes"].as_array().unwrap();
        assert_eq!(scopes.len(), all.len());
        assert!(
            scopes.iter().all(|report| report["held"] == true),
            "{answered}"
        );
        assert_eq!(answered["result"]["refreshed"], 0);
        drop(kept);
    }

    /// 5 Nov 2026: ~/Documents is a File Provider domain, and while its
    /// provider stalled a plain open() of ~/Documents/epi blocked the world
    /// refresh for 17 minutes. A FIFO opened for reading, with no writer,
    /// stands in for the stalled provider.
    #[test]
    fn an_external_root_that_does_not_open_is_named_unreachable_and_the_other_scopes_stand() {
        let world = World::new();
        let stalled = world.base.join("documents-legacy");
        fs::create_dir_all(stalled.join("ProjectCentral")).unwrap();
        fs::write(
            stalled.join("ProjectCentral/project.json"),
            serde_json::to_vec(&json!({"schema":"central.project/v1","project_id":"legacy",
                "human_source":"ProjectCentral/user",
                "wiki":{"profile":"okf-wiki/v1","source":"ProjectCentral/agents/wiki/wiki.json"}}))
            .unwrap(),
        )
        .unwrap();
        fs::write(
            world
                .root
                .join(source_horizon::CONTROL_GROUND_RELATIONS_SOURCE),
            serde_json::to_vec(&json!({
                "schema":source_horizon::CONTROL_GROUND_RELATIONS_SCHEMA,
                "project_id":"control:root","relations":[],
                "file_map":{"scopes":{
                    "epi":world.external.to_str().unwrap(),
                    "legacy":stalled.to_str().unwrap()
                }}
            }))
            .unwrap(),
        )
        .unwrap();
        let fifo = world.base.join("provider.fifo");
        let named = std::ffi::CString::new(fifo.as_os_str().as_encoded_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(named.as_ptr(), 0o600) }, 0);
        STALLED_EXTERNAL_ROOT
            .with(|slot| *slot.borrow_mut() = Some((stalled.clone(), fifo.clone())));

        let began = std::time::Instant::now();
        let (all, unreachable) = participating(&world.root).unwrap();
        assert!(
            began.elapsed() < EXTERNAL_ROOT_DEADLINE + std::time::Duration::from_secs(2),
            "the stalled root held the scopes for {:?}",
            began.elapsed()
        );
        assert!(all.iter().any(|scope| scope.world == "project:epi"));
        assert!(all.iter().all(|scope| scope.world != "project:legacy"));
        assert_eq!(unreachable.len(), 1);
        assert_eq!(unreachable[0].project, "legacy");
        let message = format!(
            "unreachable: {} did not open within 3s (a File Provider or network volume not \
             answering)",
            stalled.display()
        );
        assert_eq!(unreachable[0].message, message);

        // Asked again while that open still waits, it answers at once: no
        // second thread blocks on the same root.
        let again = std::time::Instant::now();
        let (_, still) = participating(&world.root).unwrap();
        assert!(again.elapsed() < std::time::Duration::from_secs(1));
        assert_eq!(still.len(), 1);

        // The world refresh names it in its own entry and the others refresh.
        let reports = world_refresh_reports(&all, &unreachable, |_| {
            Ok(json!({"entries": 1, "changes": [], "diagnostics": []}))
        });
        assert!(reports
            .iter()
            .filter(|report| report["project"] != "legacy")
            .all(|report| report["ok"] == true));
        let legacy = reports
            .iter()
            .find(|report| report["project"] == "legacy")
            .unwrap();
        assert_eq!(legacy["ok"], false);
        assert_eq!(legacy["unreachable"], true);
        assert_eq!(legacy["error"], message);
        // Asked for by name, it says why rather than being an unknown map.
        let refused = execute(&world.root, "inspect", &json!({"project":"legacy"})).unwrap_err();
        assert_eq!(refused.to_string(), message);
        // Read across the World while it does not open, every answer names it
        // beside the result (round 21): a search that could not include it
        // never reads as the whole World.
        let answered = execute(&world.root, "search", &json!({"query":"anything"})).unwrap();
        assert_eq!(
            answered["unreachable"],
            json!([{"project": "legacy", "root": stalled.to_str().unwrap(), "message": message}])
        );

        // The provider answers: the abandoned open returns, and the root is
        // reachable again.
        STALLED_EXTERNAL_ROOT.with(|slot| *slot.borrow_mut() = None);
        let _writer = fs::OpenOptions::new().write(true).open(&fifo).unwrap();
        let recovered = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            let (all, unreachable) = participating(&world.root).unwrap();
            if unreachable.is_empty() {
                assert!(all.iter().any(|scope| scope.world == "project:legacy"));
                let answered =
                    execute(&world.root, "search", &json!({"query":"anything"})).unwrap();
                assert!(answered.get("unreachable").is_none(), "{answered}");
                break;
            }
            assert!(
                std::time::Instant::now() < recovered,
                "the root never opened again"
            );
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    }
}

#[cfg(test)]
mod folder_scope_tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT: AtomicU64 = AtomicU64::new(0);
    /// An act's answer: its envelope's `result`.
    /// What an operation does once admitted (`admitted`): the person's acts among these are theirs, whose gate
    /// tests/file_map_moves_person.rs pins against the built `ctrl`, so these tests of what they do run past it.
    fn run(root: &Path, op: &str, input: &Value) -> io::Result<Value> {
        admitted(root, op, input).map(|envelope| envelope["result"].clone())
    }
    /// A ground with one Work project, and beside it a plain Music folder: sound files, a text note,
    /// a set, an AppleDouble companion and a nested folder. No ProjectCentral anywhere in it.
    struct World {
        base: PathBuf,
        root: PathBuf,
        music: PathBuf,
    }
    impl World {
        fn new() -> Self {
            let scratch = Path::new(env!("CARGO_MANIFEST_DIR")).join("../ProjectCentral/now/tmp");
            fs::create_dir_all(&scratch).unwrap();
            let base = scratch.canonicalize().unwrap().join(format!(
                "file-map-folder-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            let root = base.join("ground");
            let music = base.join("Music");
            fs::create_dir_all(root.join("Control/user")).unwrap();
            fs::create_dir_all(root.join("Work/Alpha/ProjectCentral")).unwrap();
            fs::write(
                root.join("Work/Alpha/ProjectCentral/project.json"),
                serde_json::to_vec(&json!({"schema":"central.project/v1","project_id":"alpha",
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
                    "project_id":"control:root","relations":[]
                }))
                .unwrap(),
            )
            .unwrap();
            fs::create_dir_all(music.join("Field recordings")).unwrap();
            fs::create_dir_all(music.join("Sets")).unwrap();
            fs::write(
                music.join("Field recordings/estuary low tide.wav"),
                b"RIFF....WAVEfmt sound",
            )
            .unwrap();
            fs::write(
                music.join("Field recordings/._estuary low tide.wav"),
                b"Mac OS X attrs",
            )
            .unwrap();
            fs::write(
                music.join("market-hall.AIFF"),
                b"FORM....AIFFCOMM sound sound",
            )
            .unwrap();
            fs::write(music.join("voice.mp3"), b"ID3 sound").unwrap();
            fs::write(music.join("notes.txt"), "what I recorded\n").unwrap();
            fs::write(music.join("Sets/first.als"), b"gzip").unwrap();
            Self { base, root, music }
        }
        fn music(&self) -> String {
            self.music.to_str().unwrap().to_owned()
        }
        /// Every path under the Music folder with its bytes and modification time, so a test can
        /// prove that no byte under it changed.
        fn music_bytes(&self) -> BTreeMap<PathBuf, (Vec<u8>, i64, i64)> {
            let mut out = BTreeMap::new();
            let mut stack = vec![self.music.clone()];
            while let Some(dir) = stack.pop() {
                let meta = fs::symlink_metadata(&dir).unwrap();
                out.insert(dir.clone(), (Vec::new(), meta.mtime(), meta.mtime_nsec()));
                for entry in fs::read_dir(&dir).unwrap() {
                    let path = entry.unwrap().path();
                    if path.is_dir() {
                        stack.push(path);
                    } else {
                        let meta = fs::symlink_metadata(&path).unwrap();
                        out.insert(
                            path.clone(),
                            (fs::read(&path).unwrap(), meta.mtime(), meta.mtime_nsec()),
                        );
                    }
                }
            }
            out
        }
        fn revision(&self) -> String {
            run(&self.root, "inspect", &json!({"resources":false})).unwrap()["revision"]
                .as_str()
                .unwrap()
                .to_owned()
        }
        fn register(&self, name: &str, path: &str, media: Value) -> io::Result<Value> {
            run(
                &self.root,
                "scope-register",
                &json!({"name":name,"path":path,"kind":"folder","allow_external":true,
                    "media":media,"expected_revision":self.revision()}),
            )
        }
        /// The person's grant for the three moves at the root, written into this disposable ground; the token it authenticates.
        fn person_moves(&self) -> &'static str {
            const TOKEN: &str = "file-map-folder-moves-controlled-test-not-a-live-secret";
            let source = "Control/user/file-map-moves-authority.json";
            let actions: Vec<_> = PERSON_ACTS
                .iter()
                .map(|(op, _, _)| format!("central.file-map.{op}"))
                .collect();
            fs::write(self.root.join(source), serde_json::to_vec(&json!({"schema":"central.native-action-authority/v1",
                "scope_ref":"control:root","grants":[{"principal_ref":"human:file-map-folder-test","actor_kind":"human",
                    "token_sha256":crate::continuous_work::source::key(TOKEN),"scope_refs":["control:root"],"actions":actions,
                    "expires_at_unix_seconds":u64::MAX}]})).unwrap()).unwrap();
            let path = self
                .root
                .join(source_horizon::CONTROL_GROUND_RELATIONS_SOURCE);
            let mut relations: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
            relations["relations"].as_array_mut().unwrap().push(json!({"ref":source_horizon::source_ref("control:root", source),
                "path":source,"roles":[crate::continuous_work::authority::AUTHORITY_ROLE],"provenance":"human-adopted",
                "standing":"architecture-contract","treatment":"control-user","recognition":"controlled-test-only",
                "recorded_at_unix_seconds":1}));
            fs::write(path, serde_json::to_vec(&relations).unwrap()).unwrap();
            TOKEN
        }
    }
    impl Drop for World {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.base);
        }
    }
    fn refused(result: io::Result<Value>) -> String {
        result.unwrap_err().to_string()
    }

    /// Central issue: a plain folder (a world's Music) is a scope its readers can ask for, its media
    /// listed by metadata, and Central writes nothing inside it.
    #[test]
    fn a_plain_folder_is_a_scope_its_media_listed_by_metadata_and_nothing_written_inside_it() {
        let world = World::new();
        let before = world.music_bytes();
        let registered = world
            .register(
                "Music",
                &world.music(),
                json!(["wav", ".AIFF", "aif", "mp3"]),
            )
            .unwrap();
        assert_eq!(registered["world_ref"], "folder:Music");
        assert_eq!(registered["kind"], "folder");
        assert_eq!(registered["changed"], true);
        assert_eq!(registered["media"], json!(["aif", "aiff", "mp3", "wav"]));

        // Its declaration is the ground's, beside its derived state.
        let declared: Value = serde_json::from_slice(
            &fs::read(world.root.join(".central/scopes/Music/scope.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(declared["schema"], "central.file-map.folder-scope/v1");
        assert_eq!(declared["name"], "Music");
        assert_eq!(declared["root"], world.music());
        assert_eq!(declared["kind"], "folder");
        assert_eq!(declared["media"], json!(["aif", "aiff", "mp3", "wav"]));
        assert!(declared["registered_at"].as_str().unwrap().ends_with('Z'));

        // It stands among the scopes, as the kind it is.
        let scopes = run(&world.root, "inspect", &json!({"resources":false})).unwrap();
        let listed = scopes["scopes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|scope| scope["world_ref"] == "folder:Music")
            .unwrap()
            .clone();
        assert_eq!(listed["kind"], "folder");
        assert_eq!(listed["project"], "Music");
        assert_eq!(listed["path"], world.music());
        assert_eq!(listed["media"], json!(["aif", "aiff", "mp3", "wav"]));
        let kinds: BTreeSet<String> = scopes["scopes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|scope| scope["kind"].as_str().unwrap().to_owned())
            .collect();
        assert_eq!(
            kinds,
            BTreeSet::from(["control".into(), "folder".into(), "project".into()])
        );

        // Its entries are its admitted media, each by path, size, time and kind; nothing else in it.
        let read = run(
            &world.root,
            "inspect",
            &json!({"project":"folder:Music","resources":true,"path_prefixes":[world.music()]}),
        )
        .unwrap();
        let mut entries: Vec<(String, String, u64)> = read["resources"]
            .as_array()
            .unwrap()
            .iter()
            .map(|entry| {
                assert!(entry["mtime"].as_i64().is_some(), "{entry}");
                (
                    entry["path"].as_str().unwrap().to_owned(),
                    entry["kind"].as_str().unwrap().to_owned(),
                    entry["size"].as_u64().unwrap(),
                )
            })
            .collect();
        entries.sort();
        let at = |name: &str| world.music.join(name).to_str().unwrap().to_owned();
        assert_eq!(
            entries,
            vec![
                (
                    at("Field recordings/estuary low tide.wav"),
                    "audio".into(),
                    21
                ),
                (at("market-hall.AIFF"), "audio".into(), 28),
                (at("voice.mp3"), "audio".into(), 9),
            ]
        );

        // A selected read answers it as any Source, by its metadata; its content is never read: asked
        // for, it is refused in words.
        let reference = read["resources"]
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["path"] == at("voice.mp3"))
            .unwrap()["source"]["ref"]
            .as_str()
            .unwrap()
            .to_owned();
        let resolved = run(&world.root, "resolve", &json!({"source_ref":reference})).unwrap();
        assert_eq!(resolved["kind"], "audio");
        assert_eq!(resolved["size"], 9);
        assert_eq!(resolved["project"], "Music");
        let words = refused(run(
            &world.root,
            "resolve",
            &json!({"source_ref":reference,"content":true}),
        ));
        assert!(
            words.contains("voice.mp3 is a media entry (audio)"),
            "{words}"
        );
        assert!(words.contains("reads none of its content"), "{words}");

        // Asked again, the same folder is the same scope; and nothing is pooled, registered or linked
        // inside it.
        let again = world
            .register(
                "Music",
                &world.music(),
                json!(["mp3", "wav", "aiff", "aif"]),
            )
            .unwrap();
        assert_eq!(again["changed"], false);
        let pooled = refused(run(
            &world.root,
            "pool",
            &json!({"project":"folder:Music","enable":true}),
        ));
        assert!(
            pooled.contains("Music is a folder scope: Central reads"),
            "{pooled}"
        );
        assert!(pooled.contains("writes nothing inside it"), "{pooled}");

        // A sound put there is listed at once: the folder's listing keeps no snapshot.
        assert_eq!(
            world.music_bytes(),
            before,
            "a byte under the folder changed"
        );
        fs::write(world.music.join("later.wav"), b"RIFF").unwrap();
        let put = world.music_bytes();
        let later = run(&world.root, "inspect", &json!({"project":"folder:Music"})).unwrap();
        assert!(later["resources"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["path"] == at("later.wav")));
        assert_eq!(
            world.music_bytes(),
            put,
            "reading it again changed a byte under the folder"
        );
    }

    #[test]
    fn a_folder_scope_that_cannot_be_registered_is_refused_in_words() {
        let world = World::new();
        let words =
            |name: &str, path: &str, media: Value| refused(world.register(name, path, media));
        let missing = world.base.join("Nowhere");
        assert_eq!(
            words("Music", missing.to_str().unwrap(), json!(["wav"])),
            format!("There is no folder at {}", missing.display())
        );
        let inside = world.root.join("Control/user");
        assert!(words("Music", inside.to_str().unwrap(), json!(["wav"]))
            .starts_with(&format!("{} is inside this ground", inside.display())));
        assert!(words("Music", world.base.to_str().unwrap(), json!(["wav"]))
            .contains("holds this ground"));
        assert!(words("Music", "Music", json!(["wav"]))
            .contains("is named by its absolute path, and Music is not one"));
        let note = world.music.join("notes.txt");
        assert_eq!(
            words("Music", note.to_str().unwrap(), json!(["wav"])),
            format!("{} is a file, not a folder", note.display())
        );
        assert_eq!(
            words("Music", &world.music(), json!(["wav", "ogg"])),
            "Central lists no media of kind ogg: it admits wav, aif, aiff, flac, mp3 and m4a"
        );
        assert!(words("Music", &world.music(), json!([]))
            .starts_with("A folder scope names the media it admits"));
        assert!(words("Alpha", &world.music(), json!(["wav"]))
            .contains("The scope name Alpha is already taken by a project of this ground"));
        let unasked = refused(run(
            &world.root,
            "scope-register",
            &json!({"name":"Music","path":world.music(),"kind":"folder","media":["wav"],
                "expected_revision":world.revision()}),
        ));
        assert!(
            unasked
                .contains("a folder scope is registered only when asked, with allow_external=true"),
            "{unasked}"
        );
        let other = world.base.join("Samples");
        fs::create_dir_all(other.join("ProjectCentral")).unwrap();
        fs::write(other.join("ProjectCentral/project.json"), b"{}").unwrap();
        assert_eq!(
            words("Samples", other.to_str().unwrap(), json!(["wav"])),
            format!(
                "{} holds a ProjectCentral: register it as a project, not as a folder",
                other.display()
            )
        );
        world
            .register("Music", &world.music(), json!(["wav"]))
            .unwrap();
        fs::remove_dir_all(other.join("ProjectCentral")).unwrap();
        assert_eq!(
            words("Music", other.to_str().unwrap(), json!(["wav"])),
            format!(
                "The scope name Music is already taken: it is the folder scope over {}",
                world.music()
            )
        );
        assert_eq!(
            words("Sounds", &world.music(), json!(["wav"])),
            format!("{} is already the folder scope Music", world.music())
        );
        // Nothing of the refusals was written: one declaration stands.
        let declared: Vec<String> = fs::read_dir(world.root.join(".central/scopes"))
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|dir| dir.join("scope.json").is_file())
            .map(|dir| dir.file_name().unwrap().to_str().unwrap().to_owned())
            .collect();
        assert_eq!(declared, vec!["Music".to_owned()]);
    }

    /// A folder scope's reader asks for its entries page by page, and an app's own folders inside
    /// the person's (its backups, its packs) never fill a page: the scope excludes them by name,
    /// and no folder scope walks a hidden folder, a package or a symbolic link.
    #[test]
    fn a_folder_scope_pages_its_media_and_never_walks_what_it_excludes_a_hidden_folder_or_a_package(
    ) {
        let world = World::new();
        for (relative, bytes) in [
            ("Album Project/Samples/kick.wav", &b"RIFF kick"[..]),
            (
                "Album Project/Backup/Album [2026-10-01 120000].wav",
                b"RIFF old",
            ),
            ("Album Project/Ableton Project Info/cache.wav", b"RIFF info"),
            ("Ableton/Factory Packs/Pack/Samples/loop.wav", b"RIFF pack"),
            (".hidden/secret.wav", b"RIFF secret"),
            ("Logic.logicx/Media/take.wav", b"RIFF logic"),
            ("a/d.wav", b"RIFF d"),
            ("a b/c.wav", b"RIFF c"),
        ] {
            let path = world.music.join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, bytes).unwrap();
        }
        let elsewhere = world.base.join("Elsewhere");
        fs::create_dir_all(&elsewhere).unwrap();
        fs::write(elsewhere.join("linked.wav"), b"RIFF linked").unwrap();
        std::os::unix::fs::symlink(&elsewhere, world.music.join("Linked")).unwrap();
        let registered = run(
            &world.root,
            "scope-register",
            &json!({"name":"Music","path":world.music(),"kind":"folder","allow_external":true,
                "media":["wav","aiff","mp3"],"exclude":["Factory Packs","Backup","Ableton Project Info","Backup"],
                "expected_revision":world.revision()}),
        )
        .unwrap();
        assert_eq!(
            registered["exclude"],
            json!(["Ableton Project Info", "Backup", "Factory Packs"])
        );
        let listed = run(&world.root, "inspect", &json!({"resources":false})).unwrap();
        let music = listed["scopes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|scope| scope["world_ref"] == "folder:Music")
            .unwrap()
            .clone();
        assert_eq!(
            music["exclude"],
            json!(["Ableton Project Info", "Backup", "Factory Packs"])
        );
        let read = |extra: Value| {
            let mut input =
                json!({"project":"folder:Music","resources":true,"path_prefixes":[world.music()]});
            input
                .as_object_mut()
                .unwrap()
                .extend(extra.as_object().unwrap().clone());
            run(&world.root, "inspect", &input)
        };
        let paths = |answer: &Value| -> Vec<String> {
            answer["resources"]
                .as_array()
                .unwrap()
                .iter()
                .map(|entry| {
                    entry["path"]
                        .as_str()
                        .unwrap()
                        .strip_prefix(&format!("{}/", world.music()))
                        .unwrap()
                        .to_owned()
                })
                .collect()
        };
        // The whole listing: the person's own media, in path order, and nothing the walk leaves out.
        let whole = read(json!({})).unwrap();
        let mut all = paths(&whole);
        all.sort_by(|a, b| Path::new(a).cmp(Path::new(b)));
        assert_eq!(
            all,
            [
                "Album Project/Samples/kick.wav",
                "Field recordings/estuary low tide.wav",
                "a/d.wav",
                "a b/c.wav",
                "market-hall.AIFF",
                "voice.mp3",
            ]
        );
        // A folder scope is never read whole: a read that names no limit is a page of the default 2000.
        assert_eq!(
            whole["page"],
            json!({"limit":2000,"after":null,"truncated":false,"next":null})
        );
        // Page by page, two at a time: each page says whether more stand and where the next starts.
        let mut pages = Vec::new();
        let mut after = Value::Null;
        loop {
            let answer = read(json!({"limit":2,"after":after})).unwrap();
            let page = answer["page"].clone();
            assert_eq!(page["limit"], 2);
            pages.push((paths(&answer), page["truncated"].clone()));
            if page["truncated"] != true {
                assert_eq!(page["next"], Value::Null);
                break;
            }
            assert_eq!(
                page["next"].as_str().unwrap(),
                answer["resources"][1]["path"].as_str().unwrap()
            );
            after = page["next"].clone();
        }
        assert_eq!(
            pages,
            vec![
                (
                    vec![
                        "Album Project/Samples/kick.wav".to_owned(),
                        "Field recordings/estuary low tide.wav".to_owned()
                    ],
                    json!(true)
                ),
                (
                    vec!["a/d.wav".to_owned(), "a b/c.wav".to_owned()],
                    json!(true)
                ),
                (
                    vec!["market-hall.AIFF".to_owned(), "voice.mp3".to_owned()],
                    json!(false)
                ),
            ]
        );
        // A page asked for wrongly is refused in words, and so is an exclusion that is not one folder's name.
        assert_eq!(
            refused(read(json!({"limit":0}))),
            "limit is how many entries one page holds, from 1 to 5000"
        );
        assert_eq!(
            refused(read(json!({"limit":2,"after":"Music/a"}))),
            "after is the absolute path of the last entry of the page before"
        );
        assert_eq!(
            refused(run(
                &world.root,
                "scope-register",
                &json!({"name":"Samples","path":elsewhere,"kind":"folder","allow_external":true,
                    "media":["wav"],"exclude":["Album Project/Backup"],"expected_revision":world.revision()}),
            )),
            "A folder scope excludes folders by name, one plain path component each, and Album Project/Backup is not one"
        );
        assert_eq!(
            refused(run(
                &world.root,
                "scope-register",
                &json!({"name":"Music","path":world.music(),"kind":"folder","allow_external":true,
                    "media":["wav","aiff","mp3"],"exclude":["Backup"],"expected_revision":world.revision()}),
            )),
            format!(
                "The folder scope Music already stands over {} admitting aiff, mp3, wav and excluding Ableton Project Info, Backup, Factory Packs: it is registered once, as it was asked first",
                world.music()
            )
        );
    }

    #[test]
    fn a_folder_taken_away_is_named_unreachable_and_the_other_scopes_stand() {
        let world = World::new();
        world
            .register("Music", &world.music(), json!(["wav"]))
            .unwrap();
        let moved = world.base.join("Music moved");
        fs::rename(&world.music, &moved).unwrap();
        let (all, unreachable) = participating(&world.root.canonicalize().unwrap()).unwrap();
        assert!(all.iter().any(|scope| scope.world == "project:alpha"));
        assert!(all.iter().all(|scope| scope.world != "folder:Music"));
        assert_eq!(unreachable.len(), 1);
        assert_eq!(
            unreachable[0].message,
            format!(
                "unreachable: the folder scope Music stands over {}, and no folder is there now",
                world.music()
            )
        );
        run(&world.root, "inspect", &json!({"resources":false})).unwrap();
        fs::rename(&moved, &world.music).unwrap();
        assert!(participating(&world.root)
            .unwrap()
            .0
            .iter()
            .any(|scope| scope.world == "folder:Music"));
    }

    // ---- Central #328's fresh review (DLW build ruling 2026-11-09, central-47b) ----
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

    fn with_seams<T>(
        seams: crate::file_map_catalog::FolderWalkSeams,
        act: impl FnOnce() -> T,
    ) -> T {
        crate::file_map_catalog::FOLDER_WALK_SEAMS.with(|slot| *slot.borrow_mut() = seams);
        let out = act();
        crate::file_map_catalog::FOLDER_WALK_SEAMS
            .with(|slot| *slot.borrow_mut() = Default::default());
        out
    }
    /// The entries of an answer, by their paths under the Music folder.
    fn music_paths(world: &World, answer: &Value) -> Vec<String> {
        answer["resources"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|entry| {
                entry["path"]
                    .as_str()
                    .unwrap()
                    .strip_prefix(&format!("{}/", world.music()))
                    .map(str::to_owned)
            })
            .collect()
    }
    fn put(world: &World, relative: &str) {
        let path = world.music.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, b"RIFF sound").unwrap();
    }
    /// Gives a folder back its permissions when a test ends, however it ends, so the fixture can be removed.
    struct Opened(PathBuf);
    impl Drop for Opened {
        fn drop(&mut self) {
            let _ = fs::set_permissions(&self.0, fs::Permissions::from_mode(0o755));
        }
    }
    fn close(path: &Path) -> Opened {
        fs::set_permissions(path, fs::Permissions::from_mode(0o000)).unwrap();
        Opened(path.to_path_buf())
    }

    /// Review 1, 5 and 10 (central-47b items 1, 5 and 10): every file-map act on a folder scope leaves its tree as it was, byte
    /// for byte, and makes no `.central` inside it. What would keep state for it says so in words instead: a projection receipt is
    /// refused, a refresh builds no index, a search names it as listed, not searched, and a stale revision writes nothing.
    #[test]
    fn every_file_map_act_on_a_folder_scope_leaves_its_tree_byte_for_byte() {
        let world = World::new();
        world
            .register("Music", &world.music(), json!(["wav", "aiff", "mp3"]))
            .unwrap();
        let before = world.music_bytes();
        let unchanged = |act: &str| {
            assert_eq!(
                world.music_bytes(),
                before,
                "{act} changed a byte under the folder"
            );
            assert!(
                !world.music.join(".central").exists(),
                "{act} made a .central inside the folder"
            );
        };
        let voice = source_horizon::source_ref("folder:Music", "voice.mp3");

        let named = run(&world.root, "inspect", &json!({"project":"folder:Music"})).unwrap();
        assert_eq!(named["resources"].as_array().unwrap().len(), 3);
        unchanged("an inspect named to it");
        let federated = run(
            &world.root,
            "inspect",
            &json!({"federated":true,"path_prefixes":[world.music()]}),
        )
        .unwrap();
        assert_eq!(music_paths(&world, &federated).len(), 3, "{federated}");
        unchanged("a federated inspect with path_prefixes");
        run(&world.root, "resolve", &json!({"source_ref":voice})).unwrap();
        run(
            &world.root,
            "locate",
            &json!({"path":world.music.join("voice.mp3")}),
        )
        .unwrap();
        unchanged("resolve and locate");

        // A search leaves the folder out and says so; it never reads as a map not initialized.
        let listed =
            json!([{"world_ref":"folder:Music","words":"listed by metadata, not searched"}]);
        let searched = run(
            &world.root,
            "search",
            &json!({"federated":true,"query":"sound"}),
        )
        .unwrap();
        assert_eq!(searched["listed_not_searched"], listed, "{searched}");
        assert!(
            searched["absences"]
                .as_array()
                .unwrap()
                .iter()
                .all(|absence| !absence.to_string().contains("folder:Music")),
            "{searched}"
        );
        let alone = run(
            &world.root,
            "search",
            &json!({"project":"folder:Music","query":"sound"}),
        )
        .unwrap();
        assert_eq!(alone["hits"], json!([]));
        assert_eq!(alone["absences"], json!([]), "{alone}");
        assert_eq!(alone["listed_not_searched"], listed);
        unchanged("search");

        // A refresh builds no index for it, alone or in the world's upkeep.
        let refreshed = run(&world.root, "refresh", &json!({"project":"folder:Music"})).unwrap();
        assert_eq!(refreshed["world_ref"], "folder:Music");
        assert_eq!(refreshed["kind"], "folder");
        assert_eq!(refreshed["indexed"], false);
        assert_eq!(
            refreshed["words"],
            format!(
                "Music is a folder scope: Central lists {} by metadata, page by page, and builds no search index for it",
                world.music()
            )
        );
        let all = scopes(&world.root).unwrap();
        let reports = refresh_each(&all, |scope| {
            assert!(
                !scope.is_folder(),
                "the world refresh refreshed {}",
                scope.world
            );
            Ok(json!({"entries":0,"changes":[],"diagnostics":[],"truncated":[]}))
        });
        let music = reports
            .iter()
            .find(|report| report["world_ref"] == "folder:Music")
            .unwrap();
        assert_eq!(music["ok"], true, "{music}");
        assert_eq!(music["indexed"], false, "{music}");
        assert_eq!(music["words"], refreshed["words"]);
        assert!(
            !Backend::for_scope(all.iter().find(|scope| scope.is_folder()).unwrap()).present(),
            "a bkmr index was made for the folder scope"
        );
        unchanged("refresh");

        // No projection receipt is kept for it, inside it or beside it.
        let words = format!(
            "Music is a folder scope: Central reads {} where it stands and keeps no index or projection receipt for it",
            world.music()
        );
        assert_eq!(
            refused(run(
                &world.root,
                "projection-record",
                &json!({"project":"folder:Music","source_ref":voice,"source_revision":"any",
                    "path":world.music.join("voice.mp3"),"allow_external":true,"owner":"aikit"}),
            )),
            words
        );
        assert_eq!(
            refused(run(
                &world.root,
                "projection-read",
                &json!({"project":"folder:Music"})
            )),
            words
        );
        unchanged("projection-record and projection-read");

        // What would write there is refused, and so is a registration made on a stale revision.
        for (op, input) in [
            (
                "register",
                json!({"project":"folder:Music","path":"voice.mp3","expected_revision":world.revision()}),
            ),
            (
                "link",
                json!({"project":"folder:Music","path":"again.mp3","source_ref":voice,"owner":"aikit"}),
            ),
            ("pool", json!({"project":"folder:Music","enable":true})),
        ] {
            let words = refused(run(&world.root, op, &input));
            assert!(words.contains("writes nothing inside it"), "{op}: {words}");
        }
        let stale = refused(run(
            &world.root,
            "scope-register",
            &json!({"name":"Sounds","path":world.base.join("Elsewhere"),"kind":"folder",
                "allow_external":true,"media":["wav"],"expected_revision":"stale"}),
        ));
        assert!(
            stale.starts_with("Source-relations revision changed: expected stale"),
            "{stale}"
        );
        assert!(!world.root.join(".central/scopes/Sounds").exists());
        unchanged("the refused writes");
    }

    /// Review 5 (central-47b item 5): a folder scope's refresh takes no lock, the ground's least of all: it answers while
    /// another writer keeps the ground's file-map lock.
    #[test]
    fn a_folder_scope_refresh_answers_while_the_ground_lock_is_kept() {
        let world = World::new();
        world
            .register("Music", &world.music(), json!(["wav"]))
            .unwrap();
        let ground = world.root.canonicalize().unwrap();
        let _kept = crate::source_safety::lock(&ground, "file-map.lock").unwrap();
        let (send, receive) = std::sync::mpsc::channel();
        let root = world.root.clone();
        std::thread::spawn(move || {
            let _ = send.send(
                run(&root, "refresh", &json!({"project":"folder:Music"}))
                    .map_err(|error| error.to_string()),
            );
        });
        // A guard against a hang, not a timing claim: the refresh either answers or waits on the lock for good.
        let answered = receive
            .recv_timeout(std::time::Duration::from_secs(30))
            .expect("the folder scope's refresh waited on the ground's lock");
        assert_eq!(answered.unwrap()["indexed"], false);
    }

    /// Review 2 (central-47b item 2): a federated read walks a folder scope only when it names it or a path prefix reaches into it,
    /// and every read of one is a page: 2000 entries unless the read asks for fewer, at most 5000.
    #[test]
    fn a_federated_read_walks_a_folder_scope_only_when_asked_and_always_by_the_page() {
        let world = World::new();
        world
            .register("Music", &world.music(), json!(["wav", "aiff", "mp3"]))
            .unwrap();
        let unasked = run(&world.root, "inspect", &json!({"federated":true})).unwrap();
        assert!(music_paths(&world, &unasked).is_empty(), "{unasked}");
        assert_eq!(
            unasked["folders"],
            json!([{"world_ref":"folder:Music","listed":false,
                "words":"a folder scope is listed only when a read names it or a path_prefixes entry reaches into it"}])
        );
        assert!(unasked.get("page").is_none(), "{unasked}");
        // A prefix reaches into it from inside, at its root or from around it.
        for prefix in [
            world.music.join("Field recordings"),
            world.music.clone(),
            world.base.clone(),
        ] {
            let read = run(
                &world.root,
                "inspect",
                &json!({"federated":true,"path_prefixes":[prefix]}),
            )
            .unwrap();
            assert_eq!(read["page"]["limit"], 2000, "{read}");
            assert_eq!(read["folders"][0]["world_ref"], "folder:Music");
            assert_eq!(read["folders"][0]["listed"], true);
            let expected = if prefix == world.music.join("Field recordings") {
                1
            } else {
                3
            };
            assert_eq!(music_paths(&world, &read).len(), expected, "{read}");
        }
        // A prefix beside it does not.
        let beside = run(
            &world.root,
            "inspect",
            &json!({"federated":true,"path_prefixes":[world.root.join("Work")]}),
        )
        .unwrap();
        assert_eq!(beside["folders"][0]["listed"], false, "{beside}");
        // Named, it is read by the page; a page larger than 5000 is refused.
        let named = run(&world.root, "inspect", &json!({"project":"folder:Music"})).unwrap();
        assert_eq!(named["page"]["limit"], 2000);
        assert_eq!(
            refused(run(
                &world.root,
                "inspect",
                &json!({"project":"folder:Music","limit":5001})
            )),
            "limit is how many entries one page holds, from 1 to 5000"
        );
    }

    /// Review 3 (central-47b item 3), its own case: `limit: 2` with a prefix. The page fills with entries under the prefix, and
    /// `truncated` means more of them stand, however many entries outside the prefix the walk passes first.
    #[test]
    fn a_page_fills_with_matching_entries_and_truncated_means_more_of_them_stand() {
        let world = World::new();
        for name in ["a1.wav", "a2.wav", "a3.wav", "a4.wav"] {
            put(&world, &format!("Album/{name}"));
        }
        for name in ["z1.wav", "z2.wav", "z3.wav"] {
            put(&world, &format!("Zebra/{name}"));
        }
        world
            .register("Music", &world.music(), json!(["wav", "aiff", "mp3"]))
            .unwrap();
        let prefix = world.music.join("Zebra");
        let page = |after: Value| {
            run(
                &world.root,
                "inspect",
                &json!({"project":"folder:Music","path_prefixes":[prefix],"limit":2,"after":after}),
            )
            .unwrap()
        };
        let first = page(Value::Null);
        assert_eq!(
            music_paths(&world, &first),
            ["Zebra/z1.wav", "Zebra/z2.wav"]
        );
        assert_eq!(first["page"]["truncated"], true, "{first}");
        let second = page(first["page"]["next"].clone());
        assert_eq!(music_paths(&world, &second), ["Zebra/z3.wav"]);
        assert_eq!(second["page"]["truncated"], false, "{second}");
        assert_eq!(second["page"]["next"], Value::Null);
        // Two entries under a prefix that holds exactly two: the page is full and nothing more stands.
        fs::remove_file(world.music.join("Zebra/z3.wav")).unwrap();
        let exact = page(Value::Null);
        assert_eq!(
            music_paths(&world, &exact),
            ["Zebra/z1.wav", "Zebra/z2.wav"]
        );
        assert_eq!(exact["page"]["truncated"], false, "{exact}");
    }

    /// Review 4 (central-47b item 4): a folder that cannot be read, or an entry gone while the walk reads it, is skipped, counted
    /// and named, and the rest is listed. A folder scope whose own folder cannot be read is that scope's absence, in its words,
    /// in a federated read, and the other scopes answer.
    #[test]
    fn one_bad_entry_never_aborts_a_folder_scope_read() {
        let world = World::new();
        put(&world, "Locked/inside.wav");
        put(&world, "Field recordings/gone.wav");
        world
            .register("Music", &world.music(), json!(["wav", "aiff", "mp3"]))
            .unwrap();
        let locked = world.music.join("Locked");
        let gone = world.music.join("Field recordings/gone.wav");
        let _opened = close(&locked);
        // Run as root, a closed folder still reads: the unreadable case is then not made here.
        let unreadable = fs::read_dir(&locked).is_err();
        let read = with_seams(
            crate::file_map_catalog::FolderWalkSeams {
                vanish: Some(gone.clone()),
                ..Default::default()
            },
            || run(&world.root, "inspect", &json!({"project":"folder:Music"})).unwrap(),
        );
        assert_eq!(
            music_paths(&world, &read),
            [
                "Field recordings/estuary low tide.wav",
                "market-hall.AIFF",
                "voice.mp3"
            ]
        );
        let walked = &read["folders"][0];
        let named = walked["skipped_named"].as_array().unwrap();
        assert_eq!(
            walked["skipped"],
            if unreadable { 2 } else { 1 },
            "{walked}"
        );
        assert!(named.iter().any(|skipped| skipped["path"] == json!(gone)
            && skipped["why"] == "it was gone when it was read"));
        if unreadable {
            assert!(
                named.iter().any(|skipped| skipped["path"] == json!(locked)
                    && skipped["why"]
                        .as_str()
                        .unwrap()
                        .starts_with("it could not be read: Permission denied")),
                "{walked}"
            );
        }
        drop(_opened);
        // The folder itself gone between its opening and its walk (a drive taken away): named alone, the read fails in words;
        // federated, it is that scope's absence, and the other scopes answer.
        let words = format!(
            "Music is a folder scope over {}, and its folder cannot be read: No such file or directory (os error 2)",
            world.music()
        );
        let seams = || crate::file_map_catalog::FolderWalkSeams {
            vanish: Some(world.music.clone()),
            ..Default::default()
        };
        let named = with_seams(seams(), || {
            refused(run(
                &world.root,
                "inspect",
                &json!({"project":"folder:Music"}),
            ))
        });
        assert_eq!(named, words);
        fs::create_dir_all(&world.music).unwrap();
        let federated = with_seams(seams(), || {
            run(
                &world.root,
                "inspect",
                &json!({"federated":true,"path_prefixes":[world.base]}),
            )
            .unwrap()
        });
        assert_eq!(
            federated["absences"],
            json!([{"world_ref":"folder:Music","words":words}]),
            "{federated}"
        );
        assert!(federated["resources"].is_array());
        // A folder it may not open is unreachable before any walk, in its words, and the other scopes answer.
        fs::create_dir_all(&world.music).unwrap();
        let _closed = close(&world.music);
        if fs::read_dir(&world.music).is_err() {
            let envelope = execute(&world.root, "inspect", &json!({"resources":false})).unwrap();
            assert_eq!(
                envelope["unreachable"],
                json!([{"project":"Music","world_ref":"folder:Music","root":world.music(),
                    "message":format!("unreachable: the folder scope Music stands over {}, and it did not open: Permission denied (os error 13)", world.music())}])
            );
        }
    }

    /// Review 6 (central-47b item 6): hidden and packaged mean what macOS means. A package is a folder with a package's extension
    /// or a bundle's Contents/Info.plist; hidden is a leading dot, an AppleDouble `._` file or Finder's hidden flag; an exclusion
    /// matches whatever the case. A resolve or a locate of what the walk leaves out answers it as outside the scope.
    #[test]
    fn hidden_and_packaged_mean_what_macos_means_in_the_walk_and_in_a_resolve() {
        let world = World::new();
        let left_out = [
            "Album.logic/take.wav",
            "Synth.clap/patch.wav",
            "Rack.aaxplugin/rack.wav",
            "Edit.fcpbundle/clip.wav",
            "Disk.sparsebundle/band.wav",
            "Song.pages/sung.wav",
            "Tool.appex/tool.wav",
            "Bundle/inner.wav",
            "Album Project/BACKUP/old.wav",
            "._ghost.wav",
            ".hidden/secret.wav",
        ];
        for relative in left_out {
            put(&world, relative);
        }
        put(&world, "Contents only/Contents/kept.wav");
        fs::create_dir_all(world.music.join("Bundle/Contents")).unwrap();
        fs::write(world.music.join("Bundle/Contents/Info.plist"), b"<plist/>").unwrap();
        // Finder's hidden flag is macOS's own: elsewhere nothing is flagged.
        let flagged: &[&str] = if cfg!(target_os = "macos") {
            put(&world, "Flagged folder/under.wav");
            put(&world, "flagged.wav");
            for relative in ["Flagged folder", "flagged.wav"] {
                let status = std::process::Command::new("chflags")
                    .arg("hidden")
                    .arg(world.music.join(relative))
                    .status()
                    .unwrap();
                assert!(status.success());
            }
            &["Flagged folder/under.wav", "flagged.wav"]
        } else {
            &[]
        };
        run(
            &world.root,
            "scope-register",
            &json!({"name":"Music","path":world.music(),"kind":"folder","allow_external":true,
                "media":["wav","aiff","mp3"],"exclude":["Backup"],"expected_revision":world.revision()}),
        )
        .unwrap();
        let read = run(&world.root, "inspect", &json!({"project":"folder:Music"})).unwrap();
        assert_eq!(
            music_paths(&world, &read),
            [
                "Contents only/Contents/kept.wav",
                "Field recordings/estuary low tide.wav",
                "market-hall.AIFF",
                "voice.mp3"
            ]
        );
        for relative in left_out.iter().chain(flagged) {
            let reference = source_horizon::source_ref("folder:Music", relative);
            let resolved = refused(run(
                &world.root,
                "resolve",
                &json!({"source_ref":reference}),
            ));
            assert!(
                resolved.contains("outside participating maps"),
                "{relative}: {resolved}"
            );
            let located = refused(run(
                &world.root,
                "locate",
                &json!({"path":world.music.join(relative)}),
            ));
            assert_eq!(located, "Location is unregistered", "{relative}");
        }
        // What the walk lists, a resolve answers.
        let kept = source_horizon::source_ref("folder:Music", "Contents only/Contents/kept.wav");
        assert_eq!(
            run(&world.root, "resolve", &json!({"source_ref":kept})).unwrap()["kind"],
            "audio"
        );
    }

    /// Review 7 (central-47b item 7): a folder scope that does not open (a component turned into a file or a symbolic link, a
    /// folder it may not enter, a declaration that does not read, a name a project holds) is unreachable, in words, and every
    /// other act goes on. Its name never selects a project, and a project's name never selects it.
    #[test]
    fn a_folder_scope_that_does_not_open_is_unreachable_and_every_other_act_goes_on() {
        let world = World::new();
        let drive = world.base.join("Drive");
        fs::create_dir_all(drive.join("Samples")).unwrap();
        world
            .register("Music", &world.music(), json!(["wav"]))
            .unwrap();
        world
            .register(
                "Samples",
                drive.join("Samples").to_str().unwrap(),
                json!(["wav"]),
            )
            .unwrap();
        let ground = world.root.canonicalize().unwrap();
        let unreachable_words = || -> Vec<String> {
            let (all, unreachable) = participating(&ground).unwrap();
            assert!(all.iter().any(|scope| scope.world == "project:alpha"));
            run(&world.root, "inspect", &json!({"resources":false})).unwrap();
            unreachable.into_iter().map(|scope| scope.message).collect()
        };
        let moved = world.base.join("Music moved");
        fs::rename(&world.music, &moved).unwrap();
        // A component turned into a file.
        fs::write(&world.music, b"not a folder").unwrap();
        assert_eq!(
            unreachable_words(),
            [format!(
                "unreachable: the folder scope Music stands over {0}, and it did not open: {0} is not a folder",
                world.music()
            )]
        );
        // A component turned into a symbolic link.
        fs::remove_file(&world.music).unwrap();
        std::os::unix::fs::symlink(&moved, &world.music).unwrap();
        let words = unreachable_words();
        assert_eq!(words.len(), 1);
        assert!(
            words[0].starts_with(&format!(
                "unreachable: the folder scope Music stands over {}, and it did not open: refusing symlink",
                world.music()
            )),
            "{words:?}"
        );
        fs::remove_file(&world.music).unwrap();
        fs::rename(&moved, &world.music).unwrap();
        // A folder it may not enter.
        let closed = close(&drive);
        if fs::read_dir(&drive).is_err() {
            let words = unreachable_words();
            assert_eq!(words.len(), 1);
            assert!(
                words[0].starts_with(&format!(
                    "unreachable: the folder scope Samples stands over {}, and it did not open: Permission denied",
                    drive.join("Samples").display()
                )),
                "{words:?}"
            );
        }
        drop(closed);
        assert!(unreachable_words().is_empty());
        // A declaration that does not read.
        let declaration = world.root.join(".central/scopes/Samples/scope.json");
        let kept = fs::read(&declaration).unwrap();
        fs::write(&declaration, b"{not json").unwrap();
        let words = unreachable_words();
        assert_eq!(words.len(), 1);
        assert!(
            words[0].starts_with(
                "unreachable: the folder scope declared in .central/scopes/Samples does not read: "
            ),
            "{words:?}"
        );
        fs::write(&declaration, kept).unwrap();
        // A project of the ground made under the folder scope's name: the project stands and is the one its name selects.
        fs::create_dir_all(world.root.join("Work/Music/ProjectCentral")).unwrap();
        fs::write(
            world.root.join("Work/Music/ProjectCentral/project.json"),
            serde_json::to_vec(&json!({"schema":"central.project/v1","project_id":"music",
                "human_source":"ProjectCentral/user",
                "wiki":{"profile":"okf-wiki/v1","source":"ProjectCentral/agents/wiki/wiki.json"}}))
            .unwrap(),
        )
        .unwrap();
        assert_eq!(
            unreachable_words(),
            ["unreachable: the folder scope Music shares its name with a project of this ground (Work/Music): the project stands, and the folder is not read until one of them is renamed"]
        );
        let project = run(
            &world.root,
            "inspect",
            &json!({"project":"Music","resources":false}),
        )
        .unwrap();
        assert_eq!(project["world_ref"], "project:music");
        let folder = refused(run(
            &world.root,
            "inspect",
            &json!({"project":"folder:Music","resources":false}),
        ));
        assert!(
            folder.starts_with("unreachable: the folder scope Music shares its name"),
            "{folder}"
        );
        fs::remove_dir_all(world.root.join("Work/Music")).unwrap();
        // The folder scope's own name selects it, never a project's spelling of it.
        let all = scopes(&ground).unwrap();
        assert_eq!(
            selected(&all, &json!({"project":"folder:Music"}))
                .unwrap()
                .world,
            "folder:Music"
        );
        assert!(selected(&all, &json!({"project":"Music"})).is_err());
    }

    /// Review 8 and 10 (central-47b items 8 and 10): the refusals of registration hold at every open, so a declaration edited by
    /// hand to name the ground, a folder around it, `/` or a project is refused; and a folder nested in or around another folder or
    /// external scope is never registered. Each refusal is asked by a path that is not canonical (a link to the ground, a trailing
    /// slash, another case, `/` spelled with `..`): compared as spelled, each would pass.
    #[test]
    fn refusals_hold_at_every_open_and_for_every_spelling() {
        let world = World::new();
        world
            .register("Music", &world.music(), json!(["wav"]))
            .unwrap();
        let ground = world.root.canonicalize().unwrap();
        let declaration = world.root.join(".central/scopes/Music/scope.json");
        let declared: Value = serde_json::from_slice(&fs::read(&declaration).unwrap()).unwrap();
        let edited = |root: &str| {
            let mut edited = declared.clone();
            edited["root"] = json!(root);
            fs::write(&declaration, serde_json::to_vec(&edited).unwrap()).unwrap();
            let (all, unreachable) = participating(&ground).unwrap();
            assert!(all.iter().all(|scope| scope.world != "folder:Music"));
            assert_eq!(unreachable.len(), 1, "{root}");
            unreachable[0].message.clone()
        };
        let shown = ground.display().to_string();
        let opened = |root: &str, why: String| {
            format!("unreachable: the folder scope Music stands over {root}, and it did not open: {why}")
        };
        let inside = ground.join("Control/user");
        let inside = inside.to_str().unwrap();
        assert_eq!(
            edited(inside),
            opened(inside, format!("{inside} is inside this ground"))
        );
        assert_eq!(
            edited(&shown),
            opened(&shown, format!("{shown} is inside this ground"))
        );
        let base = world.base.to_str().unwrap();
        assert_eq!(
            edited(base),
            opened(base, format!("{base} holds this ground"))
        );
        assert_eq!(edited("/"), opened("/", "/ holds this ground".to_owned()));
        let project = world.base.join("Project");
        fs::create_dir_all(project.join("ProjectCentral")).unwrap();
        fs::write(project.join("ProjectCentral/project.json"), b"{}").unwrap();
        let project = project.to_str().unwrap();
        assert_eq!(
            edited(project),
            opened(project, format!("{project} holds a ProjectCentral"))
        );
        fs::write(&declaration, serde_json::to_vec(&declared).unwrap()).unwrap();
        assert!(participating(&ground).unwrap().1.is_empty());

        // Registration, by spellings that are not canonical: each under its own name, so each is judged alone, and every one that
        // was not refused as it must be is named together.
        let link = world.base.join("ground link");
        std::os::unix::fs::symlink(&ground, &link).unwrap();
        let linked = link.join("Control/user");
        let slashed = format!("{}/", world.music());
        let dotted = format!("{}/..", world.root.display());
        let other_case = world.base.join("GROUND/Control/user");
        let mut spellings = vec![
            (
                "a link to the ground",
                "Linked",
                linked.to_str().unwrap().to_owned(),
                format!("{} is inside this ground", linked.display()),
            ),
            (
                "a trailing slash",
                "Slashed",
                slashed.clone(),
                format!("{slashed} is already the folder scope Music"),
            ),
            (
                "a folder around the ground spelled with ..",
                "Dotted",
                dotted.clone(),
                format!("{dotted} holds this ground"),
            ),
            (
                "/",
                "Rooted",
                "/".to_owned(),
                "/ holds this ground".to_owned(),
            ),
        ];
        // Another case names the same folder only where the file system ignores case, as macOS's does by default.
        if other_case.exists() {
            spellings.push((
                "another case",
                "Cased",
                other_case.to_str().unwrap().to_owned(),
                format!("{} is inside this ground", other_case.display()),
            ));
        }
        let passed: Vec<String> = spellings
            .iter()
            .filter_map(|(label, name, path, refusal)| {
                match world.register(name, path, json!(["wav"])) {
                    Err(error) if error.to_string().starts_with(refusal.as_str()) => None,
                    other => Some(format!("{label}: {other:?}")),
                }
            })
            .collect();
        assert!(passed.is_empty(), "{passed:#?}");
        let words = |path: &str| refused(world.register("Elsewhere", path, json!(["wav"])));
        // Nested in or around another folder scope, or an external project's.
        let field = world.music.join("Field recordings");
        assert_eq!(
            words(&format!("{}/", field.display())),
            format!(
                "{}/ is inside the folder scope Music ({}): a folder scope never stands inside another scope",
                field.display(),
                world.music()
            )
        );
        let outer = world.base.join("Outer");
        fs::create_dir_all(outer.join("Inner")).unwrap();
        world
            .register(
                "Inner",
                outer.join("Inner").to_str().unwrap(),
                json!(["wav"]),
            )
            .unwrap();
        assert_eq!(
            words(outer.to_str().unwrap()),
            format!(
                "{} holds the folder scope Inner ({}): a folder scope never stands around another scope",
                outer.display(),
                outer.join("Inner").display()
            )
        );
        let epi = world.base.join("documents-epi");
        fs::create_dir_all(epi.join("ProjectCentral")).unwrap();
        fs::create_dir_all(epi.join("Audio")).unwrap();
        fs::write(
            epi.join("ProjectCentral/project.json"),
            serde_json::to_vec(&json!({"schema":"central.project/v1","project_id":"epi",
                "human_source":"ProjectCentral/user",
                "wiki":{"profile":"okf-wiki/v1","source":"ProjectCentral/agents/wiki/wiki.json"}}))
            .unwrap(),
        )
        .unwrap();
        run(
            &world.root,
            "scope-register",
            &json!({"name":"epi","path":epi,"allow_external":true,"expected_revision":world.revision()}),
        )
        .unwrap();
        assert_eq!(
            words(epi.join("Audio").to_str().unwrap()),
            format!(
                "{} is inside the external project scope epi ({}): a folder scope never stands inside another scope",
                epi.join("Audio").display(),
                epi.display()
            )
        );
        assert!(!world.root.join(".central/scopes/Elsewhere").exists());
    }

    /// Review 9 (central-47b item 9): the walk never crosses into another volume mounted inside the folder; it names where it
    /// stopped, and a resolve of a file there answers it as outside the scope. The mount is the walk's seam: a fixture holds no
    /// real one.
    #[test]
    fn a_folder_scope_walk_never_crosses_a_mount() {
        let world = World::new();
        put(&world, "Volume/far.wav");
        world
            .register("Music", &world.music(), json!(["wav", "aiff", "mp3"]))
            .unwrap();
        let volume = world.music.join("Volume");
        let seams = || crate::file_map_catalog::FolderWalkSeams {
            other_volume: Some(volume.clone()),
            ..Default::default()
        };
        let read = with_seams(seams(), || {
            run(&world.root, "inspect", &json!({"project":"folder:Music"})).unwrap()
        });
        assert!(!music_paths(&world, &read).contains(&"Volume/far.wav".to_owned()));
        assert_eq!(
            read["folders"][0]["skipped_named"],
            json!([{"path":volume,"why":"another volume is mounted here"}])
        );
        let reference = source_horizon::source_ref("folder:Music", "Volume/far.wav");
        let resolved = with_seams(seams(), || {
            refused(run(
                &world.root,
                "resolve",
                &json!({"source_ref":reference}),
            ))
        });
        assert!(
            resolved.contains("outside participating maps"),
            "{resolved}"
        );
    }

    /// Review 2 (central-47b item 2): the walk is bounded by a count of folders (here) and by time (the deadline tests). A bound
    /// that stops it makes the page partial: `truncated`, with words naming the bound, and a `next` that misses nothing, so the
    /// pages still list the whole folder, each entry once.
    #[test]
    fn a_folder_scope_walk_stops_at_its_directory_bound_and_says_so() {
        let world = World::new();
        for folder in ["A", "B", "C", "D", "E"] {
            put(&world, &format!("{folder}/one.wav"));
            put(&world, &format!("{folder}/Deeper/two.wav"));
        }
        world
            .register("Music", &world.music(), json!(["wav", "aiff", "mp3"]))
            .unwrap();
        let whole = music_paths(
            &world,
            &run(&world.root, "inspect", &json!({"project":"folder:Music"})).unwrap(),
        );
        assert_eq!(whole.len(), 13);
        // Three folders a page: the pages go on until the walk is done, and together they are the whole listing.
        let mut listed = Vec::new();
        let mut after = Value::Null;
        let mut pages = 0;
        loop {
            let answer = with_seams(
                crate::file_map_catalog::FolderWalkSeams {
                    directories: Some(3),
                    ..Default::default()
                },
                || {
                    run(
                        &world.root,
                        "inspect",
                        &json!({"project":"folder:Music","limit":100,"after":after}),
                    )
                    .unwrap()
                },
            );
            pages += 1;
            assert!(pages < 20, "the pages did not end");
            listed.extend(music_paths(&world, &answer));
            let page = &answer["page"];
            if page["truncated"] != true {
                break;
            }
            assert_eq!(
                page["bound"],
                "the directory bound stopped the walk: 3 folders were read before the page filled",
                "{answer}"
            );
            assert_eq!(answer["folders"][0]["bound"], page["bound"]);
            assert!(page["next"].is_string(), "{answer}");
            after = page["next"].clone();
        }
        assert!(pages > 2, "{pages}");
        assert_eq!(listed, whole);
        // The time bound is a_folder_walk_runs_once_at_a_time_and_reads_nothing_after_its_deadline's: a logical clock, not load.
    }

    // ---- #328's delta review (DLW build ruling 2026-11-09, round 48 item 0C, central-47d) ----

    /// The time bound's words, as the walk and its reader both say them.
    fn time_words() -> String {
        format!(
            "the time bound stopped the walk: {}s passed before the page filled",
            crate::file_map_catalog::FOLDER_WALK_TIME.as_secs_f64()
        )
    }
    fn walk_log() -> std::sync::Arc<std::sync::Mutex<Vec<PathBuf>>> {
        std::sync::Arc::default()
    }

    /// 0C item 1: a name that is not UTF-8 never aborts a read. The walk skips it, counts it, names it lossily with why, and the rest
    /// answers. Where the filesystem refuses such a name (APFS keeps only UTF-8 names), the walk is handed one through its seam, as a
    /// Linux disk or a network share would hand it.
    #[test]
    fn a_name_that_is_not_utf8_is_skipped_and_named_and_the_rest_answers() {
        use std::os::unix::ffi::OsStrExt;
        let world = World::new();
        let raw = world
            .music
            .join(std::ffi::OsStr::from_bytes(b"caf\xe9.wav"));
        let seams = match fs::write(&raw, b"RIFF sound") {
            Ok(()) => Default::default(),
            Err(_) => {
                put(&world, "cafe.wav");
                crate::file_map_catalog::FolderWalkSeams {
                    not_utf8: vec![(world.music.join("cafe.wav"), b"caf\xe9.wav".to_vec())],
                    ..Default::default()
                }
            }
        };
        world
            .register("Music", &world.music(), json!(["wav", "aiff", "mp3"]))
            .unwrap();
        let answer = with_seams(seams, || {
            run(&world.root, "inspect", &json!({"project":"folder:Music"}))
        })
        .unwrap();
        assert_eq!(
            music_paths(&world, &answer),
            [
                "Field recordings/estuary low tide.wav",
                "market-hall.AIFF",
                "voice.mp3"
            ]
        );
        assert_eq!(answer["folders"][0]["skipped"], 1, "{answer}");
        assert_eq!(
            answer["folders"][0]["skipped_named"],
            json!([{"path": format!("{}/caf\u{FFFD}.wav", world.music()), "why": "its name is not UTF-8"}])
        );
    }

    /// 0C item 2: a project's link into a folder scope stands when the folder does not. Unplugged, or the file outside the folder's
    /// rules, the link is named broken in words, and the project's own inspect and search still answer.
    #[test]
    fn a_project_link_into_a_folder_stands_when_the_folder_does_not() {
        let world = World::new();
        world
            .register("Music", &world.music(), json!(["wav", "aiff", "mp3"]))
            .unwrap();
        let voice = source_horizon::source_ref("folder:Music", "voice.mp3");
        let alpha = |input: Value| {
            let mut input = input;
            input["project"] = json!("Alpha");
            input
        };
        let revision = run(&world.root, "inspect", &alpha(json!({"resources":false}))).unwrap()
            ["revision"]
            .clone();
        run(
            &world.root,
            "link",
            &alpha(json!({"path":"sounds/voice.mp3","source_ref":voice,"owner":"aikit","expected_revision":revision})),
        )
        .unwrap();
        let linked = run(&world.root, "inspect", &alpha(json!({}))).unwrap();
        assert!(
            linked["resources"].to_string().contains("voice.mp3"),
            "{linked}"
        );
        assert!(linked.get("broken_links").is_none(), "{linked}");
        let broken = |words: &str| json!([{"path":"sounds/voice.mp3","source_ref":voice,"world_ref":"folder:Music","words":words}]);
        // Unplugged: the folder is not there.
        let away = world.base.join("Music away");
        fs::rename(&world.music, &away).unwrap();
        let unplugged = "sounds/voice.mp3 links to a file of folder:Music, which is not reachable now, so it is not listed";
        let read = run(&world.root, "inspect", &alpha(json!({}))).unwrap();
        assert_eq!(read["broken_links"], broken(unplugged), "{read}");
        assert!(!read["resources"].to_string().contains("voice.mp3"));
        let searched = run(
            &world.root,
            "search",
            &alpha(json!({"query":"sound","limit":0})),
        )
        .unwrap();
        assert_eq!(searched["broken_links"], broken(unplugged), "{searched}");
        let found = run(&world.root, "search", &alpha(json!({"query":"sound"}))).unwrap();
        assert_eq!(found["broken_links"], broken(unplugged), "{found}");
        fs::rename(&away, &world.music).unwrap();
        // Back, but the folder's declaration no longer admits the file's kind: outside its rules.
        let declaration = world.root.join(".central/scopes/Music/scope.json");
        let mut declared: Value = serde_json::from_slice(&fs::read(&declaration).unwrap()).unwrap();
        declared["media"] = json!(["wav"]);
        fs::write(&declaration, serde_json::to_vec(&declared).unwrap()).unwrap();
        let outside = "sounds/voice.mp3 links to a file folder:Music no longer lists (gone, or outside its rules), so it is not listed";
        let read = run(&world.root, "inspect", &alpha(json!({}))).unwrap();
        assert_eq!(read["broken_links"], broken(outside), "{read}");
        let searched = run(
            &world.root,
            "search",
            &alpha(json!({"query":"sound","limit":0})),
        )
        .unwrap();
        assert_eq!(searched["broken_links"], broken(outside), "{searched}");
        let found = run(&world.root, "search", &alpha(json!({"query":"sound"}))).unwrap();
        assert_eq!(found["broken_links"], broken(outside), "{found}");
    }

    /// central-47e item 2 (review 2): a link into a folder whose lookup errs (here a folder on its way that cannot be read, as an
    /// unreadable share or a disk's error leaves one) is named in `broken_links` with its cause, beside the unplugged and
    /// out-of-rules cases, and the rest of the project's read stands, in `inspect` and in `search` (its main path too).
    #[test]
    fn a_project_link_into_a_folder_that_errs_is_named_with_its_cause() {
        let world = World::new();
        put(&world, "Samples/kick.wav");
        world
            .register("Music", &world.music(), json!(["wav", "aiff", "mp3"]))
            .unwrap();
        let kick = source_horizon::source_ref("folder:Music", "Samples/kick.wav");
        let alpha = |input: Value| {
            let mut input = input;
            input["project"] = json!("Alpha");
            input
        };
        let revision = run(&world.root, "inspect", &alpha(json!({"resources":false}))).unwrap()
            ["revision"]
            .clone();
        run(
            &world.root,
            "link",
            &alpha(json!({"path":"sounds/kick.wav","source_ref":kick,"owner":"aikit","expected_revision":revision})),
        )
        .unwrap();
        let samples = world.music.join("Samples");
        fs::set_permissions(&samples, fs::Permissions::from_mode(0o000)).unwrap();
        let reads = (
            run(&world.root, "inspect", &alpha(json!({}))),
            run(
                &world.root,
                "search",
                &alpha(json!({"query":"sound","limit":0})),
            ),
            run(&world.root, "search", &alpha(json!({"query":"sound"}))),
        );
        fs::set_permissions(&samples, fs::Permissions::from_mode(0o755)).unwrap();
        for read in [reads.0, reads.1, reads.2] {
            let read = read.unwrap();
            let rows = read["broken_links"].as_array().cloned().unwrap_or_default();
            assert_eq!(rows.len(), 1, "{read}");
            let words = rows[0]["words"].as_str().unwrap();
            assert!(
                words.starts_with(
                    "sounds/kick.wav links to a file of folder:Music, which cannot be read now ("
                ) && words.contains("ermission denied")
                    && words.ends_with("), so it is not listed"),
                "{words}"
            );
            assert_eq!(rows[0]["source_ref"], json!(kick));
            assert!(
                !read["resources"].to_string().contains("kick.wav"),
                "{read}"
            );
        }
    }

    /// DLW build ruling 2026-11-09, 47f item 2 (review 2): a link into a folder is looked up in that folder alone, so only the
    /// folder's own errors are its cause. Project Beta's relations that do not read, beside Alpha's healthy link into the folder,
    /// are never named as the folder's: Alpha's read, which no longer reads Beta for that link, lists the link as before, and a
    /// read of Beta fails in Beta's own words.
    #[test]
    fn another_scopes_error_is_never_a_folder_links_cause() {
        let world = World::new();
        put(&world, "Samples/kick.wav");
        world
            .register("Music", &world.music(), json!(["wav", "aiff", "mp3"]))
            .unwrap();
        let beta = world.root.join("Work/Beta/ProjectCentral");
        fs::create_dir_all(beta.join("relations")).unwrap();
        fs::write(
            beta.join("project.json"),
            serde_json::to_vec(&json!({"schema":"central.project/v1","project_id":"beta",
                "human_source":"ProjectCentral/user",
                "wiki":{"profile":"okf-wiki/v1","source":"ProjectCentral/agents/wiki/wiki.json"}}))
            .unwrap(),
        )
        .unwrap();
        let kick = source_horizon::source_ref("folder:Music", "Samples/kick.wav");
        let alpha = |input: Value| {
            let mut input = input;
            input["project"] = json!("Alpha");
            input
        };
        let revision = run(&world.root, "inspect", &alpha(json!({"resources":false}))).unwrap()
            ["revision"]
            .clone();
        run(
            &world.root,
            "link",
            &alpha(json!({"path":"sounds/kick.wav","source_ref":kick,"owner":"aikit","expected_revision":revision})),
        )
        .unwrap();
        let healthy = run(&world.root, "inspect", &alpha(json!({}))).unwrap();
        assert!(healthy.get("broken_links").is_none(), "{healthy}");
        fs::write(beta.join("relations/source-relations.json"), b"{not json").unwrap();
        let reads = [
            run(&world.root, "inspect", &alpha(json!({}))),
            run(
                &world.root,
                "search",
                &alpha(json!({"query":"sound","limit":0})),
            ),
            run(&world.root, "search", &alpha(json!({"query":"sound"}))),
        ];
        for read in reads {
            let read = read.unwrap();
            assert!(read.get("broken_links").is_none(), "{read}");
        }
        let listed = run(&world.root, "inspect", &alpha(json!({}))).unwrap();
        assert!(
            listed["resources"].to_string().contains("kick.wav"),
            "{listed}"
        );
        let beta = refused(run(&world.root, "inspect", &json!({"project":"Beta"})));
        assert!(
            beta.contains("key must be a string at line 1 column 2"),
            "{beta}"
        );
        assert!(!beta.contains("folder:Music"), "{beta}");
    }

    /// DLW build ruling 2026-11-09, 47g item 2 (47f review 2): only a link into a folder is looked up in that folder alone. An
    /// ordinary link, from Alpha to a file of project Gamma, is looked up across every scope as before, so project Beta's relations
    /// that do not read fail Alpha's read in Beta's own words.
    #[test]
    fn an_ordinary_link_is_looked_up_across_every_scope_as_before() {
        let world = World::new();
        let project = |name: &str, id: &str| {
            let central = world.root.join(format!("Work/{name}/ProjectCentral"));
            fs::create_dir_all(central.join("relations")).unwrap();
            fs::create_dir_all(central.join("user")).unwrap();
            fs::write(
                central.join("project.json"),
                serde_json::to_vec(&json!({"schema":"central.project/v1","project_id":id,
                    "human_source":"ProjectCentral/user",
                    "wiki":{"profile":"okf-wiki/v1","source":"ProjectCentral/agents/wiki/wiki.json"}}))
                .unwrap(),
            )
            .unwrap();
            central
        };
        let beta = project("Beta", "beta");
        let gamma = project("Gamma", "gamma");
        fs::write(gamma.join("user/native.md"), b"a note of Gamma's\n").unwrap();
        let native = source_horizon::project_source_bindings(&world.root.join("Work/Gamma"))
            .unwrap()
            .into_iter()
            .find(|binding| binding.path == "ProjectCentral/user/native.md")
            .unwrap();
        let alpha = |input: Value| {
            let mut input = input;
            input["project"] = json!("Alpha");
            input
        };
        let revision = run(&world.root, "inspect", &alpha(json!({"resources":false}))).unwrap()
            ["revision"]
            .clone();
        run(
            &world.root,
            "link",
            &alpha(
                json!({"path":"notes/gamma.md","source_ref":native.source_ref,"owner":"aikit",
                "expected_revision":revision}),
            ),
        )
        .unwrap();
        let healthy = run(&world.root, "inspect", &alpha(json!({}))).unwrap();
        assert!(healthy.get("broken_links").is_none(), "{healthy}");
        fs::write(beta.join("relations/source-relations.json"), b"{not json").unwrap();
        let failed = refused(run(&world.root, "inspect", &alpha(json!({}))));
        assert!(
            failed.contains("key must be a string at line 1 column 2"),
            "{failed}"
        );
        assert!(
            !failed.contains("Registered link owner is unavailable"),
            "{failed}"
        );
    }

    /// 0C item 3: no index for a folder. adopt-db and record-adopt refuse a folder scope in the projection acts' own words, and
    /// bkmr is never run with a folder as its working directory; the folder's tree stays as it was, byte for byte.
    #[test]
    fn no_act_keeps_an_index_for_a_folder_scope_nor_runs_bkmr_in_it() {
        let world = World::new();
        world
            .register("Music", &world.music(), json!(["wav", "aiff", "mp3"]))
            .unwrap();
        let before = world.music_bytes();
        let words = format!(
            "Music is a folder scope: Central reads {} where it stands and keeps no index or projection receipt for it",
            world.music()
        );
        let voice = source_horizon::source_ref("folder:Music", "voice.mp3");
        for (op, input) in [
            (
                "adopt-db",
                json!({"project":"folder:Music","source":world.base.join("index.db"),"expected_revision":world.revision()}),
            ),
            (
                "record-adopt",
                json!({"project":"folder:Music","source_ref":voice,"expected_revision":world.revision()}),
            ),
            ("projection-read", json!({"project":"folder:Music"})),
        ] {
            assert_eq!(refused(run(&world.root, op, &input)), words, "{op}");
        }
        let all = scopes(&world.root).unwrap();
        let folder = all.iter().find(|scope| scope.is_folder()).unwrap();
        let ran = Backend::for_scope(folder).run(&["info".into()]);
        assert_eq!(
            ran.unwrap_err().to_string(),
            format!(
                "bkmr is never run for a folder scope: Central reads {} where it stands",
                world.music()
            )
        );
        assert_eq!(world.music_bytes(), before);
        assert!(!world.music.join(".central").exists());
        assert!(!world
            .root
            .join(".central/scopes/Music/.central/bkmr")
            .exists());
    }

    /// 0C items 4 and 5: one walk of a folder runs at a time, and it reads nothing after its deadline. The deadline here is a logical
    /// clock (the walk's count of folders read), so the page is the same on any machine; a walk parked in a read that never answers
    /// is left at the bound, and a second read of the folder meanwhile answers at once.
    #[test]
    fn a_folder_walk_runs_once_at_a_time_and_reads_nothing_after_its_deadline() {
        let world = World::new();
        for folder in ["A", "B", "C", "D", "E"] {
            put(&world, &format!("{folder}/one.wav"));
            put(&world, &format!("{folder}/Deeper/two.wav"));
        }
        world
            .register("Music", &world.music(), json!(["wav", "aiff", "mp3"]))
            .unwrap();
        let first = [
            "A/Deeper/two.wav",
            "A/one.wav",
            "B/Deeper/two.wav",
            "B/one.wav",
        ];
        let read = walk_log();
        let answer = with_seams(
            crate::file_map_catalog::FolderWalkSeams {
                deadline_after: Some(5),
                read: Some(read.clone()),
                ..Default::default()
            },
            || run(&world.root, "inspect", &json!({"project":"folder:Music"})),
        )
        .unwrap();
        let page = &answer["page"];
        assert_eq!(page["truncated"], true, "{answer}");
        assert_eq!(page["bound"], time_words());
        assert_eq!(music_paths(&world, &answer), first);
        assert_eq!(page["next"], json!(world.music.join("B")));
        let late: Vec<_> = read
            .lock()
            .unwrap()
            .iter()
            .filter(|path| {
                ["C", "D", "E"]
                    .iter()
                    .any(|after| path.starts_with(world.music.join(after)))
            })
            .cloned()
            .collect();
        assert_eq!(late, Vec::<PathBuf>::new(), "read after the deadline");

        // A walk parked in a read that never answers: the reader leaves it at the bound with what it listed.
        let fifo = world.base.join("stall");
        assert!(std::process::Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .unwrap()
            .success());
        let parked = with_seams(
            crate::file_map_catalog::FolderWalkSeams {
                stall: Some((world.music.join("C"), fifo.clone())),
                ..Default::default()
            },
            || run(&world.root, "inspect", &json!({"project":"folder:Music"})),
        )
        .unwrap();
        assert_eq!(parked["page"]["bound"], time_words(), "{parked}");
        assert_eq!(music_paths(&world, &parked), first);
        // While it is parked, another read of the folder starts no second walk: it answers within its own bound, in words.
        let again = with_seams(
            crate::file_map_catalog::FolderWalkSeams {
                time: Some(std::time::Duration::from_millis(20)),
                ..Default::default()
            },
            || run(&world.root, "inspect", &json!({"project":"folder:Music"})),
        )
        .unwrap();
        assert_eq!(
            again["page"]["bound"],
            "a walk of this folder is still running, so this page lists none of it: ask again when it ends",
            "{again}"
        );
        assert_eq!(music_paths(&world, &again), Vec::<String>::new());
        assert_eq!(again["page"]["truncated"], true);
        // Let the parked walk go; once it has ended, the folder is walked again.
        let mut released = false;
        for _ in 0..400 {
            if fs::OpenOptions::new()
                .write(true)
                .custom_flags(libc::O_NONBLOCK)
                .open(&fifo)
                .is_ok()
            {
                released = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(released);
        let root = fs::canonicalize(&world.music).unwrap();
        for _ in 0..400 {
            if !crate::file_map_catalog::folder_walk_running(&root) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let whole = run(&world.root, "inspect", &json!({"project":"folder:Music"})).unwrap();
        assert_eq!(music_paths(&world, &whole).len(), 13, "{whole}");
    }

    /// 0C item 4: a prefix is read by its components, so a trailing slash or `/` reads as the folder it names; a prefix outside the
    /// folder answers at once, reading nothing.
    #[test]
    fn a_prefix_is_read_by_its_components_and_one_outside_answers_at_once() {
        let world = World::new();
        world
            .register("Music", &world.music(), json!(["wav", "aiff", "mp3"]))
            .unwrap();
        for prefix in [format!("{}/", world.music()), "/".to_owned()] {
            let answer = run(
                &world.root,
                "inspect",
                &json!({"project":"folder:Music","path_prefixes":[prefix]}),
            )
            .unwrap();
            assert_eq!(music_paths(&world, &answer).len(), 3, "{prefix}: {answer}");
            assert_eq!(answer["page"]["truncated"], false, "{prefix}");
        }
        let read = walk_log();
        let elsewhere = world.base.join("Elsewhere");
        let answer = with_seams(
            crate::file_map_catalog::FolderWalkSeams {
                read: Some(read.clone()),
                ..Default::default()
            },
            || {
                run(
                    &world.root,
                    "inspect",
                    &json!({"project":"folder:Music","path_prefixes":[elsewhere]}),
                )
            },
        )
        .unwrap();
        assert_eq!(music_paths(&world, &answer), Vec::<String>::new());
        assert_eq!(answer["page"]["truncated"], false, "{answer}");
        assert_eq!(
            answer["folders"][0],
            json!({"world_ref":"folder:Music","listed":false,"words":"no path_prefixes entry reaches into it"})
        );
        assert_eq!(
            *read.lock().unwrap(),
            Vec::<PathBuf>::new(),
            "the folder was read"
        );
    }

    /// 0C item 4: the default page of a federated read that reaches a folder cuts only the folder's entries; a project's keep their
    /// old paging, whole.
    #[test]
    fn the_federated_default_page_cuts_only_folder_entries() {
        let world = World::new();
        world
            .register("Music", &world.music(), json!(["wav", "aiff", "mp3"]))
            .unwrap();
        // The ground's own entry: a NOW record, which every read lists live.
        let record = world.root.join("Control/agents/now/agents/handoff.json");
        fs::create_dir_all(record.parent().unwrap()).unwrap();
        fs::write(&record, b"{\"kind\":\"handoff\"}").unwrap();
        let ground = world.root.to_str().unwrap().to_owned();
        let projects = |answer: &Value| -> Vec<String> {
            answer["resources"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|entry| !entry["world_ref"].as_str().unwrap().starts_with("folder:"))
                .map(|entry| entry["path"].as_str().unwrap().to_owned())
                .collect()
        };
        let alone = run(
            &world.root,
            "inspect",
            &json!({"federated":true,"path_prefixes":[ground]}),
        )
        .unwrap();
        assert!(!projects(&alone).is_empty(), "{alone}");
        let both = with_seams(
            crate::file_map_catalog::FolderWalkSeams {
                page_default: Some(1),
                ..Default::default()
            },
            || {
                run(
                    &world.root,
                    "inspect",
                    &json!({"federated":true,"path_prefixes":[ground, world.music()]}),
                )
            },
        )
        .unwrap();
        assert_eq!(projects(&both), projects(&alone), "{both}");
        assert_eq!(music_paths(&world, &both).len(), 1, "{both}");
        assert_eq!(both["page"]["truncated"], true);
    }

    /// 0C item 4: a project named `folder:…` never shadows a folder scope: it is unreachable in words, and the folder scope is read.
    #[test]
    fn a_project_named_like_a_folder_scope_never_shadows_it() {
        let world = World::new();
        world
            .register("Music", &world.music(), json!(["wav", "aiff", "mp3"]))
            .unwrap();
        let shadow = world.root.join("Work/folder:Music/ProjectCentral");
        fs::create_dir_all(&shadow).unwrap();
        fs::write(
            shadow.join("project.json"),
            serde_json::to_vec(&json!({"schema":"central.project/v1","project_id":"shadow",
                "human_source":"ProjectCentral/user",
                "wiki":{"profile":"okf-wiki/v1","source":"ProjectCentral/agents/wiki/wiki.json"}}))
            .unwrap(),
        )
        .unwrap();
        let (all, unreachable) = participating(&world.root.canonicalize().unwrap()).unwrap();
        assert!(all
            .iter()
            .any(|scope| scope.world == "folder:Music" && scope.is_folder()));
        assert!(all
            .iter()
            .all(|scope| scope.project.as_deref() != Some("folder:Music") || scope.is_folder()));
        assert_eq!(
            unreachable.iter().map(|scope| scope.message.as_str()).collect::<Vec<_>>(),
            ["unreachable: a project named folder:Music (Work/folder:Music) does not stand: folder: names a folder scope, so the project is not read"]
        );
        let read = run(&world.root, "inspect", &json!({"project":"folder:Music"})).unwrap();
        assert_eq!(read["world_ref"], "folder:Music");
        assert_eq!(music_paths(&world, &read).len(), 3);
    }

    /// 0C item 4: a folder scope's name is one plain name: `Music/` is refused at registration, and nothing is made.
    #[test]
    fn a_folder_scope_name_with_a_trailing_slash_is_refused_at_registration() {
        let world = World::new();
        let words = refused(world.register("Music/", &world.music(), json!(["wav"])));
        assert_eq!(
            words,
            "A folder scope's name is one plain name, and Music/ is not: name it Music"
        );
        assert!(!world.root.join(".central/scopes/Music").exists());
        world
            .register("Music", &world.music(), json!(["wav"]))
            .unwrap();
    }

    /// 0C items 4 and 5: the world's refresh, run directly: every row has one shape (numbers for entries, changes and truncated),
    /// a folder's row says it builds no index, and an unreachable folder is named, its row shaped as the others.
    #[test]
    fn the_world_refresh_answers_one_shape_for_every_row() {
        let world = World::new();
        world
            .register("Music", &world.music(), json!(["wav", "aiff", "mp3"]))
            .unwrap();
        let sounds = world.base.join("Sounds");
        fs::create_dir_all(&sounds).unwrap();
        world
            .register("Sounds", sounds.to_str().unwrap(), json!(["wav"]))
            .unwrap();
        fs::remove_dir_all(&sounds).unwrap();
        // Every bkmr run of this refresh (a project's, and its version) has a home under the fixture, never the user's.
        let home = world.base.join("home");
        fs::create_dir_all(&home).unwrap();
        let answer = with_bkmr(
            crate::file_map_backend::BkmrSeams {
                home: Some(home),
                ..Default::default()
            },
            || execute(&world.root, "refresh", &json!({"federated":true})),
        )
        .unwrap();
        let result = &answer["result"];
        let rows = result["scopes"].as_array().unwrap();
        for row in rows {
            for key in ["entries", "changes", "truncated"] {
                assert!(row[key].is_u64(), "{key} of {row}");
            }
            assert!(row["diagnostics"].is_array(), "{row}");
            assert!(row["seconds"].is_number(), "{row}");
            assert!(row["ok"].is_boolean(), "{row}");
        }
        let music = rows
            .iter()
            .find(|row| row["world_ref"] == "folder:Music")
            .unwrap();
        assert_eq!(
            (music["ok"].clone(), music["indexed"].clone()),
            (json!(true), json!(false)),
            "{music}"
        );
        let gone = rows
            .iter()
            .find(|row| row["world_ref"] == "folder:Sounds")
            .unwrap();
        assert_eq!(
            (gone["ok"].clone(), gone["unreachable"].clone()),
            (json!(false), json!(true)),
            "{gone}"
        );
        let failed = rows.iter().filter(|row| row["ok"] != true).count();
        assert_eq!(result["failed"], failed);
        assert_eq!(result["refreshed"], rows.len() - failed);
        assert!(!world.music.join(".central").exists());
    }

    /// 0C item 4: the words and kinds name their own cause: a folder gone is not found, not a timeout; a resolve of a file of an
    /// unreachable folder names the folder; a move of a folder's file says Central moves nothing there.
    #[test]
    fn a_folder_scope_failure_names_its_own_cause() {
        let world = World::new();
        world
            .register("Music", &world.music(), json!(["wav", "aiff", "mp3"]))
            .unwrap();
        let voice = source_horizon::source_ref("folder:Music", "voice.mp3");
        // A move is the person's own act (`the_persons_act`): it is their press that meets the folder's words.
        let person = world.person_moves();
        let moved = refused(execute_with_token(
            &world.root,
            "move-plan",
            &json!({"source_ref":voice,"destination":"Elsewhere/voice.mp3","expected_revision":world.revision()}),
            Some(person),
        ));
        assert!(moved.contains("writes nothing inside it"), "{moved}");
        let away = world.base.join("Music away");
        fs::rename(&world.music, &away).unwrap();
        let gone = run(&world.root, "inspect", &json!({"project":"folder:Music"})).unwrap_err();
        assert_eq!(gone.kind(), io::ErrorKind::NotFound, "{gone}");
        let resolved = refused(run(&world.root, "resolve", &json!({"source_ref":voice})));
        assert_eq!(
            resolved,
            format!(
                "Source is in folder:Music, which is unreachable: the folder scope Music stands over {}, and no folder is there now",
                world.music()
            )
        );
        fs::rename(&away, &world.music).unwrap();
    }

    /// 0C item 5: every one of the 24 package extensions is a package, in the walk and in a resolve: nothing inside one is listed.
    #[test]
    fn every_package_extension_is_a_package_in_the_walk_and_in_a_resolve() {
        let world = World::new();
        let packages = [
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
        assert_eq!(packages.len(), 24);
        // Every extension in lower case, and one in upper case: a package is known by its extension whatever its case.
        let extensions: Vec<String> = packages
            .iter()
            .map(|extension| extension.to_string())
            .chain(["LOGICX".to_owned()])
            .collect();
        for extension in &extensions {
            put(&world, &format!("Inside.{extension}/kept.wav"));
        }
        world
            .register("Music", &world.music(), json!(["wav", "aiff", "mp3"]))
            .unwrap();
        let answer = run(&world.root, "inspect", &json!({"project":"folder:Music"})).unwrap();
        assert_eq!(music_paths(&world, &answer).len(), 3, "{answer}");
        // A file beside the packages resolves: the refusals below are the packages', not every resolve's.
        let voice = source_horizon::source_ref("folder:Music", "voice.mp3");
        let control = run(&world.root, "resolve", &json!({"source_ref":voice})).unwrap();
        assert_eq!(control["kind"], "audio", "{control}");
        for extension in &extensions {
            let reference =
                source_horizon::source_ref("folder:Music", &format!("Inside.{extension}/kept.wav"));
            let words = refused(run(
                &world.root,
                "resolve",
                &json!({"source_ref":reference}),
            ));
            assert!(
                words.contains("outside participating maps"),
                "{extension}: {words}"
            );
        }
    }

    /// 0C item 5: the spellings registration refuses are refused in a hand-edited declaration too: a trailing slash, `..`, a link,
    /// another case. Each is unreachable in words; the declaration as Central wrote it opens again.
    #[test]
    fn a_hand_edited_declaration_is_refused_for_every_spelling_registration_refuses() {
        let world = World::new();
        world
            .register("Music", &world.music(), json!(["wav"]))
            .unwrap();
        let ground = world.root.canonicalize().unwrap();
        let declaration = world.root.join(".central/scopes/Music/scope.json");
        let declared: Value = serde_json::from_slice(&fs::read(&declaration).unwrap()).unwrap();
        let canonical = declared["root"].as_str().unwrap().to_owned();
        let link = world.base.join("Music link");
        std::os::unix::fs::symlink(&canonical, &link).unwrap();
        let parent = Path::new(&canonical).parent().unwrap();
        let mut spellings = vec![
            format!("{canonical}/"),
            format!("{}/Field recordings/../", canonical),
            format!("{}/../Music", canonical.trim_end_matches('/')),
            link.to_str().unwrap().to_owned(),
        ];
        // Another case names the same folder only on a volume that folds case.
        let upper = parent.join("MUSIC");
        if upper.is_dir() {
            spellings.push(upper.to_str().unwrap().to_owned());
        }
        let mut opened = Vec::new();
        for spelling in &spellings {
            let mut edited = declared.clone();
            edited["root"] = json!(spelling);
            fs::write(&declaration, serde_json::to_vec(&edited).unwrap()).unwrap();
            let (all, unreachable) = participating(&ground).unwrap();
            let refused = !all.iter().any(|scope| scope.world == "folder:Music")
                && unreachable.iter().any(|scope| {
                    scope.message.starts_with(&format!(
                        "unreachable: the folder scope Music stands over {spelling}, and it did not open: "
                    ))
                });
            if !refused {
                opened.push(spelling.clone());
            }
        }
        assert_eq!(
            opened,
            Vec::<String>::new(),
            "opened by a spelling that is not the folder's own"
        );
        fs::write(&declaration, serde_json::to_vec(&declared).unwrap()).unwrap();
        let (all, unreachable) = participating(&ground).unwrap();
        assert!(unreachable.is_empty());
        assert!(all.iter().any(|scope| scope.world == "folder:Music"));
    }

    // ---- #332's review (DLW build ruling 2026-11-09, central-47e) ----

    fn with_bkmr<T>(seams: crate::file_map_backend::BkmrSeams, act: impl FnOnce() -> T) -> T {
        crate::file_map_backend::BKMR_SEAMS.with(|slot| *slot.borrow_mut() = seams);
        let out = act();
        crate::file_map_backend::BKMR_SEAMS.with(|slot| *slot.borrow_mut() = Default::default());
        out
    }
    fn fifo(world: &World) -> PathBuf {
        let fifo = world.base.join("stall");
        assert!(std::process::Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .unwrap()
            .success());
        fifo
    }
    /// Lets a walk parked on `fifo` go: true once it was let go.
    fn release(fifo: &Path) -> bool {
        for _ in 0..400 {
            if fs::OpenOptions::new()
                .write(true)
                .custom_flags(libc::O_NONBLOCK)
                .open(fifo)
                .is_ok()
            {
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        false
    }
    /// Whether the walk of the folder at `root` has ended.
    fn walk_ends(root: &Path) -> bool {
        for _ in 0..400 {
            if !crate::file_map_catalog::folder_walk_running(root) {
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        false
    }

    /// central-47e item 1 (review 1): the reader's own time bound, not a word from the walk, lets it go. The walk parks in its first
    /// read and says nothing; at 50 ms the reader answers an empty page in the time bound's words, with nowhere to go on from. Let
    /// go after the reader has left, the walk reads nothing more: not even the listing it was parked before (47f item 5, the check
    /// after a folder's own read). The walk's own deadline is a logical clock that never passes, so only the reader's leaving stops
    /// it, on any machine.
    #[test]
    fn a_reader_leaves_a_walk_that_never_answers_at_its_own_time_bound() {
        let world = World::new();
        world
            .register("Music", &world.music(), json!(["wav", "aiff", "mp3"]))
            .unwrap();
        let root = fs::canonicalize(&world.music).unwrap();
        let fifo = fifo(&world);
        // Were the reader to wait for the walk, the walk is let go after 20 s, so the test fails rather than hangs.
        {
            let fifo = fifo.clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_secs(20));
                let _ = fs::OpenOptions::new()
                    .write(true)
                    .custom_flags(libc::O_NONBLOCK)
                    .open(&fifo);
            });
        }
        let (read, listed) = (walk_log(), walk_log());
        let answer = with_seams(
            crate::file_map_catalog::FolderWalkSeams {
                time: Some(std::time::Duration::from_millis(50)),
                stall: Some((root.clone(), fifo.clone())),
                silent: true,
                deadline_after: Some(usize::MAX),
                read: Some(read.clone()),
                listed: Some(listed.clone()),
                ..Default::default()
            },
            || run(&world.root, "inspect", &json!({"project":"folder:Music"})),
        )
        .unwrap();
        let page = &answer["page"];
        assert_eq!(
            page["bound"], "the time bound stopped the walk: 0.05s passed before the page filled",
            "{answer}"
        );
        assert_eq!(music_paths(&world, &answer), Vec::<String>::new());
        assert_eq!(
            (page["truncated"].clone(), page["next"].clone()),
            (json!(true), Value::Null)
        );
        assert!(release(&fifo), "the walk was not parked");
        assert!(walk_ends(&root), "the walk did not end");
        assert_eq!(
            *read.lock().unwrap(),
            vec![root.clone()],
            "the walk read on after its reader left"
        );
        assert_eq!(
            *listed.lock().unwrap(),
            Vec::<PathBuf>::new(),
            "the walk listed its folder after its reader left"
        );
    }

    /// DLW build ruling 2026-11-09, 47f item 5 (review 5): a folder row's `lstat`, or its bundle probe, that does not answer at once
    /// outlasts its reader, and once let go the walk reads nothing more: not the probe after the `lstat`, not the folder's listing
    /// after the probe. Each stall says nothing; the reader leaves at its own time bound, and the walk's deadline is a logical clock
    /// that never passes.
    #[test]
    fn a_folder_rows_read_that_stalls_reads_nothing_after_its_reader_left() {
        let world = World::new();
        world
            .register("Music", &world.music(), json!(["wav", "aiff", "mp3"]))
            .unwrap();
        let root = fs::canonicalize(&world.music).unwrap();
        let field = root.join("Field recordings");
        let probe = field.join("Contents");
        for (at, expected) in [
            (field.clone(), vec![root.clone(), field.clone()]),
            (
                probe.clone(),
                vec![root.clone(), field.clone(), probe.clone()],
            ),
        ] {
            let fifo = fifo(&world);
            let (read, listed) = (walk_log(), walk_log());
            let answer = with_seams(
                crate::file_map_catalog::FolderWalkSeams {
                    time: Some(std::time::Duration::from_secs(2)),
                    stall: Some((at.clone(), fifo.clone())),
                    silent: true,
                    deadline_after: Some(usize::MAX),
                    read: Some(read.clone()),
                    listed: Some(listed.clone()),
                    ..Default::default()
                },
                || run(&world.root, "inspect", &json!({"project":"folder:Music"})),
            )
            .unwrap();
            assert_eq!(
                answer["page"]["bound"],
                "the time bound stopped the walk: 2s passed before the page filled",
                "{answer}"
            );
            assert!(release(&fifo), "the walk was not parked at {at:?}");
            assert!(walk_ends(&root), "the walk did not end");
            fs::remove_file(&fifo).unwrap();
            assert_eq!(
                *read.lock().unwrap(),
                expected,
                "the walk read on after its reader left, parked at {at:?}"
            );
            assert_eq!(*listed.lock().unwrap(), vec![root.clone()], "{at:?}");
        }
    }

    /// central-47e items 1 and 8 (reviews 1 and 8): past its deadline the walk reads nothing more: not a file's metadata, not a
    /// folder's, not a folder's bundle probe. The deadline passes as the walk reads a chosen path, its time bound 5 s (so a walk that
    /// never ends fails the test then, not after an hour; 47g item 3), so the outcome is the same on any machine.
    #[test]
    fn past_its_deadline_the_walk_reads_no_file_folder_or_bundle_probe() {
        let world = World::new();
        put(&world, "0first.wav");
        world
            .register("Music", &world.music(), json!(["wav", "aiff", "mp3"]))
            .unwrap();
        let root = fs::canonicalize(&world.music).unwrap();
        let walk = |at: &Path| {
            let read = walk_log();
            let answer = with_seams(
                crate::file_map_catalog::FolderWalkSeams {
                    time: Some(std::time::Duration::from_secs(5)),
                    pass_deadline_at: Some(at.to_path_buf()),
                    read: Some(read.clone()),
                    ..Default::default()
                },
                || run(&world.root, "inspect", &json!({"project":"folder:Music"})),
            )
            .unwrap();
            assert_eq!(
                answer["page"]["bound"],
                "the time bound stopped the walk: 5s passed before the page filled",
                "{answer}"
            );
            assert_eq!(music_paths(&world, &answer), Vec::<String>::new());
            let log = read.lock().unwrap().clone();
            assert!(!crate::file_map_catalog::folder_walk_running(&root));
            log
        };
        // Passed as the folder is listed: its first row, a file, is not read.
        assert_eq!(
            walk(&root),
            vec![root.clone()],
            "a file read past the deadline"
        );
        // With no file first, the first row is a folder: its metadata is not read.
        fs::remove_file(world.music.join("0first.wav")).unwrap();
        assert_eq!(
            walk(&root),
            vec![root.clone()],
            "a folder's metadata read past the deadline"
        );
        // Passed as a folder's metadata is read: its bundle probe is not read.
        let field = root.join("Field recordings");
        assert_eq!(
            walk(&field),
            [root.clone(), field.clone()],
            "a bundle probe read past the deadline"
        );
    }

    /// DLW build ruling 2026-11-09, 47f items 4 and 5 (reviews 4 and 5): a read that waited for another walk of its folder walks its
    /// own bound after the wait, and its page is its own: the still-walking words are said only while a walk runs, and the page keeps
    /// what its walk skipped. Its own walk's deadline is a logical clock (it passes as the walk reads `Field recordings`), and its
    /// time bound is short, so a walk never let go fails the test in that bound rather than holding it for an hour.
    #[test]
    fn a_read_that_waited_out_another_walk_walks_its_own_bound_and_keeps_its_page() {
        let world = World::new();
        put(&world, "Entered-e/inside.wav");
        world
            .register("Music", &world.music(), json!(["wav", "aiff", "mp3"]))
            .unwrap();
        let root = fs::canonicalize(&world.music).unwrap();
        let fifo = fifo(&world);
        let first = with_seams(
            crate::file_map_catalog::FolderWalkSeams {
                stall: Some((root.clone(), fifo.clone())),
                ..Default::default()
            },
            || run(&world.root, "inspect", &json!({"project":"folder:Music"})),
        )
        .unwrap();
        assert_eq!(first["page"]["bound"], time_words(), "{first}");
        // Once a second read waits for it, the first walk is let go.
        let releaser = {
            let (fifo, root) = (fifo.clone(), root.clone());
            std::thread::spawn(move || {
                for _ in 0..4000 {
                    if crate::file_map_catalog::folder_waited(&root) {
                        return release(&fifo);
                    }
                    std::thread::sleep(std::time::Duration::from_millis(5));
                }
                false
            })
        };
        let second = with_seams(
            crate::file_map_catalog::FolderWalkSeams {
                time: Some(std::time::Duration::from_secs(5)),
                not_utf8: vec![(root.join("Entered-e"), b"Ent\xe9red".to_vec())],
                pass_deadline_at: Some(root.join("Field recordings")),
                ..Default::default()
            },
            || run(&world.root, "inspect", &json!({"project":"folder:Music"})),
        )
        .unwrap();
        let running = crate::file_map_catalog::folder_walk_running(&root);
        assert!(releaser.join().unwrap(), "the first walk was not let go");
        assert!(!running, "a walk still ran when the read answered");
        assert_eq!(
            second["page"]["bound"],
            "the time bound stopped the walk: 5s passed before the page filled",
            "{second}"
        );
        assert_eq!(music_paths(&world, &second), Vec::<String>::new());
        assert_eq!(second["page"]["next"], Value::Null);
        let folder = &second["folders"][0];
        assert_eq!(folder["skipped"], 1, "{second}");
        assert_eq!(
            folder["skipped_named"].as_array().map(Vec::len),
            Some(1),
            "{second}"
        );
        assert!(walk_ends(&root));
        let whole = run(&world.root, "inspect", &json!({"project":"folder:Music"})).unwrap();
        assert_eq!(music_paths(&world, &whole).len(), 4, "{whole}");
    }

    /// DLW build ruling 2026-11-09, 47g item 4 (47f review, point b): a read that waited out another walk of its folder takes its
    /// own bound from the end of the wait, not from its start. The first walk is let go 400 ms after the second read is known to
    /// wait; the second walk parks at the root and says nothing, so only its reader's own bound of 2 s ends the read, and the read
    /// answers no sooner than that bound after the release. The assertion is a least time: a loaded machine cannot fail the least
    /// time; it can only fail the wait, which has 1.6 s (central-47h item 3: the bound was 1 s, a margin of 600 ms).
    #[test]
    fn a_read_that_waited_walks_its_own_bound_after_the_wait() {
        let world = World::new();
        world
            .register("Music", &world.music(), json!(["wav", "aiff", "mp3"]))
            .unwrap();
        let root = fs::canonicalize(&world.music).unwrap();
        let fifo_a = fifo(&world);
        let fifo_b = world.base.join("stall-b");
        assert!(std::process::Command::new("mkfifo")
            .arg(&fifo_b)
            .status()
            .unwrap()
            .success());
        let first = with_seams(
            crate::file_map_catalog::FolderWalkSeams {
                stall: Some((root.clone(), fifo_a.clone())),
                ..Default::default()
            },
            || run(&world.root, "inspect", &json!({"project":"folder:Music"})),
        )
        .unwrap();
        assert_eq!(first["page"]["bound"], time_words(), "{first}");
        let releaser = {
            let (fifo_a, root) = (fifo_a.clone(), root.clone());
            std::thread::spawn(move || {
                for _ in 0..4000 {
                    if crate::file_map_catalog::folder_waited(&root) {
                        std::thread::sleep(std::time::Duration::from_millis(400));
                        let released = std::time::Instant::now();
                        return (release(&fifo_a), Some(released));
                    }
                    std::thread::sleep(std::time::Duration::from_millis(5));
                }
                (false, None)
            })
        };
        let bound = std::time::Duration::from_secs(2);
        let second = with_seams(
            crate::file_map_catalog::FolderWalkSeams {
                time: Some(bound),
                stall: Some((root.clone(), fifo_b.clone())),
                silent: true,
                deadline_after: Some(usize::MAX),
                ..Default::default()
            },
            || run(&world.root, "inspect", &json!({"project":"folder:Music"})),
        )
        .unwrap();
        let answered = std::time::Instant::now();
        let (let_go, released) = releaser.join().unwrap();
        assert!(let_go, "the first walk was not let go");
        assert_eq!(
            second["page"]["bound"],
            "the time bound stopped the walk: 2s passed before the page filled",
            "{second}"
        );
        let after_release = answered.duration_since(released.unwrap());
        assert!(after_release >= bound - std::time::Duration::from_millis(50),
            "the read answered {after_release:?} after the release: its walk had less than its own bound");
        assert!(release(&fifo_b));
        assert!(walk_ends(&root));
    }

    /// central-47e item 3 (review 3): a read's path prefixes are normalised once, and every use reads them so. One that climbs with
    /// `..`, is empty or is not absolute is refused in words; a lone `.` component is dropped; a prefix equal to the folder lists
    /// it all.
    #[test]
    fn a_path_prefix_is_normalised_once_and_one_that_climbs_or_is_empty_is_refused() {
        let world = World::new();
        world
            .register("Music", &world.music(), json!(["wav", "aiff", "mp3"]))
            .unwrap();
        let music = world.music();
        for (prefix, words) in [
            (
                format!("{music}/.."),
                format!("The path_prefixes entry {music}/.. climbs with ..: name the folder it means"),
            ),
            (
                format!("{music}/../Music"),
                format!("The path_prefixes entry {music}/../Music climbs with ..: name the folder it means"),
            ),
            (
                String::new(),
                "A path_prefixes entry is empty: each is an absolute path".to_owned(),
            ),
            (
                ".".to_owned(),
                "The path_prefixes entry . is not an absolute path".to_owned(),
            ),
        ] {
            for read in [
                json!({"project":"folder:Music","path_prefixes":[prefix]}),
                json!({"federated":true,"path_prefixes":[prefix]}),
            ] {
                assert_eq!(refused(run(&world.root, "inspect", &read)), words, "{read}");
            }
        }
        let listed = |prefix: String| {
            let answer = run(
                &world.root,
                "inspect",
                &json!({"project":"folder:Music","path_prefixes":[prefix]}),
            )
            .unwrap();
            assert_eq!(answer["page"]["truncated"], false, "{answer}");
            music_paths(&world, &answer)
        };
        assert_eq!(
            listed(format!("{music}/./Field recordings/.")),
            ["Field recordings/estuary low tide.wav"]
        );
        assert_eq!(listed(music.clone()).len(), 3);
    }

    /// central-47e item 4 (review 4): the default page of a federated read that walks a folder lists each entry once across its
    /// pages: every project entry on the first, whole, and the folder's entries page by page.
    #[test]
    fn a_continued_default_page_lists_each_entry_once() {
        let world = World::new();
        world
            .register("Music", &world.music(), json!(["wav", "aiff", "mp3"]))
            .unwrap();
        let record = world.root.join("Control/agents/now/agents/handoff.json");
        fs::create_dir_all(record.parent().unwrap()).unwrap();
        fs::write(&record, b"{\"kind\":\"handoff\"}").unwrap();
        let ground = world.root.to_str().unwrap().to_owned();
        let paths = |answer: &Value| -> Vec<String> {
            answer["resources"]
                .as_array()
                .unwrap()
                .iter()
                .map(|entry| entry["path"].as_str().unwrap().to_owned())
                .collect()
        };
        let projects = paths(
            &run(
                &world.root,
                "inspect",
                &json!({"federated":true,"path_prefixes":[ground]}),
            )
            .unwrap(),
        );
        assert!(!projects.is_empty());
        let mut listed = Vec::new();
        let mut after = Value::Null;
        for pages in 1.. {
            assert!(pages < 10, "the pages did not end");
            let answer = with_seams(
                crate::file_map_catalog::FolderWalkSeams {
                    page_default: Some(1),
                    ..Default::default()
                },
                || {
                    run(
                        &world.root,
                        "inspect",
                        &json!({"federated":true,"path_prefixes":[ground, world.music()],"after":after}),
                    )
                },
            )
            .unwrap();
            listed.extend(paths(&answer));
            if answer["page"]["truncated"] != true {
                break;
            }
            after = answer["page"]["next"].clone();
        }
        let mut once = listed.clone();
        once.sort();
        once.dedup();
        assert_eq!(once.len(), listed.len(), "listed twice: {listed:#?}");
        let mut expected = projects;
        expected.extend(
            [
                "Field recordings/estuary low tide.wav",
                "market-hall.AIFF",
                "voice.mp3",
            ]
            .map(|path| format!("{}/{path}", world.music())),
        );
        expected.sort();
        assert_eq!(once, expected);
    }

    /// DLW build ruling 2026-11-09, 47f item 3 (review 3): `after` continues a page even when no folder is walked on it, the
    /// folder unplugged between the pages: the project entries the first page listed whole are not listed again. The Action's
    /// description says what `after` and a `limit` page.
    #[test]
    fn a_continuation_with_the_folder_unplugged_lists_no_project_entry_again() {
        let world = World::new();
        world
            .register("Music", &world.music(), json!(["wav", "aiff", "mp3"]))
            .unwrap();
        let record = world.root.join("Control/agents/now/agents/handoff.json");
        fs::create_dir_all(record.parent().unwrap()).unwrap();
        fs::write(&record, b"{\"kind\":\"handoff\"}").unwrap();
        let ground = world.root.to_str().unwrap().to_owned();
        let read = |after: &Value| {
            with_seams(
                crate::file_map_catalog::FolderWalkSeams {
                    page_default: Some(1),
                    ..Default::default()
                },
                || {
                    run(
                        &world.root,
                        "inspect",
                        &json!({"federated":true,"path_prefixes":[ground, world.music()],"after":after}),
                    )
                },
            )
            .unwrap()
        };
        let first = read(&Value::Null);
        let projects = first["resources"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|entry| entry["world_ref"] != "folder:Music")
            .count();
        assert!(projects > 0, "{first}");
        assert_eq!(first["page"]["truncated"], true, "{first}");
        let next = first["page"]["next"].clone();
        let away = world.base.join("Music away");
        fs::rename(&world.music, &away).unwrap();
        let continued = read(&next);
        fs::rename(&away, &world.music).unwrap();
        assert_eq!(continued["resources"], json!([]), "{continued}");
        assert_eq!(
            continued["page"],
            json!({"limit":1,"after":next,"truncated":false,"next":null})
        );

        let mut registry = ActionRegistry::default();
        register(&mut registry);
        let description = registry
            .get("central.file-map.inspect")
            .unwrap()
            .description
            .clone();
        assert!(
            description.contains("With no `limit`, only the folders' entries are paged: a read's first page lists every project \
                 entry whole, and `after` (the `next` of the page before) continues a page and lists none of them; a `limit` pages \
                 all entries."),
            "{description}"
        );
    }

    /// central-47e item 5 (review 5): a folder that is unreachable names its own cause in every read: asked for by its World (a
    /// project that only shares its spelling never answers for it), a Source of it in a batch, and a location in it.
    #[test]
    fn an_unreachable_folder_names_its_own_cause_in_every_read() {
        let world = World::new();
        world
            .register("Music", &world.music(), json!(["wav", "aiff", "mp3"]))
            .unwrap();
        let shadow = world.root.join("Work/folder:Music/ProjectCentral");
        fs::create_dir_all(&shadow).unwrap();
        fs::write(
            shadow.join("project.json"),
            serde_json::to_vec(&json!({"schema":"central.project/v1","project_id":"shadow",
                "human_source":"ProjectCentral/user",
                "wiki":{"profile":"okf-wiki/v1","source":"ProjectCentral/agents/wiki/wiki.json"}}))
            .unwrap(),
        )
        .unwrap();
        let voice = source_horizon::source_ref("folder:Music", "voice.mp3");
        let away = world.base.join("Music away");
        fs::rename(&world.music, &away).unwrap();
        let cause = format!(
            "unreachable: the folder scope Music stands over {}, and no folder is there now",
            world.music()
        );
        let asked = run(&world.root, "inspect", &json!({"project":"folder:Music"})).unwrap_err();
        let batch = run(
            &world.root,
            "resolve-batch",
            &json!({"source_refs":[voice]}),
        );
        let located = run(
            &world.root,
            "locate",
            &json!({"path": world.music.join("voice.mp3")}),
        );
        fs::rename(&away, &world.music).unwrap();
        assert_eq!(
            (asked.kind(), asked.to_string()),
            (io::ErrorKind::NotFound, cause.clone())
        );
        let batch = batch.unwrap();
        assert_eq!(
            batch["readings"][0]["refusal"]["message"],
            format!("Source is in folder:Music, which is {cause}"),
            "{batch}"
        );
        assert_eq!(
            batch["readings"][0]["refusal"]["code"],
            "central.file_map_not_found"
        );
        let located = located.unwrap_err();
        assert_eq!(
            (located.kind(), located.to_string()),
            (
                io::ErrorKind::NotFound,
                format!("Location is in folder:Music, which is {cause}")
            )
        );
    }

    /// DLW build ruling 2026-11-09, 47f item 1 (review 1): a folder whose declaration does not read names no root, so it never
    /// fails another scope's `locate`: a project's location answers as it did. 47g item 1 (47f review 1): a location no scope holds
    /// keeps its normal answer too, unregistered with `binding_only` and not found without, and only the folder's words, with no
    /// absolute path in them, say it may be in that folder.
    #[test]
    fn a_folder_declaration_that_does_not_read_fails_no_other_scopes_locate() {
        let world = World::new();
        world
            .register("Music", &world.music(), json!(["wav", "aiff", "mp3"]))
            .unwrap();
        fs::write(world.root.join("Work/Alpha/notes.md"), "a note\n").unwrap();
        let located = || {
            [
                json!({"path": world.root.join("Work/Alpha/notes.md"), "binding_only": true}),
                json!({"project": "Alpha", "path": "notes.md", "binding_only": true}),
                json!({"path": world.root.join("Work/Alpha/notes.md")}),
            ]
            .map(|input| {
                run(&world.root, "locate", &input)
                    .map_err(|error| (error.kind(), error.to_string()))
            })
        };
        let before = located();
        assert!(before[0].is_ok(), "{before:?}");
        fs::write(
            world.root.join(".central/scopes/Music/scope.json"),
            b"not json",
        )
        .unwrap();
        assert_eq!(located(), before);

        let cause = "unreachable: the folder scope declared in .central/scopes/Music does not read: The folder scope \
                     declaration .central/scopes/Music/scope.json does not read as one: expected ident at line 1 column 2";
        let voice = world.music.join("voice.mp3");
        let bound = execute(
            &world.root,
            "locate",
            &json!({"path": voice, "binding_only": true}),
        )
        .unwrap();
        assert_eq!(
            bound["result"],
            json!({"ownership":"unregistered","binding_only":true,"requested_path":voice}),
            "{bound}"
        );
        assert_eq!(
            bound["unreachable"],
            json!([{"project":"Music","root":null,"message":cause,"world_ref":"folder:Music"}]),
            "{bound}"
        );
        let refused = run(&world.root, "locate", &json!({"path": voice})).unwrap_err();
        assert_eq!(
            (refused.kind(), refused.to_string()),
            (
                io::ErrorKind::NotFound,
                format!("Location is unregistered; it may be in folder:Music, which is {cause}")
            )
        );
        for words in [bound.to_string(), refused.to_string()] {
            let base = world.base.to_str().unwrap();
            assert!(
                !words.replace(voice.to_str().unwrap(), "").contains(base),
                "{words}"
            );
        }
    }

    /// A `locate` answer as one line: the result, or the error's kind, words and typed details (`read_failure_details`).
    fn located_shape(result: io::Result<Value>) -> String {
        match result {
            Ok(envelope) => format!("OK {}", envelope["result"]),
            Err(error) => format!(
                "ERR {:?} {error} DETAILS {}",
                error.kind(),
                crate::file_map_catalog::read_failure_details(&error)
            ),
        }
    }

    /// DLW build ruling 2026-11-09, central-47h item 2 (#334 review, finding 2): while a folder's declaration does not read, a typed
    /// refusal of `locate` keeps its kind, its words and its details. A file under a `.no-agent-retrieval` folder that no scope holds
    /// is refused the same, details and all, on a ground with a corrupt folder declaration as on a healthy one. The guards are the
    /// answers expected (central-47i item 1, #336 review, finding 1): on the healthy ground both spellings are the source treatment's
    /// refusal, held at source admission and withheld; and the corruption is seen to bite, a plain location no scope holds then
    /// saying it may be in folder:Music, before the refusals are compared.
    #[test]
    fn typed_refusals_keep_their_details_while_a_folder_declaration_does_not_read() {
        let world = World::new();
        world
            .register("Music", &world.music(), json!(["wav", "aiff", "mp3"]))
            .unwrap();
        let denied = world.base.join("Elsewhere");
        fs::create_dir_all(&denied).unwrap();
        fs::write(denied.join(".no-agent-retrieval"), b"").unwrap();
        fs::write(denied.join("x.txt"), b"x").unwrap();
        let open = world.base.join("Open");
        fs::create_dir_all(&open).unwrap();
        fs::write(open.join("y.txt"), b"y").unwrap();
        let plain = || {
            located_shape(execute(
                &world.root,
                "locate",
                &json!({"path": open.join("y.txt")}),
            ))
        };
        let inputs = [
            json!({"path": denied.join("x.txt")}),
            json!({"path": denied.join("x.txt"), "binding_only": true}),
        ];
        let located = || {
            inputs
                .clone()
                .map(|input| located_shape(execute(&world.root, "locate", &input)))
        };
        let before = located();
        for shape in &before {
            assert!(
                shape.starts_with(
                    "ERR PermissionDenied Location is excluded by current source treatment DETAILS "
                ) && shape.contains(r#""failure_stage":"source_admission""#)
                    && shape.contains(r#""material_state":"withheld""#),
                "{shape}"
            );
        }
        assert!(!plain().contains("folder:Music"), "{}", plain());
        fs::write(
            world.root.join(".central/scopes/Music/scope.json"),
            b"not json",
        )
        .unwrap();
        assert!(
            plain().contains("it may be in folder:Music"),
            "the corrupt declaration is read as corrupt: {}",
            plain()
        );
        assert_eq!(located(), before);
    }

    /// DLW build ruling 2026-11-09, central-47h item 1 (#334 review, finding 1): a folder that is unplugged but whose declaration
    /// reads names its root, so it claims no location outside that root. A location no scope holds answers as on a healthy ground
    /// while such a folder is away, and when a second folder's declaration does not read, only that folder's words are added. And
    /// the folder is reported while it is away (central-47i item 2, #336 review, finding 1): a location under its own root answers
    /// that it is in folder:Other, which is unreachable.
    #[test]
    fn an_unplugged_folder_with_a_root_claims_no_location_it_does_not_hold() {
        let world = World::new();
        world
            .register("Music", &world.music(), json!(["wav", "aiff", "mp3"]))
            .unwrap();
        let other = world.base.join("Other");
        fs::create_dir_all(&other).unwrap();
        fs::write(other.join("o.wav"), b"RIFF....WAVEfmt sound").unwrap();
        world
            .register("Other", other.to_str().unwrap(), json!(["wav"]))
            .unwrap();
        let open = world.base.join("Open");
        fs::create_dir_all(&open).unwrap();
        fs::write(open.join("y.txt"), b"y").unwrap();
        let input = json!({"path": open.join("y.txt")});
        let healthy = located_shape(execute(&world.root, "locate", &input));
        let away = world.base.join("Other away");
        fs::rename(&other, &away).unwrap();
        let unplugged = located_shape(execute(&world.root, "locate", &input));
        let own = located_shape(execute(
            &world.root,
            "locate",
            &json!({"path": other.join("o.wav")}),
        ));
        fs::write(
            world.root.join(".central/scopes/Music/scope.json"),
            b"not json",
        )
        .unwrap();
        let both = located_shape(execute(&world.root, "locate", &input));
        fs::rename(&away, &other).unwrap();
        assert!(
            healthy.starts_with("ERR NotFound Location is unregistered DETAILS"),
            "{healthy}"
        );
        assert_eq!(
            unplugged, healthy,
            "the unplugged folder's root does not hold the location"
        );
        assert!(
            own.contains("Location is in folder:Other, which is "),
            "a location under the away folder's root is reported in it: {own}"
        );
        assert!(
            both.contains("it may be in folder:Music") && !both.contains("folder:Other"),
            "only the folder that does not read is named: {both}"
        );
    }

    /// central-47e item 6 (review 6): the skip of a name that is not UTF-8 counts and names only what the walk would have listed:
    /// a folder it would enter and a media file. A note, a hidden file and a hidden folder of such a name are passed by, as under
    /// any name. APFS keeps only UTF-8 names, so the walk is handed these through its seam.
    #[test]
    fn the_not_utf8_skip_names_only_what_the_walk_would_list() {
        let world = World::new();
        for path in [
            "notes-e.txt",
            ".hidden-e.wav",
            ".Hidden-e/inside.wav",
            "Entered-e/inside.wav",
            "cafe.wav",
        ] {
            put(&world, path);
        }
        world
            .register("Music", &world.music(), json!(["wav", "aiff", "mp3"]))
            .unwrap();
        let root = fs::canonicalize(&world.music).unwrap();
        let renamed: Vec<(PathBuf, Vec<u8>)> = [
            ("notes-e.txt", &b"notes\xe9.txt"[..]),
            (".hidden-e.wav", &b".hidd\xe9n.wav"[..]),
            (".Hidden-e", &b".Hidd\xe9n"[..]),
            ("Entered-e", &b"Ent\xe9red"[..]),
            ("cafe.wav", &b"caf\xe9.wav"[..]),
        ]
        .iter()
        .map(|(at, bytes)| (root.join(at), bytes.to_vec()))
        .collect();
        let answer = with_seams(
            crate::file_map_catalog::FolderWalkSeams {
                not_utf8: renamed,
                ..Default::default()
            },
            || run(&world.root, "inspect", &json!({"project":"folder:Music"})),
        )
        .unwrap();
        assert_eq!(
            music_paths(&world, &answer),
            [
                "Field recordings/estuary low tide.wav",
                "market-hall.AIFF",
                "voice.mp3"
            ]
        );
        let walked = &answer["folders"][0];
        assert_eq!(
            (walked["skipped"].clone(), walked["skipped_named"].clone()),
            (
                json!(2),
                json!([
                    {"path": format!("{}/Ent\u{FFFD}red", world.music()), "why": "its name is not UTF-8"},
                    {"path": format!("{}/caf\u{FFFD}.wav", world.music()), "why": "its name is not UTF-8"}
                ])
            ),
            "{walked}"
        );
    }

    /// central-47e item 7 (review 7): a folder keeps no index whoever calls: adopt and record-adopt refuse it at their head, and
    /// bkmr's preparation refuses it before it makes anything. bkmr's version is asked in the scope's state, never wherever the
    /// host process started.
    #[test]
    fn a_folder_keeps_no_index_whoever_calls_and_bkmr_never_starts_in_it() {
        let world = World::new();
        world
            .register("Music", &world.music(), json!(["wav", "aiff", "mp3"]))
            .unwrap();
        let all = scopes(&world.root).unwrap();
        let folder = all.iter().find(|scope| scope.is_folder()).unwrap();
        let words = format!(
            "Music is a folder scope: Central reads {} where it stands and keeps no index or projection receipt for it",
            world.music()
        );
        let database = world.base.join("index.db");
        fs::write(&database, b"SQLite format 3\0").unwrap();
        let voice = source_horizon::source_ref("folder:Music", "voice.mp3");
        let revision = world.revision();
        assert_eq!(
            refused(crate::file_map_adoption::adopt(
                folder,
                &json!({"quiesced":true,"database":database,"expected_revision":revision})
            )),
            words
        );
        assert_eq!(
            refused(crate::file_map_adoption::record_adopt(
                folder,
                &json!({"source_ref":voice,"record_id":1,"record_revision":"x","expected_revision":revision})
            )),
            words
        );
        assert!(!folder.state.join(".central").exists());
        let none = crate::file_map_backend::BkmrSeams {
            binary: Some(world.base.join("no-bkmr")),
            ..Default::default()
        };
        let prepared = with_bkmr(none, || Backend::for_scope(folder).prepare());
        assert_eq!(
            prepared.unwrap_err().to_string(),
            format!(
                "bkmr is never run for a folder scope: Central reads {} where it stands",
                world.music()
            )
        );
        assert!(
            !folder.state.join(".central").exists(),
            "prepare made its area for a folder"
        );
        let pwd = world.base.join("pwd-bkmr");
        fs::write(&pwd, "#!/bin/sh\npwd -P\n").unwrap();
        fs::set_permissions(&pwd, fs::Permissions::from_mode(0o755)).unwrap();
        let version = with_bkmr(
            crate::file_map_backend::BkmrSeams {
                binary: Some(pwd),
                ..Default::default()
            },
            || Backend::for_scope(folder).version(),
        )
        .unwrap();
        assert_eq!(
            Path::new(version.trim()),
            folder.state.canonicalize().unwrap()
        );
    }

    /// central-47e item 8 (review 8): inspect's Action says what a folder that stops answering costs each later read.
    #[test]
    fn inspect_says_what_a_stalled_folder_costs() {
        let mut registry = ActionRegistry::default();
        register(&mut registry);
        let description = &registry
            .get("central.file-map.inspect")
            .unwrap()
            .description;
        assert!(
            description.contains("after a call into the folder that never returns (a stalled share or volume), every read of that folder costs a full time bound and lists none of it, until the call returns"),
            "{description}"
        );
        assert!(
            description.contains("A read that waits for another walk of the folder to end can cost up to two time bounds: the wait, then its own walk."),
            "{description}"
        );
        assert!(!registry
            .get("central.file-map.search")
            .unwrap()
            .description
            .contains("time bound"));
    }
}
