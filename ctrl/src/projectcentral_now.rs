use crate::action::{
    ActionAvailability, ActionDescriptor, ActionExecutionContext, ActionInputDefinition,
    ActionOutputDefinition, ActionRegistry, MutationClass,
};
use crate::projectcentral::{read_project_manifest, HUMAN_SOURCE_DIR};
use crate::result::{ActionResult, ResultStatus};
use crate::root::resolve_central_root;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub const NOW_DIR: &str = "ProjectCentral/now";
pub const NOW_USER_DIR: &str = "ProjectCentral/now/user";
pub const NOW_AGENT_DIR: &str = "ProjectCentral/now/agents";
pub const NOW_DAY_DIR: &str = "ProjectCentral/now/day";
pub const NOW_POLICY: &str = "ProjectCentral/now/policy.json";
pub const NOW_PROMOTIONS: &str = "ProjectCentral/now/promotions.json";
pub const WIKI_RETURN_DIR: &str = "ProjectCentral/agents/wiki/returns";

const POLICY_SCHEMA: &str = "central.project-now.policy/v1";
const HANDOFF_SCHEMA: &str = "central.project-now.handoff/v1";
const PROMOTIONS_SCHEMA: &str = "central.project-now.promotions/v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NowPolicy {
    pub schema: String,
    pub carry_statuses: Vec<String>,
    pub remove_statuses: Vec<String>,
    pub protect_when_preserve_refs_exist: bool,
    pub human_scratch_cleanup: String,
    pub day_boundary: String,
}

