use crate::action::{
    ActionAvailability, ActionDescriptor, ActionExecutionContext, ActionInputDefinition,
    ActionOutputDefinition, ActionRegistry, MutationClass,
};
use crate::control::AGENT_RETRIEVAL_DENY_MARKER;
use crate::projectcentral::{
    read_project_manifest, ProjectCentralManifest, AGENT_DIR, HUMAN_SOURCE_DIR, PROJECTCENTRAL_DIR,
    PROJECT_MANIFEST, WIKI_PROFILE,
};
use crate::result::{ActionResult, ResultStatus};
use crate::root::resolve_central_root;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub const GROUND_RELATIONS_DIR: &str = "ProjectCentral/relations";
pub const GROUND_RELATIONS_SOURCE: &str = "ProjectCentral/relations/source-relations.json";
pub const GROUND_RELATIONS_SCHEMA: &str = "central.project.ground-relations/v1";

const MAX_NATIVE_SCAN_DEPTH: usize = 5;
const MAX_HUMAN_SCAN_DEPTH: usize = 12;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GroundStatus {
    Empty,
    Partial,
    Established,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SourceProvenance {
    HumanAuthored,
    HumanEditedDraft,
    HumanAdopted,
    GeneratedSuggestion,
    GeneratedDerived,
    AgentMaintained,
    Observed,
    Inference,
    Unresolved,
}

impl SourceProvenance {
    fn from_input(raw: &str) -> io::Result<Self> {
        match raw {
            "human-authored" => Ok(Self::HumanAuthored),
            "human-edited-draft" => Ok(Self::HumanEditedDraft),
            "human-adopted" => Ok(Self::HumanAdopted),
            "generated-suggestion" => Ok(Self::GeneratedSuggestion),
            "generated-derived" => Ok(Self::GeneratedDerived),
            "agent-maintained" => Ok(Self::AgentMaintained),
            "observed" => Ok(Self::Observed),
            "inference" => Ok(Self::Inference),
            "unresolved" => Ok(Self::Unresolved),
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("unsupported source provenance: {raw}"),
            )),
        }
    }

    fn is_recognised_human_source(self) -> bool {
        matches!(self, Self::HumanAuthored | Self::HumanAdopted)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SourceStanding {
    Unspecified,
    AuthoredHumanPosition,
    DesignCommitment,
    ArchitectureContract,
    ImplementationFact,
    ObservedEvidence,
    CurrentDevelopmentState,
    AgentInference,
}

impl SourceStanding {
    fn from_input(raw: &str) -> io::Result<Self> {
        match raw {
            "unspecified" => Ok(Self::Unspecified),
            "authored-human-position" => Ok(Self::AuthoredHumanPosition),
            "design-commitment" => Ok(Self::DesignCommitment),
            "architecture-contract" => Ok(Self::ArchitectureContract),
            "implementation-fact" => Ok(Self::ImplementationFact),
            "observed-evidence" => Ok(Self::ObservedEvidence),
            "current-development-state" => Ok(Self::CurrentDevelopmentState),
            "agent-inference" => Ok(Self::AgentInference),
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("unsupported source standing: {raw}"),
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SourceTreatment {
    ProjectcentralUser,
    RetainNativeInPlace,
    OrdinaryProjectSource,
    GeneratedDerived,
    Unresolved,
}

impl SourceTreatment {
    fn from_input(raw: &str) -> io::Result<Self> {
        match raw {
            "projectcentral-user" => Ok(Self::ProjectcentralUser),
            "retain-native-in-place" => Ok(Self::RetainNativeInPlace),
            "ordinary-project-source" => Ok(Self::OrdinaryProjectSource),
            "generated-derived" => Ok(Self::GeneratedDerived),
            "unresolved" => Ok(Self::Unresolved),
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("unsupported source treatment: {raw}"),
            )),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroundSourceRelation {
    #[serde(rename = "ref")]
    pub source_ref: String,
    pub path: String,
    pub provenance: SourceProvenance,
    pub standing: SourceStanding,
    #[serde(default)]
    pub roles: Vec<String>,
    pub treatment: SourceTreatment,
    pub recognition: String,
    pub recorded_at_unix_seconds: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct GroundRelationsFile {
    schema: String,
    project_id: String,
    #[serde(default)]
    relations: Vec<GroundSourceRelation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GroundSourceRecord {
    #[serde(rename = "ref")]
    pub source_ref: String,
    pub path: String,
    pub exists: bool,
    pub provenance: SourceProvenance,
    pub standing: SourceStanding,
    pub roles: Vec<String>,
    pub treatment: SourceTreatment,
    pub basis: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GroundCandidate {
    pub path: String,
    pub role_hints: Vec<String>,
    pub authorship: String,
    pub standing: String,
    pub reason: String,
    pub suggested_treatments: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GroundSkippedSource {
    pub path: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GroundReturnPolicy {
    pub difference_automatically_mutates_human_source: bool,
    pub agent_wiki_may_be_maintained_independently: bool,
    pub human_source_return: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GroundAccountHandoff {
    pub preferred_authored_aperture: String,
    pub recognised_human_sources: Vec<GroundSourceRecord>,
    pub other_source_relations: Vec<GroundSourceRecord>,
    pub agent_wiki_source: Option<String>,
    pub agent_wiki_space_ref: Option<String>,
    pub source_relations: Option<String>,
    pub account_is_source: bool,
    pub html_is_source: bool,
    pub projection_is_source: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GroundInspection {
    pub project_root: PathBuf,
    pub projectcentral_ready: bool,
    pub project_id: Option<String>,
    pub human_source: String,
    pub status: GroundStatus,
    pub recognised_sources: Vec<GroundSourceRecord>,
    pub native_candidates: Vec<GroundCandidate>,
    pub skipped_sources: Vec<GroundSkippedSource>,
    pub account_handoff: GroundAccountHandoff,
    pub return_policy: GroundReturnPolicy,
    pub next_actions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GroundPlanItem {
    pub kind: String,
    pub source: Option<String>,
    pub recommendation: String,
    pub why: String,
    pub changes: Vec<String>,
    pub requires_human_acceptance: bool,
    pub apply_supported: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GroundPlan {
    pub current: GroundInspection,
    pub proposals: Vec<GroundPlanItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GroundApplyResult {
    pub relation_source: String,
    pub relation: GroundSourceRelation,
    pub ground_status: GroundStatus,
    pub source_bytes_mutated: bool,
    pub source_path_mutated: bool,
    pub relation_metadata_mutated: bool,
}

pub fn inspect_project_ground(project_root: &Path) -> io::Result<GroundInspection> {
    ensure_project_directory(project_root)?;
    let manifest = read_manifest_if_present(project_root)?;
    let projectcentral_ready = manifest.is_some();
    let project_id = manifest.as_ref().map(|value| value.project_id.clone());
    let human_source = manifest
        .as_ref()
        .map(|value| value.human_source.clone())
        .unwrap_or_else(|| HUMAN_SOURCE_DIR.to_owned());
    let project_key = project_id
        .clone()
        .or_else(|| project_root.file_name().and_then(|name| name.to_str()).map(str::to_owned))
        .unwrap_or_else(|| "project".to_owned());

    let explicit_relations = read_ground_relations(project_root, project_id.as_deref())?;
    let mut recognised_by_path = BTreeMap::<String, GroundSourceRecord>::new();
    for relation in explicit_relations {
        let exists = project_root.join(&relation.path).exists();
        recognised_by_path.insert(
            relation.path.clone(),
            GroundSourceRecord {
                source_ref: relation.source_ref,
                path: relation.path,
                exists,
                provenance: relation.provenance,
                standing: relation.standing,
                roles: relation.roles,
                treatment: relation.treatment,
                basis: "explicit accepted ProjectCentral source relation".to_owned(),
            },
        );
    }

    let mut skipped_sources = Vec::new();
    let human_root = project_root.join(&human_source);
    if human_root.is_dir() {
        let mut human_files = Vec::new();
        collect_human_files(
            &human_root,
            project_root,
            0,
            &mut human_files,
            &mut skipped_sources,
        )?;
        for path in human_files {
            recognised_by_path.entry(path.clone()).or_insert_with(|| GroundSourceRecord {
                source_ref: source_ref(&project_key, &path),
                path,
                exists: true,
                provenance: SourceProvenance::Unresolved,
                standing: SourceStanding::Unspecified,
                roles: Vec::new(),
                treatment: SourceTreatment::ProjectcentralUser,
                basis: "human-owned ProjectCentral/user aperture; authorship is intentionally unresolved until direct authorship or adoption is recognised".to_owned(),
            });
        }
    }

    let recognised_paths = recognised_by_path.keys().cloned().collect::<BTreeSet<_>>();
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

    let recognised_sources = recognised_by_path.into_values().collect::<Vec<_>>();
    let accepted_human_source = recognised_sources.iter().any(|source| {
        source.exists && source.provenance.is_recognised_human_source()
    });
    let status = if accepted_human_source {
        GroundStatus::Established
    } else if recognised_sources.is_empty() {
        GroundStatus::Empty
    } else {
        GroundStatus::Partial
    };

    let (agent_wiki_source, agent_wiki_space_ref) = manifest
        .as_ref()
        .map(|manifest| {
            let source = manifest.wiki.source.clone();
            let space_ref = read_wiki_space_ref(&project_root.join(&source)).ok().flatten();
            (Some(source), space_ref)
        })
        .unwrap_or((None, None));

    let recognised_human_sources = recognised_sources
        .iter()
        .filter(|source| source.exists && source.provenance.is_recognised_human_source())
        .cloned()
        .collect::<Vec<_>>();
    let other_source_relations = recognised_sources
        .iter()
        .filter(|source| !source.provenance.is_recognised_human_source())
        .cloned()
        .collect::<Vec<_>>();
    let relation_path = project_root.join(GROUND_RELATIONS_SOURCE);
    let source_relations = relation_path
        .is_file()
        .then(|| GROUND_RELATIONS_SOURCE.to_owned());

    let next_actions = ground_next_actions(projectcentral_ready, status, !native_candidates.is_empty());
    Ok(GroundInspection {
        project_root: project_root.to_path_buf(),
        projectcentral_ready,
        project_id,
        human_source: human_source.clone(),
        status,
        recognised_sources,
        native_candidates,
        skipped_sources,
        account_handoff: GroundAccountHandoff {
            preferred_authored_aperture: human_source,
            recognised_human_sources,
            other_source_relations,
            agent_wiki_source,
            agent_wiki_space_ref,
            source_relations,
            account_is_source: false,
            html_is_source: false,
            projection_is_source: false,
        },
        return_policy: GroundReturnPolicy {
            difference_automatically_mutates_human_source: false,
            agent_wiki_may_be_maintained_independently: true,
            human_source_return: "direct human authorship or an explicit proposal/review/accepted source mutation".to_owned(),
        },
        next_actions,
    })
}

pub fn plan_project_ground(project_root: &Path) -> io::Result<GroundPlan> {
    let current = inspect_project_ground(project_root)?;
    let mut proposals = Vec::new();

    if !current.projectcentral_ready {
        proposals.push(GroundPlanItem {
            kind: "initialize-projectcentral".to_owned(),
            source: None,
            recommendation: "Initialize ProjectCentral before recording durable ground relations.".to_owned(),
            why: "The relation ledger belongs to ProjectCentral, while the native Project remains ordinary.".to_owned(),
            changes: vec![
                "create ProjectCentral/user and ProjectCentral/agents/{governance,wiki}".to_owned(),
                "create identity/binding metadata and canonical Agent Wiki".to_owned(),
                "do not generate a human-authored document".to_owned(),
            ],
            requires_human_acceptance: true,
            apply_supported: false,
        });
    } else if current.status == GroundStatus::Empty {
        proposals.push(GroundPlanItem {
            kind: "author-project-ground".to_owned(),
            source: None,
            recommendation: "Author one natural source under ProjectCentral/user, or explicitly recognise an existing native human source.".to_owned(),
            why: "Purpose, intended experience, design judgement or another high-altitude human responsibility is useful when the Project needs it; no document taxonomy is required.".to_owned(),
            changes: vec![
                "human authors ordinary ProjectCentral/user content directly, or keeps an existing source native".to_owned(),
                "a one-time accepted source relation establishes machine-readable provenance/standing".to_owned(),
                "Agent Wiki remains separate".to_owned(),
            ],
            requires_human_acceptance: true,
            apply_supported: true,
        });
    }

    for candidate in &current.native_candidates {
        proposals.push(GroundPlanItem {
            kind: "review-native-source".to_owned(),
            source: Some(candidate.path.clone()),
            recommendation: "Prefer retaining the native source in place if the human recognises it as Project ground.".to_owned(),
            why: "The path suggests a potentially useful authoring role, but path and extension do not establish human authorship or authority.".to_owned(),
            changes: vec![
                "accepted retain-native-in-place treatment writes relation metadata only".to_owned(),
                "source bytes and source path remain unchanged".to_owned(),
                "alternatively leave it ordinary Project source".to_owned(),
                "move/copy/reorganisation remains a separate reviewed source mutation, not an adoption default".to_owned(),
            ],
            requires_human_acceptance: true,
            apply_supported: true,
        });
    }

    if current.status == GroundStatus::Established {
        proposals.push(GroundPlanItem {
            kind: "no-required-reorganisation".to_owned(),
            source: None,
            recommendation: "Keep the established ground as-is unless returned reality gives a reason to revise it.".to_owned(),
            why: "Established authored ground is an authority relation, not a documentation completeness score.".to_owned(),
            changes: vec!["no filesystem mutation required".to_owned()],
            requires_human_acceptance: false,
            apply_supported: false,
        });
    }

    Ok(GroundPlan { current, proposals })
}

pub fn apply_accepted_ground_relation(
    project_root: &Path,
    source: &str,
    provenance: SourceProvenance,
    standing: SourceStanding,
    treatment: SourceTreatment,
    roles: Vec<String>,
) -> io::Result<GroundApplyResult> {
    ensure_project_directory(project_root)?;
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
    let source_path = project_root.join(&source);
    if !source_path.exists() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("ground source does not exist: {source}"),
        ));
    }

    let in_human_aperture = source == manifest.human_source
        || source.starts_with(&format!("{}/", manifest.human_source));
    let in_agent_region = source == AGENT_DIR || source.starts_with(&format!("{AGENT_DIR}/"));

    if treatment == SourceTreatment::ProjectcentralUser && !in_human_aperture {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "projectcentral-user treatment requires a source already inside ProjectCentral/user; this Action does not move files",
        ));
    }
    if treatment == SourceTreatment::RetainNativeInPlace && source.starts_with(&format!("{PROJECTCENTRAL_DIR}/")) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "retain-native-in-place is for ordinary Project source outside ProjectCentral",
        ));
    }
    if in_agent_region && provenance.is_recognised_human_source() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "ProjectCentral/agents is not a human-authored source region; Agent Wiki/governance authority must remain distinct",
        ));
    }

    let mut relations = read_relations_file(project_root, Some(&manifest.project_id))?
        .unwrap_or_else(|| GroundRelationsFile {
            schema: GROUND_RELATIONS_SCHEMA.to_owned(),
            project_id: manifest.project_id.clone(),
            relations: Vec::new(),
        });
    let normalised_roles = normalize_roles(roles);
    let recorded_at_unix_seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let existing_ref = relations
        .relations
        .iter()
        .find(|relation| relation.path == source)
        .map(|relation| relation.source_ref.clone());
    let relation = GroundSourceRelation {
        source_ref: existing_ref.unwrap_or_else(|| source_ref(&manifest.project_id, &source)),
        path: source.clone(),
        provenance,
        standing,
        roles: normalised_roles,
        treatment,
        recognition: "human-accepted source relation".to_owned(),
        recorded_at_unix_seconds,
    };

    if let Some(existing) = relations
        .relations
        .iter_mut()
        .find(|existing| existing.path == source)
    {
        *existing = relation.clone();
    } else {
        relations.relations.push(relation.clone());
        relations.relations.sort_by(|a, b| a.path.cmp(&b.path));
    }
    write_relations_file(project_root, &relations)?;
    let inspection = inspect_project_ground(project_root)?;
    Ok(GroundApplyResult {
        relation_source: GROUND_RELATIONS_SOURCE.to_owned(),
        relation,
        ground_status: inspection.status,
        source_bytes_mutated: false,
        source_path_mutated: false,
        relation_metadata_mutated: true,
    })
}

fn read_manifest_if_present(project_root: &Path) -> io::Result<Option<ProjectCentralManifest>> {
    if !project_root.join(PROJECTCENTRAL_DIR).join(PROJECT_MANIFEST).is_file() {
        return Ok(None);
    }
    let manifest = read_project_manifest(project_root)?;
    let validation = manifest.validate();
    if !validation.valid {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            validation.errors.join("; "),
        ));
    }
    Ok(Some(manifest))
}

fn read_ground_relations(
    project_root: &Path,
    project_id: Option<&str>,
) -> io::Result<Vec<GroundSourceRelation>> {
    Ok(read_relations_file(project_root, project_id)?
        .map(|value| value.relations)
        .unwrap_or_default())
}

fn read_relations_file(
    project_root: &Path,
    project_id: Option<&str>,
) -> io::Result<Option<GroundRelationsFile>> {
    let path = project_root.join(GROUND_RELATIONS_SOURCE);
    if !path.is_file() {
        return Ok(None);
    }
    let value: GroundRelationsFile = serde_json::from_slice(&fs::read(&path)?).map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{} is not a valid ground relation file: {error}", path.display()),
        )
    })?;
    if value.schema != GROUND_RELATIONS_SCHEMA {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("ground relation schema must be {GROUND_RELATIONS_SCHEMA}"),
        ));
    }
    if let Some(project_id) = project_id {
        if value.project_id != project_id {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "ground relation project_id does not match ProjectCentral/project.json",
            ));
        }
    }
    Ok(Some(value))
}

