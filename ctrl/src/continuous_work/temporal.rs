use super::{
    authority::{self, Principal},
    history, placement,
    source::{self, conflict, denied, encoded, invalid, text, Scope},
};
use chrono::{DateTime, Datelike, Duration, Offset, TimeZone, Timelike, Utc};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
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
    if policy.schema != "central.civil-time-policy/v1"
        || policy.scope_ref != root.world_ref
        || policy.day_boundary_minutes >= 1440
    {
        return Err(invalid(
            "invalid root civil-time policy; no harness-local fallback",
        ));
    }
    let timezone: Tz = policy
        .timezone
        .parse()
        .map_err(|_| invalid("civil-time policy requires an IANA timezone"))?;
    let seconds: i64 = now
        .try_into()
        .map_err(|_| invalid("time outside supported range"))?;
    let local = DateTime::<Utc>::from_timestamp(seconds, 0)
        .ok_or_else(|| invalid("time outside supported range"))?
        .with_timezone(&timezone);
    // Boundary is a civil-wall-time cut, not an assumed 24-hour UTC duration.
    let civil = local
        .naive_local()
        .checked_sub_signed(Duration::minutes(policy.day_boundary_minutes as i64))
        .ok_or_else(|| invalid("civil date underflow"))?
        .date()
        .to_string();
    Ok(TimeReading {
        schema: "central.civil-time-reading/v1".into(),
        source_ref: reading.source.source_ref,
        source_revision: reading.revision.revision,
        revision: basis,
        policy,
        civil_date: civil,
        local_time: local.to_rfc3339(),
        observed_at_unix_seconds: now,
    })
}
/// Deterministic occurrence resolution for scheduled automations, over the
/// recognised root civil-time policy.
///
/// Central owns all calendar meaning: the schedule is resolved in the policy
/// timezone, a spring-forward nonexistent local time resolves forward by the
/// transition gap and is named in the response, and an autumn-fold ambiguous
/// local time yields two distinct instants (and therefore two distinct
/// occurrence refs). The action resolves instants only — it never advances
/// "today", never implies Day closure and never invokes the Day lifecycle.
///
/// An `occurrence_ref` is a pure function of (schedule, instant). It is stable
/// across windows — so a delivered occurrence stays delivered when the caller's
/// horizon slides — and across policy edits — so a policy re-baseline changes
/// the reading basis without changing occurrence identity. The response is
/// deterministic given (policy revision, schedule, window).
pub fn time_occurrences(scope: &Scope, input: &Value) -> io::Result<Value> {
    const OCCURRENCES_SCHEMA: &str = "central.time-occurrences/v1";
    const MAX_OCCURRENCES: usize = 4096;
    let root = Scope::resolve(&scope.central_root, None)?;
    let (reading, _, basis) = authority::recognised_source(&root, "civil-time-policy")?;
    let policy: TimePolicy = serde_json::from_str(&reading.content)?;
    if policy.schema != "central.civil-time-policy/v1"
        || policy.scope_ref != root.world_ref
        || policy.day_boundary_minutes >= 1440
    {
        return Err(invalid(
            "invalid root civil-time policy; no harness-local fallback",
        ));
    }
    let timezone: Tz = policy
        .timezone
        .parse()
        .map_err(|_| invalid("civil-time policy requires an IANA timezone"))?;
    let schedule = input
        .get("schedule")
        .ok_or_else(|| invalid("schedule required"))?;
    if !schedule.is_object() {
        return Err(invalid("schedule must be an object"));
    }
    let window_from = window_ms(input, "window_from_unix_ms")?;
    let window_to = window_ms(input, "window_to_unix_ms")?;
    if window_to < window_from {
        return Err(invalid("window_to_unix_ms precedes window_from_unix_ms"));
    }
    validate_schedule(schedule)?;

    let mut resolved = Vec::new();
    let mut named = Vec::new();
    match schedule["kind"].as_str().unwrap_or_default() {
        "daily" => {
            let (hour, minute) = parse_hh_mm(schedule["time"].as_str().unwrap_or_default())?;
            let start = DateTime::<Utc>::from_timestamp_millis(window_from)
                .ok_or_else(|| invalid("window_from_unix_ms outside the supported range"))?
                .with_timezone(&timezone)
                .date_naive();
            let end = DateTime::<Utc>::from_timestamp_millis(window_to)
                .ok_or_else(|| invalid("window_to_unix_ms outside the supported range"))?
                .with_timezone(&timezone)
                .date_naive();
            let mut date = start;
            while date <= end {
                let naive = date
                    .and_hms_opt(hour, minute, 0)
                    .ok_or_else(|| invalid("daily time does not exist on this calendar"))?;
                match timezone.from_local_datetime(&naive) {
                    chrono::LocalResult::Single(local) => resolved.push(local.with_timezone(&Utc)),
                    chrono::LocalResult::Ambiguous(earliest, latest) => {
                        resolved.push(earliest.with_timezone(&Utc));
                        resolved.push(latest.with_timezone(&Utc));
                    }
                    chrono::LocalResult::None => {
                        // Spring-forward gap: the requested local time does not
                        // exist. Policy rule: resolve forward by the transition
                        // gap and say so, never silently skip or drop.
                        let gap_seconds = transition_gap(&timezone, &naive)?;
                        let shifted = naive + Duration::seconds(gap_seconds);
                        let local = match timezone.from_local_datetime(&shifted) {
                            chrono::LocalResult::Single(local) => local,
                            _ => {
                                return Err(invalid(
                                    "spring-forward gap rule did not resolve to one instant",
                                ))
                            }
                        };
                        named.push(json!({
                            "requested_local_time": naive.to_string(),
                            "resolved_due_unix_ms": local.with_timezone(&Utc).timestamp_millis(),
                            "rule": "spring-forward-nonexistent-local-time-resolved-forward-by-gap",
                        }));
                        resolved.push(local.with_timezone(&Utc));
                    }
                }
                date += Duration::days(1);
            }
        }
        "cron" => {
            let expression = schedule["expression"].as_str().unwrap_or_default();
            let matcher = CronExpression::parse(expression)?;
            // Walk UTC minutes and match each instant's local wall time. A
            // nonexistent local time in a spring-forward gap never occurs (no
            // UTC instant maps into the gap); an autumn-fold local hour occurs
            // twice and yields two distinct instants/refs.
            let first_minute = ceil_div(window_from, 60_000) * 60_000;
            let mut minute = first_minute;
            while minute <= window_to {
                let utc = DateTime::<Utc>::from_timestamp_millis(minute)
                    .ok_or_else(|| invalid("window outside the supported range"))?;
                let local = utc.with_timezone(&timezone).naive_local();
                if matcher.matches(&local) {
                    resolved.push(utc);
                }
                minute += 60_000;
            }
        }
        "every" => {
            let interval_ms = schedule["interval_ms"].as_u64().unwrap_or(0);
            if interval_ms == 0 {
                return Err(invalid("every interval_ms must be a positive integer"));
            }
            // Anchored at Unix-epoch multiples so overlapping windows agree on
            // the same instants — a slid horizon never mints a second series.
            let mut instant_ms = ceil_div(window_from, interval_ms as i64) * interval_ms as i64;
            while instant_ms <= window_to {
                resolved.push(
                    DateTime::<Utc>::from_timestamp_millis(instant_ms)
                        .ok_or_else(|| invalid("resolved instant outside the supported range"))?,
                );
                instant_ms += interval_ms as i64;
            }
        }
        "once" => {
            let instant_ms = if let Some(due) = schedule.get("due_unix_ms") {
                due.as_i64()
                    .ok_or_else(|| invalid("once due_unix_ms must be an integer"))?
            } else {
                let rfc3339 = schedule["rfc3339"].as_str().unwrap_or_default();
                DateTime::parse_from_rfc3339(rfc3339)
                    .map_err(|_| invalid("once rfc3339 is not an RFC 3339 timestamp"))?
                    .with_timezone(&Utc)
                    .timestamp_millis()
            };
            if instant_ms >= window_from && instant_ms <= window_to {
                resolved.push(
                    DateTime::<Utc>::from_timestamp_millis(instant_ms)
                        .ok_or_else(|| invalid("resolved instant outside the supported range"))?,
                );
            }
        }
        _ => unreachable!("validate_schedule rejects unknown kinds"),
    }

    resolved.sort();
    resolved.dedup();
    // The window bounds the resolved instants themselves: a local calendar date
    // that begins inside the horizon but resolves outside it is not an
    // occurrence of this window.
    resolved.retain(|instant| {
        let due = instant.timestamp_millis();
        due >= window_from && due <= window_to
    });
    if resolved.len() > MAX_OCCURRENCES {
        return Err(invalid(format!(
            "window resolves more than {MAX_OCCURRENCES} occurrences; narrow the window"
        )));
    }
    let occurrences: Vec<Value> = resolved
        .iter()
        .map(|instant| {
            let due_unix_ms = instant.timestamp_millis();
            json!({
                "occurrence_ref": occurrence_ref(schedule, due_unix_ms),
                "due_unix_ms": due_unix_ms,
            })
        })
        .collect();
    let mut result = json!({
        "schema": OCCURRENCES_SCHEMA,
        "time_policy_ref": reading.source.source_ref,
        "time_policy_revision": basis,
        "timezone": policy.timezone,
        "day_boundary_minutes": policy.day_boundary_minutes,
        "occurrences": occurrences,
    });
    if !named.is_empty() {
        result["named_resolutions"] = json!(named);
    }
    Ok(result)
}

