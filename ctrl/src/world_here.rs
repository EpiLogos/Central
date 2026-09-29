//! `central.world.here` — where a working directory stands in the Central
//! World: the Local World (`control:root`), the Project World a path belongs
//! to, the Workcells this machine declares and whether the World's authored
//! record agrees with its identity.
//!
//! Read-only over native state. Every facet reports a state
//! (`present | absent | ambiguous | unavailable`) with its reason and the
//! source that answered; absence and ambiguity are results, never errors, and
//! nothing is inferred from terminal labels or recent UI state. No lock is
//! taken and nothing is written, so it is safe against a live personal root.

use crate::action::{
    ActionAvailability, ActionDescriptor, ActionExecutionContext, ActionInputDefinition,
    ActionOutputDefinition, ActionRegistry, MutationClass,
};
use crate::agent_set_store::RelationRecordStore;
use crate::continuous_work::{placement, source::Scope, temporal};
use crate::machine::{MachineDeclaration, WORKCELL_BINDING_KIND};
use crate::pasu::{PasuIdentityManifest, PASU_IDENTITY_MANIFEST_PATH};
use crate::result::{ActionResult, ResultStatus};
use crate::root::resolve_central_root;
use crate::source_horizon::CONTROL_WORLD_REF;
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub const WORLD_HERE_SCHEMA: &str = "central.world-here/v1";
pub const WORLD_HERE_ACTION: &str = "central.world.here";
const MACHINES_DIR: &str = "Control/machines";

/// How a path resolved to (at most) one Work member.
enum Resolution {
    Member {
        name: String,
        via: &'static str,
        worktree: Option<Value>,
    },
    Ambiguous {
        candidates: Vec<Value>,
        reason: String,
    },
    Absent {
        reason: String,
        source: String,
    },
    Unavailable {
        reason: String,
        source: String,
    },
}

/// Resolve where `cwd` (absolute) stands in the World rooted at
/// `central_root`. `project`, when given, names the Work member directly and
/// wins over `cwd`, which is still reported.
pub fn world_here(
    central_root: &Path,
    cwd: &Path,
    project: Option<&str>,
    now: u64,
) -> io::Result<Value> {
    let root = fs::canonicalize(central_root)?;
    if !root.join("Control").is_dir() || !root.join("Work").is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!(
                "{} is not a Central root (Control/ and Work/ are required)",
                root.display()
            ),
        ));
    }
    let (cwd_reading, from_cwd) = resolve_cwd(&root, cwd, now);
    let resolution = match project {
        Some(name) => Resolution::Member {
            name: name.to_owned(),
            via: "input",
            worktree: None,
        },
        None => from_cwd,
    };
    let (project_world, project_scope) = project_world(&root, resolution);
    let (workcells, workcells_unreadable) = workcells(&root);
    Ok(json!({
        "schema": WORLD_HERE_SCHEMA,
        "local_world": local_world(&root, now),
        "project_world": project_world,
        "cwd": cwd_reading,
        "workcells": workcells,
        "workcells_unreadable": workcells_unreadable,
        "world_record": world_record(&root, project_scope.as_ref()),
        "automatic_agent_or_model_invocation": false,
    }))
}

