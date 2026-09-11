//! Central-owned source relations for the O:I Development Field.
//!
//! This module deliberately does not implement Wiki cognition, Factory runs, Git
//! execution, Workcell actuality, or O:I suite selection. It binds those worlds
//! back to Central's existing source identity/revision/provenance law.

use crate::action::{
    ActionAvailability, ActionDescriptor, ActionExecutionContext, ActionInputDefinition,
    ActionOutputDefinition, ActionRegistry, MutationClass,
};
use crate::machine::read_machine_declaration;
use crate::projectcentral::read_project_manifest;
use crate::projectcentral_flow::{reject_symlink_components, relative_member};
use crate::projectcentral_ground::{
    apply_accepted_ground_relation, SourceProvenance, SourceStanding, SourceTreatment,
};
use crate::result::{ActionResult, ResultStatus};
use crate::root::resolve_central_root;
use crate::source_horizon::{
    content_revision, control_source_bindings, project_source_bindings, source_ref, SourceBinding,
    SourceRevision, CONTROL_GROUND_RELATIONS_SCHEMA, CONTROL_GROUND_RELATIONS_SOURCE,
    CONTROL_WORLD_REF,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub const ROOT_SELF_DIR: &str = "Control/self";
pub const PROJECT_SELF_DIR: &str = "ProjectCentral/self";
pub const ROOT_DEVELOPMENT_RELATIONS: &str = "Control/relations/development-field.json";
pub const PROJECT_DEVELOPMENT_RELATIONS: &str = "ProjectCentral/relations/development-field.json";
pub const DEVELOPMENT_RELATIONS_SCHEMA: &str = "central.development-field-sources/v1";
pub const DEVELOPMENT_READING_SCHEMA: &str = "central.development-field-reading/v1";
pub const OI_SUITE_POLICY_BINDING_KIND: &str = "oi-suite-policy";

const MAX_SELF_DEPTH: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum SelfApertureStatus {
    Present,
    LegacyMigratableAbsence,
    InvalidBrokenSourceState,
    AmbiguousSourceRelation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TierBinding {
    pub tier: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub canonical_label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub canonical_path: Option<String>,
    #[serde(default)]
    pub source_refs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UxBinding {
    pub ux_ref: String,
    pub source_ref: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExBinding {
    pub ex_ref: String,
    pub source_ref: String,
    #[serde(default)]
    pub ux_refs: Vec<String>,
    #[serde(default)]
    pub artifact_refs: Vec<String>,
    pub recorded_at_unix_seconds: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DevelopmentSourceRelations {
    pub schema: String,
    pub scope_ref: String,
    #[serde(default)]
    pub tiers: Vec<TierBinding>,
    #[serde(default)]
    pub ux: Vec<UxBinding>,
    #[serde(default)]
    pub ex: Vec<ExBinding>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ResolvedDevelopmentSource {
    #[serde(rename = "ref")]
    pub source_ref: String,
    pub path: String,
    pub roles: Vec<String>,
    pub provenance: String,
    pub standing: String,
    pub treatment: String,
    pub revision: SourceRevision,
    pub agent_retrieval_allowed: bool,
    pub retained_native: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UnboundSelfSource {
    #[serde(rename = "ref")]
    pub source_ref: String,
    pub path: String,
    pub revision: SourceRevision,
    pub standing: String,
    pub provenance: String,
    pub authority_from_location: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TierReading {
    pub tier: u8,
    pub semantic_office: String,
    pub canonical_label: Option<String>,
    pub canonical_path: Option<String>,
    pub sources: Vec<Option<ResolvedDevelopmentSource>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UxReading {
    pub ux_ref: String,
    pub source: Option<ResolvedDevelopmentSource>,
    pub intended_experience: bool,
    pub implementation_fact: bool,
    pub test_result: bool,
    pub agent_inference: bool,
    pub ex_human_return: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExReading {
    pub ex_ref: String,
    pub ux_refs: Vec<String>,
    pub source: Option<ResolvedDevelopmentSource>,
    pub artifact_refs: Vec<String>,
    pub recorded_at_unix_seconds: u64,
    pub human_experience_return: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SelfApertureReading {
    pub path: String,
    pub status: SelfApertureStatus,
    pub exists: bool,
    pub issues: Vec<String>,
    pub linked_sources: Vec<ResolvedDevelopmentSource>,
    pub unbound_sources: Vec<UnboundSelfSource>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DevelopmentFieldReading {
    pub schema: String,
    pub scope_ref: String,
    pub self_aperture: SelfApertureReading,
    pub tier_bindings: Vec<TierReading>,
    pub ux: Vec<UxReading>,
    pub ex: Vec<ExReading>,
    pub relation_source: String,
    pub canonical_tier_labels_invented: bool,
    pub source_payloads_exposed: bool,
    pub automatic_agent_or_model_invocation: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SelfEnsureReceipt {
    pub scope_ref: String,
    pub path: String,
    pub previous_status: SelfApertureStatus,
    pub current_status: SelfApertureStatus,
    pub documentation_moved: bool,
    pub matrices_moved: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MachineOiSuitePolicyReading {
    pub role: String,
    pub binding_kind: String,
    pub state: String,
    pub desired_policy_refs: Vec<String>,
    pub authored_source: PathBuf,
    pub installed_suite_receipt_owned_by_central: bool,
    pub material_executable_observation_owned_by_central: bool,
}

fn unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn semantic_office(tier: u8) -> io::Result<&'static str> {
    match tier {
        0 => Ok("originating authored ground / why / positions / intent"),
        1 => Ok("intended experience / vision"),
        2 => Ok("design / capabilities / functional form"),
        3 => Ok("architecture / contracts / owned relations"),
        4 => Ok("implementation / active development / operational form"),
        5 => Ok("evidence / returned reality / Recognition pressure"),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "document-stage tier must be one of the stable identities 0..5",
        )),
    }
}

fn normalise_refs(values: Vec<String>) -> Vec<String> {
    let mut values = values
        .into_iter()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    values.sort();
    values.dedup();
    values
}

fn validate_relations(value: &DevelopmentSourceRelations, scope_ref: &str) -> io::Result<()> {
    if value.schema != DEVELOPMENT_RELATIONS_SCHEMA {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("development-field relation schema must be {DEVELOPMENT_RELATIONS_SCHEMA}"),
        ));
    }
    if value.scope_ref != scope_ref {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "development-field relation scope_ref does not match the scoped World",
        ));
    }
    let mut tiers = BTreeSet::new();
    for tier in &value.tiers {
        semantic_office(tier.tier)?;
        if !tiers.insert(tier.tier) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("development-field tier {} occurs more than once", tier.tier),
            ));
        }
        if tier
            .source_refs
            .iter()
            .any(|reference| reference.trim().is_empty())
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "tier source refs must be non-empty",
            ));
        }
    }
    let mut ux = BTreeSet::new();
    for binding in &value.ux {
        if binding.ux_ref.trim().is_empty()
            || binding.source_ref.trim().is_empty()
            || !ux.insert(&binding.ux_ref)
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "UX relations require unique non-empty ux_ref and source_ref values",
            ));
        }
    }
    let mut ex = BTreeSet::new();
    for binding in &value.ex {
        if binding.ex_ref.trim().is_empty()
            || binding.source_ref.trim().is_empty()
            || !ex.insert(&binding.ex_ref)
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "EX relations require unique non-empty ex_ref and source_ref values",
            ));
        }
    }
    Ok(())
}

fn empty_relations(scope_ref: &str) -> DevelopmentSourceRelations {
    DevelopmentSourceRelations {
        schema: DEVELOPMENT_RELATIONS_SCHEMA.to_owned(),
        scope_ref: scope_ref.to_owned(),
        tiers: Vec::new(),
        ux: Vec::new(),
        ex: Vec::new(),
    }
}

fn read_relations(path: &Path, scope_ref: &str) -> io::Result<DevelopmentSourceRelations> {
    if !path.is_file() {
        return Ok(empty_relations(scope_ref));
    }
    let value: DevelopmentSourceRelations =
        serde_json::from_slice(&fs::read(path)?).map_err(|error| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "{} is not a valid Development Field source relation file: {error}",
                    path.display()
                ),
            )
        })?;
    validate_relations(&value, scope_ref)?;
    Ok(value)
}

