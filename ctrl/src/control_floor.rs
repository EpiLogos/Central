//! The root world's floor (manual §18.5 and §10.11; O-I #1005): the instruction file every harness reads at the Central root.
//!
//! `control.floor.render` writes `AGENTS.md` at the root from the law adopted on the root world's tier 0: every durable statement
//! under `Control/agents/governance/`, each under its topic as a heading with its source path, its own words under it. The file
//! states its derived standing and its renderer in a header comment, and rendering it again from the same law gives the same
//! bytes. The instruction files the harnesses read (`files`: `CLAUDE.md`, `GEMINI.md`, ...) are made as links to it. A floor
//! Central did not render is never overwritten, and a floor past its byte budget is never written: both are refused in Central's
//! words, changing nothing.
//!
//! `control.floor.read` says what each named instruction file carries: rendered by Central (the statements it carries), written
//! by another hand (`foreign`: the statements it names by their path or by their topic as a heading), or absent.
//!
//! `control.law.adopt` adopts one statement written on tier 0, on the person's own credential: its relation is declared durable
//! and human-authored, and the floor is rendered again.
use crate::action::{
    ActionAvailability, ActionDescriptor, ActionExecutionContext, ActionInputDefinition,
    ActionOutputDefinition, ActionRegistry, MutationClass,
};
use crate::continuous_work::{authority, source, temporal};
use crate::result::{ActionResult, ResultStatus};
use crate::root::resolve_central_root;
use serde_json::{json, Value};
use std::fs;
use std::io;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

/// The floor every harness reads at the root, and the file each named instruction file links to.
pub const FLOOR_FILE: &str = "AGENTS.md";
/// The reading `control.floor.read` answers.
pub const FLOOR_READING_SCHEMA: &str = "central.floor-reading/v1";
/// The receipt `control.floor.render` answers.
pub const FLOOR_RENDER_SCHEMA: &str = "central.floor-render/v1";
/// The receipt `control.law.adopt` answers.
pub const LAW_ADOPTION_SCHEMA: &str = "central.law-adoption/v1";
/// The renderer a floor Central rendered names in its header, with the version of its form.
pub const FLOOR_RENDERER: &str = "central control.floor.render@1";
const RENDERER_PREFIX: &str = "central control.floor.render@";
/// The floor's byte budget: the owner's smallest-sufficient law, as the person's own floor keeps it (`FLOOR_BUDGET_BYTES` of the
/// session strap's renderer). A floor that would pass it is refused; a statement is shortened or moved behind a door, never the
/// budget raised to fit.
pub const FLOOR_BUDGET_BYTES: usize = 4096;
/// Where the root world's tier 0 keeps its law.
pub const GOVERNANCE_DIR: &str = "Control/agents/governance";
const GOVERNANCE_ROLE: &str = "agent-governance-source";

const RENDER: &str = "control.floor.render";
const READ: &str = "control.floor.read";
pub const LAW_ADOPT: &str = "control.law.adopt";

/// A refusal in Central's own words that changed nothing.
fn refused(words: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::PermissionDenied, words.into())
}

/// The instruction-file names an Action was given (`files`): each one plain file name at the root, never a path.
fn file_names(input: &Value, required: bool) -> io::Result<Vec<String>> {
    let Some(given) = input.get("files").filter(|value| !value.is_null()) else {
        return if required {
            Err(source::invalid(
                "files names the instruction files the harnesses read (CLAUDE.md, GEMINI.md, ...)",
            ))
        } else {
            Ok(Vec::new())
        };
    };
    let names = given.as_array().ok_or_else(|| {
        source::invalid("files is a list of instruction-file names (CLAUDE.md, GEMINI.md, ...)")
    })?;
    if names.len() > 32 {
        return Err(source::invalid("files names at most 32 instruction files"));
    }
    let mut out: Vec<String> = Vec::new();
    for name in names {
        let name = name
            .as_str()
            .map(str::trim)
            .filter(|name| {
                !name.is_empty()
                    && name.len() <= 128
                    && *name != "."
                    && *name != ".."
                    && !name.contains(['/', '\\', '\0'])
            })
            .ok_or_else(|| {
                source::invalid(
                    "each of files is one instruction file's name at the Central root (CLAUDE.md), never a path",
                )
            })?;
        if !out.iter().any(|held| held == name) {
            out.push(name.to_owned());
        }
    }
    Ok(out)
}

/// One statement of the root world's tier 0, as the governance index reads it.
#[derive(Clone, Debug)]
struct Statement {
    file: String,
    topic: String,
    standing: String,
}