fn window_ms(input: &Value, field: &str) -> io::Result<i64> {
    input
        .get(field)
        .and_then(Value::as_i64)
        .ok_or_else(|| invalid(format!("{field} must be an integer")))
}

fn ceil_div(value: i64, divisor: i64) -> i64 {
    value.div_euclid(divisor) + i64::from(value.rem_euclid(divisor) != 0)
}

fn parse_hh_mm(raw: &str) -> io::Result<(u32, u32)> {
    let (hour, minute) = raw
        .split_once(':')
        .ok_or_else(|| invalid("daily time must be HH:MM"))?;
    let hour: u32 = hour
        .parse()
        .map_err(|_| invalid("daily hour must be an integer"))?;
    let minute: u32 = minute
        .parse()
        .map_err(|_| invalid("daily minute must be an integer"))?;
    if hour > 23 || minute > 59 {
        return Err(invalid("daily time must be a valid HH:MM wall time"));
    }
    Ok((hour, minute))
}

fn validate_schedule(schedule: &Value) -> io::Result<()> {
    let kind = schedule["kind"].as_str().unwrap_or_default();
    match kind {
        "daily" => {
            parse_hh_mm(schedule["time"].as_str().unwrap_or_default())?;
        }
        "cron" => {
            CronExpression::parse(schedule["expression"].as_str().unwrap_or_default())?;
        }
        "every" => {
            let interval = schedule["interval_ms"].as_u64().unwrap_or(0);
            if interval == 0 {
                return Err(invalid("every interval_ms must be a positive integer"));
            }
        }
        "once" => {
            let due = schedule.get("due_unix_ms");
            let rfc3339 = schedule.get("rfc3339");
            match (due, rfc3339) {
                (Some(value), None) => {
                    if value.as_i64().is_none() {
                        return Err(invalid("once due_unix_ms must be an integer"));
                    }
                }
                (None, Some(value)) => {
                    let raw = value.as_str().unwrap_or_default();
                    DateTime::parse_from_rfc3339(raw)
                        .map_err(|_| invalid("once rfc3339 is not an RFC 3339 timestamp"))?;
                }
                _ => {
                    return Err(invalid(
                        "once requires exactly one of due_unix_ms or rfc3339",
                    ))
                }
            }
        }
        other => {
            return Err(invalid(format!(
                "schedule kind must be daily, cron, every or once, not `{other}`"
            )))
        }
    }
    Ok(())
}

