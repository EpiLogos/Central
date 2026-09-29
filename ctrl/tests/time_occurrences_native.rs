//! `central.time.occurrences` — deterministic occurrence resolution over the
//! recognised civil-time policy. Covers the spec's DST and day-boundary law:
//! Europe/London spring-forward nonexistent local times resolve forward by the
//! gap and are named, autumn-fold ambiguous local times yield two distinct
//! occurrence refs, the day boundary is the policy's, the response is
//! deterministic given (policy revision, schedule, window), and the action is
//! read-only over the Day lifecycle — it never advances "today".

use central_ctrl::continuous_work::{execute_at, source::Scope};
use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::AtomicU64,
    time::{SystemTime, UNIX_EPOCH},
};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct World(PathBuf);
impl World {
    fn path(&self) -> &std::path::Path {
        &self.0
    }
}
impl Drop for World {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// A root whose only recognised policy source is the civil-time policy, so the
/// occurrences action is exercised exactly through the ground it owns.
fn world(timezone: &str, rollover: bool) -> World {
    let path = std::env::temp_dir().join(format!(
        "central-time-occurrences-{}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    fs::create_dir_all(path.join("Control/user")).unwrap();
    fs::create_dir_all(path.join("Control/relations")).unwrap();
    fs::create_dir_all(path.join("Work")).unwrap();
    let policy_path = "Control/user/civil-time-policy.json";
    fs::write(
        path.join(policy_path),
        serde_json::to_vec_pretty(&json!({
            "schema":"central.civil-time-policy/v1","scope_ref":"control:root",
            "timezone":timezone,"day_boundary_minutes":0,"automatic_day_rollover":rollover
        }))
        .unwrap(),
    )
    .unwrap();
    let scope = Scope::resolve(&path, None).unwrap();
    fs::write(
        path.join(&scope.relations_path),
        serde_json::to_vec_pretty(&json!({
            "schema":scope.relations_schema,"project_id":scope.relations_id,
            "relations":[{
                "ref":scope.source_ref(policy_path),"path":policy_path,"roles":["civil-time-policy"],
                "provenance":"human-adopted","standing":"architecture-contract","treatment":"projectcentral-user",
                "recognition":"controlled-test-fixture-not-personal-adoption","recorded_at_unix_seconds":1
            }]
        }))
        .unwrap(),
    )
    .unwrap();
    World(path)
}

fn ms(value: &str) -> i64 {
    DateTime::parse_from_rfc3339(value)
        .unwrap()
        .with_timezone(&Utc)
        .timestamp_millis()
}

fn occurrences(world: &World, schedule: Value, from: &str, to: &str) -> Value {
    execute_at(
        world.path(),
        "time_occurrences",
        &json!({
            "schedule": schedule,
            "window_from_unix_ms": ms(from),
            "window_to_unix_ms": ms(to),
        }),
        100,
    )
    .unwrap()
}

fn dues(reading: &Value) -> Vec<i64> {
    reading["occurrences"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["due_unix_ms"].as_i64().unwrap())
        .collect()
}

fn refs(reading: &Value) -> Vec<String> {
    reading["occurrences"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["occurrence_ref"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn daily_occurrences_resolve_in_the_policy_timezone_not_the_harness_clock() {
    let world = world("Europe/London", false);
    let schedule = json!({"kind":"daily","time":"06:00"});
    // July: British Summer Time, so local 06:00 is 05:00Z.
    let summer = occurrences(
        &world,
        schedule.clone(),
        "2026-07-01T00:00:00Z",
        "2026-07-03T00:00:00Z",
    );
    assert_eq!(summer["schema"], "central.time-occurrences/v1");
    assert_eq!(summer["timezone"], "Europe/London");
    assert!(!summer["time_policy_revision"]
        .as_str()
        .unwrap_or_default()
        .is_empty());
    assert_eq!(
        dues(&summer),
        vec![ms("2026-07-01T05:00:00Z"), ms("2026-07-02T05:00:00Z")]
    );
    // December: GMT, so local 06:00 is 06:00Z.
    let winter = occurrences(
        &world,
        schedule,
        "2026-12-01T00:00:00Z",
        "2026-12-02T00:00:00Z",
    );
    assert_eq!(dues(&winter), vec![ms("2026-12-01T06:00:00Z")]);
}

#[test]
fn spring_forward_nonexistent_local_time_resolves_forward_and_is_named() {
    let world = world("Europe/London", false);
    // 2027-03-28 is the spring-forward morning: local 01:00–01:59 does not exist.
    let reading = occurrences(
        &world,
        json!({"kind":"daily","time":"01:30"}),
        "2027-03-27T00:00:00Z",
        "2027-03-30T00:00:00Z",
    );
    // 27th: 01:30 GMT = 01:30Z. 28th: gap — resolved forward by the one-hour
    // transition to 02:30 BST = 01:30Z. 29th: 01:30 BST = 00:30Z.
    assert_eq!(
        dues(&reading),
        vec![
            ms("2027-03-27T01:30:00Z"),
            ms("2027-03-28T01:30:00Z"),
            ms("2027-03-29T00:30:00Z"),
        ]
    );
    let named = reading["named_resolutions"]
        .as_array()
        .expect("gap is named");
    assert_eq!(named.len(), 1);
    assert_eq!(named[0]["requested_local_time"], "2027-03-28 01:30:00");
    assert_eq!(named[0]["resolved_due_unix_ms"], ms("2027-03-28T01:30:00Z"));
    assert_eq!(
        named[0]["rule"],
        "spring-forward-nonexistent-local-time-resolved-forward-by-gap"
    );
}

#[test]
fn autumn_fold_ambiguous_local_time_yields_two_distinct_occurrences() {
    let world = world("Europe/London", false);
    // 2027-10-31 is the autumn-fold morning: local 01:30 happens twice.
    let reading = occurrences(
        &world,
        json!({"kind":"daily","time":"01:30"}),
        "2027-10-31T00:00:00Z",
        "2027-10-31T23:59:59Z",
    );
    assert_eq!(
        dues(&reading),
        vec![ms("2027-10-31T00:30:00Z"), ms("2027-10-31T01:30:00Z")]
    );
    let distinct = refs(&reading);
    assert_eq!(distinct.len(), 2);
    assert_ne!(distinct[0], distinct[1], "fold instants stay distinct");
    assert!(reading.get("named_resolutions").is_none());
}

#[test]
fn daily_occurrences_honour_the_policy_day_boundary_at_midnight() {
    let world = world("Europe/London", false);
    let reading = occurrences(
        &world,
        json!({"kind":"daily","time":"00:00"}),
        "2026-12-30T12:00:00Z",
        "2027-01-01T12:00:00Z",
    );
    // Local midnights of 31 Dec, 1 Jan (and 30 Dec 00:00 already passed the
    // window start — the window begins at 12:00Z on the 30th).
    assert_eq!(
        dues(&reading),
        vec![ms("2026-12-31T00:00:00Z"), ms("2027-01-01T00:00:00Z")]
    );
}

#[test]
fn every_anchors_at_epoch_multiples_and_stays_inside_the_window() {
    let world = world("Europe/London", false);
    let reading = occurrences(
        &world,
        json!({"kind":"every","interval_ms":900_000}),
        "2026-09-23T10:07:00Z",
        "2026-09-23T10:40:00Z",
    );
    let from = ms("2026-09-23T10:07:00Z");
    // Epoch-multiple anchors: the first 15-minute mark at or after the window
    // start, and none beyond the end.
    for due in dues(&reading) {
        assert_eq!(due % 900_000, 0, "anchored at epoch multiples");
        assert!(due >= from);
    }
    assert!(dues(&reading).len() > 1);
}

#[test]
fn once_resolves_exactly_one_occurrence_inside_the_window() {
    let world = world("Europe/London", false);
    let schedule = json!({"kind":"once","rfc3339":"2026-09-23T18:00:00+01:00"});
    let inside = occurrences(
        &world,
        schedule.clone(),
        "2026-09-23T00:00:00Z",
        "2026-09-24T00:00:00Z",
    );
    assert_eq!(dues(&inside), vec![ms("2026-09-23T17:00:00Z")]);
    let outside = occurrences(
        &world,
        schedule,
        "2026-09-24T00:00:00Z",
        "2026-09-25T00:00:00Z",
    );
    assert_eq!(dues(&outside), Vec::<i64>::new());
}

#[test]
fn cron_fires_twice_in_the_autumn_fold_and_once_on_an_ordinary_day() {
    let world = world("Europe/London", false);
    let schedule = json!({"kind":"cron","expression":"30 1 * * *"});
    let fold = occurrences(
        &world,
        schedule.clone(),
        "2027-10-31T00:00:00Z",
        "2027-10-31T23:59:59Z",
    );
    assert_eq!(
        dues(&fold),
        vec![ms("2027-10-31T00:30:00Z"), ms("2027-10-31T01:30:00Z")]
    );
    let ordinary = occurrences(
        &world,
        schedule,
        "2027-10-01T00:00:00Z",
        "2027-10-01T23:59:59Z",
    );
    assert_eq!(dues(&ordinary), vec![ms("2027-10-01T00:30:00Z")]);
}

#[test]
fn cron_steps_lists_and_ranges_match_expected_minutes() {
    let world = world("Europe/London", false);
    let reading = occurrences(
        &world,
        json!({"kind":"cron","expression":"0,30 */2 * * *"}),
        "2026-12-01T00:00:00Z",
        "2026-12-01T05:59:59Z",
    );
    assert_eq!(
        dues(&reading),
        vec![
            ms("2026-12-01T00:00:00Z"),
            ms("2026-12-01T00:30:00Z"),
            ms("2026-12-01T02:00:00Z"),
            ms("2026-12-01T02:30:00Z"),
            ms("2026-12-01T04:00:00Z"),
            ms("2026-12-01T04:30:00Z"),
        ]
    );
}

#[test]
fn resolution_is_deterministic_and_overlapping_windows_share_occurrence_identity() {
    let world = world("Europe/London", false);
    let schedule = json!({"kind":"daily","time":"06:00"});
    let first = occurrences(
        &world,
        schedule.clone(),
        "2026-07-01T00:00:00Z",
        "2026-07-02T00:00:00Z",
    );
    let second = occurrences(
        &world,
        schedule.clone(),
        "2026-07-01T00:00:00Z",
        "2026-07-02T00:00:00Z",
    );
    assert_eq!(first, second, "same input, same reading");
    // A slid horizon must not mint a second identity for the same instant,
    // because delivered suppression depends on stable occurrence refs.
    let wider = occurrences(
        &world,
        schedule,
        "2026-06-30T00:00:00Z",
        "2026-07-03T00:00:00Z",
    );
    let earlier_refs = refs(&first);
    let wider_refs = refs(&wider);
    for reference in &earlier_refs {
        assert!(
            wider_refs.contains(reference),
            "occurrence identity survives a window slide: {reference}"
        );
    }
}

#[test]
fn policy_edit_rebaselines_the_reading_without_changing_occurrence_identity() {
    let world = world("Europe/London", false);
    let schedule = json!({"kind":"daily","time":"06:00"});
    let before = occurrences(
        &world,
        schedule.clone(),
        "2026-12-01T00:00:00Z",
        "2026-12-02T00:00:00Z",
    );
    // Edit the authored policy source: the reading's basis revision moves.
    let policy_path = world.path().join("Control/user/civil-time-policy.json");
    fs::write(
        &policy_path,
        serde_json::to_vec_pretty(&json!({
            "schema":"central.civil-time-policy/v1","scope_ref":"control:root",
            "timezone":"Europe/London","day_boundary_minutes":0,"automatic_day_rollover":true
        }))
        .unwrap(),
    )
    .unwrap();
    let after = occurrences(
        &world,
        schedule,
        "2026-12-01T00:00:00Z",
        "2026-12-02T00:00:00Z",
    );
    assert_ne!(
        before["time_policy_revision"], after["time_policy_revision"],
        "a policy edit re-baselines the reading"
    );
    assert_eq!(refs(&before), refs(&after), "occurrence identity is stable");
}

#[test]
fn occurrences_never_advance_today_or_touch_the_day_lifecycle() {
    let world = world("Europe/London", false);
    let relations_path = world.path().join("Control/relations/source-relations.json");
    let before = fs::read(&relations_path).unwrap();
    let reading = occurrences(
        &world,
        json!({"kind":"daily","time":"00:00"}),
        "2026-12-31T00:00:00Z",
        "2027-01-02T00:00:00Z",
    );
    assert!(!dues(&reading).is_empty());
    assert_eq!(
        fs::read(&relations_path).unwrap(),
        before,
        "the relations ground is byte-identical after resolution"
    );
    assert!(!world.path().join("Control/user/day").exists());
}

#[test]
fn malformed_schedules_are_refused_in_plain_words() {
    let world = world("Europe/London", false);
    for schedule in [
        json!({"kind":"hourly"}),
        json!({"kind":"daily","time":"25:00"}),
        json!({"kind":"cron","expression":"* * * *"}),
        json!({"kind":"every","interval_ms":0}),
        json!({"kind":"once"}),
        json!({"kind":"once","due_unix_ms":1,"rfc3339":"2026-01-01T00:00:00Z"}),
    ] {
        let result = execute_at(
            world.path(),
            "time_occurrences",
            &json!({
                "schedule": schedule,
                "window_from_unix_ms": ms("2026-09-23T00:00:00Z"),
                "window_to_unix_ms": ms("2026-09-24T00:00:00Z"),
            }),
            100,
        );
        assert!(result.is_err(), "schedule must be refused: {schedule}");
    }
}

#[test]
fn the_action_runs_through_the_real_ctrl_binary_contract() {
    // The binary contract: AIKit execs `ctrl --json --root <path> action run
    // central.time.occurrences <input>` the way it execs central.time.policy,
    // so the join is proven through the ordinary action surface, not only the
    // in-process call.
    let world = world("Europe/London", false);
    let input = json!({
        "schedule": {"kind":"daily","time":"06:00"},
        "window_from_unix_ms": ms("2026-07-01T00:00:00Z"),
        "window_to_unix_ms": ms("2026-07-02T00:00:00Z"),
    });
    let output = Command::new(env!("CARGO_BIN_EXE_ctrl"))
        .args([
            "--json",
            "--root",
            world.path().to_str().unwrap(),
            "action",
            "run",
            "central.time.occurrences",
            &input.to_string(),
        ])
        .env_remove("CENTRAL_NATIVE_TOKEN")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let envelope: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(envelope["ok"], true);
    assert_eq!(envelope["data"]["schema"], "central.time-occurrences/v1");
    assert_eq!(dues(&envelope["data"]), vec![ms("2026-07-01T05:00:00Z")]);
}
