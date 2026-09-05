use crate::action::{
    ActionAvailability, ActionDescriptor, ActionExecutionContext, ActionInputDefinition,
    ActionOutputDefinition, ActionRegistry, MutationClass,
};
use crate::projectcentral::read_project_manifest;
use crate::result::{ActionResult, ResultStatus};
use crate::root::resolve_central_root;
use crate::source_horizon::{
    collect_files, normalize_relative, retrieval_allowed, source_ref, SourceBinding,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub const SKILL_BODY: &str = "SKILL.md";
pub const SKILL_MANIFEST: &str = "skill.json";
pub const SKILL_MANIFEST_SCHEMA: &str = "central.skill/v1";
/// Skills participate in the ground relations model under this treatment.
pub const CONTROL_SKILL_TREATMENT: &str = "control-skill";
pub const PERSONAL_SKILL_DIR: &str = "Control/user/skills";
pub const SKILLS_SEGMENT: &str = "skills";
pub const PROJECT_SKILL_DIR: &str = "ProjectCentral/user/skills";

pub const SCOPE_CONTROL_USER: &str = "control-user";
pub const SCOPE_CONTROL_MACHINE: &str = "control-machine";
pub const SCOPE_PROJECTCENTRAL_USER: &str = "projectcentral-user";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SkillScope {
    ControlUser,
    ControlMachine,
    ProjectcentralUser,
}

impl SkillScope {
    fn from_input(raw: &str) -> io::Result<Self> {
        match raw {
            SCOPE_CONTROL_USER => Ok(Self::ControlUser),
            SCOPE_CONTROL_MACHINE => Ok(Self::ControlMachine),
            SCOPE_PROJECTCENTRAL_USER => Ok(Self::ProjectcentralUser),
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("unsupported skill scope: {raw}"),
            )),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::ControlUser => SCOPE_CONTROL_USER,
            Self::ControlMachine => SCOPE_CONTROL_MACHINE,
            Self::ProjectcentralUser => SCOPE_PROJECTCENTRAL_USER,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SkillStanding {
    Active,
    Retired,
}

impl SkillStanding {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Retired => "retired",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SkillProvenance {
    HumanAuthored,
    Adopted,
}

impl SkillProvenance {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::HumanAuthored => "human-authored",
            Self::Adopted => "adopted",
        }
    }

    /// The Source Change Horizon vocabulary recognises adoption as human
    /// adoption (`human-adopted`); the manifest keeps the short form.
    fn horizon_str(self) -> &'static str {
        match self {
            Self::HumanAuthored => "human-authored",
            Self::Adopted => "human-adopted",
        }
    }
}

/// The retirement record: provenance of a standing change. `retired_by` is
/// declared by the caller and recorded verbatim; it is never inferred and
/// never substituted for the human's own authorship.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillRetirement {
    pub retired_by: String,
    pub retired_at_unix_seconds: u64,
    pub retirement_reason: String,
}

/// The skill ground manifest. Standing and provenance live here and nowhere
/// else; Central never infers them from a path. Unknown fields are preserved
/// so a standing change never silently discards authored metadata.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SkillManifest {
    pub schema: String,
    pub name: String,
    pub scope: SkillScope,
    pub standing: SkillStanding,
    pub provenance: SkillProvenance,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retirement: Option<SkillRetirement>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

/// One resolved skill scope on disk: the skills directory, its world, and how
/// paths inside it are addressed.
#[derive(Debug, Clone, PartialEq, Eq)]
struct SkillLocation {
    scope: SkillScope,
    machine: Option<String>,
    project: Option<String>,
    /// The ProjectCentral project id used by the canonical ref grammar.
    project_id: Option<String>,
    /// Absolute skills directory (may not exist — absence is disclosed, not repaired).
    skills_root: PathBuf,
    /// Central-root-relative display path of the skills directory.
    skills_root_display: String,
    /// The world this scope's sources belong to (`control:root` / `project:{id}`).
    world_ref: String,
    /// Path prefix of the skills directory relative to its world root.
    world_relative_prefix: String,
}

impl SkillLocation {
    fn skill_display_path(&self, name: &str) -> String {
        format!("{}/{}", self.skills_root_display, name)
    }

    fn skill_world_relative(&self, name: &str) -> String {
        format!("{}/{}", self.world_relative_prefix, name)
    }

    fn body_source_ref(&self, name: &str) -> String {
        source_ref(&self.world_ref, &self.skill_world_relative(name))
            + "/"
            + SKILL_BODY
    }
}

/// A skill as disclosed by inspection: scope, standing, provenance, faults.
/// Everything is reported as found; nothing is inferred.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SkillRecord {
    pub name: String,
    pub scope: String,
    pub machine: Option<String>,
    pub project: Option<String>,
    /// Central-root-relative path of the skill directory.
    pub path: String,
    /// Canonical source ref of the skill body (present when SKILL.md exists).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body_source_ref: Option<String>,
    pub manifest_present: bool,
    pub standing: String,
    pub provenance: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retirement: Option<SkillRetirement>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub faults: Vec<String>,
}