/// Offset jump across the DST transition enclosing `naive`, in seconds.
/// Probes both wall-clock sides; a real transition separates them.
fn transition_gap(timezone: &Tz, naive: &chrono::NaiveDateTime) -> io::Result<i64> {
    let before = *naive - Duration::hours(2);
    let after = *naive + Duration::hours(2);
    let before_offset = match timezone.from_local_datetime(&before) {
        chrono::LocalResult::Single(local) => local.offset().fix().local_minus_utc(),
        _ => return Err(invalid("cannot locate the spring-forward transition")),
    };
    let after_offset = match timezone.from_local_datetime(&after) {
        chrono::LocalResult::Single(local) => local.offset().fix().local_minus_utc(),
        _ => return Err(invalid("cannot locate the spring-forward transition")),
    };
    Ok((after_offset - before_offset) as i64)
}

/// Stable occurrence identity: a pure function of the schedule and the instant,
/// never of the caller's window or the current policy revision — a slid horizon
/// or a policy re-baseline must not mint a second identity for one occurrence.
/// Two instants in a DST fold differ in `due_unix_ms` and therefore stay distinct.
fn occurrence_ref(schedule: &Value, due_unix_ms: i64) -> String {
    let canonical = serde_json::to_string(&json!({
        "schema": "central.time-occurrence-ref/v1",
        "schedule": schedule,
        "due_unix_ms": due_unix_ms,
    }))
    .unwrap_or_default();
    let digest = Sha256::digest(canonical.as_bytes());
    format!("central:occurrence/{digest:x}")
}

/// One parsed 5-field cron expression over local wall time.
struct CronExpression {
    minute: CronField,
    hour: CronField,
    day_of_month: CronField,
    month: CronField,
    day_of_week: CronField,
}

impl CronExpression {
    fn parse(raw: &str) -> io::Result<Self> {
        let fields: Vec<&str> = raw.split_whitespace().collect();
        if fields.len() != 5 {
            return Err(invalid(
                "cron expression must have exactly 5 fields: minute hour day-of-month month day-of-week",
            ));
        }
        Ok(Self {
            minute: CronField::parse(fields[0], 0, 59, false)?,
            hour: CronField::parse(fields[1], 0, 23, false)?,
            day_of_month: CronField::parse(fields[2], 1, 31, false)?,
            month: CronField::parse(fields[3], 1, 12, false)?,
            day_of_week: CronField::parse(fields[4], 0, 7, true)?,
        })
    }

