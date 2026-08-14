use serde::Serialize;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub const CONTROL_ROOTS: [&str; 3] = ["user", "agents", "machines"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceClass {
    Authored,
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
pub struct ControlSearchResult {
    pub query: String,
    pub roots: Vec<ControlSourceRoot>,
    pub files_scanned: usize,
    pub matches: Vec<ControlSearchMatch>,
}

pub fn locate_control_root(central_root: &Path, target: &str) -> Result<ControlSourceRoot, String> {
    if !CONTROL_ROOTS.contains(&target) {
        return Err(format!("Control root must be one of: {}.", CONTROL_ROOTS.join(", ")));
    }
    let path = central_root.join("Control").join(target);
    let exists = fs::metadata(&path).map(|metadata| metadata.is_dir()).unwrap_or(false);
    Ok(ControlSourceRoot {
        target: target.to_owned(),
        path,
        source_class: SourceClass::Authored,
        exists,
    })
}

fn readable_files(directory: &Path, files: &mut Vec<PathBuf>) -> io::Result<()> {
    let mut entries = fs::read_dir(directory)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            readable_files(&entry.path(), files)?;
        } else if file_type.is_file() {
            files.push(entry.path());
        }
    }
    Ok(())
}

pub fn search_control(central_root: &Path, query: &str) -> io::Result<ControlSearchResult> {
    let query = query.trim();
    if query.is_empty() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "Control search requires a non-empty query."));
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

    for root in &roots {
        let mut files = Vec::new();
        readable_files(&root.path, &mut files)?;
        for path in files {
            let text = match fs::read_to_string(&path) {
                Ok(text) => text,
                Err(error) if error.kind() == io::ErrorKind::InvalidData => continue,
                Err(error) => return Err(error),
            };
            files_scanned += 1;
            let source_path = path.strip_prefix(central_root).unwrap_or(&path).to_path_buf();
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
        matches,
    })
}