/// One skill scope surface: honest about existence, even when absent.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SkillScopeSurface {
    pub scope: String,
    pub path: String,
    pub exists: bool,
    pub machine: Option<String>,
    pub project: Option<String>,
    pub skill_names: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SkillProjectionPolicy {
    pub retired_standing_projects: bool,
    pub projection_is_derived_from_ground: bool,
    pub central_projects_skills: bool,
    pub note: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SkillsInspection {
    pub central_root: PathBuf,
    pub scopes: Vec<SkillScopeSurface>,
    pub skills: Vec<SkillRecord>,
    pub active_skills: usize,
    pub retired_skills: usize,
    pub unresolved_skills: usize,
    pub projection_policy: SkillProjectionPolicy,
    pub next_actions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SkillMutationReceipt {
    pub skill: SkillRecord,
    /// Central-root-relative path of the manifest that was mutated.
    pub manifest_path: String,
    pub previous_standing: String,
    pub standing: String,
    /// The retirement record removed by a restore, for the transcript.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub removed_retirement: Option<SkillRetirement>,
    pub manifest_mutated: bool,
    pub skill_directory_mutated: bool,
    pub skill_bytes_mutated: bool,
}

pub fn read_skill_manifest(skill_dir: &Path) -> io::Result<Option<SkillManifest>> {
    let path = skill_dir.join(SKILL_MANIFEST);
    if !path.is_file() {
        return Ok(None);
    }
    let manifest: SkillManifest = serde_json::from_slice(&fs::read(&path)?).map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{} is not a valid skill manifest: {error}", path.display()),
        )
    })?;
    if manifest.schema != SKILL_MANIFEST_SCHEMA {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("skill manifest schema must be {SKILL_MANIFEST_SCHEMA}: {}", path.display()),
        ));
    }
    Ok(Some(manifest))
}

fn write_skill_manifest(skill_dir: &Path, manifest: &SkillManifest) -> io::Result<()> {
    let path = skill_dir.join(SKILL_MANIFEST);
    let mut bytes = serde_json::to_vec_pretty(manifest)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    bytes.push(b'\n');
    fs::write(path, bytes)
}

fn validate_segment(raw: &str, field: &str) -> io::Result<String> {
    if raw.trim().is_empty() || raw != raw.trim() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{field} must be a non-empty name without surrounding whitespace"),
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
            format!("{field} must be a single path segment"),
        ));
    }
    Ok(raw.to_owned())
}

fn resolve_skill_location(
    central_root: &Path,
    scope: SkillScope,
    machine: Option<&str>,
    project: Option<&str>,
) -> io::Result<SkillLocation> {
    match scope {
        SkillScope::ControlUser => {
            if machine.is_some() || project.is_some() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "control-user scope takes no machine or project",
                ));
            }
            Ok(SkillLocation {
                scope,
                machine: None,
                project: None,
                project_id: None,
                skills_root: central_root.join(PERSONAL_SKILL_DIR),
                skills_root_display: PERSONAL_SKILL_DIR.to_owned(),
                world_ref: "control:root".to_owned(),
                world_relative_prefix: PERSONAL_SKILL_DIR.to_owned(),
            })
        }
        SkillScope::ControlMachine => {
            let machine =
                machine.ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "control-machine scope requires a machine role name",
                    )
                })?;
            validate_segment(machine, "machine")?;
            if project.is_some() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "control-machine scope takes no project",
                ));
            }
            let display = format!("Control/machines/{machine}/{SKILLS_SEGMENT}");
            Ok(SkillLocation {
                scope,
                machine: Some(machine.to_owned()),
                project: None,
                project_id: None,
                skills_root: central_root.join(&display),
                skills_root_display: display.clone(),
                world_ref: "control:root".to_owned(),
                world_relative_prefix: display,
            })
        }
        SkillScope::ProjectcentralUser => {
            let project = project.ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "projectcentral-user scope requires a project name",
                )
            })?;
            validate_segment(project, "project")?;
            if machine.is_some() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "projectcentral-user scope takes no machine",
                ));
            }
            let project_root = central_root.join("Work").join(project);
            if !project_root.is_dir() {
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("project root does not exist: {}", project_root.display()),
                ));
            }
            // The ref grammar addresses project sources by ProjectCentral id;
            // fall back to the directory name exactly as ground inspection does.
            let project_id = read_project_manifest(&project_root)
                .ok()
                .map(|manifest| manifest.project_id)
                .unwrap_or_else(|| project.to_owned());
            let display = format!("Work/{project}/{PROJECT_SKILL_DIR}");
            Ok(SkillLocation {
                scope,
                machine: None,
                project: Some(project.to_owned()),
                project_id: Some(project_id.clone()),
                skills_root: project_root.join(PROJECT_SKILL_DIR),
                skills_root_display: display,
                world_ref: format!("project:{project_id}"),
                world_relative_prefix: PROJECT_SKILL_DIR.to_owned(),
            })
        }
    }
}

