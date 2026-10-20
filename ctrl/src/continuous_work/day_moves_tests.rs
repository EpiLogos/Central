//! The Day's moves contract (lane C1): settings, run record, moves reading, due
//! occurrences, two Days at a time and the close carry. Every world here is a
//! disposable temporary Central root with its own civil-time policy, placement
//! policy and credentials; none of it touches a real ground.
use super::*;
use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use std::fs;

const HUMAN: &str = "c1-day-moves-human-token-not-a-live-credential";
const AGENT: &str = "c1-day-moves-agent-token-not-a-live-credential";
const HOST: &str = "c1-day-moves-host-token--not-a-live-credential";
const ALL_ACTIONS: [&str; 8] = [
    "central.day.ensure",
    "central.day.lifecycle",
    "central.day.settings.write",
    "central.day.move.record",
    "central.day.rollover",
    "central.day.close-carry",
    "central.document.create",
    "central.document.mutate",
];
const DEFAULTS_REASON: &str =
    "The Day's settings are the defaults; nothing runs by itself until you save them.";

fn at(rfc3339: &str) -> u64 {
    DateTime::parse_from_rfc3339(rfc3339)
        .unwrap()
        .with_timezone(&Utc)
        .timestamp() as u64
}
fn ms(rfc3339: &str) -> i64 {
    DateTime::parse_from_rfc3339(rfc3339)
        .unwrap()
        .with_timezone(&Utc)
        .timestamp_millis()
}

fn world(timezone: &str, boundary_minutes: u32) -> crate::TempDir {
    let temp = crate::tempdir().unwrap();
    for path in [
        "Control/user",
        "Control/relations",
        "Control/agents",
        "Work/sources",
    ] {
        fs::create_dir_all(temp.path().join(path)).unwrap();
    }
    let scope = Scope::resolve(temp.path(), None).unwrap();
    let grant = |token: &str, principal: &str, kind: &str| {
        json!({"principal_ref":principal,"actor_kind":kind,"token_sha256":format!("{:x}",Sha256::digest(token.as_bytes())),
            "scope_refs":["control:root"],"actions":ALL_ACTIONS,"expires_at_unix_seconds":4_000_000_000u64})
    };
    let mut relations = vec![];
    for (name, role, value) in [
        (
            "placement.json",
            "work-placement-policy",
            json!({"schema":"central.work-placement-policy/v1","scope_ref":"control:root","writable":[{"path":"Work/sources","class":"repository"}],"enforcement":"native-actions","required_coverage":["file-content"],"lease_seconds":300}),
        ),
        (
            "time.json",
            "civil-time-policy",
            json!({"schema":"central.civil-time-policy/v1","scope_ref":"control:root","timezone":timezone,"day_boundary_minutes":boundary_minutes,"automatic_day_rollover":false}),
        ),
        (
            "authority.json",
            "native-action-authority",
            json!({"schema":"central.native-action-authority/v1","scope_ref":"control:root","grants":[
                grant(HUMAN,"human:test","human"),
                grant(AGENT,"agent:test","agent"),
                grant(HOST,"native-service:host","native-service"),
            ]}),
        ),
    ] {
        let source = format!("Control/user/{name}");
        fs::write(
            temp.path().join(&source),
            serde_json::to_vec(&value).unwrap(),
        )
        .unwrap();
        relations.push(json!({"ref":scope.source_ref(&source),"path":source,"roles":[role],"provenance":"human-adopted","standing":"architecture-contract","treatment":"control-user","recognition":"controlled-test-only","recorded_at_unix_seconds":1}));
    }
    fs::write(
        temp.path().join("Control/relations/source-relations.json"),
        serde_json::to_vec(&json!({"schema":"central.control.ground-relations/v1","project_id":"control:root","relations":relations})).unwrap(),
    )
    .unwrap();
    temp
}

fn run(
    root: &Path,
    operation: &str,
    input: Value,
    token: Option<&str>,
    now: u64,
) -> io::Result<Value> {
    execute_with_token_at(root, operation, &input, token, now)
}
fn read(root: &Path, operation: &str, input: Value, now: u64) -> Value {
    execute_at(root, operation, &input, now).unwrap()
}
fn kind_of(result: io::Result<Value>) -> io::ErrorKind {
    result.expect_err("the operation must refuse").kind()
}
fn words_of(result: io::Result<Value>) -> String {
    result.expect_err("the operation must refuse").to_string()
}

fn ensure(root: &Path, now: u64) -> Value {
    let time = read(root, "time_policy", json!({}), now);
    run(
        root,
        "day_ensure",
        json!({"expected_time_policy_revision":time["revision"]}),
        Some(HUMAN),
        now,
    )
    .unwrap()
}
fn save(root: &Path, patch: Value, now: u64) -> Value {
    let current = read(root, "day_settings_read", json!({}), now);
    let mut input = patch;
    input["expected_revision"] = current["revision"].clone();
    run(root, "day_settings_write", input, Some(HUMAN), now).unwrap()
}
fn settings_path(root: &Path) -> std::path::PathBuf {
    root.join("Control/user/day/settings.json")
}
fn log_path(root: &Path, civil_date: &str) -> std::path::PathBuf {
    root.join(format!("Control/user/day/{civil_date}/moves.jsonl"))
}
fn begin(root: &Path, mv: &str, at_ms: i64, token: &str, now: u64) -> io::Result<Value> {
    run(
        root,
        "day_move_record",
        json!({"event":"begin","move":mv,"at_unix_ms":at_ms,"trigger":"press"}),
        Some(token),
        now,
    )
}
fn end(
    root: &Path,
    run_ref: &Value,
    at_ms: i64,
    state: &str,
    token: &str,
    now: u64,
) -> io::Result<Value> {
    run(
        root,
        "day_move_record",
        json!({"event":"end","run_ref":run_ref,"at_unix_ms":at_ms,"state":state,
            "gave":{"summary":"Three tasks, one of them a must.","proposals":["central:proposal:1"],"counts":{"must":1,"should":1,"could":1}},
            "reason":if state=="done" {Value::Null} else {json!("The agent could not start.")}}),
        Some(token),
        now,
    )
}

const T: &str = "2026-10-20T09:00:00Z";

#[test]
fn settings_absent_read_gives_defaults_with_close_at_day_start() {
    let w = world("Europe/London", 0);
    let r = read(w.path(), "day_settings_read", json!({}), at(T));
    assert_eq!(r["schema"], "central.day-settings-reading/v1");
    assert_eq!(r["revision"], "absent");
    assert_eq!(r["saved"], false);
    let s = &r["settings"];
    assert_eq!(s["schema"], "central.day-settings/v1");
    assert_eq!(s["scope_ref"], "control:root");
    assert_eq!(s["rollover"], Value::Null);
    assert_eq!(s["agent_writes"], "propose");
    assert_eq!(s["close_routine"], true);
    for source in ["central", "factory", "expressions", "techne", "live"] {
        assert_eq!(s["sources"][source], true, "{source}");
    }
    for mv in ["tasks", "patterns", "context", "synthesis"] {
        assert_eq!(s["moves"][mv]["runs_at"], json!({"kind":"off"}), "{mv}");
    }
    assert_eq!(s["moves"]["close"]["runs_at"], json!({"kind":"day_start"}));
    let mut own = std::collections::BTreeSet::new();
    for mv in ["tasks", "patterns", "context", "synthesis", "close"] {
        assert_eq!(s["moves"][mv]["prompt"], Value::Null, "{mv} uses its own");
        let prompt = r["own_prompts"][mv].as_str().unwrap();
        assert!(prompt.len() > 80, "{mv}: {prompt}");
        assert_eq!(r["effective_prompts"][mv], prompt);
        assert_eq!(r["standing"]["moves"][mv]["runs_at"], "default");
        assert_eq!(r["standing"]["moves"][mv]["prompt"], "default");
        own.insert(prompt.to_owned());
    }
    assert_eq!(own.len(), 5, "each move has its own prompt");
    assert!(!settings_path(w.path()).exists(), "a read writes nothing");
    assert_eq!(r["automatic_agent_or_model_invocation"], false);
}

