use super::{authority::{self, Principal}, history, placement, source::{self, conflict, denied, encoded, invalid, text, Scope}};
use chrono::{DateTime, Duration, Utc};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{fs, io, path::Path};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TimePolicy {
    pub schema: String,
    pub scope_ref: String,
    pub timezone: String,
    pub day_boundary_minutes: u32,
    pub automatic_day_rollover: bool,
}
#[derive(Debug, Serialize)]
pub struct TimeReading {
    pub schema: String,
    pub source_ref: String,
    pub source_revision: String,
    pub revision: String,
    pub policy: TimePolicy,
    pub civil_date: String,
    pub local_time: String,
    pub observed_at_unix_seconds: u64,
}
pub fn time_policy(scope: &Scope, now: u64) -> io::Result<TimeReading> {
    let root = Scope::resolve(&scope.central_root, None)?;
    let (reading, _, basis) = authority::recognised_source(&root, "civil-time-policy")?;
    let policy: TimePolicy = serde_json::from_str(&reading.content)?;
    if policy.schema != "central.civil-time-policy/v1" || policy.scope_ref != root.world_ref || policy.day_boundary_minutes >= 1440 {
        return Err(invalid("invalid root civil-time policy; no harness-local fallback"));
    }
    let timezone: Tz = policy.timezone.parse().map_err(|_| invalid("civil-time policy requires an IANA timezone"))?;
    let seconds: i64 = now.try_into().map_err(|_| invalid("time outside supported range"))?;
    let local = DateTime::<Utc>::from_timestamp(seconds, 0).ok_or_else(|| invalid("time outside supported range"))?.with_timezone(&timezone);
    // Boundary is a civil-wall-time cut, not an assumed 24-hour UTC duration.
    let civil = local.naive_local().checked_sub_signed(Duration::minutes(policy.day_boundary_minutes as i64))
        .ok_or_else(|| invalid("civil date underflow"))?.date().to_string();
    Ok(TimeReading { schema: "central.civil-time-reading/v1".into(), source_ref: reading.source.source_ref,
        source_revision: reading.revision.revision, revision: basis, policy, civil_date: civil,
        local_time: local.to_rfc3339(), observed_at_unix_seconds: now })
}
pub(crate) fn save_relations(scope: &Scope, relations: &Value, basis: &str) -> io::Result<()> {
    let content = encoded(relations)?;
    if basis == "absent" { source::put_new(&scope.root, &scope.relations_path, &content)?; }
    else { crate::source_safety::replace(&scope.root, &scope.relations_path, basis, &content)?; }
    Ok(())
}
fn day_relation<'a>(relations: &'a Value, day_ref: &str) -> io::Result<&'a Value> {
    relations["relations"].as_array().ok_or_else(|| invalid("invalid relations"))?
        .iter().find(|entry| entry["temporal"]["day_ref"] == day_ref)
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "DayRef is not recorded in this World"))
}
fn reading(scope: &Scope, relations: &Value, relation_basis: &str, entry: &Value) -> io::Result<Value> {
    let source = scope.read(text(entry, "ref")?)?;
    Ok(json!({"schema":"central.day-reading/v1","day_ref":entry["temporal"]["day_ref"],"source":source.source,"revision":source.revision,"content":source.content,"temporal":entry["temporal"],"relations_revision":relation_basis,"today":relations["temporal"]["today"],"automatic_agent_or_model_invocation":false}))
}
pub fn day_read(scope: &Scope, input: &Value) -> io::Result<Value> {
    let (relations, basis) = scope.relations()?;
    let reference = input.get("day_ref").and_then(Value::as_str)
        .or_else(|| relations["temporal"]["today"]["day_ref"].as_str())
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no DayRef or current today pointer"))?;
    let entry = day_relation(&relations, reference)?;
    reading(scope, &relations, &basis, entry)
}
pub fn ensure_day(scope: &Scope, input: &Value, principal: &Principal, now: u64) -> io::Result<Value> {
    let time = time_policy(scope, now)?;
    if text(input, "expected_time_policy_revision")? != time.revision { return Err(conflict("civil-time policy changed before Day allocation")); }
    if !principal.is_human() && !time.policy.automatic_day_rollover {
        return Err(denied("blank Day rollover is not authorised by the human's current time policy"));
    }
    let day_ref = format!("central:day:{}:{}", scope.world_ref, time.civil_date);
    let (mut relations, mut basis) = scope.relations()?;
    let mut created = false;
    if day_relation(&relations, &day_ref).is_err() {
        // A genuinely blank native text source. No unavailable template field
        // names are invented; structured template import is a separate operation.
        let path = format!("{}/{}/day.md", scope.day_dir(), time.civil_date);
        let had_bytes = match crate::source_safety::read(&scope.root, &path) {
            Ok(_) => true,
            Err(e) if e.kind() == io::ErrorKind::NotFound => false,
            Err(e) => return Err(e),
        };
        if !had_bytes { source::put_new(&scope.root, &path, "")?; created = true; }
        let binding = crate::source_horizon::SourceBinding {
            source_ref: scope.source_ref(&path), path, roles: vec!["human-day".into(),
                if scope.project.is_some() { "project-human-source-aperture".into() } else { "personal-human-source-aperture".into() }],
            provenance: "observed".into(), standing: "current-development-state".into(), treatment: "projectcentral-user".into(), agent_retrieval_allowed: true,
        };
        scope.bind(&binding, now)?;
        (relations, basis) = scope.relations()?;
        let entry = relations["relations"].as_array_mut().ok_or_else(|| invalid("invalid relations"))?
            .iter_mut().find(|entry| entry["ref"] == binding.source_ref).ok_or_else(|| invalid("Day source relation missing after publication"))?;
        if entry.get("temporal").is_some() { return Err(conflict("existing source already has a different temporal identity")); }
        entry["temporal"] = json!({"schema":"central.human-day/v1","day_ref":day_ref,"civil_date":time.civil_date,
            "timezone":time.policy.timezone,"created_at_unix_seconds":now,"lifecycle":"open",
            "time_policy_ref":time.source_ref,"time_policy_revision":time.revision,"project_day_refs":[],
            "template_fidelity":"native-blank-text-not-an-original-HTML-fixture","preexisting_bytes_retained":had_bytes});
    }
    let old_date = relations["temporal"]["today"]["civil_date"].as_str().unwrap_or("");
    // A clock correction must not silently replace a later today pointer.
    let advanced = time.civil_date.as_str() > old_date;
    if advanced {
        if !relations["temporal"].is_object() { relations["temporal"] = json!({}); }
        relations["temporal"]["today"] = json!({"day_ref":day_ref,"civil_date":time.civil_date,"updated_at_unix_seconds":now});
    }
    let entry = day_relation(&relations, &day_ref)?.clone();
    save_relations(scope, &relations, &basis)?;
    let (relations, basis) = scope.relations()?;
    scope.reconcile(Some((&principal.principal_ref, &principal.actor_kind, None)), &[text(&entry, "ref")?.into()])?;
    let mut result = reading(scope, &relations, &basis, &entry)?;
    result["created"] = json!(created);
    result["today_advanced"] = json!(advanced);
    result["prior_writing_closed"] = json!(false);
    result["open_editor_changed"] = json!(false);
    result["tasks_carried_or_ticked"] = json!(false);
    result["now_cleared_or_archived"] = json!(false);
    Ok(result)
}
pub fn day_lifecycle(scope: &Scope, input: &Value, principal: &Principal, now: u64) -> io::Result<Value> {
    principal.require_human()?;
    let (mut relations, basis) = scope.relations()?;
    if basis != text(input, "expected_relations_revision")? { return Err(conflict("Day metadata changed")); }
    let reference = text(input, "day_ref")?;
    let entry = day_relation(&relations, reference)?;
    let current = scope.read(text(entry, "ref")?)?;
    if current.revision.revision != text(input, "expected_revision")? { return Err(conflict("Day source changed")); }
    let state = text(input, "lifecycle")?;
    if !matches!(state, "open" | "closed") { return Err(invalid("Day lifecycle is open or closed; closing does not archive/delete source")); }
    let entry = relations["relations"].as_array_mut().ok_or_else(|| invalid("invalid relations"))?
        .iter_mut().find(|entry| entry["temporal"]["day_ref"] == reference).ok_or_else(|| invalid("Day relation disappeared"))?;
    entry["temporal"]["lifecycle"] = json!(state);
    entry["temporal"]["lifecycle_at_unix_seconds"] = json!(now);
    entry["temporal"]["lifecycle_actor_ref"] = json!(principal.principal_ref);
    save_relations(scope, &relations, &basis)?;
    day_read(scope, input)
}
fn outstanding_returns(scope: &Scope, now_ref: &str, source_ref: &str) -> io::Result<Vec<String>> {
    let mut pending = Vec::new();
    for directory in [".central/source-returns", ".central/source-returns/contributions"] {
        let entries = match fs::read_dir(scope.root.join(directory)) {
            Ok(entries) => entries,
            Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e),
        };
        for entry in entries {
            let entry = entry?;
            if entry.path().extension().and_then(|e| e.to_str()) != Some("json") { continue; }
            let path = format!("{directory}/{}", entry.file_name().to_string_lossy());
            let raw = crate::source_safety::read(&scope.root, &path)?;
            let item: Value = serde_json::from_str(&raw)?;
            if (item["now_ref"] == now_ref || item["source_ref"] == source_ref)
                && !matches!(item["status"].as_str(), Some("accepted" | "included" | "rejected" | "cancelled")) {
                pending.push(item["return_ref"].as_str().unwrap_or(&path).into());
            }
        }
    }
    Ok(pending)
}
pub fn now_lifecycle(scope: &Scope, input: &Value, principal: &Principal, now: u64) -> io::Result<Value> {
    placement::checked_policy(scope, input, now)?;
    let (mut record, current) = placement::read_now(scope, text(input, "now_ref")?)?;
    if current.revision.revision != text(input, "expected_revision")? { return Err(conflict("NOW source changed")); }
    let next = text(input, "lifecycle")?;
    if !matches!(next, "active" | "quiescent" | "closed" | "archived") { return Err(invalid("NOW lifecycle is active, quiescent, closed or archived")); }
    if next == "archived" {
        if record.lifecycle != "closed" { return Err(denied("NOW must be explicitly closed before archive; Day rollover is not closure")); }
        let pending = outstanding_returns(scope, &record.now_ref, &record.source_ref)?;
        if !pending.is_empty() { return Err(conflict(format!("NOW has outstanding receiving obligations: {}", pending.join(", ")))); }
        for reference in &record.obligations {
            let obligation = scope.read(reference)?;
            let value: Value = serde_json::from_str(&obligation.content)?;
            if value["schema"] != "central.now-obligation/v1" || value["now_ref"] != record.now_ref
                || !matches!(value["status"].as_str(), Some("settled" | "cancelled"))
                || value["evidence_refs"].as_array().is_none_or(|v| v.is_empty()) {
                return Err(conflict(format!("NOW obligation is not settled with retained evidence: {reference}")));
            }
            for evidence in value["evidence_refs"].as_array().ok_or_else(|| invalid("invalid obligation evidence"))? {
                let evidence: placement::SourceBasis = serde_json::from_value(evidence.clone())?;
                if scope.read(&evidence.source_ref)?.revision.revision != evidence.revision { return Err(conflict("NOW settlement evidence is stale")); }
            }
        }
        record.archive_ref = Some(format!("central:archive:{}:{}", record.now_ref, current.revision.revision));
    }
    if next == "active" && record.lifecycle != "active" {
        // Fresh owner policy and a real existing destination are required even
        // when the underlying NOW identity has survived a session or archive.
        let destination = placement::now_destination(scope, &current.source.path)?;
        crate::file_mutation::directory(&scope.root, destination.strip_prefix(&scope.root).map_err(io::Error::other)?)?;
    }
    record.lifecycle = next.into();
    let changed = history::replace(scope, &current, &encoded(&record)?, &principal.principal_ref, &principal.actor_kind, now)?;
    Ok(json!({"schema":"central.now-lifecycle/v1","record":record,"source":changed.source,"revision":changed.revision,
        "archive_source_bytes_retained":true,"artifacts_deleted":false,"processes_stopped":false,
        "scope_of_settlement":"recorded native obligations and receiving ledger, not an inferred global process census",
        "automatic_agent_or_model_invocation":false}))
}
pub fn now_obligations(scope: &Scope, input: &Value, principal: &Principal, now: u64) -> io::Result<Value> {
    let (mut record, current) = placement::read_now(scope, text(input, "now_ref")?)?;
    if current.revision.revision != text(input, "expected_revision")? { return Err(conflict("NOW source changed")); }
    if record.lifecycle == "archived" { return Err(denied("re-enter NOW explicitly before adding obligations")); }
    let values: Vec<String> = serde_json::from_value(input.get("obligation_refs").cloned().ok_or_else(|| invalid("obligation_refs required"))?)?;
    if values.len() > 256 { return Err(invalid("at most 256 obligation refs per operation")); }
    for reference in values {
        let obligation = scope.read(&reference)?;
        let value: Value = serde_json::from_str(&obligation.content)?;
        if value["schema"] != "central.now-obligation/v1" || value["now_ref"] != record.now_ref { return Err(invalid("obligation does not belong to this NOW")); }
        if !record.obligations.contains(&reference) { record.obligations.push(reference); }
    }
    let result = history::replace(scope, &current, &encoded(&record)?, &principal.principal_ref, &principal.actor_kind, now)?;
    Ok(json!({"record":record,"source":result.source,"revision":result.revision,"obligations_removed":false}))
}