impl Default for NowPolicy {
    fn default() -> Self {
        Self {
            schema: POLICY_SCHEMA.into(),
            carry_statuses: vec!["active".into(), "waiting".into(), "carried".into()],
            remove_statuses: vec!["resolved".into(), "expired".into(), "promoted".into()],
            protect_when_preserve_refs_exist: true,
            human_scratch_cleanup: "human-owned-manual".into(),
            day_boundary: "caller-supplied-local-civil-date".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NowHandoff {
    pub schema: String,
    pub id: String,
    pub provenance: String,
    pub actor: String,
    pub kind: String,
    pub recorded_at_unix_seconds: u64,
    pub subject: String,
    pub result: String,
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub focus_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_refs: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evidence_refs: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub preserve_refs: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub carried_from_days: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub promoted_to: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PromotionReceipt {
    pub source: String,
    pub target: String,
    pub destination: String,
    pub acceptance: String,
    pub recorded_at_unix_seconds: u64,
    pub source_preserved: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct PromotionLedger {
    schema: String,
    entries: Vec<PromotionReceipt>,
}

impl Default for PromotionLedger {
    fn default() -> Self {
        Self {
            schema: PROMOTIONS_SCHEMA.into(),
            entries: vec![],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NowPaths {
    pub root: PathBuf,
    pub user: PathBuf,
    pub agents: PathBuf,
    pub day: PathBuf,
    pub policy: PathBuf,
    pub promotions: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NowInspection {
    pub project_root: PathBuf,
    pub exists: bool,
    pub paths: NowPaths,
    pub policy: Option<NowPolicy>,
    pub human_scratch: Vec<String>,
    pub active_items: Vec<NowHandoff>,
    pub open_questions: Vec<NowHandoff>,
    pub inactive_items: Vec<NowHandoff>,
    pub invalid_items: Vec<String>,
    pub day_records: Vec<String>,
    pub promotions: Vec<PromotionReceipt>,
    pub boundaries: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NowPromotion {
    pub source: String,
    pub destination: String,
    pub target: String,
    pub source_preserved: bool,
    pub semantic_effect: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RolloverReport {
    pub day: String,
    pub next_day: String,
    pub day_record: String,
    pub day_sources: String,
    pub carried: Vec<String>,
    pub removed: Vec<String>,
    pub protected: Vec<String>,
    pub human_scratch: Vec<String>,
    pub promotions: Vec<PromotionReceipt>,
    pub cleanup_failures: Vec<String>,
}

fn now_paths(project_root: &Path) -> NowPaths {
    NowPaths {
        root: project_root.join(NOW_DIR),
        user: project_root.join(NOW_USER_DIR),
        agents: project_root.join(NOW_AGENT_DIR),
        day: project_root.join(NOW_DAY_DIR),
        policy: project_root.join(NOW_POLICY),
        promotions: project_root.join(NOW_PROMOTIONS),
    }
}

fn input(name: &str, required: bool) -> ActionInputDefinition {
    ActionInputDefinition {
        name: name.into(),
        input_type: "string".into(),
        required,
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
    inputs: &[(&str, bool)],
) -> ActionDescriptor {
    ActionDescriptor {
        id: id.into(),
        title: title.into(),
        description: description.into(),
        inputs: inputs
            .iter()
            .map(|(name, required)| input(name, *required))
            .collect(),
        output: ActionOutputDefinition {
            output_type: output_type.into(),
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

fn string_array(input: &Value, field: &str, action: &str) -> Result<Vec<String>, ActionResult> {
    let Some(value) = input.get(field) else {
        return Ok(vec![]);
    };
    if let Some(value) = value.as_str().map(str::trim).filter(|value| !value.is_empty()) {
        return Ok(vec![value.to_owned()]);
    }
    let Some(values) = value.as_array() else {
        return Err(ActionResult::failure(
            Some(action),
            ResultStatus::InvalidInput,
            format!("{action} field {field} must be a string or array of strings."),
            None,
        ));
    };
    let mut output = Vec::with_capacity(values.len());
    for value in values {
        let Some(value) = value.as_str().map(str::trim).filter(|value| !value.is_empty()) else {
            return Err(ActionResult::failure(
                Some(action),
                ResultStatus::InvalidInput,
                format!("{action} field {field} must contain only non-empty strings."),
                None,
            ));
        };
        output.push(value.to_owned());
    }
    Ok(output)
}

fn relative_member(raw: &str) -> io::Result<PathBuf> {
    if raw.trim().is_empty() || raw != raw.trim() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "path must be non-empty without surrounding whitespace",
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
            "path must be project-relative and contain no parent/root components",
        ));
    }
    Ok(path.to_path_buf())
}

fn validate_id(raw: &str) -> io::Result<()> {
    let path = relative_member(raw)?;
    if path.components().count() != 1 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "handoff id must be one filesystem-safe path component",
        ));
    }
    Ok(())
}

fn reject_symlink_components(project_root: &Path, relative: &Path) -> io::Result<()> {
    let mut current = project_root.to_path_buf();
    for component in relative.components() {
        let Component::Normal(component) = component else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "path contains a non-normal component",
            ));
        };
        current.push(component);
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("refusing symlink path component: {}", current.display()),
                ));
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => break,
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

fn project_context(
    action: &str,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> Result<PathBuf, ActionResult> {
    let project = required(input, "project", action)?;
    let project = relative_member(&project).map_err(|error| {
        ActionResult::failure(
            Some(action),
            ResultStatus::InvalidInput,
            error.to_string(),
            None,
        )
    })?;
    let root = resolve_central_root(context.root_options)
        .map_err(|message| {
            ActionResult::failure(
                Some(action),
                ResultStatus::InvalidInput,
                message,
                None,
            )
        })?
        .path;
    let project_root = root.join("Work").join(project);
    if !project_root.is_dir() {
        return Err(ActionResult::failure(
            Some(action),
            ResultStatus::InvalidInput,
            format!(
                "Project root does not exist as a directory: {}",
                project_root.display()
            ),
            None,
        ));
    }
    let manifest = read_project_manifest(&project_root).map_err(|error| {
        ActionResult::failure(
            Some(action),
            ResultStatus::InvalidCentralStructure,
            format!("NOW requires an existing valid ProjectCentral: {error}"),
            None,
        )
    })?;
    let validation = manifest.validate();
    if !validation.valid {
        return Err(ActionResult::failure(
            Some(action),
            ResultStatus::VerificationFailure,
            "NOW requires a valid ProjectCentral manifest.",
            Some(json!({"errors": validation.errors})),
        ));
    }
    Ok(project_root)
}

fn unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn unique_id(prefix: &str) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("{prefix}-{nanos}")
}

fn relative(project_root: &Path, path: &Path) -> String {
    path.strip_prefix(project_root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn write_json(path: &Path, value: &impl Serialize, overwrite: bool) -> io::Result<()> {
    if !overwrite && path.exists() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!("refusing to overwrite existing file: {}", path.display()),
        ));
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut bytes = serde_json::to_vec_pretty(value)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    bytes.push(b'\n');
    fs::write(path, bytes)
}

fn read_policy(path: &Path) -> io::Result<NowPolicy> {
    let policy: NowPolicy = serde_json::from_slice(&fs::read(path)?)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    if policy.schema != POLICY_SCHEMA {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("NOW policy schema must be {POLICY_SCHEMA}"),
        ));
    }
    Ok(policy)
}

fn read_handoff(path: &Path) -> io::Result<NowHandoff> {
    let handoff: NowHandoff = serde_json::from_slice(&fs::read(path)?)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    if handoff.schema != HANDOFF_SCHEMA {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("NOW handoff schema must be {HANDOFF_SCHEMA}"),
        ));
    }
    validate_kind(&handoff.kind)?;
    validate_status(&handoff.status)?;
    validate_id(&handoff.id)?;
    Ok(handoff)
}

fn write_handoff(project_root: &Path, handoff: &NowHandoff) -> io::Result<PathBuf> {
    validate_id(&handoff.id)?;
    let path = project_root
        .join(NOW_AGENT_DIR)
        .join(format!("{}.json", handoff.id));
    write_json(&path, handoff, true)?;
    Ok(path)
}

fn read_promotions(path: &Path) -> io::Result<PromotionLedger> {
    if !path.exists() {
        return Ok(PromotionLedger::default());
    }
    let ledger: PromotionLedger = serde_json::from_slice(&fs::read(path)?)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    if ledger.schema != PROMOTIONS_SCHEMA {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("NOW promotions schema must be {PROMOTIONS_SCHEMA}"),
        ));
    }
    Ok(ledger)
}

fn append_promotion(path: &Path, receipt: PromotionReceipt) -> io::Result<()> {
    let mut ledger = read_promotions(path)?;
    ledger.entries.push(receipt);
    write_json(path, &ledger, true)
}

fn list_files(root: &Path, project_root: &Path) -> io::Result<Vec<String>> {
    fn visit(root: &Path, project_root: &Path, output: &mut Vec<String>) -> io::Result<()> {
        if !root.exists() {
            return Ok(());
        }
        let mut entries = fs::read_dir(root)?.collect::<Result<Vec<_>, _>>()?;
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let path = entry.path();
            let file_type = entry.file_type()?;
            if file_type.is_dir() {
                visit(&path, project_root, output)?;
            } else if file_type.is_file() {
                output.push(relative(project_root, &path));
            }
        }
        Ok(())
    }

    let mut output = vec![];
    visit(root, project_root, &mut output)?;
    Ok(output)
}

