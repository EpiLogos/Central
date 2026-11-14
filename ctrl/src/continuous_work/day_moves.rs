//! The Day's moves: what Central keeps so that the Day device's five pads have
//! an owner (manual 18.2 "Moves", "Two Days at a time").
//!
//! Central keeps each move's Prompt and Runs at, the Day's own settings, a record
//! of every run, which runs are due, and two Days open at once. Central never
//! runs an agent: every reading here says `automatic_agent_or_model_invocation:
//! false`, and that stays true. The agent that runs a move is AIKit's; the
//! runner that presses a due move is the native host's. Both call these actions.
//!
//! Nothing runs by itself on defaults. A schedule exists only once the person
//! has saved it (`central.day.settings.write`, human principal): the due list,
//! a scheduled begin, and the agent paths of rollover and close all read the
//! saved settings, never the defaults.
use super::{
    authority::Principal,
    documents, history,
    source::{self, conflict, denied, encoded, invalid, text, Scope},
    temporal::{self, TimeReading},
};
use crate::action::{ActionExecutionContext, ActionRegistry};
use crate::result::ActionResult;
use chrono::{DateTime, Duration, LocalResult, NaiveDate, Offset, TimeZone, Timelike, Utc};
use chrono_tz::Tz;
use serde_json::{json, Map, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    io,
};

pub(crate) const MOVES: [&str; 5] = ["tasks", "patterns", "context", "synthesis", "close"];
/// What sends entries to the Day (manual 18.2 Sources): each mode, and the shared
/// field. The field is a source of its own, not Technē's (Central #324; DLW build
/// ruling 2026-11-08): what reaches you there comes in every mode (manual 2, "Every
/// kind of work takes place in the field") and names who sent it and how it came,
/// so turning Technē's sessions off never silences the field, nor the reverse.
const SOURCES: [&str; 6] = [
    "central",
    "factory",
    "expressions",
    "techne",
    "live",
    "field",
];
const AGENT_WRITES: [&str; 3] = ["off", "propose", "write"];
const SETTINGS_SCHEMA: &str = "central.day-settings/v1";
const EVENT_SCHEMA: &str = "central.day-move-event/v1";
const RUN_REF_PREFIX: &str = "central:day-move-run:";
const DEFAULTS_REASON: &str =
    "The Day's settings are the defaults; nothing runs by itself until you save them.";
const NO_SCHEDULE_SAVED: &str =
    "No move's Runs at is saved as a schedule; nothing runs by itself until you save one.";
/// The Day document keys the close carry reads and writes. They are the page
/// template's own (ql-daily-die.html, `ql-daily v0.1`): face 5's "Tomorrow's
/// focus, as a proposal for the next ground" is the body fixture `p5_tomorrow`;
/// face 0's Ground is `fields.p0_quick_thoughts`. Central's document model does
/// not name a face field of its own: a Day document maps whichever template
/// pointers its creator supplied (documents.rs `fields`), so the carry finds the
/// face 0 field by this pointer and reads face 5's text from the payload.
const FACE_FIVE_PROPOSAL: &str = "/body/p5_tomorrow";
const FACE_ZERO_GROUND: &str = "/fields/p0_quick_thoughts";
const MAX_DUE_WINDOW_MS: i64 = 400 * 24 * 3600 * 1000;

/// The shape of the Sources setting, in words, from `SOURCES` itself.
fn sources_shape() -> String {
    format!("sources is {{{}}}", SOURCES.join(", "))
}

fn label(mv: &str) -> &'static str {
    match mv {
        "tasks" => "Tasks",
        "patterns" => "Patterns",
        "context" => "Context",
        "synthesis" => "Synthesis",
        _ => "Close",
    }
}

/// Each move's own prompt: what the move asks of the agent until the person
/// makes it theirs (manual 18.2, "What the move does").
fn own_prompt(mv: &str) -> &'static str {
    match mv {
        "tasks" => "Read face 0, the Ground of the Day: what the person wants from the day, what worries them and what arrived. Propose the tasks that follow from it on face 1, each in one of three tiers: must, should or could. Propose only; the person accepts, re-tiers or removes each task.",
        "patterns" => "Read what ran today, the sessions of every mode on face 2. Propose the patterns that appear across that work on face 3, and with each pattern the items that show it. The person keeps the patterns that hold and discards the rest.",
        "context" => "Gather what the day's sessions touched, from every session on face 2: the moments, people, places and files. Add them to face 4, each entry showing its author, so the person can see where the day sat.",
        "synthesis" => "Read faces 1 to 4 and draft face 5: what the day taught, what stays open, and a proposal for tomorrow's face 0. Write it as a draft the person can rewrite; each entry shows its author.",
        _ => "Run the close routine for the Day: summarise the day, send what the person learned to their knowledge, and carry face 5's proposal for tomorrow into tomorrow's face 0. Then the Day closes and is kept.",
    }
}

// ---- Runs at ----------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
enum RunsAt {
    Off,
    Daily(String),
    Rollover,
    DayStart,
}
impl RunsAt {
    fn parse(value: &Value) -> io::Result<Self> {
        let object = value
            .as_object()
            .ok_or_else(|| invalid("runs_at is {kind: off | daily | rollover | day_start}"))?;
        let kind = object
            .get("kind")
            .and_then(Value::as_str)
            .ok_or_else(|| invalid("runs_at needs a kind"))?;
        let only = |allowed: &[&str]| -> io::Result<()> {
            match object.keys().find(|key| !allowed.contains(&key.as_str())) {
                Some(key) => Err(invalid(format!("runs_at {kind} has no `{key}`"))),
                None => Ok(()),
            }
        };
        match kind {
            "off" => only(&["kind"]).map(|_| RunsAt::Off),
            "rollover" => only(&["kind"]).map(|_| RunsAt::Rollover),
            "day_start" => only(&["kind"]).map(|_| RunsAt::DayStart),
            "daily" => {
                only(&["kind", "time"])?;
                let time = object
                    .get("time")
                    .and_then(Value::as_str)
                    .ok_or_else(|| invalid("a daily runs_at needs a time as HH:MM"))?;
                Ok(RunsAt::Daily(canonical_time(time)?))
            }
            other => Err(invalid(format!(
                "runs_at kind is off, daily, rollover or day_start, not `{other}`"
            ))),
        }
    }
    fn to_json(&self) -> Value {
        match self {
            RunsAt::Off => json!({"kind":"off"}),
            RunsAt::Daily(time) => json!({"kind":"daily","time":time}),
            RunsAt::Rollover => json!({"kind":"rollover"}),
            RunsAt::DayStart => json!({"kind":"day_start"}),
        }
    }
    fn kind(&self) -> &'static str {
        match self {
            RunsAt::Off => "off",
            RunsAt::Daily(_) => "daily",
            RunsAt::Rollover => "rollover",
            RunsAt::DayStart => "day_start",
        }
    }
}

/// HH:MM through Central's own `parse_hh_mm`, kept as two zero-padded digits.
fn canonical_time(raw: &str) -> io::Result<String> {
    if raw.matches(':').count() != 1 || !raw.chars().all(|c| c.is_ascii_digit() || c == ':') {
        return Err(invalid("a time is HH:MM"));
    }
    let (hour, minute) = temporal::parse_hh_mm(raw)?;
    Ok(format!("{hour:02}:{minute:02}"))
}

// ---- settings ---------------------------------------------------------------

