use serde::Serialize;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub const CONTROL_ROOTS: [&str; 3] = ["user", "agents", "machines"];
pub const AGENT_RETRIEVAL_DENY_MARKER: &str = ".no-agent-retrieval";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceClass {
    Authored,
    Mixed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ControlSourceRoot {
    pub target: String,
    pub path: PathBuf,
    pub source_class: SourceClass,
    pub exists: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ControlSearchMatch {
    pub target: String,
    pub source_path: PathBuf,
    pub line: usize,
    pub text: String,
    pub source_class: SourceClass,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ControlSkippedSource {
    pub target: String,
    pub source_path: PathBuf,
    pub source_class: SourceClass,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ControlSearchResult {
    pub query: String,
    pub roots: Vec<ControlSourceRoot>,
    pub files_scanned: usize,
    pub skipped_sources: Vec<ControlSkippedSource>,
    pub matches: Vec<ControlSearchMatch>,
}

pub fn locate_control_root(central_root: &Path, target: &str) -> Result<ControlSourceRoot, String> {
    if !CONTROL_ROOTS.contains(&target) {
        return Err(format!(
            "Control root must be one of: {}.",
            CONTROL_ROOTS.join(", ")
        ));
    }
    let path = central_root.join("Control").join(target);
    let exists = fs::metadata(&path)
        .map(|metadata| metadata.is_dir())
        .unwrap_or(false);
    Ok(ControlSourceRoot {
        target: target.to_owned(),
        path,
        // Control/agents is intentionally a container of two authorities:
        // human-authored governance (plus preserved pre-split authored files) and
        // Agent-maintained Wiki knowledge. Human Control search below excludes wiki/.
        source_class: if target == "agents" {
            SourceClass::Mixed
        } else {
            SourceClass::Authored
        },
        exists,
    })
}

fn readable_files(
    central_root: &Path,
    target: &str,
    directory: &Path,
    files: &mut Vec<PathBuf>,
    skipped_sources: &mut Vec<ControlSkippedSource>,
) -> io::Result<()> {
    // `control.search` is the human-authored Control-source reader. Agent Wiki
    // knowledge has its own SemanticWiki path and must not be relabelled authored
    // merely because it is nested under Control/agents.
    if target == "agents" && directory == central_root.join("Control/agents/wiki") {
        return Ok(());
    }

    let deny_marker = directory.join(AGENT_RETRIEVAL_DENY_MARKER);
    if deny_marker.is_file() {
        skipped_sources.push(ControlSkippedSource {
            target: target.to_owned(),
            source_path: directory
                .strip_prefix(central_root)
                .unwrap_or(directory)
                .to_path_buf(),
            source_class: SourceClass::Authored,
            reason: "not_agent_readable".to_owned(),
        });
        return Ok(());
    }

    let mut entries = fs::read_dir(directory)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let path = entry.path();
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            readable_files(central_root, target, &path, files, skipped_sources)?;
        } else if file_type.is_file() && entry.file_name() != AGENT_RETRIEVAL_DENY_MARKER {
            files.push(path);
        }
    }
    Ok(())
}

pub fn search_control(central_root: &Path, query: &str) -> io::Result<ControlSearchResult> {
    let query = query.trim();
    if query.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Control search requires a non-empty query.",
        ));
    }

    let mut roots = Vec::new();
    for target in CONTROL_ROOTS {
        let root = locate_control_root(central_root, target)
            .map_err(|message| io::Error::new(io::ErrorKind::InvalidInput, message))?;
        if !root.exists {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("Control/{target} is missing."),
            ));
        }
        roots.push(root);
    }

    let needle = query.to_lowercase();
    let mut matches = Vec::new();
    let mut files_scanned = 0;
    let mut skipped_sources = Vec::new();

    for root in &roots {
        let mut files = Vec::new();
        readable_files(
            central_root,
            &root.target,
            &root.path,
            &mut files,
            &mut skipped_sources,
        )?;
        for path in files {
            let source_path = path
                .strip_prefix(central_root)
                .unwrap_or(&path)
                .to_path_buf();
            let bytes = fs::read(&path)?;
            let text = match String::from_utf8(bytes) {
                Ok(text) => text,
                Err(_) => {
                    skipped_sources.push(ControlSkippedSource {
                        target: root.target.clone(),
                        source_path,
                        source_class: SourceClass::Authored,
                        reason: "unsupported_non_text_source".to_owned(),
                    });
                    continue;
                }
            };
            files_scanned += 1;
            for (index, line) in text.lines().enumerate() {
                if line.to_lowercase().contains(&needle) {
                    matches.push(ControlSearchMatch {
                        target: root.target.clone(),
                        source_path: source_path.clone(),
                        line: index + 1,
                        text: line.to_owned(),
                        source_class: SourceClass::Authored,
                    });
                }
            }
        }
    }

    Ok(ControlSearchResult {
        query: query.to_owned(),
        roots,
        files_scanned,
        skipped_sources,
        matches,
    })
}

