//! The Day's turn by Central's civil clock, and the receipts laid into a Day (manual §18.2: "No hours are set on the Day.
//! Central keeps the day's bounds. The Day opens by itself and the close runs by itself"; O-I #1005).
//!
//! `central.day.turn` does what the person's adopted civil-time policy determines at this instant, and nothing else: today's Day
//! opens on its civil date (its blank source, its relation, today's pointer); its document is made from the Day form the host
//! offers (`form`, a location Central's file route gives) when it has none, carrying the set of the Day before it (the worlds it
//! held, its rows with the pages that still play, the Day added to its lineage); and an earlier Day still open closes at its turn,
//! its close routine asked to carry face 5's proposal as the person's Day settings keep it (the clock writes no one's words into a
//! Day, so the proposal waits for the person and stays on the closed Day: day_moves.rs `close_at_turn`). It acts as Central's own
//! civil clock and holds no credential: the person's consent is the policy they adopted (`automatic_day_rollover`), and a policy
//! without it is refused in words. Asked again with nothing to do, it writes nothing.
//!
//! `central.day.receipt.record` lays what an act did into the Day of the civil date it happened on: append-only, one line each,
//! never rewritten, kept by its id, and read back with that Day (`central.day.read`'s `receipts`). A late receipt keeps the time it
//! happened.
use super::{
    authority::{self, Principal},
    day_moves, documents, placement,
    source::{self, denied, invalid, Scope},
    temporal::{self, TimeReading},
};
use serde_json::{json, Value};
use std::io;

/// Who the turn acts as: Central's own civil clock, never a person or an agent.
pub(crate) const CIVIL_CLOCK: &str = "central:civil-clock";
const TURN_SCHEMA: &str = "central.day-turn/v1";
const RECEIPT_SCHEMA: &str = "central.day-receipt/v1";
const NOT_AUTOMATIC: &str = "Your civil-time policy keeps the Day from turning by itself (automatic_day_rollover is off): today's Day opens and an earlier Day closes only by your own act";
/// A page that plays has an active NOW whose task names it (centralTrackAdapter.ts `PAGE_TASK`).
const PAGE_TASK: &str = "oi:page:";
const DAY_SET_SCHEMA: &str = "oi.day-set/v1";

/// The Day's turn (`central.day.turn`): see the module's account. Answers today's Day reading with what the turn did.
pub(crate) fn turn(scope: &Scope, input: &Value, now: u64) -> io::Result<Value> {
    let time = temporal::time_policy(scope, now)?;
    if !time.policy.automatic_day_rollover {
        return Err(denied(NOT_AUTOMATIC));
    }
    let clock = authority::civil_clock(scope, &time);
    let today = time.civil_date.clone();
    // Today's Day and the pointer to it, written only when either moves.
    let (mut relations, basis, day_ref, _) = temporal::open_day(scope, &time, &today, now)?;
    let (on_disk, _) = scope.relations()?;
    let opened = temporal::day_relation(&on_disk, &day_ref).is_err();
    let pointer = relations["temporal"]["today"]["civil_date"]
        .as_str()
        .unwrap_or("")
        .to_owned();
    if today.as_str() > pointer.as_str() {
        if !relations["temporal"].is_object() {
            relations["temporal"] = json!({});
        }
        relations["temporal"]["today"] =
            json!({"day_ref": day_ref, "civil_date": today, "updated_at_unix_seconds": now});
        let reached = relations["temporal"]["tomorrow"]["civil_date"]
            .as_str()
            .is_some_and(|date| date <= today.as_str());
        if reached {
            if let Some(temporal) = relations["temporal"].as_object_mut() {
                temporal.remove("tomorrow");
            }
        }
    }
    if relations != on_disk {
        let entry = temporal::day_relation(&relations, &day_ref)?.clone();
        temporal::save_relations(scope, &relations, &basis)?;
        if opened {
            scope.reconcile(
                Some((&clock.principal_ref, &clock.actor_kind, None)),
                &[source::text(&entry, "ref")?.into()],
            )?;
        }
    }
    // Today's document, from the Day form, carrying the set of the Day before it.
    let reading = temporal::day_read(scope, &json!({"day_ref": day_ref}))?;
    let document = if reading["document_state"] == "ready" {
        json!({"state": "stood"})
    } else if !reading["content"].as_str().unwrap_or("").is_empty() {
        json!({"state": "kept", "why": "Today's Day holds writing that is no Day document; Central keeps it as it is and makes none over it"})
    } else {
        match input.get("form").filter(|form| !form.is_null()) {
            None => {
                json!({"state": "wants-form", "why": "No Day form was offered, so today's Day stands without its page"})
            }
            Some(form) => match make_document(scope, &time, &reading, form, &clock, now) {
                Ok(made) => made,
                Err(error) => json!({"state": "refused", "why": error.to_string()}),
            },
        }
    };
    // Each earlier Day still open closes at its turn, oldest first.
    let (relations, _) = scope.relations()?;
    let mut earlier: Vec<(String, String)> = relations["relations"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|entry| entry["temporal"]["lifecycle"] == "open")
        .filter_map(|entry| {
            let date = entry["temporal"]["civil_date"].as_str()?;
            let reference = entry["temporal"]["day_ref"].as_str()?;
            (date < today.as_str()).then(|| (date.to_owned(), reference.to_owned()))
        })
        .collect();
    earlier.sort();
    let mut closed = Vec::new();
    for (date, closing) in earlier {
        let carry = day_moves::close_at_turn(scope, &closing, &day_ref, &clock, now)?;
        closed.push(json!({"day_ref": closing, "civil_date": date, "carry": carry}));
    }
    let mut result = temporal::day_read(scope, &json!({"day_ref": day_ref}))?;
    result["turn"] = json!({"schema": TURN_SCHEMA, "by": CIVIL_CLOCK, "civil_date": today, "opened": opened,
        "document": document, "closed": closed, "time_policy_revision": time.revision});
    Ok(result)
}