    fn matches(&self, local: &chrono::NaiveDateTime) -> bool {
        let month = local.month();
        let day_of_month = local.day();
        let day_of_week = local.weekday().num_days_from_sunday();
        let day_matches = match (
            self.day_of_month == CronField::Any,
            self.day_of_week == CronField::Any,
        ) {
            // Vixie semantics: when both day fields are restricted, either may
            // fire; when one is `*`, only the restricted one speaks.
            (false, false) => {
                self.day_of_month.contains(day_of_month) || self.day_of_week.contains(day_of_week)
            }
            (false, true) => self.day_of_month.contains(day_of_month),
            (true, false) => self.day_of_week.contains(day_of_week),
            (true, true) => true,
        };
        self.minute.contains(local.minute())
            && self.hour.contains(local.hour())
            && self.month.contains(month)
            && day_matches
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CronField {
    Any,
    Set(u128),
}

impl CronField {
    fn parse(raw: &str, min: u32, max: u32, wrap_sunday: bool) -> io::Result<Self> {
        let mut set: u128 = 0;
        for part in raw.split(',') {
            let (range, step) = match part.split_once('/') {
                Some((range, step)) => (range, step),
                None => (part, ""),
            };
            let step: u32 = match step {
                "" => 1,
                _ => step
                    .parse()
                    .map_err(|_| invalid(format!("cron step `{step}` is not an integer")))?,
            };
            if step == 0 {
                return Err(invalid("cron step must be positive"));
            }
            let (low, high) = if range == "*" {
                (min, max)
            } else if let Some((start, end)) = range.split_once('-') {
                let start: u32 = start
                    .parse()
                    .map_err(|_| invalid(format!("cron value `{start}` is not an integer")))?;
                let end: u32 = end
                    .parse()
                    .map_err(|_| invalid(format!("cron value `{end}` is not an integer")))?;
                (start, end)
            } else {
                let value: u32 = range
                    .parse()
                    .map_err(|_| invalid(format!("cron value `{range}` is not an integer")))?;
                (value, value)
            };
            if low < min || high > max || low > high {
                return Err(invalid(format!(
                    "cron range {low}-{high} is outside {min}-{max}"
                )));
            }
            let mut value = low;
            while value <= high {
                set |= 1u128 << value;
                value += step;
            }
        }
        if set == 0 {
            return Err(invalid("cron field selects no values"));
        }
        // 0 and 7 both name Sunday; fold 7 onto 0 after range expansion.
        if wrap_sunday && set & (1u128 << 7) != 0 {
            set = (set & !(1u128 << 7)) | 1;
        }
        // A field that selects every value in range is the `*` case for the
        // day-field semantics.
        let full: u128 = (min..=max)
            .map(|value| 1u128 << value)
            .fold(0, |a, b| a | b);
        if set == full {
            Ok(CronField::Any)
        } else {
            Ok(CronField::Set(set))
        }
    }

    fn contains(&self, value: u32) -> bool {
        match self {
            CronField::Any => true,
            CronField::Set(set) => set & (1u128 << value) != 0,
        }
    }
}

pub(crate) fn save_relations(scope: &Scope, relations: &Value, basis: &str) -> io::Result<()> {
    let content = encoded(relations)?;
    if basis == "absent" {
        source::put_new(&scope.root, &scope.relations_path, &content)?;
    } else {
        crate::source_safety::replace(&scope.root, &scope.relations_path, basis, &content)?;
    }
    Ok(())
}
fn day_relation<'a>(relations: &'a Value, day_ref: &str) -> io::Result<&'a Value> {
    relations["relations"]
        .as_array()
        .ok_or_else(|| invalid("invalid relations"))?
        .iter()
        .find(|entry| entry["temporal"]["day_ref"] == day_ref)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "DayRef is not recorded in this World",
            )
        })
}
fn reading(
    scope: &Scope,
    relations: &Value,
    relation_basis: &str,
    entry: &Value,
) -> io::Result<Value> {
    let source = scope.read(text(entry, "ref")?)?;
    // The temporal carrier is not automatically a Daily Die. Only the native
    // document relation selects a document; no filename/template search and
    // no catch-and-empty downgrade of a broken/denied document reading.
    let document = if let Some(meta) = entry.get("document") {
        if meta["kind"] != "day" {
            return Err(invalid("human Day source is bound to a non-Day document"));
        }
        let value = super::documents::read(
            scope,
            &json!({
                "source_ref": source.source.source_ref, "document_id": text(meta, "document_id")?
            }),
        )?;
        if value["source"]["ref"] != source.source.source_ref
            || value["revision"]["revision"] != source.revision.revision
            || value["document"]["day_ref"] != entry["temporal"]["day_ref"]
        {
            return Err(conflict(
                "Day/document binding or revision changed during resolution",
            ));
        }
        value
    } else {
        Value::Null
    };
    Ok(
        json!({"schema":"central.day-reading/v1","day_ref":entry["temporal"]["day_ref"],"source":source.source,"revision":source.revision,"content":source.content,"temporal":entry["temporal"],"relations_revision":relation_basis,"today":relations["temporal"]["today"],"document_state":if document.is_null(){"uninitialised"}else{"ready"},"document":document,"automatic_agent_or_model_invocation":false}),
    )
}
pub fn day_read(scope: &Scope, input: &Value) -> io::Result<Value> {
    let (relations, basis) = scope.relations()?;
    let reference = input
        .get("day_ref")
        .and_then(Value::as_str)
        .or_else(|| relations["temporal"]["today"]["day_ref"].as_str())
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "no DayRef or current today pointer",
            )
        })?;
    let entry = day_relation(&relations, reference)?;
    reading(scope, &relations, &basis, entry)
}
pub fn ensure_day(
    scope: &Scope,
    input: &Value,
    principal: &Principal,
    now: u64,
) -> io::Result<Value> {
    let time = time_policy(scope, now)?;
    if text(input, "expected_time_policy_revision")? != time.revision {
        return Err(conflict("civil-time policy changed before Day allocation"));
    }
    if !principal.is_human() && !time.policy.automatic_day_rollover {
        return Err(denied(
            "blank Day rollover is not authorised by the human's current time policy",
        ));
    }
    let day_ref = format!("central:day:{}:{}", scope.world_ref, time.civil_date);
    let (mut relations, mut basis) = scope.relations()?;
    let mut created = false;
    if day_relation(&relations, &day_ref).is_err() {
        // Native blank text, not an invented original HTML-template payload.
        let path = format!("{}/{}/day.md", scope.day_dir(), time.civil_date);
        let had_bytes = match crate::source_safety::read(&scope.root, &path) {
            Ok(_) => true,
            Err(e) if e.kind() == io::ErrorKind::NotFound => false,
            Err(e) => return Err(e),
        };
        if !had_bytes {
            source::put_new(&scope.root, &path, "")?;
            created = true;
        }
        let binding = crate::source_horizon::SourceBinding {
            source_ref: scope.source_ref(&path),
            path,
            roles: vec![
                "human-day".into(),
                if scope.project.is_some() {
                    "project-human-source-aperture".into()
                } else {
                    "personal-human-source-aperture".into()
                },
            ],
            provenance: "observed".into(),
            standing: "current-development-state".into(),
            treatment: "projectcentral-user".into(),
            agent_retrieval_allowed: true,
        };
        scope.bind(&binding, now)?;
        (relations, basis) = scope.relations()?;
        let entry = relations["relations"]
            .as_array_mut()
            .ok_or_else(|| invalid("invalid relations"))?
            .iter_mut()
            .find(|entry| entry["ref"] == binding.source_ref)
            .ok_or_else(|| invalid("Day source relation missing after publication"))?;
        if entry.get("temporal").is_some() {
            return Err(conflict(
                "existing source already has a different temporal identity",
            ));
        }
        entry["temporal"] = json!({"schema":"central.human-day/v1","day_ref":day_ref,"civil_date":time.civil_date,
            "timezone":time.policy.timezone,"created_at_unix_seconds":now,"lifecycle":"open",
            "time_policy_ref":time.source_ref,"time_policy_revision":time.revision,"project_day_refs":[],
            "template_fidelity":"native-blank-text-not-an-original-HTML-fixture","preexisting_bytes_retained":had_bytes});
    }
    let old_date = relations["temporal"]["today"]["civil_date"]
        .as_str()
        .unwrap_or("");
    let advanced = time.civil_date.as_str() > old_date;
    if advanced {
        if !relations["temporal"].is_object() {
            relations["temporal"] = json!({});
        }
        relations["temporal"]["today"] =
            json!({"day_ref":day_ref,"civil_date":time.civil_date,"updated_at_unix_seconds":now});
    }
    let entry = day_relation(&relations, &day_ref)?.clone();
    save_relations(scope, &relations, &basis)?;
    let (relations, basis) = scope.relations()?;
    scope.reconcile(
        Some((&principal.principal_ref, &principal.actor_kind, None)),
        &[text(&entry, "ref")?.into()],
    )?;
    let mut result = reading(scope, &relations, &basis, &entry)?;
    result["created"] = json!(created);
    result["today_advanced"] = json!(advanced);
    result["prior_writing_closed"] = json!(false);
    result["open_editor_changed"] = json!(false);
    result["tasks_carried_or_ticked"] = json!(false);
    result["now_cleared_or_archived"] = json!(false);
    // The live NOW horizon crosses the Day boundary untouched: active Workcell
    // root and child clearings carry, quiescent ones are reported released.
    result["now_horizon"] = placement::horizon_reading(scope)
        .unwrap_or_else(|error| json!({"state":"unavailable","reason":error.to_string()}));
    Ok(result)
}
pub fn day_lifecycle(
    scope: &Scope,
    input: &Value,
    principal: &Principal,
    now: u64,
) -> io::Result<Value> {
    principal.require_human()?;
    let (mut relations, basis) = scope.relations()?;
    if basis != text(input, "expected_relations_revision")? {
        return Err(conflict("Day metadata changed"));
    }
    let reference = text(input, "day_ref")?;
    let entry = day_relation(&relations, reference)?;
    let current = scope.read(text(entry, "ref")?)?;
    if current.revision.revision != text(input, "expected_revision")? {
        return Err(conflict("Day source changed"));
    }
    let state = text(input, "lifecycle")?;
    if !matches!(state, "open" | "closed") {
        return Err(invalid(
            "Day lifecycle is open or closed; closing does not archive/delete source",
        ));
    }
    let entry = relations["relations"]
        .as_array_mut()
        .ok_or_else(|| invalid("invalid relations"))?
        .iter_mut()
        .find(|entry| entry["temporal"]["day_ref"] == reference)
        .ok_or_else(|| invalid("Day relation disappeared"))?;
    entry["temporal"]["lifecycle"] = json!(state);
    entry["temporal"]["lifecycle_at_unix_seconds"] = json!(now);
    entry["temporal"]["lifecycle_actor_ref"] = json!(principal.principal_ref);
    save_relations(scope, &relations, &basis)?;
    day_read(scope, input)
}
/// Every receiving Return item keyed to this NOW/source, paired with its
/// on-disk path (the return_ref's fallback identity). One scan feeds both the
/// archive-gate obligation check (`outstanding_returns`) and the read-model
/// composition (`composed_returns`), so the two can never disagree about what
/// a NOW is answerable for.
fn scan_returns(
    scope: &Scope,
    now_ref: &str,
    source_ref: &str,
) -> io::Result<Vec<(String, Value)>> {
    let mut items = Vec::new();
    for directory in [
        ".central/source-returns",
        ".central/source-returns/contributions",
    ] {
        let entries = match fs::read_dir(scope.root.join(directory)) {
            Ok(entries) => entries,
            Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e),
        };
        for entry in entries {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().into_owned();
            // The receiving cursor is bookkeeping, not a Return. Other native
            // subdirectories hold document mutation intents, not ledger entries.
            if name == "cursor.json"
                || entry.path().extension().and_then(|e| e.to_str()) != Some("json")
            {
                continue;
            }
            let path = format!("{directory}/{name}");
            let raw = crate::source_safety::read(&scope.root, &path)?;
            let item: Value = serde_json::from_str(&raw)?;
            if item["now_ref"] != now_ref && item["source_ref"] != source_ref {
                continue;
            }
            items.push((path, item));
        }
    }
    Ok(items)
}