#[test]
fn settings_write_then_read_back_with_saved_standing() {
    let w = world("Europe/London", 0);
    let written = save(
        w.path(),
        json!({"rollover":"22:30","agent_writes":"write","close_routine":false,
            "sources":{"live":false},
            "moves":{"tasks":{"prompt":"Be brief.","runs_at":{"kind":"daily","time":"8:05"}}}}),
        at(T),
    );
    let read_back = read(w.path(), "day_settings_read", json!({}), at(T));
    assert_ne!(read_back["revision"], "absent");
    assert_eq!(read_back["revision"], written["revision"]);
    assert_eq!(read_back["saved"], true);
    let s = &read_back["settings"];
    assert_eq!(s["rollover"], "22:30");
    assert_eq!(s["agent_writes"], "write");
    assert_eq!(s["close_routine"], false);
    assert_eq!(s["sources"]["live"], false);
    assert_eq!(s["sources"]["central"], true);
    assert_eq!(s["moves"]["tasks"]["prompt"], "Be brief.");
    assert_eq!(
        s["moves"]["tasks"]["runs_at"],
        json!({"kind":"daily","time":"08:05"}),
        "a time is kept as HH:MM"
    );
    assert_eq!(read_back["effective_prompts"]["tasks"], "Be brief.");
    assert_eq!(
        read_back["effective_prompts"]["patterns"], read_back["own_prompts"]["patterns"],
        "an untouched prompt stays the move's own"
    );
    assert_eq!(read_back["standing"]["rollover"], "saved");
    assert_eq!(read_back["standing"]["sources"]["live"], "saved");
    assert_eq!(read_back["standing"]["sources"]["central"], "default");
    assert_eq!(read_back["standing"]["moves"]["tasks"]["runs_at"], "saved");
    assert_eq!(
        read_back["standing"]["moves"]["patterns"]["runs_at"],
        "default"
    );
    let file: Value =
        serde_json::from_str(&fs::read_to_string(settings_path(w.path())).unwrap()).unwrap();
    assert_eq!(file["schema"], "central.day-settings/v1");
    assert_eq!(file["scope_ref"], "control:root");
}

#[test]
fn saving_one_setting_does_not_save_the_others() {
    let w = world("Europe/London", 0);
    save(w.path(), json!({"agent_writes":"off"}), at(T));
    let r = read(w.path(), "day_settings_read", json!({}), at(T));
    assert_eq!(r["saved"], true);
    assert_eq!(r["standing"]["agent_writes"], "saved");
    assert_eq!(
        r["standing"]["moves"]["close"]["runs_at"], "default",
        "Close at Day start stays the default until the person saves it"
    );
    assert_eq!(
        r["settings"]["moves"]["close"]["runs_at"]["kind"],
        "day_start"
    );
}

#[test]
fn settings_write_with_a_stale_revision_is_refused() {
    let w = world("Europe/London", 0);
    let first = save(w.path(), json!({"agent_writes":"off"}), at(T));
    let before = fs::read_to_string(settings_path(w.path())).unwrap();
    for stale in [json!("absent"), json!("central.content-fnv1a64/v1:1:00")] {
        assert_eq!(
            kind_of(run(
                w.path(),
                "day_settings_write",
                json!({"expected_revision":stale,"agent_writes":"write"}),
                Some(HUMAN),
                at(T)
            )),
            io::ErrorKind::AlreadyExists
        );
    }
    // The first write needs "absent" and nothing else.
    let other = world("Europe/London", 0);
    assert_eq!(
        kind_of(run(
            other.path(),
            "day_settings_write",
            json!({"expected_revision":first["revision"],"agent_writes":"write"}),
            Some(HUMAN),
            at(T)
        )),
        io::ErrorKind::AlreadyExists
    );
    assert_eq!(fs::read_to_string(settings_path(w.path())).unwrap(), before);
    assert!(!settings_path(other.path()).exists());
    // The expected revision is required, not defaulted.
    assert_eq!(
        kind_of(run(
            w.path(),
            "day_settings_write",
            json!({"agent_writes":"write"}),
            Some(HUMAN),
            at(T)
        )),
        io::ErrorKind::InvalidInput
    );
}

#[test]
fn settings_write_by_an_agent_or_without_a_credential_is_refused() {
    let w = world("Europe/London", 0);
    for token in [Some(AGENT), Some(HOST), None] {
        assert_eq!(
            kind_of(run(
                w.path(),
                "day_settings_write",
                json!({"expected_revision":"absent","agent_writes":"write"}),
                token,
                at(T)
            )),
            io::ErrorKind::PermissionDenied,
            "{token:?}"
        );
    }
    assert!(!settings_path(w.path()).exists());
}

#[test]
fn rollover_runs_at_while_rollover_is_off_is_refused_in_words() {
    let w = world("Europe/London", 0);
    let message = words_of(run(
        w.path(),
        "day_settings_write",
        json!({"expected_revision":"absent","moves":{"synthesis":{"runs_at":{"kind":"rollover"}}}}),
        Some(HUMAN),
        at(T),
    ));
    assert!(
        message.contains("Rollover is Off, so a move cannot run at rollover"),
        "{message}"
    );
    assert!(!settings_path(w.path()).exists());
    // With the rollover time given in the same write it stands.
    save(
        w.path(),
        json!({"rollover":"21:00","moves":{"synthesis":{"runs_at":{"kind":"rollover"}}}}),
        at(T),
    );
    // Turning Rollover off while a move still runs at it is the same refusal.
    let current = read(w.path(), "day_settings_read", json!({}), at(T));
    let message = words_of(run(
        w.path(),
        "day_settings_write",
        json!({"expected_revision":current["revision"],"rollover":null}),
        Some(HUMAN),
        at(T),
    ));
    assert!(message.contains("Rollover is Off"), "{message}");
}

#[test]
fn settings_with_a_bad_time_or_value_are_refused() {
    let w = world("Europe/London", 0);
    for patch in [
        json!({"rollover":"25:00"}),
        json!({"rollover":"22"}),
        json!({"rollover":"22:61"}),
        json!({"rollover":7}),
        json!({"agent_writes":"maybe"}),
        json!({"close_routine":"yes"}),
        json!({"sources":{"elsewhere":true}}),
        json!({"sources":{"live":"no"}}),
        json!({"moves":{"daydream":{"prompt":"x"}}}),
        json!({"moves":{"tasks":{"runs_at":{"kind":"daily","time":"08:61"}}}}),
        json!({"moves":{"tasks":{"runs_at":{"kind":"daily"}}}}),
        json!({"moves":{"tasks":{"runs_at":{"kind":"weekly"}}}}),
        json!({"moves":{"tasks":{"runs_at":{"kind":"rollover","time":"08:00"}}}}),
        json!({"moves":{"tasks":{"prompt":""}}}),
        json!({"moves":{"tasks":{"prompt":7}}}),
        json!({"moves":{"tasks":{"colour":"red"}}}),
        json!({"unknown":true}),
        json!({}),
    ] {
        let mut input = patch.clone();
        input["expected_revision"] = json!("absent");
        assert_eq!(
            kind_of(run(
                w.path(),
                "day_settings_write",
                input,
                Some(HUMAN),
                at(T)
            )),
            io::ErrorKind::InvalidInput,
            "{patch}"
        );
    }
    assert!(!settings_path(w.path()).exists());
}

