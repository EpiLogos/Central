use std::{
    fs, io,
    path::{Path, PathBuf},
};

use serde::Serialize;

pub const REQUIRED_DIRS: [&str; 4] = ["Control/user", "Control/agents", "Control/machines", "Work"];

#[derive(Debug, Clone, Default)]
pub struct RootContext {
    pub explicit_root: Option<PathBuf>,
    pub configured_root: Option<PathBuf>,
    pub home: Option<PathBuf>,
}

impl RootContext {
    pub fn from_process(explicit_root: Option<PathBuf>) -> Self {
        Self {
            explicit_root,
            configured_root: std::env::var_os("CENTRAL_ROOT").map(PathBuf::from),
            home: std::env::var_os("HOME").map(PathBuf::from),
        }
    }
}

#[derive(Debug)]
pub enum RootError {
    InvalidInput(String),
    Io(io::Error),
}

impl From<io::Error> for RootError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct InitReport {
    pub root: String,
    pub ensured: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DoctorReport {
    pub root: String,
    pub valid: bool,
    pub missing: Vec<String>,
    pub invalid: Vec<String>,
}

pub fn resolve_root(context: &RootContext) -> Result<PathBuf, RootError> {
    if let Some(root) = context.explicit_root.as_ref() {
        return validate_root_value(root);
    }
    if let Some(root) = context.configured_root.as_ref() {
        return validate_root_value(root);
    }
    if let Some(home) = context.home.as_ref() {
        return validate_root_value(&home.join("Central"));
    }
    Err(RootError::InvalidInput(
        "cannot resolve Central root: provide --root, CENTRAL_ROOT, or HOME".into(),
    ))
}

fn validate_root_value(root: &Path) -> Result<PathBuf, RootError> {
    if root.as_os_str().is_empty() {
        return Err(RootError::InvalidInput(
            "Central root cannot be empty".into(),
        ));
    }
    Ok(root.to_path_buf())
}

pub fn initialize(root: &Path) -> Result<InitReport, io::Error> {
    fs::create_dir_all(root)?;
    for relative in REQUIRED_DIRS {
        fs::create_dir_all(root.join(relative))?;
    }
    Ok(InitReport {
        root: root.display().to_string(),
        ensured: REQUIRED_DIRS
            .iter()
            .map(|value| (*value).to_string())
            .collect(),
    })
}

pub fn doctor(root: &Path) -> Result<DoctorReport, io::Error> {
    let mut missing = Vec::new();
    let mut invalid = Vec::new();

    inspect_directory(root, ".", &mut missing, &mut invalid)?;
    for relative in REQUIRED_DIRS {
        inspect_directory(&root.join(relative), relative, &mut missing, &mut invalid)?;
    }

    Ok(DoctorReport {
        root: root.display().to_string(),
        valid: missing.is_empty() && invalid.is_empty(),
        missing,
        invalid,
    })
}

fn inspect_directory(
    path: &Path,
    label: &str,
    missing: &mut Vec<String>,
    invalid: &mut Vec<String>,
) -> Result<(), io::Error> {
    match fs::metadata(path) {
        Ok(metadata) if metadata.is_dir() => Ok(()),
        Ok(_) => {
            invalid.push(label.to_string());
            Ok(())
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            missing.push(label.to_string());
            Ok(())
        }
        Err(error) => Err(error),
    }
}
