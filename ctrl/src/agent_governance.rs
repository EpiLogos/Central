use crate::control::AGENT_RETRIEVAL_DENY_MARKER;
use crate::projectcentral::{
    read_project_manifest, AGENT_GOVERNANCE_DIR, PROJECTCENTRAL_DIR, PROJECT_MANIFEST,
    ROOT_AGENT_GOVERNANCE_DIR,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;
use std::path::{Component, Path};
use std::time::{SystemTime, UNIX_EPOCH};

pub const GOVERNANCE_RELATIONS_SOURCE: &str = "ProjectCentral/relations/governance-relations.json";
pub const GOVERNANCE_RELATIONS_SCHEMA: &str = "central.agent-governance-relations/v1";

const MAX_GOVERNANCE_SCAN_DEPTH: usize = 12;
const MAX_NATIVE_SCAN_DEPTH: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GovernanceProvenance {
    HumanAuthored,
    HumanAdopted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GovernanceTreatment {
    CanonicalGovernance,
    RetainNativeInPlace,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GovernanceSourceRelation {
    #[serde(rename = "ref")]
    pub source_ref: String,
    pub path: String,
    pub provenance: GovernanceProvenance,
    pub treatment: GovernanceTreatment,
    #[serde(default)]
    pub roles: Vec<String>,
    pub recognition: String,
    pub recorded_at_unix_seconds: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct GovernanceRelationsFile {
    schema: String,
    project_id: String,
    #[serde(default)]
    relations: Vec<GovernanceSourceRelation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GovernanceSourceRecord {
    #[serde(rename = "ref")]
    pub source_ref: String,
    pub path: String,
    pub scope: String,
    pub provenance: GovernanceProvenance,
    pub treatment: GovernanceTreatment,
    pub roles: Vec<String>,
    pub basis: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GovernanceCandidate {
    pub path: String,
    pub source_role: String,
    pub provenance: String,
    pub reason: String,
    pub suggested_treatments: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GovernanceSkippedSource {
    pub path: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GovernanceCompositionBoundary {
    pub root_source: String,
    pub project_source: String,
    pub situated_instruction_layer: String,
    pub operational_resolution_owner: String,
    pub operational_precedence_defined_by_central: bool,
    pub conflicts_must_remain_explainable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GovernanceMaintenancePolicy {
    pub observation_automatically_mutates_governance: bool,
    pub proposal_then_human_adoption: bool,
    pub pruning_is_normal_maintenance: bool,
    pub procedures_should_prefer_skills: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RootGovernanceInspection {
    pub central_root: String,
    pub governance_root: String,
    pub sources: Vec<GovernanceSourceRecord>,
    pub skipped_sources: Vec<GovernanceSkippedSource>,
    pub composition: GovernanceCompositionBoundary,
    pub maintenance: GovernanceMaintenancePolicy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProjectGovernanceInspection {
    pub project_root: String,
    pub projectcentral_ready: bool,
    pub project_id: Option<String>,
    pub governance_root: String,
    pub canonical_sources: Vec<GovernanceSourceRecord>,
    pub retained_native_sources: Vec<GovernanceSourceRecord>,
    pub native_candidates: Vec<GovernanceCandidate>,
    pub skipped_sources: Vec<GovernanceSkippedSource>,
    pub relation_source: Option<String>,
    pub composition: GovernanceCompositionBoundary,
    pub maintenance: GovernanceMaintenancePolicy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GovernancePlanItem {
    pub source: String,
    pub recommendation: String,
    pub why: String,
    pub changes: Vec<String>,
    pub requires_human_acceptance: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProjectGovernancePlan {
    pub current: ProjectGovernanceInspection,
    pub proposals: Vec<GovernancePlanItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GovernanceApplyResult {
    pub relation_source: String,
    pub relation: GovernanceSourceRelation,
    pub source_bytes_mutated: bool,
    pub source_path_mutated: bool,
    pub relation_metadata_mutated: bool,
    pub operational_precedence_mutated: bool,
}

pub fn inspect_root_governance(central_root: &Path) -> io::Result<RootGovernanceInspection> {
    ensure_directory(central_root, "Central root")?;
    let governance_root = central_root.join(ROOT_AGENT_GOVERNANCE_DIR);
    let mut files = Vec::new();
    let mut skipped_sources = Vec::new();
    if governance_root.is_dir() {
        collect_governance_files(
            &governance_root,
            central_root,
            0,
            &mut files,
            &mut skipped_sources,
        )?;
    }
    let sources = files
        .into_iter()
        .map(|path| GovernanceSourceRecord {
            source_ref: governance_ref("root", &path),
            path,
            scope: "cross-project".to_owned(),
            provenance: GovernanceProvenance::HumanAuthored,
            treatment: GovernanceTreatment::CanonicalGovernance,
            roles: Vec::new(),
            basis: "canonical Control/agents/governance human-authored source region".to_owned(),
        })
        .collect();
    Ok(RootGovernanceInspection {
        central_root: normalize_path(central_root),
        governance_root: ROOT_AGENT_GOVERNANCE_DIR.to_owned(),
        sources,
        skipped_sources,
        composition: composition_boundary(),
        maintenance: maintenance_policy(),
    })
}

pub fn inspect_project_governance(project_root: &Path) -> io::Result<ProjectGovernanceInspection> {
    ensure_directory(project_root, "Project root")?;
    let manifest_path = project_root.join(PROJECTCENTRAL_DIR).join(PROJECT_MANIFEST);
    let manifest = if manifest_path.is_file() {
        let manifest = read_project_manifest(project_root)?;
        let validation = manifest.validate();
        if !validation.valid {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                validation.errors.join("; "),
            ));
        }
        Some(manifest)
    } else {
        None
    };
    let project_id = manifest.as_ref().map(|value| value.project_id.clone());
    let project_key = project_id
        .clone()
        .or_else(|| project_root.file_name().and_then(|value| value.to_str()).map(str::to_owned))
        .unwrap_or_else(|| "project".to_owned());

    let mut skipped_sources = Vec::new();
    let mut canonical_files = Vec::new();
    let governance_root = project_root.join(AGENT_GOVERNANCE_DIR);
    if governance_root.is_dir() {
        collect_governance_files(
            &governance_root,
            project_root,
            0,
            &mut canonical_files,
            &mut skipped_sources,
        )?;
    }
    let canonical_sources = canonical_files
        .into_iter()
        .map(|path| GovernanceSourceRecord {
            source_ref: governance_ref(&project_key, &path),
            path,
            scope: "project".to_owned(),
            provenance: GovernanceProvenance::HumanAuthored,
            treatment: GovernanceTreatment::CanonicalGovernance,
            roles: Vec::new(),
            basis: "canonical ProjectCentral/agents/governance human-authored source region".to_owned(),
        })
        .collect::<Vec<_>>();

    let relations = read_relations_file(project_root, project_id.as_deref())?
        .map(|value| value.relations)
        .unwrap_or_default();
    let retained_native_sources = relations
        .iter()
        .filter(|relation| project_root.join(&relation.path).is_file())
        .map(|relation| GovernanceSourceRecord {
            source_ref: relation.source_ref.clone(),
            path: relation.path.clone(),
            scope: "project".to_owned(),
            provenance: relation.provenance,
            treatment: relation.treatment,
            roles: relation.roles.clone(),
            basis: "explicit human-accepted Project governance source relation".to_owned(),
        })
        .collect::<Vec<_>>();
    let recognised_paths = canonical_sources
        .iter()
        .map(|source| source.path.clone())
        .chain(retained_native_sources.iter().map(|source| source.path.clone()))
        .collect::<BTreeSet<_>>();

    let mut native_candidates = Vec::new();
    collect_native_candidates(
        project_root,
        project_root,
        0,
        &recognised_paths,
        &mut native_candidates,
        &mut skipped_sources,
    )?;
    native_candidates.sort_by(|a, b| a.path.cmp(&b.path));
    native_candidates.dedup_by(|a, b| a.path == b.path);

    Ok(ProjectGovernanceInspection {
        project_root: normalize_path(project_root),
        projectcentral_ready: manifest.is_some(),
        project_id,
        governance_root: AGENT_GOVERNANCE_DIR.to_owned(),
        canonical_sources,
        retained_native_sources,
        native_candidates,
        skipped_sources,
        relation_source: project_root
            .join(GOVERNANCE_RELATIONS_SOURCE)
            .is_file()
            .then(|| GOVERNANCE_RELATIONS_SOURCE.to_owned()),
        composition: composition_boundary(),
        maintenance: maintenance_policy(),
    })
}

pub fn plan_project_governance(project_root: &Path) -> io::Result<ProjectGovernancePlan> {
    let current = inspect_project_governance(project_root)?;
    let mut proposals = Vec::new();
    for candidate in &current.native_candidates {
        proposals.push(GovernancePlanItem {
            source: candidate.path.clone(),
            recommendation: "Retain the native instruction file in place if the human recognises it as Project Agent governance.".to_owned(),
            why: "The filename/path suggests Agent-oriented instruction, but Central does not infer human governance authority from a conventional filename alone.".to_owned(),
            changes: vec![
                "record a stable Project governance source relation".to_owned(),
                "leave source bytes and source path unchanged".to_owned(),
                "leave operational precedence/composition to AIKit".to_owned(),
                "alternatively leave the file as ordinary Project source".to_owned(),
            ],
            requires_human_acceptance: true,
        });
    }
    Ok(ProjectGovernancePlan { current, proposals })
}

pub fn apply_project_governance_relation(
    project_root: &Path,
    source: &str,
    provenance: GovernanceProvenance,
    roles: Vec<String>,
) -> io::Result<GovernanceApplyResult> {
    ensure_directory(project_root, "Project root")?;
    ensure_project_member(source)?;
    let manifest = read_project_manifest(project_root)?;
    let validation = manifest.validate();
    if !validation.valid {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            validation.errors.join("; "),
        ));
    }
    let source = normalize_relative(source);
    if source.starts_with(&format!("{PROJECTCENTRAL_DIR}/")) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "retain-native governance relation is for Project-owned source outside ProjectCentral; canonical ProjectCentral governance is already a human-authored source region",
        ));
    }
    if !project_root.join(&source).is_file() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("governance source does not exist as a file: {source}"),
        ));
    }

    let mut file = read_relations_file(project_root, Some(&manifest.project_id))?
        .unwrap_or_else(|| GovernanceRelationsFile {
            schema: GOVERNANCE_RELATIONS_SCHEMA.to_owned(),
            project_id: manifest.project_id.clone(),
            relations: Vec::new(),
        });
    let existing_ref = file
        .relations
        .iter()
        .find(|relation| relation.path == source)
        .map(|relation| relation.source_ref.clone());
    let relation = GovernanceSourceRelation {
        source_ref: existing_ref.unwrap_or_else(|| governance_ref(&manifest.project_id, &source)),
        path: source.clone(),
        provenance,
        treatment: GovernanceTreatment::RetainNativeInPlace,
        roles: normalize_roles(roles),
        recognition: "human-accepted Project Agent-governance source relation".to_owned(),
        recorded_at_unix_seconds: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
    };
    if let Some(existing) = file
        .relations
        .iter_mut()
        .find(|existing| existing.path == source)
    {
        *existing = relation.clone();
    } else {
        file.relations.push(relation.clone());
        file.relations.sort_by(|a, b| a.path.cmp(&b.path));
    }
    write_relations_file(project_root, &file)?;
    Ok(GovernanceApplyResult {
        relation_source: GOVERNANCE_RELATIONS_SOURCE.to_owned(),
        relation,
        source_bytes_mutated: false,
        source_path_mutated: false,
        relation_metadata_mutated: true,
        operational_precedence_mutated: false,
    })
}

fn composition_boundary() -> GovernanceCompositionBoundary {
    GovernanceCompositionBoundary {
        root_source: ROOT_AGENT_GOVERNANCE_DIR.to_owned(),
        project_source: AGENT_GOVERNANCE_DIR.to_owned(),
        situated_instruction_layer: "task/session/Focus-specific instruction owned outside Central source hierarchy".to_owned(),
        operational_resolution_owner: "AIKit".to_owned(),
        operational_precedence_defined_by_central: false,
        conflicts_must_remain_explainable: true,
    }
}

fn maintenance_policy() -> GovernanceMaintenancePolicy {
    GovernanceMaintenancePolicy {
        observation_automatically_mutates_governance: false,
        proposal_then_human_adoption: true,
        pruning_is_normal_maintenance: true,
        procedures_should_prefer_skills: true,
    }
}

fn collect_governance_files(
    current: &Path,
    base: &Path,
    depth: usize,
    output: &mut Vec<String>,
    skipped: &mut Vec<GovernanceSkippedSource>,
) -> io::Result<()> {
    if depth > MAX_GOVERNANCE_SCAN_DEPTH {
        return Ok(());
    }
    if current.join(AGENT_RETRIEVAL_DENY_MARKER).is_file() {
        skipped.push(GovernanceSkippedSource {
            path: relative_string(base, current)?,
            reason: "not-agent-readable".to_owned(),
        });
        return Ok(());
    }
    for entry in fs::read_dir(current)? {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            collect_governance_files(&path, base, depth + 1, output, skipped)?;
        } else if file_type.is_file()
            && entry.file_name().to_string_lossy() != AGENT_RETRIEVAL_DENY_MARKER
        {
            output.push(relative_string(base, &path)?);
        }
    }
    output.sort();
    output.dedup();
    Ok(())
}

fn collect_native_candidates(
    current: &Path,
    project_root: &Path,
    depth: usize,
    recognised: &BTreeSet<String>,
    output: &mut Vec<GovernanceCandidate>,
    skipped: &mut Vec<GovernanceSkippedSource>,
) -> io::Result<()> {
    if depth > MAX_NATIVE_SCAN_DEPTH {
        return Ok(());
    }
    if current != project_root && current.join(AGENT_RETRIEVAL_DENY_MARKER).is_file() {
        skipped.push(GovernanceSkippedSource {
            path: relative_string(project_root, current)?,
            reason: "not-agent-readable".to_owned(),
        });
        return Ok(());
    }
    for entry in fs::read_dir(current)? {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;
        let name = entry.file_name().to_string_lossy().to_string();
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            if should_skip_directory(&name) {
                continue;
            }
            collect_native_candidates(
                &path,
                project_root,
                depth + 1,
                recognised,
                output,
                skipped,
            )?;
            continue;
        }
        if !file_type.is_file() || name == AGENT_RETRIEVAL_DENY_MARKER {
            continue;
        }
        let relative = relative_string(project_root, &path)?;
        if recognised.contains(&relative) || !looks_like_agent_instruction(&relative) {
            continue;
        }
        output.push(GovernanceCandidate {
            path: relative,
            source_role: "possible-project-agent-governance".to_owned(),
            provenance: "unresolved".to_owned(),
            reason: "conventional Agent/instruction filename or path signal only; human governance authority is not inferred from naming".to_owned(),
            suggested_treatments: vec![
                "retain-native-in-place".to_owned(),
                "leave-ordinary-project-source".to_owned(),
                "move-procedure-to-skill-after-review".to_owned(),
                "human-judgement-required".to_owned(),
            ],
        });
    }
    Ok(())
}

fn looks_like_agent_instruction(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    let file = Path::new(&lower)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    matches!(
        file,
        "agents.md" | "agent.md" | "claude.md" | "gemini.md" | "copilot-instructions.md"
    ) || lower.contains("agent-instruction")
        || lower.contains("agent_instruction")
        || lower.contains("/instructions/agent")
        || lower.contains("/.github/instructions")
}

fn should_skip_directory(name: &str) -> bool {
    matches!(
        name,
        ".git"
            | ".central"
            | ".obsidian"
            | "node_modules"
            | "target"
            | "dist"
            | "build"
            | "vendor"
            | ".venv"
            | "venv"
            | PROJECTCENTRAL_DIR
    )
}

fn read_relations_file(
    project_root: &Path,
    project_id: Option<&str>,
) -> io::Result<Option<GovernanceRelationsFile>> {
    let path = project_root.join(GOVERNANCE_RELATIONS_SOURCE);
    if !path.is_file() {
        return Ok(None);
    }
    let value: GovernanceRelationsFile = serde_json::from_slice(&fs::read(&path)?).map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{} is not a valid governance relation file: {error}", path.display()),
        )
    })?;
    if value.schema != GOVERNANCE_RELATIONS_SCHEMA {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("governance relation schema must be {GOVERNANCE_RELATIONS_SCHEMA}"),
        ));
    }
    if let Some(project_id) = project_id {
        if value.project_id != project_id {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "governance relation project_id does not match ProjectCentral/project.json",
            ));
        }
    }
    Ok(Some(value))
}

