//! `central.files.reach`: for a reader of Central's ground, what is withheld from retrieval and why. The admission that
//! `central.files.list` and `central.files.read` enforce (source_horizon.rs `retrieval_path_verdict`) already finds the reason
//! a place is refused — a `.no-agent-retrieval` marker on it or on a folder above it, a link on the way, a path outside the
//! ground — and `retrieval_allowed` reduced it to a bool. This read keeps the reason, for the places a caller names and for the
//! withheld places under one folder.
//!
//! Central's treatment is the world's: it is the same for every agent that reads the ground, and Central records no reach of one
//! agent's own (an agent's own Reach, set by the person, is the Software Factory's Scope write, WO-SF2). The reader a caller names
//! is carried back so the answer says whom it was asked for, and `same_for_every_agent` says so plainly.
use super::answer;
use crate::action::{
    ActionAvailability, ActionDescriptor, ActionExecutionContext, ActionInputDefinition,
    ActionOutputDefinition, ActionRegistry, MutationClass,
};
use crate::control::AGENT_RETRIEVAL_DENY_MARKER;
use crate::root::resolve_central_root;
use crate::source_horizon::{retrieval_withheld, RetrievalWithheld};
use serde_json::{json, Value};
use std::collections::VecDeque;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

pub const REACH_ACTION: &str = "central.files.reach";
pub const REACH_SCHEMA: &str = "central.files-reach/v1";
/// The most places one call judges.
const MAX_REACH_PATHS: usize = 256;
/// The most folders one walk under a folder opens, and the most withheld places it reports: past either, the answer says it is not
/// complete instead of presenting a part as the whole.
const MAX_REACH_FOLDERS: usize = 20_000;
const MAX_REACH_WITHHELD: usize = 256;

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message.into())
}

/// A path as a caller names it, placed in the ground: absolute under the root (as the root is configured or canonical), or
/// relative to it. `None` for an absolute path outside the ground.
fn ground_relative(root: &Path, configured: &Path, path: &str) -> Option<PathBuf> {
    let asked = Path::new(path);
    if !asked.is_absolute() {
        return Some(asked.components().collect());
    }
    asked
        .strip_prefix(root)
        .or_else(|_| asked.strip_prefix(configured))
        .ok()
        .map(Path::to_path_buf)
}

/// One named place, judged: reachable, or withheld with Central's reason.
fn judge(root: &Path, configured: &Path, path: &str) -> Value {
    let (relative, source) = match ground_relative(root, configured, path) {
        Some(relative) => (
            Some(relative.to_string_lossy().into_owned()),
            root.join(&relative),
        ),
        None => (None, PathBuf::from(path)),
    };
    // The root itself is named by an empty relative path; `retrieval_withheld` judges it by its own marker.
    let withheld = retrieval_withheld(root, &source);
    let mut place = json!({"path": path, "relative": relative, "reachable": withheld.is_none()});
    if let Some(withheld) = withheld {
        place["withheld"] = json!(withheld);
    }
    place
}

/// The withheld places under `folder` (relative to the root): every link or unordinary entry, and every folder that carries a
/// marker, without descending into what is withheld or into the tool-state folders no scan of Central's opens.
fn withheld_under(root: &Path, folder: &Path) -> (Vec<RetrievalWithheld>, bool) {
    let mut found = Vec::new();
    let mut queue = VecDeque::from([root.join(folder)]);
    let mut opened = 0usize;
    while let Some(directory) = queue.pop_front() {
        if opened == MAX_REACH_FOLDERS {
            return (found, false);
        }
        opened += 1;
        let Ok(entries) = fs::read_dir(&directory) else {
            if let Some(reason) = retrieval_withheld(root, &directory) {
                found.push(reason);
            }
            continue;
        };
        let mut entries: Vec<_> = entries.filter_map(Result::ok).collect();
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let path = entry.path();
            let Ok(metadata) = fs::symlink_metadata(&path) else {
                continue;
            };
            let name = entry.file_name();
            let kind = metadata.file_type();
            let suspect = if kind.is_symlink() || !(kind.is_dir() || kind.is_file()) {
                true
            } else if kind.is_dir() {
                if crate::source_horizon::scan_skips_dir(&name.to_string_lossy()) {
                    continue;
                }
                fs::symlink_metadata(path.join(AGENT_RETRIEVAL_DENY_MARKER)).is_ok()
            } else {
                false
            };
            if suspect {
                if let Some(reason) = retrieval_withheld(root, &path) {
                    if found.len() == MAX_REACH_WITHHELD {
                        return (found, false);
                    }
                    found.push(reason);
                    continue;
                }
            }
            if kind.is_dir() {
                queue.push_back(path);
            }
        }
    }
    (found, true)
}