fn write_relations(path: &Path, value: &DevelopmentSourceRelations) -> io::Result<()> {
    validate_relations(value, &value.scope_ref)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut bytes = serde_json::to_vec_pretty(value).map_err(io::Error::other)?;
    bytes.push(b'\n');
    crate::file_mutation::atomic_record(path, &bytes)
}

fn safe_member(raw: &str) -> io::Result<PathBuf> {
    if raw.trim().is_empty() || raw != raw.trim() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "path must be a non-empty relative member",
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
            "path must not escape its self-description aperture",
        ));
    }
    Ok(path.to_path_buf())
}

fn collect_self_files(
    current: &Path,
    world_root: &Path,
    depth: usize,
    output: &mut Vec<PathBuf>,
) -> io::Result<()> {
    if depth > MAX_SELF_DEPTH || !current.is_dir() {
        return Ok(());
    }
    let mut entries = fs::read_dir(current)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let path = entry.path();
        let file_type = entry.file_type()?;
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            collect_self_files(&path, world_root, depth + 1, output)?;
        } else if file_type.is_file() {
            let canonical_root = fs::canonicalize(world_root)?;
            let canonical_file = fs::canonicalize(&path)?;
            if canonical_file.starts_with(canonical_root) {
                output.push(path);
            }
        }
    }
    Ok(())
}

fn resolve_source(
    world_root: &Path,
    self_prefix: &str,
    bindings: &BTreeMap<String, SourceBinding>,
    source_reference: &str,
) -> io::Result<Option<ResolvedDevelopmentSource>> {
    let Some(binding) = bindings.get(source_reference) else {
        return Ok(None);
    };
    let revision = content_revision(&world_root.join(&binding.path))?;
    let self_member =
        binding.path == self_prefix || binding.path.starts_with(&format!("{self_prefix}/"));
    Ok(Some(ResolvedDevelopmentSource {
        source_ref: binding.source_ref.clone(),
        path: binding.path.clone(),
        roles: binding.roles.clone(),
        provenance: binding.provenance.clone(),
        standing: binding.standing.clone(),
        treatment: binding.treatment.clone(),
        revision,
        agent_retrieval_allowed: binding.agent_retrieval_allowed,
        retained_native: binding.treatment == "retain-native-in-place" || !self_member,
    }))
}

fn inspect_scope(
    world_root: &Path,
    scope_ref: &str,
    self_prefix: &str,
    relation_source: &str,
    source_bindings: io::Result<Vec<SourceBinding>>,
) -> io::Result<DevelopmentFieldReading> {
    let self_path = world_root.join(self_prefix);
    let relation_path = world_root.join(relation_source);
    let metadata = fs::symlink_metadata(&self_path);
    let mut issues = Vec::new();
    let mut status = match metadata {
        Err(error) if error.kind() == io::ErrorKind::NotFound && !relation_path.exists() => {
            SelfApertureStatus::LegacyMigratableAbsence
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            issues.push(
                "Development Field relations exist while the self-description aperture is absent."
                    .to_owned(),
            );
            SelfApertureStatus::InvalidBrokenSourceState
        }
        Err(error) => return Err(error),
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
            issues.push(
                "self-description aperture must be an ordinary directory, not a file or symlink"
                    .to_owned(),
            );
            SelfApertureStatus::InvalidBrokenSourceState
        }
        Ok(_) => SelfApertureStatus::Present,
    };

    let relations = match read_relations(&relation_path, scope_ref) {
        Ok(value) => value,
        Err(error) => {
            issues.push(error.to_string());
            status = SelfApertureStatus::InvalidBrokenSourceState;
            empty_relations(scope_ref)
        }
    };
    let bindings = source_bindings?
        .into_iter()
        .map(|binding| (binding.source_ref.clone(), binding))
        .collect::<BTreeMap<_, _>>();

    let relation_refs = relations
        .tiers
        .iter()
        .flat_map(|tier| tier.source_refs.iter())
        .chain(relations.ux.iter().map(|binding| &binding.source_ref))
        .chain(relations.ex.iter().map(|binding| &binding.source_ref))
        .cloned()
        .collect::<BTreeSet<_>>();
    let missing = relation_refs
        .iter()
        .filter(|reference| !bindings.contains_key(*reference))
        .cloned()
        .collect::<Vec<_>>();
    if !missing.is_empty() && status != SelfApertureStatus::InvalidBrokenSourceState {
        status = SelfApertureStatus::AmbiguousSourceRelation;
        issues.push(format!(
            "Development Field relations reference sources that are no longer resolvable: {}",
            missing.join(", ")
        ));
    }

    let mut tier_bindings = Vec::new();
    for tier in &relations.tiers {
        let mut sources = Vec::new();
        for reference in &tier.source_refs {
            sources.push(resolve_source(
                world_root,
                self_prefix,
                &bindings,
                reference,
            )?);
        }
        tier_bindings.push(TierReading {
            tier: tier.tier,
            semantic_office: semantic_office(tier.tier)?.to_owned(),
            canonical_label: tier.canonical_label.clone(),
            canonical_path: tier.canonical_path.clone(),
            sources,
        });
    }
    tier_bindings.sort_by_key(|tier| tier.tier);

    let mut ux = Vec::new();
    for binding in &relations.ux {
        ux.push(UxReading {
            ux_ref: binding.ux_ref.clone(),
            source: resolve_source(world_root, self_prefix, &bindings, &binding.source_ref)?,
            intended_experience: true,
            implementation_fact: false,
            test_result: false,
            agent_inference: false,
            ex_human_return: false,
        });
    }
    let mut ex = Vec::new();
    for binding in &relations.ex {
        ex.push(ExReading {
            ex_ref: binding.ex_ref.clone(),
            ux_refs: binding.ux_refs.clone(),
            source: resolve_source(world_root, self_prefix, &bindings, &binding.source_ref)?,
            artifact_refs: binding.artifact_refs.clone(),
            recorded_at_unix_seconds: binding.recorded_at_unix_seconds,
            human_experience_return: true,
        });
    }

    let mut linked_sources = Vec::new();
    let mut linked_paths = BTreeSet::new();
    for reference in relation_refs {
        if let Some(source) = resolve_source(world_root, self_prefix, &bindings, &reference)? {
            linked_paths.insert(source.path.clone());
            linked_sources.push(source);
        }
    }
    for binding in bindings.values() {
        if binding.path == self_prefix || binding.path.starts_with(&format!("{self_prefix}/")) {
            if linked_paths.insert(binding.path.clone()) {
                if let Some(source) =
                    resolve_source(world_root, self_prefix, &bindings, &binding.source_ref)?
                {
                    linked_sources.push(source);
                }
            }
        }
    }
    linked_sources.sort_by(|a, b| a.path.cmp(&b.path));

    let mut unbound_sources = Vec::new();
    if self_path.is_dir() {
        let mut files = Vec::new();
        collect_self_files(&self_path, world_root, 0, &mut files)?;
        for file in files {
            let relative = file
                .strip_prefix(world_root)
                .map_err(|_| {
                    io::Error::new(io::ErrorKind::InvalidData, "self source escaped its world")
                })?
                .to_string_lossy()
                .replace('\\', "/");
            if linked_paths.contains(&relative) {
                continue;
            }
            unbound_sources.push(UnboundSelfSource {
                source_ref: source_ref(scope_ref, &relative),
                path: relative,
                revision: content_revision(&file)?,
                standing: "unresolved".to_owned(),
                provenance: "unresolved".to_owned(),
                authority_from_location: false,
            });
        }
    }

    Ok(DevelopmentFieldReading {
        schema: DEVELOPMENT_READING_SCHEMA.to_owned(),
        scope_ref: scope_ref.to_owned(),
        self_aperture: SelfApertureReading {
            path: self_prefix.to_owned(),
            status,
            exists: self_path.is_dir(),
            issues,
            linked_sources,
            unbound_sources,
        },
        tier_bindings,
        ux,
        ex,
        relation_source: relation_source.to_owned(),
        canonical_tier_labels_invented: false,
        source_payloads_exposed: false,
        automatic_agent_or_model_invocation: false,
    })
}

pub fn inspect_root_development_field(central_root: &Path) -> io::Result<DevelopmentFieldReading> {
    inspect_scope(
        central_root,
        CONTROL_WORLD_REF,
        ROOT_SELF_DIR,
        ROOT_DEVELOPMENT_RELATIONS,
        control_source_bindings(central_root),
    )
}