fn write_relations_file(project_root: &Path, value: &GovernanceRelationsFile) -> io::Result<()> {
    let path = project_root.join(GOVERNANCE_RELATIONS_SOURCE);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut bytes = serde_json::to_vec_pretty(value)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    bytes.push(b'\n');
    fs::write(path, bytes)
}

fn normalize_roles(roles: Vec<String>) -> Vec<String> {
    let mut roles = roles
        .into_iter()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    roles.sort();
    roles.dedup();
    roles
}

fn governance_ref(scope: &str, path: &str) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in path.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("central:agent-governance:{scope}:{hash:016x}")
}

fn ensure_directory(path: &Path, label: &str) -> io::Result<()> {
    if path.is_dir() {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("{label} does not exist as a directory: {}", path.display()),
        ))
    }
}

fn ensure_project_member(raw: &str) -> io::Result<()> {
    if raw.trim().is_empty() || raw != raw.trim() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "source must be a non-empty project-root-relative path",
        ));
    }
    let path = Path::new(raw);
    if path.is_absolute()
        || !path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "source must remain inside the Project and may not contain parent/root components",
        ));
    }
    Ok(())
}

fn normalize_relative(raw: &str) -> String {
    Path::new(raw).to_string_lossy().replace('\\', "/")
}

fn relative_string(base: &Path, path: &Path) -> io::Result<String> {
    path.strip_prefix(base)
        .map(normalize_path)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "source escaped expected root"))
}