fn list_day_records(day_root: &Path, project_root: &Path) -> io::Result<Vec<String>> {
    if !day_root.exists() {
        return Ok(vec![]);
    }
    let mut records = fs::read_dir(day_root)?
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .filter_map(|entry| {
            let path = entry.path();
            match entry.file_type() {
                Ok(file_type)
                    if file_type.is_file()
                        && path.extension().and_then(|value| value.to_str()) == Some("md") =>
                {
                    Some(relative(project_root, &path))
                }
                _ => None,
            }
        })
        .collect::<Vec<_>>();
    records.sort();
    Ok(records)
}

fn boundaries() -> Vec<String> {
    vec![
        "NOW is a moving session-independent working horizon, not a Session, Run, Focus, Wiki, or authored Project canon.".into(),
        "DAY is a dated aggregation/rollover boundary, not a Project history database or automatic truth promotion mechanism.".into(),
        "Run, Session, Focus, source, evidence, and other external identities remain refs owned by their native systems.".into(),
    ]
}

pub fn inspect_now(project_root: &Path) -> io::Result<NowInspection> {
    let paths = now_paths(project_root);
    if !paths.root.exists() {
        return Ok(NowInspection {
            project_root: project_root.to_path_buf(),
            exists: false,
            paths,
            policy: None,
            human_scratch: vec![],
            active_items: vec![],
            open_questions: vec![],
            inactive_items: vec![],
            invalid_items: vec![],
            day_records: vec![],
            promotions: vec![],
            boundaries: boundaries(),
        });
    }

    let policy = read_policy(&paths.policy)?;
    let human_scratch = list_files(&paths.user, project_root)?;
    let mut active_items = vec![];
    let mut open_questions = vec![];
    let mut inactive_items = vec![];
    let mut invalid_items = vec![];

    if paths.agents.exists() {
        let mut entries = fs::read_dir(&paths.agents)?.collect::<Result<Vec<_>, _>>()?;
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let path = entry.path();
            if !entry.file_type()?.is_file()
                || path.extension().and_then(|value| value.to_str()) != Some("json")
            {
                continue;
            }
            match read_handoff(&path) {
                Ok(handoff) => {
                    let active = policy
                        .carry_statuses
                        .iter()
                        .any(|status| status == &handoff.status);
                    if active {
                        if handoff.kind == "question" {
                            open_questions.push(handoff.clone());
                        }
                        active_items.push(handoff);
                    } else {
                        inactive_items.push(handoff);
                    }
                }
                Err(error) => {
                    invalid_items.push(format!("{}: {error}", relative(project_root, &path)))
                }
            }
        }
    }

    Ok(NowInspection {
        project_root: project_root.to_path_buf(),
        exists: true,
        day_records: list_day_records(&paths.day, project_root)?,
        promotions: read_promotions(&paths.promotions)?.entries,
        paths,
        policy: Some(policy),
        human_scratch,
        active_items,
        open_questions,
        inactive_items,
        invalid_items,
        boundaries: boundaries(),
    })
}

pub fn initialize_now(project_root: &Path) -> io::Result<NowInspection> {
    let paths = now_paths(project_root);
    if paths.root.exists() {
        return inspect_now(project_root);
    }
    fs::create_dir_all(&paths.user)?;
    fs::create_dir_all(&paths.agents)?;
    fs::create_dir_all(&paths.day)?;
    write_json(&paths.policy, &NowPolicy::default(), false)?;
    write_json(&paths.promotions, &PromotionLedger::default(), false)?;
    inspect_now(project_root)
}

fn validate_status(status: &str) -> io::Result<()> {
    if matches!(
        status,
        "active" | "waiting" | "resolved" | "carried" | "promoted" | "expired"
    ) {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "status must be active, waiting, resolved, carried, promoted, or expired",
        ))
    }
}

fn validate_kind(kind: &str) -> io::Result<()> {
    if matches!(kind, "handoff" | "question" | "note" | "learning") {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "kind must be handoff, question, note, or learning",
        ))
    }
}

fn create_handoff(input: &Value, project_root: &Path, action: &str) -> Result<NowHandoff, ActionResult> {
    let actor = required(input, "actor", action)?;
    let kind = required(input, "kind", action)?;
    let subject = required(input, "subject", action)?;
    let result = required(input, "result", action)?;
    let status = required(input, "status", action)?;
    validate_kind(&kind).map_err(|error| {
        ActionResult::failure(
            Some(action),
            ResultStatus::InvalidInput,
            error.to_string(),
            None,
        )
    })?;
    validate_status(&status).map_err(|error| {
        ActionResult::failure(
            Some(action),
            ResultStatus::InvalidInput,
            error.to_string(),
            None,
        )
    })?;

    let id = optional(input, "id").unwrap_or_else(|| unique_id("handoff"));
    validate_id(&id).map_err(|error| {
        ActionResult::failure(
            Some(action),
            ResultStatus::InvalidInput,
            error.to_string(),
            None,
        )
    })?;
    let path = project_root
        .join(NOW_AGENT_DIR)
        .join(format!("{id}.json"));
    if path.exists() {
        return Err(ActionResult::failure(
            Some(action),
            ResultStatus::InvalidInput,
            format!("NOW handoff already exists: {id}"),
            None,
        ));
    }

    Ok(NowHandoff {
        schema: HANDOFF_SCHEMA.into(),
        id,
        provenance: "agent-authored-bounded-return".into(),
        actor,
        kind,
        recorded_at_unix_seconds: unix_seconds(),
        subject,
        result,
        status,
        run_ref: optional(input, "run_ref"),
        session_ref: optional(input, "session_ref"),
        focus_ref: optional(input, "focus_ref"),
        source_refs: string_array(input, "source_refs", action)?,
        evidence_refs: string_array(input, "evidence_refs", action)?,
        preserve_refs: string_array(input, "preserve_refs", action)?,
        carried_from_days: vec![],
        promoted_to: vec![],
    })
}