fn read_skill_record(location: &SkillLocation, name: &str) -> SkillRecord {
    let skill_dir = location.skills_root.join(name);
    let mut faults = Vec::new();
    let manifest = match read_skill_manifest(&skill_dir) {
        Ok(manifest) => manifest,
        Err(error) => {
            faults.push(error.to_string());
            None
        }
    };
    let (standing, provenance, retirement) = match &manifest {
        Some(manifest) => {
            if manifest.scope != location.scope {
                faults.push(format!(
                    "manifest scope {} does not match location {}",
                    manifest.scope.as_str(),
                    location.scope.as_str()
                ));
            }
            if manifest.name != name {
                faults.push(format!(
                    "manifest name {} does not match directory {name}",
                    manifest.name
                ));
            }
            if manifest.standing == SkillStanding::Retired && manifest.retirement.is_none() {
                faults.push("standing is retired without a retirement record".to_owned());
            }
            (
                manifest.standing.as_str().to_owned(),
                manifest.provenance.as_str().to_owned(),
                manifest.retirement.clone(),
            )
        }
        None => ("unresolved".to_owned(), "unresolved".to_owned(), None),
    };
    let body = skill_dir.join(SKILL_BODY);
    SkillRecord {
        name: name.to_owned(),
        scope: location.scope.as_str().to_owned(),
        machine: location.machine.clone(),
        project: location.project.clone(),
        path: location.skill_display_path(name),
        body_source_ref: body.is_file().then(|| location.body_source_ref(name)),
        manifest_present: skill_dir.join(SKILL_MANIFEST).is_file() || manifest.is_some(),
        standing,
        provenance,
        retirement,
        faults,
    }
}

pub(crate) fn child_directories(root: &Path) -> io::Result<Vec<String>> {
    if !root.is_dir() {
        return Ok(Vec::new());
    }
    let mut names = fs::read_dir(root)?
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .filter(|entry| entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false))
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    names.sort();
    Ok(names)
}

fn scope_surface(location: &SkillLocation) -> io::Result<SkillScopeSurface> {
    let skill_names = child_directories(&location.skills_root)?;
    Ok(SkillScopeSurface {
        scope: location.scope.as_str().to_owned(),
        path: location.skills_root_display.clone(),
        exists: location.skills_root.is_dir(),
        machine: location.machine.clone(),
        project: location.project.clone(),
        skill_names: skill_names.clone(),
    })
}

pub fn inspect_control_skills(central_root: &Path) -> io::Result<SkillsInspection> {
    let mut scopes = Vec::new();
    let mut skills = Vec::new();

    let personal = resolve_skill_location(central_root, SkillScope::ControlUser, None, None)?;
    scopes.push(scope_surface(&personal)?);
    skills.extend(
        child_directories(&personal.skills_root)?
            .iter()
            .map(|name| read_skill_record(&personal, name)),
    );

    let machines_root = central_root.join("Control/machines");
    let machines = child_directories(&machines_root)?;
    if machines.is_empty() {
        scopes.push(SkillScopeSurface {
            scope: SCOPE_CONTROL_MACHINE.to_owned(),
            path: "Control/machines".to_owned(),
            exists: false,
            machine: None,
            project: None,
            skill_names: Vec::new(),
        });
    } else {
        for machine in machines {
            let location =
                resolve_skill_location(central_root, SkillScope::ControlMachine, Some(&machine), None)?;
            scopes.push(scope_surface(&location)?);
            skills.extend(
                child_directories(&location.skills_root)?
                    .iter()
                    .map(|name| read_skill_record(&location, name)),
            );
        }
    }

    let work_root = central_root.join("Work");
    let mut project_scopes = Vec::new();
    for project in child_directories(&work_root)? {
        let Ok(location) =
            resolve_skill_location(central_root, SkillScope::ProjectcentralUser, None, Some(&project))
        else {
            continue;
        };
        if !location.skills_root.is_dir() {
            continue;
        }
        project_scopes.push(scope_surface(&location)?);
        skills.extend(
            child_directories(&location.skills_root)?
                .iter()
                .map(|name| read_skill_record(&location, name)),
        );
    }
    if project_scopes.is_empty() {
        project_scopes.push(SkillScopeSurface {
            scope: SCOPE_PROJECTCENTRAL_USER.to_owned(),
            path: "Work/*/ProjectCentral/user/skills".to_owned(),
            exists: false,
            machine: None,
            project: None,
            skill_names: Vec::new(),
        });
    }
    scopes.extend(project_scopes);

    let active_skills = skills.iter().filter(|skill| skill.standing == "active").count();
    let retired_skills = skills.iter().filter(|skill| skill.standing == "retired").count();
    let unresolved_skills = skills
        .iter()
        .filter(|skill| skill.standing == "unresolved")
        .count();

    let mut next_actions = Vec::new();
    if skills.is_empty() {
        next_actions.push(
            "No skills are authored at any scope; author one only when its absence would degrade future operation (the Control persistence rule).".to_owned(),
        );
    }
    if unresolved_skills > 0 {
        next_actions.push(format!(
            "{unresolved_skills} skill(s) have no valid ground manifest; author skill.json so standing and provenance are not unresolved."
        ));
    }
    if retired_skills > 0 {
        next_actions.push(
            "Retired skills remain ground with their provenance; restore via control.skills.restore only when returned reality warrants it.".to_owned(),
        );
    }
    if !skills.iter().any(|skill| skill.standing == "retired") && !skills.is_empty() {
        next_actions.push(
            "Retire via control.skills.retire when a skill stops earning ground; retirement records who, when and why instead of deleting.".to_owned(),
        );
    }

    Ok(SkillsInspection {
        central_root: central_root.to_path_buf(),
        scopes,
        skills,
        active_skills,
        retired_skills,
        unresolved_skills,
        projection_policy: SkillProjectionPolicy {
            retired_standing_projects: false,
            projection_is_derived_from_ground: true,
            central_projects_skills: false,
            note: "Central owns skill ground and standing; projection into harness paths is derived from this ground by harness tooling and includes active standing only.".to_owned(),
        },
        next_actions,
    })
}