fn settings_path(scope: &Scope) -> String {
    format!("{}/settings.json", scope.day_dir())
}
fn record_path(scope: &Scope, civil_date: &str) -> String {
    format!("{}/{civil_date}/moves.jsonl", scope.day_dir())
}
fn read_optional(scope: &Scope, path: &str) -> io::Result<Option<String>> {
    match crate::source_safety::read(&scope.root, path) {
        Ok(raw) => Ok(Some(raw)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

/// What the person has saved: a sparse file. A key that is absent is the
/// default and has the standing "default"; a key that is present, even when it
/// equals the default, is the person's own and has the standing "saved".
struct Loaded {
    raw: Option<String>,
    revision: String,
    saved: Map<String, Value>,
}
impl Loaded {
    fn exists(&self) -> bool {
        self.raw.is_some()
    }
    fn rollover(&self) -> (Option<String>, bool) {
        match self.saved.get("rollover") {
            Some(Value::String(time)) => (canonical_time(time).ok(), true),
            Some(_) => (None, true),
            None => (None, false),
        }
    }
    fn agent_writes(&self) -> (String, bool) {
        match self.saved.get("agent_writes").and_then(Value::as_str) {
            Some(value) => (value.to_owned(), true),
            None => ("propose".to_owned(), false),
        }
    }
    fn close_routine(&self) -> (bool, bool) {
        match self.saved.get("close_routine").and_then(Value::as_bool) {
            Some(value) => (value, true),
            None => (true, false),
        }
    }
    /// The Day form the person saved (`day_form`, its path under the world's Central root): the one form the Day's turn makes a
    /// page from (day_turn.rs). None until they save one.
    fn day_form(&self) -> (Option<String>, bool) {
        match self.saved.get("day_form") {
            Some(Value::String(path)) => (Some(path.clone()), true),
            Some(_) => (None, true),
            None => (None, false),
        }
    }
    fn source(&self, name: &str) -> (bool, bool) {
        match self
            .saved
            .get("sources")
            .and_then(|sources| sources.get(name))
            .and_then(Value::as_bool)
        {
            Some(value) => (value, true),
            None => (true, false),
        }
    }
    /// When a source last turned On, as Central kept it from the settings write
    /// that turned it On (Central #324: turning the field Off and On again does
    /// not backfill what came while it was Off). Absent while it never turned On
    /// from Off.
    fn on_since(&self, name: &str) -> Option<i64> {
        self.saved
            .get("on_since")
            .and_then(|on_since| on_since.get(name))
            .and_then(Value::as_i64)
    }
    fn setting(&self, mv: &str, key: &str) -> Option<&Value> {
        self.saved
            .get("moves")
            .and_then(|moves| moves.get(mv))
            .and_then(|setting| setting.get(key))
    }
    fn prompt(&self, mv: &str) -> (Option<String>, bool) {
        match self.setting(mv, "prompt") {
            Some(Value::String(prompt)) => (Some(prompt.clone()), true),
            Some(_) => (None, true),
            None => (None, false),
        }
    }
    fn runs_at(&self, mv: &str) -> io::Result<(RunsAt, bool)> {
        match self.setting(mv, "runs_at") {
            Some(value) => Ok((RunsAt::parse(value)?, true)),
            None => Ok((
                if mv == "close" {
                    RunsAt::DayStart
                } else {
                    RunsAt::Off
                },
                false,
            )),
        }
    }
    fn effective_prompt(&self, mv: &str) -> String {
        self.prompt(mv)
            .0
            .unwrap_or_else(|| own_prompt(mv).to_owned())
    }
    /// A move is armed when the person saved a schedule for it.
    fn armed(&self, mv: &str) -> io::Result<bool> {
        let (runs_at, saved) = self.runs_at(mv)?;
        Ok(saved && runs_at != RunsAt::Off)
    }
}

fn load_settings(scope: &Scope) -> io::Result<Loaded> {
    let raw = read_optional(scope, &settings_path(scope))?;
    let Some(content) = raw else {
        return Ok(Loaded {
            raw: None,
            revision: "absent".into(),
            saved: Map::new(),
        });
    };
    let value: Value = serde_json::from_str(&content).map_err(|_| {
        invalid("the Day's settings source is not JSON; nothing is guessed from it")
    })?;
    let saved = value
        .as_object()
        .cloned()
        .ok_or_else(|| invalid("the Day's settings source is not an object"))?;
    validate_saved(scope, &saved)?;
    Ok(Loaded {
        revision: source::revision(&content),
        raw: Some(content),
        saved,
    })
}

fn validate_saved(scope: &Scope, saved: &Map<String, Value>) -> io::Result<()> {
    for key in saved.keys() {
        if !matches!(
            key.as_str(),
            "schema"
                | "scope_ref"
                | "rollover"
                | "agent_writes"
                | "sources"
                | "close_routine"
                | "moves"
                | "on_since"
                | "day_form"
        ) {
            return Err(invalid(format!("the Day's settings have no `{key}`")));
        }
    }
    match saved.get("day_form") {
        None | Some(Value::Null) => {}
        Some(Value::String(path)) => {
            day_form_path(path)?;
        }
        Some(_) => return Err(invalid(DAY_FORM_SHAPE)),
    }
    if saved.get("schema") != Some(&json!(SETTINGS_SCHEMA)) {
        return Err(invalid(format!(
            "the Day's settings schema is {SETTINGS_SCHEMA}"
        )));
    }
    if saved.get("scope_ref") != Some(&json!(scope.world_ref)) {
        return Err(invalid("the Day's settings belong to another World"));
    }
    match saved.get("rollover") {
        None | Some(Value::Null) => {}
        Some(Value::String(time)) => {
            canonical_time(time)?;
        }
        Some(_) => return Err(invalid("rollover is a time as HH:MM, or null for Off")),
    }
    if let Some(value) = saved.get("agent_writes") {
        if !value
            .as_str()
            .is_some_and(|value| AGENT_WRITES.contains(&value))
        {
            return Err(invalid("agent_writes is off, propose or write"));
        }
    }
    if saved.get("close_routine").is_some_and(|v| !v.is_boolean()) {
        return Err(invalid("close_routine is true or false"));
    }
    if let Some(sources) = saved.get("sources") {
        let sources = sources
            .as_object()
            .ok_or_else(|| invalid(sources_shape()))?;
        for (name, value) in sources {
            if !SOURCES.contains(&name.as_str()) {
                return Err(invalid(format!(
                    "`{name}` is not one of the Day's sources: {}",
                    sources_shape()
                )));
            }
            if !value.is_boolean() {
                return Err(invalid(format!("source {name} is true or false")));
            }
        }
    }
    if let Some(on_since) = saved.get("on_since") {
        let on_since = on_since
            .as_object()
            .ok_or_else(|| invalid("on_since is {field: milliseconds}"))?;
        for (name, value) in on_since {
            if name != "field" || !value.as_i64().is_some_and(|at| at >= 0) {
                return Err(invalid(
                    "on_since keeps when the field last turned On, as milliseconds",
                ));
            }
        }
    }
    if let Some(moves) = saved.get("moves") {
        let moves = moves
            .as_object()
            .ok_or_else(|| invalid("moves is {tasks, patterns, context, synthesis, close}"))?;
        for (name, setting) in moves {
            if !MOVES.contains(&name.as_str()) {
                return Err(invalid(format!("`{name}` is not one of the Day's moves")));
            }
            let setting = setting
                .as_object()
                .ok_or_else(|| invalid(format!("move {name} is {{prompt, runs_at}}")))?;
            for (key, value) in setting {
                match key.as_str() {
                    "prompt" => match value {
                        Value::Null => {}
                        Value::String(prompt)
                            if !prompt.trim().is_empty() && prompt.len() <= 8192 => {}
                        _ => {
                            return Err(invalid(format!(
                                "{name}'s prompt is text of at most 8192 bytes, or null for the move's own"
                            )))
                        }
                    },
                    "runs_at" => {
                        RunsAt::parse(value)?;
                    }
                    other => return Err(invalid(format!("move {name} has no `{other}`"))),
                }
            }
        }
    }
    // A move cannot run at a rollover that is Off.
    let rollover_on = matches!(saved.get("rollover"), Some(Value::String(_)));
    if !rollover_on {
        for mv in MOVES {
            if let Some(runs_at) = saved
                .get("moves")
                .and_then(|moves| moves.get(mv))
                .and_then(|setting| setting.get("runs_at"))
            {
                if RunsAt::parse(runs_at)? == RunsAt::Rollover {
                    return Err(invalid(format!(
                        "Rollover is Off, so a move cannot run at rollover: {}'s Runs at is set to it",
                        label(mv)
                    )));
                }
            }
        }
    }
    Ok(())
}

const DAY_FORM_SHAPE: &str = "day_form is the Day form's path under your Central root (as Control/forms/ql-daily-die.html), or null for none";

/// A saved Day form's path, as the settings keep it: relative to the world's Central root, plain components only.
fn day_form_path(path: &str) -> io::Result<&str> {
    let relative = std::path::Path::new(path);
    if path.is_empty()
        || path.len() > 1024
        || path.contains('\0')
        || relative.is_absolute()
        || !relative
            .components()
            .all(|part| matches!(part, std::path::Component::Normal(_)))
    {
        return Err(invalid(DAY_FORM_SHAPE));
    }
    Ok(path)
}

/// The Day form the person saved in the Day's settings, or None while they have saved none (day_turn.rs makes today's page from
/// it, and from nothing else).
pub(crate) fn saved_day_form(scope: &Scope) -> io::Result<Option<String>> {
    Ok(load_settings(scope)?.day_form().0)
}

fn settings_reading(scope: &Scope, loaded: &Loaded) -> io::Result<Value> {
    let standing = |saved: bool| if saved { "saved" } else { "default" };
    let (rollover, rollover_saved) = loaded.rollover();
    let (agent_writes, agent_writes_saved) = loaded.agent_writes();
    let (close_routine, close_routine_saved) = loaded.close_routine();
    let (day_form, day_form_saved) = loaded.day_form();
    let mut sources = Map::new();
    let mut sources_standing = Map::new();
    for name in SOURCES {
        let (on, saved) = loaded.source(name);
        sources.insert(name.into(), json!(on));
        sources_standing.insert(name.into(), json!(standing(saved)));
    }
    let mut moves = Map::new();
    let mut moves_standing = Map::new();
    let mut own = Map::new();
    let mut effective = Map::new();
    for mv in MOVES {
        let (runs_at, runs_at_saved) = loaded.runs_at(mv)?;
        let (prompt, prompt_saved) = loaded.prompt(mv);
        moves.insert(
            mv.into(),
            json!({"prompt":prompt,"runs_at":runs_at.to_json()}),
        );
        moves_standing.insert(
            mv.into(),
            json!({"prompt":standing(prompt_saved),"runs_at":standing(runs_at_saved)}),
        );
        own.insert(mv.into(), json!(own_prompt(mv)));
        effective.insert(mv.into(), json!(loaded.effective_prompt(mv)));
    }
    Ok(json!({
        "schema":"central.day-settings-reading/v1",
        "scope_ref":scope.world_ref,
        "path":settings_path(scope),
        "revision":loaded.revision,
        "saved":loaded.exists(),
        "settings":{"schema":SETTINGS_SCHEMA,"scope_ref":scope.world_ref,"rollover":rollover,
            "agent_writes":agent_writes,"sources":sources,"close_routine":close_routine,"moves":moves,"day_form":day_form},
        "standing":{"rollover":standing(rollover_saved),"agent_writes":standing(agent_writes_saved),
            "sources":sources_standing,"close_routine":standing(close_routine_saved),"moves":moves_standing,
            "day_form":standing(day_form_saved)},
        "own_prompts":own,
        "effective_prompts":effective,
        "nothing_runs_by_itself_on_defaults":true,
        "automatic_agent_or_model_invocation":false,
    }))
}

pub(crate) fn settings_read(scope: &Scope) -> io::Result<Value> {
    settings_reading(scope, &load_settings(scope)?)
}

pub(crate) fn settings_write(
    scope: &Scope,
    input: &Value,
    principal: &Principal,
    now: u64,
) -> io::Result<Value> {
    principal.require_human()?;
    let object = input
        .as_object()
        .ok_or_else(|| invalid("native operation requires an object"))?;
    let expected = text(input, "expected_revision")?;
    let mut changes = 0;
    for key in object.keys() {
        match key.as_str() {
            "project" | "expected_revision" | "expected_authority_revision" => {}
            "rollover" | "agent_writes" | "sources" | "close_routine" | "moves" | "day_form" => {
                changes += 1
            }
            other => return Err(invalid(format!("the Day's settings have no `{other}`"))),
        }
    }
    if changes == 0 {
        return Err(invalid(
            "central.day.settings.write needs at least one setting to save",
        ));
    }
    let current = load_settings(scope)?;
    if current.revision != expected {
        return Err(conflict("the Day's settings changed; read them again"));
    }
    // The file holds only what the person set: a patch over what is saved.
    let mut merged = current.saved.clone();
    merged.insert("schema".into(), json!(SETTINGS_SCHEMA));
    merged.insert("scope_ref".into(), json!(scope.world_ref));
    if let Some(value) = object.get("rollover") {
        merged.insert(
            "rollover".into(),
            match value {
                Value::Null => Value::Null,
                Value::String(time) => json!(canonical_time(time)?),
                _ => return Err(invalid("rollover is a time as HH:MM, or null for Off")),
            },
        );
    }
    for key in ["agent_writes", "close_routine"] {
        if let Some(value) = object.get(key) {
            merged.insert(key.into(), value.clone());
        }
    }
    // The Day form is saved only as a Day form Central reads where it stands: the turn makes today's page from it alone.
    if let Some(value) = object.get("day_form") {
        merged.insert(
            "day_form".into(),
            match value {
                Value::Null => Value::Null,
                Value::String(path) => {
                    super::day_turn::read_day_form(scope, day_form_path(path)?)?;
                    json!(path)
                }
                _ => return Err(invalid(DAY_FORM_SHAPE)),
            },
        );
    }
    if let Some(patch) = object.get("sources") {
        let patch = patch.as_object().ok_or_else(|| invalid(sources_shape()))?;
        let mut sources = merged
            .get("sources")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        for (name, value) in patch {
            sources.insert(name.clone(), value.clone());
        }
        merged.insert("sources".into(), Value::Object(sources));
        // The field turning On from Off is kept: what came while it was Off is
        // never recorded after (DLW build ruling 2026-11-08).
        let now_on = merged["sources"]
            .get("field")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        if now_on && !current.source("field").0 {
            let mut on_since = merged
                .get("on_since")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
            on_since.insert("field".into(), json!((now as i64) * 1000));
            merged.insert("on_since".into(), Value::Object(on_since));
        }
    }
    if let Some(patch) = object.get("moves") {
        let patch = patch
            .as_object()
            .ok_or_else(|| invalid("moves is {tasks, patterns, context, synthesis, close}"))?;
        let mut moves = merged
            .get("moves")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        for (name, setting) in patch {
            let setting = setting
                .as_object()
                .ok_or_else(|| invalid(format!("move {name} is {{prompt, runs_at}}")))?;
            let mut entry = moves
                .get(name)
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
            for (key, value) in setting {
                entry.insert(
                    key.clone(),
                    if key == "runs_at" {
                        RunsAt::parse(value)?.to_json()
                    } else {
                        value.clone()
                    },
                );
            }
            moves.insert(name.clone(), Value::Object(entry));
        }
        merged.insert("moves".into(), Value::Object(moves));
    }
    validate_saved(scope, &merged)?;
    let content = encoded(&Value::Object(merged.clone()))?;
    let path = settings_path(scope);
    if current.raw.as_deref() != Some(content.as_str()) {
        match current.raw {
            None => {
                source::put_new(&scope.root, &path, &content)?;
                scope.reconcile(
                    Some((&principal.principal_ref, &principal.actor_kind, None)),
                    &[scope.source_ref(&path)],
                )?;
            }
            Some(_) => {
                let reading = scope.read(&scope.source_ref(&path))?;
                history::replace(
                    scope,
                    &reading,
                    &content,
                    &principal.principal_ref,
                    &principal.actor_kind,
                    now,
                )?;
            }
        }
    }
    let mut result = settings_read(scope)?;
    result["written_by"] = json!(principal.principal_ref);
    Ok(result)
}

// ---- schedules --------------------------------------------------------------

fn boundary_time(time: &TimeReading) -> String {
    let minutes = time.policy.day_boundary_minutes;
    format!("{:02}:{:02}", minutes / 60, minutes % 60)
}

/// The daily schedule a `runs_at` stands for: Day start is the civil day
/// boundary of the time policy, rollover is the saved Rollover time.
fn schedule_for(
    runs_at: &RunsAt,
    settings: &Loaded,
    time: &TimeReading,
) -> io::Result<Option<Value>> {
    Ok(match runs_at {
        RunsAt::Off => None,
        RunsAt::Daily(at) => Some(json!({"kind":"daily","time":at})),
        RunsAt::DayStart => Some(json!({"kind":"daily","time":boundary_time(time)})),
        RunsAt::Rollover => {
            let (rollover, _) = settings.rollover();
            let at = rollover
                .ok_or_else(|| invalid("Rollover is Off, so a move cannot run at rollover"))?;
            Some(json!({"kind":"daily","time":at}))
        }
    })
}

/// The same occurrence resolution as `central.time.occurrences`.
fn instants(scope: &Scope, schedule: &Value, from_ms: i64, to_ms: i64) -> io::Result<Vec<i64>> {
    let resolved = temporal::time_occurrences(
        scope,
        &json!({"schedule":schedule,"window_from_unix_ms":from_ms,"window_to_unix_ms":to_ms}),
    )?;
    Ok(resolved["occurrences"]
        .as_array()
        .map(|rows| {
            rows.iter()
                .filter_map(|row| row["due_unix_ms"].as_i64())
                .collect()
        })
        .unwrap_or_default())
}

fn civil_date_at(scope: &Scope, instant_ms: i64) -> io::Result<String> {
    let seconds = u64::try_from(instant_ms.div_euclid(1000))
        .map_err(|_| invalid("instant outside the supported range"))?;
    Ok(temporal::time_policy(scope, seconds)?.civil_date)
}

/// The civil date of the Day an occurrence belongs to, by the one rule the
/// civil date is named by (temporal::civil_date_in, DLW build ruling
/// 2026-11-09): the Day it falls on, except that Close at Day start closes the
/// Day before the one its occurrence falls on. That is the Day that ends there:
/// in a spring-forward gap the boundary's occurrence resolves forward, past the
/// instant the date flips, so the instant before it is the new Day's already.
fn occurrence_civil_date(
    scope: &Scope,
    mv: &str,
    kind: &str,
    instant_ms: i64,
) -> io::Result<String> {
    let date = civil_date_at(scope, instant_ms)?;
    if mv == "close" && kind == "day_start" {
        previous_civil_date(&date)
    } else {
        Ok(date)
    }
}

pub(super) fn day_ref_of(scope: &Scope, civil_date: &str) -> String {
    format!("central:day:{}:{}", scope.world_ref, civil_date)
}

fn previous_civil_date(civil_date: &str) -> io::Result<String> {
    let date = NaiveDate::parse_from_str(civil_date, "%Y-%m-%d")
        .map_err(|_| invalid("a Day's civil date is YYYY-MM-DD"))?;
    Ok(date
        .checked_sub_signed(Duration::days(1))
        .ok_or_else(|| invalid("civil date out of range"))?
        .to_string())
}

fn next_civil_date(civil_date: &str) -> io::Result<String> {
    let date = NaiveDate::parse_from_str(civil_date, "%Y-%m-%d")
        .map_err(|_| invalid("a Day's civil date is YYYY-MM-DD"))?;
    Ok(date
        .checked_add_signed(Duration::days(1))
        .ok_or_else(|| invalid("civil date out of range"))?
        .to_string())
}

fn timezone_of(time: &TimeReading) -> io::Result<Tz> {
    time.policy
        .timezone
        .parse()
        .map_err(|_| invalid("civil-time policy requires an IANA timezone"))
}

fn clock_of(time: &TimeReading, at_ms: i64) -> io::Result<String> {
    let instant = DateTime::<Utc>::from_timestamp_millis(at_ms)
        .ok_or_else(|| invalid("instant outside the supported range"))?;
    Ok(instant
        .with_timezone(&timezone_of(time)?)
        .format("%H:%M")
        .to_string())
}

// ---- the run record ---------------------------------------------------------

pub(super) struct Log {
    exists: bool,
    raw: String,
    revision: String,
    pub(super) events: Vec<Value>,
}

fn load_log(scope: &Scope, civil_date: &str) -> io::Result<Log> {
    load_lines(
        scope,
        &record_path(scope, civil_date),
        EVENT_SCHEMA,
        "move record",
    )
}

/// One of the Day's append-only records: every line one document of `schema`.
pub(super) fn load_lines(scope: &Scope, path: &str, schema: &str, what: &str) -> io::Result<Log> {
    let Some(raw) = read_optional(scope, path)? else {
        return Ok(Log {
            exists: false,
            raw: String::new(),
            revision: "absent".into(),
            events: vec![],
        });
    };
    let mut events = Vec::new();
    for line in raw.lines().filter(|line| !line.trim().is_empty()) {
        let event: Value = serde_json::from_str(line).map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("a line of the Day's {what} is not JSON; it is kept as it is"),
            )
        })?;
        if event["schema"] != schema {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("a line of the Day's {what} is not a {schema}"),
            ));
        }
        events.push(event);
    }
    Ok(Log {
        exists: true,
        revision: source::revision(&raw),
        raw,
        events,
    })
}

/// Append one event. Earlier lines are never rewritten: the new content is the
/// old bytes plus one line, published on the exact revision it was read at.
fn append_event(
    scope: &Scope,
    civil_date: &str,
    log: &Log,
    event: &Value,
    principal: &Principal,
) -> io::Result<()> {
    append_line(
        scope,
        &record_path(scope, civil_date),
        log,
        event,
        principal,
    )
}

fn append_line(
    scope: &Scope,
    path: &str,
    log: &Log,
    event: &Value,
    principal: &Principal,
) -> io::Result<()> {
    let by = (
        principal.principal_ref.as_str(),
        principal.actor_kind.as_str(),
    );
    append_lines(scope, path, log, std::slice::from_ref(event), by)
}

/// Append several events in one rewrite, on the exact revision read.
/// `by` is what source history names as the writer (actor, actor kind): the
/// principal for what it recorded itself, the Day's field source for what
/// reached you from the field.
pub(super) fn append_lines(
    scope: &Scope,
    path: &str,
    log: &Log,
    events: &[Value],
    by: (&str, &str),
) -> io::Result<()> {
    let path = path.to_owned();
    let mut content = log.raw.clone();
    if !content.is_empty() && !content.ends_with('\n') {
        content.push('\n');
    }
    for event in events {
        content.push_str(&serde_json::to_string(event)?);
        content.push('\n');
    }
    if log.exists {
        crate::source_safety::replace(&scope.root, &path, &log.revision, &content)?;
    } else {
        source::put_new(&scope.root, &path, &content)?;
    }
    scope.reconcile(Some((by.0, by.1, None)), &[scope.source_ref(&path)])?;
    Ok(())
}

