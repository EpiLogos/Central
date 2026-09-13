//! The T/T' contemplative stream of a bounded NOW. `T/` holds the raw
//! contemplative fixtures of one clearing — dated, attributed markdown. The
//! prime folder holds learnings distilled from T, each naming the fixture(s)
//! it was parsed from. Both ride the NOW clearing like every other fixture:
//! written only while the NOW is active, snapshotted by the project day
//! close, never cleaned by this module.
//!
//! The prime stream's on-disk name is `T-prime`; the apostrophe spelling is
//! the concept's name, not the directory name, which must survive shells and
//! globs. Fixture and learning files are one markdown document each with a
//! `---`-delimited JSON front-matter block: machine-checkable attribution
//! and linkage in front, the human-readable text after.
use super::placement::{self, NowRecord};
use super::source::{self, denied, invalid, text, Scope};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub const T_DIR: &str = "T";
pub const T_PRIME_DIR: &str = "T-prime";
pub const T_FIXTURE_SCHEMA: &str = "central.t-fixture/v1";
pub const T_LEARNING_SCHEMA: &str = "central.t-learning/v1";
pub const THOUGHTS_READING_SCHEMA: &str = "central.thoughts-reading/v1";
pub const LEARNINGS_READING_SCHEMA: &str = "central.learnings-reading/v1";

const DEFAULT_READ_LIMIT: usize = 256;
const MAX_READ_LIMIT: usize = 4096;
const MAX_SOURCE_FIXTURES: usize = 256;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContemplativeFrontMatter {
    pub schema: String,
    pub now_ref: String,
    /// The caller-supplied local civil date the fixture belongs to.
    pub day: String,
    pub actor: String,
    pub actor_kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_session_ref: Option<String>,
    pub recorded_at_unix_seconds: u64,
    /// Learning linkage: the T fixture filenames this learning was parsed
    /// from. Always empty for raw T fixtures.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_fixtures: Vec<String>,
}

pub(crate) fn stream_destination(
    scope: &Scope,
    source_path: &str,
    prime: bool,
) -> io::Result<PathBuf> {
    let t = placement::now_destination(scope, source_path)?;
    if prime {
        Ok(t.parent()
            .ok_or_else(|| invalid("NOW clearing parent missing"))?
            .join(T_PRIME_DIR))
    } else {
        Ok(t)
    }
}

fn validate_day(value: &str) -> io::Result<String> {
    chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map(|parsed| parsed.format("%Y-%m-%d").to_string())
        .map_err(|_| invalid("day must be a valid YYYY-MM-DD local civil date"))
}

fn validate_slug(value: &str) -> io::Result<String> {
    let lawful = !value.is_empty()
        && value.len() <= 96
        && value
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && !value.starts_with('-')
        && !value.ends_with('-')
        && !value.contains("--");
    if !lawful {
        return Err(invalid(
            "slug must be kebab-case ASCII (lowercase letters, digits, single hyphens; at most 96 bytes)",
        ));
    }
    Ok(value.to_owned())
}

fn validate_attribution(actor: &str, actor_kind: &str) -> io::Result<()> {
    if actor.is_empty() || actor.len() > 256 {
        return Err(invalid("actor must be non-empty text of at most 256 bytes"));
    }
    if !matches!(actor_kind, "human" | "agent") {
        return Err(invalid("actor_kind is human or agent"));
    }
    Ok(())
}

fn optional_ref(input: &Value, field: &str) -> io::Result<Option<String>> {
    match input.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) if !value.trim().is_empty() && value.len() <= 4096 => {
            Ok(Some(value.clone()))
        }
        Some(_) => Err(invalid(format!(
            "{field} must be non-empty text of at most 4096 bytes when present"
        ))),
    }
}

fn read_limit(input: &Value) -> io::Result<usize> {
    match input.get("limit") {
        None | Some(Value::Null) => Ok(DEFAULT_READ_LIMIT),
        Some(value @ Value::Number(_)) => {
            let limit = value
                .as_u64()
                .filter(|limit| (1..=MAX_READ_LIMIT as u64).contains(limit))
                .ok_or_else(|| invalid(format!("limit must be 1..={MAX_READ_LIMIT}")))?;
            Ok(limit as usize)
        }
        Some(_) => Err(invalid("limit must be a positive integer")),
    }
}