fn parse_day(value: &str) -> io::Result<(u32, u32, u32)> {
    let bytes = value.as_bytes();
    let shape = bytes.len() == 10
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| index == 4 || index == 7 || byte.is_ascii_digit());
    if !shape {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "day must use YYYY-MM-DD local civil date form",
        ));
    }
    let year = value[0..4].parse::<u32>().map_err(|_| {
        io::Error::new(io::ErrorKind::InvalidInput, "invalid DAY year")
    })?;
    let month = value[5..7].parse::<u32>().map_err(|_| {
        io::Error::new(io::ErrorKind::InvalidInput, "invalid DAY month")
    })?;
    let day = value[8..10].parse::<u32>().map_err(|_| {
        io::Error::new(io::ErrorKind::InvalidInput, "invalid DAY day")
    })?;
    if year == 0 || !(1..=12).contains(&month) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "DAY must be a valid civil date",
        ));
    }
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let max_day = match month {
        2 if leap => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    };
    if day == 0 || day > max_day {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "DAY must be a valid civil date",
        ));
    }
    Ok((year, month, day))
}

fn path_after_root(source: &str, root: &str) -> io::Result<PathBuf> {
    Path::new(source)
        .strip_prefix(root)
        .map(Path::to_path_buf)
        .map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("{source} is not inside {root}"),
            )
        })
}

fn snapshot_day_sources(
    project_root: &Path,
    day_root: &Path,
    day: &str,
    human_scratch: &[String],
    handoffs: &[(String, NowHandoff)],
) -> io::Result<PathBuf> {
    let snapshot_root = day_root.join(format!("{day}.sources"));
    if snapshot_root.exists() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!("DAY source snapshot already exists: {day}"),
        ));
    }
    fs::create_dir(&snapshot_root)?;

    let copy_result = (|| -> io::Result<()> {
        for source in human_scratch {
            let suffix = path_after_root(source, NOW_USER_DIR)?;
            let source_path = project_root.join(source);
            let destination = snapshot_root.join("user").join(suffix);
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(source_path, destination)?;
        }
        for (source, _) in handoffs {
            let suffix = path_after_root(source, NOW_AGENT_DIR)?;
            let source_path = project_root.join(source);
            let destination = snapshot_root.join("agents").join(suffix);
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(source_path, destination)?;
        }
        Ok(())
    })();

    if let Err(error) = copy_result {
        let _ = fs::remove_dir_all(&snapshot_root);
        return Err(error);
    }
    Ok(snapshot_root)
}

fn indented(text: &str) -> String {
    if text.is_empty() {
        return "    <empty>\n".into();
    }
    text.lines()
        .map(|line| format!("    {line}\n"))
        .collect::<String>()
}

fn snapshot_ref(project_root: &Path, snapshot_root: &Path, class: &str, suffix: &Path) -> String {
    relative(project_root, &snapshot_root.join(class).join(suffix))
}