/// Every run of a Day, oldest first: each begin with its end, when it has one.
fn runs_of(events: &[Value]) -> io::Result<Vec<Value>> {
    let broken = |what: &str| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("the Day's move record is inconsistent: {what}"),
        )
    };
    let mut order: Vec<String> = Vec::new();
    let mut begins: BTreeMap<String, Value> = BTreeMap::new();
    let mut ends: BTreeMap<String, Value> = BTreeMap::new();
    for event in events {
        let run_ref = event["run_ref"]
            .as_str()
            .ok_or_else(|| broken("an event has no run_ref"))?
            .to_owned();
        match event["event"].as_str() {
            Some("begin") => {
                if begins.insert(run_ref.clone(), event.clone()).is_some() {
                    return Err(broken("a run begins twice"));
                }
                order.push(run_ref);
            }
            Some("end") => {
                if ends.insert(run_ref, event.clone()).is_some() {
                    return Err(broken("a run ends twice"));
                }
            }
            _ => return Err(broken("an event is neither a begin nor an end")),
        }
    }
    if ends.keys().any(|run_ref| !begins.contains_key(run_ref)) {
        return Err(broken("a run ends that never began"));
    }
    Ok(order
        .into_iter()
        .map(|run_ref| {
            let begin = begins[&run_ref].clone();
            let end = ends.get(&run_ref).cloned();
            json!({
                "run_ref":run_ref,
                "move":begin["move"],
                "day_ref":begin["day_ref"],
                "state":end.as_ref().map(|e| e["state"].clone()).unwrap_or(json!("running")),
                "trigger":begin["trigger"],
                "occurrence_ref":begin.get("occurrence_ref").cloned().unwrap_or(Value::Null),
                "prompt":begin["prompt"],
                "agent_writes":begin["agent_writes"],
                "begun_at_unix_ms":begin["at_unix_ms"],
                "ended_at_unix_ms":end.as_ref().map(|e| e["at_unix_ms"].clone()).unwrap_or(Value::Null),
                "begin":begin,
                "end":end,
            })
        })
        .collect())
}

fn move_name(input: &Value) -> io::Result<&str> {
    let mv = text(input, "move")?;
    MOVES
        .contains(&mv)
        .then_some(mv)
        .ok_or_else(|| invalid("move is tasks, patterns, context, synthesis or close"))
}

fn instant_ms(input: &Value) -> io::Result<i64> {
    input
        .get("at_unix_ms")
        .and_then(Value::as_i64)
        .filter(|at| *at >= 0)
        .ok_or_else(|| invalid("at_unix_ms is when it happened at its source, as milliseconds"))
}

/// The Day an event is about: the one named, or the one Central names as today.
fn target_day(relations: &Value, input: &Value) -> io::Result<(String, Value)> {
    let day_ref = match input.get("day_ref") {
        None | Some(Value::Null) => relations["temporal"]["today"]["day_ref"]
            .as_str()
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    "no DayRef or current today pointer",
                )
            })?
            .to_owned(),
        Some(Value::String(day_ref)) if !day_ref.trim().is_empty() => day_ref.clone(),
        _ => return Err(invalid("day_ref is a DayRef, or absent for today")),
    };
    let entry = temporal::day_relation(relations, &day_ref)?.clone();
    Ok((day_ref, entry))
}

fn civil_date_of(entry: &Value) -> io::Result<String> {
    entry["temporal"]["civil_date"]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| invalid("a Day relation lacks its civil date"))
}

/// A scheduled begin is only ever a saved schedule's own occurrence, on the Day
/// the occurrence belongs to: nothing runs by itself on the defaults.
fn verify_scheduled(
    scope: &Scope,
    settings: &Loaded,
    time: &TimeReading,
    mv: &str,
    occurrence: &str,
    civil_date: &str,
) -> io::Result<()> {
    let malformed =
        || invalid("occurrence_ref is <move>:<kind>:<instant> for the move that begins");
    let parts: Vec<&str> = occurrence.splitn(3, ':').collect();
    if parts.len() != 3 || parts[0] != mv {
        return Err(malformed());
    }
    let kind = parts[1];
    let instant: i64 = parts[2].parse().map_err(|_| malformed())?;
    if !matches!(kind, "daily" | "rollover" | "day_start") {
        return Err(malformed());
    }
    let (runs_at, saved) = settings.runs_at(mv)?;
    if !saved || runs_at == RunsAt::Off {
        return Err(denied(format!(
            "Nothing runs by itself on the Day's defaults: {}'s Runs at is not a schedule you saved.",
            label(mv)
        )));
    }
    if runs_at.kind() != kind {
        return Err(invalid(format!(
            "{} runs at {}, not {kind}",
            label(mv),
            runs_at.kind()
        )));
    }
    let schedule = schedule_for(&runs_at, settings, time)?.ok_or_else(malformed)?;
    if !instants(scope, &schedule, instant, instant)?.contains(&instant) {
        return Err(invalid(format!(
            "{occurrence} is no occurrence of {}'s saved schedule",
            label(mv)
        )));
    }
    let belongs = occurrence_civil_date(scope, mv, kind, instant)?;
    if belongs != civil_date {
        return Err(invalid(format!(
            "{occurrence} belongs to the Day of {belongs}, not the Day of {civil_date}"
        )));
    }
    Ok(())
}

fn parse_run_ref(run_ref: &str) -> io::Result<(String, String)> {
    let rest = run_ref
        .strip_prefix(RUN_REF_PREFIX)
        .ok_or_else(|| invalid("run_ref is central:day-move-run:<day_ref>:<move>:<n>"))?;
    let mut parts = rest.rsplitn(3, ':');
    let (number, mv, day_ref) = (parts.next(), parts.next(), parts.next());
    match (number, mv, day_ref) {
        (Some(number), Some(mv), Some(day_ref))
            if number.parse::<u64>().is_ok() && MOVES.contains(&mv) && !day_ref.is_empty() =>
        {
            Ok((day_ref.to_owned(), mv.to_owned()))
        }
        _ => Err(invalid(
            "run_ref is central:day-move-run:<day_ref>:<move>:<n>",
        )),
    }
}

/// A list of refs a run gave, each on one line and plain, as a record's names
/// are (`plain_characters`; be2j ruling 3): the Day shows them as they are kept.
fn bounded_text_list(gave: &Value, key: &str) -> io::Result<Vec<String>> {
    match gave.get(key) {
        None | Some(Value::Null) => Ok(vec![]),
        Some(Value::Array(rows)) if rows.len() <= 256 => rows
            .iter()
            .map(|row| {
                let row = row
                    .as_str()
                    .filter(|row| !row.trim().is_empty() && row.len() <= 4096)
                    .ok_or_else(|| invalid(format!("gave.{key} is a list of refs")))?;
                if !plain_characters(row, false) {
                    return Err(invalid(format!(
                        "gave.{key} is a list of refs, each on one line, with no control character and no invisible one (a line or paragraph separator, a format character or a blank filler)"
                    )));
                }
                Ok(row.to_owned())
            })
            .collect(),
        _ => Err(invalid(format!("gave.{key} is a list of refs"))),
    }
}

/// Central's code for an end given to a run that has ended already (be2j.c3,
/// DLW build ruling 2026-11-11). Before, it answered with the one code every
/// conflict has (`stale_basis_or_identity_conflict`, `action_result` in
/// `mod.rs`), which also says that the Day's record changed under a write. The
/// two ask different things of a caller: a changed Day is read again and the
/// same end given once more, an ended run is finished and nothing is given
/// again. A caller told them apart by reading the Day's moves a second time
/// (AIKit's runner, `crates/aikit-cli/src/day_move_runner.rs`); now the code
/// says which, and the refusal carries the run as it ended. Every other
/// conflict keeps the old code.
pub(crate) const RUN_ENDED_ALREADY: &str = "run_ended_already";

/// What an end given to an ended run is refused with: its words are the ones
/// the refusal always had, and it carries the run as Central holds it.
#[derive(Debug)]
struct EndedAlready {
    run_ref: String,
    run: Value,
}

impl std::fmt::Display for EndedAlready {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} has ended already", self.run_ref)
    }
}

impl std::error::Error for EndedAlready {}

/// The Action result of an end refused because its run has ended already, or
/// none for any other error. Its status and its words are what they were; the
/// code is the run's own (`RUN_ENDED_ALREADY`), and beside what every native
/// refusal's details carry stand the run's ref and the run as it ended, with
/// its state and what it gave.
pub(crate) fn ended_already_result(action: &str, error: &io::Error) -> Option<ActionResult> {
    let ended = error.get_ref()?.downcast_ref::<EndedAlready>()?;
    Some(ActionResult::failure_coded(
        Some(action),
        crate::result::ResultStatus::VerificationFailure,
        RUN_ENDED_ALREADY,
        error.to_string(),
        Some(
            json!({"retry_policy_action":"central.work.policy","automatic_authority_widening":false,
            "outside_writes_prevented":false,"run_ref":ended.run_ref,"run":ended.run}),
        ),
    ))
}

fn run_result(created: bool, run: &Value, revision: &str) -> Value {
    json!({"schema":"central.day-move-run-record/v1","created":created,"run":run,"revision":revision,
        "automatic_agent_or_model_invocation":false})
}

pub(crate) fn move_record(
    scope: &Scope,
    input: &Value,
    principal: &Principal,
    now: u64,
) -> io::Result<Value> {
    let time = temporal::time_policy(scope, now)?;
    let recorded_at_unix_ms = (now as i64) * 1000;
    match text(input, "event")? {
        "begin" => {
            let mv = move_name(input)?;
            let at_ms = instant_ms(input)?;
            let (relations, _) = scope.relations()?;
            let (day_ref, entry) = target_day(&relations, input)?;
            let civil_date = civil_date_of(&entry)?;
            if entry["temporal"]["lifecycle"] == "closed" {
                return Err(conflict(format!(
                    "The Day of {civil_date} is closed; a closed Day is kept and no move begins on it"
                )));
            }
            let trigger = text(input, "trigger")?;
            let occurrence = match input.get("occurrence_ref") {
                None | Some(Value::Null) => None,
                Some(Value::String(occurrence)) if !occurrence.trim().is_empty() => {
                    Some(occurrence.as_str())
                }
                _ => return Err(invalid("occurrence_ref is text, or absent")),
            };
            let settings = load_settings(scope)?;
            match (trigger, occurrence) {
                ("press", None) => {}
                ("press", Some(_)) => {
                    return Err(invalid(
                        "a press has no occurrence_ref; only a schedule does",
                    ))
                }
                ("schedule", None) => {
                    return Err(invalid("a scheduled begin names its occurrence_ref"))
                }
                ("schedule", Some(occurrence)) => {
                    verify_scheduled(scope, &settings, &time, mv, occurrence, &civil_date)?
                }
                _ => return Err(invalid("trigger is press or schedule")),
            }
            let log = load_log(scope, &civil_date)?;
            let runs = runs_of(&log.events)?;
            if let Some(occurrence) = occurrence {
                if let Some(existing) = runs
                    .iter()
                    .find(|run| run["move"] == mv && run["occurrence_ref"] == occurrence)
                {
                    return Ok(run_result(false, existing, &log.revision));
                }
            }
            if let Some(running) = runs
                .iter()
                .find(|run| run["move"] == mv && run["state"] == "running")
            {
                return Err(conflict(format!(
                    "{} is running since {}",
                    label(mv),
                    clock_of(&time, running["begun_at_unix_ms"].as_i64().unwrap_or(at_ms))?
                )));
            }
            let number = runs.iter().filter(|run| run["move"] == mv).count() + 1;
            let run_ref = format!("{RUN_REF_PREFIX}{day_ref}:{mv}:{number}");
            let mut event = json!({"schema":EVENT_SCHEMA,"event":"begin","run_ref":run_ref,"day_ref":day_ref,
                "move":mv,"at_unix_ms":at_ms,"trigger":trigger,"prompt":settings.effective_prompt(mv),
                "agent_writes":settings.agent_writes().0,"actor_ref":principal.principal_ref,
                "recorded_at_unix_ms":recorded_at_unix_ms});
            if let Some(occurrence) = occurrence {
                event["occurrence_ref"] = json!(occurrence);
            }
            append_event(scope, &civil_date, &log, &event, principal)?;
            let log = load_log(scope, &civil_date)?;
            let run = runs_of(&log.events)?
                .into_iter()
                .find(|run| run["run_ref"] == run_ref.as_str())
                .ok_or_else(|| invalid("the run was not recorded"))?;
            Ok(run_result(true, &run, &log.revision))
        }
        "end" => {
            let run_ref = text(input, "run_ref")?;
            let (day_ref, mv) = parse_run_ref(run_ref)?;
            if input.get("move").is_some_and(|given| given != mv.as_str()) {
                return Err(invalid("move does not match the run_ref"));
            }
            let at_ms = instant_ms(input)?;
            let (relations, _) = scope.relations()?;
            let entry = temporal::day_relation(&relations, &day_ref)
                .map_err(|_| invalid("unknown run: its Day is not recorded in this World"))?
                .clone();
            let civil_date = civil_date_of(&entry)?;
            let log = load_log(scope, &civil_date)?;
            let runs = runs_of(&log.events)?;
            let run = runs
                .iter()
                .find(|run| run["run_ref"] == run_ref)
                .ok_or_else(|| invalid("unknown run: no begin is recorded for this run_ref"))?;
            if !run["end"].is_null() {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    EndedAlready {
                        run_ref: run_ref.to_owned(),
                        run: run.clone(),
                    },
                ));
            }
            let begun_at = run["begun_at_unix_ms"].as_i64().unwrap_or(0);
            if at_ms < begun_at {
                return Err(invalid("a run cannot end before it began"));
            }
            let state = text(input, "state")?;
            if !matches!(state, "done" | "refused" | "stalled" | "stopped") {
                return Err(invalid("state is done, refused, stalled or stopped"));
            }
            let reason = match input.get("reason") {
                None | Some(Value::Null) => None,
                Some(Value::String(reason))
                    if !reason.trim().is_empty() && reason.len() <= 4096 =>
                {
                    // The Day shows why a run stopped as it is kept: plain text,
                    // as a record's words are (be2j ruling 3).
                    if !plain_characters(reason, true) {
                        return Err(invalid(
                            "reason holds no control character but line breaks and tabs (a \\r only before a \\n), and no invisible one (a line or paragraph separator, a format character or a blank filler)",
                        ));
                    }
                    Some(reason.clone())
                }
                _ => return Err(invalid("reason is text, or absent")),
            };
            if matches!(state, "refused" | "stalled") && reason.is_none() {
                return Err(invalid(format!(
                    "a run that {state} says why in a reason: nothing runs where you cannot see it"
                )));
            }
            let gave = match input.get("gave") {
                None | Some(Value::Null) => None,
                Some(gave) => {
                    let object = gave.as_object().ok_or_else(|| {
                        invalid("gave is {summary, proposals?, writes?, counts?, points_at?}")
                    })?;
                    if let Some(key) = object.keys().find(|key| {
                        !matches!(
                            key.as_str(),
                            "summary" | "proposals" | "writes" | "counts" | "points_at"
                        )
                    }) {
                        return Err(invalid(format!("gave has no `{key}`")));
                    }
                    // The Patterns move names the one skill or method its
                    // pattern's items point at, so the pattern can open a
                    // refinement of it (O-I manual §18.2, §19.7).
                    match object.get("points_at") {
                        None | Some(Value::Null) => {}
                        Some(_) if mv != "patterns" => {
                            return Err(invalid(
                                "only the Patterns move names what its pattern's items point at",
                            ))
                        }
                        Some(Value::String(points_at))
                            if !points_at.is_empty()
                                && points_at.len() <= 512
                                && !points_at.chars().any(|c| {
                                    c.is_whitespace() || c.is_control() || invisible(c)
                                }) => {}
                        Some(_) => {
                            return Err(invalid(
                                "gave.points_at names one skill or method by its ref",
                            ))
                        }
                    }
                    // An agent's words, as the Day's moves give them back: plain
                    // text, as a record's words are (be2i ruling 7).
                    if !plain_characters(text(gave, "summary")?, true) {
                        return Err(invalid(
                            "gave.summary holds no control character but line breaks and tabs (a \\r only before a \\n), and no invisible one (a line or paragraph separator, a format character or a blank filler)",
                        ));
                    }
                    let proposals = bounded_text_list(gave, "proposals")?;
                    let writes = bounded_text_list(gave, "writes")?;
                    match run["agent_writes"].as_str().unwrap_or("propose") {
                        "off" if !proposals.is_empty() || !writes.is_empty() => {
                            return Err(invalid("Agent writes was Off when this run began, so it gave no proposals or writes"))
                        }
                        "propose" if !writes.is_empty() => {
                            return Err(invalid("Agent writes was Propose when this run began, so it wrote nothing directly"))
                        }
                        _ => {}
                    }
                    match object.get("counts") {
                        None | Some(Value::Null) => {}
                        Some(Value::Object(counts))
                            if serde_json::to_string(counts)?.len() <= 16 * 1024 => {}
                        _ => return Err(invalid("gave.counts is an object")),
                    }
                    Some(gave.clone())
                }
            };
            if state == "done" && gave.is_none() {
                return Err(invalid("a run that is done says what it gave"));
            }
            let mut event = json!({"schema":EVENT_SCHEMA,"event":"end","run_ref":run_ref,"day_ref":day_ref,
                "move":mv,"at_unix_ms":at_ms,"state":state,"actor_ref":principal.principal_ref,
                "recorded_at_unix_ms":recorded_at_unix_ms});
            if let Some(gave) = gave {
                event["gave"] = gave;
            }
            if let Some(reason) = reason {
                event["reason"] = json!(reason);
            }
            append_event(scope, &civil_date, &log, &event, principal)?;
            let log = load_log(scope, &civil_date)?;
            let run = runs_of(&log.events)?
                .into_iter()
                .find(|run| run["run_ref"] == run_ref)
                .ok_or_else(|| invalid("the end was not recorded"))?;
            Ok(run_result(true, &run, &log.revision))
        }
        _ => Err(invalid("event is begin or end")),
    }
}

