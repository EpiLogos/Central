//! Central-owned default-tree stamping.
//!
//! Central owns the structure behaviour and the base forms of its guidance
//! file types. A stamp lays those defaults down as clearly marked drafts:
//! every stamped file carries "distributed default" standing in its own
//! text, existing files are never read, compared or overwritten, and a
//! repeated stamp is stable. Stamping is opt-in and additive only — it
//! never deletes, moves or rewrites source.

use crate::action::{
    ActionDescriptor, ActionExecutionContext, ActionRegistry, MutationClass,
};
use crate::projectcentral::{read_project_manifest, PROJECTCENTRAL_DIR};
use crate::result::{ActionResult, ResultStatus};
use crate::root::resolve_central_root;
use serde::Serialize;
use serde_json::Value;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const CONTROL_ROOT_MARKER: &str = "Control";

const ROOT_DEFAULTS: &[(&str, &str)] = &[
    (
        "Control/agents/governance/repos/repo-content-and-structure.md",
        include_str!("../defaults/control-governance-repo-content-and-structure.md"),
    ),
    (
        "Control/agents/governance/field-and-now/session-work-placement.md",
        include_str!("../defaults/control-governance-field-and-now-session-work-placement.md"),
    ),
    (
        "Control/agents/governance/field-and-now/day-close.md",
        include_str!("../defaults/control-governance-field-and-now-day-close.md"),
    ),
    (
        "Control/agents/governance/field-and-now/wiki-field-law.md",
        include_str!("../defaults/control-governance-field-and-now-wiki-field-law.md"),
    ),
    (
        "Control/agents/now/policy.json",
        include_str!("../defaults/control-now-policy.json"),
    ),
    (
        "Control/agents/now/promotions.json",
        include_str!("../defaults/control-now-promotions.json"),
    ),
    (
        "Control/agents/now/README.md",
        include_str!("../defaults/control-now-readme.md"),
    ),
];

const PROJECT_DEFAULTS: &[(&str, &str)] = &[
    (
        "ProjectCentral/agents/governance/repo-structure.md",
        include_str!("../defaults/projectcentral-repo-structure.md"),
    ),
    (
        "ProjectCentral/agents/governance/repo-content.md",
        include_str!("../defaults/projectcentral-repo-content.md"),
    ),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StampScope {
    Root,
    Project,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StampFileAction {
    Create,
    SkipExisting,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StampFile {
    pub path: String,
    pub action: StampFileAction,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TemplateStampPlan {
    pub target_root: PathBuf,
    pub scope: StampScope,
    pub files: Vec<StampFile>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TemplateStampResult {
    pub target_root: PathBuf,
    pub scope: StampScope,
    pub created: Vec<String>,
    pub skipped_existing: Vec<String>,
}

fn default_files(scope: StampScope) -> &'static [(&'static str, &'static str)] {
    match scope {
        StampScope::Root => ROOT_DEFAULTS,
        StampScope::Project => PROJECT_DEFAULTS,
    }
}

/// Root scope requires an initialized Central root. The stamp never creates
/// the Control root itself: establishing the world is `central.init`'s law.
fn ensure_root_scope(central_root: &Path) -> io::Result<()> {
    if central_root.join(CONTROL_ROOT_MARKER).is_dir() {
        return Ok(());
    }
    Err(io::Error::new(
        io::ErrorKind::InvalidInput,
        "Central root has no Control root; run central.init before stamping defaults.",
    ))
}

/// Project scope requires ProjectCentral identity. Stamping supplements an
/// established fractal; it does not establish one.
fn ensure_project_scope(project_root: &Path) -> io::Result<()> {
    if !project_root.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Project directory does not exist.",
        ));
    }
    read_project_manifest(project_root).map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "Project has no readable ProjectCentral manifest; run projectcentral.init before stamping defaults: {error}"
            ),
        )
    })?;
    if !project_root.join(PROJECTCENTRAL_DIR).is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Project has a manifest but no ProjectCentral directory.",
        ));
    }
    Ok(())
}

pub fn stamp_plan_for_root(central_root: &Path) -> io::Result<TemplateStampPlan> {
    ensure_root_scope(central_root)?;
    Ok(plan_for(central_root, StampScope::Root))
}

pub fn stamp_plan_for_project(project_root: &Path) -> io::Result<TemplateStampPlan> {
    ensure_project_scope(project_root)?;
    Ok(plan_for(project_root, StampScope::Project))
}

fn plan_for(target_root: &Path, scope: StampScope) -> TemplateStampPlan {
    let files = default_files(scope)
        .iter()
        .map(|(relative, _)| StampFile {
            path: (*relative).to_owned(),
            action: if target_root.join(relative).exists() {
                StampFileAction::SkipExisting
            } else {
                StampFileAction::Create
            },
        })
        .collect();
    TemplateStampPlan {
        target_root: target_root.to_path_buf(),
        scope,
        files,
    }
}

pub fn stamp_root(central_root: &Path) -> io::Result<TemplateStampResult> {
    stamp(central_root, StampScope::Root)
}

pub fn stamp_project(project_root: &Path) -> io::Result<TemplateStampResult> {
    stamp(project_root, StampScope::Project)
}