fn write_relations_file(project_root: &Path, value: &GroundRelationsFile) -> io::Result<()> {
    let path = project_root.join(GROUND_RELATIONS_SOURCE);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut bytes = serde_json::to_vec_pretty(value)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    bytes.push(b'\n');
    fs::write(path, bytes)
}

fn collect_human_files(
    current: &Path,
    project_root: &Path,
    depth: usize,
    output: &mut Vec<String>,
    skipped: &mut Vec<GroundSkippedSource>,
) -> io::Result<()> {
    if depth > MAX_HUMAN_SCAN_DEPTH {
        return Ok(());
    }
    if current.join(AGENT_RETRIEVAL_DENY_MARKER).is_file() {
        skipped.push(GroundSkippedSource {
            path: relative_string(project_root, current)?,
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
            collect_human_files(&path, project_root, depth + 1, output, skipped)?;
        } else if file_type.is_file()
            && entry.file_name().to_string_lossy() != AGENT_RETRIEVAL_DENY_MARKER
        {
            output.push(relative_string(project_root, &path)?);
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
    recognised_paths: &BTreeSet<String>,
    output: &mut Vec<GroundCandidate>,
    skipped: &mut Vec<GroundSkippedSource>,
) -> io::Result<()> {
    if depth > MAX_NATIVE_SCAN_DEPTH {
        return Ok(());
    }
    if current != project_root && current.join(AGENT_RETRIEVAL_DENY_MARKER).is_file() {
        skipped.push(GroundSkippedSource {
            path: relative_string(project_root, current)?,
            reason: "not-agent-readable".to_owned(),
        });
        return Ok(());
    }

    for entry in fs::read_dir(current)? {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            if should_skip_native_directory(&name) {
                continue;
            }
            collect_native_candidates(
                &path,
                project_root,
                depth + 1,
                recognised_paths,
                output,
                skipped,
            )?;
            continue;
        }
        if !file_type.is_file() || name == AGENT_RETRIEVAL_DENY_MARKER {
            continue;
        }
        let relative = relative_string(project_root, &path)?;
        if recognised_paths.contains(&relative) {
            continue;
        }
        let role_hints = role_hints_for(&relative);
        if role_hints.is_empty() {
            continue;
        }
        output.push(GroundCandidate {
            path: relative,
            role_hints,
            authorship: "unresolved".to_owned(),
            standing: "unresolved".to_owned(),
            reason: "role-like path/filename signal only; Central does not infer human authorship or authority from path or extension".to_owned(),
            suggested_treatments: vec![
                "retain-native-in-place".to_owned(),
                "ordinary-project-source".to_owned(),
                "explicit-reorganisation-after-review".to_owned(),
            ],
        });
    }
    Ok(())
}

fn should_skip_native_directory(name: &str) -> bool {
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

fn role_hints_for(relative: &str) -> Vec<String> {
    let lower = relative.to_ascii_lowercase();
    let mut roles = BTreeSet::new();
    let mut add = |role: &str| {
        roles.insert(role.to_owned());
    };
    if lower.contains("readme") || lower.contains("overview") {
        add("project-overview");
        add("purpose");
    }
    if lower.contains("vision") {
        add("vision");
        add("purpose");
    }
    if lower.contains("intent") || lower.contains("purpose") || lower.contains("why") {
        add("intent");
        add("purpose");
    }
    if lower.contains("position") || lower.contains("principle") || lower.contains("founding") {
        add("positions");
    }
    if lower.contains("experience") || lower.contains("ux") || lower.contains("interaction") {
        add("desired-experience");
        add("interaction-direction");
    }
    if lower.contains("visual") || lower.contains("ui") {
        add("visual-direction");
    }
    if lower.contains("design") {
        add("design");
    }
    if lower.contains("mockup") || lower.contains("wireframe") || lower.contains("prototype") {
        add("mockup-prototype");
    }
    if lower.ends_with(".html") || lower.ends_with(".htm") {
        add("html-prototype-or-presentation");
    }
    if lower.contains("plan") || lower.contains("roadmap") {
        add("plans");
    }
    if lower.contains("research") || lower.contains("framing") {
        add("research-framing");
    }
    if lower.contains("architecture") || lower.contains("adr") {
        add("architecture");
    }
    roles.into_iter().collect()
}

fn normalize_roles(roles: Vec<String>) -> Vec<String> {
    let mut values = roles
        .into_iter()
        .map(|role| role.trim().to_owned())
        .filter(|role| !role.is_empty())
        .collect::<Vec<_>>();
    values.sort();
    values.dedup();
    values
}

fn parse_roles(raw: Option<&str>) -> Vec<String> {
    raw.map(|raw| raw.split(',').map(str::to_owned).collect())
        .map(normalize_roles)
        .unwrap_or_default()
}

fn source_ref(project_id: &str, path: &str) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in path.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("central:project-source:{project_id}:{hash:016x}")
}

fn read_wiki_space_ref(path: &Path) -> io::Result<Option<String>> {
    if !path.is_file() {
        return Ok(None);
    }
    let value: Value = serde_json::from_slice(&fs::read(path)?)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    Ok(value
        .get("objects")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find_map(|object| {
            if object.get("profile").and_then(Value::as_str) == Some(WIKI_PROFILE)
                && object.get("object").and_then(Value::as_str) == Some("space")
            {
                object
                    .get("ref")
                    .and_then(Value::as_str)
                    .filter(|value| !value.trim().is_empty())
                    .map(str::to_owned)
            } else {
                None
            }
        }))
}

fn ground_next_actions(
    projectcentral_ready: bool,
    status: GroundStatus,
    has_native_candidates: bool,
) -> Vec<String> {
    let mut actions = Vec::new();
    if !projectcentral_ready {
        actions.push("Initialize ProjectCentral; initialization will not generate a human document.".to_owned());
        return actions;
    }
    match status {
        GroundStatus::Empty => actions.push(
            "Optionally author a natural source under ProjectCentral/user, or review a native Project source for an accepted relation; no template is required.".to_owned(),
        ),
        GroundStatus::Partial => actions.push(
            "Review unresolved/draft source provenance once; ordinary edits to an already recognised human source do not need per-keystroke approval.".to_owned(),
        ),
        GroundStatus::Established => actions.push(
            "No additional document is required; revise authored ground only when human judgement or returned reality warrants it.".to_owned(),
        ),
    }
    if has_native_candidates {
        actions.push("Review native candidates conservatively; retain useful source in place by default rather than moving everything into ProjectCentral.".to_owned());
    }
    actions
}

fn ensure_project_directory(project_root: &Path) -> io::Result<()> {
    if project_root.is_dir() {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("Project root does not exist as a directory: {}", project_root.display()),
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

fn relative_string(project_root: &Path, path: &Path) -> io::Result<String> {
    path.strip_prefix(project_root)
        .map(normalize_path)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "source escaped Project root"))
}

fn normalize_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn action_input(name: &str, required: bool, choices: Option<&[&str]>) -> ActionInputDefinition {
    ActionInputDefinition {
        name: name.to_owned(),
        input_type: "string".to_owned(),
        required,
        choices: choices.map(|values| values.iter().map(|value| (*value).to_owned()).collect()),
        selection: None,
    }
}

fn descriptor(
    id: &str,
    title: &str,
    description: &str,
    mutation_class: MutationClass,
    output_type: &str,
    inputs: Vec<ActionInputDefinition>,
    preview_supported: bool,
) -> ActionDescriptor {
    ActionDescriptor {
        id: id.to_owned(),
        title: title.to_owned(),
        description: description.to_owned(),
        inputs,
        output: ActionOutputDefinition {
            output_type: output_type.to_owned(),
        },
        mutation_class,
        preview_supported,
        required_ports: vec![],
        availability: ActionAvailability {
            available: true,
            reason: None,
        },
    }
}

fn required(input: &Value, field: &str, action: &str) -> Result<String, ActionResult> {
    input
        .get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| {
            ActionResult::failure(
                Some(action),
                ResultStatus::InvalidInput,
                format!("{action} requires {field}."),
                None,
            )
        })
}

fn project_context(
    action: &str,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> Result<PathBuf, ActionResult> {
    let project = required(input, "project", action)?;
    ensure_project_member(&project).map_err(|error| {
        ActionResult::failure(Some(action), ResultStatus::InvalidInput, error.to_string(), None)
    })?;
    let root = resolve_central_root(context.root_options).map_err(|message| {
        ActionResult::failure(Some(action), ResultStatus::InvalidInput, message, None)
    })?;
    let project_root = root.path.join("Work").join(project);
    ensure_project_directory(&project_root).map_err(|error| {
        ActionResult::failure(Some(action), ResultStatus::InvalidInput, error.to_string(), None)
    })?;
    Ok(project_root)
}

fn io_failure(action: &str, error: io::Error) -> ActionResult {
    let status = match error.kind() {
        io::ErrorKind::InvalidInput | io::ErrorKind::NotFound | io::ErrorKind::AlreadyExists => {
            ResultStatus::InvalidInput
        }
        io::ErrorKind::InvalidData => ResultStatus::VerificationFailure,
        _ => ResultStatus::InternalFailure,
    };
    ActionResult::failure(Some(action), status, error.to_string(), None)
}

fn inspect_action(
    _: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let action = "projectcentral.ground.inspect";
    let project_root = match project_context(action, input, context) {
        Ok(value) => value,
        Err(result) => return result,
    };
    inspect_project_ground(&project_root)
        .map(|value| {
            ActionResult::success(
                action,
                serde_json::to_value(value).expect("ground inspection serializes"),
            )
        })
        .unwrap_or_else(|error| io_failure(action, error))
}

fn plan_action(
    _: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let action = "projectcentral.ground.plan";
    let project_root = match project_context(action, input, context) {
        Ok(value) => value,
        Err(result) => return result,
    };
    plan_project_ground(&project_root)
        .map(|value| {
            ActionResult::success(
                action,
                serde_json::to_value(value).expect("ground plan serializes"),
            )
        })
        .unwrap_or_else(|error| io_failure(action, error))
}

fn apply_action(
    _: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let action = "projectcentral.ground.apply";
    let project_root = match project_context(action, input, context) {
        Ok(value) => value,
        Err(result) => return result,
    };
    let source = match required(input, "source", action) {
        Ok(value) => value,
        Err(result) => return result,
    };
    let provenance = match required(input, "provenance", action)
        .and_then(|value| SourceProvenance::from_input(&value).map_err(|error| {
            ActionResult::failure(Some(action), ResultStatus::InvalidInput, error.to_string(), None)
        })) {
        Ok(value) => value,
        Err(result) => return result,
    };
    let standing = match required(input, "standing", action)
        .and_then(|value| SourceStanding::from_input(&value).map_err(|error| {
            ActionResult::failure(Some(action), ResultStatus::InvalidInput, error.to_string(), None)
        })) {
        Ok(value) => value,
        Err(result) => return result,
    };
    let treatment = match required(input, "treatment", action)
        .and_then(|value| SourceTreatment::from_input(&value).map_err(|error| {
            ActionResult::failure(Some(action), ResultStatus::InvalidInput, error.to_string(), None)
        })) {
        Ok(value) => value,
        Err(result) => return result,
    };
    let acceptance = match required(input, "acceptance", action) {
        Ok(value) => value,
        Err(result) => return result,
    };
    if acceptance != "human-accepted" {
        return ActionResult::failure(
            Some(action),
            ResultStatus::InvalidInput,
            "projectcentral.ground.apply requires explicit human-accepted source-relation recognition; Agent suggestion alone is not authorship.",
            None,
        );
    }
    let roles = parse_roles(input.get("roles").and_then(Value::as_str));
    apply_accepted_ground_relation(
        &project_root,
        &source,
        provenance,
        standing,
        treatment,
        roles,
    )
    .map(|value| {
        ActionResult::success(
            action,
            serde_json::to_value(value).expect("ground apply result serializes"),
        )
    })
    .unwrap_or_else(|error| io_failure(action, error))
}

pub fn register_projectcentral_ground_actions(registry: &mut ActionRegistry) {
    let project = || action_input("project", true, None);
    registry
        .register(
            descriptor(
                "projectcentral.ground.inspect",
                "Inspect authored Project ground",
                "Read ProjectCentral authored-ground state, accepted source relations, unresolved native candidates, account handoff refs and return policy without inferring human authorship from path alone.",
                MutationClass::ReadOnly,
                "projectcentral-ground-inspection",
                vec![project()],
                false,
            ),
            inspect_action,
        )
        .expect("ProjectCentral ground Action ids are valid");
    registry
        .register(
            descriptor(
                "projectcentral.ground.plan",
                "Plan authored Project ground",
                "Describe optional authored-ground establishment and conservative native-source treatments without moving, copying or generating Project documents.",
                MutationClass::ReadOnly,
                "projectcentral-ground-plan",
                vec![project()],
                false,
            ),
            plan_action,
        )
        .expect("ProjectCentral ground Action ids are valid");
    registry
        .register(
            descriptor(
                "projectcentral.ground.apply",
                "Apply accepted Project ground relation",
                "Record one explicitly human-accepted source/provenance/standing/treatment relation while preserving the source bytes and path. This Action does not generate authored prose or move source.",
                MutationClass::LocallyMutating,
                "projectcentral-ground-apply",
                vec![
                    project(),
                    action_input("source", true, None),
                    action_input(
                        "provenance",
                        true,
                        Some(&[
                            "human-authored",
                            "human-edited-draft",
                            "human-adopted",
                            "generated-suggestion",
                            "generated-derived",
                            "agent-maintained",
                            "observed",
                            "inference",
                            "unresolved",
                        ]),
                    ),
                    action_input(
                        "standing",
                        true,
                        Some(&[
                            "unspecified",
                            "authored-human-position",
                            "design-commitment",
                            "architecture-contract",
                            "implementation-fact",
                            "observed-evidence",
                            "current-development-state",
                            "agent-inference",
                        ]),
                    ),
                    action_input(
                        "treatment",
                        true,
                        Some(&[
                            "projectcentral-user",
                            "retain-native-in-place",
                            "ordinary-project-source",
                            "generated-derived",
                            "unresolved",
                        ]),
                    ),
                    action_input("roles", false, None),
                    action_input("acceptance", true, Some(&["human-accepted"])),
                ],
                true,
            ),
            apply_action,
        )
        .expect("ProjectCentral ground Action ids are valid");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::projectcentral::{HUMAN_SOURCE_DIR, WIKI_SOURCE};
    use crate::projectcentral_ops::initialize_projectcentral;
    use tempfile::tempdir;

    #[test]
    fn new_project_starts_empty_and_one_recognised_note_establishes_ground() {
        let temp = tempdir().unwrap();
        let central = temp.path().join("Central");
        let project = central.join("Work/small");
        fs::create_dir_all(&project).unwrap();
        initialize_projectcentral(&central, &project, "example/small").unwrap();

        let empty = inspect_project_ground(&project).unwrap();
        assert_eq!(empty.status, GroundStatus::Empty);
        assert!(empty.recognised_sources.is_empty());
        assert!(project.join(WIKI_SOURCE).is_file());
        assert_eq!(fs::read_dir(project.join(HUMAN_SOURCE_DIR)).unwrap().count(), 0);

        fs::write(project.join(HUMAN_SOURCE_DIR).join("note.md"), "What this small project is for.\n").unwrap();
        let unresolved = inspect_project_ground(&project).unwrap();
        assert_eq!(unresolved.status, GroundStatus::Partial);
        assert_eq!(unresolved.recognised_sources[0].provenance, SourceProvenance::Unresolved);

        apply_accepted_ground_relation(
            &project,
            "ProjectCentral/user/note.md",
            SourceProvenance::HumanAuthored,
            SourceStanding::AuthoredHumanPosition,
            SourceTreatment::ProjectcentralUser,
            vec!["purpose".to_owned()],
        )
        .unwrap();
        let established = inspect_project_ground(&project).unwrap();
        assert_eq!(established.status, GroundStatus::Established);
        assert_eq!(established.account_handoff.recognised_human_sources.len(), 1);
    }

    #[test]
    fn native_role_signals_do_not_claim_human_authorship_and_can_be_retained_in_place() {
        let temp = tempdir().unwrap();
        let central = temp.path().join("Central");
        let project = central.join("Work/deep");
        fs::create_dir_all(project.join("docs/design")).unwrap();
        fs::write(project.join("README.md"), "native overview\n").unwrap();
        fs::write(project.join("docs/VISION.md"), "native vision\n").unwrap();
        fs::write(project.join("docs/design/interaction.md"), "native interaction\n").unwrap();
        initialize_projectcentral(&central, &project, "example/deep").unwrap();

        let before = inspect_project_ground(&project).unwrap();
        assert_eq!(before.status, GroundStatus::Empty);
        let vision = before
            .native_candidates
            .iter()
            .find(|candidate| candidate.path == "docs/VISION.md")
            .unwrap();
        assert_eq!(vision.authorship, "unresolved");
        assert_eq!(vision.standing, "unresolved");

        apply_accepted_ground_relation(
            &project,
            "docs/VISION.md",
            SourceProvenance::HumanAuthored,
            SourceStanding::AuthoredHumanPosition,
            SourceTreatment::RetainNativeInPlace,
            vec!["vision".to_owned(), "purpose".to_owned()],
        )
        .unwrap();
        assert_eq!(fs::read_to_string(project.join("docs/VISION.md")).unwrap(), "native vision\n");
        let after = inspect_project_ground(&project).unwrap();
        assert_eq!(after.status, GroundStatus::Established);
        assert!(after
            .recognised_sources
            .iter()
            .any(|source| source.path == "docs/VISION.md" && source.provenance == SourceProvenance::HumanAuthored));
    }

    #[test]
    fn generated_suggestion_in_user_does_not_become_human_authored_by_location() {
        let temp = tempdir().unwrap();
        let central = temp.path().join("Central");
        let project = central.join("Work/assisted");
        fs::create_dir_all(&project).unwrap();
        initialize_projectcentral(&central, &project, "example/assisted").unwrap();
        fs::write(project.join(HUMAN_SOURCE_DIR).join("suggested-vision.md"), "agent suggestion\n").unwrap();

        let unresolved = inspect_project_ground(&project).unwrap();
        assert_eq!(unresolved.status, GroundStatus::Partial);
        assert_eq!(unresolved.recognised_sources[0].provenance, SourceProvenance::Unresolved);

        apply_accepted_ground_relation(
            &project,
            "ProjectCentral/user/suggested-vision.md",
            SourceProvenance::GeneratedSuggestion,
            SourceStanding::Unspecified,
            SourceTreatment::ProjectcentralUser,
            vec!["vision".to_owned()],
        )
        .unwrap();
        assert_eq!(inspect_project_ground(&project).unwrap().status, GroundStatus::Partial);

        apply_accepted_ground_relation(
            &project,
            "ProjectCentral/user/suggested-vision.md",
            SourceProvenance::HumanAdopted,
            SourceStanding::AuthoredHumanPosition,
            SourceTreatment::ProjectcentralUser,
            vec!["vision".to_owned()],
        )
        .unwrap();
        assert_eq!(inspect_project_ground(&project).unwrap().status, GroundStatus::Established);
    }

    #[test]
    fn account_handoff_and_return_policy_preserve_authority_without_source_mutation() {
        let temp = tempdir().unwrap();
        let central = temp.path().join("Central");
        let project = central.join("Work/account");
        fs::create_dir_all(project.join("src")).unwrap();
        initialize_projectcentral(&central, &project, "example/account").unwrap();
        let authored = project.join(HUMAN_SOURCE_DIR).join("purpose.md");
        fs::write(&authored, "The intended experience stays human-authored.\n").unwrap();
        fs::write(project.join("src/lib.rs"), "pub fn current() {}\n").unwrap();
        apply_accepted_ground_relation(
            &project,
            "ProjectCentral/user/purpose.md",
            SourceProvenance::HumanAuthored,
            SourceStanding::AuthoredHumanPosition,
            SourceTreatment::ProjectcentralUser,
            vec!["purpose".to_owned(), "desired-experience".to_owned()],
        )
        .unwrap();
        apply_accepted_ground_relation(
            &project,
            "src/lib.rs",
            SourceProvenance::Observed,
            SourceStanding::ImplementationFact,
            SourceTreatment::OrdinaryProjectSource,
            vec!["implementation".to_owned()],
        )
        .unwrap();

        let inspection = inspect_project_ground(&project).unwrap();
        assert_eq!(inspection.account_handoff.recognised_human_sources.len(), 1);
        assert_eq!(inspection.account_handoff.other_source_relations.len(), 1);
        assert!(inspection.account_handoff.agent_wiki_source.is_some());
        assert!(inspection.account_handoff.agent_wiki_space_ref.is_some());
        assert!(!inspection.account_handoff.account_is_source);
        assert!(!inspection.account_handoff.html_is_source);
        assert!(!inspection.account_handoff.projection_is_source);
        assert!(!inspection.return_policy.difference_automatically_mutates_human_source);
        assert!(inspection.return_policy.agent_wiki_may_be_maintained_independently);
        assert_eq!(
            fs::read_to_string(authored).unwrap(),
            "The intended experience stays human-authored.\n"
        );
    }

    #[test]
    fn ground_plan_never_treats_reorganisation_as_adoption_default() {
        let temp = tempdir().unwrap();
        let central = temp.path().join("Central");
        let project = central.join("Work/plan");
        fs::create_dir_all(project.join("docs")).unwrap();
        fs::write(project.join("docs/vision.md"), "vision\n").unwrap();
        initialize_projectcentral(&central, &project, "example/plan").unwrap();
        let plan = plan_project_ground(&project).unwrap();
        let source = plan
            .proposals
            .iter()
            .find(|item| item.source.as_deref() == Some("docs/vision.md"))
            .unwrap();
        assert!(source.changes.iter().any(|change| change.contains("source bytes and source path remain unchanged")));
        assert!(source.changes.iter().any(|change| change.contains("separate reviewed source mutation")));
    }
}