// ---- readings ---------------------------------------------------------------

pub(crate) fn moves_read(scope: &Scope, input: &Value, now: u64) -> io::Result<Value> {
    let time = temporal::time_policy(scope, now)?;
    let (relations, _) = scope.relations()?;
    let (day_ref, entry) = target_day(&relations, input)?;
    let civil_date = civil_date_of(&entry)?;
    let settings = load_settings(scope)?;
    let log = load_log(scope, &civil_date)?;
    let runs = runs_of(&log.events)?;
    let now_ms = (now as i64) * 1000;
    let mut moves = Map::new();
    for mv in MOVES {
        let (runs_at, runs_at_saved) = settings.runs_at(mv)?;
        let (_, prompt_saved) = settings.prompt(mv);
        let armed = settings.armed(mv)?;
        let next_at = if armed {
            match schedule_for(&runs_at, &settings, &time)? {
                Some(schedule) => {
                    instants(scope, &schedule, now_ms + 1, now_ms + 72 * 3600 * 1000)?
                        .into_iter()
                        .find(|instant| *instant > now_ms)
                        .map_or(Value::Null, |instant| json!(instant))
                }
                None => Value::Null,
            }
        } else {
            Value::Null
        };
        let own: Vec<&Value> = runs.iter().filter(|run| run["move"] == mv).collect();
        let running = own
            .iter()
            .find(|run| run["state"] == "running")
            .map_or(Value::Null, |run| (*run).clone());
        let last = own
            .iter()
            .filter(|run| !run["end"].is_null())
            .max_by_key(|run| run["ended_at_unix_ms"].as_i64().unwrap_or(0))
            .map_or(Value::Null, |run| (*run).clone());
        moves.insert(
            mv.into(),
            json!({
                "move":mv,"label":label(mv),
                "runs_at":runs_at.to_json(),
                "runs_at_standing":if runs_at_saved {"saved"} else {"default"},
                "prompt":settings.effective_prompt(mv),
                "own_prompt":own_prompt(mv),
                "prompt_standing":if prompt_saved {"saved"} else {"default"},
                "armed":armed,
                "next_at_unix_ms":next_at,
                "running":running,
                "last":last,
                "runs":own,
            }),
        );
    }
    Ok(json!({
        "schema":"central.day-moves-reading/v1",
        "day_ref":day_ref,
        "civil_date":civil_date,
        "lifecycle":entry["temporal"]["lifecycle"],
        "revision":log.revision,
        "settings":settings_reading(scope, &settings)?,
        "moves":moves,
        "automatic_agent_or_model_invocation":false,
    }))
}

pub(crate) fn moves_due(scope: &Scope, input: &Value, now: u64) -> io::Result<Value> {
    let field = |name: &str| {
        input
            .get(name)
            .and_then(Value::as_i64)
            .ok_or_else(|| invalid(format!("{name} must be an integer")))
    };
    let now_ms = field("now_unix_ms")?;
    let since_ms = field("since_unix_ms")?;
    if since_ms > now_ms {
        return Err(invalid("since_unix_ms is after now_unix_ms"));
    }
    if now_ms - since_ms > MAX_DUE_WINDOW_MS {
        return Err(invalid(
            "the window is longer than 400 days; narrow it to what has not been polled",
        ));
    }
    let time = temporal::time_policy(scope, now)?;
    let settings = load_settings(scope)?;
    let mut armed = Vec::new();
    for mv in MOVES {
        if settings.armed(mv)? {
            armed.push(mv);
        }
    }
    let mut result = json!({
        "schema":"central.day-moves-due/v1",
        "now_unix_ms":now_ms,"since_unix_ms":since_ms,
        "standing":if settings.exists() {"saved"} else {"default"},
        "time_policy_revision":time.revision,
        "due":[],
        "automatic_agent_or_model_invocation":false,
    });
    if armed.is_empty() {
        result["reason"] = json!(if settings.exists() {
            NO_SCHEDULE_SAVED
        } else {
            DEFAULTS_REASON
        });
        return Ok(result);
    }
    let mut rows: Vec<(i64, usize, Value)> = Vec::new();
    let mut logs: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    for (position, mv) in MOVES.iter().enumerate() {
        if !armed.contains(mv) {
            continue;
        }
        let (runs_at, _) = settings.runs_at(mv)?;
        let Some(schedule) = schedule_for(&runs_at, &settings, &time)? else {
            continue;
        };
        for instant in instants(scope, &schedule, since_ms + 1, now_ms)? {
            let kind = runs_at.kind();
            let occurrence_ref = format!("{mv}:{kind}:{instant}");
            let civil_date = occurrence_civil_date(scope, mv, kind, instant)?;
            if !logs.contains_key(&civil_date) {
                let runs = runs_of(&load_log(scope, &civil_date)?.events)?;
                logs.insert(civil_date.clone(), runs);
            }
            let begun = logs[&civil_date]
                .iter()
                .find(|run| run["move"] == *mv && run["occurrence_ref"] == occurrence_ref.as_str());
            rows.push((
                instant,
                position,
                json!({
                    "move":mv,"kind":kind,"occurrence_ref":occurrence_ref,"due_unix_ms":instant,
                    "civil_date":civil_date,"day_ref":day_ref_of(scope, &civil_date),
                    "begun":begun.is_some(),
                    "run_ref":begun.map_or(Value::Null, |run| run["run_ref"].clone()),
                }),
            ));
        }
    }
    rows.sort_by_key(|(instant, position, _)| (*instant, *position));
    result["due"] = Value::Array(rows.into_iter().map(|(_, _, row)| row).collect());
    Ok(result)
}

// ---- two Days at a time ------------------------------------------------------

fn set_tomorrow(relations: &mut Value, day_ref: &str, civil_date: &str, now: u64) {
    if !relations["temporal"].is_object() {
        relations["temporal"] = json!({});
    }
    relations["temporal"]["tomorrow"] =
        json!({"day_ref":day_ref,"civil_date":civil_date,"opened_at_unix_seconds":now});
}

/// An authenticated principal that is not the person acts on the person's saved
/// consent only: the Rollover time they saved, once the civil clock has reached it.
fn consent_to_rollover(settings: &Loaded, time: &TimeReading, now: u64) -> io::Result<()> {
    let at = match settings.rollover() {
        (Some(at), true) => at,
        _ => {
            return Err(denied(
                "An agent opens tomorrow's Day only at a Rollover time you saved; the Day's settings hold none (they are the defaults, or Rollover is Off).",
            ))
        }
    };
    let timezone = timezone_of(time)?;
    let local = DateTime::<Utc>::from_timestamp(now as i64, 0)
        .ok_or_else(|| invalid("time outside supported range"))?
        .with_timezone(&timezone);
    let (hour, minute) = temporal::parse_hh_mm(&at)?;
    let boundary = i64::from(time.policy.day_boundary_minutes);
    // Minutes into the civil Day, which begins at the policy's boundary.
    let into_day = |minutes: i64| (minutes - boundary).rem_euclid(1440);
    let wall = i64::from(local.hour()) * 60 + i64::from(local.minute());
    if into_day(wall) < into_day(i64::from(hour) * 60 + i64::from(minute)) {
        return Err(denied(format!(
            "It is not yet the Rollover time you saved, {at}; an agent opens tomorrow's Day then."
        )));
    }
    Ok(())
}

pub(crate) fn rollover(
    scope: &Scope,
    input: &Value,
    principal: &Principal,
    now: u64,
) -> io::Result<Value> {
    let time = temporal::time_policy(scope, now)?;
    if text(input, "expected_time_policy_revision")? != time.revision {
        return Err(conflict(
            "civil-time policy changed before tomorrow's Day opened",
        ));
    }
    let (relations, _) = scope.relations()?;
    let today = &relations["temporal"]["today"];
    let today_date = today["civil_date"].as_str().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "no current Day: ensure today's Day before opening tomorrow's",
        )
    })?;
    if today_date != time.civil_date {
        return Err(invalid(format!(
            "Today's Day is not open: Central's Day is {today_date} and civil today is {}; open today first",
            time.civil_date
        )));
    }
    if !principal.is_human() {
        consent_to_rollover(&load_settings(scope)?, &time, now)?;
    }
    let next_date = next_civil_date(today_date)?;
    let named = &relations["temporal"]["tomorrow"];
    if named["civil_date"] == next_date.as_str() {
        if let Some(day_ref) = named["day_ref"].as_str() {
            if temporal::day_relation(&relations, day_ref).is_ok() {
                let mut result = temporal::day_read(scope, &json!({"day_ref":day_ref}))?;
                result["created"] = json!(false);
                result["today_advanced"] = json!(false);
                return Ok(result);
            }
        }
    }
    let (mut relations, basis, day_ref, created) =
        temporal::open_day(scope, &time, &next_date, now)?;
    set_tomorrow(&mut relations, &day_ref, &next_date, now);
    let entry = temporal::day_relation(&relations, &day_ref)?.clone();
    temporal::save_relations(scope, &relations, &basis)?;
    scope.reconcile(
        Some((&principal.principal_ref, &principal.actor_kind, None)),
        &[text(&entry, "ref")?.into()],
    )?;
    let mut result = temporal::day_read(scope, &json!({"day_ref":day_ref}))?;
    result["created"] = json!(created);
    result["today_advanced"] = json!(false);
    Ok(result)
}

// ---- the close carry --------------------------------------------------------

fn plain_text(html: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.replace("&nbsp;", " ")
}

/// The person's consent to a close they do not press themselves: an agent's Close move, or Central's civil clock at the Day's turn
/// (day_turn.rs). It holds only where they saved Close at Day start, the boundary that ends the Day has passed (`today` is a later
/// civil date), and they have not opened the Day again after its turn. Each refusal says, in words, why the Day stays open.
fn consent_to_close(
    scope: &Scope,
    settings: &Loaded,
    entry: &Value,
    today: &str,
) -> io::Result<()> {
    let civil_date = civil_date_of(entry)?;
    let (runs_at, saved) = settings.runs_at("close")?;
    if !saved {
        return Err(denied(
            "The Close routine runs by itself only when you saved Close at Day start, and the Day's settings hold no such saving: the Day stays open until you close it.",
        ));
    }
    match runs_at {
        RunsAt::DayStart => {}
        RunsAt::Off => {
            return Err(denied(
                "Close is Off in your Day's settings: the Day stays open until you close it.",
            ))
        }
        RunsAt::Daily(time) => {
            return Err(denied(format!(
                "Close is saved to run daily at {time}, not at Day start: the Close routine runs by itself only at Day start, so the Day stays open until you close it."
            )))
        }
        RunsAt::Rollover => {
            return Err(denied(
                "Close is saved to run at rollover, not at Day start: the Close routine runs by itself only at Day start, so the Day stays open until you close it.",
            ))
        }
    }
    if today <= civil_date.as_str() {
        return Err(denied(format!(
            "The Day boundary that ends the Day of {civil_date} has not passed; an agent closes it then."
        )));
    }
    // Opened again by the person (central.day.lifecycle, theirs alone) at or after the instant the Day turned: their own act since
    // the consent was saved, so only they close it.
    let turned_at = day_spans(scope, &civil_date)?
        .last()
        .map(|span| span.1.div_euclid(1000));
    let opened_at = entry["temporal"]["lifecycle_at_unix_seconds"].as_i64();
    if entry["temporal"]["lifecycle"] == "open"
        && opened_at
            .zip(turned_at)
            .is_some_and(|(at, turned)| at >= turned)
    {
        return Err(denied(
            "You opened this Day again after its turn: it stays open until you close it.",
        ));
    }
    Ok(())
}

/// Why an open Day whose date is past `today` stays open, in words (the Day's reading carries it, temporal.rs): the person's consent
/// to a close they do not press, refused; or, where it holds, a carry Central's civil clock cannot file (`the_carry_cannot_wait`).
/// None where the clock closes it at its turn, or the Day is closed or not yet past.
pub(super) fn stays_open(scope: &Scope, entry: &Value, today: &str) -> Option<String> {
    if entry["temporal"]["lifecycle"] != "open"
        || entry["temporal"]["civil_date"]
            .as_str()
            .is_none_or(|date| date >= today)
    {
        return None;
    }
    let why = || -> io::Result<Option<String>> {
        let settings = load_settings(scope)?;
        if let Err(refused) = consent_to_close(scope, &settings, entry, today) {
            return Ok(Some(refused.to_string()));
        }
        if !settings.close_routine().0 {
            return Ok(None);
        }
        let next_date = next_civil_date(&civil_date_of(entry)?)?.max(today.to_owned());
        let plan = carry_plan(scope, entry, &day_ref_of(scope, &next_date))?;
        the_carry_cannot_wait(scope, &plan, &next_date)
    };
    why().unwrap_or_else(|error| Some(error.to_string()))
}