/// Make today's document from the Day form at `form`: the form's own data island, its fields mapped as they stand (and each of
/// its keys as a whole), its date and zone Central's, the set of the Day before it carried, and the form's location and revision
/// kept so its page reads that form where it stands.
fn make_document(
    scope: &Scope,
    time: &TimeReading,
    day: &Value,
    form: &Value,
    clock: &Principal,
    now: u64,
) -> io::Result<Value> {
    let location: crate::files::CentralPathRef = serde_json::from_value(form.clone())
        .map_err(|_| invalid("form is the Day form's location as Central's file route gives it (central.path-ref/v1)"))?;
    let original = crate::files::read_file(
        &scope.central_root,
        &location,
        crate::files::FileEncoding::Utf8,
    )?;
    let mut payload = form_payload(&original.content)?;
    if payload.get("_oi_form_source").is_some() {
        return Err(invalid(
            "the Day form already carries a retained form binding; offer its original source",
        ));
    }
    let pointer = |key: &str| key.replace('~', "~0").replace('/', "~1");
    let mut fields: Vec<Value> = payload["fields"]
        .as_object()
        .ok_or_else(|| invalid("the Day form has no field map"))?
        .keys()
        .map(|key| json!({"id": key, "label": key, "template_pointer": format!("/fields/{}", pointer(key))}))
        .collect();
    // Each of the form's keys as a whole, so its typed body, checkboxes, notes, media and session rows are kept as they are.
    for key in payload
        .as_object()
        .ok_or_else(|| invalid("the Day form's data is no object"))?
        .keys()
    {
        fields.push(
            json!({"id": format!("original:{key}"), "label": format!("Original {key}"),
            "template_pointer": format!("/{}", pointer(key))}),
        );
    }
    let day_ref = source::text(day, "day_ref")?.to_owned();
    let document_id = format!("document:{day_ref}");
    if let Some(meta) = payload.get_mut("meta").and_then(Value::as_object_mut) {
        for (key, value) in [
            ("date", json!(time.civil_date)),
            ("timezone", json!(time.policy.timezone)),
        ] {
            if meta
                .get(key)
                .is_some_and(|held| !held.is_null() && held != &value)
            {
                return Err(invalid(format!(
                    "the Day form carries another {key}; offer its blank original"
                )));
            }
            meta.insert(key.into(), value);
        }
        if meta.get("uuid").is_none_or(Value::is_null) {
            meta.insert("uuid".into(), json!(document_id));
        }
        if let Some(set) = carried_set(scope, &time.civil_date)? {
            meta.insert("set".into(), set);
        }
    }
    payload["_oi_form_source"] = json!({"schema": "oi.original-day-form/v1", "location": original.location,
        "revision": original.revision});
    let policy = placement::effective_policy(scope, now)?;
    let input = json!({"kind": "day", "day_ref": day_ref, "document_id": document_id,
        "expected_revision": day["revision"]["revision"], "expected_policy_revision": policy.revision,
        "template_payload": payload, "fields": fields});
    let made = documents::create(scope, &input, clock, now)?;
    Ok(
        json!({"state": "made", "document_id": document_id, "form": original.location,
        "form_revision": original.revision, "revision": made["revision"]}),
    )
}

