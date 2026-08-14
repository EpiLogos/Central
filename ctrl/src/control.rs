use std::{fs, path::{Path, PathBuf}};

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::result::{ActionResult, FailureCode, ResultStatus};

const SUPPORTED_TEXT_EXTENSIONS: [&str; 7] = ["md", "markdown", "txt", "json", "yaml", "yml", "toml"];

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ControlRoot {
    User,
    Agents,
    Machines,
}

impl ControlRoot {
    pub const ALL: [Self; 3] = [Self::User, Self::Agents, Self::Machines];

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "user" => Some(Self::User),
            "agents" => Some(Self::Agents),
            "machines" => Some(Self::Machines),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Agents => "agents",
            Self::Machines => "machines",
        }
    }

    pub fn path(self, central_root: &Path) -> PathBuf {
        central_root.join("Control").join(self.as_str())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ControlOpenReport {
    pub root: ControlRoot,
    pub path: String,
    pub source_class: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ControlMatch {
    pub root: ControlRoot,
    pub path: String,
    pub line: usize,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkippedSource {
    pub root: ControlRoot,
    pub path: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ControlSearchReport {
    pub query: String,
    pub roots: Vec<ControlRoot>,
    pub matches: Vec<ControlMatch>,
    pub skipped: Vec<SkippedSource>,
    pub supported_extensions: Vec<String>,
}

pub fn open(central_root: &Path, target: &str) -> ActionResult {
    let Some(root) = ControlRoot::parse(target) else {
        return ActionResult::failure(
            "control.open",
            ResultStatus::InvalidInput,
            FailureCode::InvalidInput,
            "Control root must be one of: user, agents, machines",
        );
    };

    let path = root.path(central_root);
    match fs::metadata(&path) {
        Ok(metadata) if metadata.is_dir() => ActionResult::success(
            "control.open",
            serde_json::to_value(ControlOpenReport {
                root,
                path: path.display().to_string(),
                source_class: "authored".into(),
            })
            .expect("ControlOpenReport serializes"),
        ),
        Ok(_) => ActionResult::failure_with_data(
            "control.open",
            ResultStatus::InvalidCentralStructure,
            FailureCode::InvalidCentralStructure,
            "Control source root is not a directory",
            json!({ "root": root, "path": path.display().to_string() }),
        ),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            ActionResult::failure_with_data(
                "control.open",
                ResultStatus::InvalidCentralStructure,
                FailureCode::InvalidCentralStructure,
                "Control source root is missing",
                json!({ "root": root, "path": path.display().to_string() }),
            )
        }
        Err(error) => ActionResult::failure(
            "control.open",
            ResultStatus::InternalFailure,
            FailureCode::InternalFailure,
            error.to_string(),
        ),
    }
}

pub fn search(central_root: &Path, query: &str) -> ActionResult {
    if query.trim().is_empty() {
        return ActionResult::failure(
            "control.search",
            ResultStatus::InvalidInput,
            FailureCode::InvalidInput,
            "search query must not be empty",
        );
    }

    let mut matches = Vec::new();
    let mut skipped = Vec::new();
    let mut missing = Vec::new();
    let query_folded = query.to_lowercase();

    for root in ControlRoot::ALL {
        let source_root = root.path(central_root);
        if !source_root.is_dir() {
            missing.push(json!({
                "root": root,
                "path": source_root.display().to_string(),
            }));
            continue;
        }

        if let Err(error) = search_directory(
            central_root,
            root,
            &source_root,
            &query_folded,
            &mut matches,
            &mut skipped,
        ) {
            return ActionResult::failure(
                "control.search",
                ResultStatus::InternalFailure,
                FailureCode::InternalFailure,
                error.to_string(),
            );
        }
    }

    if !missing.is_empty() {
        return ActionResult::failure_with_data(
            "control.search",
            ResultStatus::InvalidCentralStructure,
            FailureCode::InvalidCentralStructure,
            "one or more Control source roots are missing",
            json!({ "missing_roots": missing }),
        );
    }

    matches.sort_by(|left, right| {
        left.path
            .cmp(&right.path)
            .then_with(|| left.line.cmp(&right.line))
    });
    skipped.sort_by(|left, right| left.path.cmp(&right.path));

    ActionResult::success(
        "control.search",
        serde_json::to_value(ControlSearchReport {
            query: query.into(),
            roots: ControlRoot::ALL.to_vec(),
            matches,
            skipped,
            supported_extensions: SUPPORTED_TEXT_EXTENSIONS
                .iter()
                .map(|extension| (*extension).into())
                .collect(),
        })
        .expect("ControlSearchReport serializes"),
    )
}

fn search_directory(
    central_root: &Path,
    root: ControlRoot,
    directory: &Path,
    query_folded: &str,
    matches: &mut Vec<ControlMatch>,
    skipped: &mut Vec<SkippedSource>,
) -> Result<(), std::io::Error> {
    let mut entries = fs::read_dir(directory)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.file_name());

    for entry in entries {
        let file_type = entry.file_type()?;
        let path = entry.path();
        if file_type.is_symlink() {
            skipped.push(skipped_source(central_root, root, &path, "symlink_not_followed"));
            continue;
        }
        if file_type.is_dir() {
            search_directory(central_root, root, &path, query_folded, matches, skipped)?;
            continue;
        }
        if !file_type.is_file() {
            continue;
        }

        if !is_supported_text_source(&path) {
            skipped.push(skipped_source(central_root, root, &path, "unsupported_format"));
            continue;
        }

        let source = match fs::read_to_string(&path) {
            Ok(source) => source,
            Err(error) if error.kind() == std::io::ErrorKind::InvalidData => {
                skipped.push(skipped_source(central_root, root, &path, "not_utf8"));
                continue;
            }
            Err(error) => return Err(error),
        };

        for (index, line) in source.lines().enumerate() {
            if line.to_lowercase().contains(query_folded) {
                matches.push(ControlMatch {
                    root,
                    path: relative_source_path(central_root, &path),
                    line: index + 1,
                    text: line.to_string(),
                });
            }
        }
    }

    Ok(())
}

fn is_supported_text_source(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| {
            SUPPORTED_TEXT_EXTENSIONS
                .iter()
                .any(|supported| extension.eq_ignore_ascii_case(supported))
        })
        .unwrap_or(false)
}

fn skipped_source(
    central_root: &Path,
    root: ControlRoot,
    path: &Path,
    reason: &str,
) -> SkippedSource {
    SkippedSource {
        root,
        path: relative_source_path(central_root, path),
        reason: reason.into(),
    }
}

fn relative_source_path(central_root: &Path, path: &Path) -> String {
    path.strip_prefix(central_root)
        .unwrap_or(path)
        .display()
        .to_string()
}