fn normalize_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::projectcentral_ops::initialize_projectcentral;
    use tempfile::tempdir;

    #[test]
    fn root_and_project_governance_are_stable_distinct_human_sources() {
        let temp = tempdir().unwrap();
        let central = temp.path().join("Central");
        let project = central.join("Work/example");
        fs::create_dir_all(central.join(ROOT_AGENT_GOVERNANCE_DIR)).unwrap();
        fs::create_dir_all(&project).unwrap();
        fs::write(
            central.join(ROOT_AGENT_GOVERNANCE_DIR).join("collaboration.md"),
            "Prefer evidence-backed completion claims.\n",
        )
        .unwrap();
        initialize_projectcentral(&central, &project, "example/project").unwrap();
        fs::write(
            project.join(AGENT_GOVERNANCE_DIR).join("project.md"),
            "Consult authored product ground when meaning changes.\n",
        )
        .unwrap();

        let root = inspect_root_governance(&central).unwrap();
        let local = inspect_project_governance(&project).unwrap();
        assert_eq!(root.sources.len(), 1);
        assert_eq!(local.canonical_sources.len(), 1);
        assert_ne!(root.sources[0].source_ref, local.canonical_sources[0].source_ref);
        assert_eq!(root.sources[0].scope, "cross-project");
        assert_eq!(local.canonical_sources[0].scope, "project");
        assert_eq!(local.composition.operational_resolution_owner, "AIKit");
        assert!(!local.composition.operational_precedence_defined_by_central);
    }

    #[test]
    fn existing_agent_instruction_is_unresolved_then_retained_without_source_mutation() {
        let temp = tempdir().unwrap();
        let central = temp.path().join("Central");
        let project = central.join("Work/existing");
        fs::create_dir_all(&project).unwrap();
        fs::write(project.join("AGENTS.md"), "Project-native instructions.\n").unwrap();
        initialize_projectcentral(&central, &project, "example/existing").unwrap();

        let before = inspect_project_governance(&project).unwrap();
        assert_eq!(before.native_candidates.len(), 1);
        assert_eq!(before.native_candidates[0].path, "AGENTS.md");
        assert_eq!(before.native_candidates[0].provenance, "unresolved");

        let applied = apply_project_governance_relation(
            &project,
            "AGENTS.md",
            GovernanceProvenance::HumanAdopted,
            vec!["project-collaboration".to_owned()],
        )
        .unwrap();
        assert!(!applied.source_bytes_mutated);
        assert!(!applied.source_path_mutated);
        assert!(!applied.operational_precedence_mutated);
        assert_eq!(
            fs::read_to_string(project.join("AGENTS.md")).unwrap(),
            "Project-native instructions.\n"
        );
        let after = inspect_project_governance(&project).unwrap();
        assert_eq!(after.retained_native_sources.len(), 1);
        assert_eq!(after.retained_native_sources[0].path, "AGENTS.md");
        assert!(after.native_candidates.is_empty());
    }

    #[test]
    fn retrieval_denial_excludes_private_governance_from_agent_read_model() {
        let temp = tempdir().unwrap();
        let central = temp.path().join("Central");
        let private = central.join(ROOT_AGENT_GOVERNANCE_DIR).join("private");
        fs::create_dir_all(&private).unwrap();
        fs::write(private.join(AGENT_RETRIEVAL_DENY_MARKER), "").unwrap();
        fs::write(private.join("notes.md"), "not agent-readable\n").unwrap();

        let inspection = inspect_root_governance(&central).unwrap();
        assert!(inspection.sources.is_empty());
        assert_eq!(inspection.skipped_sources.len(), 1);
        assert_eq!(inspection.skipped_sources[0].reason, "not-agent-readable");
    }

    #[test]
    fn planning_never_claims_filename_authority_or_operational_precedence() {
        let temp = tempdir().unwrap();
        let central = temp.path().join("Central");
        let project = central.join("Work/plan");
        fs::create_dir_all(project.join(".github")).unwrap();
        fs::write(
            project.join(".github/copilot-instructions.md"),
            "Existing tool-shaped instruction source.\n",
        )
        .unwrap();
        initialize_projectcentral(&central, &project, "example/plan").unwrap();

        let plan = plan_project_governance(&project).unwrap();
        assert_eq!(plan.proposals.len(), 1);
        assert!(plan.proposals[0].why.contains("does not infer human governance authority"));
        assert!(plan.proposals[0]
            .changes
            .iter()
            .any(|change| change.contains("operational precedence/composition to AIKit")));
        assert!(!plan.current.composition.operational_precedence_defined_by_central);
    }

    #[test]
    fn maintenance_policy_requires_proposal_adoption_and_normalises_pruning() {
        let policy = maintenance_policy();
        assert!(!policy.observation_automatically_mutates_governance);
        assert!(policy.proposal_then_human_adoption);
        assert!(policy.pruning_is_normal_maintenance);
        assert!(policy.procedures_should_prefer_skills);
    }
}