pub fn inspect_project_development_field(
    project_root: &Path,
) -> io::Result<DevelopmentFieldReading> {
    let manifest = read_project_manifest(project_root)?;
    let validation = manifest.validate();
    if !validation.valid {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            validation.errors.join("; "),
        ));
    }
    inspect_scope(
        project_root,
        &format!("project:{}", manifest.project_id),
        PROJECT_SELF_DIR,
        PROJECT_DEVELOPMENT_RELATIONS,
        project_source_bindings(project_root),
    )
}

fn ensure_self(world_root: &Path, self_prefix: &str) -> io::Result<()> {
    let path = world_root.join(self_prefix);
    if let Ok(metadata) = fs::symlink_metadata(&path) {
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "self-description aperture exists but is not an ordinary directory",
            ));
        }
        return Ok(());
    }
    fs::create_dir_all(&path)?;
    reject_symlink_components(world_root, Path::new(self_prefix))
}

pub fn ensure_root_self(central_root: &Path) -> io::Result<SelfEnsureReceipt> {
    let before = inspect_root_development_field(central_root)?
        .self_aperture
        .status;
    ensure_self(central_root, ROOT_SELF_DIR)?;
    let after = inspect_root_development_field(central_root)?
        .self_aperture
        .status;
    Ok(SelfEnsureReceipt {
        scope_ref: CONTROL_WORLD_REF.to_owned(),
        path: ROOT_SELF_DIR.to_owned(),
        previous_status: before,
        current_status: after,
        documentation_moved: false,
        matrices_moved: false,
    })
}

pub fn ensure_project_self(project_root: &Path) -> io::Result<SelfEnsureReceipt> {
    let manifest = read_project_manifest(project_root)?;
    let before = inspect_project_development_field(project_root)?
        .self_aperture
        .status;
    ensure_self(project_root, PROJECT_SELF_DIR)?;
    let after = inspect_project_development_field(project_root)?
        .self_aperture
        .status;
    Ok(SelfEnsureReceipt {
        scope_ref: format!("project:{}", manifest.project_id),
        path: PROJECT_SELF_DIR.to_owned(),
        previous_status: before,
        current_status: after,
        documentation_moved: false,
        matrices_moved: false,
    })
}

fn parse_standing(raw: &str) -> io::Result<SourceStanding> {
    match raw {
        "unspecified" => Ok(SourceStanding::Unspecified),
        "authored-human-position" => Ok(SourceStanding::AuthoredHumanPosition),
        "design-commitment" => Ok(SourceStanding::DesignCommitment),
        "architecture-contract" => Ok(SourceStanding::ArchitectureContract),
        "implementation-fact" => Ok(SourceStanding::ImplementationFact),
        "observed-evidence" => Ok(SourceStanding::ObservedEvidence),
        "current-development-state" => Ok(SourceStanding::CurrentDevelopmentState),
        "agent-inference" => Ok(SourceStanding::AgentInference),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("unsupported source standing: {raw}"),
        )),
    }
}

fn parse_provenance(raw: &str) -> io::Result<SourceProvenance> {
    match raw {
        "human-authored" => Ok(SourceProvenance::HumanAuthored),
        "human-edited-draft" => Ok(SourceProvenance::HumanEditedDraft),
        "human-adopted" => Ok(SourceProvenance::HumanAdopted),
        "generated-suggestion" => Ok(SourceProvenance::GeneratedSuggestion),
        "generated-derived" => Ok(SourceProvenance::GeneratedDerived),
        "agent-maintained" => Ok(SourceProvenance::AgentMaintained),
        "observed" => Ok(SourceProvenance::Observed),
        "inference" => Ok(SourceProvenance::Inference),
        "unresolved" => Ok(SourceProvenance::Unresolved),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("unsupported source provenance: {raw}"),
        )),
    }
}

fn recognised_human(provenance: &str) -> bool {
    matches!(provenance, "human-authored" | "human-adopted")
}

fn bind_tier(
    relations: &mut DevelopmentSourceRelations,
    tier: u8,
    source_reference: &str,
    label: Option<String>,
    path: Option<String>,
) -> io::Result<()> {
    semantic_office(tier)?;
    let entry = if let Some(entry) = relations.tiers.iter_mut().find(|entry| entry.tier == tier) {
        entry
    } else {
        relations.tiers.push(TierBinding {
            tier,
            canonical_label: None,
            canonical_path: None,
            source_refs: Vec::new(),
        });
        relations.tiers.last_mut().expect("tier was just inserted")
    };
    if let Some(label) = label {
        if !label.trim().is_empty() {
            entry.canonical_label = Some(label.trim().to_owned());
        }
    }
    if let Some(path) = path {
        if !path.trim().is_empty() {
            entry.canonical_path = Some(path.trim().to_owned());
        }
    }
    if !entry
        .source_refs
        .iter()
        .any(|reference| reference == source_reference)
    {
        entry.source_refs.push(source_reference.to_owned());
        entry.source_refs.sort();
    }
    relations.tiers.sort_by_key(|entry| entry.tier);
    Ok(())
}

fn relate_ux(
    relations: &mut DevelopmentSourceRelations,
    ux_ref: &str,
    source_reference: &str,
) -> io::Result<()> {
    if ux_ref.trim().is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "ux_ref must be non-empty",
        ));
    }
    if let Some(existing) = relations.ux.iter().find(|binding| binding.ux_ref == ux_ref) {
        if existing.source_ref != source_reference {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "ux_ref is already bound to another source",
            ));
        }
    } else {
        relations.ux.push(UxBinding {
            ux_ref: ux_ref.to_owned(),
            source_ref: source_reference.to_owned(),
        });
        relations.ux.sort_by(|a, b| a.ux_ref.cmp(&b.ux_ref));
    }
    bind_tier(relations, 1, source_reference, None, None)
}

fn relate_ex(
    relations: &mut DevelopmentSourceRelations,
    ex_ref: &str,
    source_reference: &str,
    ux_refs: Vec<String>,
    artifact_refs: Vec<String>,
) -> io::Result<()> {
    if ex_ref.trim().is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "ex_ref must be non-empty",
        ));
    }
    if relations
        .ex
        .iter()
        .any(|binding| binding.ex_ref == ex_ref && binding.source_ref != source_reference)
    {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "ex_ref is already bound to another source",
        ));
    }
    let record = ExBinding {
        ex_ref: ex_ref.to_owned(),
        source_ref: source_reference.to_owned(),
        ux_refs: normalise_refs(ux_refs),
        artifact_refs: normalise_refs(artifact_refs),
        recorded_at_unix_seconds: unix_seconds(),
    };
    if let Some(existing) = relations
        .ex
        .iter_mut()
        .find(|binding| binding.ex_ref == ex_ref)
    {
        *existing = record;
    } else {
        relations.ex.push(record);
        relations.ex.sort_by(|a, b| a.ex_ref.cmp(&b.ex_ref));
    }
    bind_tier(relations, 5, source_reference, None, None)
}

fn write_root_ground_relation(
    central_root: &Path,
    relative: &str,
    provenance: &str,
    standing: &str,
    roles: Vec<String>,
    treatment: &str,
) -> io::Result<String> {
    let relation_path = central_root.join(CONTROL_GROUND_RELATIONS_SOURCE);
    let mut value = if relation_path.is_file() {
        serde_json::from_slice::<Value>(&fs::read(&relation_path)?)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?
    } else {
        json!({
            "schema": CONTROL_GROUND_RELATIONS_SCHEMA,
            "project_id": CONTROL_WORLD_REF,
            "relations": []
        })
    };
    if value.get("schema").and_then(Value::as_str) != Some(CONTROL_GROUND_RELATIONS_SCHEMA)
        || value.get("project_id").and_then(Value::as_str) != Some(CONTROL_WORLD_REF)
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Control ground relations have an unsupported schema or world id",
        ));
    }
    let source_reference = source_ref(CONTROL_WORLD_REF, relative);
    let relations = value
        .as_object_mut()
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "Control ground relation file must be an object",
            )
        })?
        .entry("relations")
        .or_insert_with(|| json!([]))
        .as_array_mut()
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "Control ground relations must be an array",
            )
        })?;
    relations.retain(|relation| relation.get("path").and_then(Value::as_str) != Some(relative));
    relations.push(json!({
        "ref": source_reference,
        "path": relative,
        "provenance": provenance,
        "standing": standing,
        "roles": normalise_refs(roles),
        "treatment": treatment,
        "recognition": "explicit self-description source relation",
        "recorded_at_unix_seconds": unix_seconds()
    }));
    relations.sort_by(|a, b| {
        a.get("path")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .cmp(b.get("path").and_then(Value::as_str).unwrap_or_default())
    });
    if let Some(parent) = relation_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut bytes = serde_json::to_vec_pretty(&value).map_err(io::Error::other)?;
    bytes.push(b'\n');
    crate::file_mutation::atomic_record(&relation_path, &bytes)?;
    Ok(source_reference)
}