fn stamp(target_root: &Path, scope: StampScope) -> io::Result<TemplateStampResult> {
    match scope {
        StampScope::Root => ensure_root_scope(target_root)?,
        StampScope::Project => ensure_project_scope(target_root)?,
    }
    let mut created = Vec::new();
    let mut skipped_existing = Vec::new();
    for (relative, content) in default_files(scope) {
        let target = target_root.join(relative);
        if target.exists() {
            skipped_existing.push((*relative).to_owned());
            continue;
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&target, content)?;
        created.push((*relative).to_owned());
    }
    Ok(TemplateStampResult {
        target_root: target_root.to_path_buf(),
        scope,
        created,
        skipped_existing,
    })
}

fn stamp_target(
    action: &str,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> Result<(PathBuf, StampScope), ActionResult> {
    let root = resolve_central_root(context.root_options)
        .map_err(|message| {
            ActionResult::failure(Some(action), ResultStatus::InvalidInput, message, None)
        })?
        .path;
    let project = optional_project(action, input)?;
    match project {
        Some(project) => {
            let project_root = root.join("Work").join(&project);
            Ok((project_root, StampScope::Project))
        }
        None => Ok((root, StampScope::Root)),
    }
}

fn optional_project(action: &str, input: &Value) -> Result<Option<String>, ActionResult> {
    let raw = match input.get("project") {
        None | Some(Value::Null) => return Ok(None),
        Some(value) => value
            .as_str()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                ActionResult::failure(
                    Some(action),
                    ResultStatus::InvalidInput,
                    format!("{action} input project must be a non-empty string when present."),
                    None,
                )
            })?,
    };
    let invalid = |reason: &str| {
        ActionResult::failure(
            Some(action),
            ResultStatus::InvalidInput,
            format!("{action} input project is invalid: {reason}"),
            None,
        )
    };
    if raw == "." || raw == ".." || raw.contains('/') || raw.contains('\\') {
        return Err(invalid("a project is one Work member name"));
    }
    if !raw
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_'))
    {
        return Err(invalid("use letters, digits, '.', '-' and '_' only"));
    }
    Ok(Some(raw.to_owned()))
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

fn stamp_preview_action(_: &ActionRegistry, input: &Value, context: &ActionExecutionContext<'_>) -> ActionResult {
    let action = "central.template.preview";
    let (target_root, scope) = match stamp_target(action, input, context) {
        Ok(value) => value,
        Err(result) => return result,
    };
    let outcome = || -> io::Result<TemplateStampPlan> {
        match scope {
            StampScope::Root => stamp_plan_for_root(&target_root),
            StampScope::Project => stamp_plan_for_project(&target_root),
        }
    }();
    outcome
        .map(|value| {
            ActionResult::success(action, serde_json::to_value(value).expect("plan serializes"))
        })
        .unwrap_or_else(|error| io_failure(action, error))
}

fn stamp_action(_: &ActionRegistry, input: &Value, context: &ActionExecutionContext<'_>) -> ActionResult {
    let action = "central.template.stamp";
    let (target_root, scope) = match stamp_target(action, input, context) {
        Ok(value) => value,
        Err(result) => return result,
    };
    let outcome = || -> io::Result<TemplateStampResult> {
        match scope {
            StampScope::Root => stamp_root(&target_root),
            StampScope::Project => stamp_project(&target_root),
        }
    }();
    outcome
        .map(|value| {
            ActionResult::success(action, serde_json::to_value(value).expect("result serializes"))
        })
        .unwrap_or_else(|error| io_failure(action, error))
}

fn descriptor(
    id: &str,
    title: &str,
    description: &str,
    mutation_class: MutationClass,
    output_type: &str,
    preview_supported: bool,
) -> ActionDescriptor {
    ActionDescriptor {
        id: id.to_owned(),
        title: title.to_owned(),
        description: description.to_owned(),
        inputs: vec![crate::action::ActionInputDefinition {
            name: "project".to_owned(),
            input_type: "string".to_owned(),
            required: false,
            choices: None,
            selection: None,
        }],
        output: crate::action::ActionOutputDefinition { output_type: output_type.to_owned() },
        mutation_class,
        preview_supported,
        required_ports: vec![],
        availability: crate::action::ActionAvailability { available: true, reason: None },
    }
}

pub fn register_template_stamp_actions(registry: &mut ActionRegistry) {
    let actions: [(ActionDescriptor, crate::action::ActionHandler); 2] = [
        (
            descriptor(
                "central.template.preview",
                "Preview default-tree stamp",
                "Preview which Central-owned default sources a stamp would create or skip at the Central root or a named Work project, without mutation.",
                MutationClass::ReadOnly,
                "central-template-stamp-plan",
                false,
            ),
            stamp_preview_action,
        ),
        (
            descriptor(
                "central.template.stamp",
                "Stamp default tree",
                "Create missing Central-owned default sources as marked distributed drafts: the repo guidance protocol, the field-and-now governance statements and the root NOW/DAY field skeleton at Control scope; repo-structure/repo-content starters at ProjectCentral scope. Never overwrites, moves or deletes existing source; a repeated stamp is stable.",
                MutationClass::LocallyMutating,
                "central-template-stamp",
                true,
            ),
            stamp_action,
        ),
    ];
    for (descriptor, handler) in actions {
        registry
            .register(descriptor, handler)
            .expect("template stamp Action ids are valid");
    }
}