fn render_day(
    project_root: &Path,
    snapshot_root: &Path,
    day: &str,
    next_day: &str,
    human_scratch: &[String],
    handoffs: &[(String, NowHandoff)],
    carried: &[String],
    removed: &[String],
    protected: &[String],
    promotions: &[PromotionReceipt],
) -> io::Result<String> {
    let mut output = format!(
        "# DAY — {day}\n\nDerived closure reading for the ProjectCentral NOW horizon. Human and Agent authorship remain attached to separately snapshotted source records; this aggregation is not Project canon.\n\n- next local civil day: `{next_day}`\n- DAY source snapshot: `{}`\n- NOW remains the moving working horizon after this boundary\n\n## Human current source at close\n\n",
        relative(project_root, snapshot_root)
    );

    if human_scratch.is_empty() {
        output.push_str("- none\n");
    } else {
        for source in human_scratch {
            let suffix = path_after_root(source, NOW_USER_DIR)?;
            let snapshot = snapshot_ref(project_root, snapshot_root, "user", &suffix);
            let bytes = fs::read(project_root.join(&snapshot))?;
            output.push_str(&format!(
                "### `{source}`\n\n- provenance: `human-authored-temporal-source`\n- DAY snapshot: `{snapshot}`\n"
            ));
            match String::from_utf8(bytes) {
                Ok(text) => {
                    output.push_str("\nHuman source at close:\n\n");
                    output.push_str(&indented(&text));
                }
                Err(error) => {
                    output.push_str(&format!(
                        "- non-UTF-8 source retained byte-for-byte in DAY snapshot ({} bytes)\n",
                        error.as_bytes().len()
                    ));
                }
            }
            output.push('\n');
        }
    }

    output.push_str("## Agent returns at close\n\n");
    if handoffs.is_empty() {
        output.push_str("- none\n");
    } else {
        for (source, handoff) in handoffs {
            let suffix = path_after_root(source, NOW_AGENT_DIR)?;
            let snapshot = snapshot_ref(project_root, snapshot_root, "agents", &suffix);
            output.push_str(&format!(
                "### {}\n\n- source: `{source}`\n- DAY snapshot: `{snapshot}`\n- actor: `{}`\n- provenance: `{}`\n- kind: `{}`\n- status at close: `{}`\n",
                handoff.subject,
                handoff.actor,
                handoff.provenance,
                handoff.kind,
                handoff.status
            ));
            if let Some(run_ref) = &handoff.run_ref {
                output.push_str(&format!("- Run ref: `{run_ref}`\n"));
            }
            if let Some(session_ref) = &handoff.session_ref {
                output.push_str(&format!("- Session ref: `{session_ref}`\n"));
            }
            if let Some(focus_ref) = &handoff.focus_ref {
                output.push_str(&format!("- Focus ref: `{focus_ref}`\n"));
            }
            if !handoff.source_refs.is_empty() {
                output.push_str(&format!(
                    "- source refs: `{}`\n",
                    handoff.source_refs.join("`, `")
                ));
            }
            if !handoff.evidence_refs.is_empty() {
                output.push_str(&format!(
                    "- evidence refs: `{}`\n",
                    handoff.evidence_refs.join("`, `")
                ));
            }
            if !handoff.preserve_refs.is_empty() {
                output.push_str(&format!(
                    "- protected by refs: `{}`\n",
                    handoff.preserve_refs.join("`, `")
                ));
            }
            output.push_str("\nReturned result:\n\n");
            output.push_str(&indented(&handoff.result));
            output.push('\n');
        }
    }

    output.push_str("## Carry forward by stable NOW source ref\n\n");
    if carried.is_empty() {
        output.push_str("- none\n");
    } else {
        for source in carried {
            output.push_str(&format!("- `{source}`\n"));
        }
    }

    output.push_str("\n## Resolved / expired / promoted transient records removed from moving NOW\n\n");
    if removed.is_empty() {
        output.push_str("- none\n");
    } else {
        for source in removed {
            output.push_str(&format!("- `{source}`\n"));
        }
    }

    output.push_str("\n## Protected inactive records retained in NOW\n\n");
    if protected.is_empty() {
        output.push_str("- none\n");
    } else {
        for source in protected {
            output.push_str(&format!("- `{source}`\n"));
        }
    }

    output.push_str("\n## Promotion receipts\n\n");
    if promotions.is_empty() {
        output.push_str("- none\n");
    } else {
        for receipt in promotions {
            output.push_str(&format!(
                "- `{}` → **{}** → `{}` ({})\n",
                receipt.source, receipt.target, receipt.destination, receipt.acceptance
            ));
        }
    }

    Ok(output)
}

pub fn rollover(project_root: &Path, day: &str, next_day: &str) -> io::Result<RolloverReport> {
    let day_value = parse_day(day)?;
    let next_day_value = parse_day(next_day)?;
    if next_day_value <= day_value {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "next_day must be later than day",
        ));
    }

    let paths = now_paths(project_root);
    if !paths.root.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "NOW has not been initialized for this ProjectCentral",
        ));
    }
    let policy = read_policy(&paths.policy)?;
    let human_scratch = list_files(&paths.user, project_root)?;

    let mut handoffs = vec![];
    let mut entries = fs::read_dir(&paths.agents)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let path = entry.path();
        if entry.file_type()?.is_file()
            && path.extension().and_then(|value| value.to_str()) == Some("json")
        {
            handoffs.push((relative(project_root, &path), read_handoff(&path)?));
        }
    }

    let promotions = read_promotions(&paths.promotions)?.entries;
    let mut carried = vec![];
    let mut removed = vec![];
    let mut protected = vec![];
    for (source, handoff) in &handoffs {
        if policy
            .carry_statuses
            .iter()
            .any(|status| status == &handoff.status)
        {
            carried.push(source.clone());
        } else if policy
            .remove_statuses
            .iter()
            .any(|status| status == &handoff.status)
        {
            if policy.protect_when_preserve_refs_exist && !handoff.preserve_refs.is_empty() {
                protected.push(source.clone());
            } else {
                removed.push(source.clone());
            }
        } else {
            protected.push(source.clone());
        }
    }

    let day_path = paths.day.join(format!("{day}.md"));
    if day_path.exists() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!("DAY is already closed: {day}"),
        ));
    }

    let snapshot_root = snapshot_day_sources(
        project_root,
        &paths.day,
        day,
        &human_scratch,
        &handoffs,
    )?;
    let day_text = match render_day(
        project_root,
        &snapshot_root,
        day,
        next_day,
        &human_scratch,
        &handoffs,
        &carried,
        &removed,
        &protected,
        &promotions,
    ) {
        Ok(text) => text,
        Err(error) => {
            let _ = fs::remove_dir_all(&snapshot_root);
            return Err(error);
        }
    };
    if let Err(error) = fs::write(&day_path, day_text) {
        let _ = fs::remove_dir_all(&snapshot_root);
        return Err(error);
    }

    let mut cleanup_failures = vec![];
    for (source, mut handoff) in handoffs {
        let path = project_root.join(&source);
        if carried.iter().any(|value| value == &source) {
            handoff.status = "carried".into();
            if !handoff.carried_from_days.iter().any(|value| value == day) {
                handoff.carried_from_days.push(day.into());
            }
            if let Err(error) = write_json(&path, &handoff, true) {
                cleanup_failures.push(format!("carry {source}: {error}"));
            }
        } else if removed.iter().any(|value| value == &source) {
            if let Err(error) = fs::remove_file(&path) {
                cleanup_failures.push(format!("remove {source}: {error}"));
            }
        }
    }

    if let Err(error) = write_json(&paths.promotions, &PromotionLedger::default(), true) {
        cleanup_failures.push(format!("reset promotion ledger: {error}"));
    }

    Ok(RolloverReport {
        day: day.into(),
        next_day: next_day.into(),
        day_record: relative(project_root, &day_path),
        day_sources: relative(project_root, &snapshot_root),
        carried,
        removed,
        protected,
        human_scratch,
        promotions,
        cleanup_failures,
    })
}