impl Statement {
    fn value(&self) -> Value {
        json!({"file": self.file, "topic": self.topic, "standing": self.standing})
    }
    fn path(&self) -> String {
        format!("{GOVERNANCE_DIR}/{}", self.file)
    }
}

/// Every statement under the governance folder with its topic and standing; none where the folder is absent.
fn statements(root: &Path) -> io::Result<Vec<Statement>> {
    if !root.join(GOVERNANCE_DIR).is_dir() {
        return Ok(Vec::new());
    }
    Ok(crate::control::index_governance(root)?
        .statements
        .into_iter()
        .map(|entry| Statement {
            file: entry.file,
            topic: entry.topic,
            standing: entry.standing,
        })
        .collect())
}

/// A statement's own words under its topic: its text without its `# ` title line and the blank lines around it.
fn body_of(text: &str) -> String {
    let mut lines = text.lines().peekable();
    let mut out = Vec::new();
    let mut titled = false;
    while let Some(line) = lines.next() {
        if !titled && line.starts_with("# ") {
            titled = true;
            while lines.peek().is_some_and(|next| next.trim().is_empty()) {
                lines.next();
            }
            continue;
        }
        out.push(line);
    }
    out.join("\n").trim().to_owned()
}

fn header() -> String {
    format!(
        "# Central floor\n\n<!-- derived, generated-derived: Central renders this floor from the law adopted on the root world's tier 0\n\
         (Control/agents/governance/). Do not edit it here: write a statement there and adopt it (control.law.adopt),\n\
         which renders it again.\nrenderer: {FLOOR_RENDERER}\n-->\n"
    )
}

fn section(statement: &Statement, body: &str) -> String {
    let mut out = format!(
        "\n## {}\n\nSource: `{}`\n",
        statement.topic,
        statement.path()
    );
    if !body.is_empty() {
        out.push('\n');
        out.push_str(body);
        out.push('\n');
    }
    out
}

/// The floor the adopted law renders: its bytes and the statements it carries, or the refusal past the budget naming each
/// statement it could not carry.
fn rendered_floor(root: &Path) -> io::Result<(String, Vec<Statement>)> {
    let carried: Vec<Statement> = statements(root)?
        .into_iter()
        .filter(|statement| statement.standing == "durable")
        .collect();
    let mut floor = header();
    let mut over = Vec::new();
    for statement in &carried {
        let text = crate::source_safety::read(root, &statement.path())?;
        let part = section(statement, &body_of(&text));
        if floor.len() + part.len() > FLOOR_BUDGET_BYTES {
            over.push(statement);
        }
        floor.push_str(&part);
    }
    if !over.is_empty() {
        let named = over
            .iter()
            .map(|statement| format!("{} ({})", statement.topic, statement.path()))
            .collect::<Vec<_>>()
            .join("; ");
        return Err(refused(format!(
            "The floor would be {} bytes, past its budget of {FLOOR_BUDGET_BYTES} (smallest sufficient: shorten a statement or move it behind a door; the budget is not raised to fit). It could not carry: {named}. Nothing was written.",
            floor.len()
        )));
    }
    Ok((floor, carried))
}

/// What stands at a root file: absent, a link to the floor (by the name it links to), or a file of its own.
enum Standing {
    Absent,
    LinkToFloor,
    Link(String),
    File,
}

fn standing_of(root: &Path, name: &str) -> io::Result<Standing> {
    let path = root.join(name);
    match fs::symlink_metadata(&path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Standing::Absent),
        Err(error) => Err(error),
        Ok(metadata) if metadata.file_type().is_symlink() => {
            let target = fs::read_link(&path)?;
            if target == Path::new(FLOOR_FILE) || target == root.join(FLOOR_FILE) {
                Ok(Standing::LinkToFloor)
            } else {
                Ok(Standing::Link(target.to_string_lossy().into_owned()))
            }
        }
        Ok(_) => Ok(Standing::File),
    }
}

/// The renderer a floor's header names, if it names one: up to its first `;`.
fn renderer_of(text: &str) -> Option<String> {
    text.lines().find_map(|line| {
        line.trim()
            .strip_prefix("renderer:")
            .map(|named| named.split(';').next().unwrap_or("").trim().to_owned())
            .filter(|named| !named.is_empty())
    })
}

fn rendered_by_central(text: &str) -> bool {
    renderer_of(text).is_some_and(|named| named.starts_with(RENDERER_PREFIX))
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0)
}