pub fn retire_skill(
    central_root: &Path,
    scope: SkillScope,
    machine: Option<&str>,
    project: Option<&str>,
    name: &str,
    retired_by: &str,
    retirement_reason: &str,
) -> io::Result<SkillMutationReceipt> {
    let location = resolve_skill_location(central_root, scope, machine, project)?;
    let name = validate_segment(name, "skill name")?;
    let skill_dir = location.skills_root.join(&name);
    if !skill_dir.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("skill ground does not exist: {}", location.skill_display_path(&name)),
        ));
    }
    let mut manifest = read_skill_manifest(&skill_dir)?.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "skill has no ground manifest; author {SKILL_MANIFEST} first — retirement is a standing written into authored ground, never a deletion"
            ),
        )
    })?;
    if manifest.scope != location.scope {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "manifest scope {} does not match location {}",
                manifest.scope.as_str(),
                location.scope.as_str()
            ),
        ));
    }
    if manifest.name != name {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("manifest name {} does not match directory {name}", manifest.name),
        ));
    }
    if manifest.standing == SkillStanding::Retired {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "skill {name} is already retired{}",
                manifest
                    .retirement
                    .as_ref()
                    .map(|record| format!(" (by {} at unix {})", record.retired_by, record.retired_at_unix_seconds))
                    .unwrap_or_default()
            ),
        ));
    }
    let previous_standing = manifest.standing.as_str().to_owned();
    manifest.standing = SkillStanding::Retired;
    manifest.retirement = Some(SkillRetirement {
        retired_by: retired_by.to_owned(),
        retired_at_unix_seconds: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
        retirement_reason: retirement_reason.to_owned(),
    });
    write_skill_manifest(&skill_dir, &manifest)?;

    Ok(SkillMutationReceipt {
        skill: read_skill_record(&location, &name),
        manifest_path: format!("{}/{}", location.skill_display_path(&name), SKILL_MANIFEST),
        previous_standing,
        standing: SkillStanding::Retired.as_str().to_owned(),
        removed_retirement: None,
        manifest_mutated: true,
        skill_directory_mutated: false,
        skill_bytes_mutated: false,
    })
}

pub fn restore_skill(
    central_root: &Path,
    scope: SkillScope,
    machine: Option<&str>,
    project: Option<&str>,
    name: &str,
) -> io::Result<SkillMutationReceipt> {
    let location = resolve_skill_location(central_root, scope, machine, project)?;
    let name = validate_segment(name, "skill name")?;
    let skill_dir = location.skills_root.join(&name);
    if !skill_dir.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("skill ground does not exist: {}", location.skill_display_path(&name)),
        ));
    }
    let mut manifest = read_skill_manifest(&skill_dir)?.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("skill has no ground manifest; {SKILL_MANIFEST} is authored, not derived"),
        )
    })?;
    if manifest.scope != location.scope {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "manifest scope {} does not match location {}",
                manifest.scope.as_str(),
                location.scope.as_str()
            ),
        ));
    }
    if manifest.name != name {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("manifest name {} does not match directory {name}", manifest.name),
        ));
    }
    if manifest.standing != SkillStanding::Retired {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("skill {name} is active; there is no retirement to reverse"),
        ));
    }
    let removed = manifest.retirement.take();
    manifest.standing = SkillStanding::Active;
    write_skill_manifest(&skill_dir, &manifest)?;

    Ok(SkillMutationReceipt {
        skill: read_skill_record(&location, &name),
        manifest_path: format!("{}/{}", location.skill_display_path(&name), SKILL_MANIFEST),
        previous_standing: SkillStanding::Retired.as_str().to_owned(),
        standing: SkillStanding::Active.as_str().to_owned(),
        removed_retirement: removed,
        manifest_mutated: true,
        skill_directory_mutated: false,
        skill_bytes_mutated: false,
    })
}