fn carry_state(state: &str, reason: &str) -> Value {
    json!({"state":state,"reason":reason,"from":{"pointer":FACE_FIVE_PROPOSAL},"to":{"pointer":FACE_ZERO_GROUND}})
}

/// Where a close's carry goes, read before anything is written: nowhere, as face 5 holds no proposal for tomorrow; face 0 of the
/// next Day's document; or nowhere yet, as that Day has no Day document or its document maps no face 0, face 5's proposal staying.
enum CarryPlan {
    Nothing(&'static str),
    NoPage(String),
    NoFaceZero {
        proposal: String,
        reason: String,
    },
    Into {
        proposal: String,
        target: Value,
        field_id: String,
    },
}

fn carry_plan(scope: &Scope, closing: &Value, tomorrow_ref: &str) -> io::Result<CarryPlan> {
    let Some(closing_document) = closing.get("document").filter(|meta| meta.is_object()) else {
        return Ok(CarryPlan::Nothing(
            "The Day has no Day document, so face 5 holds no proposal for tomorrow.",
        ));
    };
    let document = documents::read(
        scope,
        &json!({"source_ref":closing["ref"],"document_id":closing_document["document_id"]}),
    )?;
    let proposal = document["document"]["template_payload"]
        .pointer(FACE_FIVE_PROPOSAL)
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    if plain_text(&proposal).trim().is_empty() {
        return Ok(CarryPlan::Nothing("Face 5 holds no proposal for tomorrow."));
    }
    let (relations, _) = scope.relations()?;
    let Some(tomorrow) = temporal::day_relation(&relations, tomorrow_ref)
        .ok()
        .filter(|tomorrow| tomorrow["document"].is_object())
    else {
        return Ok(CarryPlan::NoPage(proposal));
    };
    let target = documents::read(
        scope,
        &json!({"source_ref":tomorrow["ref"],"document_id":tomorrow["document"]["document_id"]}),
    )?;
    let mapped: Vec<&str> = target["document"]["fields"]
        .as_array()
        .map(|fields| {
            fields
                .iter()
                .filter_map(|field| field["template_pointer"].as_str())
                .collect()
        })
        .unwrap_or_default();
    let Some(field) = target["document"]["fields"].as_array().and_then(|fields| {
        fields
            .iter()
            .find(|field| field["template_pointer"] == FACE_ZERO_GROUND)
    }) else {
        let reason = format!(
            "Tomorrow's Day document maps no field at {FACE_ZERO_GROUND} (it maps {}), so face 0 has nowhere to take the proposal.",
            if mapped.is_empty() { "none".to_owned() } else { mapped.join(", ") }
        );
        return Ok(CarryPlan::NoFaceZero { proposal, reason });
    };
    let field_id = text(field, "id")?.to_owned();
    Ok(CarryPlan::Into {
        proposal,
        target,
        field_id,
    })
}

/// Why Central's civil clock may not close a Day whose carry it cannot file (`plan`, to the Day of `next_date`): face 5's proposal
/// has no page of that Day's to wait on in Approvals, or that page maps no face 0. The clock closes a Day only when it can file its
/// carry, so the proposal is never closed away behind the person; their own close still closes it. None where it can file it, or
/// there is none to file.
fn the_carry_cannot_wait(
    scope: &Scope,
    plan: &CarryPlan,
    next_date: &str,
) -> io::Result<Option<String>> {
    const STAYS_OPEN: &str = "Central's civil clock closes a Day only when it can file its carry in Approvals, so the Day stays open until you close it";
    Ok(match plan {
        CarryPlan::Nothing(_) | CarryPlan::Into { .. } => None,
        CarryPlan::NoFaceZero { reason, .. } => Some(format!("{reason} {STAYS_OPEN}.")),
        CarryPlan::NoPage(_) => Some(match saved_day_form(scope)? {
            None => format!(
                "No Day form is saved in your Day's settings (day_form), so face 5's proposal for tomorrow has no page of the Day of {next_date} to wait on. {STAYS_OPEN}, or save a Day form."
            ),
            Some(_) => format!(
                "The Day of {next_date} has no Day document yet, so face 5's proposal for tomorrow has no page to wait on. {STAYS_OPEN}, or it has one."
            ),
        }),
    })
}

/// Copy face 5's proposal for tomorrow into tomorrow's face 0 (the Day of `next_date`), as Central's document model allows it and as
/// `plan` found it. Whatever cannot be carried is said in words and the proposal stays on the Day being closed; a write that fails is
/// an error, so nothing is closed behind it.
fn carry(
    scope: &Scope,
    plan: CarryPlan,
    closing: &Value,
    closing_ref: &str,
    next_date: &str,
    principal: &Principal,
    now: u64,
) -> io::Result<Value> {
    let not_carried = |proposal: &str, reason: &str| {
        let mut state = carry_state("not_carried", reason);
        state["proposal_text"] = json!(proposal);
        state
    };
    let (proposal, target, field_id) = match plan {
        CarryPlan::Nothing(reason) => return Ok(carry_state("nothing_to_carry", reason)),
        CarryPlan::NoPage(proposal) => return Ok(not_carried(&proposal, "Tomorrow's Day has no Day document yet, so face 0 has nowhere to take the proposal; it stays on this Day's face 5.")),
        CarryPlan::NoFaceZero { proposal, reason } => return Ok(not_carried(&proposal, &reason)),
        CarryPlan::Into {
            proposal,
            target,
            field_id,
        } => (proposal, target, field_id),
    };
    let contribution_id = format!("close-carry:{closing_ref}");
    let mut state = carry_state("carried", "");
    state["to"] = json!({"pointer":FACE_ZERO_GROUND,"source_ref":target["source"]["ref"],
        "document_id":target["document_id"],"field_id":field_id});
    state["contribution_id"] = json!(contribution_id);
    state["reason"] = Value::Null;
    if target["document"]["contributions"]
        .as_array()
        .is_some_and(|rows| rows.iter().any(|row| row["id"] == contribution_id.as_str()))
    {
        state["state"] = json!("already_carried");
        return Ok(state);
    }
    let operation = json!({"operation":"field.append","field_id":field_id,
        "contribution_id":contribution_id,"html":proposal});
    if !principal.is_human() {
        state["state"] = json!("waits");
        let submit = json!({"source_ref":target["source"]["ref"],"document_id":target["document_id"],
            "expected_source_revision":target["revision"]["revision"],"proposal":operation});
        if principal.is_civil_clock() {
            // Central's civil clock has no one to hand the carry to: it files it in receiving itself, where the person's Approvals
            // keep it on tomorrow's face 0 or drop it, as any entry that is not theirs waits (the one submit, receiving.rs).
            let mut filing = submit.clone();
            filing["producer_key"] = json!(contribution_id);
            filing["day_ref"] = json!(day_ref_of(scope, next_date));
            filing["summary"] = json!(format!(
                "Face 5's proposal for tomorrow, from the Day of {}, for face 0 of the Day of {next_date}: Central's civil clock closed the Day at its turn and carries nothing into your Day itself. Keep it or drop it.",
                closing["temporal"]["civil_date"].as_str().unwrap_or(closing_ref),
            ));
            let filed = super::receiving::file_held(scope, &filing, principal, now)?;
            state["return_ref"] = filed["return_ref"].clone();
            state["reason"] = json!("Central's civil clock writes nothing into your Day: face 5's proposal waits in Approvals, for you to keep on tomorrow's face 0 or drop.");
        } else {
            state["reason"] = json!("An agent's entry into a Day waits for the person: submit it through central.receiving.submit.");
        }
        state["proposal"] = submit;
        return Ok(state);
    }
    let mut mutation = operation;
    mutation["source_ref"] = target["source"]["ref"].clone();
    mutation["document_id"] = target["document_id"].clone();
    mutation["expected_revision"] = target["revision"]["revision"].clone();
    mutation["request_id"] = json!(contribution_id);
    let written = documents::mutate(scope, &mutation, principal, now)?;
    state["operation_receipt"] = written["operation_receipt"].clone();
    Ok(state)
}

pub(crate) fn close_carry(
    scope: &Scope,
    input: &Value,
    principal: &Principal,
    now: u64,
) -> io::Result<Value> {
    let day_ref = text(input, "day_ref")?;
    let expected_revision = text(input, "expected_revision")?;
    let expected_relations = text(input, "expected_relations_revision")?;
    let time = temporal::time_policy(scope, now)?;
    let (relations, basis) = scope.relations()?;
    if basis != expected_relations {
        return Err(conflict("Day metadata changed"));
    }
    let entry = temporal::day_relation(&relations, day_ref)?.clone();
    if scope.read(text(&entry, "ref")?)?.revision.revision != expected_revision {
        return Err(conflict("Day source changed"));
    }
    if entry["temporal"]["lifecycle"] == "closed" {
        return Err(conflict("The Day is closed already"));
    }
    let civil_date = civil_date_of(&entry)?;
    let settings = load_settings(scope)?;
    if !principal.is_human() {
        consent_to_close(scope, &settings, &entry, &time.civil_date)?;
    }
    // Tomorrow's Day: the Day after the one closing, or today's where that date has passed already (a Day closed after a gap
    // carries to the Day that follows it now, and no past Day is opened for it). The one that stands, or opened now through the
    // same path rollover and ensure use.
    let next_date = next_civil_date(&civil_date)?.max(time.civil_date.clone());
    let tomorrow_ref = day_ref_of(scope, &next_date);
    // The carry, read before anything is written: Central's civil clock closes a Day only when it can file it.
    let plan = if settings.close_routine().0 {
        let plan = carry_plan(scope, &entry, &tomorrow_ref)?;
        if principal.is_civil_clock() {
            if let Some(why) = the_carry_cannot_wait(scope, &plan, &next_date)? {
                return Err(denied(why));
            }
        }
        Some(plan)
    } else {
        None
    };
    if temporal::day_relation(&relations, &tomorrow_ref).is_err() {
        let (mut opened, opened_basis, opened_ref, _) =
            temporal::open_day(scope, &time, &next_date, now)?;
        if opened["temporal"]["today"]["civil_date"]
            .as_str()
            .is_none_or(|today| today < next_date.as_str())
        {
            set_tomorrow(&mut opened, &opened_ref, &next_date, now);
        }
        let opened_entry = temporal::day_relation(&opened, &opened_ref)?.clone();
        temporal::save_relations(scope, &opened, &opened_basis)?;
        scope.reconcile(
            Some((&principal.principal_ref, &principal.actor_kind, None)),
            &[text(&opened_entry, "ref")?.into()],
        )?;
    }
    let carried = match plan {
        Some(plan) => carry(scope, plan, &entry, day_ref, &next_date, principal, now)?,
        None => carry_state(
            "off",
            "The Close routine is Off, so the Day closes and carries nothing.",
        ),
    };
    // The carry may have bound tomorrow's document in the relations: close on
    // the basis as it stands now, the caller's having been checked above.
    let (_, fresh) = scope.relations()?;
    temporal::change_lifecycle(
        scope,
        day_ref,
        expected_revision,
        &fresh,
        "closed",
        &principal.principal_ref,
        now,
    )?;
    let closed = temporal::day_read(scope, &json!({"day_ref":day_ref}))?;
    let tomorrow = temporal::day_read(scope, &json!({"day_ref":tomorrow_ref}))?;
    Ok(json!({
        "schema":"central.day-close-carry/v1",
        "carry":carried,
        "revisions":{"closed_day":closed["revision"]["revision"],"tomorrow_day":tomorrow["revision"]["revision"],
            "relations":closed["relations_revision"]},
        "closed":closed,
        "tomorrow":tomorrow,
        "automatic_agent_or_model_invocation":false,
    }))
}

// ---- the field source -------------------------------------------------------
//
// What reached you from the shared field is recorded into the Day under its
// field source (Central #324, O-I #858 SF08): beside the Day as its moves are,
// on face 4 where the day sat, never into the person's own writing. Each record
// names who sent it and how it came, and carries nothing that would rank it.

const FIELD_ITEM_SCHEMA: &str = "central.day-field-item/v1";
/// How a thing reaches you from the field (manual 2): addressed to you, from a
/// star you are gathered at, from someone you watch, or from a search you made.
const FIELD_HOWS: [(&str, &str); 4] = [
    ("to-you", "addressed to you"),
    ("star", "from a star you are gathered at"),
    ("watch", "from someone you watch"),
    ("search", "from a search you made"),
];
/// The Day face a field record stands on: face 4, where the day sat.
const FIELD_FACE: u64 = 4;

fn field_record_path(scope: &Scope, civil_date: &str) -> String {
    format!("{}/{civil_date}/field.jsonl", scope.day_dir())
}

fn field_hows() -> String {
    let named: Vec<String> = FIELD_HOWS
        .iter()
        .map(|(how, words)| format!("{words} ({how})"))
        .collect();
    let (last, rest) = named.split_last().expect("four ways to reach you");
    format!("how is {}, or {last}", rest.join(", "))
}

/// A bounded ref: one token, no whitespace or control characters.
/// A ref: not empty, at most 512 bytes, with no whitespace or control character.
fn is_ref(r: &str) -> bool {
    !r.is_empty() && r.len() <= 512 && !r.chars().any(|c| c.is_whitespace() || c.is_control())
}

fn bounded_ref<'a>(value: &'a Value, what: &str) -> io::Result<&'a str> {
    value
        .as_str()
        .filter(|r| is_ref(r))
        .ok_or_else(|| invalid(format!("{what} is a ref of at most 512 bytes")))
}

/// Bounded words: not blank and at most `max` characters. What characters they
/// hold is checked once the Day's kept records are looked up (`plain_record`),
/// so a record the Day keeps already answers kept whatever is refused now.
fn bounded_words<'a>(value: &'a Value, what: &str, max: usize) -> io::Result<&'a str> {
    value
        .as_str()
        .filter(|w| !w.trim().is_empty() && w.chars().count() <= max)
        .ok_or_else(|| {
            invalid(format!(
                "{what} is text of at most {max} characters, not blank"
            ))
        })
}

/// The characters that make text read other than it is, the one set every
/// check in ctrl reads (`crate::names::invisible`; DLW build ruling
/// 2026-11-09, be2i). The joiners U+200C and U+200D are in it: a name never
/// holds one, and a record's words keep one only in its place, inside an emoji
/// or a joined word (`crate::names::joiner_kept`; be2m, DLW build ruling
/// 2026-11-10). The label of an entry read as a part keeps one in its place
/// too (`plain_text_words`, by `crate::names::shown_prose`): O-I shows a part
/// label that holds one (DLW build ruling 2026-11-12).
pub(super) use crate::names::invisible;

