use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

pub const PROJECTCENTRAL_DIR: &str = "ProjectCentral";
pub const PROJECT_MANIFEST: &str = "project.json";
pub const HUMAN_APERTURE: &str = "README.md";
pub const WIKI_DIR: &str = "Wiki";
pub const WIKI_SOURCE: &str = "Wiki/wiki.json";
pub const ROOT_WIKI_SOURCE: &str = "Wiki/wiki.json";
pub const PROJECT_SCHEMA: &str = "central.project/v1";
pub const WIKI_PROFILE: &str = "okf-wiki/v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectCentralManifest {
    pub schema: String,
    pub project_id: String,
    pub wiki: WikiBinding,
    pub human_aperture: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WikiBinding {
    pub profile: String,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProjectCentralPaths {
    pub project_root: PathBuf,
    pub projectcentral_root: PathBuf,
    pub manifest: PathBuf,
    pub human_aperture: PathBuf,
    pub wiki_source: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ManifestValidation {
    pub valid: bool,
    pub errors: Vec<String>,
}

impl ProjectCentralManifest {
    pub fn new(project_id: impl Into<String>) -> Self {
        Self {
            schema: PROJECT_SCHEMA.to_owned(),
            project_id: project_id.into(),
            wiki: WikiBinding {
                profile: WIKI_PROFILE.to_owned(),
                source: WIKI_SOURCE.to_owned(),
            },
            human_aperture: HUMAN_APERTURE.to_owned(),
        }
    }

    pub fn validate(&self) -> ManifestValidation {
        let mut errors = Vec::new();
        if self.schema != PROJECT_SCHEMA {
            errors.push(format!("schema must be {PROJECT_SCHEMA}"));
        }
        if self.project_id.trim().is_empty() || self.project_id != self.project_id.trim() {
            errors.push("project_id must be a non-empty stable identity without surrounding whitespace".to_owned());
        }
        if self.wiki.profile != WIKI_PROFILE {
            errors.push(format!("wiki.profile must be {WIKI_PROFILE}"));
        }
        validate_relative_member("wiki.source", &self.wiki.source, &mut errors);
        validate_relative_member("human_aperture", &self.human_aperture, &mut errors);
        ManifestValidation { valid: errors.is_empty(), errors }
    }
}

pub fn projectcentral_paths(project_root: &Path, manifest: &ProjectCentralManifest) -> ProjectCentralPaths {
    let projectcentral_root = project_root.join(PROJECTCENTRAL_DIR);
    ProjectCentralPaths {
        project_root: project_root.to_path_buf(),
        manifest: projectcentral_root.join(PROJECT_MANIFEST),
        human_aperture: projectcentral_root.join(&manifest.human_aperture),
        wiki_source: projectcentral_root.join(&manifest.wiki.source),
        projectcentral_root,
    }
}

pub fn read_project_manifest(project_root: &Path) -> io::Result<ProjectCentralManifest> {
    let path = project_root.join(PROJECTCENTRAL_DIR).join(PROJECT_MANIFEST);
    let bytes = fs::read(&path)?;
    serde_json::from_slice(&bytes).map_err(|error| {
        io::Error::new(io::ErrorKind::InvalidData, format!("{} is not a valid ProjectCentral manifest: {error}", path.display()))
    })
}

fn validate_relative_member(field: &str, raw: &str, errors: &mut Vec<String>) {
    if raw.trim().is_empty() || raw != raw.trim() {
        errors.push(format!("{field} must be a non-empty relative path without surrounding whitespace"));
        return;
    }
    let path = Path::new(raw);
    let safe = !path.is_absolute()
        && path.components().all(|component| matches!(component, Component::Normal(_)));
    if !safe {
        errors.push(format!("{field} must remain inside ProjectCentral and may not contain parent/root components"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_manifest_is_valid() {
        let manifest = ProjectCentralManifest::new("epilogos/example");
        assert!(manifest.validate().valid);
        assert_eq!(manifest.wiki.profile, WIKI_PROFILE);
        assert_eq!(manifest.wiki.source, WIKI_SOURCE);
    }

    #[test]
    fn manifest_rejects_paths_that_escape_projectcentral() {
        let mut manifest = ProjectCentralManifest::new("epilogos/example");
        manifest.wiki.source = "../wiki.json".to_owned();
        manifest.human_aperture = "/tmp/README.md".to_owned();
        let validation = manifest.validate();
        assert!(!validation.valid);
        assert_eq!(validation.errors.len(), 2);
    }

    #[test]
    fn projectcentral_paths_are_project_local() {
        let manifest = ProjectCentralManifest::new("epilogos/example");
        let paths = projectcentral_paths(Path::new("/central/Work/example"), &manifest);
        assert_eq!(paths.projectcentral_root, Path::new("/central/Work/example/ProjectCentral"));
        assert_eq!(paths.human_aperture, Path::new("/central/Work/example/ProjectCentral/README.md"));
        assert_eq!(paths.wiki_source, Path::new("/central/Work/example/ProjectCentral/Wiki/wiki.json"));
    }
}