fn render_document(matter: &ContemplativeFrontMatter, body: &str) -> io::Result<String> {
    if body.contains('\0') {
        return Err(invalid("fixture body must not contain NUL"));
    }
    Ok(format!(
        "---\n{}---\n{}\n",
        source::encoded(matter)?,
        body.trim_end()
    ))
}

fn parse_document(raw: &str) -> (Option<ContemplativeFrontMatter>, String) {
    let rest = match raw.strip_prefix("---\n") {
        Some(rest) => rest,
        None => return (None, raw.to_owned()),
    };
    let (front, body) = match rest.split_once("\n---\n") {
        Some(split) => split,
        None => return (None, raw.to_owned()),
    };
    match serde_json::from_str::<ContemplativeFrontMatter>(front) {
        Ok(matter) => {
            let body = body.strip_suffix('\n').unwrap_or(body);
            (Some(matter), body.to_owned())
        }
        Err(_) => (None, raw.to_owned()),
    }
}

fn filename(slug: &str, day: &str) -> String {
    format!("{slug}-{day}.md")
}

/// One plain filename inside the stream directory — no separators, no
/// escape, markdown. Learnings cite T fixtures by this exact form.
fn validate_fixture_reference(scope: &Scope, t_dir: &Path, reference: &str) -> io::Result<String> {
    if reference.len() > 200
        || !reference.ends_with(".md")
        || reference.chars().any(|c| c == '/' || c == '\\')
        || reference != Path::new(reference).to_string_lossy()
    {
        return Err(invalid(
            "source fixture references must be plain T/ filenames like `<slug>-<day>.md`",
        ));
    }
    match crate::source_safety::read(
        &scope.root,
        &t_dir
            .join(reference)
            .strip_prefix(&scope.root)
            .map_err(io::Error::other)?
            .to_string_lossy(),
    ) {
        Ok(_) => Ok(reference.to_owned()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Err(invalid(format!(
            "source fixture does not exist in this NOW's T/: {reference}"
        ))),
        Err(e) => Err(e),
    }
}

struct StreamRow {
    file: String,
    revision: String,
    matter: Option<ContemplativeFrontMatter>,
    body: String,
}

fn list_stream(
    scope: &Scope,
    dir: &Path,
    limit: usize,
    include_content: bool,
) -> io::Result<(Vec<StreamRow>, usize)> {
    let mut names: Vec<String> = match fs::read_dir(dir) {
        Ok(entries) => entries
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .filter(|entry| entry.file_type().map(|t| t.is_file()).unwrap_or(false))
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name.ends_with(".md"))
            .collect(),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Vec::new(),
        Err(e) => return Err(e),
    };
    names.sort();
    let total = names.len();
    let mut rows = Vec::new();
    for name in names.into_iter().take(limit) {
        let relative = dir
            .join(&name)
            .strip_prefix(&scope.root)
            .map_err(io::Error::other)?
            .to_string_lossy()
            .into_owned();
        let raw = crate::source_safety::read(&scope.root, &relative)?;
        let (matter, body) = parse_document(&raw);
        rows.push(StreamRow {
            file: name,
            revision: source::revision(&raw),
            matter,
            body: if include_content { body } else { String::new() },
        });
    }
    Ok((rows, total))
}

fn row_value(row: &StreamRow, include_content: bool) -> Value {
    let mut value = json!({
        "file": row.file,
        "revision": row.revision,
        "conforming": row.matter.is_some(),
    });
    if let Some(matter) = &row.matter {
        value["day"] = json!(matter.day);
        value["actor"] = json!(matter.actor);
        value["actor_kind"] = json!(matter.actor_kind);
        value["agent_session_ref"] = json!(matter.agent_session_ref);
        value["recorded_at_unix_seconds"] = json!(matter.recorded_at_unix_seconds);
        if !matter.source_fixtures.is_empty() {
            value["source_fixtures"] = json!(matter.source_fixtures);
        }
    }
    if include_content {
        value["content"] = json!(row.body);
    }
    value
}