fn display(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn local_world(root: &Path, now: u64) -> Value {
    let (identity_ref, identity) = match PasuIdentityManifest::load(root) {
        Ok(manifest) => (
            Some(manifest.subject.ref_.as_str().to_owned()),
            json!({
                "state": "present",
                "source": PASU_IDENTITY_MANIFEST_PATH,
                "revision": manifest.revision,
            }),
        ),
        Err(error) => (
            None,
            json!({
                "state": "absent",
                "source": PASU_IDENTITY_MANIFEST_PATH,
                "reason": error.to_string(),
            }),
        ),
    };
    let time_policy =
        match Scope::resolve(root, None).and_then(|scope| temporal::time_policy(&scope, now)) {
            Ok(reading) => json!({
                "state": "present",
                "ref": reading.source_ref,
                "revision": reading.revision,
                "source_revision": reading.source_revision,
                "timezone": reading.policy.timezone,
                "civil_date": reading.civil_date,
            }),
            Err(error) => json!({
                "state": "unavailable",
                "source": "central.time.policy",
                "reason": error.to_string(),
            }),
        };
    let (roots, missing) = roots(root, "Control", "Control/agents/now");
    json!({
        "ref": CONTROL_WORLD_REF,
        "root": display(root),
        "identity_ref": identity_ref,
        "identity": identity,
        "roots": roots,
        "missing_roots": missing,
        "time_policy": time_policy,
    })
}

/// The register roots of one scope, relative to the Central root. Only paths
/// that exist are reported; a missing root is `null` and named in the second
/// value.
fn roots(root: &Path, base: &str, now: &str) -> (Value, Vec<String>) {
    let mut reported = Map::new();
    let mut missing = Vec::new();
    for (name, path) in [
        ("governance", format!("{base}/agents/governance")),
        ("wiki", format!("{base}/agents/wiki/wiki.json")),
        ("now", now.to_owned()),
        ("now_clearings", format!("{base}/agents/now/clearings")),
        ("user", format!("{base}/user")),
    ] {
        if root.join(&path).exists() {
            reported.insert(name.into(), json!(path));
        } else {
            reported.insert(name.into(), Value::Null);
            missing.push(name.to_owned());
        }
    }
    (Value::Object(reported), missing)
}

fn resolve_cwd(root: &Path, cwd: &Path, now: u64) -> (Value, Resolution) {
    let canonical = match fs::canonicalize(cwd) {
        Ok(path) => path,
        Err(error) => {
            let reason = format!("cwd {} cannot be resolved: {error}", cwd.display());
            return (
                json!({"path": display(cwd), "relation": "outside", "reason": reason}),
                Resolution::Unavailable {
                    reason,
                    source: "cwd".into(),
                },
            );
        }
    };
    let Ok(relative) = canonical.strip_prefix(root) else {
        let reason = format!(
            "cwd is outside the Central root {}; it belongs to no Project World",
            root.display()
        );
        return (
            json!({"path": display(&canonical), "relation": "outside"}),
            Resolution::Absent {
                reason,
                source: "cwd".into(),
            },
        );
    };
    let parts: Vec<&str> = relative
        .components()
        .filter_map(|part| match part {
            Component::Normal(value) => value.to_str(),
            _ => None,
        })
        .collect();
    if parts.len() >= 2 && parts[0] == "Work" {
        return (
            json!({"path": display(&canonical), "relation": "work-member"}),
            Resolution::Member {
                name: parts[1].to_owned(),
                via: "cwd",
                worktree: None,
            },
        );
    }
    let central_root = json!({"path": display(&canonical), "relation": "central-root"});
    let claims = match placement::worktree_claims(root, &canonical, now) {
        Ok(Some(claims)) => claims,
        Ok(None) => {
            return (
                central_root,
                Resolution::Absent {
                    reason: "cwd is inside the Central root but not inside a Work member, and no recognised root placement policy registers worktrees".into(),
                    source: "Control/user placement policy".into(),
                },
            )
        }
        Err(error) => {
            return (
                central_root,
                Resolution::Unavailable {
                    reason: format!(
                        "cwd is inside the Central root but the root placement policy cannot be read to resolve registered worktrees: {error}"
                    ),
                    source: "central.work.policy".into(),
                },
            )
        }
    };
    let members: BTreeSet<&str> = claims
        .iter()
        .filter_map(|claim| claim.member.as_deref())
        .collect();
    match members.len() {
        0 if claims.is_empty() => {
            let reason = if parts.is_empty() {
                "cwd is the Central root itself, not inside a Work member or a registered worktree".to_owned()
            } else {
                format!(
                    "cwd {} is inside the Central root but not inside a Work member or a registered worktree",
                    relative.display()
                )
            };
            (
                central_root,
                Resolution::Absent {
                    reason,
                    source: "cwd".into(),
                },
            )
        }
        0 => (
            central_root,
            Resolution::Absent {
                reason: format!(
                    "cwd is under worktree grant(s) whose registration is not proven: {}",
                    claims
                        .iter()
                        .map(|claim| format!(
                            "{}: {}",
                            claim.grant,
                            claim.reason.as_deref().unwrap_or("unproven")
                        ))
                        .collect::<Vec<_>>()
                        .join("; ")
                ),
                source: "Control/user placement policy".into(),
            },
        ),
        1 => {
            let claim = claims
                .iter()
                .find(|claim| claim.member.is_some())
                .expect("one proven member");
            (
                json!({"path": display(&canonical), "relation": "registered-worktree"}),
                Resolution::Member {
                    name: claim.member.clone().expect("proven member"),
                    via: "registered-worktree",
                    worktree: Some(json!({"grant": claim.grant})),
                },
            )
        }
        _ => (
            json!({"path": display(&canonical), "relation": "registered-worktree"}),
            Resolution::Ambiguous {
                candidates: claims
                    .iter()
                    .map(|claim| {
                        json!({"member": claim.member, "grant": claim.grant, "reason": claim.reason})
                    })
                    .collect(),
                reason: format!(
                    "{} Work members claim this path through registered worktree grants; Central never chooses between them",
                    members.len()
                ),
            },
        ),
    }
}

fn project_world(root: &Path, resolution: Resolution) -> (Value, Option<Scope>) {
    let (name, via, worktree) = match resolution {
        Resolution::Member {
            name,
            via,
            worktree,
        } => (name, via, worktree),
        Resolution::Ambiguous { candidates, reason } => {
            return (
                json!({"state": "ambiguous", "candidates": candidates, "reason": reason, "source": "Control/user placement policy"}),
                None,
            )
        }
        Resolution::Absent { reason, source } => {
            return (
                json!({"state": "absent", "reason": reason, "source": source}),
                None,
            )
        }
        Resolution::Unavailable { reason, source } => {
            return (
                json!({"state": "unavailable", "reason": reason, "source": source}),
                None,
            )
        }
    };
    let path = format!("Work/{name}");
    let manifest = format!("{path}/ProjectCentral/project.json");
    let mut facet = json!({"name": name, "path": path, "via": via, "source": manifest});
    if let Some(worktree) = worktree {
        facet["worktree"] = worktree;
    }
    let absent = |mut facet: Value, reason: String| {
        facet["state"] = json!("absent");
        facet["reason"] = json!(reason);
        (facet, None)
    };
    if !root.join(&path).is_dir() {
        return absent(facet, format!("{path} does not exist"));
    }
    if !root.join(&manifest).is_file() {
        return absent(
            facet,
            format!("{path} has no ProjectCentral ({manifest} is absent); it is a Work member without a Project World"),
        );
    }
    let scope = match Scope::resolve(root, Some(&name)) {
        Ok(scope) => scope,
        Err(error) => {
            facet["state"] = json!("unavailable");
            facet["reason"] = json!(format!("ProjectCentral cannot be read: {error}"));
            return (facet, None);
        }
    };
    let (roots, missing) = roots(
        root,
        &format!("{path}/ProjectCentral"),
        &format!("{path}/{}", crate::projectcentral_now::NOW_DIR),
    );
    facet["state"] = json!("present");
    facet["ref"] = json!(scope.world_ref);
    facet["projectcentral"] = json!(format!("{path}/ProjectCentral"));
    facet["parent_ref"] = json!(CONTROL_WORLD_REF);
    facet["roots"] = roots;
    facet["missing_roots"] = json!(missing);
    (facet, Some(scope))
}

/// Workcell bindings declared by this machine's authored role declarations
/// (`Control/machines/*.json`), each with the state of its Workcell root NOW.
fn workcells(root: &Path) -> (Vec<Value>, Vec<Value>) {
    let mut declared = Vec::new();
    let mut unreadable = Vec::new();
    let mut files: Vec<PathBuf> = match fs::read_dir(root.join(MACHINES_DIR)) {
        Ok(entries) => entries
            .filter_map(|entry| entry.ok().map(|entry| entry.path()))
            .filter(|path| {
                path.extension().and_then(|value| value.to_str()) == Some("json")
                    && fs::symlink_metadata(path).is_ok_and(|meta| meta.is_file())
            })
            .collect(),
        Err(_) => Vec::new(),
    };
    files.sort();
    for file in files {
        let name = file
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_owned();
        let source = format!("{MACHINES_DIR}/{name}");
        let declaration = fs::read(&file)
            .map_err(|error| error.to_string())
            .and_then(|bytes| {
                serde_json::from_slice::<MachineDeclaration>(&bytes)
                    .map_err(|error| error.to_string())
            });
        match declaration {
            Ok(declaration) => {
                for binding in declaration
                    .bindings
                    .iter()
                    .filter(|binding| binding.kind == WORKCELL_BINDING_KIND)
                {
                    declared.push(json!({
                        "ref": binding.reference,
                        "declared_by": source,
                        "role": declaration.role,
                    }));
                }
            }
            Err(error) => unreadable.push(json!({"path": source, "error": error})),
        }
    }
    if !declared.is_empty() {
        let root_nows = root_nows(root);
        for workcell in &mut declared {
            let reference = workcell["ref"].as_str().unwrap_or_default().to_owned();
            workcell["root_now"] = match &root_nows {
                Ok(states) => {
                    let now_ref = placement::now_ref_for(
                        CONTROL_WORLD_REF,
                        &placement::workcell_root_task_ref(&reference),
                    );
                    match states.get(&now_ref) {
                        Some(lifecycle) => {
                            json!({"state": "present", "now_ref": now_ref, "lifecycle": lifecycle})
                        }
                        None => json!({
                            "state": "absent",
                            "now_ref": now_ref,
                            "reason": "no Workcell root NOW is allocated; central.now.workcell-root ensures it",
                        }),
                    }
                }
                Err(reason) => json!({"state": "unavailable", "reason": reason}),
            };
        }
    }
    (declared, unreadable)
}

fn root_nows(root: &Path) -> Result<BTreeMap<String, String>, String> {
    let scope = Scope::resolve(root, None).map_err(|error| error.to_string())?;
    Ok(placement::scan_now(&scope)
        .map_err(|error| format!("root NOW clearings cannot be read: {error}"))?
        .into_iter()
        .map(|(record, _)| (record.now_ref, record.lifecycle))
        .collect())
}

/// Whether the World's authored record (`central.world-relations/v1`) agrees
/// with the identity the scope derives: the Project manifest's `project_id`,
/// or `control:root` for the Local World.
fn world_record(root: &Path, project: Option<&Scope>) -> Value {
    let (store, expected) = match project {
        Some(scope) => (
            RelationRecordStore::worlds_in_project(&scope.root),
            scope.world_ref.clone(),
        ),
        None => (
            RelationRecordStore::worlds_at_root(root),
            CONTROL_WORLD_REF.to_owned(),
        ),
    };
    let dir = store.source_dir();
    let relative = |path: &Path| {
        path.strip_prefix(root)
            .map(display)
            .unwrap_or_else(|_| display(path))
    };
    let source = relative(&dir);
    let entries = match fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return json!({
                "state": "absent", "expected_ref": expected, "source": source,
                "detail": format!("no world record is authored at {source}; the World inherits control:root by convention"),
            })
        }
        Err(error) => {
            return json!({"state": "unavailable", "expected_ref": expected, "source": source, "detail": error.to_string()})
        }
    };
    let mut files: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("json"))
        .collect();
    files.sort();
    let mut declared = Vec::new();
    let mut unreadable = Vec::new();
    for file in &files {
        let parsed = fs::read(file)
            .map_err(|error| error.to_string())
            .and_then(|bytes| {
                serde_json::from_slice::<Value>(&bytes).map_err(|error| error.to_string())
            });
        match parsed {
            Ok(record) => match record.get("ref").and_then(Value::as_str) {
                Some(reference) => declared.push((file.clone(), reference.to_owned(), record)),
                None => unreadable.push(format!("{}: carries no ref", relative(file))),
            },
            Err(error) => unreadable.push(format!("{}: {error}", relative(file))),
        }
    }
    let matching: Vec<_> = declared
        .iter()
        .filter(|(_, reference, _)| reference == &expected)
        .collect();
    match matching.as_slice() {
        [(file, reference, record)] => {
            let expected_path = store.source_path(reference).ok();
            if expected_path.as_deref() != Some(file.as_path()) {
                return json!({
                    "state": "mismatch", "ref": reference, "expected_ref": expected, "source": relative(file),
                    "detail": format!(
                        "the record for {reference} is stored as {} but its ref derives {}",
                        relative(file),
                        expected_path.as_deref().map(relative).unwrap_or_default()
                    ),
                });
            }
            json!({
                "state": "present", "ref": reference, "expected_ref": expected, "source": relative(file),
                "revision": record.get("revision"), "parent": record.get("parent"),
                "detail": "the authored world record matches the identity this scope derives",
            })
        }
        [] if declared.is_empty() && unreadable.is_empty() => json!({
            "state": "absent", "expected_ref": expected, "source": source,
            "detail": format!("no world record is authored at {source}; the World inherits control:root by convention"),
        }),
        [] if declared.is_empty() => json!({
            "state": "unavailable", "expected_ref": expected, "source": source,
            "detail": format!("world records cannot be read: {}", unreadable.join("; ")),
        }),
        [] => {
            let refs: Vec<&str> = declared
                .iter()
                .map(|(_, reference, _)| reference.as_str())
                .collect();
            json!({
                "state": "mismatch", "ref": refs[0], "declared_refs": refs, "expected_ref": expected, "source": source,
                "detail": format!(
                    "{source} declares {} but this World's identity is {expected}",
                    refs.join(", ")
                ),
            })
        }
        many => json!({
            "state": "mismatch", "ref": expected, "expected_ref": expected, "source": source,
            "detail": format!("{} world records declare {expected}; exactly one is required", many.len()),
        }),
    }
}