/// The form's inert data island (`<script id="ql-doc" type="application/json">`), never its scripts: a Day form's (`meta.type`
/// daily) with its field map.
fn form_payload(content: &str) -> io::Result<Value> {
    let at = content
        .find("id=\"ql-doc\"")
        .ok_or_else(|| invalid("the Day form has no ql-doc data island"))?;
    let tag = content[..at]
        .rfind("<script")
        .ok_or_else(|| invalid("ql-doc is not a script data island"))?;
    let opened = content[at..]
        .find('>')
        .map(|i| i + at)
        .ok_or_else(|| invalid("the ql-doc tag is not closed"))?;
    if !content[tag..opened].contains("application/json") {
        return Err(invalid("ql-doc must be inert application/json"));
    }
    let end = content[opened + 1..]
        .find("</script>")
        .map(|i| i + opened + 1)
        .ok_or_else(|| invalid("the ql-doc data is not closed"))?;
    let payload: Value = serde_json::from_str(&content[opened + 1..end])
        .map_err(|error| invalid(format!("the Day form's ql-doc is not JSON: {error}")))?;
    if payload["meta"]["type"] != "daily" {
        return Err(invalid(
            "the form offered is no Day form (meta.type is not daily)",
        ));
    }
    if payload["fields"].as_object().is_none() {
        return Err(invalid("the Day form has no field map"));
    }
    Ok(payload)
}

/// The set the Day before today kept (`meta.set`, oi.day-set/v1), carried (manual §18.1 "What carries to tomorrow"): the worlds it
/// held, each row with only the pages that still play (a row left with none dropped), the Day added to its lineage. None where no
/// earlier Day kept one.
fn carried_set(scope: &Scope, today: &str) -> io::Result<Option<Value>> {
    let (relations, _) = scope.relations()?;
    let before = relations["relations"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|entry| entry["document"].is_object())
        .filter(|entry| {
            entry["temporal"]["civil_date"]
                .as_str()
                .is_some_and(|date| date < today)
        })
        .max_by(|a, b| {
            a["temporal"]["civil_date"]
                .as_str()
                .cmp(&b["temporal"]["civil_date"].as_str())
        })
        .cloned();
    let Some(entry) = before else {
        return Ok(None);
    };
    let held = documents::read(
        scope,
        &json!({"source_ref": entry["ref"], "document_id": entry["document"]["document_id"]}),
    )?;
    let set = &held["document"]["template_payload"]["meta"]["set"];
    if set["schema"] != DAY_SET_SCHEMA {
        return Ok(None);
    }
    let playing = playing_pages(scope)?;
    let plays = |page: &Value| {
        page.as_str().is_some_and(|page| {
            playing
                .as_ref()
                .is_none_or(|held| held.iter().any(|p| p == page))
        })
    };
    let rows: Vec<Value> = set["rows"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|row| {
            let pages: Vec<Value> = row["pages"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|page| plays(page))
                .cloned()
                .collect();
            (!pages.is_empty()).then(|| {
                let mut row = row.clone();
                row["pages"] = Value::Array(pages);
                row
            })
        })
        .collect();
    let mut lineage: Vec<Value> = set["lineage"].as_array().cloned().unwrap_or_default();
    lineage.push(entry["temporal"]["day_ref"].clone());
    Ok(Some(
        json!({"schema": DAY_SET_SCHEMA, "tracks": set["tracks"].as_array().cloned().unwrap_or_default(),
        "rows": rows, "lineage": lineage}),
    ))
}