fn safe_source(project_root: &Path, raw: &str, expected_root: &str) -> io::Result<PathBuf> {
    let relative = relative_member(raw)?;
    if !relative.starts_with(Path::new(expected_root)) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("source must be inside {expected_root}"),
        ));
    }
    reject_symlink_components(project_root, &relative)?;
    let path = project_root.join(&relative);
    let metadata = fs::symlink_metadata(&path)?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "promotion source must be an ordinary file",
        ));
    }
    Ok(path)
}

fn safe_destination(project_root: &Path, base: &str, raw: &str) -> io::Result<PathBuf> {
    let suffix = relative_member(raw)?;
    let relative = Path::new(base).join(suffix);
    reject_symlink_components(project_root, &relative)?;
    let path = project_root.join(&relative);
    if path.exists() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!("promotion destination already exists: {}", path.display()),
        ));
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    reject_symlink_components(project_root, relative.parent().unwrap_or(Path::new(base)))?;
    Ok(path)
}

pub fn promote(
    project_root: &Path,
    source: &str,
    target: &str,
    destination: &str,
    acceptance: &str,
) -> io::Result<NowPromotion> {
    let paths = now_paths(project_root);
    if !paths.root.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "NOW has not been initialized for this ProjectCentral",
        ));
    }

    let (source_path, destination_path, semantic_effect) = match target {
        "human-ground" => {
            if acceptance != "human-accepted" {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "human-ground promotion requires acceptance=human-accepted",
                ));
            }
            (
                safe_source(project_root, source, NOW_USER_DIR)?,
                safe_destination(project_root, HUMAN_SOURCE_DIR, destination)?,
                "copied into the human-owned Project ground by explicit human acceptance"
                    .to_owned(),
            )
        }
        "agent-wiki" => {
            if acceptance != "agent-return" {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "agent-wiki promotion requires acceptance=agent-return",
                ));
            }
            (
                safe_source(project_root, source, NOW_AGENT_DIR)?,
                safe_destination(project_root, WIKI_RETURN_DIR, destination)?,
                "returned into the Agent Wiki owner path as a source for Wiki maintenance; wiki.json is not silently rewritten".to_owned(),
            )
        }
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "target must be human-ground or agent-wiki",
            ))
        }
    };

    let destination_ref = relative(project_root, &destination_path);
    let receipt = PromotionReceipt {
        source: source.into(),
        target: target.into(),
        destination: destination_ref.clone(),
        acceptance: acceptance.into(),
        recorded_at_unix_seconds: unix_seconds(),
        source_preserved: true,
    };

    if target == "human-ground" {
        fs::copy(&source_path, &destination_path)?;
        if let Err(error) = append_promotion(&paths.promotions, receipt.clone()) {
            let _ = fs::remove_file(&destination_path);
            return Err(error);
        }
    } else {
        let original = read_handoff(&source_path)?;
        let mut promoted = original.clone();
        promoted.status = "promoted".into();
        if !promoted.promoted_to.contains(&destination_ref) {
            promoted.promoted_to.push(destination_ref.clone());
        }

        write_json(&source_path, &promoted, true)?;
        if let Err(error) = write_json(&destination_path, &promoted, false) {
            let _ = write_json(&source_path, &original, true);
            return Err(error);
        }
        if let Err(error) = append_promotion(&paths.promotions, receipt.clone()) {
            let _ = fs::remove_file(&destination_path);
            let _ = write_json(&source_path, &original, true);
            return Err(error);
        }
    }

    Ok(NowPromotion {
        source: source.into(),
        destination: destination_ref,
        target: target.into(),
        source_preserved: true,
        semantic_effect,
    })
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
    let action = "projectcentral.now.inspect";
    let project_root = match project_context(action, input, context) {
        Ok(value) => value,
        Err(result) => return result,
    };
    inspect_now(&project_root)
        .map(|value| {
            ActionResult::success(
                action,
                serde_json::to_value(value).expect("NOW inspection serializes"),
            )
        })
        .unwrap_or_else(|error| io_failure(action, error))
}

fn init_action(
    _: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let action = "projectcentral.now.init";
    let project_root = match project_context(action, input, context) {
        Ok(value) => value,
        Err(result) => return result,
    };
    initialize_now(&project_root)
        .map(|value| {
            ActionResult::success(
                action,
                serde_json::to_value(value).expect("NOW initialization serializes"),
            )
        })
        .unwrap_or_else(|error| io_failure(action, error))
}

fn return_action(
    _: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let action = "projectcentral.now.return";
    let project_root = match project_context(action, input, context) {
        Ok(value) => value,
        Err(result) => return result,
    };
    let paths = now_paths(&project_root);
    if !paths.root.is_dir() {
        return io_failure(
            action,
            io::Error::new(
                io::ErrorKind::NotFound,
                "NOW has not been initialized for this ProjectCentral",
            ),
        );
    }
    let handoff = match create_handoff(input, &project_root, action) {
        Ok(value) => value,
        Err(result) => return result,
    };
    let path = project_root
        .join(NOW_AGENT_DIR)
        .join(format!("{}.json", handoff.id));
    match write_json(&path, &handoff, false) {
        Ok(()) => ActionResult::success(
            action,
            json!({"source": relative(&project_root, &path), "handoff": handoff}),
        ),
        Err(error) => io_failure(action, error),
    }
}