fn write_fixture(
    scope: &Scope,
    record: &placement::NowRecord,
    reading: &source::SourceReading,
    prime: bool,
    slug: &str,
    day: &str,
    actor: &str,
    actor_kind: &str,
    agent_session_ref: Option<String>,
    body: &str,
    source_fixtures: Vec<String>,
    now: u64,
) -> io::Result<Value> {
    if record.lifecycle != "active" {
        return Err(denied(
            "task NOW is not active; use explicit lifecycle re-entry before any task write",
        ));
    }
    let matter = ContemplativeFrontMatter {
        schema: if prime {
            T_LEARNING_SCHEMA.to_owned()
        } else {
            T_FIXTURE_SCHEMA.to_owned()
        },
        now_ref: record.now_ref.clone(),
        day: day.to_owned(),
        actor: actor.to_owned(),
        actor_kind: actor_kind.to_owned(),
        agent_session_ref,
        recorded_at_unix_seconds: now,
        source_fixtures,
    };
    let dir = stream_destination(scope, &reading.source.path, prime)?;
    let relative = dir
        .join(filename(slug, day))
        .strip_prefix(&scope.root)
        .map_err(io::Error::other)?
        .to_string_lossy()
        .into_owned();
    let document = render_document(&matter, body)?;
    let created = source::put_new(&scope.root, &relative, &document)?;
    Ok(json!({
        "schema": if prime { "central.t-learning-receipt/v1" } else { "central.t-fixture-receipt/v1" },
        "now_ref": record.now_ref,
        "file": filename(slug, day),
        "path": relative,
        "revision": source::revision(&document),
        "created": created,
        "recorded_at_unix_seconds": now,
        "automatic_agent_or_model_invocation": false,
    }))
}

pub(crate) fn thoughts_read(scope: &Scope, input: &Value) -> io::Result<Value> {
    let (record, reading) = placement::read_now(scope, text(input, "now_ref")?)?;
    let limit = read_limit(input)?;
    let include_content = matches!(input.get("include_content"), Some(Value::Bool(true)));
    let dir = stream_destination(scope, &reading.source.path, false)?;
    let (rows, total) = list_stream(scope, &dir, limit, include_content)?;
    Ok(json!({
        "schema": THOUGHTS_READING_SCHEMA,
        "now_ref": record.now_ref,
        "source_ref": record.source_ref,
        "revision": reading.revision,
        "directory": dir.strip_prefix(&scope.root).map_err(io::Error::other)?.to_string_lossy(),
        "total": total,
        "truncated": total > rows.len(),
        "fixtures": rows.iter().map(|row| row_value(row, include_content)).collect::<Vec<_>>(),
        "automatic_agent_or_model_invocation": false,
    }))
}

pub(crate) fn thoughts_append(scope: &Scope, input: &Value, now: u64) -> io::Result<Value> {
    let (record, reading) = placement::read_now(scope, text(input, "now_ref")?)?;
    let slug = validate_slug(text(input, "slug")?)?;
    let day = validate_day(text(input, "day")?)?;
    let actor = text(input, "actor")?;
    let actor_kind = text(input, "actor_kind")?;
    validate_attribution(actor, actor_kind)?;
    write_fixture(
        scope,
        &record,
        &reading,
        false,
        &slug,
        &day,
        actor,
        actor_kind,
        optional_ref(input, "agent_session_ref")?,
        text(input, "content")?,
        Vec::new(),
        now,
    )
}

pub(crate) fn learnings_read(scope: &Scope, input: &Value) -> io::Result<Value> {
    let limit = read_limit(input)?;
    let include_content = matches!(input.get("include_content"), Some(Value::Bool(true)));
    let mut learnings = Vec::new();
    let mut truncated = false;
    match input.get("now_ref") {
        None | Some(Value::Null) => {
            for binding in scope
                .bindings()?
                .into_iter()
                .filter(|b| b.roles.iter().any(|r| r == "now-clearing"))
            {
                let source = scope.read(&binding.source_ref)?;
                let record: NowRecord = serde_json::from_str(&source.content)?;
                if record.schema != placement::NOW_SCHEMA
                    || record.scope_ref != scope.world_ref
                    || record.source_ref != binding.source_ref
                {
                    return Err(invalid("NOW source identity/schema mismatch"));
                }
                let dir = stream_destination(scope, &source.source.path, true)?;
                let (rows, total) = list_stream(scope, &dir, limit, include_content)?;
                truncated |= total > rows.len();
                for row in &rows {
                    learnings.push(json!({
                        "now_ref": record.now_ref,
                        "fixture": row_value(row, include_content),
                    }));
                }
            }
            Ok(json!({
                "schema": LEARNINGS_READING_SCHEMA,
                "now_ref": Value::Null,
                "total": learnings.len(),
                "truncated": truncated,
                "learnings": learnings,
                "automatic_agent_or_model_invocation": false,
            }))
        }
        Some(reference) => {
            let (record, reading) = placement::read_now(scope, text(input, "now_ref")?)?;
            let dir = stream_destination(scope, &reading.source.path, true)?;
            let (rows, total) = list_stream(scope, &dir, limit, include_content)?;
            Ok(json!({
                "schema": LEARNINGS_READING_SCHEMA,
                "now_ref": reference.as_str().unwrap_or(""),
                "source_ref": record.source_ref,
                "revision": reading.revision,
                "directory": dir.strip_prefix(&scope.root).map_err(io::Error::other)?.to_string_lossy(),
                "total": total,
                "truncated": total > rows.len(),
                "learnings": rows.iter().map(|row| row_value(row, include_content)).collect::<Vec<_>>(),
                "automatic_agent_or_model_invocation": false,
            }))
        }
    }
}