/// Text that reads as it is: no control or invisible character, save, where it
/// may run to `lines` (a run's reason and summary; a name stands on one line),
/// a line break or a tab, and a `\r` only as the start of `\r\n`. The whole
/// set, joiners too: a record's words are read by `crate::names::prose_fault`,
/// which keeps a joiner in its place.
fn plain_characters(text: &str, lines: bool) -> bool {
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        let kept = match c {
            '\n' | '\t' => lines,
            '\r' => lines && chars.peek() == Some(&'\n'),
            c => !c.is_control() && !invisible(c),
        };
        if !kept {
            return false;
        }
    }
    true
}

/// A record's names, as `plain_characters` reads them, and its words, as prose
/// is read (`crate::names::prose_fault`; be2m): a joiner inside an emoji or a
/// joined word is part of the words, and what stands in the way is named by
/// its kind, so the words can be given again without it.
fn plain_record(item: &Value) -> io::Result<()> {
    for (what, text) in [
        ("from.name", &item["from"]["name"]),
        ("star.name", &item["star"]["name"]),
    ] {
        if text
            .as_str()
            .is_some_and(|name| !plain_characters(name, false))
        {
            return Err(invalid(format!(
                "{what} stands on one line, with no control character and no invisible one (a line or paragraph separator, a format character such as a bidi control or mark, a zero-width character, a soft hyphen or a tag character, or a blank filler)"
            )));
        }
    }
    if let Some(kind) = item["words"].as_str().and_then(crate::names::prose_fault) {
        return Err(invalid(format!(
            "words hold {kind}: they hold no control character but line breaks and tabs (a \\r only before a \\n), and no invisible one (a line or paragraph separator, a format character such as a bidi control or mark, a zero-width character, a soft hyphen or a tag character, or a blank filler) but a joiner inside an emoji or a joined word"
        )));
    }
    Ok(())
}

fn exact_keys(object: &Map<String, Value>, allowed: &[&str], what: &str) -> io::Result<()> {
    match object.keys().find(|key| !allowed.contains(&key.as_str())) {
        Some(key) => Err(invalid(format!(
            "{what} has no `{key}`: it names who sent it, how it came, what it is, its words and when, and nothing else"
        ))),
        None => Ok(()),
    }
}

/// The keys one record names: who sent it, how it came, what it is, its words
/// and when it reached you.
const FIELD_ITEM_KEYS: [&str; 7] = [
    "item_ref",
    "from",
    "how",
    "star",
    "what",
    "words",
    "at_unix_ms",
];
/// One call records at most this many things (Central #324, DLW build ruling
/// 2026-11-08: what waits is sent as one batch).
const FIELD_BATCH_MAX: usize = 64;
/// What every field record says of where it came from: received through the
/// shared field and recorded by the Day under the person's Field setting. The
/// person wrote none of it; the words are the sender's.
const FIELD_RECEIVED: (&str, &str) = ("shared-field", "day:field-source");

/// The record of one thing that reached you, as the input names it. `call_keys`
/// are the keys the call itself may carry beside it (the single form's).
fn field_item(input: &Value, call_keys: &[&str]) -> io::Result<Value> {
    let object = input
        .as_object()
        .ok_or_else(|| invalid("a field record is an object"))?;
    let allowed: Vec<&str> = FIELD_ITEM_KEYS.iter().chain(call_keys).copied().collect();
    exact_keys(object, &allowed, "A field record")?;
    let item_ref = bounded_ref(&input["item_ref"], "item_ref")?;
    let from = input["from"]
        .as_object()
        .ok_or_else(|| invalid("from is {world_ref, name}: who sent it"))?;
    exact_keys(from, &["world_ref", "name"], "from")?;
    let from = json!({"world_ref":bounded_ref(&input["from"]["world_ref"], "from.world_ref")?,
        "name":bounded_words(&input["from"]["name"], "from.name", 120)?.trim()});
    let how = input["how"]
        .as_str()
        .filter(|how| FIELD_HOWS.iter().any(|(known, _)| known == how))
        .ok_or_else(|| invalid(field_hows()))?;
    let star = match (how, input.get("star")) {
        ("star", None | Some(Value::Null)) => return Err(invalid(
            "what came from a star you are gathered at names the star: star is {star_ref, name}",
        )),
        (_, None | Some(Value::Null)) => Value::Null,
        ("star", Some(Value::Object(star))) => {
            exact_keys(star, &["star_ref", "name"], "star")?;
            json!({"star_ref":bounded_ref(&input["star"]["star_ref"], "star.star_ref")?,
                "name":bounded_words(&input["star"]["name"], "star.name", 160)?.trim()})
        }
        ("star", Some(_)) => return Err(invalid("star is {star_ref, name}")),
        (_, Some(_)) => {
            return Err(invalid(
                "a star is named only for what came from a star you are gathered at (how: star)",
            ))
        }
    };
    let what = input["what"]
        .as_str()
        .filter(|what| {
            !what.is_empty()
                && what.len() <= 64
                && what
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        })
        .ok_or_else(|| {
            invalid("what is the kind of thing it is, as the field names it (question, finding, showed, …)")
        })?;
    let words = bounded_words(&input["words"], "words", 4096)?;
    Ok(
        json!({"item_ref":item_ref,"from":from,"how":how,"star":star,"what":what,"words":words,
        "at_unix_ms":instant_ms(input)?}),
    )
}

/// The Day's field source as the person's settings hold it: on or off, its
/// standing, and when it last turned On (absent while it never turned On from
/// Off).
fn field_source(settings: &Loaded) -> Value {
    let (on, saved) = settings.source("field");
    json!({"name":"field","on":on,"standing":if saved {"saved"} else {"default"},
        "on_since_unix_ms":settings.on_since("field")})
}

/// The instants on which the Day of `civil_date` stands (DLW build ruling
/// 2026-11-08): every span whose civil date, by the very rule
/// `central.time.policy` names today's date with (temporal::civil_date_in: the
/// wall in the policy's zone, less the day boundary), is that date. The date
/// can flip only where the wall reads the boundary (once, or twice in an autumn
/// fold) or where the zone's offset changes (a spring-forward gap, the fold
/// itself), so the spans are read between those instants, and they agree with
/// the civil date on every instant: one span on most Days, two where a fold
/// carries the wall back across the boundary.
pub(super) fn day_spans(scope: &Scope, civil_date: &str) -> io::Result<Vec<(i64, i64)>> {
    const DAY: i64 = 24 * 3600 * 1000;
    let date = NaiveDate::parse_from_str(civil_date, "%Y-%m-%d")
        .map_err(|_| invalid("a Day's civil date is YYYY-MM-DD"))?;
    let noon = date
        .and_hms_opt(12, 0, 0)
        .ok_or_else(|| invalid("civil date out of range"))?
        .and_utc()
        .timestamp_millis();
    let time = temporal::time_policy(scope, (noon / 1000).max(0) as u64)?;
    let zone = timezone_of(&time)?;
    let boundary = time.policy.day_boundary_minutes;
    let (low, high) = (noon - 2 * DAY, noon + 3 * DAY);
    let mut cuts = vec![low, high];
    for offset in -3..=3 {
        let on = date + Duration::days(offset);
        let wall = on
            .and_hms_opt(boundary / 60, boundary % 60, 0)
            .ok_or_else(|| invalid("the day boundary is no time of day"))?;
        match zone.from_local_datetime(&wall) {
            LocalResult::Single(at) => cuts.push(at.timestamp_millis()),
            LocalResult::Ambiguous(first, second) => {
                cuts.push(first.timestamp_millis());
                cuts.push(second.timestamp_millis());
            }
            LocalResult::None => {}
        }
    }
    cuts.extend(offset_changes(&zone, low, high)?);
    cuts.retain(|cut| (low..=high).contains(cut));
    cuts.sort_unstable();
    cuts.dedup();
    let mut spans: Vec<(i64, i64)> = Vec::new();
    for pair in cuts.windows(2) {
        let (from, to) = (pair[0], pair[1]);
        if temporal::civil_date_in(&zone, boundary, from.div_euclid(1000))? != date {
            continue;
        }
        match spans.last_mut() {
            Some(last) if last.1 == from => last.1 = to,
            _ => spans.push((from, to)),
        }
    }
    match (spans.first(), spans.last()) {
        (Some(first), Some(last)) if first.0 > low && last.1 < high => Ok(spans),
        _ => Err(invalid(format!(
            "the civil-time policy names no instant that stands on the Day of {civil_date}"
        ))),
    }
}

/// Every instant in [low, high] at which the zone's offset from UTC changes, to
/// the second (offsets are sampled hourly; a zone changes at most once an hour).
fn offset_changes(zone: &Tz, low: i64, high: i64) -> io::Result<Vec<i64>> {
    const STEP: i64 = 3600;
    let offset = |second: i64| -> io::Result<i32> {
        Ok(DateTime::<Utc>::from_timestamp(second, 0)
            .ok_or_else(|| invalid("instant outside the supported range"))?
            .with_timezone(zone)
            .offset()
            .fix()
            .local_minus_utc())
    };
    let (low, high) = (low.div_euclid(1000), high.div_euclid(1000));
    let mut changes = Vec::new();
    let (mut at, mut before) = (low, offset(low)?);
    while at < high {
        let next = (at + STEP).min(high);
        let after = offset(next)?;
        if after != before {
            let (mut same, mut changed) = (at, next);
            while changed - same > 1 {
                let middle = same + (changed - same) / 2;
                if offset(middle)? == before {
                    same = middle;
                } else {
                    changed = middle;
                }
            }
            changes.push(changed * 1000);
        }
        (at, before) = (next, after);
    }
    Ok(changes)
}

/// What the Day takes from the field: what reached you on its own date, and
/// only since the field last turned On — each span of the Day from the later
/// of its start and `on_since`.
fn field_takes(day: &[(i64, i64)], on_since: Option<i64>) -> Vec<(i64, i64)> {
    day.iter()
        .filter_map(|&(from, until)| {
            let from = on_since.map_or(from, |since| since.max(from));
            (from < until).then_some((from, until))
        })
        .collect()
}

/// Every field record of a Day, in the order each reached you (and, for two at
/// the same moment, the order they were recorded): by time, and by nothing else.
fn field_items(log: &Log) -> Vec<Value> {
    let mut items = log.events.clone();
    items.sort_by_key(|item| item["at_unix_ms"].as_i64().unwrap_or(0));
    items
}

/// One record's answer: made, kept already, or refused in words.
enum FieldOutcome {
    Created(Value),
    Kept(Value),
    Refused(Option<String>, io::Error),
}

/// Record what reached you into one Day in one rewrite. Each record is
/// answered on its own, so one that is refused refuses none of the others; a
/// record the Day keeps already answers kept before anything else is asked.
fn record_field_items(
    scope: &Scope,
    input: &Value,
    items: Vec<(Option<String>, io::Result<Value>)>,
    principal: &Principal,
    now: u64,
) -> io::Result<(String, Vec<FieldOutcome>, String)> {
    let (relations, _) = scope.relations()?;
    let (day_ref, entry) = target_day(&relations, input)?;
    let civil_date = civil_date_of(&entry)?;
    let settings = load_settings(scope)?;
    let on = settings.source("field").0;
    let closed = entry["temporal"]["lifecycle"] == "closed";
    // The Day's own date, by the civil date's rule; where it cannot be named, a
    // record the Day keeps still answers kept, and each other is refused in words.
    let day = day_spans(scope, &civil_date).map_err(|error| (error.kind(), error.to_string()));
    let on_since = settings.on_since("field");
    let path = field_record_path(scope, &civil_date);
    let log = load_lines(scope, &path, FIELD_ITEM_SCHEMA, "field record")?;
    let mut made: Vec<Value> = Vec::new();
    let mut outcomes = Vec::new();
    for (named, item) in items {
        let item = match item {
            Ok(item) => item,
            Err(error) => {
                outcomes.push(FieldOutcome::Refused(named, error));
                continue;
            }
        };
        let item_ref = item["item_ref"].as_str().map(str::to_owned);
        if let Some(existing) = log
            .events
            .iter()
            .chain(made.iter())
            .find(|existing| existing["item_ref"] == item["item_ref"])
        {
            let same = FIELD_ITEM_KEYS
                .iter()
                .all(|key| existing[*key] == item[*key]);
            outcomes.push(if same {
                FieldOutcome::Kept(existing.clone())
            } else {
                FieldOutcome::Refused(
                    item_ref.clone(),
                    conflict(format!(
                        "{} is recorded in this Day already, with other content; a record is never rewritten",
                        item_ref.as_deref().unwrap_or_default()
                    )),
                )
            });
            continue;
        }
        let at = item["at_unix_ms"].as_i64().unwrap_or(0);
        let refusal = if let Err(error) = plain_record(&item) {
            Some(error)
        } else if !on {
            Some(denied(
                "The Day's Sources keep the field Off, so nothing that reached you from the field is recorded into the Day.",
            ))
        } else if closed {
            Some(conflict(format!(
                "The Day of {civil_date} is closed; a closed Day is kept and nothing more is recorded into it"
            )))
        } else if let Err((kind, words)) = &day {
            Some(io::Error::new(
                *kind,
                format!("The Day's own date cannot be named, so nothing new is recorded into it: {words}"),
            ))
        } else if !day
            .as_ref()
            .is_ok_and(|spans| spans.iter().any(|&(from, until)| at >= from && at < until))
        {
            let its_date = civil_date_at(scope, at).unwrap_or_else(|_| "another date".to_owned());
            Some(invalid(format!(
                "It reached you on {its_date}, and this is the Day of {civil_date}: a Day takes only what reached you on its own date"
            )))
        } else if on_since.is_some_and(|since| at < since) {
            Some(invalid(
                "It reached you before the field was last turned On, so it is not recorded: the Day takes what reaches you from when the field turns On, never what came while it was Off",
            ))
        } else {
            None
        };
        if let Some(refusal) = refusal {
            outcomes.push(FieldOutcome::Refused(item_ref, refusal));
            continue;
        }
        let mut record = item;
        record["schema"] = json!(FIELD_ITEM_SCHEMA);
        record["day_ref"] = json!(day_ref);
        record["source"] = json!("field");
        record["face"] = json!(FIELD_FACE);
        record["received"] = json!({"through":FIELD_RECEIVED.0,"recorded_by":FIELD_RECEIVED.1});
        // The person's standing Field setting authorises the record; the
        // credential that carried it is the one they granted.
        record["authorised_by"] = json!(principal.principal_ref);
        record["recorded_at_unix_ms"] = json!((now as i64) * 1000);
        made.push(record.clone());
        outcomes.push(FieldOutcome::Created(record));
    }
    let revision = if made.is_empty() {
        log.revision.clone()
    } else {
        // Source history names what recorded it: the Day's field source, under
        // the person's Field setting, as the record itself says (`received`).
        append_lines(
            scope,
            &path,
            &log,
            &made,
            (FIELD_RECEIVED.1, "native-service"),
        )?;
        load_lines(scope, &path, FIELD_ITEM_SCHEMA, "field record")?.revision
    };
    Ok((day_ref, outcomes, revision))
}