#[test]
fn record_begin_and_end_round_trip_and_a_run_keeps_the_prompt_it_ran_with() {
    let w = world("Europe/London", 0);
    let day = ensure(w.path(), at(T));
    let day_ref = day["day_ref"].as_str().unwrap().to_owned();
    let own = read(w.path(), "day_settings_read", json!({}), at(T))["own_prompts"]["tasks"].clone();
    let begun = begin(w.path(), "tasks", ms("2026-10-20T09:00:05Z"), HOST, at(T)).unwrap();
    assert_eq!(begun["schema"], "central.day-move-run-record/v1");
    assert_eq!(begun["created"], true);
    let run_ref = begun["run"]["run_ref"].clone();
    assert_eq!(
        run_ref,
        format!("central:day-move-run:{day_ref}:tasks:1").as_str()
    );
    assert_eq!(begun["run"]["state"], "running");
    let b = &begun["run"]["begin"];
    assert_eq!(b["schema"], "central.day-move-event/v1");
    assert_eq!(b["event"], "begin");
    assert_eq!(b["day_ref"], day_ref.as_str());
    assert_eq!(b["move"], "tasks");
    assert_eq!(b["at_unix_ms"], ms("2026-10-20T09:00:05Z"));
    assert_eq!(b["trigger"], "press");
    assert_eq!(b["prompt"], own);
    assert_eq!(b["agent_writes"], "propose");
    assert_eq!(
        b["actor_ref"], "native-service:host",
        "Central names the actor from the credential"
    );
    // The person changes the prompt: the run already made keeps its own.
    save(
        w.path(),
        json!({"moves":{"tasks":{"prompt":"Be brief."}}}),
        at(T),
    );
    let ended = end(
        w.path(),
        &run_ref,
        ms("2026-10-20T09:02:00Z"),
        "done",
        AGENT,
        at(T),
    )
    .unwrap();
    assert_eq!(ended["created"], true);
    assert_eq!(ended["run"]["state"], "done");
    assert_eq!(ended["run"]["end"]["event"], "end");
    assert_eq!(ended["run"]["end"]["run_ref"], run_ref);
    assert_eq!(
        ended["run"]["end"]["gave"]["summary"],
        "Three tasks, one of them a must."
    );
    assert_eq!(ended["run"]["end"]["gave"]["counts"]["must"], 1);
    assert_eq!(ended["run"]["end"]["actor_ref"], "agent:test");
    let moves = read(w.path(), "day_moves_read", json!({}), at(T));
    let tasks = &moves["moves"]["tasks"];
    assert_eq!(tasks["running"], Value::Null);
    assert_eq!(tasks["last"]["run_ref"], run_ref);
    assert_eq!(
        tasks["last"]["begin"]["prompt"], own,
        "the prompt it ran with"
    );
    assert_eq!(tasks["last"]["end"]["state"], "done");
    assert_eq!(
        tasks["prompt"], "Be brief.",
        "the next run uses the new prompt"
    );
    let second = begin(w.path(), "tasks", ms("2026-10-20T09:10:00Z"), HOST, at(T)).unwrap();
    assert_eq!(second["run"]["begin"]["prompt"], "Be brief.");
    assert_eq!(
        second["run"]["run_ref"],
        format!("central:day-move-run:{day_ref}:tasks:2").as_str()
    );
    // One event per line, in the Day's own folder.
    let lines = fs::read_to_string(log_path(w.path(), "2026-10-20")).unwrap();
    assert_eq!(lines.lines().count(), 3);
    for line in lines.lines() {
        let event: Value = serde_json::from_str(line).unwrap();
        assert_eq!(event["schema"], "central.day-move-event/v1");
    }
}

#[test]
fn a_scheduled_occurrence_begins_once() {
    let w = world("Europe/London", 0);
    ensure(w.path(), at(T));
    save(
        w.path(),
        json!({"moves":{"tasks":{"runs_at":{"kind":"daily","time":"10:00"}}}}),
        at(T),
    );
    let occurrence = format!("tasks:daily:{}", ms("2026-10-20T09:00:00Z"));
    let input = json!({"event":"begin","move":"tasks","at_unix_ms":ms("2026-10-20T09:00:03Z"),
        "trigger":"schedule","occurrence_ref":occurrence});
    let first = run(
        w.path(),
        "day_move_record",
        input.clone(),
        Some(HOST),
        at(T),
    )
    .unwrap();
    assert_eq!(first["created"], true);
    assert_eq!(first["run"]["begin"]["occurrence_ref"], occurrence.as_str());
    let again = run(
        w.path(),
        "day_move_record",
        input.clone(),
        Some(HOST),
        at(T),
    )
    .unwrap();
    assert_eq!(again["created"], false);
    assert_eq!(again["run"]["run_ref"], first["run"]["run_ref"]);
    assert_eq!(
        fs::read_to_string(log_path(w.path(), "2026-10-20"))
            .unwrap()
            .lines()
            .count(),
        1,
        "the retry wrote nothing"
    );
    // Once it has ended, the same occurrence still answers with that run.
    end(
        w.path(),
        &first["run"]["run_ref"],
        ms("2026-10-20T09:05:00Z"),
        "done",
        HOST,
        at(T),
    )
    .unwrap();
    let after = run(w.path(), "day_move_record", input, Some(HOST), at(T)).unwrap();
    assert_eq!(after["created"], false);
    assert_eq!(after["run"]["state"], "done");
    assert_eq!(
        fs::read_to_string(log_path(w.path(), "2026-10-20"))
            .unwrap()
            .lines()
            .count(),
        2
    );
}

#[test]
fn a_scheduled_begin_needs_a_saved_schedule_and_a_real_occurrence() {
    let w = world("Europe/London", 0);
    ensure(w.path(), at(T));
    let on_defaults = json!({"event":"begin","move":"close","at_unix_ms":ms("2026-10-20T23:00:00Z"),
        "trigger":"schedule","occurrence_ref":format!("close:day_start:{}", ms("2026-10-20T23:00:00Z"))});
    let message = words_of(run(
        w.path(),
        "day_move_record",
        on_defaults.clone(),
        Some(HOST),
        at(T),
    ));
    assert!(message.contains("by itself"), "{message}");
    // Saved Close at Day start: an instant that is not a Day start is no occurrence.
    save(
        w.path(),
        json!({"moves":{"close":{"runs_at":{"kind":"day_start"}}}}),
        at(T),
    );
    let mut wrong_instant = on_defaults.clone();
    wrong_instant["occurrence_ref"] =
        json!(format!("close:day_start:{}", ms("2026-10-20T22:00:00Z")));
    assert_eq!(
        kind_of(run(
            w.path(),
            "day_move_record",
            wrong_instant,
            Some(HOST),
            at(T)
        )),
        io::ErrorKind::InvalidInput
    );
    let mut wrong_move = on_defaults.clone();
    wrong_move["move"] = json!("tasks");
    assert_eq!(
        kind_of(run(
            w.path(),
            "day_move_record",
            wrong_move,
            Some(HOST),
            at(T)
        )),
        io::ErrorKind::InvalidInput
    );
    let mut no_ref = on_defaults.clone();
    no_ref.as_object_mut().unwrap().remove("occurrence_ref");
    assert_eq!(
        kind_of(run(w.path(), "day_move_record", no_ref, Some(HOST), at(T))),
        io::ErrorKind::InvalidInput
    );
    // Close at Day start closes the Day that ends at that instant: the midnight
    // that opens the 21st ends the 20th, which is today's Day here.
    let day = read(w.path(), "day_read", json!({}), at(T));
    assert_eq!(day["temporal"]["civil_date"], "2026-10-20");
    let ok = run(w.path(), "day_move_record", on_defaults, Some(HOST), at(T)).unwrap();
    assert_eq!(ok["created"], true);
}

#[test]
fn a_second_begin_while_the_move_is_running_is_refused_in_words() {
    let w = world("Europe/London", 0);
    ensure(w.path(), at(T));
    let first = begin(w.path(), "tasks", ms("2026-10-20T09:00:05Z"), HOST, at(T)).unwrap();
    let message = words_of(begin(
        w.path(),
        "tasks",
        ms("2026-10-20T09:30:00Z"),
        HOST,
        at(T),
    ));
    assert!(
        message.contains("Tasks is running since 10:00"),
        "{message}"
    );
    assert_eq!(
        kind_of(begin(
            w.path(),
            "tasks",
            ms("2026-10-20T09:30:00Z"),
            HOST,
            at(T)
        )),
        io::ErrorKind::AlreadyExists
    );
    // Another move is its own; and once the first ends, the move can run again.
    begin(
        w.path(),
        "patterns",
        ms("2026-10-20T09:31:00Z"),
        HOST,
        at(T),
    )
    .unwrap();
    end(
        w.path(),
        &first["run"]["run_ref"],
        ms("2026-10-20T09:20:00Z"),
        "stopped",
        HOST,
        at(T),
    )
    .unwrap();
    begin(w.path(), "tasks", ms("2026-10-20T09:40:00Z"), HOST, at(T)).unwrap();
}