fn reach(input: &Value, context: &ActionExecutionContext<'_>) -> io::Result<Value> {
    let configured = resolve_central_root(context.root_options)
        .map_err(io::Error::other)?
        .path;
    let root = configured.canonicalize()?;
    let paths: Vec<&str> = match input.get("paths") {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Array(paths)) if paths.len() <= MAX_REACH_PATHS => paths
            .iter()
            .map(|path| {
                path.as_str()
                    .ok_or_else(|| invalid("paths must be strings"))
            })
            .collect::<io::Result<_>>()?,
        Some(_) => {
            return Err(invalid(format!(
                "paths must be an array of at most {MAX_REACH_PATHS} paths"
            )))
        }
    };
    let under = match input.get("under") {
        None | Some(Value::Null) => None,
        Some(Value::String(folder)) => Some(folder.as_str()),
        Some(_) => return Err(invalid("under must be a path")),
    };
    if paths.is_empty() && under.is_none() {
        return Err(invalid(
            "central.files.reach needs paths to judge, a folder to look under, or both",
        ));
    }
    let reader = match input.get("agent_session_ref") {
        None | Some(Value::Null) => None,
        Some(Value::String(session)) if !session.trim().is_empty() => Some(session.clone()),
        Some(_) => return Err(invalid("agent_session_ref must be a non-empty string")),
    };
    let places: Vec<Value> = paths
        .iter()
        .map(|path| judge(&root, &configured, path))
        .collect();
    let under = match under {
        None => Value::Null,
        Some(folder) => {
            let relative = ground_relative(&root, &configured, folder).filter(|relative| {
                relative
                    .components()
                    .all(|part| matches!(part, Component::Normal(_)))
            });
            let held = relative.as_ref().map_or_else(
                || retrieval_withheld(&root, Path::new(folder)),
                |relative| retrieval_withheld(&root, &root.join(relative)),
            );
            match (relative, held) {
                // The folder itself is withheld: that is the answer, and nothing under it is opened.
                (_, Some(withheld)) => {
                    json!({"folder": folder, "withheld": [withheld], "complete": true})
                }
                (Some(relative), None) if root.join(&relative).is_dir() => {
                    let (withheld, complete) = withheld_under(&root, &relative);
                    json!({"folder": folder, "withheld": withheld, "complete": complete})
                }
                _ => {
                    return Err(invalid(format!(
                        "{folder} is no folder in Central's ground"
                    )))
                }
            }
        }
    };
    Ok(json!({
        "schema": REACH_SCHEMA,
        "root": root.to_string_lossy(),
        "reader": {"agent_session_ref": reader},
        "same_for_every_agent": true,
        "places": places,
        "under": under,
        "automatic_agent_or_model_invocation": false,
    }))
}

pub(super) fn register(registry: &mut ActionRegistry) {
    registry
        .register(
            ActionDescriptor {
                id: REACH_ACTION.into(),
                title: "Read what a reader may not reach, and why".into(),
                description: "For a reader of Central's ground, the places withheld from retrieval and the reason for each (central.files-reach/v1): the admission central.files.list and central.files.read enforce, its reason kept instead of reduced to retrieval_allowed. paths (up to 256, absolute under the root or relative to it) are each answered reachable, or withheld with a code — deny-marker (a .no-agent-retrieval on the place or a folder above it), redirected (a link or unordinary material on the way), outside (not in the ground), not-a-member (a climbing path, or a file standing as a folder), unobserved (Central could not look, and a failed look grants nothing) — the place that withholds it (at) and Central's words. under names a folder: every withheld place below it is listed, without descending into what is withheld or into the version-control, build and tool-state folders no scan of Central's opens (.git, node_modules, target …); past 20 000 folders or 256 places the answer says complete false. Central's treatment is the world's, the same for every agent (same_for_every_agent); agent_session_ref names the reader asked for and changes nothing. Read-only.".into(),
                inputs: vec![
                    ActionInputDefinition { name: "paths".into(), input_type: "array".into(), required: false, choices: None, selection: None },
                    ActionInputDefinition { name: "under".into(), input_type: "string".into(), required: false, choices: None, selection: None },
                    ActionInputDefinition { name: "agent_session_ref".into(), input_type: "string".into(), required: false, choices: None, selection: None },
                ],
                output: ActionOutputDefinition { output_type: "central-files-reach".into() },
                mutation_class: MutationClass::ReadOnly,
                preview_supported: false,
                required_ports: vec![],
                availability: ActionAvailability { available: true, reason: None },
            },
            |_, input, context| answer(REACH_ACTION, reach(input, context)),
        )
        .expect("Unique native filesystem actions");
}