fn create_text_source(
    world_root: &Path,
    self_prefix: &str,
    member: &str,
    content: &str,
) -> io::Result<String> {
    if content.len() > crate::source_safety::MAX_SOURCE || content.contains('\0') {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "self-description source exceeds bounded UTF-8 text constraints",
        ));
    }
    ensure_self(world_root, self_prefix)?;
    let member = safe_member(member)?;
    let relative = Path::new(self_prefix).join(member);
    reject_symlink_components(world_root, &relative)?;
    let path = world_root.join(&relative);
    if path.exists() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!(
                "self-description source already exists: {}",
                relative.display()
            ),
        ));
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    reject_symlink_components(
        world_root,
        relative.parent().unwrap_or(Path::new(self_prefix)),
    )?;
    crate::file_mutation::atomic_record(&path, content.as_bytes())?;
    Ok(relative.to_string_lossy().replace('\\', "/"))
}

fn source_create_authority(
    provenance: &str,
    actor_kind: &str,
    agent_session_ref: Option<&str>,
    acceptance: Option<&str>,
) -> io::Result<()> {
    match provenance {
        "human-authored" | "human-adopted" => {
            if actor_kind != "human"
                || agent_session_ref.is_some()
                || acceptance != Some("human-accepted")
            {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "human-authored/adopted self source requires a declared human caller, no agent_session_ref, and explicit human-accepted source recognition",
                ));
            }
        }
        "generated-suggestion"
        | "generated-derived"
        | "agent-maintained"
        | "observed"
        | "inference"
        | "unresolved"
        | "human-edited-draft" => {
            if actor_kind == "human" && agent_session_ref.is_some() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "a declared human caller cannot also carry an agent_session_ref",
                ));
            }
        }
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "unsupported source provenance",
            ))
        }
    }
    Ok(())
}

pub fn read_machine_oi_suite_policy(
    central_root: &Path,
    role: &str,
) -> Result<MachineOiSuitePolicyReading, crate::machine::MachineDeclarationError> {
    let authored = read_machine_declaration(central_root, role)?;
    let refs = authored
        .declaration
        .bindings
        .iter()
        .filter(|binding| binding.kind == OI_SUITE_POLICY_BINDING_KIND)
        .map(|binding| binding.reference.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let state = match refs.len() {
        0 => "absent",
        1 => "authored-intent",
        _ => "ambiguous-human-decision-required",
    };
    Ok(MachineOiSuitePolicyReading {
        role: role.to_owned(),
        binding_kind: OI_SUITE_POLICY_BINDING_KIND.to_owned(),
        state: state.to_owned(),
        desired_policy_refs: refs,
        authored_source: authored.source.path,
        installed_suite_receipt_owned_by_central: false,
        material_executable_observation_owned_by_central: false,
    })
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

fn optional(input: &Value, field: &str) -> Option<String> {
    input
        .get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn string_array(input: &Value, field: &str, action: &str) -> Result<Vec<String>, ActionResult> {
    let Some(raw) = input.get(field) else {
        return Ok(Vec::new());
    };
    let Some(values) = raw.as_array() else {
        return Err(ActionResult::failure(
            Some(action),
            ResultStatus::InvalidInput,
            format!("{action} {field} must be an array of refs."),
            None,
        ));
    };
    let mut refs = Vec::new();
    for value in values {
        let Some(value) = value
            .as_str()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            return Err(ActionResult::failure(
                Some(action),
                ResultStatus::InvalidInput,
                format!("{action} {field} must contain only non-empty strings."),
                None,
            ));
        };
        refs.push(value.to_owned());
    }
    Ok(normalise_refs(refs))
}

fn tier_input(input: &Value, action: &str) -> Result<u8, ActionResult> {
    let tier = input
        .get("tier")
        .and_then(Value::as_u64)
        .filter(|tier| *tier <= 5)
        .ok_or_else(|| {
            ActionResult::failure(
                Some(action),
                ResultStatus::InvalidInput,
                format!("{action} requires integer tier 0..5."),
                None,
            )
        })?;
    Ok(tier as u8)
}

fn root_context(
    action: &str,
    context: &ActionExecutionContext<'_>,
) -> Result<PathBuf, ActionResult> {
    resolve_central_root(context.root_options)
        .map(|root| root.path)
        .map_err(|message| {
            ActionResult::failure(Some(action), ResultStatus::InvalidInput, message, None)
        })
}

fn project_context(
    action: &str,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> Result<PathBuf, ActionResult> {
    let project = required(input, "project", action)?;
    let member = relative_member(&project).map_err(|error| {
        ActionResult::failure(
            Some(action),
            ResultStatus::InvalidInput,
            error.to_string(),
            None,
        )
    })?;
    let root = root_context(action, context)?;
    reject_symlink_components(&root, &Path::new("Work").join(&member)).map_err(|error| {
        ActionResult::failure(
            Some(action),
            ResultStatus::InvalidInput,
            error.to_string(),
            None,
        )
    })?;
    let project_root = root.join("Work").join(member);
    if !project_root.is_dir() {
        return Err(ActionResult::failure(
            Some(action),
            ResultStatus::InvalidInput,
            format!("Project root does not exist: {}", project_root.display()),
            None,
        ));
    }
    read_project_manifest(&project_root).map_err(|error| {
        ActionResult::failure(
            Some(action),
            ResultStatus::InvalidCentralStructure,
            error.to_string(),
            None,
        )
    })?;
    Ok(project_root)
}

fn io_failure(action: &str, error: io::Error) -> ActionResult {
    let status = match error.kind() {
        io::ErrorKind::InvalidInput | io::ErrorKind::NotFound | io::ErrorKind::AlreadyExists => {
            ResultStatus::InvalidInput
        }
        io::ErrorKind::PermissionDenied => ResultStatus::UnavailableCapability,
        io::ErrorKind::InvalidData => ResultStatus::VerificationFailure,
        _ => ResultStatus::InternalFailure,
    };
    ActionResult::failure(Some(action), status, error.to_string(), None)
}

fn mutate_relations<F>(
    path: &Path,
    scope_ref: &str,
    update: F,
) -> io::Result<DevelopmentSourceRelations>
where
    F: FnOnce(&mut DevelopmentSourceRelations) -> io::Result<()>,
{
    let mut relations = read_relations(path, scope_ref)?;
    update(&mut relations)?;
    write_relations(path, &relations)?;
    Ok(relations)
}

fn ensure_source(bindings: &[SourceBinding], source_reference: &str) -> io::Result<SourceBinding> {
    bindings
        .iter()
        .find(|binding| binding.source_ref == source_reference)
        .cloned()
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "source_ref is not a participating source of this scoped World",
            )
        })
}

fn root_inspect_action(
    _: &ActionRegistry,
    _: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    const ACTION: &str = "central.self.inspect";
    let root = match root_context(ACTION, context) {
        Ok(root) => root,
        Err(result) => return result,
    };
    inspect_root_development_field(&root)
        .map(|reading| {
            ActionResult::success(
                ACTION,
                serde_json::to_value(reading).expect("development reading serialises"),
            )
        })
        .unwrap_or_else(|error| io_failure(ACTION, error))
}

fn project_inspect_action(
    _: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    const ACTION: &str = "projectcentral.self.inspect";
    let root = match project_context(ACTION, input, context) {
        Ok(root) => root,
        Err(result) => return result,
    };
    inspect_project_development_field(&root)
        .map(|reading| {
            ActionResult::success(
                ACTION,
                serde_json::to_value(reading).expect("development reading serialises"),
            )
        })
        .unwrap_or_else(|error| io_failure(ACTION, error))
}