pub(crate) fn learnings_distill(scope: &Scope, input: &Value, now: u64) -> io::Result<Value> {
    let (record, reading) = placement::read_now(scope, text(input, "now_ref")?)?;
    let slug = validate_slug(text(input, "slug")?)?;
    let day = validate_day(text(input, "day")?)?;
    let actor = text(input, "actor")?;
    let actor_kind = text(input, "actor_kind")?;
    validate_attribution(actor, actor_kind)?;
    let references: Vec<String> = serde_json::from_value(
        input
            .get("source_fixtures")
            .cloned()
            .ok_or_else(|| invalid("source_fixtures required"))?,
    )?;
    if references.is_empty() || references.len() > MAX_SOURCE_FIXTURES {
        return Err(invalid(format!(
            "a learning names one or more source fixtures, at most {MAX_SOURCE_FIXTURES}"
        )));
    }
    let t_dir = stream_destination(scope, &reading.source.path, false)?;
    let mut source_fixtures = Vec::new();
    for reference in &references {
        source_fixtures.push(validate_fixture_reference(scope, &t_dir, reference)?);
    }
    source_fixtures.sort();
    source_fixtures.dedup();
    write_fixture(
        scope,
        &record,
        &reading,
        true,
        &slug,
        &day,
        actor,
        actor_kind,
        optional_ref(input, "agent_session_ref")?,
        text(input, "content")?,
        source_fixtures,
        now,
    )
}

/// One clearing's contemplative streams, byte-exact at the day close.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StreamDaySnapshot {
    pub schema: String,
    pub clearing: String,
    pub fixtures: usize,
    pub learnings: usize,
    pub snapshot_dir: String,
}

fn copy_dir_recursive(source: &Path, destination: &Path) -> io::Result<usize> {
    let mut copied = 0;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let target = destination.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copied += copy_dir_recursive(&entry.path(), &target)?;
        } else {
            fs::create_dir_all(destination)?;
            fs::copy(entry.path(), target)?;
            copied += 1;
        }
    }
    Ok(copied)
}