/// Render the floor; the caller holds the root's source lock. Everything is checked before anything is written: a floor Central
/// did not render, a named file that is neither absent nor a link to the floor, and a floor past its budget are refused in
/// Central's words, changing nothing.
fn render_locked(root: &Path, files: &[String]) -> io::Result<Value> {
    let floor_path = root.join(FLOOR_FILE);
    let before = match standing_of(root, FLOOR_FILE)? {
        Standing::Absent => None,
        Standing::File => Some(fs::read(&floor_path)?),
        Standing::LinkToFloor | Standing::Link(_) => {
            return Err(refused(format!(
                "{FLOOR_FILE} at {} is a link Central did not make, so it is not a floor Central rendered; Central leaves it as it is. Nothing was written.",
                root.display()
            )))
        }
    };
    if let Some(bytes) = &before {
        let text = String::from_utf8_lossy(bytes);
        if !rendered_by_central(&text) {
            let by = renderer_of(&text)
                .map(|named| format!("its header names {named} as its renderer"))
                .unwrap_or_else(|| "its header names no renderer".to_owned());
            return Err(refused(format!(
                "{FLOOR_FILE} at {} was not rendered by Central ({by}); Central does not overwrite a floor it did not render. Nothing was written.",
                root.display()
            )));
        }
    }
    let mut links = Vec::new();
    for name in files.iter().filter(|name| name.as_str() != FLOOR_FILE) {
        match standing_of(root, name)? {
            Standing::Absent => links.push((name.clone(), "made")),
            Standing::LinkToFloor => links.push((name.clone(), "kept")),
            Standing::Link(target) => {
                return Err(refused(format!(
                    "{name} at the Central root links to {target}, not to {FLOOR_FILE}; Central leaves an instruction file it did not make as it is. Nothing was written."
                )))
            }
            Standing::File => {
                return Err(refused(format!(
                    "{name} at the Central root is a file of its own, not a link to {FLOOR_FILE}; Central leaves an instruction file it did not make as it is. Nothing was written."
                )))
            }
        }
    }
    let (floor, carried) = rendered_floor(root)?;
    let changed = before.as_deref() != Some(floor.as_bytes());
    if changed {
        crate::file_mutation::atomic_record(
            root,
            Path::new(FLOOR_FILE),
            floor.as_bytes(),
            crate::file_mutation::RecordDisposition::ReplaceOrCreate,
        )?;
    }
    for (name, state) in &links {
        if *state == "made" {
            make_link(root, name)?;
        }
    }
    Ok(json!({
        "schema": FLOOR_RENDER_SCHEMA,
        "root": root.to_string_lossy(),
        "floor": FLOOR_FILE,
        "renderer": FLOOR_RENDERER,
        "changed": changed,
        "revision": crate::source_safety::content_revision_bytes(floor.as_bytes()),
        "bytes": floor.len(),
        "budget_bytes": FLOOR_BUDGET_BYTES,
        "statements": carried.iter().map(Statement::value).collect::<Vec<_>>(),
        "links": links.iter().map(|(file, state)| json!({"file": file, "to": FLOOR_FILE, "state": state})).collect::<Vec<_>>(),
        "automatic_agent_or_model_invocation": false,
    }))
}

#[cfg(unix)]
fn make_link(root: &Path, name: &str) -> io::Result<()> {
    std::os::unix::fs::symlink(FLOOR_FILE, root.join(name))
}
#[cfg(not(unix))]
fn make_link(_: &Path, name: &str) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        format!("{name} cannot be made as a link to {FLOOR_FILE} on this system"),
    ))
}

/// The root world's scope, or why `root` is no Central ground in words: it keeps no Control and Work.
fn ground_scope(root: &Path) -> io::Result<source::Scope> {
    source::Scope::resolve(root, None).map_err(|error| match error.kind() {
        io::ErrorKind::NotFound => io::Error::new(
            io::ErrorKind::NotFound,
            format!("{} is no Central ground (it keeps no Control and Work): there is no law here to render a floor from", root.display()),
        ),
        _ => error,
    })
}

/// Render the root world's floor (`control.floor.render`).
pub fn render(root: &Path, input: &Value) -> io::Result<Value> {
    let files = file_names(input, false)?;
    let scope = ground_scope(root)?;
    let _locks = source::lock(&scope)?;
    render_locked(&scope.central_root, &files)
}