fn valid_member_name(raw: &str) -> bool {
    let path = Path::new(raw);
    !raw.trim().is_empty()
        && raw == raw.trim()
        && !raw.starts_with('.')
        && path.components().count() == 1
        && matches!(path.components().next(), Some(Component::Normal(_)))
}

fn here_action(
    _: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let invalid = |message: &str| {
        ActionResult::failure(
            Some(WORLD_HERE_ACTION),
            ResultStatus::InvalidInput,
            message,
            None,
        )
    };
    let root = match resolve_central_root(context.root_options) {
        Ok(root) => root.path,
        Err(message) => return invalid(&message),
    };
    let process_cwd = std::env::current_dir();
    let cwd = match input.get("cwd") {
        None | Some(Value::Null) => match process_cwd {
            Ok(cwd) => cwd,
            Err(error) => {
                return invalid(&format!(
                    "the process working directory is unavailable ({error}); pass cwd"
                ))
            }
        },
        Some(Value::String(raw)) if !raw.trim().is_empty() => {
            let raw = PathBuf::from(raw);
            if raw.is_absolute() {
                raw
            } else {
                match process_cwd {
                    Ok(cwd) => cwd.join(raw),
                    Err(_) => return invalid(
                        "a relative cwd needs a process working directory; pass an absolute cwd",
                    ),
                }
            }
        }
        Some(_) => return invalid("cwd must be a non-empty path string"),
    };
    let project = match input.get("project") {
        None | Some(Value::Null) => None,
        Some(Value::String(raw)) if valid_member_name(raw) => Some(raw.as_str()),
        Some(_) => {
            return invalid("project must name one Central/Work member (a single path component)")
        }
    };
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or_default();
    match world_here(&root, &cwd, project, now) {
        Ok(reading) => ActionResult::success(WORLD_HERE_ACTION, reading),
        Err(error) => ActionResult::failure(
            Some(WORLD_HERE_ACTION),
            ResultStatus::InvalidCentralStructure,
            error.to_string(),
            None,
        ),
    }
}

fn optional_string(name: &str) -> ActionInputDefinition {
    ActionInputDefinition {
        name: name.into(),
        input_type: "string".into(),
        required: false,
        choices: None,
        selection: None,
    }
}

pub fn register_world_here_action(registry: &mut ActionRegistry) {
    registry
        .register(
            ActionDescriptor {
                id: WORLD_HERE_ACTION.into(),
                title: "Show where a path stands in the World".into(),
                description: "Resolve a working directory (default: the process cwd) or a named Work member to the Local World (control:root) and its Project World: register roots at both scopes, the civil-time policy ref and revision, the Workcells this machine declares with their root NOW state, and whether the World's authored record matches its identity. A registered development worktree resolves through its repository grant. Absence and ambiguity are reported as data. Read-only.".into(),
                inputs: vec![optional_string("cwd"), optional_string("project")],
                output: ActionOutputDefinition {
                    output_type: WORLD_HERE_SCHEMA.into(),
                },
                mutation_class: MutationClass::ReadOnly,
                preview_supported: false,
                required_ports: vec![],
                availability: ActionAvailability {
                    available: true,
                    reason: None,
                },
            },
            here_action,
        )
        .expect("central.world.here is registered once");
}