/// Insert ground-relation bindings for every skill under one skills directory.
/// Skills participate with treatment `control-skill`; standing and provenance
/// come from each skill's manifest and are never inferred from paths. Called
/// before the generic tree fallback so one physical source keeps one logical
/// binding under the skill treatment; an explicit accepted ground relation
/// still overrides both, as it does for every other source.
pub(crate) fn insert_skill_bindings(
    world_root: &Path,
    skills_root: &Path,
    world_ref: &str,
    bindings: &mut BTreeMap<String, SourceBinding>,
) -> io::Result<()> {
    for name in child_directories(skills_root)? {
        let skill_dir = skills_root.join(&name);
        let manifest = read_skill_manifest(&skill_dir)?;
        let (provenance, standing) = match &manifest {
            Some(manifest) => (
                manifest.provenance.horizon_str().to_owned(),
                manifest.standing.as_str().to_owned(),
            ),
            None => ("unresolved".to_owned(), "unspecified".to_owned()),
        };
        let mut files = Vec::new();
        collect_files(&skill_dir, world_root, 0, &mut files)?;
        for file in files {
            let relative = normalize_relative(file.strip_prefix(world_root).map_err(|_| {
                io::Error::new(io::ErrorKind::InvalidData, "source escaped its world root")
            })?);
            let reference = source_ref(world_ref, &relative);
            bindings.insert(
                reference.clone(),
                SourceBinding {
                    source_ref: reference,
                    path: relative,
                    roles: vec!["skill-source".to_owned()],
                    provenance: provenance.clone(),
                    standing: standing.clone(),
                    treatment: CONTROL_SKILL_TREATMENT.to_owned(),
                    agent_retrieval_allowed: retrieval_allowed(world_root, &file),
                },
            );
        }
    }
    Ok(())
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

fn optional(input: &Value, field: &str) -> Option<String> {
    input
        .get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn central_root(action: &str, context: &ActionExecutionContext<'_>) -> Result<PathBuf, ActionResult> {
    resolve_central_root(context.root_options).map_err(|message| {
        ActionResult::failure(Some(action), ResultStatus::InvalidInput, message, None)
    }).map(|root| root.path)
}

fn skill_target(
    action: &str,
    input: &Value,
) -> Result<(SkillScope, Option<String>, Option<String>, String), ActionResult> {
    let scope = required(input, "scope", action)
        .and_then(|value| {
            SkillScope::from_input(&value).map_err(|error| {
                ActionResult::failure(Some(action), ResultStatus::InvalidInput, error.to_string(), None)
            })
        });
    let scope = match scope {
        Ok(value) => value,
        Err(result) => return Err(result),
    };
    let machine = optional(input, "machine");
    let project = optional(input, "project");
    let name = match required(input, "name", action) {
        Ok(value) => value,
        Err(result) => return Err(result),
    };
    Ok((scope, machine, project, name))
}

fn io_failure(action: &str, error: io::Error) -> ActionResult {
    let status = match error.kind() {
        io::ErrorKind::InvalidInput | io::ErrorKind::NotFound => ResultStatus::InvalidInput,
        io::ErrorKind::InvalidData => ResultStatus::VerificationFailure,
        _ => ResultStatus::InternalFailure,
    };
    ActionResult::failure(Some(action), status, error.to_string(), None)
}

fn inspect_action(
    _: &ActionRegistry,
    _: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let action = "control.skills.inspect";
    let root = match central_root(action, context) {
        Ok(value) => value,
        Err(result) => return result,
    };
    inspect_control_skills(&root)
        .map(|value| {
            ActionResult::success(
                action,
                serde_json::to_value(value).expect("skills inspection serializes"),
            )
        })
        .unwrap_or_else(|error| io_failure(action, error))
}

fn retire_action(
    _: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let action = "control.skills.retire";
    let (scope, machine, project, name) = match skill_target(action, input) {
        Ok(value) => value,
        Err(result) => return result,
    };
    let retired_by = match required(input, "retired_by", action) {
        Ok(value) => value,
        Err(result) => return result,
    };
    let retirement_reason = match required(input, "retirement_reason", action) {
        Ok(value) => value,
        Err(result) => return result,
    };
    let root = match central_root(action, context) {
        Ok(value) => value,
        Err(result) => return result,
    };
    retire_skill(
        &root,
        scope,
        machine.as_deref(),
        project.as_deref(),
        &name,
        // Declared by the caller, recorded verbatim: a retirement declaration
        // never becomes inferred authorship.
        &retired_by,
        &retirement_reason,
    )
    .map(|value| {
        ActionResult::success(
            action,
            serde_json::to_value(value).expect("skill retire receipt serializes"),
        )
    })
    .unwrap_or_else(|error| io_failure(action, error))
}

fn restore_action(
    _: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let action = "control.skills.restore";
    let (scope, machine, project, name) = match skill_target(action, input) {
        Ok(value) => value,
        Err(result) => return result,
    };
    let root = match central_root(action, context) {
        Ok(value) => value,
        Err(result) => return result,
    };
    restore_skill(&root, scope, machine.as_deref(), project.as_deref(), &name)
        .map(|value| {
            ActionResult::success(
                action,
                serde_json::to_value(value).expect("skill restore receipt serializes"),
            )
        })
        .unwrap_or_else(|error| io_failure(action, error))
}

pub fn register_control_skills_actions(registry: &mut ActionRegistry) {
    let scope_choices = [SCOPE_CONTROL_USER, SCOPE_CONTROL_MACHINE, SCOPE_PROJECTCENTRAL_USER];
    registry
        .register(
            descriptor(
                "control.skills.inspect",
                "Inspect skill ground",
                "Disclose the authored skill surface at every Control scope (personal, machine, ProjectCentral) with scope, standing, provenance and retirement records; empty scopes are disclosed honestly as absent, never repaired.",
                MutationClass::ReadOnly,
                "control-skills-inspection",
                vec![],
            ),
            inspect_action,
        )
        .expect("Control skills Action ids are valid");
    registry
        .register(
            descriptor(
                "control.skills.retire",
                "Retire a skill",
                "Write standing retired into a skill's ground manifest with who/when/why provenance. The skill directory and body are left untouched; retirement is an auditable, reversible standing, not a deletion.",
                MutationClass::LocallyMutating,
                "control-skill-retirement",
                vec![
                    action_input("scope", true, Some(&scope_choices)),
                    action_input("machine", false, None),
                    action_input("project", false, None),
                    action_input("name", true, None),
                    action_input("retired_by", true, None),
                    action_input("retirement_reason", true, None),
                ],
            ),
            retire_action,
        )
        .expect("Control skills Action ids are valid");
    registry
        .register(
            descriptor(
                "control.skills.restore",
                "Restore a retired skill",
                "Reverse a retirement: return the skill's manifest to standing active and clear the retirement record (earlier states remain in source history). Refuses a skill that is not retired.",
                MutationClass::LocallyMutating,
                "control-skill-restoration",
                vec![
                    action_input("scope", true, Some(&scope_choices)),
                    action_input("machine", false, None),
                    action_input("project", false, None),
                    action_input("name", true, None),
                ],
            ),
            restore_action,
        )
        .expect("Control skills Action ids are valid");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_central(label: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "central-skills-{label}-{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn seed_skill(root: &Path, relative_skill_dir: &str) -> PathBuf {
        let skill_dir = root.join(relative_skill_dir);
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(skill_dir.join(SKILL_BODY), "---\nname: seeded\n---\nBody.\n").unwrap();
        let manifest = SkillManifest {
            schema: SKILL_MANIFEST_SCHEMA.to_owned(),
            name: skill_dir
                .file_name()
                .unwrap()
                .to_string_lossy()
                .into_owned(),
            scope: SkillScope::ControlUser,
            standing: SkillStanding::Active,
            provenance: SkillProvenance::HumanAuthored,
            retirement: None,
            extra: BTreeMap::new(),
        };
        // Seed through a JSON document so unknown-field preservation is proven
        // against real authored bytes, not the typed round-trip alone.
        let raw = serde_json::to_value(&manifest).unwrap();
        let mut authored = raw.as_object().unwrap().clone();
        authored.insert("authored_note".to_owned(), Value::String("keep me".to_owned()));
        fs::write(
            skill_dir.join(SKILL_MANIFEST),
            serde_json::to_vec_pretty(&Value::Object(authored)).unwrap(),
        )
        .unwrap();
        skill_dir
    }

    #[test]
    fn inspect_discloses_a_seed_skill_with_scope_and_honest_absence() {
        let central = temp_central("inspect");
        seed_skill(&central, "Control/user/skills/central-ground-keeping");

        let inspection = inspect_control_skills(&central).unwrap();
        let skill = inspection
            .skills
            .iter()
            .find(|skill| skill.name == "central-ground-keeping")
            .unwrap();
        assert_eq!(skill.scope, "control-user");
        assert_eq!(skill.standing, "active");
        assert_eq!(skill.provenance, "human-authored");
        assert_eq!(skill.path, "Control/user/skills/central-ground-keeping");
        assert_eq!(
            skill.body_source_ref.as_deref(),
            Some("central:source:control:root:Control/user/skills/central-ground-keeping/SKILL.md")
        );
        assert!(skill.faults.is_empty());
        assert!(skill.retirement.is_none());
        assert_eq!(inspection.active_skills, 1);

        // Honest absence: no machine scope and no project scope exist yet.
        let machine_surface = inspection
            .scopes
            .iter()
            .find(|surface| surface.scope == "control-machine")
            .unwrap();
        assert!(!machine_surface.exists);
        assert!(machine_surface.skill_names.is_empty());
        let project_surface = inspection
            .scopes
            .iter()
            .find(|surface| surface.scope == "projectcentral-user")
            .unwrap();
        assert!(!project_surface.exists);
        assert!(!inspection.projection_policy.retired_standing_projects);
        let _ = fs::remove_dir_all(&central);
    }

    #[test]
    fn skill_without_a_manifest_is_disclosed_unresolved_never_inferred() {
        let central = temp_central("unresolved");
        let skill_dir = central.join("Control/user/skills/anonymous");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(skill_dir.join(SKILL_BODY), "body\n").unwrap();

        let inspection = inspect_control_skills(&central).unwrap();
        let skill = &inspection.skills[0];
        assert_eq!(skill.standing, "unresolved");
        assert_eq!(skill.provenance, "unresolved");
        assert!(!skill.manifest_present);
        assert_eq!(inspection.unresolved_skills, 1);
        assert!(inspection
            .next_actions
            .iter()
            .any(|action| action.contains("no valid ground manifest")));

        let refusal = retire_skill(
            &central,
            SkillScope::ControlUser,
            None,
            None,
            "anonymous",
            "owner",
            "no longer earns ground",
        )
        .unwrap_err();
        assert_eq!(refusal.kind(), io::ErrorKind::InvalidInput);
        assert!(refusal.to_string().contains("no ground manifest"));
        let _ = fs::remove_dir_all(&central);
    }

    #[test]
    fn retire_writes_standing_and_provenance_into_the_manifest_on_disk() {
        let central = temp_central("retire");
        seed_skill(&central, "Control/user/skills/central-ground-keeping");

        let receipt = retire_skill(
            &central,
            SkillScope::ControlUser,
            None,
            None,
            "central-ground-keeping",
            "owner-in-session",
            "superseded by the ontology skill",
        )
        .unwrap();
        assert_eq!(receipt.previous_standing, "active");
        assert_eq!(receipt.standing, "retired");
        assert!(receipt.manifest_mutated);
        assert!(!receipt.skill_bytes_mutated);
        assert!(!receipt.skill_directory_mutated);
        assert_eq!(receipt.skill.standing, "retired");
        let record = receipt.skill.retirement.as_ref().unwrap();
        assert_eq!(record.retired_by, "owner-in-session");
        assert_eq!(record.retirement_reason, "superseded by the ontology skill");
        assert!(record.retired_at_unix_seconds > 0);

        // The manifest on disk carries the record, and authored extras survive.
        let disk: Value = serde_json::from_slice(
            &fs::read(central.join("Control/user/skills/central-ground-keeping/skill.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(disk["schema"], SKILL_MANIFEST_SCHEMA);
        assert_eq!(disk["standing"], "retired");
        assert_eq!(disk["retirement"]["retired_by"], "owner-in-session");
        assert_eq!(disk["retirement"]["retirement_reason"], "superseded by the ontology skill");
        assert_eq!(disk["authored_note"], "keep me");
        // The skill body is untouched.
        assert_eq!(
            fs::read_to_string(central.join("Control/user/skills/central-ground-keeping/SKILL.md")).unwrap(),
            "---\nname: seeded\n---\nBody.\n"
        );

        // Retiring twice refuses with the existing provenance.
        let second = retire_skill(
            &central,
            SkillScope::ControlUser,
            None,
            None,
            "central-ground-keeping",
            "owner-in-session",
            "again",
        )
        .unwrap_err();
        assert_eq!(second.kind(), io::ErrorKind::InvalidInput);
        assert!(second.to_string().contains("already retired"));
        assert!(second.to_string().contains("owner-in-session"));

        // A nonexistent skill refuses with NotFound.
        let missing = retire_skill(
            &central,
            SkillScope::ControlUser,
            None,
            None,
            "never-authored",
            "owner-in-session",
            "missing",
        )
        .unwrap_err();
        assert_eq!(missing.kind(), io::ErrorKind::NotFound);
        let _ = fs::remove_dir_all(&central);
    }

    #[test]
    fn restore_reverses_retirement_and_refuses_an_active_skill() {
        let central = temp_central("restore");
        seed_skill(&central, "Control/user/skills/central-ground-keeping");

        let refusal = restore_skill(&central, SkillScope::ControlUser, None, None, "central-ground-keeping")
            .unwrap_err();
        assert_eq!(refusal.kind(), io::ErrorKind::InvalidInput);
        assert!(refusal.to_string().contains("active"));

        retire_skill(
            &central,
            SkillScope::ControlUser,
            None,
            None,
            "central-ground-keeping",
            "owner-in-session",
            "superseded",
        )
        .unwrap();
        let receipt =
            restore_skill(&central, SkillScope::ControlUser, None, None, "central-ground-keeping")
                .unwrap();
        assert_eq!(receipt.previous_standing, "retired");
        assert_eq!(receipt.standing, "active");
        assert_eq!(receipt.removed_retirement.as_ref().unwrap().retirement_reason, "superseded");
        assert!(receipt.skill.retirement.is_none());

        let disk: Value = serde_json::from_slice(
            &fs::read(central.join("Control/user/skills/central-ground-keeping/skill.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(disk["standing"], "active");
        assert!(disk.get("retirement").is_none());
        let _ = fs::remove_dir_all(&central);
    }

    #[test]
    fn scope_and_name_mismatches_refuse_rather_than_rewrite_authored_ground() {
        let central = temp_central("mismatch");
        let skill_dir = central.join("Control/machines/primary-workstation/skills/brandkit");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join(SKILL_MANIFEST),
            serde_json::to_vec_pretty(&json_manifest("brandkit", SkillScope::ControlUser)).unwrap(),
        )
        .unwrap();

        let scope_mismatch = retire_skill(
            &central,
            SkillScope::ControlMachine,
            Some("primary-workstation"),
            None,
            "brandkit",
            "owner",
            "test",
        )
        .unwrap_err();
        assert_eq!(scope_mismatch.kind(), io::ErrorKind::InvalidData);
        assert!(scope_mismatch.to_string().contains("does not match location"));

        let inspection = inspect_control_skills(&central).unwrap();
        let skill = inspection.skills[0].clone();
        assert_eq!(skill.scope, "control-machine");
        assert!(skill.faults.iter().any(|fault| fault.contains("does not match location")));
        let _ = fs::remove_dir_all(&central);
    }

    #[test]
    fn ref_grammar_addresses_every_scope_through_the_canonical_source_ref() {
        let central = temp_central("refs");
        let personal = resolve_skill_location(&central, SkillScope::ControlUser, None, None).unwrap();
        assert_eq!(
            personal.body_source_ref("central-ground-keeping"),
            "central:source:control:root:Control/user/skills/central-ground-keeping/SKILL.md"
        );

        let machine = resolve_skill_location(
            &central,
            SkillScope::ControlMachine,
            Some("primary-workstation"),
            None,
        )
        .unwrap();
        assert_eq!(
            machine.body_source_ref("brandkit"),
            "central:source:control:root:Control/machines/primary-workstation/skills/brandkit/SKILL.md"
        );

        let project = temp_central("refs-project");
        let project_root = project.join("Work/Suite");
        fs::create_dir_all(project_root.join("ProjectCentral")).unwrap();
        fs::write(
            project_root.join("ProjectCentral/project.json"),
            serde_json::to_vec_pretty(&serde_json::json!({
                "schema": "central.project/v1",
                "project_id": "example/suite",
                "human_source": "ProjectCentral/user",
                "wiki": { "profile": "okf-wiki/v1", "source": "ProjectCentral/agents/wiki/wiki.json" }
            }))
            .unwrap(),
        )
        .unwrap();
        let location =
            resolve_skill_location(&project, SkillScope::ProjectcentralUser, None, Some("Suite")).unwrap();
        assert_eq!(location.world_ref, "project:example/suite");
        assert_eq!(
            location.body_source_ref("suite-operator"),
            "central:source:project:example/suite:ProjectCentral/user/skills/suite-operator/SKILL.md"
        );
        // Without a manifest the ref falls back to the project directory name,
        // exactly as ProjectCentral ground inspection does.
        fs::remove_file(project_root.join("ProjectCentral/project.json")).unwrap();
        let fallback =
            resolve_skill_location(&project, SkillScope::ProjectcentralUser, None, Some("Suite")).unwrap();
        assert_eq!(fallback.world_ref, "project:Suite");
        let _ = fs::remove_dir_all(&central);
        let _ = fs::remove_dir_all(&project);
    }

    fn json_manifest(name: &str, scope: SkillScope) -> Value {
        serde_json::json!({
            "schema": SKILL_MANIFEST_SCHEMA,
            "name": name,
            "scope": scope.as_str(),
            "standing": "active",
            "provenance": "human-authored"
        })
    }
}