/// The statements a rendered floor carries, by the source path under each heading.
fn carried_by(text: &str, index: &[Statement]) -> Vec<Value> {
    let mut topic = None;
    let mut out = Vec::new();
    for line in text.lines() {
        if let Some(heading) = line.strip_prefix("## ") {
            topic = Some(heading.trim().to_owned());
        } else if let Some(path) = line
            .strip_prefix("Source: `")
            .and_then(|rest| rest.strip_suffix('`'))
        {
            let Some(file) = path.strip_prefix(&format!("{GOVERNANCE_DIR}/")) else {
                continue;
            };
            let known = index.iter().find(|statement| statement.file == file);
            out.push(json!({
                "file": file,
                "topic": known.map(|s| s.topic.clone()).or_else(|| topic.clone()).unwrap_or_default(),
                "standing": known.map_or("absent", |s| s.standing.as_str()),
            }));
        }
    }
    out
}

/// The governance statements a floor Central did not render names: by their path under the governance folder, or by their
/// topic as a heading.
fn named_by(text: &str, index: &[Statement]) -> Vec<Value> {
    let headings: Vec<String> = text
        .lines()
        .filter_map(|line| {
            let rest = line.trim_start_matches('#');
            (rest.len() < line.len() && rest.starts_with(' ')).then(|| rest.trim().to_lowercase())
        })
        .collect();
    index
        .iter()
        .filter(|statement| {
            text.contains(&statement.path())
                || headings
                    .iter()
                    .any(|heading| *heading == statement.topic.trim().to_lowercase())
        })
        .map(Statement::value)
        .collect()
}

/// Read what each named instruction file carries (`control.floor.read`).
pub fn read(root: &Path, input: &Value) -> io::Result<Value> {
    let files = file_names(input, true)?;
    let root = root.canonicalize().map_err(|error| match error.kind() {
        io::ErrorKind::NotFound => io::Error::new(
            io::ErrorKind::NotFound,
            format!(
                "no Central ground at {}: there are no floors to read",
                root.display()
            ),
        ),
        _ => error,
    })?;
    let index = statements(&root)?;
    let mut floors = Vec::new();
    for name in &files {
        let link = matches!(standing_of(&root, name)?, Standing::LinkToFloor);
        let text = match fs::read(root.join(name)) {
            Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                let why = if link {
                    format!("{name} links to {FLOOR_FILE}, which is not at the Central root")
                } else {
                    format!("no {name} at the Central root")
                };
                floors.push(json!({"file": name, "state": "absent", "statements": [], "why": why}));
                continue;
            }
            Err(error) => return Err(error),
        };
        if rendered_by_central(&text) {
            floors.push(
                json!({"file": name, "state": "rendered", "renderer": renderer_of(&text),
                "statements": carried_by(&text, &index)}),
            );
        } else {
            let renderer = renderer_of(&text);
            let why = match &renderer {
                Some(named) => format!("{name} was rendered by {named}, not by Central: Central reads the law it names and leaves it as it is"),
                None => format!("{name} names no renderer, so Central did not render it: Central reads the law it names and leaves it as it is"),
            };
            let mut floor = json!({"file": name, "state": "foreign", "statements": named_by(&text, &index), "why": why});
            if let Some(named) = renderer {
                floor["renderer"] = json!(named);
            }
            floors.push(floor);
        }
    }
    Ok(json!({
        "schema": FLOOR_READING_SCHEMA,
        "root": root.to_string_lossy(),
        "floors": floors,
        "automatic_agent_or_model_invocation": false,
    }))
}

/// The statement `control.law.adopt` names: one Markdown file under the governance folder, never the folder's README or its
/// compiled prompt. Answers its path under the root.
fn statement_path(input: &Value) -> io::Result<String> {
    let given = input
        .get("statement")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|given| !given.is_empty() && given.len() <= 1024)
        .ok_or_else(|| {
            source::invalid(format!(
                "statement names one law statement under {GOVERNANCE_DIR}/ (its path there, as craft/plain-words.md)"
            ))
        })?;
    let file = given
        .strip_prefix(&format!("{GOVERNANCE_DIR}/"))
        .unwrap_or(given);
    let relative = Path::new(file);
    if !file.ends_with(".md")
        || relative.is_absolute()
        || !relative
            .components()
            .all(|part| matches!(part, std::path::Component::Normal(_)))
        || matches!(
            relative.file_name().and_then(|name| name.to_str()),
            Some("README.md" | "foundational-prompt.md")
        )
    {
        return Err(source::invalid(format!(
            "statement is one law statement's Markdown file under {GOVERNANCE_DIR}/, never its README, its compiled prompt or a path outside it"
        )));
    }
    Ok(format!("{GOVERNANCE_DIR}/{file}"))
}