fn root_ensure_action(
    _: &ActionRegistry,
    _: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    const ACTION: &str = "central.self.ensure";
    let root = match root_context(ACTION, context) {
        Ok(root) => root,
        Err(result) => return result,
    };
    ensure_root_self(&root)
        .map(|receipt| {
            ActionResult::success(
                ACTION,
                serde_json::to_value(receipt).expect("ensure receipt serialises"),
            )
        })
        .unwrap_or_else(|error| io_failure(ACTION, error))
}

fn project_ensure_action(
    _: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    const ACTION: &str = "projectcentral.self.ensure";
    let root = match project_context(ACTION, input, context) {
        Ok(root) => root,
        Err(result) => return result,
    };
    ensure_project_self(&root)
        .map(|receipt| {
            ActionResult::success(
                ACTION,
                serde_json::to_value(receipt).expect("ensure receipt serialises"),
            )
        })
        .unwrap_or_else(|error| io_failure(ACTION, error))
}

fn root_source_create_action(
    _: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    const ACTION: &str = "central.self.source.create";
    let root = match root_context(ACTION, context) {
        Ok(root) => root,
        Err(result) => return result,
    };
    let member = match required(input, "path", ACTION) {
        Ok(value) => value,
        Err(result) => return result,
    };
    let content = input.get("content").and_then(Value::as_str).unwrap_or("");
    let provenance = match required(input, "provenance", ACTION) {
        Ok(value) => value,
        Err(result) => return result,
    };
    let standing = match required(input, "standing", ACTION) {
        Ok(value) => value,
        Err(result) => return result,
    };
    if let Err(error) =
        parse_standing(&standing).and_then(|_| parse_provenance(&provenance).map(|_| ()))
    {
        return io_failure(ACTION, error);
    }
    let actor_kind = match required(input, "actor_kind", ACTION) {
        Ok(value) => value,
        Err(result) => return result,
    };
    if let Err(error) = source_create_authority(
        &provenance,
        &actor_kind,
        optional(input, "agent_session_ref").as_deref(),
        optional(input, "acceptance").as_deref(),
    ) {
        return io_failure(ACTION, error);
    }
    let tier = input
        .get("tier")
        .and_then(Value::as_u64)
        .map(|value| value as u8);
    if let Some(tier) = tier {
        if let Err(error) = semantic_office(tier) {
            return io_failure(ACTION, error);
        }
    }
    let result = (|| {
        let relative = create_text_source(&root, ROOT_SELF_DIR, &member, content)?;
        let mut roles = vec![
            "self-description-source".to_owned(),
            "root-self-source-aperture".to_owned(),
        ];
        if let Some(tier) = tier {
            roles.push(format!("document-tier-{tier}"));
        }
        let source_reference = write_root_ground_relation(
            &root,
            &relative,
            &provenance,
            &standing,
            roles,
            "control-self",
        )?;
        if let Some(tier) = tier {
            mutate_relations(
                &root.join(ROOT_DEVELOPMENT_RELATIONS),
                CONTROL_WORLD_REF,
                |relations| {
                    bind_tier(
                        relations,
                        tier,
                        &source_reference,
                        optional(input, "canonical_label"),
                        optional(input, "canonical_path"),
                    )
                },
            )?;
        }
        inspect_root_development_field(&root)
    })();
    result
        .map(|reading| {
            ActionResult::success(
                ACTION,
                serde_json::to_value(reading).expect("reading serialises"),
            )
        })
        .unwrap_or_else(|error| io_failure(ACTION, error))
}

fn project_source_create_action(
    _: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    const ACTION: &str = "projectcentral.self.source.create";
    let root = match project_context(ACTION, input, context) {
        Ok(root) => root,
        Err(result) => return result,
    };
    let member = match required(input, "path", ACTION) {
        Ok(value) => value,
        Err(result) => return result,
    };
    let content = input.get("content").and_then(Value::as_str).unwrap_or("");
    let provenance_raw = match required(input, "provenance", ACTION) {
        Ok(value) => value,
        Err(result) => return result,
    };
    let standing_raw = match required(input, "standing", ACTION) {
        Ok(value) => value,
        Err(result) => return result,
    };
    let provenance = match parse_provenance(&provenance_raw) {
        Ok(value) => value,
        Err(error) => return io_failure(ACTION, error),
    };
    let standing = match parse_standing(&standing_raw) {
        Ok(value) => value,
        Err(error) => return io_failure(ACTION, error),
    };
    let actor_kind = match required(input, "actor_kind", ACTION) {
        Ok(value) => value,
        Err(result) => return result,
    };
    if let Err(error) = source_create_authority(
        &provenance_raw,
        &actor_kind,
        optional(input, "agent_session_ref").as_deref(),
        optional(input, "acceptance").as_deref(),
    ) {
        return io_failure(ACTION, error);
    }
    let tier = input
        .get("tier")
        .and_then(Value::as_u64)
        .map(|value| value as u8);
    if let Some(tier) = tier {
        if let Err(error) = semantic_office(tier) {
            return io_failure(ACTION, error);
        }
    }
    let result = (|| {
        let relative = create_text_source(&root, PROJECT_SELF_DIR, &member, content)?;
        let mut roles = vec![
            "self-description-source".to_owned(),
            "project-self-source-aperture".to_owned(),
        ];
        if let Some(tier) = tier {
            roles.push(format!("document-tier-{tier}"));
        }
        let applied = apply_accepted_ground_relation(
            &root,
            &relative,
            provenance,
            standing,
            SourceTreatment::OrdinaryProjectSource,
            roles,
        )?;
        let manifest = read_project_manifest(&root)?;
        if let Some(tier) = tier {
            mutate_relations(
                &root.join(PROJECT_DEVELOPMENT_RELATIONS),
                &format!("project:{}", manifest.project_id),
                |relations| {
                    bind_tier(
                        relations,
                        tier,
                        &applied.relation.source_ref,
                        optional(input, "canonical_label"),
                        optional(input, "canonical_path"),
                    )
                },
            )?;
        }
        inspect_project_development_field(&root)
    })();
    result
        .map(|reading| {
            ActionResult::success(
                ACTION,
                serde_json::to_value(reading).expect("reading serialises"),
            )
        })
        .unwrap_or_else(|error| io_failure(ACTION, error))
}

fn tier_relate(
    world_root: &Path,
    project: bool,
    input: &Value,
    action: &str,
) -> io::Result<DevelopmentFieldReading> {
    let source_reference = input
        .get("source_ref")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("{action} requires source_ref"),
            )
        })?;
    let tier = input
        .get("tier")
        .and_then(Value::as_u64)
        .filter(|value| *value <= 5)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("{action} requires integer tier 0..5"),
            )
        })? as u8;
    let label = input
        .get("canonical_label")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let path = input
        .get("canonical_path")
        .and_then(Value::as_str)
        .map(str::to_owned);
    if project {
        let manifest = read_project_manifest(world_root)?;
        let bindings = project_source_bindings(world_root)?;
        ensure_source(&bindings, source_reference)?;
        let scope_ref = format!("project:{}", manifest.project_id);
        mutate_relations(
            &world_root.join(PROJECT_DEVELOPMENT_RELATIONS),
            &scope_ref,
            |relations| bind_tier(relations, tier, source_reference, label, path),
        )?;
        inspect_project_development_field(world_root)
    } else {
        let bindings = control_source_bindings(world_root)?;
        ensure_source(&bindings, source_reference)?;
        mutate_relations(
            &world_root.join(ROOT_DEVELOPMENT_RELATIONS),
            CONTROL_WORLD_REF,
            |relations| bind_tier(relations, tier, source_reference, label, path),
        )?;
        inspect_root_development_field(world_root)
    }
}

fn root_tier_action(
    _: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    const ACTION: &str = "central.self.tier.relate";
    let root = match root_context(ACTION, context) {
        Ok(root) => root,
        Err(result) => return result,
    };
    if optional(input, "acceptance").as_deref() != Some("human-accepted") {
        return ActionResult::failure(
            Some(ACTION),
            ResultStatus::InvalidInput,
            "tier relation changes require explicit human-accepted relation acknowledgement",
            None,
        );
    }
    tier_relate(&root, false, input, ACTION)
        .map(|reading| ActionResult::success(ACTION, serde_json::to_value(reading).unwrap()))
        .unwrap_or_else(|error| io_failure(ACTION, error))
}

