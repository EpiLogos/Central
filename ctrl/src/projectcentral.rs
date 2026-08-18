use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

pub const PROJECTCENTRAL_DIR: &str = "ProjectCentral";
pub const PROJECT_MANIFEST: &str = "project.json";
pub const HUMAN_SOURCE_DIR: &str = "ProjectCentral/user";
pub const AGENT_DIR: &str = "ProjectCentral/agents";
pub const AGENT_GOVERNANCE_DIR: &str = "ProjectCentral/agents/governance";
pub const WIKI_DIR: &str = "ProjectCentral/agents/wiki";
pub const WIKI_SOURCE: &str = "ProjectCentral/agents/wiki/wiki.json";
pub const ROOT_HUMAN_SOURCE_DIR: &str = "Control/user";
pub const ROOT_AGENT_DIR: &str = "Control/agents";
pub const ROOT_AGENT_GOVERNANCE_DIR: &str = "Control/agents/governance";
pub const ROOT_WIKI_DIR: &str = "Control/agents/wiki";
pub const ROOT_WIKI_SOURCE: &str = "Control/agents/wiki/wiki.json";
pub const PROJECT_SCHEMA: &str = "central.project/v1";
pub const WIKI_PROFILE: &str = "okf-wiki/v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectCentralManifest {
    pub schema: String,
    pub project_id: String,
    pub human_source: String,
    pub wiki: WikiBinding,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WikiBinding {
    pub profile: String,
    pub source: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub adopted_sources: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProjectCentralPaths {
    pub project_root: PathBuf,
    pub projectcentral_root: PathBuf,
    pub manifest: PathBuf,
    pub human_source: PathBuf,
    pub agent_root: PathBuf,
    pub agent_governance: PathBuf,
    pub wiki_root: PathBuf,
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
            human_source: HUMAN_SOURCE_DIR.to_owned(),
            wiki: WikiBinding {
                profile: WIKI_PROFILE.to_owned(),
                source: WIKI_SOURCE.to_owned(),
                adopted_sources: Vec::new(),
            },
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
        if self.human_source != HUMAN_SOURCE_DIR {
            errors.push(format!("human_source must be the canonical fractal human-authorship root {HUMAN_SOURCE_DIR}"));
        }
        if self.wiki.profile != WIKI_PROFILE {
            errors.push(format!("wiki.profile must be {WIKI_PROFILE}"));
        }
        if self.wiki.source != WIKI_SOURCE {
            errors.push(format!("wiki.source must be the canonical Agent Wiki source {WIKI_SOURCE}"));
        }
        validate_project_member("human_source", &self.human_source, &mut errors);
        validate_project_member("wiki.source", &self.wiki.source, &mut errors);
        for (index, source) in self.wiki.adopted_sources.iter().enumerate() {
            validate_project_member(&format!("wiki.adopted_sources[{index}]"), source, &mut errors);
        }
        ManifestValidation { valid: errors.is_empty(), errors }
    }
}

pub fn projectcentral_paths(project_root: &Path, manifest: &ProjectCentralManifest) -> ProjectCentralPaths {
    let projectcentral_root = project_root.join(PROJECTCENTRAL_DIR);
    ProjectCentralPaths {
        project_root: project_root.to_path_buf(),
        manifest: projectcentral_root.join(PROJECT_MANIFEST),
        human_source: project_root.join(&manifest.human_source),
        agent_root: project_root.join(AGENT_DIR),
        agent_governance: project_root.join(AGENT_GOVERNANCE_DIR),
        wiki_root: project_root.join(WIKI_DIR),
        wiki_source: project_root.join(&manifest.wiki.source),
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

fn validate_project_member(field: &str, raw: &str, errors: &mut Vec<String>) {
    if raw.trim().is_empty() || raw != raw.trim() {
        errors.push(format!("{field} must be a non-empty project-root-relative path without surrounding whitespace"));
        return;
    }
    let path = Path::new(raw);
    let safe = !path.is_absolute()
        && path.components().all(|component| matches!(component, Component::Normal(_)));
    if !safe {
        errors.push(format!("{field} must remain inside the Project and may not contain parent/root components"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_manifest_is_valid_and_fractal() {
        let manifest = ProjectCentralManifest::new("epilogos/example");
        assert!(manifest.validate().valid);
        assert_eq!(manifest.human_source, HUMAN_SOURCE_DIR);
        assert_eq!(manifest.wiki.profile, WIKI_PROFILE);
        assert_eq!(manifest.wiki.source, WIKI_SOURCE);
        assert!(manifest.wiki.adopted_sources.is_empty());
    }

    #[test]
    fn manifest_keeps_canonical_human_and_agent_roots_but_allows_safe_adopted_sources() {
        let mut manifest = ProjectCentralManifest::new("epilogos/example");
        manifest.wiki.adopted_sources.push("docs/wiki.json".to_owned());
        assert!(manifest.validate().valid);

        manifest.human_source = "README.md".to_owned();
        manifest.wiki.source = "docs/wiki.json".to_owned();
        manifest.wiki.adopted_sources.push("../wiki.json".to_owned());
        let validation = manifest.validate();
        assert!(!validation.valid);
        assert_eq!(validation.errors.len(), 3);
    }

    #[test]
    fn projectcentral_paths_are_the_control_fractal() {
        let manifest = ProjectCentralManifest::new("epilogos/example");
        let paths = projectcentral_paths(Path::new("/central/Work/example"), &manifest);
        assert_eq!(paths.projectcentral_root, Path::new("/central/Work/example/ProjectCentral"));
        assert_eq!(paths.human_source, Path::new("/central/Work/example/ProjectCentral/user"));
        assert_eq!(paths.agent_governance, Path::new("/central/Work/example/ProjectCentral/agents/governance"));
        assert_eq!(paths.wiki_source, Path::new("/central/Work/example/ProjectCentral/agents/wiki/wiki.json"));
        assert_eq!(ROOT_WIKI_SOURCE, "Control/agents/wiki/wiki.json");
    }
}