/// Adopt one statement written on the root world's tier 0 (`control.law.adopt`), on the person's own credential: the statement
/// stands on the revision the person read, its relation is declared durable and human-authored (a relation it had is changed in
/// place, so its identity stays), and the floor is rendered again. Where the floor cannot be rendered (another hand rendered it,
/// or it would pass its budget), the adoption stands and the receipt carries the floor's refusal in its words.
pub fn adopt_with_token_at(
    central: &Path,
    input: &Value,
    token: Option<&str>,
    now: u64,
) -> io::Result<Value> {
    let path = statement_path(input)?;
    let expected = input
        .get("expected_revision")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            source::invalid("expected_revision is the statement's revision as you read it")
        })?;
    let files = file_names(input, false)?;
    let scope = ground_scope(central)?;
    let principal = authority::authenticate(
        &scope,
        token,
        LAW_ADOPT,
        input
            .get("expected_authority_revision")
            .and_then(Value::as_str),
        now,
    )?;
    principal.require_human()?;
    let _locks = source::lock(&scope)?;
    let bytes = match crate::source_safety::read(&scope.root, &path) {
        Ok(text) => text.into_bytes(),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!(
                    "{path} is not written yet: write the statement on tier 0 first, then adopt it"
                ),
            ))
        }
        Err(error) => return Err(error),
    };
    let revision = crate::source_safety::content_revision_bytes(&bytes);
    if revision != expected {
        return Err(source::conflict(format!(
            "{path} changed since you read it (it stands at {revision}): read it again before adopting it. Nothing was adopted."
        )));
    }
    let (mut relations, basis) = scope.relations()?;
    let member = crate::source_safety::normal_member_key(&path)?;
    let entries = relations["relations"]
        .as_array_mut()
        .ok_or_else(|| source::invalid("invalid source relations"))?;
    let recognition = format!("control-law-adoption:{}", principal.principal_ref);
    let mut adopted_before = false;
    let mut found = false;
    for entry in entries.iter_mut() {
        let same = entry["path"]
            .as_str()
            .map(crate::source_safety::normal_member_key)
            .transpose()?
            .is_some_and(|key| key == member);
        if !same {
            continue;
        }
        found = true;
        adopted_before = entry["standing"] == "durable-source"
            && entry["provenance"] == "human-authored"
            && entry["recognition"] == recognition.as_str();
        entry["provenance"] = json!("human-authored");
        entry["standing"] = json!("durable-source");
        let mut roles: Vec<Value> = entry["roles"].as_array().cloned().unwrap_or_default();
        if !roles.iter().any(|role| role == GOVERNANCE_ROLE) {
            roles.push(json!(GOVERNANCE_ROLE));
        }
        entry["roles"] = Value::Array(roles);
        if entry["treatment"].as_str().is_none_or(str::is_empty) {
            entry["treatment"] = json!("retain-native-in-place");
        }
        entry["recognition"] = json!(recognition);
        entry["recorded_at_unix_seconds"] = json!(now);
    }
    if !found {
        entries.push(json!({
            "ref": scope.source_ref(&path), "path": path, "roles": [GOVERNANCE_ROLE],
            "provenance": "human-authored", "standing": "durable-source", "treatment": "retain-native-in-place",
            "recognition": recognition, "recorded_at_unix_seconds": now,
        }));
    }
    if !adopted_before {
        temporal::save_relations(&scope, &relations, &basis)?;
        scope.reconcile(
            Some((&principal.principal_ref, &principal.actor_kind, None)),
            &[scope.source_ref(&path)],
        )?;
    }
    let floor = match render_locked(&scope.central_root, &files) {
        Ok(receipt) => receipt,
        Err(error) if error.kind() == io::ErrorKind::PermissionDenied => {
            json!({"schema": FLOOR_RENDER_SCHEMA, "floor": FLOOR_FILE, "changed": false, "refused": error.to_string()})
        }
        Err(error) => return Err(error),
    };
    Ok(json!({
        "schema": LAW_ADOPTION_SCHEMA,
        "statement": {"path": path, "file": path.strip_prefix(&format!("{GOVERNANCE_DIR}/")).unwrap_or(&path),
            "revision": revision, "standing": "durable", "provenance": "human-authored"},
        "adopted": !adopted_before,
        "adopted_by": principal.principal_ref,
        "floor": floor,
        "automatic_agent_or_model_invocation": false,
    }))
}