fn project_tier_action(
    _: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    const ACTION: &str = "projectcentral.self.tier.relate";
    let root = match project_context(ACTION, input, context) {
        Ok(root) => root,
        Err(result) => return result,
    };
    if optional(input, "acceptance").as_deref() != Some("human-accepted") {
        return ActionResult::failure(
            Some(ACTION),
            ResultStatus::InvalidInput,
            "tier relation changes require explicit human-accepted relation acknowledgement",
            None,
        );
    }
    tier_relate(&root, true, input, ACTION)
        .map(|reading| ActionResult::success(ACTION, serde_json::to_value(reading).unwrap()))
        .unwrap_or_else(|error| io_failure(ACTION, error))
}

fn project_retain_tier_action(
    _: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    const ACTION: &str = "projectcentral.self.retain-tier";
    let root = match project_context(ACTION, input, context) {
        Ok(root) => root,
        Err(result) => return result,
    };
    if optional(input, "acceptance").as_deref() != Some("human-accepted") {
        return ActionResult::failure(
            Some(ACTION),
            ResultStatus::InvalidInput,
            "retained-native source relation requires explicit human-accepted acknowledgement",
            None,
        );
    }
    let source = match required(input, "source", ACTION) {
        Ok(value) => value,
        Err(result) => return result,
    };
    let tier = match tier_input(input, ACTION) {
        Ok(value) => value,
        Err(result) => return result,
    };
    let provenance = match required(input, "provenance", ACTION)
        .and_then(|value| parse_provenance(&value).map_err(|error| io_failure(ACTION, error)))
    {
        Ok(value) => value,
        Err(result) => return result,
    };
    let standing = match required(input, "standing", ACTION)
        .and_then(|value| parse_standing(&value).map_err(|error| io_failure(ACTION, error)))
    {
        Ok(value) => value,
        Err(result) => return result,
    };
    let result = (|| {
        let applied = apply_accepted_ground_relation(
            &root,
            &source,
            provenance,
            standing,
            SourceTreatment::RetainNativeInPlace,
            vec![
                "self-description-source".to_owned(),
                format!("document-tier-{tier}"),
            ],
        )?;
        let manifest = read_project_manifest(&root)?;
        let scope_ref = format!("project:{}", manifest.project_id);
        mutate_relations(
            &root.join(PROJECT_DEVELOPMENT_RELATIONS),
            &scope_ref,
            |relations| {
                bind_tier(
                    relations,
                    tier,
                    &applied.relation.source_ref,
                    optional(input, "canonical_label"),
                    optional(input, "canonical_path"),
                )
            },
        )?;
        inspect_project_development_field(&root)
    })();
    result
        .map(|reading| ActionResult::success(ACTION, serde_json::to_value(reading).unwrap()))
        .unwrap_or_else(|error| io_failure(ACTION, error))
}

fn experience_relate(
    world_root: &Path,
    project: bool,
    input: &Value,
    ex: bool,
) -> io::Result<DevelopmentFieldReading> {
    let source_reference = input
        .get("source_ref")
        .and_then(Value::as_str)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "source_ref is required"))?;
    let bindings = if project {
        project_source_bindings(world_root)?
    } else {
        control_source_bindings(world_root)?
    };
    let source = ensure_source(&bindings, source_reference)?;
    if !recognised_human(&source.provenance) {
        return Err(io::Error::new(io::ErrorKind::PermissionDenied, "UX/EX source must already carry human-authored or human-adopted provenance; location alone and generated/Agent/observed provenance are insufficient"));
    }
    let (scope_ref, path) = if project {
        let manifest = read_project_manifest(world_root)?;
        (
            format!("project:{}", manifest.project_id),
            world_root.join(PROJECT_DEVELOPMENT_RELATIONS),
        )
    } else {
        (
            CONTROL_WORLD_REF.to_owned(),
            world_root.join(ROOT_DEVELOPMENT_RELATIONS),
        )
    };
    if ex {
        if source.standing != "observed-evidence" {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "EX source must carry observed-evidence standing while preserving its human authorship/adoption provenance"));
        }
        let ex_ref = input
            .get("ex_ref")
            .and_then(Value::as_str)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "ex_ref is required"))?;
        let ux_refs = input
            .get("ux_refs")
            .and_then(Value::as_array)
            .map(|values| {
                values
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default();
        let artifact_refs = input
            .get("artifact_refs")
            .and_then(Value::as_array)
            .map(|values| {
                values
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default();
        mutate_relations(&path, &scope_ref, |relations| {
            relate_ex(relations, ex_ref, source_reference, ux_refs, artifact_refs)
        })?;
    } else {
        if !matches!(
            source.standing.as_str(),
            "authored-human-position" | "design-commitment"
        ) {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "UX intended-experience source must carry authored-human-position or design-commitment standing, not implementation/evidence/inference standing"));
        }
        let ux_ref = input
            .get("ux_ref")
            .and_then(Value::as_str)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "ux_ref is required"))?;
        mutate_relations(&path, &scope_ref, |relations| {
            relate_ux(relations, ux_ref, source_reference)
        })?;
    }
    if project {
        inspect_project_development_field(world_root)
    } else {
        inspect_root_development_field(world_root)
    }
}

fn root_ux_action(
    _: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    const ACTION: &str = "central.self.ux.relate";
    let root = match root_context(ACTION, context) {
        Ok(root) => root,
        Err(result) => return result,
    };
    if optional(input, "acceptance").as_deref() != Some("human-accepted") {
        return ActionResult::failure(
            Some(ACTION),
            ResultStatus::InvalidInput,
            "UX relation requires explicit human-accepted acknowledgement",
            None,
        );
    }
    experience_relate(&root, false, input, false)
        .map(|reading| ActionResult::success(ACTION, serde_json::to_value(reading).unwrap()))
        .unwrap_or_else(|error| io_failure(ACTION, error))
}

fn project_ux_action(
    _: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    const ACTION: &str = "projectcentral.self.ux.relate";
    let root = match project_context(ACTION, input, context) {
        Ok(root) => root,
        Err(result) => return result,
    };
    if optional(input, "acceptance").as_deref() != Some("human-accepted") {
        return ActionResult::failure(
            Some(ACTION),
            ResultStatus::InvalidInput,
            "UX relation requires explicit human-accepted acknowledgement",
            None,
        );
    }
    experience_relate(&root, true, input, false)
        .map(|reading| ActionResult::success(ACTION, serde_json::to_value(reading).unwrap()))
        .unwrap_or_else(|error| io_failure(ACTION, error))
}

fn ex_authority(input: &Value, action: &str) -> Result<(), ActionResult> {
    if optional(input, "actor_kind").as_deref() != Some("human")
        || optional(input, "agent_session_ref").is_some()
        || optional(input, "acceptance").as_deref() != Some("human-accepted")
    {
        return Err(ActionResult::failure(
            Some(action),
            ResultStatus::UnavailableCapability,
            "EX relation requires a declared human caller, explicit human-accepted acknowledgement, and no agent_session_ref; an Agent-only operation cannot promote experiential truth",
            Some(json!({"ex_created": false, "human_experience_fabricated": false})),
        ));
    }
    Ok(())
}

fn root_ex_action(
    _: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    const ACTION: &str = "central.self.ex.relate";
    if let Err(result) = ex_authority(input, ACTION) {
        return result;
    }
    let root = match root_context(ACTION, context) {
        Ok(root) => root,
        Err(result) => return result,
    };
    experience_relate(&root, false, input, true)
        .map(|reading| ActionResult::success(ACTION, serde_json::to_value(reading).unwrap()))
        .unwrap_or_else(|error| io_failure(ACTION, error))
}

fn project_ex_action(
    _: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    const ACTION: &str = "projectcentral.self.ex.relate";
    if let Err(result) = ex_authority(input, ACTION) {
        return result;
    }
    let root = match project_context(ACTION, input, context) {
        Ok(root) => root,
        Err(result) => return result,
    };
    experience_relate(&root, true, input, true)
        .map(|reading| ActionResult::success(ACTION, serde_json::to_value(reading).unwrap()))
        .unwrap_or_else(|error| io_failure(ACTION, error))
}