#[test]
fn a_second_end_and_an_unknown_run_are_refused() {
    let w = world("Europe/London", 0);
    let day = ensure(w.path(), at(T));
    let first = begin(w.path(), "context", ms("2026-10-20T09:00:05Z"), HOST, at(T)).unwrap();
    let run_ref = first["run"]["run_ref"].clone();
    end(
        w.path(),
        &run_ref,
        ms("2026-10-20T09:01:00Z"),
        "done",
        HOST,
        at(T),
    )
    .unwrap();
    let lines_before = fs::read_to_string(log_path(w.path(), "2026-10-20")).unwrap();
    assert_eq!(
        kind_of(end(
            w.path(),
            &run_ref,
            ms("2026-10-20T09:02:00Z"),
            "done",
            HOST,
            at(T)
        )),
        io::ErrorKind::AlreadyExists
    );
    let unknown = json!(format!(
        "central:day-move-run:{}:context:9",
        day["day_ref"].as_str().unwrap()
    ));
    assert_eq!(
        kind_of(end(
            w.path(),
            &unknown,
            ms("2026-10-20T09:02:00Z"),
            "done",
            HOST,
            at(T)
        )),
        io::ErrorKind::InvalidInput
    );
    assert_eq!(
        kind_of(end(
            w.path(),
            &json!("not a run ref"),
            ms("2026-10-20T09:02:00Z"),
            "done",
            HOST,
            at(T)
        )),
        io::ErrorKind::InvalidInput
    );
    // A run cannot end before it began, nor in a state the pad does not know.
    let other = begin(w.path(), "context", ms("2026-10-20T10:00:00Z"), HOST, at(T)).unwrap();
    assert_eq!(
        kind_of(end(
            w.path(),
            &other["run"]["run_ref"],
            ms("2026-10-20T09:59:00Z"),
            "done",
            HOST,
            at(T)
        )),
        io::ErrorKind::InvalidInput
    );
    assert_eq!(
        kind_of(run(
            w.path(),
            "day_move_record",
            json!({"event":"end","run_ref":other["run"]["run_ref"],"at_unix_ms":ms("2026-10-20T10:01:00Z"),"state":"vanished"}),
            Some(HOST),
            at(T)
        )),
        io::ErrorKind::InvalidInput
    );
    assert_eq!(
        fs::read_to_string(log_path(w.path(), "2026-10-20"))
            .unwrap()
            .lines()
            .count(),
        lines_before.lines().count() + 1,
        "only the one good begin was added"
    );
}

#[test]
fn a_late_end_keeps_its_own_time_and_rewrites_nothing() {
    let w = world("Europe/London", 0);
    ensure(w.path(), at(T));
    let begun = begin(
        w.path(),
        "synthesis",
        ms("2026-10-20T09:00:05Z"),
        HOST,
        at(T),
    )
    .unwrap();
    let before = fs::read_to_string(log_path(w.path(), "2026-10-20")).unwrap();
    // The machine was offline: the result arrives three days later.
    let later = at("2026-10-23T08:00:00Z");
    let ended = end(
        w.path(),
        &begun["run"]["run_ref"],
        ms("2026-10-20T09:30:00Z"),
        "done",
        HOST,
        later,
    )
    .unwrap();
    assert_eq!(
        ended["run"]["end"]["at_unix_ms"],
        ms("2026-10-20T09:30:00Z")
    );
    assert_eq!(
        ended["run"]["end"]["recorded_at_unix_ms"],
        (later * 1000) as i64,
        "when Central received it is kept apart from when it happened"
    );
    let after = fs::read_to_string(log_path(w.path(), "2026-10-20")).unwrap();
    assert!(after.starts_with(&before), "the earlier line is untouched");
    assert_eq!(after.lines().count(), 2);
}

#[test]
fn a_closed_day_refuses_a_begin_but_lets_a_run_already_begun_land() {
    let w = world("Europe/London", 0);
    let day = ensure(w.path(), at(T));
    let tasks = begin(w.path(), "tasks", ms("2026-10-20T09:00:05Z"), HOST, at(T)).unwrap();
    let close = begin(w.path(), "close", ms("2026-10-20T20:00:00Z"), HOST, at(T)).unwrap();
    run(
        w.path(),
        "day_lifecycle",
        json!({"day_ref":day["day_ref"],"expected_revision":day["revision"]["revision"],
            "expected_relations_revision":day["relations_revision"],"lifecycle":"closed"}),
        Some(HUMAN),
        at(T),
    )
    .unwrap();
    assert_eq!(
        kind_of(begin(
            w.path(),
            "patterns",
            ms("2026-10-20T20:05:00Z"),
            HOST,
            at(T)
        )),
        io::ErrorKind::AlreadyExists,
        "a closed Day refuses a begin"
    );
    // The Close move's end lands after the close it performed; a late result of
    // any run already begun keeps its time, so it lands too.
    let closed = end(
        w.path(),
        &close["run"]["run_ref"],
        ms("2026-10-20T20:00:30Z"),
        "done",
        HOST,
        at(T),
    )
    .unwrap();
    assert_eq!(closed["run"]["state"], "done");
    let late = end(
        w.path(),
        &tasks["run"]["run_ref"],
        ms("2026-10-20T09:10:00Z"),
        "done",
        HOST,
        at(T),
    )
    .unwrap();
    assert_eq!(late["run"]["end"]["at_unix_ms"], ms("2026-10-20T09:10:00Z"));
}

#[test]
fn recording_needs_a_credential_and_the_agent_writes_it_ran_under() {
    let w = world("Europe/London", 0);
    ensure(w.path(), at(T));
    assert_eq!(
        kind_of(run(
            w.path(),
            "day_move_record",
            json!({"event":"begin","move":"tasks","at_unix_ms":1,"trigger":"press"}),
            None,
            at(T)
        )),
        io::ErrorKind::PermissionDenied
    );
    assert_eq!(
        kind_of(run(
            w.path(),
            "day_move_record",
            json!({"event":"begin","move":"tasks","at_unix_ms":1,"trigger":"press"}),
            Some("a-token-that-no-grant-names-at-all-anywhere"),
            at(T)
        )),
        io::ErrorKind::PermissionDenied
    );
    let by_human = begin(w.path(), "tasks", ms("2026-10-20T09:00:05Z"), HUMAN, at(T)).unwrap();
    assert_eq!(by_human["run"]["begin"]["actor_ref"], "human:test");
    // Agent writes Off when the run began: it cannot have given proposals or writes.
    save(w.path(), json!({"agent_writes":"off"}), at(T));
    let off = begin(
        w.path(),
        "patterns",
        ms("2026-10-20T09:10:00Z"),
        AGENT,
        at(T),
    )
    .unwrap();
    assert_eq!(off["run"]["begin"]["agent_writes"], "off");
    assert_eq!(
        kind_of(end(
            w.path(),
            &off["run"]["run_ref"],
            ms("2026-10-20T09:11:00Z"),
            "done",
            AGENT,
            at(T)
        )),
        io::ErrorKind::InvalidInput,
        "the run gave proposals though Agent writes was Off"
    );
    let plain = run(
        w.path(),
        "day_move_record",
        json!({"event":"end","run_ref":off["run"]["run_ref"],"at_unix_ms":ms("2026-10-20T09:11:00Z"),"state":"done","gave":{"summary":"Nothing was written."}}),
        Some(AGENT),
        at(T),
    )
    .unwrap();
    assert_eq!(plain["run"]["state"], "done");
}

fn timed_world() -> (crate::TempDir, String) {
    let w = world("Europe/London", 0);
    let day = ensure(w.path(), at("2027-03-27T12:00:00Z"));
    save(
        w.path(),
        json!({"rollover":"22:00","moves":{
            "tasks":{"runs_at":{"kind":"daily","time":"01:30"}},
            "patterns":{"runs_at":{"kind":"daily","time":"06:00"}},
            "context":{"runs_at":{"kind":"off"}},
            "synthesis":{"runs_at":{"kind":"rollover"}},
            "close":{"runs_at":{"kind":"day_start"}}}}),
        at("2027-03-27T12:00:00Z"),
    );
    let day_ref = day["day_ref"].as_str().unwrap().to_owned();
    (w, day_ref)
}
fn next_at(w: &Path, day_ref: &str, mv: &str, now: &str) -> Value {
    read(w, "day_moves_read", json!({"day_ref":day_ref}), at(now))["moves"][mv]["next_at_unix_ms"]
        .clone()
}

