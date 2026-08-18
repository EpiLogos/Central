use serde::Serialize;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

// Root initialisation establishes the same human-source / Agent-space relation that
// ProjectCentral repeats per Project. The root Wiki itself is created separately at
// Control/agents/wiki/wiki.json by ensure_root_federation; no human document is implied.
pub const REQUIRED_DIRECTORIES: [&str; 6] = [
    "Control/user",
    "Control/agents/governance",
    "Control/agents/wiki",
    "Control/machines",
    ".central",
    "Work",
];

#[derive(Debug, Clone, Default)]
pub struct RootOptions {
    pub explicit_root: Option<PathBuf>,
    pub configured_root: Option<PathBuf>,
    pub home: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RootSource {
    Explicit,
    Environment,
    Default,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ResolvedRoot {
    pub path: PathBuf,
    pub source: RootSource,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DirectoryCheck {
    pub path: String,
    pub valid: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CentralHealth {
    pub root: PathBuf,
    pub root_state: String,
    pub valid: bool,
    pub checks: Vec<DirectoryCheck>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CentralInitialization {
    pub root: PathBuf,
    pub directories: Vec<String>,
}

pub fn resolve_central_root(options: &RootOptions) -> Result<ResolvedRoot, String> {
    if let Some(path) = options.explicit_root.as_ref().filter(|path| !path.as_os_str().is_empty()) {
        return Ok(ResolvedRoot { path: path.clone(), source: RootSource::Explicit });
    }
    if let Some(path) = options.configured_root.as_ref().filter(|path| !path.as_os_str().is_empty()) {
        return Ok(ResolvedRoot { path: path.clone(), source: RootSource::Environment });
    }
    if let Some(home) = options.home.as_ref().filter(|path| !path.as_os_str().is_empty()) {
        return Ok(ResolvedRoot { path: home.join("Central"), source: RootSource::Default });
    }
    Err("Central root cannot be resolved because no home directory is available.".to_owned())
}

pub fn inspect_central(root: &Path) -> io::Result<CentralHealth> {
    let root_state = match fs::metadata(root) {
        Ok(metadata) if metadata.is_dir() => "directory",
        Ok(_) => "not_directory",
        Err(error) if error.kind() == io::ErrorKind::NotFound => "missing",
        Err(error) => return Err(error),
    };

    let checks = REQUIRED_DIRECTORIES
        .iter()
        .map(|relative| DirectoryCheck {
            path: (*relative).to_owned(),
            valid: fs::metadata(root.join(relative)).map(|metadata| metadata.is_dir()).unwrap_or(false),
        })
        .collect::<Vec<_>>();
    let valid = root_state == "directory" && checks.iter().all(|check| check.valid);

    Ok(CentralHealth {
        root: root.to_path_buf(),
        root_state: root_state.to_owned(),
        valid,
        checks,
    })
}

pub fn initialize_central(root: &Path) -> io::Result<CentralInitialization> {
    if let Ok(metadata) = fs::metadata(root) {
        if !metadata.is_dir() {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "Central root exists but is not a directory."));
        }
    }
    fs::create_dir_all(root)?;
    for relative in REQUIRED_DIRECTORIES {
        fs::create_dir_all(root.join(relative))?;
    }
    crate::projectcentral_ops::ensure_root_federation(root, None)?;
    Ok(CentralInitialization {
        root: root.to_path_buf(),
        directories: REQUIRED_DIRECTORIES.iter().map(|item| (*item).to_owned()).collect(),
    })
}
