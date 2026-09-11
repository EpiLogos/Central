//! Source-owned snapshots use the existing .central/file-history format/store.
//! This is recovery material, not another source database or authored authority.
use super::source::{self, conflict, invalid, Scope, SourceReading};
use serde_json::{json, Value};
use std::{fs, io::{self, Read}, path::Path};

fn key(value: &str) -> String { source::revision(value).replace([':', '/'], "_") }
fn read_json(scope: &Scope, path: &str) -> io::Result<Value> {
    let file = crate::file_mutation::open_native_file(&scope.root, path)?;
    let bound = (crate::source_safety::MAX_SOURCE * 6 + 65536) as u64;
    if !file.metadata()?.is_file() || file.metadata()?.len() > bound { return Err(invalid("history record is not bounded regular data")); }
    Ok(serde_json::from_reader(file.take(bound))?)
}
fn write_json(scope: &Scope, path: &str, value: &Value) -> io::Result<()> {
    let relative = crate::projectcentral_flow::relative_member(path)?;
    source::directories(&scope.root, relative.parent().ok_or_else(|| invalid("history parent missing"))?)?;
    crate::file_mutation::atomic_record(&scope.root.join(relative), &serde_json::to_vec(value)?)
}
fn area(scope: &Scope, source_ref: &str) -> io::Result<String> {
    let area = format!(".central/file-history/{}", key(source_ref));
    source::directories(&scope.root, Path::new(&area))?;
    let path = format!("{area}/identity.json");
    let identity = json!({"schema":"central.source-history-identity/v1","source_ref":source_ref,"world_ref":scope.world_ref});
    match read_json(scope, &path) {
        Ok(old) if old == identity => {},
        Ok(_) => return Err(conflict("existing native history identity differs")),
        Err(e) if e.kind() == io::ErrorKind::NotFound => write_json(scope, &path, &identity)?,
        Err(e) => return Err(e),
    }
    Ok(area)
}
fn snapshot(scope: &Scope, area: &str, content: &str) -> io::Result<()> {
    let revision = source::revision(content);
    let path = format!("{area}/{}.json", key(&revision));
    let value = json!({"revision":revision,"content":content});
    match read_json(scope, &path) {
        Ok(old) if old == value => Ok(()),
        Ok(_) => Err(conflict("native history revision collision")),
        Err(e) if e.kind() == io::ErrorKind::NotFound => write_json(scope, &path, &value),
        Err(e) => Err(e),
    }
}
fn events(scope: &Scope, area: &str) -> io::Result<Vec<Value>> {
    let mut entries = Vec::new();
    for entry in fs::read_dir(scope.root.join(area))? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.strip_prefix("event-").and_then(|n| n.strip_suffix(".json")).and_then(|n| n.parse::<u64>().ok()).is_some() {
            entries.push(read_json(scope, &format!("{area}/{name}"))?);
        }
    }
    entries.sort_by_key(|v| v["cursor"].as_u64().unwrap_or(0));
    Ok(entries)
}
fn reconcile_pending(scope: &Scope, source_ref: &str, area: &str) -> io::Result<()> {
    let path = format!("{area}/pending.json");
    let pending = match read_json(scope, &path) {
        Ok(value) => value,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e),
    };
    let current = scope.read(source_ref)?;
    if pending["revision"] == current.revision.revision {
        let event = format!("{area}/event-{}.json", pending["cursor"].as_u64().ok_or_else(|| invalid("invalid history cursor"))?);
        write_json(scope, &event, &pending)?;
    } else if pending["previous_revision"] != current.revision.revision {
        return Err(conflict("source has unresolved interrupted history: bytes match neither exact basis nor intended result; explicit recovery required"));
    }
    fs::remove_file(scope.root.join(path))?;
    crate::file_mutation::directory(&scope.root, Path::new(area))?.sync_all()
}
/// Caller holds the source-mutation locks and has already enforced native
/// authorship/contribution permissions. Source bytes are never merged here.
pub(crate) fn replace(scope: &Scope, reading: &SourceReading, content: &str, actor: &str, kind: &str, now: u64) -> io::Result<SourceReading> {
    source::directories(&scope.root, Path::new(".central/file-history"))?;
    let _history = crate::source_safety::lock(&scope.root, "file-history/lock")?;
    let area = area(scope, &reading.source.source_ref)?;
    reconcile_pending(scope, &reading.source.source_ref, &area)?;
    if scope.read(&reading.source.source_ref)?.revision != reading.revision { return Err(conflict("source changed before its history commit")); }
    if content == reading.content { return Ok(reading.clone()); }
    snapshot(scope, &area, &reading.content)?;
    snapshot(scope, &area, content)?;
    let cursor = events(scope, &area)?.last().and_then(|v| v["cursor"].as_u64()).unwrap_or(0)
        .checked_add(1).ok_or_else(|| invalid("history cursor exhausted"))?;
    let event = json!({"cursor":cursor,"previous_revision":reading.revision.revision,"revision":source::revision(content),"actor":actor,"actor_kind":kind,"agent_session_ref":null,"restored_from":null,"source_path":reading.source.path,"recorded_at_unix_seconds":now});
    write_json(scope, &format!("{area}/pending.json"), &event)?;
    crate::source_safety::replace(&scope.root, &reading.source.path, &reading.revision.revision, content)?;
    reconcile_pending(scope, &reading.source.source_ref, &area)?;
    scope.reconcile(Some((actor, kind, None)), std::slice::from_ref(&reading.source.source_ref))?;
    scope.read(&reading.source.source_ref)
}
pub(crate) fn read(scope: &Scope, input: &Value) -> io::Result<Value> {
    let source_ref = source::text(input, "source_ref")?;
    let current = scope.read(source_ref)?;
    let area = format!(".central/file-history/{}", key(source_ref));
    if !scope.root.join(&area).exists() { return Ok(json!({"source":current.source,"revision":current.revision,"entries":[],"more":false,"next_before":null})); }
    let limit = input.get("limit").and_then(Value::as_u64).unwrap_or(20);
    if limit == 0 || limit > 200 { return Err(invalid("history limit must be 1..200")); }
    let before = input.get("before").and_then(Value::as_u64).unwrap_or(u64::MAX);
    let mut entries: Vec<_> = events(scope, &area)?.into_iter().rev().filter(|v| v["cursor"].as_u64().is_some_and(|n| n < before)).collect();
    let more = entries.len() > limit as usize;
    entries.truncate(limit as usize);
    let next = if more { entries.last().map(|v| v["cursor"].clone()) } else { None };
    Ok(json!({"schema":"central.source-history/v1","source":current.source,"revision":current.revision,"entries":entries,"more":more,"next_before":next,"provider":"central.native-file-history","automatic_agent_or_model_invocation":false}))
}