/// The pages that play now: each active NOW whose task names a page, in the root register and every project. None where a
/// register cannot be read, so no row loses a page Central could not see.
fn playing_pages(scope: &Scope) -> io::Result<Option<Vec<String>>> {
    let mut scopes = vec![Scope::resolve(&scope.central_root, None)];
    if let Ok(members) = std::fs::read_dir(scope.central_root.join("Work")) {
        for member in members.flatten() {
            if !member.path().join("ProjectCentral/project.json").is_file() {
                continue;
            }
            if let Some(name) = member.file_name().to_str().filter(|n| !n.starts_with('.')) {
                scopes.push(Scope::resolve(&scope.central_root, Some(name)));
            }
        }
    }
    let mut pages = Vec::new();
    for resolved in scopes {
        let Ok(records) = resolved.and_then(|register| placement::scan_now(&register)) else {
            return Ok(None);
        };
        pages.extend(records.into_iter().filter_map(|(record, _)| {
            (record.lifecycle == "active")
                .then(|| record.task_ref.strip_prefix(PAGE_TASK).map(str::to_owned))
                .flatten()
        }));
    }
    Ok(Some(pages))
}

// ---- receipts ----------------------------------------------------------------

fn receipts_path(scope: &Scope, civil_date: &str) -> String {
    format!("{}/{civil_date}/receipts.jsonl", scope.day_dir())
}

fn bounded<'a>(
    input: &'a Value,
    key: &str,
    max: usize,
    required: bool,
) -> io::Result<Option<&'a str>> {
    match input.get(key) {
        None | Some(Value::Null) if !required => Ok(None),
        Some(Value::String(text))
            if !text.trim().is_empty() && text.len() <= max && !text.contains('\0') =>
        {
            Ok(Some(text.as_str()))
        }
        _ => Err(invalid(format!(
            "{key} is {}text of at most {max} bytes",
            if required { "" } else { "absent or " }
        ))),
    }
}

/// Lay one act's receipt into the Day of the civil date it happened on (`central.day.receipt.record`): `act` (the Action or op),
/// `owner` (the product that did it), `subject` (what it acted on), `source` (where its effect stands), `receipt` (what the owner
/// answered, bounded) and `receipt_id` (the same id is laid once). Answers the entry and whether it was laid now.
pub(crate) fn record_receipt(scope: &Scope, input: &Value, now: u64) -> io::Result<Value> {
    let receipt_id = bounded(input, "receipt_id", 512, true)?.unwrap_or_default();
    let act = bounded(input, "act", 256, true)?.unwrap_or_default();
    let owner = bounded(input, "owner", 64, true)?.unwrap_or_default();
    let subject = bounded(input, "subject", 2048, true)?.unwrap_or_default();
    let source = bounded(input, "source", 2048, false)?;
    let receipt = input.get("receipt").cloned().unwrap_or(Value::Null);
    if serde_json::to_string(&receipt)?.len() > 16 * 1024 {
        return Err(invalid("receipt is at most 16 KiB of JSON"));
    }
    let occurred = match input.get("occurred_at_unix_seconds") {
        None | Some(Value::Null) => now,
        Some(value) => value
            .as_u64()
            .filter(|at| *at <= now + 300)
            .ok_or_else(|| invalid("occurred_at_unix_seconds is an instant no later than now"))?,
    };
    let civil_date = temporal::time_policy(scope, occurred)?.civil_date;
    let path = receipts_path(scope, &civil_date);
    let log = day_moves::load_lines(scope, &path, RECEIPT_SCHEMA, "receipts")?;
    if let Some(entry) = log
        .events
        .iter()
        .find(|entry| entry["receipt_id"] == receipt_id)
    {
        return Ok(json!({"recorded": false, "civil_date": civil_date, "entry": entry}));
    }
    let entry = json!({"schema": RECEIPT_SCHEMA, "receipt_id": receipt_id, "act": act, "owner": owner,
        "subject": subject, "source": source, "receipt": receipt, "occurred_at_unix_seconds": occurred,
        "recorded_at_unix_seconds": now});
    day_moves::append_lines(
        scope,
        &path,
        &log,
        std::slice::from_ref(&entry),
        ("central.day.receipt.record", "native-operation"),
    )?;
    Ok(json!({"recorded": true, "civil_date": civil_date, "entry": entry}))
}

/// The receipts laid into the Day of `civil_date`, in the order they were laid. A Day with none has none.
pub(crate) fn receipts_of(scope: &Scope, civil_date: &str) -> io::Result<Vec<Value>> {
    Ok(day_moves::load_lines(
        scope,
        &receipts_path(scope, civil_date),
        RECEIPT_SCHEMA,
        "receipts",
    )?
    .events)
}