// Legacy source-return acceptance already committed its source effect. New
// receiving acceptance explicitly has NOT included it.
fn return_is_settled(item: &Value) -> bool {
    matches!(
        item["status"].as_str(),
        Some("included" | "rejected" | "cancelled")
    ) || (item["schema"] == "central.source-return/v1" && item["status"] == "accepted")
}

fn outstanding_returns(scope: &Scope, now_ref: &str, source_ref: &str) -> io::Result<Vec<String>> {
    let mut pending: Vec<String> = scan_returns(scope, now_ref, source_ref)?
        .into_iter()
        .filter(|(_, item)| !return_is_settled(item))
        .map(|(path, item)| item["return_ref"].as_str().unwrap_or(&path).into())
        .collect();
    pending.sort();
    pending.dedup();
    Ok(pending)
}

/// The read-model join a NOW reading composes over its own consequential
/// output: every receiving Return keyed to this NOW/source, with its status
/// and the run/session/day/task refs the producer already stamped. This is a
/// read over the existing receiving ledger — it allocates nothing, mutates
/// nothing and introduces no new store. It answers FACTORY-AGENCY §11 ("NOW
/// composes … consequential output refs") and SESSION-GROUNDING §10: a NOW
/// reading now discloses what actually happened beneath it, not only the
/// frozen basis it was allocated with. `settled` distinguishes a resolved
/// Return from one still awaiting review, so a reader never mistakes a pending
/// outcome for a closed one.
pub(crate) fn composed_returns(
    scope: &Scope,
    now_ref: &str,
    source_ref: &str,
) -> io::Result<Vec<Value>> {
    let mut rows: Vec<Value> = scan_returns(scope, now_ref, source_ref)?
        .into_iter()
        .map(|(path, item)| {
            let return_ref = item["return_ref"].as_str().unwrap_or(&path).to_string();
            json!({
                "return_ref": return_ref,
                "status": item["status"].as_str().unwrap_or("unknown"),
                "settled": return_is_settled(&item),
                "schema": item["schema"].as_str(),
                "run_ref": item.get("run_ref").cloned().unwrap_or(Value::Null),
                "session_ref": item.get("session_ref").cloned().unwrap_or(Value::Null),
                "day_ref": item.get("day_ref").cloned().unwrap_or(Value::Null),
                "task_ref": item.get("task_ref").cloned().unwrap_or(Value::Null),
                "source_ref": item.get("source_ref").cloned().unwrap_or(Value::Null),
            })
        })
        .collect();
    rows.sort_by(|a, b| {
        a["return_ref"]
            .as_str()
            .unwrap_or_default()
            .cmp(b["return_ref"].as_str().unwrap_or_default())
    });
    rows.dedup_by(|a, b| a["return_ref"] == b["return_ref"]);
    Ok(rows)
}
pub fn now_lifecycle(
    scope: &Scope,
    input: &Value,
    principal: &Principal,
    now: u64,
) -> io::Result<Value> {
    placement::checked_policy(scope, input, now)?;
    let (mut record, current) = placement::read_now(scope, text(input, "now_ref")?)?;
    if current.revision.revision != text(input, "expected_revision")? {
        return Err(conflict("NOW source changed"));
    }
    let next = text(input, "lifecycle")?;
    if !matches!(next, "active" | "quiescent" | "closed" | "archived") {
        return Err(invalid(
            "NOW lifecycle is active, quiescent, closed or archived",
        ));
    }
    if next == "archived" {
        if record.lifecycle != "closed" {
            return Err(denied(
                "NOW must be explicitly closed before archive; Day rollover is not closure",
            ));
        }
        let pending = outstanding_returns(scope, &record.now_ref, &record.source_ref)?;
        if !pending.is_empty() {
            return Err(conflict(format!(
                "NOW has outstanding receiving obligations: {}",
                pending.join(", ")
            )));
        }
        for reference in &record.obligations {
            let obligation = scope.read(reference)?;
            let value: Value = serde_json::from_str(&obligation.content)?;
            if value["schema"] != "central.now-obligation/v1"
                || value["now_ref"] != record.now_ref
                || !matches!(value["status"].as_str(), Some("settled" | "cancelled"))
                || value["evidence_refs"]
                    .as_array()
                    .is_none_or(|v| v.is_empty())
            {
                return Err(conflict(format!(
                    "NOW obligation is not settled with retained evidence: {reference}"
                )));
            }
            for evidence in value["evidence_refs"]
                .as_array()
                .ok_or_else(|| invalid("invalid obligation evidence"))?
            {
                let evidence: placement::SourceBasis = serde_json::from_value(evidence.clone())?;
                if scope.read(&evidence.source_ref)?.revision.revision != evidence.revision {
                    return Err(conflict("NOW settlement evidence is stale"));
                }
            }
        }
        record.archive_ref = Some(format!(
            "central:archive:{}:{}",
            record.now_ref, current.revision.revision
        ));
    }
    if next == "active" && record.lifecycle != "active" {
        let destination = placement::now_destination(scope, &current.source.path)?;
        crate::file_mutation::directory(
            &scope.root,
            destination
                .strip_prefix(&scope.root)
                .map_err(io::Error::other)?,
        )?;
    }
    record.lifecycle = next.into();
    let changed = history::replace(
        scope,
        &current,
        &encoded(&record)?,
        &principal.principal_ref,
        &principal.actor_kind,
        now,
    )?;
    Ok(
        json!({"schema":"central.now-lifecycle/v1","record":record,"source":changed.source,"revision":changed.revision,
        "archive_source_bytes_retained":true,"artifacts_deleted":false,"processes_stopped":false,
        "scope_of_settlement":"recorded native obligations and receiving ledger, not an inferred global process census",
        "automatic_agent_or_model_invocation":false}),
    )
}
pub fn now_obligations(
    scope: &Scope,
    input: &Value,
    principal: &Principal,
    now: u64,
) -> io::Result<Value> {
    let (mut record, current) = placement::read_now(scope, text(input, "now_ref")?)?;
    if current.revision.revision != text(input, "expected_revision")? {
        return Err(conflict("NOW source changed"));
    }
    if record.lifecycle == "archived" {
        return Err(denied("re-enter NOW explicitly before adding obligations"));
    }
    let values: Vec<String> = serde_json::from_value(
        input
            .get("obligation_refs")
            .cloned()
            .ok_or_else(|| invalid("obligation_refs required"))?,
    )?;
    if values.len() > 256 {
        return Err(invalid("at most 256 obligation refs per operation"));
    }
    for reference in values {
        let obligation = scope.read(&reference)?;
        let value: Value = serde_json::from_str(&obligation.content)?;
        if value["schema"] != "central.now-obligation/v1" || value["now_ref"] != record.now_ref {
            return Err(invalid("obligation does not belong to this NOW"));
        }
        if !record.obligations.contains(&reference) {
            record.obligations.push(reference);
        }
    }
    let result = history::replace(
        scope,
        &current,
        &encoded(&record)?,
        &principal.principal_ref,
        &principal.actor_kind,
        now,
    )?;
    Ok(
        json!({"record":record,"source":result.source,"revision":result.revision,"obligations_removed":false}),
    )
}