fn root_of(
    action: &str,
    context: &ActionExecutionContext<'_>,
) -> Result<std::path::PathBuf, ActionResult> {
    resolve_central_root(context.root_options)
        .map(|root| root.path)
        .map_err(|message| {
            ActionResult::failure(Some(action), ResultStatus::InvalidInput, message, None)
        })
}

fn answer(action: &str, outcome: io::Result<Value>) -> ActionResult {
    match outcome {
        Ok(value) => ActionResult::success(action, value),
        Err(error) => {
            let status = match error.kind() {
                io::ErrorKind::InvalidInput | io::ErrorKind::InvalidData => {
                    ResultStatus::InvalidInput
                }
                io::ErrorKind::PermissionDenied | io::ErrorKind::NotFound => {
                    ResultStatus::UnavailableCapability
                }
                io::ErrorKind::AlreadyExists => ResultStatus::VerificationFailure,
                _ => ResultStatus::InternalFailure,
            };
            ActionResult::failure(Some(action), status, error.to_string(), None)
        }
    }
}

fn render_action(
    _: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    match root_of(RENDER, context) {
        Ok(root) => answer(RENDER, render(&root, input)),
        Err(failure) => failure,
    }
}

fn read_action(
    _: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    match root_of(READ, context) {
        Ok(root) => answer(READ, read(&root, input)),
        Err(failure) => failure,
    }
}

fn adopt_action(
    _: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    match root_of(LAW_ADOPT, context) {
        Ok(root) => {
            let token = std::env::var("CENTRAL_NATIVE_TOKEN").ok();
            answer(
                LAW_ADOPT,
                adopt_with_token_at(&root, input, token.as_deref(), now()),
            )
        }
        Err(failure) => failure,
    }
}

fn input(name: &str, input_type: &str, required: bool) -> ActionInputDefinition {
    ActionInputDefinition {
        name: name.to_owned(),
        input_type: input_type.to_owned(),
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

/// Register the floor's two Actions and the law's adoption in Central's Action field.
pub fn register_floor_actions(registry: &mut ActionRegistry) {
    let actions: [(ActionDescriptor, crate::action::ActionHandler); 3] = [
        (
            descriptor(
                RENDER,
                "Render the root world's floor",
                "Render AGENTS.md at the Central root from the law adopted on the root world's tier 0: every durable statement under Control/agents/governance/, each under its topic as a heading with its source path, the header naming its derived standing and its renderer; the same law renders the same bytes. Each name in files (CLAUDE.md, GEMINI.md, ...) is made a link to AGENTS.md. Refused, changing nothing, where AGENTS.md was not rendered by Central, where a named file is neither absent nor a link to it, and past the floor's budget of 4096 bytes, naming each statement it could not carry. A generated-derived write: it writes at the root, not under .central.",
                MutationClass::LocallyMutating,
                FLOOR_RENDER_SCHEMA,
                vec![input("files", "array", false)],
            ),
            render_action,
        ),
        (
            descriptor(
                READ,
                "Read the floors at the root",
                "For each instruction file named in files, at the Central root: rendered (by Central: the statements it carries), foreign (rendered by another hand: the governance statements it names by their path under Control/agents/governance/ or by their topic as a heading) or absent. Answers central.floor-reading/v1. Read-only.",
                MutationClass::ReadOnly,
                FLOOR_READING_SCHEMA,
                vec![input("files", "array", true)],
            ),
            read_action,
        ),
        (
            descriptor(
                LAW_ADOPT,
                "Adopt a law statement",
                "Adopt one statement written on the root world's tier 0 (statement: its path under Control/agents/governance/), on the revision you read (expected_revision) and the person's own native credential: its relation is declared durable and human-authored, and the root world's floor is rendered again (control.floor.render, with the links in files). Where the floor cannot be rendered, the adoption stands and the receipt carries the floor's refusal.",
                MutationClass::LocallyMutating,
                LAW_ADOPTION_SCHEMA,
                vec![
                    input("statement", "string", true),
                    input("expected_revision", "string", true),
                    input("files", "array", false),
                    input("expected_authority_revision", "string", false),
                ],
            ),
            adopt_action,
        ),
    ];
    for (descriptor, handler) in actions {
        registry
            .register(descriptor, handler)
            .expect("the floor's Action ids are valid and unique");
    }
}

#[cfg(test)]
mod tests;