fn update_action(
    _: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let action = "projectcentral.now.update";
    let project_root = match project_context(action, input, context) {
        Ok(value) => value,
        Err(result) => return result,
    };
    let id = match required(input, "id", action) {
        Ok(value) => value,
        Err(result) => return result,
    };
    let status = match required(input, "status", action) {
        Ok(value) => value,
        Err(result) => return result,
    };
    if let Err(error) = validate_id(&id).and_then(|_| validate_status(&status)) {
        return io_failure(action, error);
    }

    let path = project_root
        .join(NOW_AGENT_DIR)
        .join(format!("{id}.json"));
    let mut handoff = match read_handoff(&path) {
        Ok(value) => value,
        Err(error) => return io_failure(action, error),
    };
    handoff.status = status;
    match string_array(input, "preserve_refs", action) {
        Ok(refs) => {
            for reference in refs {
                if !handoff.preserve_refs.contains(&reference) {
                    handoff.preserve_refs.push(reference);
                }
            }
        }
        Err(result) => return result,
    }
    match write_handoff(&project_root, &handoff) {
        Ok(source) => ActionResult::success(
            action,
            json!({"source": relative(&project_root, &source), "handoff": handoff}),
        ),
        Err(error) => io_failure(action, error),
    }
}

fn promote_action(
    _: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let action = "projectcentral.now.promote";
    let project_root = match project_context(action, input, context) {
        Ok(value) => value,
        Err(result) => return result,
    };
    let source = match required(input, "source", action) {
        Ok(value) => value,
        Err(result) => return result,
    };
    let target = match required(input, "target", action) {
        Ok(value) => value,
        Err(result) => return result,
    };
    let destination = match required(input, "destination", action) {
        Ok(value) => value,
        Err(result) => return result,
    };
    let acceptance = match required(input, "acceptance", action) {
        Ok(value) => value,
        Err(result) => return result,
    };
    promote(
        &project_root,
        &source,
        &target,
        &destination,
        &acceptance,
    )
    .map(|value| {
        ActionResult::success(
            action,
            serde_json::to_value(value).expect("NOW promotion serializes"),
        )
    })
    .unwrap_or_else(|error| io_failure(action, error))
}

fn rollover_action(
    _: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let action = "projectcentral.now.rollover";
    let project_root = match project_context(action, input, context) {
        Ok(value) => value,
        Err(result) => return result,
    };
    let day = match required(input, "day", action) {
        Ok(value) => value,
        Err(result) => return result,
    };
    let next_day = match required(input, "next_day", action) {
        Ok(value) => value,
        Err(result) => return result,
    };
    match rollover(&project_root, &day, &next_day) {
        Ok(report) if report.cleanup_failures.is_empty() => ActionResult::success(
            action,
            serde_json::to_value(report).expect("rollover serializes"),
        ),
        Ok(report) => ActionResult::failure(
            Some(action),
            ResultStatus::PartialCompletion,
            "DAY closed, but one or more NOW cleanup operations failed.",
            Some(serde_json::to_value(report).expect("rollover serializes")),
        ),
        Err(error) => io_failure(action, error),
    }
}