pub(crate) fn field_record(
    scope: &Scope,
    input: &Value,
    principal: &Principal,
    now: u64,
) -> io::Result<Value> {
    let Some(list) = input.get("records") else {
        // One record: the call is the record, and a refusal is the call's.
        let item = field_item(
            input,
            &["project", "day_ref", "expected_authority_revision"],
        )?;
        let (_, mut outcomes, revision) =
            record_field_items(scope, input, vec![(None, Ok(item))], principal, now)?;
        let (created, item) = match outcomes.pop() {
            Some(FieldOutcome::Created(item)) => (true, item),
            Some(FieldOutcome::Kept(item)) => (false, item),
            Some(FieldOutcome::Refused(_, error)) => return Err(error),
            None => return Err(invalid("a field record was not answered")),
        };
        return Ok(
            json!({"schema":"central.day-field-record/v1","created":created,"item":item,"revision":revision,
            "automatic_agent_or_model_invocation":false}),
        );
    };
    let object = input
        .as_object()
        .ok_or_else(|| invalid("native operation requires an object"))?;
    if let Some(key) = object.keys().find(|key| {
        !matches!(
            key.as_str(),
            "records" | "project" | "day_ref" | "expected_authority_revision"
        )
    }) {
        return Err(invalid(format!(
            "A call names either one record or records, the list of them; `{key}` stands beside records"
        )));
    }
    let list = list
        .as_array()
        .ok_or_else(|| invalid("records is a list of the things that reached you"))?;
    if list.is_empty() || list.len() > FIELD_BATCH_MAX {
        return Err(invalid(format!(
            "records names at least one thing and at most {FIELD_BATCH_MAX}"
        )));
    }
    let items = list
        .iter()
        .map(|record| {
            let named = record["item_ref"].as_str().map(str::to_owned);
            (named, field_item(record, &[]))
        })
        .collect();
    let (day_ref, outcomes, revision) = record_field_items(scope, input, items, principal, now)?;
    let (mut created, mut kept, mut refused) = (0, 0, 0);
    let results: Vec<Value> = outcomes
        .into_iter()
        .map(|outcome| match outcome {
            FieldOutcome::Created(item) => {
                created += 1;
                json!({"item_ref":item["item_ref"],"created":true,"item":item})
            }
            FieldOutcome::Kept(item) => {
                kept += 1;
                json!({"item_ref":item["item_ref"],"created":false,"item":item})
            }
            FieldOutcome::Refused(item_ref, error) => {
                refused += 1;
                json!({"item_ref":item_ref,"created":false,"refused":error.to_string()})
            }
        })
        .collect();
    Ok(json!({
        "schema":"central.day-field-records/v1",
        "day_ref":day_ref,
        "results":results,
        "created":created,
        "kept":kept,
        "refused":refused,
        "revision":revision,
        "automatic_agent_or_model_invocation":false,
    }))
}

pub(crate) fn field_read(scope: &Scope, input: &Value) -> io::Result<Value> {
    let (relations, _) = scope.relations()?;
    let (day_ref, entry) = target_day(&relations, input)?;
    let civil_date = civil_date_of(&entry)?;
    let settings = load_settings(scope)?;
    // Where the Day's own date cannot be named, it takes nothing, and says why
    // (DLW build ruling 2026-11-09): the reading still answers, as a record does.
    let (day, note) = match day_spans(scope, &civil_date) {
        Ok(day) => (day, None),
        Err(error) => (
            Vec::new(),
            Some(format!(
                "The Day's own date cannot be named now, so it takes nothing from the field: {error}"
            )),
        ),
    };
    let spans = field_takes(&day, settings.on_since("field"));
    // The first span's start and the last span's end: across a fold they hold
    // the repeated time, which is the earlier Day's; `spans` is the exact window.
    let (from, until) = match (spans.first(), spans.last(), day.last()) {
        (Some(first), Some(last), _) => (json!(first.0), json!(last.1)),
        (_, _, Some(end)) => (json!(end.1), json!(end.1)),
        _ => (Value::Null, Value::Null),
    };
    let mut takes = json!({"from_unix_ms":from,"until_unix_ms":until,
        "spans":spans.iter().map(|(from, until)| json!({"from_unix_ms":from,"until_unix_ms":until})).collect::<Vec<_>>(),
        "basis":"what reached you on the Day's own date, as its civil date names it, from when the field last turned On"});
    if let Some(note) = note {
        takes["note"] = json!(note);
    }
    let log = load_lines(
        scope,
        &field_record_path(scope, &civil_date),
        FIELD_ITEM_SCHEMA,
        "field record",
    )?;
    Ok(json!({
        "schema":"central.day-field-reading/v1",
        "day_ref":day_ref,
        "civil_date":civil_date,
        "source":field_source(&settings),
        "takes":takes,
        "face":FIELD_FACE,
        "items":field_items(&log),
        "revision":log.revision,
        "order":"by when each reached you, and by nothing else",
        "automatic_agent_or_model_invocation":false,
    }))
}

// ---- a Day entry shown as a part --------------------------------------------
//
// "Nothing from my own Day enters [a star's day] unless I bring it" (O-I #858
// SF06): one entry of the Day, read exactly as the field would receive it if
// the person showed it. Central answers the part; showing it is the person's
// own act in the field, and reading it is the person's press. Nothing is
// written here but the world's part salt, once, outside every Day.

/// The field names a part in at most this many characters.
const PART_LABEL_MAX: usize = 160;

/// Plain words from a contribution's sanitised markup: its tags gone (a block
/// ends a word), the five escapes undone, whitespace collapsed. A field's own
/// value is text and never passes through here (`plain_text_words`).
fn plain_words(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    for c in html.chars() {
        match (in_tag, c) {
            (false, '<') => {
                in_tag = true;
                out.push(' ');
            }
            (true, '>') => in_tag = false,
            (false, c) => out.push(c),
            (true, _) => {}
        }
    }
    let out = out
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
        .replace("&amp;", "&");
    plain_text_words(&out)
}

/// A field's own value as words: it is text, so nothing in it is markup and
/// nothing is unescaped ("5 < 6", "R&D"); only its whitespace is collapsed.
/// Words read as they are, in the field too (DLW build ruling 2026-11-09): a
/// control character, or a line or paragraph separator, stands as a space, and
/// every other invisible character (`invisible`: a bidi control or mark, a
/// zero-width character, a soft hyphen, a tag character, a blank filler) is
/// left out, save a joiner in its place: inside an emoji or a joined word it
/// is part of the words (`crate::names::shown_prose`, which shows a line
/// break or a tab as it is; the collapse makes each a space; be2m item 4).
fn plain_text_words(text: &str) -> String {
    crate::names::shown_prose(text)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Where Central keeps the world's part salt: one file in the world's own
/// state, outside every Day's folder, so reading a part writes nothing into any
/// Day, a closed one least of all (DLW build ruling 2026-11-08).
const PART_SALT_PATH: &str = ".central/day-part-salt.json";

/// The random value Central keeps for the world's part refs, written once at
/// first use under the scope lock: a part ref is then no digest anyone could
/// compute by guessing a Day's date and its fields (DLW build ruling 2026-11-08).
fn part_salt(scope: &Scope) -> io::Result<String> {
    const SALT_SCHEMA: &str = "central.day-part-salt/v1";
    let path = PART_SALT_PATH;
    // A salt that cannot be read names its file and the way on; nothing is
    // guessed from it (DLW build ruling 2026-11-09).
    let unreadable = |what: &str| {
        invalid(format!(
            "The world's part salt at {} is {what}, so no part is read from it. Move it aside and press again: Central makes a new salt, and a part shown before is shown again under a new ref.",
            scope.root.join(path).display()
        ))
    };
    let kept = |raw: &str| -> io::Result<String> {
        let value: Value = serde_json::from_str(raw).map_err(|_| unreadable("not JSON"))?;
        value["salt"]
            .as_str()
            .filter(|salt| value["schema"] == SALT_SCHEMA && salt.len() >= 32)
            .map(str::to_owned)
            .ok_or_else(|| unreadable("not a central.day-part-salt/v1"))
    };
    if let Some(raw) = read_optional(scope, path)? {
        return kept(&raw);
    }
    let salt = uuid::Uuid::new_v4().simple().to_string();
    // The world's state, not a source: nothing is reconciled into source history.
    source::put_new(
        &scope.root,
        path,
        &encoded(&json!({"schema":SALT_SCHEMA,"salt":salt}))?,
    )?;
    Ok(salt)
}

/// Each list item's identity (DLW build ruling 2026-11-08): its own id when it
/// has one, else a digest of its words with its occurrence among the id-less
/// items that hold the same words — never its place in the list, which moves.
/// Every identity is a ref `item_id` takes: an own id that is no such ref, or
/// that could pose as another form, is named by its digest (`id:`). An item
/// with its own id keeps its part however the list moves; an id-less one keeps
/// it only while the identical items before it are unchanged (DLW build ruling
/// 2026-11-09). An own id two items share names neither: it is answered `Err`.
fn list_identities(
    items: &[Value],
    words_of: impl Fn(&Value) -> String,
) -> Result<Vec<String>, String> {
    let mut seen: BTreeMap<String, usize> = BTreeMap::new();
    let mut own: BTreeSet<&str> = BTreeSet::new();
    items
        .iter()
        .map(
            |item| match item["id"].as_str().filter(|id| !id.trim().is_empty()) {
                Some(id) if !own.insert(id) => Err(id.to_owned()),
                Some(id) if is_ref(id) && !id.starts_with("words:") && !id.starts_with("id:") => {
                    Ok(id.to_owned())
                }
                Some(id) => Ok(format!("id:{}", &source::key(id)[..32])),
                None => {
                    let digest = source::key(&words_of(item))[..16].to_owned();
                    let occurrence = seen.entry(digest.clone()).or_insert(0);
                    let identity = format!("words:{digest}:{occurrence}");
                    *occurrence += 1;
                    Ok(identity)
                }
            },
        )
        .collect()
}

/// Why an `item_id` of an id-less item no longer names one (DLW build ruling
/// 2026-11-09): it was named by its place among the items holding its words,
/// and one before it changed or went. Plain words, never a bare "no item".
fn twins_moved(field_id: &str, item_id: &str, identities: &[String]) -> Option<String> {
    let rest = item_id.strip_prefix("words:")?;
    let (digest, occurrence) = rest.rsplit_once(':')?;
    if digest.len() != 16 || occurrence.parse::<usize>().is_err() {
        return None;
    }
    let holding = identities
        .iter()
        .filter(|identity| identity.starts_with(&format!("words:{digest}:")))
        .count();
    Some(format!(
        "{field_id} has no item {item_id} now. An item with no id is named by its place among the items that hold the same words, and an identical item before it changed or went ({holding} now hold those words), so it is named anew. The part it was shown as still holds its words; name the item by its index to read it as it stands."
    ))
}

/// The label the field receives: the words whole when they fit, else cut at a
/// word with an ellipsis. The cut never ends inside a joined sequence
/// (be2m.7, DLW build ruling 2026-11-12): what the cut leaves is read once
/// more (`crate::names::shown_prose`), so a joiner whose partner was cut away
/// goes with it and the label holds no joiner out of its place. Returns
/// (label, cut).
fn part_label(words: &str) -> (String, bool) {
    if words.chars().count() <= PART_LABEL_MAX {
        return (words.to_owned(), false);
    }
    let head: String = words.chars().take(PART_LABEL_MAX - 1).collect();
    let head = match head.rfind(' ') {
        Some(at) if at > 0 => head[..at].trim_end().to_owned(),
        _ => head,
    };
    let head = crate::names::shown_prose(&head);
    (format!("{head}…"), true)
}

/// What each refusal of the person's own act opens with, before its cause (DLW
/// build ruling 2026-11-09).
pub(crate) const ENTRY_PART_IS_THE_PERSONS: &str = "Showing an entry is the person's own press";
pub(crate) const FIELD_RECORD_IS_THE_PERSONS: &str =
    "Recording what reached you is the person's own Field setting at work";

/// A refusal of the credential that carried the person's own act, opening with
/// whose act it is, then the cause.
pub(crate) fn the_persons_own(operation: &str, error: io::Error) -> io::Error {
    let opening = match operation {
        "day_entry_part" => ENTRY_PART_IS_THE_PERSONS,
        "day_field_record" => FIELD_RECORD_IS_THE_PERSONS,
        _ => return error,
    };
    io::Error::new(error.kind(), format!("{opening}: {error}"))
}

#[cfg(test)]
type PartReadCheckpoint = Box<dyn FnOnce()>;
#[cfg(test)]
thread_local! {
    /// Runs once between the press's lookup of its Day and the Day's reading,
    /// so a test can press a close there.
    pub(super) static BETWEEN_PART_READS: std::cell::RefCell<Option<PartReadCheckpoint>> = const { std::cell::RefCell::new(None) };
}

/// Bringing a Day entry to a star is the person's press (DLW build ruling
/// 2026-11-08): only a human-scoped credential reads one as a part. The press
/// shows exactly what was offered (DLW build ruling 2026-11-09): only the open
/// Day Central names as today, and only on the revision the entry was offered
/// from.
pub(crate) fn entry_part(
    scope: &Scope,
    input: &Value,
    principal: &Principal,
    now: u64,
) -> io::Result<Value> {
    if !principal.is_human() {
        return Err(the_persons_own(
            "day_entry_part",
            denied(format!(
                "this credential is {}'s, a credential of kind {}, not the person's",
                principal.principal_ref, principal.actor_kind
            )),
        ));
    }
    let field_id = match input.get("field_id") {
        None | Some(Value::Null) => None,
        Some(value) => Some(bounded_ref(value, "field_id")?),
    };
    let contribution_id = match input.get("contribution_id") {
        None | Some(Value::Null) => None,
        Some(value) => Some(bounded_ref(value, "contribution_id")?),
    };
    let index =
        match input.get("index") {
            None | Some(Value::Null) => None,
            Some(value) => Some(value.as_u64().ok_or_else(|| {
                invalid("index is the place of one item in a list field, from 0")
            })?),
        };
    let item_id = match input.get("item_id") {
        None | Some(Value::Null) => None,
        Some(value) => Some(bounded_ref(value, "item_id")?),
    };
    if index.is_some() && item_id.is_some() {
        return Err(invalid(
            "Name one item of a list by its index or by its item_id, not both",
        ));
    }
    let expected_revision = text(input, "expected_revision")?;
    let (relations, _) = scope.relations()?;
    let (day_ref, _) = target_day(&relations, input)?;
    #[cfg(test)]
    if let Some(checkpoint) = BETWEEN_PART_READS.with(|slot| slot.borrow_mut().take()) {
        checkpoint();
    }
    // Lifecycle, date and revision are all read from the one reading the
    // words come from (DLW build ruling 2026-11-09). The press runs under the
    // source lock, as every lifecycle writer does, so no close lands between
    // the lookup and this reading; the one reading is defence in depth against
    // a writer outside the lock, and a close moves no revision, so the
    // revision alone would never see one.
    let day = temporal::day_read(scope, &json!({"day_ref":day_ref}))?;
    let civil_date = civil_date_of(&day)?;
    if day["temporal"]["lifecycle"] == "closed" {
        return Err(conflict(format!(
            "The Day of {civil_date} is closed; a closed Day is kept, and none of its entries is shown from it"
        )));
    }
    let today = temporal::time_policy(scope, now)?.civil_date;
    if civil_date != today {
        return Err(conflict(format!(
            "The Day of {civil_date} is not today: Central's today is {today}, and only today's open Day shows its entries"
        )));
    }
    if day["revision"]["revision"] != expected_revision {
        return Err(conflict(
            "The Day changed since this was offered; look again.",
        ));
    }
    if day["document"].is_null() {
        return Err(conflict(format!(
            "No Day document stands for the Day of {civil_date}, so it holds no entry to show"
        )));
    }
    let document = &day["document"]["document"];
    let (address, words, author) = match (field_id, contribution_id) {
        (Some(field_id), None) => {
            let field = document["fields"]
                .as_array()
                .and_then(|fields| fields.iter().find(|field| field["id"] == field_id))
                .ok_or_else(|| invalid(format!("{field_id} is not a field of this Day")))?;
            let value = field["template_pointer"]
                .as_str()
                .and_then(|pointer| document["template_payload"].pointer(pointer))
                .cloned()
                .unwrap_or(Value::Null);
            let item_words = |item: &Value| {
                plain_text_words(
                    item.as_str()
                        .or_else(|| item["text"].as_str())
                        .unwrap_or_default(),
                )
            };
            let (words, item) = match (&value, index, item_id) {
                (Value::String(words), None, None) => (plain_text_words(words), None),
                (Value::String(_), _, _) => {
                    return Err(invalid(format!(
                        "{field_id} holds one entry, not a list: name it without an index"
                    )))
                }
                (Value::Array(_), None, None) => {
                    return Err(invalid(format!(
                        "{field_id} is a list: name one item of it by its index or its item_id"
                    )))
                }
                (Value::Array(items), index, item_id) => {
                    let identities = list_identities(items, item_words).map_err(|id| {
                        invalid(format!(
                            "{field_id} holds more than one item with the id {id}: an id names one item, so none of them is read as a part until each has its own"
                        ))
                    })?;
                    let at = match (index, item_id) {
                        (Some(index), _) => (index as usize) < items.len(),
                        _ => true,
                    }
                    .then(|| match (index, item_id) {
                        (Some(index), _) => Some(index as usize),
                        (_, Some(item_id)) => identities.iter().position(|identity| identity == item_id),
                        _ => None,
                    })
                    .flatten()
                    .ok_or_else(|| {
                        invalid(match (index, item_id) {
                            (Some(index), _) => format!("{field_id} has no item {index}"),
                            (_, item_id) => {
                                let item_id = item_id.unwrap_or_default();
                                twins_moved(field_id, item_id, &identities)
                                    .unwrap_or_else(|| format!("{field_id} has no item {item_id}"))
                            }
                        })
                    })?;
                    (item_words(&items[at]), Some(identities[at].clone()))
                }
                _ => (String::new(), None),
            };
            let mut address = json!({"field_id":field_id});
            if let Some(item) = item {
                address["item"] = json!(item);
            }
            // A field's own value is written only by the person (field.set).
            (address, words, json!({"actor_kind":"human"}))
        }
        (None, Some(contribution_id)) => {
            if index.is_some() || item_id.is_some() {
                return Err(invalid("a contribution is one entry: name it without an index"));
            }
            let contribution = document["contributions"]
                .as_array()
                .and_then(|rows| rows.iter().find(|row| row["id"] == contribution_id))
                .ok_or_else(|| {
                    invalid(format!("{contribution_id} is not a contribution of this Day"))
                })?;
            if contribution["removed"] == true {
                return Err(conflict(format!(
                    "{contribution_id} was removed from the Day: it holds no words to show"
                )));
            }
            (
                json!({"contribution_id":contribution_id}),
                plain_words(contribution["html"].as_str().unwrap_or_default()),
                json!({"actor_kind":contribution["actor_kind"],"author_ref":contribution["author_ref"]}),
            )
        }
        _ => {
            return Err(invalid(
                "Name one entry of the Day: field_id (with index for one item of a list field), or contribution_id",
            ))
        }
    };
    if words.is_empty() {
        return Err(invalid("That entry holds no words to show"));
    }
    let (label, cut) = part_label(&words);
    // An opaque, stable ref: the same entry is the same part (so showing it
    // again changes nothing and withdrawing finds it), it names neither the
    // Day's date nor its field, and the Day's salt keeps it from being guessed.
    let salt = part_salt(scope)?;
    let digest = source::key(&serde_json::to_string(&json!([salt, day_ref, address]))?);
    Ok(json!({
        "schema":"central.day-entry-part/v1",
        "day_ref":day_ref,
        "entry":address,
        "revision":day["document"]["revision"]["revision"],
        "part":{"id":format!("day-entry:{}", &digest[..16]),"kind":"page","label":label},
        "label_cut":cut,
        "author":author,
        "receives":"the part's kind and label, as they stand here",
        "stays_out":"every other face, field, entry, contribution and note of the Day, its date, and who else wrote in it",
        "shown":false,
        "automatic_agent_or_model_invocation":false,
    }))
}

// ---- operations and actions -------------------------------------------------

/// The Action an authenticated operation is granted as.
pub(crate) fn authenticated_action(operation: &str) -> Option<&'static str> {
    Some(match operation {
        "day_settings_write" => "central.day.settings.write",
        "day_move_record" => "central.day.move.record",
        "day_rollover" => "central.day.rollover",
        "day_close_carry" => "central.day.close-carry",
        "day_field_record" => "central.day.field.record",
        "day_entry_part" => "central.day.entry.part",
        "day_receipt_record" => "central.day.receipt.record",
        _ => return None,
    })
}

