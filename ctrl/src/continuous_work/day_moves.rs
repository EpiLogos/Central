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
use chrono::{DateTime, Duration, NaiveDate, Timelike, Utc};
use chrono_tz::Tz;
use serde_json::{json, Map, Value};
use std::{collections::BTreeMap, io};

pub(crate) const MOVES: [&str; 5] = ["tasks", "patterns", "context", "synthesis", "close"];
const SOURCES: [&str; 5] = ["central", "factory", "expressions", "techne", "live"];
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
        ) {
            return Err(invalid(format!("the Day's settings have no `{key}`")));
        }
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
            .ok_or_else(|| invalid("sources is {central, factory, expressions, techne, live}"))?;
        for (name, value) in sources {
            if !SOURCES.contains(&name.as_str()) {
                return Err(invalid(format!("`{name}` is not a source mode")));
            }
            if !value.is_boolean() {
                return Err(invalid(format!("source {name} is true or false")));
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

fn settings_reading(scope: &Scope, loaded: &Loaded) -> io::Result<Value> {
    let standing = |saved: bool| if saved { "saved" } else { "default" };
    let (rollover, rollover_saved) = loaded.rollover();
    let (agent_writes, agent_writes_saved) = loaded.agent_writes();
    let (close_routine, close_routine_saved) = loaded.close_routine();
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
            "agent_writes":agent_writes,"sources":sources,"close_routine":close_routine,"moves":moves},
        "standing":{"rollover":standing(rollover_saved),"agent_writes":standing(agent_writes_saved),
            "sources":sources_standing,"close_routine":standing(close_routine_saved),"moves":moves_standing},
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
            "rollover" | "agent_writes" | "sources" | "close_routine" | "moves" => changes += 1,
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
    if let Some(patch) = object.get("sources") {
        let patch = patch
            .as_object()
            .ok_or_else(|| invalid("sources is {central, factory, expressions, techne, live}"))?;
        let mut sources = merged
            .get("sources")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        for (name, value) in patch {
            sources.insert(name.clone(), value.clone());
        }
        merged.insert("sources".into(), Value::Object(sources));
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

/// The civil date of the Day an occurrence belongs to: the Day it falls in, except
/// that Close at Day start closes the Day that ends at that instant.
fn occurrence_civil_date(
    scope: &Scope,
    mv: &str,
    kind: &str,
    instant_ms: i64,
) -> io::Result<String> {
    let basis = if mv == "close" && kind == "day_start" {
        instant_ms - 1
    } else {
        instant_ms
    };
    civil_date_at(scope, basis)
}

fn day_ref_of(scope: &Scope, civil_date: &str) -> String {
    format!("central:day:{}:{}", scope.world_ref, civil_date)
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

struct Log {
    exists: bool,
    raw: String,
    revision: String,
    events: Vec<Value>,
}

fn load_log(scope: &Scope, civil_date: &str) -> io::Result<Log> {
    let Some(raw) = read_optional(scope, &record_path(scope, civil_date))? else {
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
                "a line of the Day's move record is not an event; it is kept as it is",
            )
        })?;
        if event["schema"] != EVENT_SCHEMA {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "a line of the Day's move record is not a central.day-move-event/v1",
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
    let path = record_path(scope, civil_date);
    let mut content = log.raw.clone();
    if !content.is_empty() && !content.ends_with('\n') {
        content.push('\n');
    }
    content.push_str(&serde_json::to_string(event)?);
    content.push('\n');
    if log.exists {
        crate::source_safety::replace(&scope.root, &path, &log.revision, &content)?;
    } else {
        source::put_new(&scope.root, &path, &content)?;
    }
    scope.reconcile(
        Some((&principal.principal_ref, &principal.actor_kind, None)),
        &[scope.source_ref(&path)],
    )?;
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

fn bounded_text_list(gave: &Value, key: &str) -> io::Result<Vec<String>> {
    match gave.get(key) {
        None | Some(Value::Null) => Ok(vec![]),
        Some(Value::Array(rows)) if rows.len() <= 256 => rows
            .iter()
            .map(|row| {
                row.as_str()
                    .filter(|row| !row.trim().is_empty() && row.len() <= 4096)
                    .map(str::to_owned)
                    .ok_or_else(|| invalid(format!("gave.{key} is a list of refs")))
            })
            .collect(),
        _ => Err(invalid(format!("gave.{key} is a list of refs"))),
    }
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
                return Err(conflict(format!("{run_ref} has ended already")));
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
                        invalid("gave is {summary, proposals?, writes?, counts?}")
                    })?;
                    if let Some(key) = object.keys().find(|key| {
                        !matches!(key.as_str(), "summary" | "proposals" | "writes" | "counts")
                    }) {
                        return Err(invalid(format!("gave has no `{key}`")));
                    }
                    text(gave, "summary")?;
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

fn consent_to_close(settings: &Loaded, time: &TimeReading, civil_date: &str) -> io::Result<()> {
    let (runs_at, saved) = settings.runs_at("close")?;
    if !saved || runs_at != RunsAt::DayStart {
        return Err(denied(
            "An agent runs the Close routine only when you saved Close at Day start; the Day's settings hold no such saving.",
        ));
    }
    if time.civil_date.as_str() <= civil_date {
        return Err(denied(format!(
            "The Day boundary that ends the Day of {civil_date} has not passed; an agent closes it then."
        )));
    }
    Ok(())
}

fn carry_state(state: &str, reason: &str) -> Value {
    json!({"state":state,"reason":reason,"from":{"pointer":FACE_FIVE_PROPOSAL},"to":{"pointer":FACE_ZERO_GROUND}})
}

/// Copy face 5's proposal for tomorrow into tomorrow's face 0, as Central's
/// document model allows it. Whatever cannot be carried is said in words and the
/// proposal stays on the Day being closed; a write that fails is an error, so
/// nothing is closed behind it.
fn carry(
    scope: &Scope,
    closing: &Value,
    closing_ref: &str,
    tomorrow_ref: &str,
    principal: &Principal,
    now: u64,
) -> io::Result<Value> {
    let Some(closing_document) = closing.get("document").filter(|meta| meta.is_object()) else {
        return Ok(carry_state(
            "nothing_to_carry",
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
        .unwrap_or("");
    if plain_text(proposal).trim().is_empty() {
        return Ok(carry_state(
            "nothing_to_carry",
            "Face 5 holds no proposal for tomorrow.",
        ));
    }
    let (relations, _) = scope.relations()?;
    let tomorrow = temporal::day_relation(&relations, tomorrow_ref)?.clone();
    let not_carried = |reason: String| {
        let mut state = carry_state("not_carried", &reason);
        state["proposal_text"] = json!(proposal);
        state
    };
    let Some(tomorrow_meta) = tomorrow.get("document").filter(|meta| meta.is_object()) else {
        return Ok(not_carried("Tomorrow's Day has no Day document yet, so face 0 has nowhere to take the proposal; it stays on this Day's face 5.".into()));
    };
    let target = documents::read(
        scope,
        &json!({"source_ref":tomorrow["ref"],"document_id":tomorrow_meta["document_id"]}),
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
        return Ok(not_carried(format!(
            "Tomorrow's Day document maps no field at {FACE_ZERO_GROUND} (it maps {}), so face 0 has nowhere to take the proposal.",
            if mapped.is_empty() { "none".to_owned() } else { mapped.join(", ") }
        )));
    };
    let field_id = text(field, "id")?.to_owned();
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
        // A Day takes an agent's entry only through the person (receiving).
        state["state"] = json!("waits");
        state["reason"] = json!("An agent's entry into a Day waits for the person: submit it through central.receiving.submit.");
        state["proposal"] = json!({"source_ref":target["source"]["ref"],"document_id":target["document_id"],
            "expected_source_revision":target["revision"]["revision"],"proposal":operation});
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
        consent_to_close(&settings, &time, &civil_date)?;
    }
    // Tomorrow's Day: the one that stands, or opened now through the same path
    // rollover and ensure use.
    let next_date = next_civil_date(&civil_date)?;
    let tomorrow_ref = day_ref_of(scope, &next_date);
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
    let carried = if settings.close_routine().0 {
        carry(scope, &entry, day_ref, &tomorrow_ref, principal, now)?
    } else {
        carry_state(
            "off",
            "The Close routine is Off, so the Day closes and carries nothing.",
        )
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

// ---- operations and actions -------------------------------------------------

/// The Action an authenticated operation is granted as.
pub(crate) fn authenticated_action(operation: &str) -> Option<&'static str> {
    Some(match operation {
        "day_settings_write" => "central.day.settings.write",
        "day_move_record" => "central.day.move.record",
        "day_rollover" => "central.day.rollover",
        "day_close_carry" => "central.day.close-carry",
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

pub(crate) fn register_actions(registry: &mut ActionRegistry) {
    super::register_definitions(registry, &[
        ("central.day.settings.read", "Read the Day's settings", "Read the Day's own settings (Rollover, Agent writes, Sources, Close routine, and each move's Prompt and Runs at) as they stand: the defaults where the person has saved nothing, each with its standing (default or saved) and its revision, and each move's own prompt. Read-only; nothing runs from a reading.", false, settings_read_action, &[]),
        ("central.day.settings.write", "Save the Day's settings", "Human-authenticated, exact-revision save of the Day's settings (expected_revision, or \"absent\" for the first save). Only the settings given are saved; every one saved here is the person's own and arms what it names. A move's Runs at at rollover is refused while Rollover is Off.", true, settings_write_action, &[("expected_revision","string",true),("rollover","string",false),("agent_writes","string",false),("sources","object",false),("close_routine","boolean",false),("moves","object",false),("expected_authority_revision","string",false)]),
        ("central.day.move.record", "Record a Day move's run", "Authenticated, append-only record of a move's run: a begin (press, or a saved schedule's occurrence, idempotent per occurrence) and an end (done, refused, stalled or stopped, with what it gave). Central assigns the run_ref and the actor from the credential; a run keeps the prompt it began with and a late end keeps the time it happened.", true, move_record_action, &[("event","string",true),("move","string",false),("at_unix_ms","integer",true),("day_ref","string",false),("trigger","string",false),("occurrence_ref","string",false),("run_ref","string",false),("state","string",false),("gave","object",false),("reason","string",false),("expected_authority_revision","string",false)]),
        ("central.day.moves.read", "Read the Day's moves", "Read the Day's settings and, for each move, its effective prompt, Runs at with its standing, the next time it runs by itself (only for a schedule the person saved), the open run, the last ended run, and every run of the Day oldest first. Read-only.", false, moves_read_action, &[("day_ref","string",false)]),
        ("central.day.moves.due", "List the Day's moves that are due", "List the scheduled occurrences in (since, now] for every move whose Runs at the person saved, each with a stable occurrence_ref and whether it has begun. With only the defaults it lists nothing and says why. The native host polls this; Central runs nothing.", false, moves_due_action, &[("now_unix_ms","integer",true),("since_unix_ms","integer",true)]),
        ("central.day.rollover", "Open tomorrow's Day beside today's", "Open the Day of the civil date after today without moving today, through the same Day creation as central.day.ensure, and record it as the Day's tomorrow. Idempotent. A human may open it at any time; another authenticated principal only once the Rollover time the person saved has been reached.", true, rollover_action, &[("expected_time_policy_revision","string",true),("expected_authority_revision","string",false)]),
        ("central.day.close-carry", "Close a Day and carry face 5 to tomorrow", "Run the close routine on exact revisions: carry face 5's proposal for tomorrow into tomorrow's face 0 (a human writes it; an agent's entry is returned as a proposal for receiving), then close the Day as central.day.lifecycle does, and return both revisions. A human may call it at any time; another authenticated principal only when the person saved Close at Day start and the Day boundary has passed.", true, close_carry_action, &[("day_ref","string",true),("expected_revision","string",true),("expected_relations_revision","string",true),("expected_authority_revision","string",false)]),
    ]);
}