pub fn register_projectcentral_now_actions(registry: &mut ActionRegistry) {
    let actions = [
        (
            descriptor(
                "projectcentral.now.inspect",
                "Inspect Project NOW",
                "Read the optional session-independent NOW horizon: human scratch refs, attributed Agent returns, open questions, DAY records, policy, and promotion receipts.",
                MutationClass::ReadOnly,
                "projectcentral-now-inspection",
                &[("project", true)],
            ),
            inspect_action as fn(&ActionRegistry, &Value, &ActionExecutionContext<'_>) -> ActionResult,
        ),
        (
            descriptor(
                "projectcentral.now.init",
                "Initialize Project NOW",
                "Opt a valid ProjectCentral into the ordinary-file NOW/DAY working field without changing authored Project ground or Agent Wiki canon.",
                MutationClass::LocallyMutating,
                "projectcentral-now-inspection",
                &[("project", true)],
            ),
            init_action,
        ),
        (
            descriptor(
                "projectcentral.now.return",
                "Write bounded Agent return",
                "Write one attributed Agent handoff/question/note/learning into NOW. External Run/Session/Focus/source identities remain refs rather than being duplicated.",
                MutationClass::LocallyMutating,
                "projectcentral-now-handoff",
                &[
                    ("project", true),
                    ("actor", true),
                    ("kind", true),
                    ("subject", true),
                    ("result", true),
                    ("status", true),
                    ("id", false),
                    ("run_ref", false),
                    ("session_ref", false),
                    ("focus_ref", false),
                    ("source_refs", false),
                    ("evidence_refs", false),
                    ("preserve_refs", false),
                ],
            ),
            return_action,
        ),
        (
            descriptor(
                "projectcentral.now.update",
                "Update NOW lifecycle",
                "Update the lifecycle status of one bounded Agent return and optionally pin it with durable foreign refs before rollover.",
                MutationClass::LocallyMutating,
                "projectcentral-now-handoff",
                &[
                    ("project", true),
                    ("id", true),
                    ("status", true),
                    ("preserve_refs", false),
                ],
            ),
            update_action,
        ),
        (
            descriptor(
                "projectcentral.now.promote",
                "Promote NOW material",
                "Explicitly copy human scratch into authored Project ground or return an Agent handoff into the Agent Wiki owner path while preserving source provenance.",
                MutationClass::LocallyMutating,
                "projectcentral-now-promotion",
                &[
                    ("project", true),
                    ("source", true),
                    ("target", true),
                    ("destination", true),
                    ("acceptance", true),
                ],
            ),
            promote_action,
        ),
        (
            descriptor(
                "projectcentral.now.rollover",
                "Close DAY and roll NOW",
                "Close one caller-supplied local civil DAY, snapshot source state, carry live items by reference, remove unprotected resolved/expired/promoted Agent clutter, and preserve protected material.",
                MutationClass::LocallyMutating,
                "projectcentral-now-rollover",
                &[("project", true), ("day", true), ("next_day", true)],
            ),
            rollover_action,
        ),
    ];
    for (descriptor, handler) in actions {
        registry
            .register(descriptor, handler)
            .expect("ProjectCentral NOW Action ids are valid");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::projectcentral_ops::initialize_projectcentral;
    use tempfile::tempdir;

    fn handoff(id: &str, status: &str) -> NowHandoff {
        NowHandoff {
            schema: HANDOFF_SCHEMA.into(),
            id: id.into(),
            provenance: "agent-authored-bounded-return".into(),
            actor: "agent:test".into(),
            kind: "handoff".into(),
            recorded_at_unix_seconds: unix_seconds(),
            subject: "Test return".into(),
            result: "Returned material".into(),
            status: status.into(),
            run_ref: None,
            session_ref: None,
            focus_ref: None,
            source_refs: vec![],
            evidence_refs: vec![],
            preserve_refs: vec![],
            carried_from_days: vec![],
            promoted_to: vec![],
        }
    }

    #[test]
    fn now_is_opt_in_and_does_not_change_projectcentral_validity() {
        let temp = tempdir().unwrap();
        let central = temp.path().join("Central");
        let project = central.join("Work/example");
        fs::create_dir_all(&project).unwrap();
        initialize_projectcentral(&central, &project, "example/project").unwrap();

        let before = inspect_now(&project).unwrap();
        assert!(!before.exists);
        assert!(!project.join(NOW_DIR).exists());

        let after = initialize_now(&project).unwrap();
        assert!(after.exists);
        assert!(project.join(NOW_USER_DIR).is_dir());
        assert!(project.join(NOW_AGENT_DIR).is_dir());
        assert!(project.join(NOW_DAY_DIR).is_dir());
        assert!(project.join(HUMAN_SOURCE_DIR).is_dir());
        assert!(project.join("ProjectCentral/agents/wiki/wiki.json").is_file());
    }

    #[test]
    fn day_snapshots_human_state_before_the_moving_source_changes() {
        let temp = tempdir().unwrap();
        let central = temp.path().join("Central");
        let project = central.join("Work/example");
        fs::create_dir_all(&project).unwrap();
        initialize_projectcentral(&central, &project, "example/project").unwrap();
        initialize_now(&project).unwrap();

        fs::write(project.join(NOW_USER_DIR).join("current.md"), "state A\n").unwrap();
        let report = rollover(&project, "2026-08-19", "2026-08-20").unwrap();
        fs::write(project.join(NOW_USER_DIR).join("current.md"), "state B\n").unwrap();

        let snapshot = project.join(&report.day_sources).join("user/current.md");
        assert_eq!(fs::read_to_string(snapshot).unwrap(), "state A\n");
        let day = fs::read_to_string(project.join(&report.day_record)).unwrap();
        assert!(day.contains("state A"));
        assert!(!day.contains("state B"));
    }

    #[test]
    fn protected_resolved_handoff_is_not_removed_on_rollover() {
        let temp = tempdir().unwrap();
        let central = temp.path().join("Central");
        let project = central.join("Work/example");
        fs::create_dir_all(&project).unwrap();
        initialize_projectcentral(&central, &project, "example/project").unwrap();
        initialize_now(&project).unwrap();

        let mut value = handoff("protected", "resolved");
        value.run_ref = Some("factory:run:1".into());
        value.preserve_refs = vec!["factory:artifact:1".into()];
        write_handoff(&project, &value).unwrap();

        let report = rollover(&project, "2026-08-19", "2026-08-20").unwrap();
        assert!(report
            .protected
            .iter()
            .any(|source| source.ends_with("protected.json")));
        assert!(project
            .join(NOW_AGENT_DIR)
            .join("protected.json")
            .is_file());
    }

    #[test]
    fn wiki_return_copy_carries_its_promotion_lineage() {
        let temp = tempdir().unwrap();
        let central = temp.path().join("Central");
        let project = central.join("Work/example");
        fs::create_dir_all(&project).unwrap();
        initialize_projectcentral(&central, &project, "example/project").unwrap();
        initialize_now(&project).unwrap();

        write_handoff(&project, &handoff("learning", "active")).unwrap();
        let promotion = promote(
            &project,
            "ProjectCentral/now/agents/learning.json",
            "agent-wiki",
            "now-day/learning.json",
            "agent-return",
        )
        .unwrap();
        let returned = read_handoff(&project.join(&promotion.destination)).unwrap();
        assert_eq!(returned.status, "promoted");
        assert_eq!(returned.promoted_to, vec![promotion.destination]);
    }
}