pub(crate) fn authenticated_operation(
    scope: &Scope,
    operation: &str,
    input: &Value,
    principal: &Principal,
    now: u64,
) -> io::Result<Value> {
    match operation {
        "day_settings_write" => settings_write(scope, input, principal, now),
        "day_move_record" => move_record(scope, input, principal, now),
        "day_rollover" => rollover(scope, input, principal, now),
        "day_close_carry" => close_carry(scope, input, principal, now),
        "day_field_record" => field_record(scope, input, principal, now),
        "day_entry_part" => entry_part(scope, input, principal, now),
        "day_receipt_record" => super::day_turn::record_receipt(scope, input, principal, now),
        _ => Err(invalid("unknown continuous-work operation")),
    }
}

macro_rules! handler {
    ($name:ident, $action:literal, $operation:literal) => {
        fn $name(
            _: &ActionRegistry,
            input: &Value,
            context: &ActionExecutionContext<'_>,
        ) -> ActionResult {
            super::execute($action, $operation, input, context)
        }
    };
}
handler!(
    settings_read_action,
    "central.day.settings.read",
    "day_settings_read"
);
handler!(
    settings_write_action,
    "central.day.settings.write",
    "day_settings_write"
);
handler!(
    move_record_action,
    "central.day.move.record",
    "day_move_record"
);
handler!(
    moves_read_action,
    "central.day.moves.read",
    "day_moves_read"
);
handler!(moves_due_action, "central.day.moves.due", "day_moves_due");
handler!(rollover_action, "central.day.rollover", "day_rollover");
handler!(
    close_carry_action,
    "central.day.close-carry",
    "day_close_carry"
);
handler!(
    field_record_action,
    "central.day.field.record",
    "day_field_record"
);
handler!(
    field_read_action,
    "central.day.field.read",
    "day_field_read"
);
handler!(
    entry_part_action,
    "central.day.entry.part",
    "day_entry_part"
);

pub(crate) fn register_actions(registry: &mut ActionRegistry) {
    super::register_definitions(registry, &[
        ("central.day.settings.read", "Read the Day's settings", "Read the Day's own settings (Rollover, Agent writes, Sources, Close routine, each move's Prompt and Runs at, and the Day form the turn makes the page from) as they stand: the defaults where the person has saved nothing, each with its standing (default or saved) and its revision, and each move's own prompt. Read-only; nothing runs from a reading.", false, settings_read_action, &[]),
        ("central.day.settings.write", "Save the Day's settings", "Human-authenticated, exact-revision save of the Day's settings (expected_revision, or \"absent\" for the first save). Only the settings given are saved; every one saved here is the person's own and arms what it names. A move's Runs at at rollover is refused while Rollover is Off. day_form is the Day form the Day's turn makes today's page from (its path under the Central root, read there as a Day form before it is saved; null for none).", true, settings_write_action, &[("expected_revision","string",true),("rollover","string",false),("agent_writes","string",false),("sources","object",false),("close_routine","boolean",false),("moves","object",false),("day_form","string",false),("expected_authority_revision","string",false)]),
        ("central.day.move.record", "Record a Day move's run", "Authenticated, append-only record of a move's run: a begin (press, or a saved schedule's occurrence, idempotent per occurrence) and an end (done, refused, stalled or stopped, with what it gave). Central assigns the run_ref and the actor from the credential; a run keeps the prompt it began with and a late end keeps the time it happened.", true, move_record_action, &[("event","string",true),("move","string",false),("at_unix_ms","integer",true),("day_ref","string",false),("trigger","string",false),("occurrence_ref","string",false),("run_ref","string",false),("state","string",false),("gave","object",false),("reason","string",false),("expected_authority_revision","string",false)]),
        ("central.day.moves.read", "Read the Day's moves", "Read the Day's settings and, for each move, its effective prompt, Runs at with its standing, the next time it runs by itself (only for a schedule the person saved), the open run, the last ended run, and every run of the Day oldest first. Read-only.", false, moves_read_action, &[("day_ref","string",false)]),
        ("central.day.moves.due", "List the Day's moves that are due", "List the scheduled occurrences in (since, now] for every move whose Runs at the person saved, each with a stable occurrence_ref and whether it has begun. With only the defaults it lists nothing and says why. The native host polls this; Central runs nothing.", false, moves_due_action, &[("now_unix_ms","integer",true),("since_unix_ms","integer",true)]),
        ("central.day.rollover", "Open tomorrow's Day beside today's", "Open the Day of the civil date after today without moving today, through the same Day creation as central.day.ensure, and record it as the Day's tomorrow. Idempotent. A human may open it at any time; another authenticated principal only once the Rollover time the person saved has been reached.", true, rollover_action, &[("expected_time_policy_revision","string",true),("expected_authority_revision","string",false)]),
        ("central.day.field.record", "Record what reached you from the field", "Authenticated, append-only record of what reached you from the shared field, into the Day under its field source (face 4): who sent it (from: world_ref, name), how it came (how: to-you, star, watch or search; only a star names star_ref and name), what it is, its words and when it reached you. Each record says it was received through the shared field and recorded by the Day under your Field setting (received), with the credential that setting authorises (authorised_by); the words are the sender's. It carries nothing else: no count of looks and no ranking. Name one record, or records: a list of at most 64, recorded in one rewrite and each answered on its own (created, kept already, or refused in words), so one refused refuses none of the others. A record the Day keeps already answers kept first. Otherwise each is refused while the Day's Sources keep the field Off, on a closed Day, when it did not reach you on the Day's own date (as its civil date names it, through a spring-forward gap or an autumn fold alike), and when it reached you before the field last turned On; the same ref with other content is refused. A name holds no control character and no invisible one (a line or paragraph separator, a format character such as a bidi control or mark, a zero-width character, a soft hyphen or a tag character, or a blank filler); the words hold none but line breaks and tabs (a \\r only before a \\n), and no invisible one but a joiner in its place: inside an emoji sequence Unicode lists (a family, a technologist) or inside a word of a script that joins or conjoins (a Persian word, a Devanagari conjunct), where it changes how the characters beside it are drawn. Words refused for one say which kind stood in the way (a soft hyphen, a joiner outside an emoji or a joined word, a joiner between emoji that Unicode 17.0 does not list as one emoji, a bidi control or mark). A refusal of the credential opens with \"Recording what reached you is the person's own Field setting at work\", then its cause.", true, field_record_action, &[("day_ref","string",false),("records","array",false),("item_ref","string",false),("from","object",false),("how","string",false),("star","object",false),("what","string",false),("words","string",false),("at_unix_ms","integer",false),("expected_authority_revision","string",false)]),
        ("central.day.field.read", "Read what reached the Day from the field", "Read the Day's field source (on or off, default or saved, and when it last turned On), what the Day takes (takes: spans, the exact window: the spans of instants on which the Day's own civil date stands, one on most Days and two where an autumn fold carries the wall back across the Day's boundary, each from the later of its start and when the field last turned On; from and until are the first span's start and the last span's end, so across a fold they also hold the repeated time, which is the earlier Day's; where the Day's own date cannot be named, spans is empty and a note says why), and every thing recorded into the Day from the shared field, ordered by when it reached you and by nothing else, each naming who sent it and how it came. Read-only.", false, field_read_action, &[("day_ref","string",false)]),
        ("central.day.entry.part", "Read a Day entry as a part to show", "Human-authenticated: bringing an entry to a star is the person's press, and a refusal of the credential opens with \"Showing an entry is the person's own press\", then its cause. Read one entry of the Day exactly as the shared field would receive it if you showed it: an opaque part ref, its kind (page) and its label (the entry's words, at most 160 characters, cut at a word: a field's value as its text, a list item's text, a contribution's markup as plain words; a control character or a line or paragraph separator stands as a space, and a format character, a bidi control or mark, a zero-width one, a soft hyphen or a tag character, is left out, save a joiner in its place: inside an emoji sequence Unicode lists or inside a word of a script that joins or conjoins), with who wrote it. A field that holds neither text nor a list holds no words to show. Only the open Day Central names as today (its civil-time policy's date) is read, and only on the revision the entry was offered from (expected_revision, the Day's revision as its reading gives it): a closed Day, a Day that is not today, and a Day that changed since are each refused in words: the Day is read, and nothing of it is shown or returned. Name a field (field_id, and for one item of a list field its index or its item_id) or one contribution (contribution_id). A list item with its own id keeps its part however the list moves. An item with no id is named by a digest of its words and its occurrence among the id-less items holding the same words, so it keeps its part only while the identical items before it are unchanged; when they change, its item_id says so in words. An id two items share is refused. item_id takes every identity a part names. Nothing else of the Day goes with it, and the reading shows nothing: showing it is your own act in the field. The first part read in a world writes the random salt Central keeps for it, once, in the world's own state outside every Day, so no part ref can be guessed from a Day's date and fields; nothing is written into any Day.", true, entry_part_action, &[("day_ref","string",false),("expected_revision","string",true),("field_id","string",false),("index","integer",false),("item_id","string",false),("contribution_id","string",false),("expected_authority_revision","string",false)]),
        ("central.day.close-carry", "Close a Day and carry face 5 to tomorrow", "Run the close routine on exact revisions: carry face 5's proposal for tomorrow into tomorrow's face 0 (the Day after it, or today's where that date has passed; a human writes it; an agent's entry is returned as a proposal for receiving), then close the Day as central.day.lifecycle does, and return both revisions. A human may call it at any time; another authenticated principal only when the person saved Close at Day start, the Day boundary has passed and the person has not opened the Day again after its turn. Central's civil clock closes a Day at its turn through this same close (central.day.turn), and only where it can file the carry in receiving: a proposal with no page of tomorrow's to wait on leaves the Day open, and its reading says why.", true, close_carry_action, &[("day_ref","string",true),("expected_revision","string",true),("expected_relations_revision","string",true),("expected_authority_revision","string",false)]),
    ]);
}