#[test]
fn moves_read_next_at_in_london_across_the_spring_forward() {
    let (w, day_ref) = timed_world();
    // Saturday 27 March 2027, midday GMT; the clocks go forward at 01:00 on the 28th.
    let now = "2027-03-27T12:00:00Z";
    // 01:30 does not exist on the 28th: resolved forward by the gap, as time.occurrences does.
    assert_eq!(
        next_at(w.path(), &day_ref, "tasks", now),
        json!(ms("2027-03-28T01:30:00Z"))
    );
    assert_eq!(
        next_at(w.path(), &day_ref, "patterns", now),
        json!(ms("2027-03-28T05:00:00Z"))
    );
    assert_eq!(
        next_at(w.path(), &day_ref, "synthesis", now),
        json!(ms("2027-03-27T22:00:00Z")),
        "the rollover time"
    );
    assert_eq!(
        next_at(w.path(), &day_ref, "close", now),
        json!(ms("2027-03-28T00:00:00Z")),
        "the next civil day boundary"
    );
    assert_eq!(
        next_at(w.path(), &day_ref, "context", now),
        Value::Null,
        "Off"
    );
    // After the change, the same wall times are an hour earlier in UTC.
    let now = "2027-03-28T12:00:00Z";
    assert_eq!(
        next_at(w.path(), &day_ref, "tasks", now),
        json!(ms("2027-03-29T00:30:00Z"))
    );
    assert_eq!(
        next_at(w.path(), &day_ref, "synthesis", now),
        json!(ms("2027-03-28T21:00:00Z"))
    );
    assert_eq!(
        next_at(w.path(), &day_ref, "close", now),
        json!(ms("2027-03-28T23:00:00Z"))
    );
}

#[test]
fn moves_read_next_at_in_london_across_the_autumn_fold() {
    let (w, day_ref) = timed_world();
    // 01:30 happens twice on 31 October 2027: 00:30Z (BST) and 01:30Z (GMT).
    assert_eq!(
        next_at(w.path(), &day_ref, "tasks", "2027-10-30T12:00:00Z"),
        json!(ms("2027-10-31T00:30:00Z"))
    );
    assert_eq!(
        next_at(w.path(), &day_ref, "tasks", "2027-10-31T00:45:00Z"),
        json!(ms("2027-10-31T01:30:00Z")),
        "the second 01:30 is its own occurrence"
    );
    assert_eq!(
        next_at(w.path(), &day_ref, "close", "2027-10-31T00:45:00Z"),
        json!(ms("2027-11-01T00:00:00Z"))
    );
}

#[test]
fn moves_read_marks_a_default_schedule_as_not_running_by_itself() {
    let w = world("Europe/London", 0);
    let day = ensure(w.path(), at(T));
    let moves = read(w.path(), "day_moves_read", json!({}), at(T));
    assert_eq!(moves["schema"], "central.day-moves-reading/v1");
    assert_eq!(moves["day_ref"], day["day_ref"]);
    assert_eq!(moves["revision"], "absent", "no run has been recorded");
    assert_eq!(moves["settings"]["revision"], "absent");
    let close = &moves["moves"]["close"];
    assert_eq!(close["runs_at"], json!({"kind":"day_start"}));
    assert_eq!(close["runs_at_standing"], "default");
    assert_eq!(close["armed"], false);
    assert_eq!(
        close["next_at_unix_ms"],
        Value::Null,
        "it will not run by itself until saved"
    );
    for mv in ["tasks", "patterns", "context", "synthesis", "close"] {
        let pad = &moves["moves"][mv];
        assert_eq!(pad["running"], Value::Null);
        assert_eq!(pad["last"], Value::Null);
        assert_eq!(pad["runs"], json!([]));
        assert_eq!(pad["prompt"], pad["own_prompt"]);
    }
    // Saving the same value makes it the person's: it stands and is armed.
    save(
        w.path(),
        json!({"moves":{"close":{"runs_at":{"kind":"day_start"}}}}),
        at(T),
    );
    let close = read(w.path(), "day_moves_read", json!({}), at(T))["moves"]["close"].clone();
    assert_eq!(close["runs_at_standing"], "saved");
    assert_eq!(close["armed"], true);
    assert_eq!(close["next_at_unix_ms"], json!(ms("2026-10-20T23:00:00Z")));
}