fn resolve_reading(reading: DevelopmentFieldReading, reference: &str) -> io::Result<Value> {
    if let Some(ux) = reading.ux.iter().find(|entry| entry.ux_ref == reference) {
        return Ok(json!({"kind":"ux","scope_ref":reading.scope_ref,"reading":ux}));
    }
    if let Some(ex) = reading.ex.iter().find(|entry| entry.ex_ref == reference) {
        return Ok(json!({"kind":"ex","scope_ref":reading.scope_ref,"reading":ex}));
    }
    for tier in &reading.tier_bindings {
        for source in tier.sources.iter().flatten() {
            if source.source_ref == reference {
                return Ok(
                    json!({"kind":"source","scope_ref":reading.scope_ref,"tier":tier.tier,"reading":source}),
                );
            }
        }
    }
    for source in &reading.self_aperture.linked_sources {
        if source.source_ref == reference {
            return Ok(json!({"kind":"source","scope_ref":reading.scope_ref,"reading":source}));
        }
    }
    Err(io::Error::new(
        io::ErrorKind::NotFound,
        "reference is not a Development Field UX/EX/source relation in this scope",
    ))
}

fn root_resolve_action(
    _: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    const ACTION: &str = "central.self.resolve";
    let reference = match required(input, "ref", ACTION) {
        Ok(value) => value,
        Err(result) => return result,
    };
    let root = match root_context(ACTION, context) {
        Ok(root) => root,
        Err(result) => return result,
    };
    inspect_root_development_field(&root)
        .and_then(|reading| resolve_reading(reading, &reference))
        .map(|value| ActionResult::success(ACTION, value))
        .unwrap_or_else(|error| io_failure(ACTION, error))
}

fn project_resolve_action(
    _: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    const ACTION: &str = "projectcentral.self.resolve";
    let reference = match required(input, "ref", ACTION) {
        Ok(value) => value,
        Err(result) => return result,
    };
    let root = match project_context(ACTION, input, context) {
        Ok(root) => root,
        Err(result) => return result,
    };
    inspect_project_development_field(&root)
        .and_then(|reading| resolve_reading(reading, &reference))
        .map(|value| ActionResult::success(ACTION, value))
        .unwrap_or_else(|error| io_failure(ACTION, error))
}

fn machine_policy_action(
    _: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    const ACTION: &str = "machine.oi-suite-policy";
    let role = match required(input, "role", ACTION) {
        Ok(value) => value,
        Err(result) => return result,
    };
    let root = match root_context(ACTION, context) {
        Ok(root) => root,
        Err(result) => return result,
    };
    match read_machine_oi_suite_policy(&root, &role) {
        Ok(reading) => ActionResult::success(
            ACTION,
            serde_json::to_value(reading).expect("machine suite policy reading serialises"),
        ),
        Err(error) => ActionResult::failure(
            Some(ACTION),
            ResultStatus::InvalidInput,
            error.message,
            None,
        ),
    }
}

fn text_input(name: &str, required: bool) -> ActionInputDefinition {
    ActionInputDefinition {
        name: name.to_owned(),
        input_type: "string".to_owned(),
        required,
        choices: None,
        selection: None,
    }
}

fn array_input(name: &str) -> ActionInputDefinition {
    ActionInputDefinition {
        name: name.to_owned(),
        input_type: "array".to_owned(),
        required: false,
        choices: None,
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
        preview_supported: false,
        required_ports: Vec::new(),
        availability: ActionAvailability {
            available: true,
            reason: None,
        },
    }
}