/// Snapshot every clearing's T/ and T-prime/ into the day's source snapshot.
/// Byte-exact retention before anything is cleaned; the close never removes
/// stream material — the streams outlive the day like every other fixture.
pub(crate) fn snapshot_streams(
    scope_prefix_clearings: &Path,
    snapshot_root: &Path,
    day: &str,
) -> io::Result<Vec<StreamDaySnapshot>> {
    let entries = match fs::read_dir(scope_prefix_clearings) {
        Ok(entries) => entries.collect::<Result<Vec<_>, _>>()?,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(vec![]),
        Err(e) => return Err(e),
    };
    let mut clearings: Vec<String> = entries
        .into_iter()
        .filter(|entry| entry.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    clearings.sort();
    let mut snapshots = Vec::new();
    for clearing in clearings {
        let clearing_root = scope_prefix_clearings.join(&clearing);
        let mut counts = [0usize; 2];
        for (index, name) in [T_DIR, T_PRIME_DIR].into_iter().enumerate() {
            if clearing_root.join(name).is_dir() {
                counts[index] = copy_dir_recursive(
                    &clearing_root.join(name),
                    &snapshot_root.join("clearings").join(&clearing).join(name),
                )?;
            }
        }
        if counts[0] == 0 && counts[1] == 0 {
            continue;
        }
        snapshots.push(StreamDaySnapshot {
            schema: "central.t-day-snapshot/v1".into(),
            clearing: clearing.clone(),
            fixtures: counts[0],
            learnings: counts[1],
            snapshot_dir: format!("clearings/{clearing}"),
        });
    }
    if snapshots.is_empty() {
        return Ok(snapshots);
    }
    let index = json!({"schema": "central.t-day-snapshot/v1", "day": day, "clearings": snapshots});
    let mut bytes = serde_json::to_vec_pretty(&index).map_err(io::Error::other)?;
    bytes.push(b'\n');
    fs::write(snapshot_root.join("clearings.json"), bytes)?;
    Ok(snapshots)
}

#[cfg(test)]
mod tests {
    use super::super::tests::world;
    use super::*;

    fn allocated(root: &Path, project: Option<&str>, task: &str) -> Value {
        let policy = super::super::tests::policy(root, project);
        super::super::execute_at(
            root,
            "allocate",
            &json!({"project":project,"task_ref":task,"purpose":"bounded implementation","participant_refs":["agent:test"],"source_refs":[],"expected_policy_revision":policy["revision"]}),
            100,
        )
        .unwrap()
    }

    fn append(root: &Path, project: Option<&str>, allocation: &Value, slug: &str) -> Value {
        super::super::execute_at(
            root,
            "thoughts_append",
            &json!({
                "project": project,
                "now_ref": allocation["now_ref"],
                "slug": slug,
                "day": "2026-09-13",
                "actor": "agent:test",
                "actor_kind": "agent",
                "agent_session_ref": "sess:test",
                "content": "## Raw stream fixture\n\nWhat returned today.",
            }),
            200,
        )
        .unwrap()
    }

    #[test]
    fn fixture_roundtrip_is_dated_attributed_and_readable() {
        let temp = world();
        let root = temp.path();
        for project in [None, Some("one")] {
            let allocation = allocated(root, project, "task:thoughts");
            let receipt = append(root, project, &allocation, "session-return");
            assert_eq!(receipt["schema"], "central.t-fixture-receipt/v1");
            assert_eq!(receipt["file"], "session-return-2026-09-13.md");
            assert!(receipt["created"] == true);

            let reading = super::super::execute_at(
                root,
                "thoughts_read",
                &json!({"project":project,"now_ref":allocation["now_ref"],"include_content":true}),
                201,
            )
            .unwrap();
            assert_eq!(reading["schema"], THOUGHTS_READING_SCHEMA);
            assert_eq!(reading["total"], 1);
            let fixture = &reading["fixtures"][0];
            assert_eq!(fixture["conforming"], true);
            assert_eq!(fixture["day"], "2026-09-13");
            assert_eq!(fixture["actor"], "agent:test");
            assert_eq!(fixture["actor_kind"], "agent");
            assert_eq!(fixture["agent_session_ref"], "sess:test");
            assert_eq!(fixture["recorded_at_unix_seconds"], 200);
            assert!(fixture["content"]
                .as_str()
                .unwrap()
                .contains("Raw stream fixture"));
            let scope_root = match project {
                Some(name) => root.join("Work").join(name),
                None => root.to_path_buf(),
            };
            let front = serde_json::from_str::<ContemplativeFrontMatter>(
                String::from_utf8(
                    fs::read(scope_root.join(receipt["path"].as_str().unwrap())).unwrap(),
                )
                .unwrap()
                .split("---\n")
                .nth(1)
                .unwrap(),
            )
            .unwrap();
            assert_eq!(front.schema, T_FIXTURE_SCHEMA);
            assert_eq!(front.now_ref, allocation["now_ref"]);
            assert!(front.source_fixtures.is_empty());
        }
    }

    #[test]
    fn identical_fixture_reappends_idempotently_and_different_bytes_conflict() {
        let temp = world();
        let root = temp.path();
        let allocation = allocated(root, None, "task:idempotent");
        let first = append(root, None, &allocation, "same-slug");
        let again = append(root, None, &allocation, "same-slug");
        assert!(first["created"] == true);
        assert!(again["created"] == false);
        assert_eq!(first["revision"], again["revision"]);
        let mut divergent = json!({
            "project": Value::Null,
            "now_ref": allocation["now_ref"],
            "slug": "same-slug",
            "day": "2026-09-13",
            "actor": "agent:test",
            "actor_kind": "agent",
            "content": "different bytes entirely",
        });
        assert_eq!(
            super::super::execute_at(root, "thoughts_append", &divergent, 202)
                .unwrap_err()
                .kind(),
            io::ErrorKind::AlreadyExists
        );
        divergent["slug"] = json!("other-slug");
        assert!(super::super::execute_at(root, "thoughts_append", &divergent, 202).is_ok());
    }

    #[test]
    fn append_refuses_inactive_now_and_invalid_input() {
        let temp = world();
        let root = temp.path();
        let allocation = allocated(root, None, "task:inactive");
        let now_path = root.join(allocation["source"]["path"].as_str().unwrap());
        let mut record: Value =
            serde_json::from_str(&fs::read_to_string(&now_path).unwrap()).unwrap();
        record["lifecycle"] = json!("quiescent");
        fs::write(&now_path, source::encoded(&record).unwrap()).unwrap();
        assert_eq!(
            super::super::execute_at(
                root,
                "thoughts_append",
                &json!({
                    "now_ref": allocation["now_ref"], "slug": "still-writing",
                    "day": "2026-09-13", "actor": "agent:test", "actor_kind": "agent",
                    "content": "must refuse",
                }),
                203
            )
            .unwrap_err()
            .kind(),
            io::ErrorKind::PermissionDenied
        );
        record["lifecycle"] = json!("active");
        fs::write(&now_path, source::encoded(&record).unwrap()).unwrap();
        let mutates: [fn(&mut Value); 6] = [
            |input: &mut Value| input["day"] = json!("09/13/2026"),
            |input: &mut Value| input["day"] = json!("2026-02-30"),
            |input: &mut Value| input["slug"] = json!("Bad_Slug"),
            |input: &mut Value| input["slug"] = json!("-leading"),
            |input: &mut Value| input["actor_kind"] = json!("machine"),
            |input: &mut Value| input["content"] = json!(""),
        ];
        let mut input = json!({
            "now_ref": allocation["now_ref"], "slug": "lawful-slug",
            "day": "2026-09-13", "actor": "agent:test", "actor_kind": "agent",
            "content": "lawful body",
        });
        for mutate in mutates {
            mutate(&mut input);
            assert_eq!(
                super::super::execute_at(root, "thoughts_append", &input, 203)
                    .unwrap_err()
                    .kind(),
                io::ErrorKind::InvalidInput,
                "{input}"
            );
        }
    }

    #[test]
    fn distillation_links_named_fixtures_and_cross_now_listing_works() {
        let temp = world();
        let root = temp.path();
        let alpha = allocated(root, None, "task:alpha");
        append(root, None, &alpha, "raw-finding");
        append(root, None, &alpha, "second-finding");
        let missing = super::super::execute_at(
            root,
            "learnings_distill",
            &json!({
                "now_ref": alpha["now_ref"], "slug": "premature-learning",
                "day": "2026-09-13", "actor": "agent:test", "actor_kind": "agent",
                "content": "names a fixture that does not exist",
                "source_fixtures": ["never-written-2026-09-13.md"],
            }),
            300,
        )
        .unwrap_err();
        assert_eq!(missing.kind(), io::ErrorKind::InvalidInput);
        assert!(
            super::super::execute_at(
                root,
                "learnings_distill",
                &json!({
                    "now_ref": alpha["now_ref"], "slug": "premature-learning",
                    "day": "2026-09-13", "actor": "agent:test", "actor_kind": "agent",
                    "content": "empty linkage",
                    "source_fixtures": [],
                }),
                300
            )
            .unwrap_err()
            .kind()
                == io::ErrorKind::InvalidInput
        );

        let learning = super::super::execute_at(
            root,
            "learnings_distill",
            &json!({
                "now_ref": alpha["now_ref"], "slug": "distilled-signal",
                "day": "2026-09-13", "actor": "agent:test", "actor_kind": "agent",
                "content": "The two raw findings carry one signal.",
                "source_fixtures": [
                    "second-finding-2026-09-13.md",
                    "raw-finding-2026-09-13.md",
                ],
            }),
            301,
        )
        .unwrap();
        assert_eq!(learning["schema"], "central.t-learning-receipt/v1");
        assert!(learning["path"].as_str().unwrap().contains("/T-prime/"));
        let document = fs::read_to_string(root.join(learning["path"].as_str().unwrap())).unwrap();
        assert!(document.contains("central.t-learning/v1"));
        assert!(document.contains("raw-finding-2026-09-13.md"));
        assert!(document.contains("second-finding-2026-09-13.md"));

        let one = super::super::execute_at(
            root,
            "learnings_read",
            &json!({"now_ref": alpha["now_ref"]}),
            302,
        )
        .unwrap();
        assert_eq!(one["schema"], LEARNINGS_READING_SCHEMA);
        assert_eq!(one["total"], 1);
        assert_eq!(
            one["learnings"][0]["source_fixtures"],
            json!(["raw-finding-2026-09-13.md", "second-finding-2026-09-13.md"])
        );
        let beta = allocated(root, None, "task:beta");
        append(root, None, &beta, "beta-raw");
        let all = super::super::execute_at(root, "learnings_read", &json!({}), 303).unwrap();
        assert_eq!(all["total"], 1);
        assert_eq!(all["learnings"][0]["now_ref"], alpha["now_ref"]);
        assert_eq!(all["schema"], LEARNINGS_READING_SCHEMA);
    }

    #[test]
    fn pre_law_fixtures_list_as_nonconforming_without_hiding() {
        let temp = world();
        let root = temp.path();
        let allocation = allocated(root, None, "task:legacy");
        let t_dir = Path::new(allocation["writable_destination"].as_str().unwrap()).to_path_buf();
        fs::write(
            t_dir.join("legacy-field-active-review-2026-09-12.md"),
            "# A pre-law fixture with no front matter\n",
        )
        .unwrap();
        let reading = super::super::execute_at(
            root,
            "thoughts_read",
            &json!({"now_ref": allocation["now_ref"]}),
            400,
        )
        .unwrap();
        assert_eq!(reading["total"], 1);
        assert_eq!(reading["fixtures"][0]["conforming"], false);
        assert!(reading["fixtures"][0].get("actor").is_none());
    }

    #[test]
    fn project_scope_streams_land_under_projectcentral() {
        let temp = world();
        let root = temp.path();
        let allocation = allocated(root, Some("one"), "task:project");
        let receipt = append(root, Some("one"), &allocation, "project-fixture");
        let path = receipt["path"].as_str().unwrap();
        assert!(path.starts_with("ProjectCentral/agents/now/clearings/"));
        assert!(root.join("Work/one").join(path).is_file());
        let reading = super::super::execute_at(
            root,
            "thoughts_read",
            &json!({"project":"one","now_ref":allocation["now_ref"]}),
            500,
        )
        .unwrap();
        assert_eq!(reading["total"], 1);
    }

    #[test]
    fn stream_snapshot_copies_both_streams_byte_exact() {
        let temp = world();
        let root = temp.path();
        let allocation = allocated(root, None, "task:snapshot");
        append(root, None, &allocation, "snapshotted-fixture");
        super::super::execute_at(
            root,
            "learnings_distill",
            &json!({
                "now_ref": allocation["now_ref"], "slug": "snapshotted-learning",
                "day": "2026-09-13", "actor": "agent:test", "actor_kind": "agent",
                "content": "learning body",
                "source_fixtures": ["snapshotted-fixture-2026-09-13.md"],
            }),
            600,
        )
        .unwrap();
        let snapshot_root = root.join("day/2026-09-13.sources");
        fs::create_dir_all(&snapshot_root).unwrap();
        let clearings_root = root.join("Control/agents/now/clearings");
        let snapshots = snapshot_streams(&clearings_root, &snapshot_root, "2026-09-13").unwrap();
        assert_eq!(snapshots.len(), 1);
        assert_eq!(snapshots[0].fixtures, 1);
        assert_eq!(snapshots[0].learnings, 1);
        let original = root
            .join("Control/agents/now/clearings")
            .read_dir()
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path()
            .join("T/snapshotted-fixture-2026-09-13.md");
        let copy = snapshot_root
            .join("clearings")
            .join(&snapshots[0].clearing)
            .join("T/snapshotted-fixture-2026-09-13.md");
        assert_eq!(fs::read(original).unwrap(), fs::read(copy).unwrap());
        let index: Value = serde_json::from_str(
            &fs::read_to_string(snapshot_root.join("clearings.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(index["schema"], "central.t-day-snapshot/v1");
        assert_eq!(index["clearings"][0]["learnings"], 1);
        // An empty clearings root snapshots nothing and writes no index.
        let empty = snapshot_streams(
            &root.join("Control/agents/now/nowhere"),
            &snapshot_root,
            "2026-09-13",
        )
        .unwrap();
        assert!(empty.is_empty());
    }
}