#[cfg(test)]
mod receiving_obligation_tests {
    use super::*;
    #[test]
    fn accepted_contribution_still_blocks_archive_but_cursor_is_not_a_return() {
        let world = super::super::tests::world();
        let scope = Scope::resolve(world.path(), None).unwrap();
        let area = scope.root.join(".central/source-returns/contributions");
        fs::create_dir_all(&area).unwrap();
        fs::write(
            area.join("cursor.json"),
            encoded(&json!({"schema":"central.receiving-cursor/v1","sequence":1})).unwrap(),
        )
        .unwrap();
        assert!(outstanding_returns(&scope, "now:test", "source:test")
            .unwrap()
            .is_empty());
        fs::write(area.join("accepted.json"), encoded(&json!({"schema":"central.received-contribution/v1","return_ref":"return:test","now_ref":"now:test","status":"accepted"})).unwrap()).unwrap();
        assert_eq!(
            outstanding_returns(&scope, "now:test", "source:test").unwrap(),
            vec!["return:test"]
        );
        fs::write(area.join("accepted.json"), encoded(&json!({"schema":"central.received-contribution/v1","return_ref":"return:test","now_ref":"now:test","status":"included"})).unwrap()).unwrap();
        assert!(outstanding_returns(&scope, "now:test", "source:test")
            .unwrap()
            .is_empty());
    }
    #[test]
    fn legacy_applied_acceptance_is_not_confused_with_new_review_acceptance() {
        let world = super::super::tests::world();
        let scope = Scope::resolve(world.path(), None).unwrap();
        let area = scope.root.join(".central/source-returns");
        fs::create_dir_all(&area).unwrap();
        fs::write(area.join("legacy.json"), encoded(&json!({"schema":"central.source-return/v1","return_ref":"legacy:test","source_ref":"source:test","status":"accepted"})).unwrap()).unwrap();
        assert!(outstanding_returns(&scope, "now:test", "source:test")
            .unwrap()
            .is_empty());
    }