/// One governance statement in the session-start index: file name, topic and
/// standing. Standing is resolved from the source relations so a session can
/// tell adopted law from a generated suggestion awaiting adoption.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GovernanceIndexEntry {
    pub file: String,
    pub topic: String,
    pub standing: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GovernanceIndex {
    pub statements: Vec<GovernanceIndexEntry>,
    pub drafts: usize,
}

const GOVERNANCE_DIR: &str = "Control/agents/governance";
const SOURCE_RELATIONS: &str = "Control/relations/source-relations.json";

fn governance_standing(central_root: &Path, rel: &str) -> String {
    let raw = match fs::read_to_string(central_root.join(SOURCE_RELATIONS)) {
        Ok(raw) => raw,
        Err(_) => return "undeclared".to_owned(),
    };
    let value: serde_json::Value = match serde_json::from_str(&raw) {
        Ok(value) => value,
        Err(_) => return "undeclared".to_owned(),
    };
    let wanted = format!("central:source:control:root:{rel}");
    let mut stack = vec![&value];
    while let Some(node) = stack.pop() {
        match node {
            serde_json::Value::Object(map) => {
                if map.get("ref").and_then(|v| v.as_str()) == Some(wanted.as_str()) {
                    return match map.get("standing").and_then(|v| v.as_str()) {
                        Some("durable-source") => "durable".to_owned(),
                        Some("draft-source") => "draft".to_owned(),
                        _ => "undeclared".to_owned(),
                    };
                }
                for (_, child) in map {
                    stack.push(child);
                }
            }
            serde_json::Value::Array(items) => {
                stack.extend(items);
            }
            _ => {}
        }
    }
    "undeclared".to_owned()
}

/// The session-start flash of the governance field: one entry per statement
/// file — name, topic, standing — never the content. Derived compilations and
/// folder readers are skipped.
pub fn index_governance(central_root: &Path) -> io::Result<GovernanceIndex> {
    let governance = central_root.join(GOVERNANCE_DIR);
    if !governance.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("{GOVERNANCE_DIR}/ is missing from the Central root."),
        ));
    }
    let mut files = Vec::new();
    let mut stack = vec![governance.clone()];
    while let Some(directory) = stack.pop() {
        let mut entries = fs::read_dir(directory)?.collect::<Result<Vec<_>, _>>()?;
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|ext| ext == "md")
                && path.file_name().is_some_and(|name| {
                    name != "README.md" && name != "foundational-prompt.md"
                })
            {
                files.push(path);
            }
        }
    }
    files.sort();
    let mut statements = Vec::new();
    for path in files {
        let rel = path
            .strip_prefix(central_root)
            .unwrap_or(&path)
            .to_string_lossy()
            .to_string();
        let text = fs::read_to_string(&path).unwrap_or_default();
        let topic = text
            .lines()
            .find_map(|line| line.strip_prefix("# "))
            .unwrap_or_else(|| "")
            .trim()
            .to_owned();
        statements.push(GovernanceIndexEntry {
            file: rel.strip_prefix(&format!("{GOVERNANCE_DIR}/")).unwrap_or(&rel).to_owned(),
            topic: if topic.is_empty() { path.file_stem().unwrap_or_default().to_string_lossy().to_string() } else { topic },
            standing: governance_standing(central_root, &rel),
        });
    }
    let drafts = statements
        .iter()
        .filter(|entry| entry.standing == "draft")
        .count();
    Ok(GovernanceIndex { statements, drafts })
}