#[test]
fn moves_read_reports_running_last_and_every_run_oldest_first() {
    let w = world("Europe/London", 0);
    ensure(w.path(), at(T));
    let one = begin(w.path(), "tasks", ms("2026-10-20T09:00:00Z"), HOST, at(T)).unwrap();
    end(
        w.path(),
        &one["run"]["run_ref"],
        ms("2026-10-20T09:01:00Z"),
        "refused",
        HOST,
        at(T),
    )
    .unwrap();
    let two = begin(w.path(), "tasks", ms("2026-10-20T10:00:00Z"), HOST, at(T)).unwrap();
    let pad = read(w.path(), "day_moves_read", json!({}), at(T))["moves"]["tasks"].clone();
    assert_eq!(pad["running"]["run_ref"], two["run"]["run_ref"]);
    assert_eq!(pad["running"]["state"], "running");
    assert_eq!(pad["last"]["run_ref"], one["run"]["run_ref"]);
    assert_eq!(pad["last"]["state"], "refused");
    assert_eq!(pad["last"]["end"]["reason"], "The agent could not start.");
    let runs: Vec<_> = pad["runs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["run_ref"].clone())
        .collect();
    assert_eq!(
        runs,
        vec![one["run"]["run_ref"].clone(), two["run"]["run_ref"].clone()]
    );
    let moves = read(w.path(), "day_moves_read", json!({}), at(T));
    assert_ne!(moves["revision"], "absent");
}

#[test]
fn due_on_the_defaults_is_empty_and_says_why() {
    let w = world("Europe/London", 0);
    ensure(w.path(), at(T));
    let due = read(
        w.path(),
        "day_moves_due",
        json!({"since_unix_ms":ms("2026-10-19T22:00:00Z"),"now_unix_ms":ms("2026-10-20T10:00:00Z")}),
        at(T),
    );
    assert_eq!(due["schema"], "central.day-moves-due/v1");
    assert_eq!(due["due"], json!([]));
    assert_eq!(due["reason"], DEFAULTS_REASON);
    assert_eq!(due["automatic_agent_or_model_invocation"], false);
    // Settings saved, but none of them a schedule: still nothing, said differently.
    save(w.path(), json!({"agent_writes":"write"}), at(T));
    let due = read(
        w.path(),
        "day_moves_due",
        json!({"since_unix_ms":ms("2026-10-19T22:00:00Z"),"now_unix_ms":ms("2026-10-20T10:00:00Z")}),
        at(T),
    );
    assert_eq!(due["due"], json!([]));
    assert!(
        due["reason"]
            .as_str()
            .unwrap()
            .contains("nothing runs by itself"),
        "{due}"
    );
}

#[test]
fn due_lists_a_saved_occurrence_in_the_window_and_marks_it_begun() {
    let w = world("Europe/London", 0);
    ensure(w.path(), at("2026-10-20T08:00:00Z"));
    save(
        w.path(),
        json!({"rollover":"22:00","moves":{
            "tasks":{"runs_at":{"kind":"daily","time":"10:00"}},
            "synthesis":{"runs_at":{"kind":"rollover"}},
            "close":{"runs_at":{"kind":"day_start"}}}}),
        at("2026-10-20T08:00:00Z"),
    );
    let tasks_at = ms("2026-10-20T09:00:00Z");
    let close_at = ms("2026-10-19T23:00:00Z");
    let window = |since: i64, now: i64| {
        read(
            w.path(),
            "day_moves_due",
            json!({"since_unix_ms":since,"now_unix_ms":now}),
            at("2026-10-20T10:00:00Z"),
        )
    };
    let due = window(ms("2026-10-19T22:00:00Z"), ms("2026-10-20T10:00:00Z"));
    assert!(
        due.get("reason").is_none() || due["reason"].is_null(),
        "{due}"
    );
    let items = due["due"].as_array().unwrap();
    assert_eq!(items.len(), 2, "{due}");
    assert_eq!(items[0]["move"], "close");
    assert_eq!(items[0]["kind"], "day_start");
    assert_eq!(items[0]["due_unix_ms"], close_at);
    assert_eq!(
        items[0]["occurrence_ref"],
        format!("close:day_start:{close_at}").as_str()
    );
    assert_eq!(
        items[0]["civil_date"], "2026-10-19",
        "Close at Day start closes the Day that ends"
    );
    assert_eq!(items[1]["move"], "tasks");
    assert_eq!(items[1]["kind"], "daily");
    assert_eq!(
        items[1]["occurrence_ref"],
        format!("tasks:daily:{tasks_at}").as_str()
    );
    assert_eq!(items[1]["civil_date"], "2026-10-20");
    assert_eq!(items[1]["begun"], false);
    assert_eq!(items[1]["run_ref"], Value::Null);
    // The window is (since, now]: an occurrence at `since` is out, one at `now` is in.
    assert_eq!(
        window(tasks_at, ms("2026-10-20T10:00:00Z"))["due"],
        json!([])
    );
    let at_now = window(ms("2026-10-20T08:00:00Z"), tasks_at);
    assert_eq!(at_now["due"].as_array().unwrap().len(), 1);
    // The rollover occurrence is not yet due (22:00 BST = 21:00Z); outside the window it is absent.
    assert!(items.iter().all(|i| i["move"] != "synthesis"));
    // Begin the scheduled occurrence: the listing now marks it begun, with its run.
    let begun = run(
        w.path(),
        "day_move_record",
        json!({"event":"begin","move":"tasks","at_unix_ms":tasks_at + 2000,"trigger":"schedule","occurrence_ref":format!("tasks:daily:{tasks_at}")}),
        Some(HOST),
        at("2026-10-20T10:00:00Z"),
    )
    .unwrap();
    let due = window(ms("2026-10-19T22:00:00Z"), ms("2026-10-20T10:00:00Z"));
    let tasks = due["due"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["move"] == "tasks")
        .unwrap()
        .clone();
    assert_eq!(tasks["begun"], true);
    assert_eq!(tasks["run_ref"], begun["run"]["run_ref"]);
    // A window the schedule does not touch lists nothing.
    assert_eq!(
        window(ms("2026-10-20T09:30:00Z"), ms("2026-10-20T20:00:00Z"))["due"],
        json!([])
    );
    // Bad windows are refused.
    for (since, now) in [(5, 4), (0, ms("2030-01-01T00:00:00Z"))] {
        assert_eq!(
            kind_of(execute_at(
                w.path(),
                "day_moves_due",
                &json!({"since_unix_ms":since,"now_unix_ms":now}),
                at("2026-10-20T10:00:00Z")
            )),
            io::ErrorKind::InvalidInput
        );
    }
}

fn relation_count(root: &Path) -> usize {
    let scope = Scope::resolve(root, None).unwrap();
    let (relations, _) = scope.relations().unwrap();
    relations["relations"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["temporal"]["day_ref"].is_string())
        .count()
}

#[test]
fn day_read_reports_tomorrow_only_after_a_rollover_opens_it() {
    let w = world("Europe/London", 0);
    ensure(w.path(), at(T));
    assert_eq!(
        read(w.path(), "day_read", json!({}), at(T))["tomorrow"],
        Value::Null
    );
    let time = read(w.path(), "time_policy", json!({}), at(T));
    let opened = run(
        w.path(),
        "day_rollover",
        json!({"expected_time_policy_revision":time["revision"]}),
        Some(HUMAN),
        at(T),
    )
    .unwrap();
    let day = read(w.path(), "day_read", json!({}), at(T));
    assert_eq!(day["tomorrow"]["day_ref"], opened["day_ref"]);
    assert_eq!(day["tomorrow"]["civil_date"], "2026-10-21");
    assert_eq!(day["tomorrow"]["opened_at_unix_seconds"], at(T));
}

#[test]
fn rollover_opens_tomorrow_beside_today_without_moving_today() {
    let w = world("Europe/London", 0);
    let today = ensure(w.path(), at(T));
    assert_eq!(relation_count(w.path()), 1);
    let time = read(w.path(), "time_policy", json!({}), at(T));
    let opened = run(
        w.path(),
        "day_rollover",
        json!({"expected_time_policy_revision":time["revision"]}),
        Some(HUMAN),
        at(T),
    )
    .unwrap();
    assert_eq!(opened["created"], true);
    assert_eq!(opened["today_advanced"], false);
    assert_eq!(opened["temporal"]["civil_date"], "2026-10-21");
    assert_eq!(opened["temporal"]["lifecycle"], "open");
    assert_eq!(opened["day_ref"], "central:day:control:root:2026-10-21");
    assert_eq!(opened["tomorrow"]["day_ref"], opened["day_ref"]);
    assert_eq!(opened["automatic_agent_or_model_invocation"], false);
    assert!(w.path().join("Control/user/day/2026-10-21/day.md").exists());
    assert_eq!(relation_count(w.path()), 2);
    // Today has not moved, and still reads as today's Day, open.
    let current = read(w.path(), "day_read", json!({}), at(T));
    assert_eq!(current["day_ref"], today["day_ref"]);
    assert_eq!(current["today"]["civil_date"], "2026-10-20");
    assert_eq!(current["temporal"]["lifecycle"], "open");
}

#[test]
fn rollover_is_idempotent() {
    let w = world("Europe/London", 0);
    ensure(w.path(), at(T));
    let time = read(w.path(), "time_policy", json!({}), at(T));
    let input = json!({"expected_time_policy_revision":time["revision"]});
    let first = run(w.path(), "day_rollover", input.clone(), Some(HUMAN), at(T)).unwrap();
    let relations_before =
        fs::read_to_string(w.path().join("Control/relations/source-relations.json")).unwrap();
    let carrier_before = fs::read(w.path().join("Control/user/day/2026-10-21/day.md")).unwrap();
    let again = run(
        w.path(),
        "day_rollover",
        input,
        Some(HUMAN),
        at("2026-10-20T15:00:00Z"),
    )
    .unwrap();
    assert_eq!(again["created"], false);
    assert_eq!(again["day_ref"], first["day_ref"]);
    assert_eq!(
        again["tomorrow"]["opened_at_unix_seconds"],
        at(T),
        "the first opening stands"
    );
    assert_eq!(relation_count(w.path()), 2);
    assert_eq!(
        fs::read_to_string(w.path().join("Control/relations/source-relations.json")).unwrap(),
        relations_before
    );
    assert_eq!(
        fs::read(w.path().join("Control/user/day/2026-10-21/day.md")).unwrap(),
        carrier_before
    );
}

#[test]
fn rollover_needs_a_current_policy_and_a_current_day() {
    let w = world("Europe/London", 0);
    let time = read(w.path(), "time_policy", json!({}), at(T));
    // No Day is open yet: there is no today for a tomorrow to sit beside.
    assert_eq!(
        kind_of(run(
            w.path(),
            "day_rollover",
            json!({"expected_time_policy_revision":time["revision"]}),
            Some(HUMAN),
            at(T)
        )),
        io::ErrorKind::NotFound
    );
    ensure(w.path(), at(T));
    assert_eq!(
        kind_of(run(
            w.path(),
            "day_rollover",
            json!({"expected_time_policy_revision":"stale"}),
            Some(HUMAN),
            at(T)
        )),
        io::ErrorKind::AlreadyExists
    );
    // Today's Day is yesterday's by the civil clock: open today first.
    let next_day = at("2026-10-21T09:00:00Z");
    let time = read(w.path(), "time_policy", json!({}), next_day);
    assert_eq!(
        kind_of(run(
            w.path(),
            "day_rollover",
            json!({"expected_time_policy_revision":time["revision"]}),
            Some(HUMAN),
            next_day
        )),
        io::ErrorKind::InvalidInput
    );
    assert_eq!(relation_count(w.path()), 1);
}

#[test]
fn rollover_by_an_agent_on_the_defaults_is_refused() {
    let w = world("Europe/London", 0);
    ensure(w.path(), at(T));
    let evening = at("2026-10-20T21:30:00Z"); // 22:30 BST
    let time = read(w.path(), "time_policy", json!({}), evening);
    let input = json!({"expected_time_policy_revision":time["revision"]});
    for token in [AGENT, HOST] {
        let message = words_of(run(
            w.path(),
            "day_rollover",
            input.clone(),
            Some(token),
            evening,
        ));
        assert!(message.contains("saved"), "{message}");
    }
    assert_eq!(
        kind_of(run(w.path(), "day_rollover", input.clone(), None, evening)),
        io::ErrorKind::PermissionDenied
    );
    // Settings were saved, but not the rollover time: the defaults still stand there.
    save(w.path(), json!({"agent_writes":"write"}), evening);
    assert_eq!(
        kind_of(run(
            w.path(),
            "day_rollover",
            input.clone(),
            Some(AGENT),
            evening
        )),
        io::ErrorKind::PermissionDenied
    );
    assert_eq!(relation_count(w.path()), 1);
    assert_eq!(
        read(w.path(), "day_read", json!({}), evening)["tomorrow"],
        Value::Null
    );
}

#[test]
fn rollover_by_an_agent_follows_the_saved_time() {
    let w = world("Europe/London", 0);
    ensure(w.path(), at(T));
    save(w.path(), json!({"rollover":"22:00"}), at(T));
    // 21:00 BST: before the saved rollover time.
    let early = at("2026-10-20T20:00:00Z");
    let time = read(w.path(), "time_policy", json!({}), early);
    let input = json!({"expected_time_policy_revision":time["revision"]});
    let message = words_of(run(
        w.path(),
        "day_rollover",
        input.clone(),
        Some(AGENT),
        early,
    ));
    assert!(message.contains("22:00"), "{message}");
    assert_eq!(relation_count(w.path()), 1);
    // 22:30 BST: past it.
    let late = at("2026-10-20T21:30:00Z");
    let opened = run(w.path(), "day_rollover", input, Some(AGENT), late).unwrap();
    assert_eq!(opened["created"], true);
    assert_eq!(opened["temporal"]["civil_date"], "2026-10-21");
    assert_eq!(relation_count(w.path()), 2);
}

#[test]
fn ensure_on_the_next_civil_day_clears_tomorrow_and_creates_no_second_source() {
    let w = world("Europe/London", 0);
    ensure(w.path(), at(T));
    let time = read(w.path(), "time_policy", json!({}), at(T));
    let opened = run(
        w.path(),
        "day_rollover",
        json!({"expected_time_policy_revision":time["revision"]}),
        Some(HUMAN),
        at(T),
    )
    .unwrap();
    let carrier = fs::read(w.path().join("Control/user/day/2026-10-21/day.md")).unwrap();
    let next = ensure(w.path(), at("2026-10-21T09:00:00Z"));
    assert_eq!(next["day_ref"], opened["day_ref"]);
    assert_eq!(
        next["created"], false,
        "the Day already exists; it is not created again"
    );
    assert_eq!(next["today_advanced"], true);
    assert_eq!(next["tomorrow"], Value::Null);
    assert_eq!(next["today"]["civil_date"], "2026-10-21");
    assert_eq!(relation_count(w.path()), 2);
    assert_eq!(
        fs::read(w.path().join("Control/user/day/2026-10-21/day.md")).unwrap(),
        carrier
    );
    assert_eq!(
        read(w.path(), "day_read", json!({}), at("2026-10-21T09:00:00Z"))["tomorrow"],
        Value::Null
    );
    // The old Day is untouched and still open: ensure never closes it.
    let old = read(
        w.path(),
        "day_read",
        json!({"day_ref":"central:day:control:root:2026-10-20"}),
        at("2026-10-21T09:00:00Z"),
    );
    assert_eq!(old["temporal"]["lifecycle"], "open");
}

// ---- close carry -------------------------------------------------------

const PROPOSAL: &str = "<p>Begin with the harbour map.</p>";

fn die_payload(proposal: &str) -> Value {
    let fields: Vec<String> = [
        "p0_quick_thoughts",
        "p0_adjacencies",
        "p1_intentions",
        "p1_must",
        "p1_should",
        "p1_could",
        "p2_operations",
        "p2_manual_activity",
        "p3_patterns_noticed",
        "p3_recurring_themes",
        "p3_connections_made",
        "p4_files_touched",
        "p4_people_concepts",
        "p4_graph_additions",
        "p5_synthesis",
        "p5_quintessence",
        "p5_teleological_aim",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    let mut f = serde_json::Map::new();
    for key in &fields {
        f.insert(
            key.clone(),
            if key.starts_with("p1_") && key != "p1_intentions" {
                json!([])
            } else {
                json!("")
            },
        );
    }
    json!({"meta":{"template":"ql-daily v0.1"},"fields":f,"body":{"p5_tomorrow":proposal},"capture":[]})
}
fn make_document(root: &Path, day_ref: &str, id: &str, proposal: &str, now: u64) -> Value {
    let day = read(root, "day_read", json!({"day_ref":day_ref}), now);
    let policy = read(root, "policy", json!({}), now);
    let payload = die_payload(proposal);
    let fields: Vec<Value> = payload["fields"]
        .as_object()
        .unwrap()
        .keys()
        .map(|key| json!({"id":key,"label":key,"template_pointer":format!("/fields/{key}")}))
        .collect();
    run(
        root,
        "document_create",
        json!({"kind":"day","document_id":id,"day_ref":day_ref,"expected_revision":day["revision"]["revision"],
            "expected_policy_revision":policy["revision"],"template_payload":payload,"fields":fields}),
        Some(HUMAN),
        now,
    )
    .unwrap()
}
fn rollover(root: &Path, now: u64) -> Value {
    let time = read(root, "time_policy", json!({}), now);
    run(
        root,
        "day_rollover",
        json!({"expected_time_policy_revision":time["revision"]}),
        Some(HUMAN),
        now,
    )
    .unwrap()
}
fn carry_input(root: &Path, day_ref: &str, now: u64) -> Value {
    let day = read(root, "day_read", json!({"day_ref":day_ref}), now);
    json!({"day_ref":day_ref,"expected_revision":day["revision"]["revision"],"expected_relations_revision":day["relations_revision"]})
}
const TODAY: &str = "central:day:control:root:2026-10-20";
const TOMORROW: &str = "central:day:control:root:2026-10-21";

/// Two Days, each with a native Day document, today's face 5 holding a proposal.
fn two_days() -> crate::TempDir {
    let w = world("Europe/London", 0);
    ensure(w.path(), at(T));
    make_document(w.path(), TODAY, "day:c1-today", PROPOSAL, at(T));
    rollover(w.path(), at(T));
    make_document(w.path(), TOMORROW, "day:c1-tomorrow", "", at(T));
    w
}
fn tomorrow_document(root: &Path, now: u64) -> Value {
    let day = read(root, "day_read", json!({"day_ref":TOMORROW}), now);
    day["document"].clone()
}

#[test]
fn close_carry_lands_face_fives_proposal_on_tomorrows_face_zero_and_closes_the_day() {
    let w = two_days();
    let input = carry_input(w.path(), TODAY, at(T));
    let done = run(w.path(), "day_close_carry", input, Some(HUMAN), at(T)).unwrap();
    assert_eq!(done["schema"], "central.day-close-carry/v1");
    assert_eq!(done["closed"]["temporal"]["lifecycle"], "closed");
    assert_eq!(done["carry"]["state"], "carried");
    assert_eq!(done["carry"]["from"]["pointer"], "/body/p5_tomorrow");
    assert_eq!(done["carry"]["to"]["pointer"], "/fields/p0_quick_thoughts");
    assert_eq!(done["carry"]["to"]["field_id"], "p0_quick_thoughts");
    // Tomorrow's face 0 now carries it, as a contribution with its author.
    let doc = tomorrow_document(w.path(), at(T));
    let contributions = doc["document"]["contributions"].as_array().unwrap();
    assert_eq!(contributions.len(), 1);
    assert_eq!(
        contributions[0]["id"],
        format!("close-carry:{TODAY}").as_str()
    );
    assert_eq!(contributions[0]["field_id"], "p0_quick_thoughts");
    assert!(contributions[0]["html"]
        .as_str()
        .unwrap()
        .contains("Begin with the harbour map."));
    assert_eq!(contributions[0]["author_ref"], "human:test");
    // Today's pointer has not moved; the closing Day is closed and kept, its proposal intact.
    let closed = read(w.path(), "day_read", json!({"day_ref":TODAY}), at(T));
    assert_eq!(closed["temporal"]["lifecycle"], "closed");
    assert_eq!(closed["temporal"]["lifecycle_actor_ref"], "human:test");
    assert_eq!(
        closed["document"]["document"]["template_payload"]["body"]["p5_tomorrow"],
        PROPOSAL
    );
    assert_eq!(
        read(w.path(), "day_read", json!({}), at(T))["day_ref"],
        TODAY
    );
    // Both revisions come back, and they are the current ones.
    assert_eq!(
        done["revisions"]["closed_day"],
        closed["revision"]["revision"]
    );
    assert_eq!(
        done["revisions"]["tomorrow_day"],
        doc["revision"]["revision"]
    );
    assert_eq!(done["revisions"]["relations"], closed["relations_revision"]);
    assert_eq!(done["tomorrow"]["day_ref"], TOMORROW);
    assert_eq!(done["automatic_agent_or_model_invocation"], false);
}

#[test]
fn close_carry_checks_both_revisions_before_it_writes_anything() {
    let w = two_days();
    let good = carry_input(w.path(), TODAY, at(T));
    let relations_before =
        fs::read_to_string(w.path().join("Control/relations/source-relations.json")).unwrap();
    let tomorrow_before =
        fs::read_to_string(w.path().join("Control/user/day/2026-10-21/day.md")).unwrap();
    for field in ["expected_revision", "expected_relations_revision"] {
        let mut stale = good.clone();
        stale[field] = json!("central.content-fnv1a64/v1:1:00");
        assert_eq!(
            kind_of(run(w.path(), "day_close_carry", stale, Some(HUMAN), at(T))),
            io::ErrorKind::AlreadyExists,
            "{field}"
        );
        let mut missing = good.clone();
        missing.as_object_mut().unwrap().remove(field);
        assert_eq!(
            kind_of(run(
                w.path(),
                "day_close_carry",
                missing,
                Some(HUMAN),
                at(T)
            )),
            io::ErrorKind::InvalidInput,
            "{field}"
        );
    }
    assert_eq!(
        fs::read_to_string(w.path().join("Control/relations/source-relations.json")).unwrap(),
        relations_before
    );
    assert_eq!(
        fs::read_to_string(w.path().join("Control/user/day/2026-10-21/day.md")).unwrap(),
        tomorrow_before
    );
    assert_eq!(
        read(w.path(), "day_read", json!({"day_ref":TODAY}), at(T))["temporal"]["lifecycle"],
        "open"
    );
    // A Day closed already is not closed again.
    run(w.path(), "day_close_carry", good, Some(HUMAN), at(T)).unwrap();
    let again = carry_input(w.path(), TODAY, at(T));
    assert_eq!(
        kind_of(run(w.path(), "day_close_carry", again, Some(HUMAN), at(T))),
        io::ErrorKind::AlreadyExists
    );
}

#[test]
fn close_carry_by_an_agent_needs_the_saved_close_and_a_passed_boundary() {
    let w = two_days();
    let input = carry_input(w.path(), TODAY, at(T));
    // On the defaults: refused, though Close defaults to Day start.
    let message = words_of(run(
        w.path(),
        "day_close_carry",
        input.clone(),
        Some(AGENT),
        at(T),
    ));
    assert!(message.contains("saved"), "{message}");
    assert_eq!(
        kind_of(run(
            w.path(),
            "day_close_carry",
            input.clone(),
            Some(HOST),
            at(T)
        )),
        io::ErrorKind::PermissionDenied
    );
    // Saved settings that leave Close at its default are no consent either.
    save(w.path(), json!({"agent_writes":"write"}), at(T));
    assert_eq!(
        kind_of(run(
            w.path(),
            "day_close_carry",
            input.clone(),
            Some(AGENT),
            at(T)
        )),
        io::ErrorKind::PermissionDenied
    );
    // Close saved at Day start, but the boundary has not passed: the Day is still today's.
    save(
        w.path(),
        json!({"moves":{"close":{"runs_at":{"kind":"day_start"}}}}),
        at(T),
    );
    let input = carry_input(w.path(), TODAY, at(T));
    let message = words_of(run(
        w.path(),
        "day_close_carry",
        input.clone(),
        Some(AGENT),
        at(T),
    ));
    assert!(message.contains("boundary"), "{message}");
    assert_eq!(
        read(w.path(), "day_read", json!({"day_ref":TODAY}), at(T))["temporal"]["lifecycle"],
        "open"
    );
    // Saved at a daily time instead of Day start: no consent for the close routine.
    save(
        w.path(),
        json!({"moves":{"close":{"runs_at":{"kind":"daily","time":"23:00"}}}}),
        at(T),
    );
    let next = at("2026-10-21T08:00:00Z");
    ensure(w.path(), next);
    let input = carry_input(w.path(), TODAY, next);
    assert_eq!(
        kind_of(run(w.path(), "day_close_carry", input, Some(AGENT), next)),
        io::ErrorKind::PermissionDenied
    );
}

#[test]
fn close_carry_by_an_agent_under_saved_consent_closes_and_returns_the_carry_as_a_proposal() {
    let w = two_days();
    save(
        w.path(),
        json!({"agent_writes":"write","moves":{"close":{"runs_at":{"kind":"day_start"}}}}),
        at(T),
    );
    let next = at("2026-10-21T08:00:00Z");
    ensure(w.path(), next); // the day start: tomorrow becomes today
    let input = carry_input(w.path(), TODAY, next);
    let done = run(w.path(), "day_close_carry", input, Some(AGENT), next).unwrap();
    assert_eq!(done["closed"]["temporal"]["lifecycle"], "closed");
    assert_eq!(
        done["closed"]["temporal"]["lifecycle_actor_ref"],
        "agent:test"
    );
    // An agent's entry into a Day waits for the person (receiving); nothing was written there.
    assert_eq!(done["carry"]["state"], "waits");
    let submit = &done["carry"]["proposal"];
    assert_eq!(
        submit["source_ref"],
        tomorrow_document(w.path(), next)["source"]["ref"]
    );
    assert_eq!(submit["document_id"], "day:c1-tomorrow");
    assert_eq!(
        submit["expected_source_revision"],
        done["revisions"]["tomorrow_day"]
    );
    assert_eq!(submit["proposal"]["operation"], "field.append");
    assert_eq!(submit["proposal"]["field_id"], "p0_quick_thoughts");
    assert_eq!(
        submit["proposal"]["contribution_id"],
        format!("close-carry:{TODAY}").as_str()
    );
    assert!(submit["proposal"]["html"]
        .as_str()
        .unwrap()
        .contains("harbour map"));
    let doc = tomorrow_document(w.path(), next);
    assert_eq!(doc["document"]["contributions"], json!([]));
}

#[test]
fn close_carry_closes_and_says_so_when_there_is_nothing_or_nowhere_to_carry() {
    // No Day document on either Day: nothing on face 5 to carry.
    let w = world("Europe/London", 0);
    ensure(w.path(), at(T));
    let input = carry_input(w.path(), TODAY, at(T));
    let done = run(w.path(), "day_close_carry", input, Some(HUMAN), at(T)).unwrap();
    assert_eq!(done["carry"]["state"], "nothing_to_carry");
    assert!(done["carry"]["reason"]
        .as_str()
        .unwrap()
        .contains("no Day document"));
    assert_eq!(done["closed"]["temporal"]["lifecycle"], "closed");
    // Tomorrow's Day was opened through the shared path, and today's pointer did not move.
    assert_eq!(done["tomorrow"]["day_ref"], TOMORROW);
    assert_eq!(relation_count(w.path()), 2);
    let today = read(w.path(), "day_read", json!({}), at(T));
    assert_eq!(today["day_ref"], TODAY);
    assert_eq!(today["tomorrow"]["day_ref"], TOMORROW);

    // A proposal, but tomorrow has no document yet: nowhere to put it.
    let w = world("Europe/London", 0);
    ensure(w.path(), at(T));
    make_document(w.path(), TODAY, "day:c1-today", PROPOSAL, at(T));
    rollover(w.path(), at(T));
    let input = carry_input(w.path(), TODAY, at(T));
    let done = run(w.path(), "day_close_carry", input, Some(HUMAN), at(T)).unwrap();
    assert_eq!(done["carry"]["state"], "not_carried");
    assert!(done["carry"]["reason"]
        .as_str()
        .unwrap()
        .contains("Tomorrow's Day has no Day document"));
    assert_eq!(done["carry"]["proposal_text"], PROPOSAL);
    assert_eq!(done["closed"]["temporal"]["lifecycle"], "closed");

    // An empty proposal carries nothing.
    let w = world("Europe/London", 0);
    ensure(w.path(), at(T));
    make_document(w.path(), TODAY, "day:c1-today", "<p><br></p>", at(T));
    rollover(w.path(), at(T));
    make_document(w.path(), TOMORROW, "day:c1-tomorrow", "", at(T));
    let input = carry_input(w.path(), TODAY, at(T));
    let done = run(w.path(), "day_close_carry", input, Some(HUMAN), at(T)).unwrap();
    assert_eq!(done["carry"]["state"], "nothing_to_carry");
    assert_eq!(
        tomorrow_document(w.path(), at(T))["document"]["contributions"],
        json!([])
    );
}

#[test]
fn close_carry_with_the_close_routine_off_only_closes() {
    let w = two_days();
    save(w.path(), json!({"close_routine":false}), at(T));
    let input = carry_input(w.path(), TODAY, at(T));
    let done = run(w.path(), "day_close_carry", input, Some(HUMAN), at(T)).unwrap();
    assert_eq!(done["carry"]["state"], "off");
    assert_eq!(done["closed"]["temporal"]["lifecycle"], "closed");
    assert_eq!(
        tomorrow_document(w.path(), at(T))["document"]["contributions"],
        json!([])
    );
}