    #[test]
    fn composed_returns_discloses_pending_and_settled_output_with_correlation_refs() {
        let world = super::super::tests::world();
        let scope = Scope::resolve(world.path(), None).unwrap();
        let area = scope.root.join(".central/source-returns");
        fs::create_dir_all(&area).unwrap();
        // A Factory Run Return keyed to this NOW, still awaiting review, carrying
        // the run/session/day refs its producer stamped.
        fs::write(
            area.join("run.json"),
            encoded(&json!({
                "schema": "central.received-return/v1",
                "return_ref": "return:run-a",
                "now_ref": "now:test",
                "status": "pending",
                "run_ref": "run:a",
                "session_ref": "ses:a",
                "day_ref": "day:2026-09-19"
            }))
            .unwrap(),
        )
        .unwrap();
        // A second Return, already included (settled), matched by source_ref.
        fs::write(
            area.join("done.json"),
            encoded(&json!({
                "schema": "central.received-return/v1",
                "return_ref": "return:done",
                "source_ref": "source:test",
                "status": "included"
            }))
            .unwrap(),
        )
        .unwrap();
        // A Return for a different NOW must not leak into this reading.
        fs::write(
            area.join("other.json"),
            encoded(&json!({
                "schema": "central.received-return/v1",
                "return_ref": "return:other",
                "now_ref": "now:elsewhere",
                "status": "pending"
            }))
            .unwrap(),
        )
        .unwrap();

        let rows = composed_returns(&scope, "now:test", "source:test").unwrap();
        let refs: Vec<&str> = rows
            .iter()
            .map(|r| r["return_ref"].as_str().unwrap())
            .collect();
        // Both this NOW's Returns compose in (sorted); the other NOW's does not.
        assert_eq!(refs, vec!["return:done", "return:run-a"]);

        let run = rows
            .iter()
            .find(|r| r["return_ref"] == "return:run-a")
            .unwrap();
        assert_eq!(run["status"], "pending");
        assert_eq!(run["settled"], false);
        // Producer correlation refs are surfaced verbatim, not invented.
        assert_eq!(run["run_ref"], "run:a");
        assert_eq!(run["session_ref"], "ses:a");
        assert_eq!(run["day_ref"], "day:2026-09-19");

        let done = rows
            .iter()
            .find(|r| r["return_ref"] == "return:done")
            .unwrap();
        assert_eq!(done["settled"], true);
        // Absent correlation refs are disclosed as null, never fabricated.
        assert_eq!(done["run_ref"], Value::Null);

        // The read-model join and the archive gate agree on what is still
        // outstanding: exactly the one pending Return.
        assert_eq!(
            outstanding_returns(&scope, "now:test", "source:test").unwrap(),
            vec!["return:run-a"]
        );
    }
}