pub fn register_development_field_actions(registry: &mut ActionRegistry) {
    type Handler = fn(&ActionRegistry, &Value, &ActionExecutionContext<'_>) -> ActionResult;
    let project = || text_input("project", true);
    let source_create_inputs = || {
        vec![
            text_input("path", true),
            text_input("content", false),
            text_input("provenance", true),
            text_input("standing", true),
            text_input("actor_kind", true),
            text_input("agent_session_ref", false),
            text_input("acceptance", false),
            ActionInputDefinition {
                name: "tier".to_owned(),
                input_type: "integer".to_owned(),
                required: false,
                choices: None,
                selection: None,
            },
            text_input("canonical_label", false),
            text_input("canonical_path", false),
        ]
    };
    let tier_inputs = || {
        vec![
            text_input("source_ref", true),
            ActionInputDefinition {
                name: "tier".to_owned(),
                input_type: "integer".to_owned(),
                required: true,
                choices: None,
                selection: None,
            },
            text_input("canonical_label", false),
            text_input("canonical_path", false),
            text_input("acceptance", true),
        ]
    };
    let ux_inputs = || {
        vec![
            text_input("ux_ref", true),
            text_input("source_ref", true),
            text_input("acceptance", true),
        ]
    };
    let ex_inputs = || {
        vec![
            text_input("ex_ref", true),
            text_input("source_ref", true),
            array_input("ux_refs"),
            array_input("artifact_refs"),
            text_input("actor_kind", true),
            text_input("agent_session_ref", false),
            text_input("acceptance", true),
        ]
    };

    let registrations: Vec<(ActionDescriptor, Handler)> = vec![
        (descriptor("central.self.inspect", "Inspect root self-description field", "Read the root self aperture, six stable tier bindings, UX/EX relations and their live Central source provenance/revisions without exposing source payloads.", MutationClass::ReadOnly, "development-field-reading", vec![]), root_inspect_action),
        (descriptor("central.self.ensure", "Ensure root self-description aperture", "Add Control/self when absent. Existing source, documents, matrices, Skills, Methods, Wiki, governance and temporal fields are not moved.", MutationClass::LocallyMutating, "self-aperture-ensure-receipt", vec![]), root_ensure_action),
        (descriptor("central.self.source.create", "Create root self-description source", "Create one bounded source under Control/self with explicit provenance/standing. Human authority requires a declared human caller and explicit acceptance; generated source never gains human authority from location.", MutationClass::LocallyMutating, "development-field-reading", source_create_inputs()), root_source_create_action),
        (descriptor("central.self.tier.relate", "Relate root source to document tier", "Bind a participating Central SourceRef to stable tier 0..5; canonical label/path remain optional authored data rather than invented defaults.", MutationClass::LocallyMutating, "development-field-reading", tier_inputs()), root_tier_action),
        (descriptor("central.self.ux.relate", "Relate root UX source", "Bind a human-authored/adopted intended-experience source to a stable ux_ref and tier 1 without treating implementation, tests, inference or EX as UX.", MutationClass::LocallyMutating, "development-field-reading", ux_inputs()), root_ux_action),
        (descriptor("central.self.ex.relate", "Relate root EX source", "Bind an observed-evidence source with human authorship/adoption to ex_ref and linked ux_refs. Agent-only calls cannot promote an experiential report.", MutationClass::LocallyMutating, "development-field-reading", ex_inputs()), root_ex_action),
        (descriptor("central.self.resolve", "Resolve root Development Field ref", "Resolve one ux_ref, ex_ref or linked SourceRef through Central's source/provenance/revision model.", MutationClass::ReadOnly, "development-field-reference", vec![text_input("ref", true)]), root_resolve_action),
        (descriptor("machine.oi-suite-policy", "Inspect machine O:I suite policy intent", "Read the authored opaque oi-suite-policy binding on a machine role. Installed-suite receipts and material executable observations remain owned outside Central.", MutationClass::ReadOnly, "machine-oi-suite-policy-reading", vec![text_input("role", true)]), machine_policy_action),
        (descriptor("projectcentral.self.inspect", "Inspect Project self-description field", "Read the Project self aperture, tier bindings, UX/EX relations and their live source provenance/revisions without path reconstruction or source payload disclosure.", MutationClass::ReadOnly, "development-field-reading", vec![project()]), project_inspect_action),
        (descriptor("projectcentral.self.ensure", "Ensure Project self-description aperture", "Add ProjectCentral/self when absent without moving native documentation, matrices, Skills, Methods, Wiki, governance or NOW.", MutationClass::LocallyMutating, "self-aperture-ensure-receipt", vec![project()]), project_ensure_action),
        ({ let mut inputs=vec![project()]; inputs.extend(source_create_inputs()); descriptor("projectcentral.self.source.create", "Create Project self-description source", "Create one bounded source under ProjectCentral/self and record its explicit provenance/standing relation; location alone never confers human authorship.", MutationClass::LocallyMutating, "development-field-reading", inputs) }, project_source_create_action),
        ({ let mut inputs=vec![project()]; inputs.extend(tier_inputs()); descriptor("projectcentral.self.tier.relate", "Relate Project source to document tier", "Bind a participating Project SourceRef to stable tier 0..5 with optional canonical label/path supplied by authored source.", MutationClass::LocallyMutating, "development-field-reading", inputs) }, project_tier_action),
        ({ let mut inputs=vec![project(), text_input("source", true), ActionInputDefinition { name:"tier".to_owned(), input_type:"integer".to_owned(), required:true, choices:None, selection:None }, text_input("provenance", true), text_input("standing", true), text_input("canonical_label", false), text_input("canonical_path", false), text_input("acceptance", true)]; descriptor("projectcentral.self.retain-tier", "Relate retained native source into Project tier", "Record an accepted Project ground relation for an existing native source, retain its bytes/path in place, and bind its canonical SourceRef into stable tier 0..5.", MutationClass::LocallyMutating, "development-field-reading", inputs) }, project_retain_tier_action),
        ({ let mut inputs=vec![project()]; inputs.extend(ux_inputs()); descriptor("projectcentral.self.ux.relate", "Relate Project UX source", "Bind a human-authored/adopted intended-experience source to ux_ref and tier 1 while preserving source standing/revision.", MutationClass::LocallyMutating, "development-field-reading", inputs) }, project_ux_action),
        ({ let mut inputs=vec![project()]; inputs.extend(ex_inputs()); descriptor("projectcentral.self.ex.relate", "Relate Project EX source", "Bind a human-authored/adopted observed-evidence report to ex_ref, linked ux_refs and artifact/evidence refs. Agent-only calls are refused.", MutationClass::LocallyMutating, "development-field-reading", inputs) }, project_ex_action),
        (descriptor("projectcentral.self.resolve", "Resolve Project Development Field ref", "Resolve one Project ux_ref, ex_ref or linked SourceRef through live Central source/provenance/revision relations.", MutationClass::ReadOnly, "development-field-reference", vec![project(), text_input("ref", true)]), project_resolve_action),
    ];
    for (descriptor, handler) in registrations {
        registry
            .register(descriptor, handler)
            .expect("Development Field Action ids are valid and unique");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::projectcentral_ops::initialize_projectcentral;
    use crate::root::initialize_central;
    use tempfile::tempdir;

    fn project_fixture() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let temp = tempdir().unwrap();
        let central = temp.path().join("Central");
        initialize_central(&central).unwrap();
        let project = central.join("Work/example");
        fs::create_dir_all(&project).unwrap();
        initialize_projectcentral(&central, &project, "example").unwrap();
        (temp, central, project)
    }

    #[test]
    fn legacy_project_is_valid_but_self_absence_is_migratable() {
        let (_temp, _central, project) = project_fixture();
        let reading = inspect_project_development_field(&project).unwrap();
        assert_eq!(
            reading.self_aperture.status,
            SelfApertureStatus::LegacyMigratableAbsence
        );
        let receipt = ensure_project_self(&project).unwrap();
        assert_eq!(receipt.current_status, SelfApertureStatus::Present);
        assert!(!receipt.documentation_moved);
        assert!(project.join(PROJECT_SELF_DIR).is_dir());
    }

    #[test]
    fn tier_binding_keeps_native_source_in_place_and_does_not_invent_label() {
        let (_temp, _central, project) = project_fixture();
        fs::write(project.join("VISION.md"), "human vision").unwrap();
        let applied = apply_accepted_ground_relation(
            &project,
            "VISION.md",
            SourceProvenance::HumanAdopted,
            SourceStanding::AuthoredHumanPosition,
            SourceTreatment::RetainNativeInPlace,
            vec!["self-description-source".into()],
        )
        .unwrap();
        let manifest = read_project_manifest(&project).unwrap();
        let scope_ref = format!("project:{}", manifest.project_id);
        mutate_relations(
            &project.join(PROJECT_DEVELOPMENT_RELATIONS),
            &scope_ref,
            |relations| bind_tier(relations, 1, &applied.relation.source_ref, None, None),
        )
        .unwrap();
        let reading = inspect_project_development_field(&project).unwrap();
        assert_eq!(reading.tier_bindings[0].tier, 1);
        assert!(reading.tier_bindings[0].canonical_label.is_none());
        assert_eq!(
            reading.tier_bindings[0].sources[0].as_ref().unwrap().path,
            "VISION.md"
        );
        assert!(
            reading.tier_bindings[0].sources[0]
                .as_ref()
                .unwrap()
                .retained_native
        );
        assert_eq!(
            fs::read_to_string(project.join("VISION.md")).unwrap(),
            "human vision"
        );
    }

    #[test]
    fn unbound_file_near_self_gains_no_human_authority() {
        let (_temp, _central, project) = project_fixture();
        ensure_project_self(&project).unwrap();
        fs::write(
            project.join(PROJECT_SELF_DIR).join("account.html"),
            "generated",
        )
        .unwrap();
        let reading = inspect_project_development_field(&project).unwrap();
        assert_eq!(reading.self_aperture.unbound_sources.len(), 1);
        let source = &reading.self_aperture.unbound_sources[0];
        assert_eq!(source.provenance, "unresolved");
        assert!(!source.authority_from_location);
    }

    #[test]
    fn ux_and_ex_are_separate_and_ex_requires_human_provenance() {
        let (_temp, _central, project) = project_fixture();
        fs::write(project.join("UX.md"), "intended experience").unwrap();
        fs::write(project.join("EX.md"), "actual human return").unwrap();
        let ux_source = apply_accepted_ground_relation(
            &project,
            "UX.md",
            SourceProvenance::HumanAdopted,
            SourceStanding::AuthoredHumanPosition,
            SourceTreatment::RetainNativeInPlace,
            vec![],
        )
        .unwrap();
        let ex_source = apply_accepted_ground_relation(
            &project,
            "EX.md",
            SourceProvenance::HumanAdopted,
            SourceStanding::ObservedEvidence,
            SourceTreatment::RetainNativeInPlace,
            vec![],
        )
        .unwrap();
        let manifest = read_project_manifest(&project).unwrap();
        let scope_ref = format!("project:{}", manifest.project_id);
        mutate_relations(
            &project.join(PROJECT_DEVELOPMENT_RELATIONS),
            &scope_ref,
            |relations| {
                relate_ux(relations, "ux:example:main", &ux_source.relation.source_ref)?;
                relate_ex(
                    relations,
                    "ex:example:1",
                    &ex_source.relation.source_ref,
                    vec!["ux:example:main".into()],
                    vec!["artifact:screenshot:1".into()],
                )
            },
        )
        .unwrap();
        let reading = inspect_project_development_field(&project).unwrap();
        assert_eq!(reading.ux.len(), 1);
        assert_eq!(reading.ex.len(), 1);
        assert!(!reading.ux[0].ex_human_return);
        assert!(reading.ex[0].human_experience_return);
        assert_eq!(reading.ex[0].artifact_refs, vec!["artifact:screenshot:1"]);
    }

    #[test]
    fn machine_suite_policy_is_opaque_authored_intent() {
        let temp = tempdir().unwrap();
        let central = temp.path().join("Central");
        initialize_central(&central).unwrap();
        fs::write(
            central.join("Control/machines/current.json"),
            r#"{"schema":"central.machine","version":1,"role":"current","capabilities":[],"requirements":{},"bindings":[{"kind":"workcell","reference":"workcell:local"},{"kind":"oi-suite-policy","reference":"mainline"}]}"#,
        ).unwrap();
        let reading = read_machine_oi_suite_policy(&central, "current").unwrap();
        assert_eq!(reading.state, "authored-intent");
        assert_eq!(reading.desired_policy_refs, vec!["mainline"]);
        assert!(!reading.installed_suite_receipt_owned_by_central);
        let machine = read_machine_declaration(&central, "current").unwrap();
        assert!(machine
            .declaration
            .bindings
            .iter()
            .any(|binding| binding.kind == "workcell" && binding.reference == "workcell:local"));
    }

    #[test]
    fn agent_only_ex_authority_is_refused() {
        let input = json!({"actor_kind":"agent","agent_session_ref":"agent-session:1","acceptance":"human-accepted"});
        assert!(ex_authority(&input, "test.ex").is_err());
    }
}
