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
const ALL_ACTIONS: [&str; 10] = [
    "central.day.ensure",
    "central.day.entry.part",
    "central.day.field.record",
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

/// What an agent gives back from a move run is plain text, as a record's words
/// are (be2i ruling 7): the Day's moves give `gave.summary` back as what the run
/// gave, so a summary that holds a control or invisible character is refused,
/// and one on two lines stands.
#[test]
fn a_move_runs_summary_is_plain_text_as_a_records_words_are() {
    let w = world("Europe/London", 0);
    ensure(w.path(), at(T));
    let at_ms = ms("2026-10-20T09:00:05Z");
    let run_ref = begin(w.path(), "tasks", at_ms, HOST, at(T)).unwrap()["run"]["run_ref"].clone();
    let end_saying = |summary: &str| {
        run(
            w.path(),
            "day_move_record",
            json!({"event":"end","run_ref":run_ref,"at_unix_ms":at_ms + 60_000,"state":"done",
                "gave":{"summary":summary}}),
            Some(AGENT),
            at(T),
        )
    };
    for summary in [
        "Three \u{202E}tasks.",
        "Three\u{200B} tasks.",
        "Three \u{3164} tasks.",
        "Three\u{7} tasks.",
        "Three tasks.\rOne a must.",
    ] {
        let refused = end_saying(summary).unwrap_err().to_string();
        assert!(
            refused.contains("gave.summary holds no control character")
                && refused.contains("no invisible one"),
            "{summary:?}: {refused}"
        );
    }
    let ended = end_saying("Three tasks.\nOne of them a must.").unwrap();
    assert_eq!(
        ended["run"]["end"]["gave"]["summary"],
        "Three tasks.\nOne of them a must."
    );
}

/// Why a run stopped and the refs it gave are kept as plain as a record's
/// words (be2j ruling 3, DLW build ruling 2026-11-09): the Day shows them as
/// they are kept, so a `reason` that holds a control or invisible character is
/// refused, as is a ref in `gave.proposals` or `gave.writes`, or a
/// `gave.points_at`, that holds one. A refused end leaves its run open.
#[test]
fn a_move_runs_reason_and_the_refs_it_gave_are_plain_as_a_records_words_are() {
    let w = world("Europe/London", 0);
    ensure(w.path(), at(T));
    let at_ms = ms("2026-10-20T09:00:05Z");
    let run_ref = begin(w.path(), "tasks", at_ms, HOST, at(T)).unwrap()["run"]["run_ref"].clone();
    let end = |state: &str, reason: Option<&str>, gave: Value| {
        let mut input = json!({"event":"end","run_ref":run_ref,"at_unix_ms":at_ms + 60_000,"state":state,
            "gave":gave});
        if let Some(reason) = reason {
            input["reason"] = json!(reason);
        }
        run(w.path(), "day_move_record", input, Some(AGENT), at(T))
    };
    let plain = json!({"summary":"Three tasks."});
    for reason in [
        "Waiting on the \u{202E}gate.",
        "Waiting\u{200B} on the gate.",
        "Waiting on \u{3164} the gate.",
        "Waiting\u{7} on the gate.",
        "Waiting.\rOn the gate.",
    ] {
        let refused = words_of(end("stalled", Some(reason), plain.clone()));
        assert!(
            refused.contains("reason holds no control character")
                && refused.contains("no invisible one"),
            "{reason:?}: {refused}"
        );
    }
    for (key, wrong) in [
        ("proposals", "central:proposal:\u{200B}7"),
        ("proposals", "central:proposal:7\ncentral:proposal:8"),
        ("proposals", "central:proposal:\u{E0041}"),
        ("writes", "central:write:\u{2066}7"),
        ("writes", "central:write:\u{2028}7"),
    ] {
        let refused = words_of(end(
            "done",
            None,
            json!({"summary":"Three tasks.", key: [wrong]}),
        ));
        assert!(
            refused.contains(&format!("gave.{key} is a list of refs, each on one line"))
                && refused.contains("no invisible one"),
            "{key} {wrong:?}: {refused}"
        );
    }
    let ended = end(
        "stopped",
        Some("Stopped.\nThe gate was shut."),
        json!({"summary":"Three tasks.","proposals":["central:proposal:7"]}),
    )
    .unwrap();
    assert_eq!(
        ended["run"]["end"]["reason"],
        "Stopped.\nThe gate was shut."
    );
    assert_eq!(
        ended["run"]["end"]["gave"]["proposals"],
        json!(["central:proposal:7"])
    );

    // The Patterns move's one ref is plain too.
    let second = ms("2026-10-20T09:10:00Z");
    let patterns =
        begin(w.path(), "patterns", second, HOST, at(T)).unwrap()["run"]["run_ref"].clone();
    for wrong in [
        "skill/\u{200B}alpha",
        "skill/alpha\u{E0041}",
        "skill/\u{3164}",
    ] {
        let said = words_of(run(
            w.path(),
            "day_move_record",
            json!({"event":"end","run_ref":patterns,"at_unix_ms":second + 60_000,"state":"done",
                "gave":{"summary":"Three closes pull against one skill.","points_at":wrong}}),
            Some(AGENT),
            at(T),
        ));
        assert!(
            said.contains("names one skill or method by its ref"),
            "{wrong:?}: {said}"
        );
    }
}

/// The Patterns move names the one skill or method its pattern's items point
/// at (O-I manual §18.2: "A pattern whose items point at one skill or method
/// can open a refinement of it (§19.7)"): `gave.points_at`, its ref. No other
/// move names one.
#[test]
fn the_patterns_move_names_the_one_skill_its_items_point_at() {
    let w = world("Europe/London", 0);
    ensure(w.path(), at(T));
    let end_giving = |run_ref: &Value, at_ms: i64, points_at: Value| {
        run(
            w.path(),
            "day_move_record",
            json!({"event":"end","run_ref":run_ref,"at_unix_ms":at_ms + 60_000,"state":"done",
                "gave":{"summary":"Three closes pull against one skill.","proposals":["central:proposal:7"],"points_at":points_at}}),
            Some(AGENT),
            at(T),
        )
    };
    let begun = |mv: &str, at_ms: i64| {
        begin(w.path(), mv, at_ms, HOST, at(T)).unwrap()["run"]["run_ref"].clone()
    };
    let first = ms("2026-10-20T09:00:05Z");
    let ended = end_giving(
        &begun("patterns", first),
        first,
        json!("skill/practice/alpha"),
    )
    .unwrap();
    assert_eq!(
        ended["run"]["end"]["gave"]["points_at"],
        "skill/practice/alpha"
    );
    let moves = read(w.path(), "day_moves_read", json!({}), at(T));
    assert_eq!(
        moves["moves"]["patterns"]["last"]["end"]["gave"]["points_at"],
        "skill/practice/alpha"
    );
    // A refused end leaves its run open: each wrong value is refused on the one run.
    let second = ms("2026-10-20T09:10:00Z");
    let open = begun("patterns", second);
    for wrong in [
        json!(""),
        json!("two skills"),
        json!(["skill/a", "skill/b"]),
    ] {
        let said = words_of(end_giving(&open, second, wrong.clone()));
        assert!(
            said.contains("names one skill or method by its ref"),
            "{wrong}: {said}"
        );
    }
    let third = ms("2026-10-20T09:20:00Z");
    let said = words_of(end_giving(
        &begun("tasks", third),
        third,
        json!("skill/practice/alpha"),
    ));
    assert!(said.contains("only the Patterns move"), "{said}");
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

/// A Day reading says when its Day was opened (O-I #756): the instant ensure-day opened it, read at any later moment, ensured
/// again or not. A Day whose relation states no opening says none; nothing stands in for it.
#[test]
fn a_day_reading_says_when_its_day_was_opened_and_never_invents_it() {
    let w = world("Europe/London", 0);
    let opened = ensure(w.path(), at(T));
    assert_eq!(opened["opened_at_unix_seconds"], at(T));
    let later = at("2026-10-20T15:30:00Z");
    assert_eq!(
        read(w.path(), "day_read", json!({}), later)["opened_at_unix_seconds"],
        at(T),
        "the opening, not the reading's moment"
    );
    let time = read(w.path(), "time_policy", json!({}), later);
    let again = run(
        w.path(),
        "day_ensure",
        json!({"expected_time_policy_revision":time["revision"]}),
        Some(HUMAN),
        later,
    )
    .unwrap();
    assert_eq!(again["created"], false);
    assert_eq!(
        again["opened_at_unix_seconds"],
        at(T),
        "the first opening stands"
    );

    // A relation that states no opening instant: the reading carries none.
    let path = w.path().join("Control/relations/source-relations.json");
    let mut relations: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    for entry in relations["relations"].as_array_mut().unwrap() {
        if let Some(temporal) = entry["temporal"].as_object_mut() {
            temporal.remove("created_at_unix_seconds");
        }
    }
    fs::write(&path, serde_json::to_vec(&relations).unwrap()).unwrap();
    let unstated = read(w.path(), "day_read", json!({}), later);
    assert_eq!(unstated["day_ref"], opened["day_ref"]);
    assert_eq!(unstated["opened_at_unix_seconds"], Value::Null);
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

// ---- the field source and a Day entry shown as a part (Central #324; O-I #858 SF06, SF08) ----

fn field_path(root: &Path, civil_date: &str) -> std::path::PathBuf {
    root.join(format!("Control/user/day/{civil_date}/field.jsonl"))
}
fn reached(item_ref: &str, how: &str, at_ms: i64, words: &str) -> Value {
    let mut item = json!({"item_ref":item_ref,"from":{"world_ref":"world:mara","name":"mara"},"how":how,
        "what":"question","words":words,"at_unix_ms":at_ms});
    if how == "star" {
        item["star"] = json!({"star_ref":"star:rosarium","name":"The Rosarium plates"});
    }
    item
}
fn record(root: &Path, item: Value, token: &str, now: u64) -> io::Result<Value> {
    run(root, "day_field_record", item, Some(token), now)
}

#[test]
fn the_field_is_one_of_the_days_sources_on_by_default_and_yours_to_turn_off() {
    let w = world("Europe/London", 0);
    let r = read(w.path(), "day_settings_read", json!({}), at(T));
    assert_eq!(
        r["settings"]["sources"]["field"], true,
        "All On, the field too"
    );
    assert_eq!(r["standing"]["sources"]["field"], "default");
    let saved = save(w.path(), json!({"sources":{"field":false}}), at(T));
    assert_eq!(saved["settings"]["sources"]["field"], false);
    assert_eq!(saved["standing"]["sources"]["field"], "saved");
    assert_eq!(
        saved["settings"]["sources"]["techne"], true,
        "the field is its own source: turning it off leaves Technē on"
    );
    let refused = words_of(run(
        w.path(),
        "day_settings_write",
        json!({"expected_revision":saved["revision"],"sources":{"world":true}}),
        Some(HUMAN),
        at(T),
    ));
    assert!(
        refused.contains("central, factory, expressions, techne, live, field"),
        "the refusal names every source: {refused}"
    );
}

#[test]
fn what_reached_you_from_the_field_is_recorded_in_the_day_naming_who_sent_it_and_how_it_came() {
    let w = world("Europe/London", 0);
    let day = ensure(w.path(), at(T));
    let later = reached(
        "saying:mara-2",
        "to-you",
        ms("2026-10-20T09:30:00Z"),
        "And the fifth?",
    );
    let first = reached(
        "saying:mara-1",
        "star",
        ms("2026-10-20T09:10:00Z"),
        "Which plate is this?",
    );
    let made = record(w.path(), later.clone(), HOST, at(T)).unwrap();
    assert_eq!(made["schema"], "central.day-field-record/v1");
    assert_eq!(made["created"], true);
    assert_eq!(made["item"]["source"], "field");
    assert_eq!(made["item"]["face"], 4, "where the day sat: face 4");
    assert_eq!(made["item"]["day_ref"], day["day_ref"]);
    // Received through the shared field and recorded by the Day under the person's Field setting: nobody wrote it,
    // and the words are the sender's. The credential is the one the person granted, which their setting authorises.
    assert_eq!(
        made["item"]["received"],
        json!({"through":"shared-field","recorded_by":"day:field-source"})
    );
    assert_eq!(made["item"]["authorised_by"], "native-service:host");
    assert!(made["item"].get("actor_ref").is_none(), "{made}");
    assert_eq!(made["automatic_agent_or_model_invocation"], false);
    record(w.path(), first.clone(), AGENT, at(T)).unwrap();
    // The same thing reaching the Day again is the record already made, and nothing is rewritten.
    let before = fs::read_to_string(field_path(w.path(), "2026-10-20")).unwrap();
    let again = record(w.path(), later.clone(), HOST, at(T)).unwrap();
    assert_eq!(again["created"], false);
    assert_eq!(again["item"], made["item"]);
    assert_eq!(
        fs::read_to_string(field_path(w.path(), "2026-10-20")).unwrap(),
        before
    );
    // The reading: each thing in the order it reached you, and by nothing else; who sent it, how it came, when.
    let reading = read(w.path(), "day_field_read", json!({}), at(T));
    assert_eq!(reading["schema"], "central.day-field-reading/v1");
    assert_eq!(reading["day_ref"], day["day_ref"]);
    assert_eq!(
        reading["source"],
        json!({"name":"field","on":true,"standing":"default","on_since_unix_ms":null})
    );
    // What the Day takes: what reached you on its own date (Europe/London, BST: from 2026-10-19T23:00Z).
    assert_eq!(reading["takes"]["from_unix_ms"], ms("2026-10-19T23:00:00Z"));
    assert_eq!(
        reading["takes"]["until_unix_ms"],
        ms("2026-10-20T23:00:00Z")
    );
    let items = reading["items"].as_array().unwrap();
    assert_eq!(
        items
            .iter()
            .map(|i| i["item_ref"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["saying:mara-1", "saying:mara-2"],
        "ordered by when each reached you"
    );
    assert_eq!(
        items[0]["from"],
        json!({"world_ref":"world:mara","name":"mara"})
    );
    assert_eq!(items[0]["how"], "star");
    assert_eq!(items[0]["star"]["name"], "The Rosarium plates");
    assert_eq!(items[1]["how"], "to-you");
    assert_eq!(items[1]["star"], Value::Null);
    assert!(reading["revision"].as_str().is_some_and(|r| r != "absent"));
    assert_eq!(reading["automatic_agent_or_model_invocation"], false);
}

#[test]
fn a_field_record_carries_only_who_how_what_and_when_and_never_a_count_of_looks() {
    let w = world("Europe/London", 0);
    ensure(w.path(), at(T));
    let base = reached(
        "saying:x",
        "to-you",
        ms("2026-10-20T09:10:00Z"),
        "Which plate?",
    );
    let mut looks = base.clone();
    looks["looks"] = json!(41);
    assert!(words_of(record(w.path(), looks, HOST, at(T))).contains("`looks`"));
    let mut how = base.clone();
    how["how"] = json!("popular");
    assert!(words_of(record(w.path(), how, HOST, at(T)))
        .contains("addressed to you (to-you), from a star you are gathered at (star), from someone you watch (watch), or from a search you made (search)"));
    let mut starless = base.clone();
    starless["how"] = json!("star");
    assert_eq!(
        kind_of(record(w.path(), starless, HOST, at(T))),
        io::ErrorKind::InvalidInput
    );
    let mut nameless = base.clone();
    nameless["from"] = json!({"world_ref":"world:mara","name":"  "});
    assert_eq!(
        kind_of(record(w.path(), nameless, HOST, at(T))),
        io::ErrorKind::InvalidInput
    );
    // A star is named only for what came from a star you are gathered at.
    let mut starred = base.clone();
    starred["star"] = json!({"star_ref":"star:rosarium","name":"The Rosarium plates"});
    let words = words_of(record(w.path(), starred, HOST, at(T)));
    assert!(
        words.contains("named only for what came from a star"),
        "{words}"
    );
    let mut wordless = base.clone();
    wordless["words"] = json!("");
    assert_eq!(
        kind_of(record(w.path(), wordless, HOST, at(T))),
        io::ErrorKind::InvalidInput
    );
    // The same ref with other words is a different thing claiming that name: refused, never rewritten.
    record(w.path(), base.clone(), HOST, at(T)).unwrap();
    let mut other = base.clone();
    other["words"] = json!("Another question.");
    assert_eq!(
        kind_of(record(w.path(), other, HOST, at(T))),
        io::ErrorKind::AlreadyExists
    );
    // Recording needs a credential the person granted.
    assert_eq!(
        kind_of(record(w.path(), base, "not-a-granted-token", at(T))),
        io::ErrorKind::PermissionDenied
    );
}

#[test]
fn with_the_field_source_off_or_the_day_closed_nothing_from_the_field_is_recorded() {
    let w = world("Europe/London", 0);
    ensure(w.path(), at(T));
    save(w.path(), json!({"sources":{"field":false}}), at(T));
    let refused = record(
        w.path(),
        reached(
            "saying:x",
            "to-you",
            ms("2026-10-20T09:10:00Z"),
            "Which plate?",
        ),
        HOST,
        at(T),
    );
    let words = words_of(refused);
    assert!(words.contains("Sources keep the field Off"), "{words}");
    assert!(
        !field_path(w.path(), "2026-10-20").exists(),
        "nothing is written"
    );
    let reading = read(w.path(), "day_field_read", json!({}), at(T));
    assert_eq!(reading["source"]["on"], false);
    assert_eq!(reading["source"]["standing"], "saved");
    assert_eq!(reading["items"], json!([]));

    let w = world("Europe/London", 0);
    let day = ensure(w.path(), at(T));
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
        kind_of(record(
            w.path(),
            reached(
                "saying:x",
                "to-you",
                ms("2026-10-20T09:10:00Z"),
                "Which plate?"
            ),
            HOST,
            at(T),
        )),
        io::ErrorKind::AlreadyExists
    );
    assert!(!field_path(w.path(), "2026-10-20").exists());
}

fn set_field(root: &Path, day_ref: &str, field: &str, value: Value, request: &str, now: u64) {
    let day = read(root, "day_read", json!({"day_ref":day_ref}), now);
    let doc = &day["document"];
    run(
        root,
        "document_mutate",
        json!({"source_ref":doc["source"]["ref"],"document_id":doc["document_id"],"expected_revision":doc["revision"]["revision"],
            "request_id":request,"operation":"field.set","field_id":field,"value":value}),
        Some(HUMAN),
        now,
    )
    .unwrap();
}
fn append_contribution(root: &Path, day_ref: &str, field: &str, id: &str, html: &str, now: u64) {
    let day = read(root, "day_read", json!({"day_ref":day_ref}), now);
    let doc = &day["document"];
    run(
        root,
        "document_mutate",
        json!({"source_ref":doc["source"]["ref"],"document_id":doc["document_id"],"expected_revision":doc["revision"]["revision"],
            "request_id":format!("append:{id}"),"operation":"field.append","field_id":field,"contribution_id":id,"html":html}),
        Some(HUMAN),
        now,
    )
    .unwrap();
}
/// The press as the World sends it: on the revision of the Day the entry was
/// offered from, read here just before (DLW build ruling 2026-11-09).
fn part(root: &Path, input: Value, now: u64) -> io::Result<Value> {
    run(
        root,
        "day_entry_part",
        offered(root, input, now),
        Some(HUMAN),
        now,
    )
}
fn offered(root: &Path, mut input: Value, now: u64) -> Value {
    if input.get("expected_revision").is_none() {
        let day = match input.get("day_ref") {
            Some(day_ref) => json!({"day_ref":day_ref}),
            None => json!({}),
        };
        if let Ok(day) = execute_at(root, "day_read", &day, now) {
            input["expected_revision"] = day["revision"]["revision"].clone();
        }
    }
    input
}

#[test]
fn a_day_entry_is_read_as_a_part_you_may_show_and_nothing_else_of_the_day_goes_with_it() {
    let w = world("Europe/London", 0);
    ensure(w.path(), at(T));
    make_document(w.path(), TODAY, "day:c1-today", PROPOSAL, at(T));
    set_field(
        w.path(),
        TODAY,
        "p0_quick_thoughts",
        json!("Begin with the harbour map.\n\n  Then the tide tables."),
        "set:p0",
        at(T),
    );
    set_field(
        w.path(),
        TODAY,
        "p0_adjacencies",
        json!("A private worry about the rent."),
        "set:p0a",
        at(T),
    );
    set_field(
        w.path(),
        TODAY,
        "p1_must",
        json!(["Write the letter", {"text":"Call the harbour master"}]),
        "set:p1",
        at(T),
    );
    append_contribution(
        w.path(),
        TODAY,
        "p0_quick_thoughts",
        "c:bold",
        "<p>A <strong>bold</strong> line &amp; a second</p><p>paragraph</p>",
        at(T),
    );
    let before = read(w.path(), "day_read", json!({"day_ref":TODAY}), at(T));

    let shown = part(w.path(), json!({"field_id":"p0_quick_thoughts"}), at(T)).unwrap();
    assert_eq!(shown["schema"], "central.day-entry-part/v1");
    assert_eq!(shown["day_ref"], TODAY);
    assert_eq!(shown["part"]["kind"], "page");
    assert_eq!(
        shown["part"]["label"],
        "Begin with the harbour map. Then the tide tables."
    );
    assert_eq!(shown["label_cut"], false);
    assert_eq!(shown["author"]["actor_kind"], "human");
    let id = shown["part"]["id"].as_str().unwrap();
    assert!(
        id.starts_with("day-entry:") && id.len() == "day-entry:".len() + 16,
        "an opaque ref: {id}"
    );
    assert!(
        !id.contains("2026") && !id.contains("p0_"),
        "the ref names no date and no field: {id}"
    );
    assert_eq!(
        part(w.path(), json!({"field_id":"p0_quick_thoughts"}), at(T)).unwrap()["part"]["id"],
        id,
        "the same entry is the same part"
    );
    let whole = serde_json::to_string(&shown).unwrap();
    assert!(
        !whole.contains("rent") && !whole.contains("harbour master") && !whole.contains("bold"),
        "nothing else of the Day goes with it: {whole}"
    );
    assert_eq!(
        shown["shown"], false,
        "reading a part shows nothing: showing is your act in the field"
    );
    assert_eq!(shown["automatic_agent_or_model_invocation"], false);

    let item = part(w.path(), json!({"field_id":"p1_must","index":1}), at(T)).unwrap();
    assert_eq!(item["part"]["label"], "Call the harbour master");
    assert_ne!(item["part"]["id"], id);
    let words = part(w.path(), json!({"contribution_id":"c:bold"}), at(T)).unwrap();
    assert_eq!(words["part"]["label"], "A bold line & a second paragraph");
    assert_eq!(
        words["author"],
        json!({"actor_kind":"human","author_ref":"human:test"})
    );

    // Nothing was written by reading parts.
    let after = read(w.path(), "day_read", json!({"day_ref":TODAY}), at(T));
    assert_eq!(
        after["document"]["revision"],
        before["document"]["revision"]
    );
}

#[test]
fn a_day_entry_is_named_exactly_and_holds_words_to_show() {
    let w = world("Europe/London", 0);
    ensure(w.path(), at(T));
    assert!(words_of(part(
        w.path(),
        json!({"field_id":"p0_quick_thoughts"}),
        at(T)
    ))
    .contains("No Day document stands"));
    make_document(w.path(), TODAY, "day:c1-today", PROPOSAL, at(T));
    let long = "word ".repeat(60);
    set_field(
        w.path(),
        TODAY,
        "p0_quick_thoughts",
        json!(long),
        "set:long",
        at(T),
    );
    set_field(
        w.path(),
        TODAY,
        "p1_must",
        json!(["Write the letter"]),
        "set:p1",
        at(T),
    );
    append_contribution(
        w.path(),
        TODAY,
        "p0_quick_thoughts",
        "c:gone",
        "<p>Taken back.</p>",
        at(T),
    );
    let day = read(w.path(), "day_read", json!({"day_ref":TODAY}), at(T));
    let doc = &day["document"];
    run(
        w.path(),
        "document_mutate",
        json!({"source_ref":doc["source"]["ref"],"document_id":doc["document_id"],"expected_revision":doc["revision"]["revision"],
            "request_id":"remove:c:gone","operation":"contribution.remove","contribution_id":"c:gone"}),
        Some(HUMAN),
        at(T),
    )
    .unwrap();
    assert!(
        words_of(part(w.path(), json!({"contribution_id":"c:gone"}), at(T)))
            .contains("was removed")
    );
    let cut = part(w.path(), json!({"field_id":"p0_quick_thoughts"}), at(T)).unwrap();
    let label = cut["part"]["label"].as_str().unwrap();
    assert!(
        label.chars().count() <= 160 && label.ends_with('…') && label.starts_with("word word"),
        "{label}"
    );
    assert_eq!(cut["label_cut"], true);
    for (input, words) in [
        (json!({}), "Name one entry of the Day"),
        (
            json!({"field_id":"p0_quick_thoughts","contribution_id":"c:x"}),
            "Name one entry of the Day",
        ),
        (
            json!({"field_id":"p9_nothing"}),
            "is not a field of this Day",
        ),
        (
            json!({"field_id":"p0_adjacencies"}),
            "holds no words to show",
        ),
        (
            json!({"field_id":"p1_must"}),
            "name one item of it by its index",
        ),
        (json!({"field_id":"p1_must","index":3}), "has no item 3"),
        (
            json!({"field_id":"p0_quick_thoughts","index":0}),
            "holds one entry, not a list",
        ),
        (
            json!({"contribution_id":"c:none"}),
            "is not a contribution of this Day",
        ),
    ] {
        let refused = words_of(part(w.path(), input.clone(), at(T)));
        assert!(refused.contains(words), "{input}: {refused}");
    }
}

#[test]
fn a_day_takes_only_what_reached_you_on_its_own_date() {
    // Europe/London in BST: the Day of 2026-10-21 begins at 2026-10-20T23:00:00Z.
    let w = world("Europe/London", 0);
    let day = ensure(w.path(), at(T));
    // After local midnight but before the person pressed Open: the Day of that date takes it.
    let early = reached(
        "saying:early",
        "to-you",
        ms("2026-10-19T23:30:00Z"),
        "Up early?",
    );
    assert_eq!(
        record(w.path(), early, HOST, at(T)).unwrap()["created"],
        true
    );
    // The earlier Day, still open after midnight, takes nothing of the next date.
    let next = reached(
        "saying:next",
        "to-you",
        ms("2026-10-20T23:30:00Z"),
        "After midnight.",
    );
    let late = at("2026-10-21T00:10:00Z");
    let words = words_of(record(w.path(), next.clone(), HOST, late));
    assert!(
        words.contains("2026-10-21") && words.contains("2026-10-20") && words.contains("own date"),
        "{words}"
    );
    let mut named = next.clone();
    named["day_ref"] = day["day_ref"].clone();
    assert_eq!(
        kind_of(record(w.path(), named, HOST, late)),
        io::ErrorKind::InvalidInput
    );
    let kept = fs::read_to_string(field_path(w.path(), "2026-10-20")).unwrap();
    assert_eq!(kept.lines().count(), 1, "only the early one: {kept}");
    let reading = read(w.path(), "day_field_read", json!({}), late);
    assert_eq!(reading["civil_date"], "2026-10-20");
    assert_eq!(reading["takes"]["from_unix_ms"], ms("2026-10-19T23:00:00Z"));
    assert_eq!(
        reading["takes"]["until_unix_ms"],
        ms("2026-10-20T23:00:00Z")
    );
    // Once the next Day opens, what reached you after its midnight is its own.
    let opened = at("2026-10-21T08:00:00Z");
    let today = ensure(w.path(), opened);
    let made = record(w.path(), next, HOST, opened).unwrap();
    assert_eq!(made["item"]["day_ref"], today["day_ref"]);
    assert!(field_path(w.path(), "2026-10-21").exists());
    let reading = read(w.path(), "day_field_read", json!({}), opened);
    assert_eq!(reading["takes"]["from_unix_ms"], ms("2026-10-20T23:00:00Z"));
    assert_eq!(
        reading["takes"]["until_unix_ms"],
        ms("2026-10-21T23:00:00Z")
    );
}

#[test]
fn turning_the_field_off_and_on_again_records_nothing_from_while_it_was_off() {
    let w = world("Europe/London", 0);
    ensure(w.path(), at(T));
    let reading = read(w.path(), "day_field_read", json!({}), at(T));
    assert_eq!(reading["source"]["on_since_unix_ms"], Value::Null);
    assert_eq!(reading["takes"]["from_unix_ms"], ms("2026-10-19T23:00:00Z"));
    save(
        w.path(),
        json!({"sources":{"field":false}}),
        at("2026-10-20T09:20:00Z"),
    );
    save(
        w.path(),
        json!({"sources":{"field":true}}),
        at("2026-10-20T09:40:00Z"),
    );
    let now = at("2026-10-20T09:41:00Z");
    let reading = read(w.path(), "day_field_read", json!({}), now);
    assert_eq!(
        reading["source"]["on_since_unix_ms"],
        ms("2026-10-20T09:40:00Z")
    );
    assert_eq!(
        reading["takes"]["from_unix_ms"],
        ms("2026-10-20T09:40:00Z"),
        "the later of the Day's midnight and when the field last turned On"
    );
    let while_off = reached(
        "saying:off",
        "to-you",
        ms("2026-10-20T09:30:00Z"),
        "Anyone there?",
    );
    let words = words_of(record(w.path(), while_off, HOST, now));
    assert!(words.contains("turned On"), "{words}");
    let after = reached("saying:on", "to-you", ms("2026-10-20T09:45:00Z"), "Back?");
    assert_eq!(
        record(w.path(), after, HOST, at("2026-10-20T09:46:00Z")).unwrap()["created"],
        true
    );
    // Saving it On again, or another source, leaves when it turned On.
    save(
        w.path(),
        json!({"sources":{"field":true,"techne":false}}),
        at("2026-10-20T10:00:00Z"),
    );
    let reading = read(
        w.path(),
        "day_field_read",
        json!({}),
        at("2026-10-20T10:01:00Z"),
    );
    assert_eq!(
        reading["source"]["on_since_unix_ms"],
        ms("2026-10-20T09:40:00Z")
    );
    // When it turned On is Central's to keep, never a setting to write.
    let current = read(w.path(), "day_settings_read", json!({}), at(T));
    let words = words_of(run(
        w.path(),
        "day_settings_write",
        json!({"expected_revision":current["revision"],"on_since":{"field":0}}),
        Some(HUMAN),
        at(T),
    ));
    assert!(words.contains("`on_since`"), "{words}");
}

#[test]
fn a_thing_the_day_keeps_answers_kept_whatever_the_day_refuses_now() {
    let w = world("Europe/London", 0);
    ensure(w.path(), at(T));
    let item = reached("saying:kept", "to-you", ms("2026-10-20T09:10:00Z"), "Kept?");
    record(w.path(), item.clone(), HOST, at(T)).unwrap();
    save(w.path(), json!({"sources":{"field":false}}), at(T));
    assert_eq!(
        record(w.path(), item.clone(), HOST, at(T)).unwrap()["created"],
        false,
        "kept, though the field is Off now"
    );
    let day = read(w.path(), "day_read", json!({"day_ref":TODAY}), at(T));
    run(
        w.path(),
        "day_lifecycle",
        json!({"day_ref":TODAY,"expected_revision":day["revision"]["revision"],
            "expected_relations_revision":day["relations_revision"],"lifecycle":"closed"}),
        Some(HUMAN),
        at(T),
    )
    .unwrap();
    assert_eq!(
        record(w.path(), item, HOST, at(T)).unwrap()["created"],
        false,
        "kept, though the Day is closed now"
    );
}

fn records(root: &Path, list: Vec<Value>, now: u64) -> io::Result<Value> {
    run(
        root,
        "day_field_record",
        json!({"records":list}),
        Some(HOST),
        now,
    )
}

#[test]
fn what_waits_is_recorded_in_one_call_and_each_record_is_answered_on_its_own() {
    let w = world("Europe/London", 0);
    let day = ensure(w.path(), at(T));
    let first = reached("saying:1", "to-you", ms("2026-10-20T09:10:00Z"), "One.");
    let second = reached("saying:2", "star", ms("2026-10-20T09:20:00Z"), "Two.");
    let mut bad = reached("saying:bad", "to-you", ms("2026-10-20T09:25:00Z"), "Bad.");
    bad["how"] = json!("popular");
    let tomorrow = reached(
        "saying:3",
        "to-you",
        ms("2026-10-20T23:30:00Z"),
        "Tomorrow's.",
    );
    record(w.path(), first.clone(), HOST, at(T)).unwrap();
    let before = fs::read_to_string(field_path(w.path(), "2026-10-20")).unwrap();
    let answer = records(
        w.path(),
        vec![first, second.clone(), bad, tomorrow.clone(), second],
        at(T),
    )
    .unwrap();
    assert_eq!(answer["schema"], "central.day-field-records/v1");
    assert_eq!(answer["day_ref"], day["day_ref"]);
    let results = answer["results"].as_array().unwrap();
    assert_eq!(
        results.len(),
        5,
        "one answer per record, in order: {answer}"
    );
    assert_eq!(results[0]["item_ref"], "saying:1");
    assert_eq!(results[0]["created"], false, "kept already");
    assert!(results[0].get("refused").is_none());
    assert_eq!(results[1]["item_ref"], "saying:2");
    assert_eq!(results[1]["created"], true);
    assert_eq!(results[2]["item_ref"], "saying:bad");
    assert!(results[2]["refused"]
        .as_str()
        .unwrap()
        .contains("addressed to you"));
    assert!(results[3]["refused"].as_str().unwrap().contains("own date"));
    assert_eq!(
        results[4]["created"], false,
        "the same record twice is kept once"
    );
    assert_eq!(
        (&answer["created"], &answer["kept"], &answer["refused"]),
        (&json!(1), &json!(2), &json!(2))
    );
    let after = fs::read_to_string(field_path(w.path(), "2026-10-20")).unwrap();
    assert!(
        after.starts_with(&before) && after.lines().count() == 2,
        "{after}"
    );
    assert_eq!(
        answer["revision"],
        read(w.path(), "day_field_read", json!({}), at(T))["revision"]
    );
    assert_eq!(answer["automatic_agent_or_model_invocation"], false);

    // Many new records are one rewrite: every one appended after what was kept.
    let more: Vec<Value> = (0..3)
        .map(|n| {
            reached(
                &format!("saying:more-{n}"),
                "watch",
                ms("2026-10-20T09:40:00Z") + n,
                "More.",
            )
        })
        .collect();
    let answer = records(w.path(), more, at(T)).unwrap();
    assert_eq!(answer["created"], 3);
    let grown = fs::read_to_string(field_path(w.path(), "2026-10-20")).unwrap();
    assert!(grown.starts_with(&after) && grown.lines().count() == 5);

    // The call names the Day; a record in it names none.
    let mut dated = reached(
        "saying:dated",
        "to-you",
        ms("2026-10-20T09:50:00Z"),
        "Dated.",
    );
    dated["day_ref"] = day["day_ref"].clone();
    let answer = records(w.path(), vec![dated], at(T)).unwrap();
    assert!(answer["results"][0]["refused"]
        .as_str()
        .unwrap()
        .contains("`day_ref`"));
    for (input, words) in [
        (json!({"records":[]}), "at least one"),
        (json!({"records":"saying:1"}), "a list"),
        (
            json!({"records":[reached("saying:x", "to-you", ms("2026-10-20T09:10:00Z"), "x")],"item_ref":"saying:y"}),
            "either",
        ),
    ] {
        let refused = words_of(run(
            w.path(),
            "day_field_record",
            input.clone(),
            Some(HOST),
            at(T),
        ));
        assert!(refused.contains(words), "{input}: {refused}");
    }
    let many: Vec<Value> = (0..65)
        .map(|n| {
            reached(
                &format!("saying:n-{n}"),
                "to-you",
                ms("2026-10-20T09:10:00Z"),
                "n",
            )
        })
        .collect();
    assert!(words_of(records(w.path(), many, at(T))).contains("at most 64"));

    // With the field Off, nothing new is recorded and what is kept still answers kept.
    save(w.path(), json!({"sources":{"field":false}}), at(T));
    let fresh = reached("saying:off", "to-you", ms("2026-10-20T09:55:00Z"), "Off?");
    let kept = reached("saying:1", "to-you", ms("2026-10-20T09:10:00Z"), "One.");
    let answer = records(w.path(), vec![fresh, kept], at(T)).unwrap();
    assert!(answer["results"][0]["refused"]
        .as_str()
        .unwrap()
        .contains("Off"));
    assert_eq!(answer["results"][1]["created"], false);
}

#[test]
fn a_list_item_keeps_its_part_however_the_list_moves_and_no_part_ref_is_guessed() {
    let w = world("Europe/London", 0);
    ensure(w.path(), at(T));
    make_document(w.path(), TODAY, "day:c1-today", PROPOSAL, at(T));
    set_field(
        w.path(),
        TODAY,
        "p1_must",
        json!([{"id":"t-1","text":"Call the harbour master"}, "Write the letter"]),
        "set:p1",
        at(T),
    );
    let called = part(w.path(), json!({"field_id":"p1_must","index":0}), at(T)).unwrap();
    assert_eq!(
        called["entry"],
        json!({"field_id":"p1_must","item":"t-1"}),
        "the item's own id"
    );
    let letter = part(w.path(), json!({"field_id":"p1_must","index":1}), at(T)).unwrap();
    let letter_item = letter["entry"]["item"].as_str().unwrap().to_owned();
    assert!(letter_item.starts_with("words:"), "{letter_item}");
    assert!(
        letter["entry"].get("index").is_none(),
        "an item's place is never its identity"
    );
    set_field(
        w.path(),
        TODAY,
        "p1_must",
        json!(["Write the letter", "Buy rope", {"id":"t-1","text":"Call the harbour master"}]),
        "set:p1b",
        at(T),
    );
    let id = |input: Value| part(w.path(), input, at(T)).unwrap()["part"]["id"].clone();
    assert_eq!(
        id(json!({"field_id":"p1_must","index":2})),
        called["part"]["id"]
    );
    assert_eq!(
        id(json!({"field_id":"p1_must","item_id":"t-1"})),
        called["part"]["id"]
    );
    assert_eq!(
        id(json!({"field_id":"p1_must","index":0})),
        letter["part"]["id"]
    );
    let refused = words_of(part(
        w.path(),
        json!({"field_id":"p1_must","item_id":"t-9"}),
        at(T),
    ));
    assert!(refused.contains("no item t-9"), "{refused}");

    // The salt is Central's, one per world, kept outside every Day and written once: a part ref is no digest of what
    // anyone could guess.
    let salt_path = w.path().join(".central/day-part-salt.json");
    let salt = fs::read_to_string(&salt_path).unwrap();
    id(json!({"field_id":"p1_must","index":0}));
    assert_eq!(
        fs::read_to_string(&salt_path).unwrap(),
        salt,
        "written once"
    );
    let guessed = source::key(
        &serde_json::to_string(&json!([TODAY, {"field_id":"p1_must","item":"t-1"}])).unwrap(),
    );
    assert_ne!(
        called["part"]["id"],
        format!("day-entry:{}", &guessed[..16])
    );
    let other = world("Europe/London", 0);
    ensure(other.path(), at(T));
    make_document(other.path(), TODAY, "day:c1-today", PROPOSAL, at(T));
    set_field(
        other.path(),
        TODAY,
        "p1_must",
        json!([{"id":"t-1","text":"Call the harbour master"}]),
        "set:p1",
        at(T),
    );
    assert_ne!(
        part(other.path(), json!({"field_id":"p1_must","index":0}), at(T)).unwrap()["part"]["id"],
        called["part"]["id"],
        "another ground keeps another salt"
    );

    // A field's value is text: nothing in it is markup, and nothing is unescaped.
    set_field(
        w.path(),
        TODAY,
        "p0_quick_thoughts",
        json!("5 < 6 and R&D <b>stay</b> &amp;"),
        "set:p0",
        at(T),
    );
    assert_eq!(
        part(w.path(), json!({"field_id":"p0_quick_thoughts"}), at(T)).unwrap()["part"]["label"],
        "5 < 6 and R&D <b>stay</b> &amp;"
    );
}

// ---- the Day's own date, by the civil date's own rule (DLW build ruling 2026-11-08) -----------------------------

fn takes_of(root: &Path, now: u64) -> Value {
    read(root, "day_field_read", json!({}), now)["takes"].clone()
}
fn span(from: &str, until: &str) -> Value {
    json!({"from_unix_ms":ms(from),"until_unix_ms":ms(until)})
}
fn said(name: &str, at_ms: i64) -> Value {
    reached(&format!("saying:{name}"), "to-you", at_ms, name)
}
fn created_of(answers: &Value) -> Vec<Value> {
    answers["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|result| result["created"].clone())
        .collect()
}

#[test]
fn a_day_begun_in_a_spring_forward_gap_takes_what_its_civil_date_names_from_the_instant_the_date_flips(
) {
    // Europe/London with the Day boundary at 01:30: on 2026-03-29 the clocks go from 01:00 GMT to 02:00 BST, so 01:30
    // never stands on the wall. The civil date (the wall less 01:30) flips at the change itself, 01:00Z, half an hour
    // before the boundary's own occurrence (resolved forward to 02:30 BST, 01:30Z): the Day takes from the flip.
    let w = world("Europe/London", 90);
    let now = at("2026-03-29T12:00:00Z");
    assert_eq!(
        ensure(w.path(), now)["day_ref"],
        "central:day:control:root:2026-03-29"
    );
    let takes = takes_of(w.path(), now);
    assert_eq!(takes["from_unix_ms"], ms("2026-03-29T01:00:00Z"));
    assert_eq!(takes["until_unix_ms"], ms("2026-03-30T00:30:00Z"));
    let start = ms("2026-03-29T01:00:00Z");
    let until = ms("2026-03-30T00:30:00Z");
    let answers = records(
        w.path(),
        vec![
            said("at-start", start),
            said("before-start", start - 1),
            said("before-until", until - 1),
            said("at-until", until),
        ],
        now,
    )
    .unwrap();
    assert_eq!(
        created_of(&answers),
        [json!(true), json!(false), json!(true), json!(false)],
        "{answers}"
    );
    let refused = |at: usize| {
        answers["results"][at]["refused"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    assert!(refused(1).contains("on 2026-03-28"), "{}", refused(1));
    assert!(refused(3).contains("on 2026-03-30"), "{}", refused(3));
    assert_eq!(
        takes["spans"],
        json!([span("2026-03-29T01:00:00Z", "2026-03-30T00:30:00Z")])
    );
}

#[test]
fn a_day_across_an_autumn_fold_takes_only_what_its_civil_date_names_and_the_repeated_half_hour_is_the_earlier_days(
) {
    // Europe/London with the Day boundary at 01:30: on 2026-10-25 the clocks go back from 02:00 BST to 01:00 GMT, so
    // the wall stands at 01:30 twice. The civil date flips to the 25th at the first (00:30Z), back to the 24th at the
    // change (01:00Z, the wall at 01:00 again) and to the 25th at the second (01:30Z): the Day of the 25th holds two
    // spans, and what reached you in the repeated half hour is the 24th's, as the civil date names it.
    let w = world("Europe/London", 90);
    let now = at("2026-10-25T12:00:00Z");
    ensure(w.path(), now);
    let takes = takes_of(w.path(), now);
    assert_eq!(takes["from_unix_ms"], ms("2026-10-25T00:30:00Z"));
    assert_eq!(takes["until_unix_ms"], ms("2026-10-26T01:30:00Z"));
    let change = ms("2026-10-25T01:00:00Z");
    let second = ms("2026-10-25T01:30:00Z");
    let until = ms("2026-10-26T01:30:00Z");
    let answers = records(
        w.path(),
        vec![
            said("first-start", ms("2026-10-25T00:30:00Z")),
            said("before-change", change - 1),
            said("at-change", change),
            said("in-the-repeat", second - 1),
            said("second-start", second),
            said("before-until", until - 1),
            said("at-until", until),
        ],
        now,
    )
    .unwrap();
    assert_eq!(
        created_of(&answers),
        [
            json!(true),
            json!(true),
            json!(false),
            json!(false),
            json!(true),
            json!(true),
            json!(false)
        ],
        "{answers}"
    );
    for at in [2, 3] {
        let refused = answers["results"][at]["refused"].as_str().unwrap();
        assert!(refused.contains("on 2026-10-24"), "{refused}");
    }
    assert!(answers["results"][6]["refused"]
        .as_str()
        .unwrap()
        .contains("on 2026-10-26"));
    assert_eq!(
        takes["spans"],
        json!([
            span("2026-10-25T00:30:00Z", "2026-10-25T01:00:00Z"),
            span("2026-10-25T01:30:00Z", "2026-10-26T01:30:00Z")
        ])
    );
}

#[test]
fn a_record_the_day_keeps_answers_kept_even_when_no_boundary_of_the_day_resolves() {
    let w = world("Europe/London", 0);
    ensure(w.path(), at(T));
    let kept = reached("saying:kept", "to-you", ms("2026-10-20T09:10:00Z"), "Kept?");
    record(w.path(), kept.clone(), HOST, at(T)).unwrap();
    // The civil-time policy names a zone no calendar knows: no boundary of the Day resolves now.
    let policy = w.path().join("Control/user/time.json");
    let mut value: Value = serde_json::from_slice(&fs::read(&policy).unwrap()).unwrap();
    value["timezone"] = json!("Nowhere/Never");
    fs::write(&policy, serde_json::to_vec(&value).unwrap()).unwrap();
    let new = reached("saying:new", "to-you", ms("2026-10-20T09:20:00Z"), "New?");
    let answers = records(w.path(), vec![kept.clone(), new], at(T)).unwrap();
    assert_eq!(answers["results"][0]["created"], false, "{answers}");
    assert!(answers["results"][0].get("refused").is_none(), "{answers}");
    assert!(
        answers["results"][1]["refused"]
            .as_str()
            .unwrap()
            .contains("timezone"),
        "{answers}"
    );
    assert_eq!(
        record(w.path(), kept, HOST, at(T)).unwrap()["created"],
        false,
        "the single form too"
    );
}

#[test]
fn a_name_holds_no_control_character_and_words_hold_none_but_line_breaks_and_tabs() {
    let w = world("Europe/London", 0);
    ensure(w.path(), at(T));
    let at_ms = ms("2026-10-20T09:10:00Z");
    let mut named = reached("saying:named", "to-you", at_ms, "Hello.");
    named["from"]["name"] = json!("ma\u{1b}[2Jra");
    let mut starred = reached("saying:starred", "star", at_ms, "Hello.");
    starred["star"]["name"] = json!("The Rosarium\nplates");
    let belled = reached("saying:belled", "to-you", at_ms, "Ring\u{7} ring");
    let mut two_lines = reached("saying:two-lines", "to-you", at_ms, "Hello.");
    two_lines["from"]["name"] = json!("mara\nof the plates");
    let lines = reached(
        "saying:lines",
        "to-you",
        at_ms,
        "Which plate?\nThe second,\tor the first.\r\n",
    );
    let answers = records(
        w.path(),
        vec![named, starred, belled, two_lines, lines],
        at(T),
    )
    .unwrap();
    let results = answers["results"].as_array().unwrap();
    let names = ["from.name", "star.name", "words", "from.name"];
    for (result, what) in results.iter().zip(names) {
        let refused = result["refused"].as_str().unwrap_or_default();
        assert!(
            refused.contains(what) && refused.contains("control"),
            "{result}"
        );
    }
    assert_eq!(
        results[4]["created"], true,
        "line breaks and tabs are words"
    );
}

// ---- a Day entry as a part: identity, salt and press (DLW build ruling 2026-11-08) -----------------------------

#[test]
fn two_items_with_the_same_words_are_two_parts_and_every_identity_a_part_names_selects_it() {
    let w = world("Europe/London", 0);
    ensure(w.path(), at(T));
    make_document(w.path(), TODAY, "day:c1-today", PROPOSAL, at(T));
    set_field(
        w.path(),
        TODAY,
        "p1_must",
        json!(["Write the letter", "Write the letter",
            {"id":"has a space","text":"Call the harbour master"}, {"id":"words:forged","text":"Forged"}]),
        "set:p1",
        at(T),
    );
    let item = |input: Value| part(w.path(), input, at(T)).unwrap();
    let first = item(json!({"field_id":"p1_must","index":0}));
    let second = item(json!({"field_id":"p1_must","index":1}));
    assert_ne!(
        first["part"]["id"], second["part"]["id"],
        "two items, two parts"
    );
    assert_ne!(first["entry"]["item"], second["entry"]["item"]);
    for index in 0..4 {
        let by_index = item(json!({"field_id":"p1_must","index":index}));
        let identity = by_index["entry"]["item"].clone();
        assert_eq!(
            item(json!({"field_id":"p1_must","item_id":identity}))["part"]["id"],
            by_index["part"]["id"],
            "{identity} selects item {index}"
        );
    }
    let spaced = item(json!({"field_id":"p1_must","index":2}))["entry"]["item"].clone();
    assert!(
        !spaced.as_str().unwrap().contains(' '),
        "{spaced}: a form item_id takes"
    );
    let forged = item(json!({"field_id":"p1_must","index":3}))["entry"]["item"].clone();
    assert!(
        !forged.as_str().unwrap().starts_with("words:"),
        "{forged}: an own id never poses as a digest of words"
    );
}

fn listing(dir: &Path) -> Vec<(String, String)> {
    let mut rows: Vec<_> = fs::read_dir(dir)
        .unwrap()
        .map(|entry| {
            let path = entry.unwrap().path();
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            let content = fs::read_to_string(&path).unwrap_or_default();
            (name, content)
        })
        .collect();
    rows.sort();
    rows
}

#[test]
fn bringing_an_entry_to_a_star_is_the_persons_press_and_writes_nothing_into_any_day() {
    let w = world("Europe/London", 0);
    ensure(w.path(), at(T));
    make_document(w.path(), TODAY, "day:c1-today", PROPOSAL, at(T));
    set_field(
        w.path(),
        TODAY,
        "p0_quick_thoughts",
        json!("Bring this to the star."),
        "set:p0",
        at(T),
    );
    let input = json!({"day_ref":TODAY,"field_id":"p0_quick_thoughts"});
    for token in [None, Some(AGENT)] {
        assert_eq!(
            kind_of(run(w.path(), "day_entry_part", input.clone(), token, at(T))),
            io::ErrorKind::PermissionDenied,
            "{token:?}: it is the person's press"
        );
    }
    let day_dir = w.path().join("Control/user/day/2026-10-20");
    let before = listing(&day_dir);
    let made = part(w.path(), input.clone(), at(T)).unwrap();
    assert_eq!(listing(&day_dir), before, "nothing is written into the Day");
    let salt = fs::read_to_string(w.path().join(".central/day-part-salt.json")).unwrap();
    let day = read(w.path(), "day_read", json!({"day_ref":TODAY}), at(T));
    run(
        w.path(),
        "day_lifecycle",
        json!({"day_ref":TODAY,"expected_revision":day["revision"]["revision"],
            "expected_relations_revision":day["relations_revision"],"lifecycle":"closed"}),
        Some(HUMAN),
        at(T),
    )
    .unwrap();
    let closed = listing(&day_dir);
    assert!(made["part"]["id"].is_string());
    assert_eq!(
        words_of(part(w.path(), input, at(T))),
        "The Day of 2026-10-20 is closed; a closed Day is kept, and none of its entries is shown from it",
        "a closed Day shows none of its entries (DLW build ruling 2026-11-09)"
    );
    assert_eq!(
        listing(&day_dir),
        closed,
        "a closed Day's folder is never touched"
    );
    assert_eq!(
        fs::read_to_string(w.path().join(".central/day-part-salt.json")).unwrap(),
        salt,
        "one salt for the world, written once"
    );
}

#[test]
fn source_history_names_the_days_field_source_as_what_recorded_a_field_record_not_the_person() {
    let w = world("Europe/London", 0);
    ensure(w.path(), at(T));
    let item = reached(
        "saying:history",
        "to-you",
        ms("2026-10-20T09:10:00Z"),
        "Who wrote this?",
    );
    run(w.path(), "day_field_record", item, Some(HUMAN), at(T)).unwrap();
    let horizon: Value = serde_json::from_slice(
        &fs::read(w.path().join(".central/source-change-control.json")).unwrap(),
    )
    .unwrap();
    let change = horizon["changes"]
        .as_array()
        .unwrap()
        .iter()
        .rev()
        .find(|change| change["source_path"] == "Control/user/day/2026-10-20/field.jsonl")
        .unwrap_or_else(|| panic!("no change of field.jsonl: {horizon}"));
    assert_eq!(change["actor"], "day:field-source", "{change}");
    assert_eq!(change["actor_kind"], "native-service", "{change}");
}

// ---- central-be1d: the delta review of #326 (DLW build ruling 2026-11-09) -------------------------------------

fn p1_must(root: &Path, value: Value, request: &str) {
    set_field(root, TODAY, "p1_must", value, request, at(T));
}

#[test]
fn an_item_with_no_id_keeps_its_part_only_while_the_identical_items_before_it_stand_and_says_so_when_they_change(
) {
    let w = world("Europe/London", 0);
    ensure(w.path(), at(T));
    make_document(w.path(), TODAY, "day:c1-today", PROPOSAL, at(T));
    p1_must(
        w.path(),
        json!(["Write the letter", "Write the letter"]),
        "set:twins",
    );
    let item = |input: Value| part(w.path(), input, at(T));
    let first = item(json!({"field_id":"p1_must","index":0})).unwrap();
    let second = item(json!({"field_id":"p1_must","index":1})).unwrap();
    let second_id = second["entry"]["item"].as_str().unwrap().to_owned();
    // The first twin goes: the second is named by its place among them anew.
    p1_must(w.path(), json!(["Write the letter"]), "set:one-twin");
    let moved = words_of(item(json!({"field_id":"p1_must","item_id":second_id})));
    assert!(
        moved.contains(&format!("has no item {second_id} now")),
        "{moved}"
    );
    assert!(
        moved.contains("an identical item before it changed or went (1 now hold those words)"),
        "{moved}"
    );
    assert!(
        moved.contains("The part it was shown as still holds its words"),
        "{moved}"
    );
    let survivor = item(json!({"field_id":"p1_must","index":0})).unwrap();
    assert_eq!(
        survivor["part"]["id"], first["part"]["id"],
        "the survivor stands in the first twin's place, so it is that part, and showing it again is already shown"
    );
    assert_eq!(survivor["part"]["label"], first["part"]["label"]);
    // An item with its own id keeps its part however the list moves.
    p1_must(
        w.path(),
        json!([{"id":"t1","text":"Call"}, {"id":"t2","text":"Write"}]),
        "set:ids",
    );
    let own = item(json!({"field_id":"p1_must","item_id":"t2"})).unwrap();
    p1_must(
        w.path(),
        json!([{"id":"t2","text":"Write"}]),
        "set:ids-moved",
    );
    assert_eq!(
        item(json!({"field_id":"p1_must","item_id":"t2"})).unwrap()["part"]["id"],
        own["part"]["id"]
    );
    // An unknown id is still said plainly as no item.
    assert!(
        words_of(item(json!({"field_id":"p1_must","item_id":"t9"}))).ends_with("has no item t9")
    );
}

#[test]
fn a_list_whose_items_repeat_an_id_is_refused_naming_the_id() {
    let w = world("Europe/London", 0);
    ensure(w.path(), at(T));
    make_document(w.path(), TODAY, "day:c1-today", PROPOSAL, at(T));
    p1_must(
        w.path(),
        json!([{"id":"t1","text":"Call"}, {"id":"t1","text":"Write"}, "Rest"]),
        "set:dup",
    );
    for input in [
        json!({"field_id":"p1_must","index":0}),
        json!({"field_id":"p1_must","item_id":"t1"}),
        json!({"field_id":"p1_must","index":2}),
    ] {
        let refused = words_of(part(w.path(), input.clone(), at(T)));
        assert!(
            refused.contains("p1_must holds more than one item with the id t1"),
            "{input}: {refused}"
        );
    }
}

/// The sha256 of `invisible-code-points.txt`, the list O-I keeps beside its own
/// tests too (`packages/live-shell/ui/tests/fixtures/`), byte for byte: a list
/// that moves on one side without the other's sha256 changing fails here or
/// there (DLW build ruling 2026-11-09).
const INVISIBLE_LIST_SHA256: &str =
    "cac89ddc2b2ce933f7bf2763663a4e94703da3cb501ef34484cc85ee4000e491";

#[test]
fn the_invisible_set_is_every_format_character_the_separators_and_the_controls_as_the_shared_list_names_them(
) {
    // A golden file on its own: it means something because O-I derives the
    // same list from another engine (ICU), and Python's unicodedata agrees.
    let list = include_str!("invisible-code-points.txt");
    assert_eq!(
        format!("{:x}", Sha256::digest(list.as_bytes())),
        INVISIBLE_LIST_SHA256,
        "the list is the one O-I holds"
    );
    let listed: Vec<u32> = list
        .lines()
        .map(|line| u32::from_str_radix(line, 16).unwrap())
        .collect();
    let derived: Vec<u32> = (0..=0x10FFFF)
        .filter_map(char::from_u32)
        .filter(|&c| c.is_control() || super::day_moves::invisible(c))
        .map(u32::from)
        .collect();
    assert_eq!(
        derived, listed,
        "the controls, the line and paragraph separators and every format character, derived from the general category"
    );
    for c in [
        '\u{FE0E}',
        '\u{FE0F}',
        '\u{E0100}',
        '\u{E01EF}',
        '\u{180B}',
        '\u{180D}',
    ] {
        assert!(
            !super::day_moves::invisible(c),
            "a variation selector chooses how a character is drawn, and stays: {:X}",
            u32::from(c)
        );
    }
    // be2i ruling 2: what draws as blank is invisible too, and the whole tag
    // block, assigned or not
    for c in [
        '\u{3164}',
        '\u{FFA0}',
        '\u{115F}',
        '\u{1160}',
        '\u{2800}',
        '\u{034F}',
        '\u{E0000}',
        '\u{E0002}',
        '\u{E001F}',
        '\u{E0001}',
        '\u{E007F}',
    ] {
        assert!(
            super::day_moves::invisible(c),
            "it draws nothing: {:X}",
            u32::from(c)
        );
    }
    // be2i ruling 4: a format character that draws a sign is not invisible;
    // the joiners stay in the set, as ruled
    for c in [
        '\u{0600}',
        '\u{0601}',
        '\u{0602}',
        '\u{0603}',
        '\u{0604}',
        '\u{0605}',
        '\u{06DD}',
        '\u{0890}',
        '\u{0891}',
        '\u{08E2}',
        '\u{110BD}',
        '\u{110CD}',
    ] {
        assert!(
            !super::day_moves::invisible(c),
            "it draws a sign, so text keeps it: {:X}",
            u32::from(c)
        );
    }
    assert!(super::day_moves::invisible('\u{200C}') && super::day_moves::invisible('\u{200D}'));
}

#[test]
fn invisible_characters_and_a_lone_carriage_return_are_refused_once_the_kept_records_are_looked_up()
{
    let w = world("Europe/London", 0);
    ensure(w.path(), at(T));
    let at_ms = ms("2026-10-20T09:10:00Z");
    let invisible = [
        '\u{2028}',
        '\u{2029}',
        '\u{202A}',
        '\u{202E}',
        '\u{2066}',
        '\u{2069}',
        '\u{200B}',
        '\u{200D}',
        '\u{2060}',
        '\u{FEFF}',
        '\u{200E}',
        '\u{200F}',
        '\u{061C}',
        '\u{E0001}',
        '\u{E0041}',
        '\u{E007F}',
        // every format character, not only those named by hand (DLW build ruling 2026-11-09)
        '\u{00AD}',
        '\u{180E}',
        '\u{2061}',
        '\u{2064}',
        '\u{206A}',
        '\u{206F}',
        '\u{FFF9}',
        '\u{FFFB}',
        '\u{1D173}',
        // what draws as blank, and the tag block's unassigned points (be2i ruling 2)
        '\u{3164}',
        '\u{FFA0}',
        '\u{115F}',
        '\u{1160}',
        '\u{2800}',
        '\u{034F}',
        '\u{E0000}',
        '\u{E001F}',
    ];
    let mut list = Vec::new();
    for (index, c) in invisible.iter().enumerate() {
        let mut named = reached(&format!("saying:name-{index}"), "to-you", at_ms, "Hello.");
        named["from"]["name"] = json!(format!("ma{c}ra"));
        list.push(named);
        list.push(reached(
            &format!("saying:words-{index}"),
            "to-you",
            at_ms,
            &format!("Which{c}plate?"),
        ));
    }
    let mut starred = reached("saying:star", "star", at_ms, "Hello.");
    starred["star"]["name"] = json!("The \u{202E}Rosarium");
    list.push(starred);
    list.push(reached(
        "saying:lone-cr",
        "to-you",
        at_ms,
        "Which plate?\rThe second.",
    ));
    list.push(reached(
        "saying:crlf",
        "to-you",
        at_ms,
        "Which plate?\r\nThe second.\r\n",
    ));
    // A records call names at most 64 things, so the list goes in turns of 32.
    let mut results = Vec::new();
    for turn in list.chunks(32) {
        let answers = records(w.path(), turn.to_vec(), at(T)).unwrap();
        results.extend(answers["results"].as_array().unwrap().iter().cloned());
    }
    let (refused, made) = results.split_at(results.len() - 1);
    assert_eq!(refused.len(), 2 * invisible.len() + 2);
    for result in refused {
        let words = result["refused"].as_str().unwrap_or_default();
        assert!(
            words.contains("invisible") && words.contains("control"),
            "{result}"
        );
        let named = result["item_ref"].as_str().unwrap();
        let what = if named.contains("name-") {
            "from.name"
        } else if named.contains("star") {
            "star.name"
        } else {
            "words"
        };
        assert!(words.starts_with(what), "{result}");
    }
    assert_eq!(made[0]["created"], true, "a \\r\\n is a line break");

    // What #325 kept stays kept: its retry answers kept, whatever is refused now.
    let kept = reached("saying:kept-before", "to-you", at_ms, "Kept.");
    record(w.path(), kept.clone(), HOST, at(T)).unwrap();
    let path = field_path(w.path(), "2026-10-20");
    let raw = fs::read_to_string(&path).unwrap();
    let line = raw
        .lines()
        .find(|line| line.contains("saying:kept-before"))
        .unwrap();
    assert!(line.contains("\"name\":\"mara\""), "{line}");
    fs::write(
        &path,
        raw.replace(
            line,
            &line.replace("\"name\":\"mara\"", "\"name\":\"ma\\u001bra\""),
        ),
    )
    .unwrap();
    let mut retry = kept;
    retry["from"]["name"] = json!("ma\u{1b}ra");
    assert_eq!(
        record(w.path(), retry.clone(), HOST, at(T)).unwrap()["created"],
        false,
        "the single form"
    );
    assert_eq!(
        records(w.path(), vec![retry], at(T)).unwrap()["results"][0]["created"],
        false,
        "a batch"
    );
}

#[test]
fn close_at_day_start_in_a_spring_forward_gap_closes_the_day_that_ends_there() {
    // Europe/London with the Day boundary at 01:30: on 2026-03-29 01:30 never stands on the wall. The boundary's
    // occurrence resolves forward to 02:30 BST (01:30Z), half an hour after the date flipped at 01:00Z, so the instant
    // before it is the 29th's already: Close at Day start closes the 28th, the Day that ended at the flip.
    let w = world("Europe/London", 90);
    ensure(w.path(), at("2026-03-28T12:00:00Z"));
    save(
        w.path(),
        json!({"moves":{"close":{"runs_at":{"kind":"day_start"}}}}),
        at("2026-03-28T12:00:00Z"),
    );
    let due = read(
        w.path(),
        "day_moves_due",
        json!({"since_unix_ms":ms("2026-03-28T12:00:00Z"),"now_unix_ms":ms("2026-03-29T12:00:00Z")}),
        at("2026-03-29T12:00:00Z"),
    );
    let close: Vec<&Value> = due["due"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|item| item["move"] == "close")
        .collect();
    assert_eq!(close.len(), 1, "{due}");
    assert_eq!(
        close[0]["due_unix_ms"],
        ms("2026-03-29T01:30:00Z"),
        "resolved forward, as time.occurrences does"
    );
    assert_eq!(close[0]["civil_date"], "2026-03-28", "{due}");
}

#[test]
fn the_field_reading_answers_with_no_spans_and_a_note_where_the_days_date_cannot_be_named() {
    let w = world("Europe/London", 0);
    ensure(w.path(), at(T));
    record(
        w.path(),
        reached("saying:kept", "to-you", ms("2026-10-20T09:10:00Z"), "Kept?"),
        HOST,
        at(T),
    )
    .unwrap();
    let policy = w.path().join("Control/user/time.json");
    let mut value: Value = serde_json::from_slice(&fs::read(&policy).unwrap()).unwrap();
    value["timezone"] = json!("Nowhere/Never");
    fs::write(&policy, serde_json::to_vec(&value).unwrap()).unwrap();
    let reading = read(w.path(), "day_field_read", json!({}), at(T));
    let takes = &reading["takes"];
    assert_eq!(takes["spans"], json!([]), "{reading}");
    assert!(
        takes["from_unix_ms"].is_null() && takes["until_unix_ms"].is_null(),
        "{takes}"
    );
    assert!(
        takes["note"].as_str().unwrap().starts_with(
            "The Day's own date cannot be named now, so it takes nothing from the field"
        ),
        "{takes}"
    );
    assert_eq!(
        reading["items"].as_array().unwrap().len(),
        1,
        "what it keeps still reads"
    );
}

#[test]
fn a_part_salt_that_cannot_be_read_names_its_file_and_the_way_on() {
    let w = world("Europe/London", 0);
    ensure(w.path(), at(T));
    make_document(w.path(), TODAY, "day:c1-today", PROPOSAL, at(T));
    set_field(
        w.path(),
        TODAY,
        "p0_quick_thoughts",
        json!("Bring this."),
        "set:p0",
        at(T),
    );
    let salt = w.path().join(".central/day-part-salt.json");
    fs::create_dir_all(salt.parent().unwrap()).unwrap();
    for (raw, what) in [
        ("not json", "is not JSON"),
        (
            "{\"schema\":\"central.day-part-salt/v1\",\"salt\":\"short\"}",
            "is not a central.day-part-salt/v1",
        ),
    ] {
        fs::write(&salt, raw).unwrap();
        let refused = words_of(part(
            w.path(),
            json!({"field_id":"p0_quick_thoughts"}),
            at(T),
        ));
        assert!(
            refused.contains(&format!(".central/day-part-salt.json {what}")),
            "{refused}"
        );
        assert!(
            refused.contains("Move it aside and press again"),
            "{refused}"
        );
    }
}

#[test]
fn a_day_in_a_half_hour_zone_across_its_fold_takes_only_its_own_spans() {
    // Australia/Lord_Howe shifts by half an hour: on 2026-04-05 the wall goes back from 02:00 (+11:00) to 01:30
    // (+10:30). With the Day boundary at 01:45, the wall reads 01:45 twice, at 14:45Z and at 15:15Z on the 4th: the
    // Day of the 5th holds two spans, and the repeated quarter hour between the change (15:00Z) and 15:15Z is the 4th's.
    let w = world("Australia/Lord_Howe", 105);
    let now = at("2026-04-05T02:00:00Z");
    assert_eq!(
        ensure(w.path(), now)["day_ref"],
        "central:day:control:root:2026-04-05"
    );
    let takes = takes_of(w.path(), now);
    assert_eq!(
        takes["spans"],
        json!([
            span("2026-04-04T14:45:00Z", "2026-04-04T15:00:00Z"),
            span("2026-04-04T15:15:00Z", "2026-04-05T15:15:00Z")
        ])
    );
    assert_eq!(takes["from_unix_ms"], ms("2026-04-04T14:45:00Z"));
    assert_eq!(takes["until_unix_ms"], ms("2026-04-05T15:15:00Z"));
    let answers = records(
        w.path(),
        vec![
            said("first-start", ms("2026-04-04T14:45:00Z")),
            said("at-change", ms("2026-04-04T15:00:00Z")),
            said("in-the-repeat", ms("2026-04-04T15:14:59Z")),
            said("second-start", ms("2026-04-04T15:15:00Z")),
        ],
        now,
    )
    .unwrap();
    assert_eq!(
        created_of(&answers),
        [json!(true), json!(false), json!(false), json!(true)],
        "{answers}"
    );
    assert!(answers["results"][1]["refused"]
        .as_str()
        .unwrap()
        .contains("on 2026-04-04"));
}

#[test]
fn the_earlier_day_across_an_autumn_fold_takes_the_repeated_half_hour() {
    // Europe/London with the Day boundary at 01:30: the Day of 2026-10-24 runs from 00:30Z on the 24th to the 25th's
    // first 01:30 (00:30Z), and again through the repeated half hour, from the change (01:00Z) to the second 01:30.
    let w = world("Europe/London", 90);
    let now = at("2026-10-24T12:00:00Z");
    assert_eq!(
        ensure(w.path(), now)["day_ref"],
        "central:day:control:root:2026-10-24"
    );
    let takes = takes_of(w.path(), now);
    assert_eq!(
        takes["spans"],
        json!([
            span("2026-10-24T00:30:00Z", "2026-10-25T00:30:00Z"),
            span("2026-10-25T01:00:00Z", "2026-10-25T01:30:00Z")
        ])
    );
    let answers = records(
        w.path(),
        vec![
            said("first-of-the-25th", ms("2026-10-25T00:45:00Z")),
            said("in-the-repeat", ms("2026-10-25T01:15:00Z")),
            said("second-of-the-25th", ms("2026-10-25T01:30:00Z")),
        ],
        at("2026-10-25T02:00:00Z"),
    )
    .unwrap();
    assert_eq!(
        created_of(&answers),
        [json!(false), json!(true), json!(false)],
        "{answers}"
    );
}

#[test]
fn a_day_closed_before_the_first_press_is_refused_never_touched_and_no_salt_is_written() {
    let w = world("Europe/London", 0);
    ensure(w.path(), at(T));
    make_document(w.path(), TODAY, "day:c1-today", PROPOSAL, at(T));
    set_field(
        w.path(),
        TODAY,
        "p0_quick_thoughts",
        json!("Bring this."),
        "set:p0",
        at(T),
    );
    let day = read(w.path(), "day_read", json!({"day_ref":TODAY}), at(T));
    run(
        w.path(),
        "day_lifecycle",
        json!({"day_ref":TODAY,"expected_revision":day["revision"]["revision"],
            "expected_relations_revision":day["relations_revision"],"lifecycle":"closed"}),
        Some(HUMAN),
        at(T),
    )
    .unwrap();
    let salt = w.path().join(".central/day-part-salt.json");
    assert!(!salt.exists(), "no part was read yet");
    let day_dir = w.path().join("Control/user/day/2026-10-20");
    let closed = listing(&day_dir);
    let refused = part(
        w.path(),
        json!({"day_ref":TODAY,"field_id":"p0_quick_thoughts"}),
        at(T),
    );
    assert_eq!(kind_of(refused), io::ErrorKind::AlreadyExists);
    assert!(
        !salt.exists(),
        "a press on a closed Day is refused before anything is written, the world's salt too (DLW build ruling 2026-11-09)"
    );
    assert_eq!(listing(&day_dir), closed, "and nothing into the closed Day");
}

/// No writer in ctrl closes a Day outside the source lock (the test below
/// proves the press holds it). This one does, through
/// `temporal::change_lifecycle` directly, between the press's lookup and its
/// reading: the press reads lifecycle, date and revision from the one reading
/// its words come from, so it sees the close (be2i ruling 5; the review's
/// mutant M5, a lifecycle read before the lookup, is red here).
#[test]
fn a_close_written_outside_the_source_lock_between_the_press_lookup_and_its_reading_is_seen_on_the_one_reading(
) {
    let w = world("Europe/London", 0);
    ensure(w.path(), at(T));
    make_document(w.path(), TODAY, "day:c1-today", PROPOSAL, at(T));
    set_field(
        w.path(),
        TODAY,
        "p0_quick_thoughts",
        json!("Bring this."),
        "set:p0",
        at(T),
    );
    let day = read(w.path(), "day_read", json!({"day_ref":TODAY}), at(T));
    let offered = day["revision"]["revision"].clone();
    // The close moves the relation only, so the revision the entry was
    // offered from still stands.
    let root = w.path().to_path_buf();
    let (revision, relations) = (
        offered.as_str().unwrap().to_owned(),
        day["relations_revision"].as_str().unwrap().to_owned(),
    );
    super::day_moves::BETWEEN_PART_READS.with(|slot| {
        *slot.borrow_mut() = Some(Box::new(move || {
            let scope = Scope::resolve(&root, None).unwrap();
            temporal::change_lifecycle(
                &scope,
                TODAY,
                &revision,
                &relations,
                "closed",
                "test:closer",
                at(T),
            )
            .unwrap();
        }));
    });
    let words = words_of(part(
        w.path(),
        json!({"day_ref":TODAY,"field_id":"p0_quick_thoughts","expected_revision":offered}),
        at(T),
    ));
    assert!(
        words.contains("is closed"),
        "the press reads lifecycle, date and revision from the one reading its words come from (DLW build ruling 2026-11-09): {words}"
    );
    let after = read(w.path(), "day_read", json!({"day_ref":TODAY}), at(T));
    assert_eq!(after["temporal"]["lifecycle"], "closed");
    assert_eq!(
        after["revision"]["revision"], offered,
        "a close moves no revision, so expected_revision alone never sees it"
    );
}

#[test]
fn a_close_pressed_while_the_press_reads_its_day_waits_under_the_source_lock_and_the_next_press_sees_it(
) {
    let w = world("Europe/London", 0);
    ensure(w.path(), at(T));
    make_document(w.path(), TODAY, "day:c1-today", PROPOSAL, at(T));
    set_field(
        w.path(),
        TODAY,
        "p0_quick_thoughts",
        json!("Bring this."),
        "set:p0",
        at(T),
    );
    let day = read(w.path(), "day_read", json!({"day_ref":TODAY}), at(T));
    let offered = day["revision"]["revision"].clone();
    // Another hand closes the Day through Central's own dispatch while the
    // press is between its lookup and the Day's reading. Every operation runs
    // under the source lock, the press and the close alike (DLW build ruling
    // 2026-11-09), so the close waits for the press: it cannot land between.
    let root = w.path().to_path_buf();
    let close = json!({"day_ref":TODAY,"expected_revision":offered,
        "expected_relations_revision":day["relations_revision"],"lifecycle":"closed"});
    let closer = std::rc::Rc::new(std::cell::RefCell::new(None));
    let started = closer.clone();
    super::day_moves::BETWEEN_PART_READS.with(|slot| {
        *slot.borrow_mut() = Some(Box::new(move || {
            assert!(
                crate::source_safety::lock_within(
                    &root,
                    "source-mutation.lock",
                    std::time::Duration::ZERO
                )
                .unwrap()
                .is_none(),
                "the press holds the source lock between its lookup and its reading"
            );
            // That `lock_within` is the deterministic proof (be2i ruling 8):
            // the mutant that skips the lock for the press is red there. The
            // wait below is a timing check that can only go green: it never
            // fails by chance, and on a slow enough runner it would pass with
            // no lock at all.
            let handle = std::thread::spawn(move || {
                execute_with_token_at(&root, "day_lifecycle", &close, Some(HUMAN), at(T))
            });
            std::thread::sleep(std::time::Duration::from_millis(200));
            assert!(!handle.is_finished(), "the close waits for the press");
            *started.borrow_mut() = Some(handle);
        }));
    });
    let shown = part(
        w.path(),
        json!({"day_ref":TODAY,"field_id":"p0_quick_thoughts","expected_revision":offered}),
        at(T),
    )
    .expect("the press reads the Day it looked up, still open");
    assert_eq!(shown["part"]["label"], "Bring this.");
    let handle = closer
        .borrow_mut()
        .take()
        .expect("the close was pressed during the press");
    handle
        .join()
        .unwrap()
        .expect("the close lands once the press is done");
    let after = read(w.path(), "day_read", json!({"day_ref":TODAY}), at(T));
    assert_eq!(after["temporal"]["lifecycle"], "closed");
    assert_eq!(
        after["revision"]["revision"], offered,
        "a close moves no revision, so expected_revision alone never sees it"
    );
    let words = words_of(part(
        w.path(),
        json!({"day_ref":TODAY,"field_id":"p0_quick_thoughts","expected_revision":offered}),
        at(T),
    ));
    assert!(
        words.contains("is closed"),
        "the next press reads the lifecycle from the reading its words come from, not from the revision: {words}"
    );
}

#[test]
fn each_refusal_of_the_persons_own_act_opens_with_whose_act_it_is_then_its_cause() {
    let w = world("Europe/London", 0);
    ensure(w.path(), at(T));
    make_document(w.path(), TODAY, "day:c1-today", PROPOSAL, at(T));
    set_field(
        w.path(),
        TODAY,
        "p0_quick_thoughts",
        json!("Bring this."),
        "set:p0",
        at(T),
    );
    let input = json!({"day_ref":TODAY,"field_id":"p0_quick_thoughts"});
    let unknown = "an-unknown-token-that-no-grant-holds-0000";
    let press = |token: Option<&str>| {
        words_of(run(w.path(), "day_entry_part", input.clone(), token, at(T)))
    };
    let agent = press(Some(AGENT));
    assert_eq!(
        agent,
        "Showing an entry is the person's own press: this credential is agent:test's, a credential of kind agent, not the person's"
    );
    for (refused, cause) in [
        (press(None), "CENTRAL_NATIVE_TOKEN"),
        (press(Some(unknown)), "not authorised"),
    ] {
        assert!(
            refused.starts_with("Showing an entry is the person's own press: "),
            "{refused}"
        );
        assert!(refused.contains(cause), "{refused}");
    }
    let item = reached(
        "saying:press",
        "to-you",
        ms("2026-10-20T09:10:00Z"),
        "Hello.",
    );
    for token in [None, Some(unknown)] {
        let refused = words_of(run(
            w.path(),
            "day_field_record",
            item.clone(),
            token,
            at(T),
        ));
        assert!(
            refused.starts_with(
                "Recording what reached you is the person's own Field setting at work: "
            ),
            "{refused}"
        );
    }
}

// ---- central-be1e: the press shows exactly what was offered (DLW build ruling 2026-11-09) ----------------------

#[test]
fn the_press_reads_the_entry_only_on_the_revision_it_was_offered_from() {
    let w = world("Europe/London", 0);
    ensure(w.path(), at(T));
    make_document(w.path(), TODAY, "day:c1-today", PROPOSAL, at(T));
    set_field(
        w.path(),
        TODAY,
        "p0_quick_thoughts",
        json!("As it was offered."),
        "set:p0",
        at(T),
    );
    let offered_on =
        read(w.path(), "day_read", json!({"day_ref":TODAY}), at(T))["revision"]["revision"].clone();
    set_field(
        w.path(),
        TODAY,
        "p0_quick_thoughts",
        json!("Changed after the offer."),
        "set:p0:again",
        at(T),
    );
    let press = |input: Value| run(w.path(), "day_entry_part", input, Some(HUMAN), at(T));
    let moved = press(
        json!({"day_ref":TODAY,"field_id":"p0_quick_thoughts","expected_revision":offered_on}),
    );
    assert_eq!(
        words_of(moved),
        "The Day changed since this was offered; look again."
    );
    assert_eq!(
        kind_of(press(
            json!({"day_ref":TODAY,"field_id":"p0_quick_thoughts","expected_revision":offered_on})
        )),
        io::ErrorKind::AlreadyExists
    );
    assert!(
        !w.path().join(".central/day-part-salt.json").exists(),
        "a refused press writes nothing, the world's salt neither"
    );
    assert!(
        words_of(press(
            json!({"day_ref":TODAY,"field_id":"p0_quick_thoughts"})
        ))
        .contains("expected_revision"),
        "the press names the revision it was offered from"
    );
    assert_eq!(
        part(w.path(), json!({"field_id":"p0_quick_thoughts"}), at(T)).unwrap()["part"]["label"],
        "Changed after the offer.",
        "on the revision it stands at, it is read as it stands"
    );
}

#[test]
fn only_the_open_day_central_names_as_today_shows_its_entries() {
    let w = world("Europe/London", 0);
    ensure(w.path(), at(T));
    make_document(w.path(), TODAY, "day:c1-today", PROPOSAL, at(T));
    set_field(
        w.path(),
        TODAY,
        "p0_quick_thoughts",
        json!("A thought of the 20th."),
        "set:p0",
        at(T),
    );
    let time = read(w.path(), "time_policy", json!({}), at(T));
    run(
        w.path(),
        "day_rollover",
        json!({"expected_time_policy_revision":time["revision"]}),
        Some(HUMAN),
        at(T),
    )
    .unwrap();
    assert_eq!(
        words_of(part(
            w.path(),
            json!({"day_ref":TOMORROW,"field_id":"p0_quick_thoughts"}),
            at(T)
        )),
        "The Day of 2026-10-21 is not today: Central's today is 2026-10-20, and only today's open Day shows its entries",
        "tomorrow's Day, open beside today's, is not today"
    );
    assert!(part(w.path(), json!({"field_id":"p0_quick_thoughts"}), at(T)).is_ok());
    // Past the boundary the Day of the 20th still stands open, and Central's pointer still names it until today's Day is
    // ensured: it is not today, by day_ref or by the pointer.
    let after = at("2026-10-21T08:00:00Z");
    for input in [
        json!({"day_ref":TODAY,"field_id":"p0_quick_thoughts"}),
        json!({"field_id":"p0_quick_thoughts"}),
    ] {
        assert_eq!(
            words_of(part(w.path(), input.clone(), after)),
            "The Day of 2026-10-20 is not today: Central's today is 2026-10-21, and only today's open Day shows its entries",
            "{input}"
        );
    }
}

#[test]
fn a_label_reads_as_it_is_and_a_field_that_holds_no_text_offers_none() {
    let w = world("Europe/London", 0);
    ensure(w.path(), at(T));
    make_document(w.path(), TODAY, "day:c1-today", PROPOSAL, at(T));
    set_field(
        w.path(),
        TODAY,
        "p0_quick_thoughts",
        json!("Read\u{7}the\u{200B} pla\u{AD}te\u{2028}now\u{202E}, \u{2066}he\u{200E}r\u{200F}e\u{061C}\u{2069}.\u{FEFF}\u{E0001}\u{E0067}\u{E007F}\u{2061}\u{180E}\u{FFF9}"),
        "set:p0",
        at(T),
    );
    set_field(
        w.path(),
        TODAY,
        "p1_must",
        json!([{"text":"Call\u{200D} the\u{1B} harbour \u{2764}\u{FE0F}"}, {"text":"An item as an object"}]),
        "set:p1",
        at(T),
    );
    append_contribution(
        w.path(),
        TODAY,
        "p0_quick_thoughts",
        "c:marks",
        "<p>A\u{2060} line\u{2029}apart</p>",
        at(T),
    );
    let label = |input: Value| part(w.path(), input, at(T)).unwrap()["part"]["label"].clone();
    assert_eq!(
        label(json!({"field_id":"p0_quick_thoughts"})),
        "Read the plate now, here.",
        "a control character or a separator stands as a space; a format character is left out"
    );
    assert_eq!(
        label(json!({"field_id":"p1_must","index":0})),
        "Call the harbour \u{2764}\u{FE0F}",
        "a variation selector is no format character: the emoji keeps its presentation"
    );
    assert_eq!(
        label(json!({"field_id":"p1_must","index":1})),
        "An item as an object",
        "a list item is read by its text"
    );
    assert_eq!(label(json!({"contribution_id":"c:marks"})), "A line apart");
    set_field(
        w.path(),
        TODAY,
        "p0_adjacencies",
        json!({"text":"An object in a field of its own"}),
        "set:p0a",
        at(T),
    );
    assert!(
        words_of(part(w.path(), json!({"field_id":"p0_adjacencies"}), at(T)))
            .contains("holds no words to show"),
        "a field's own value is read only as text: the World offers no other"
    );
}

/// be2j.c4 item 2 (DLW build ruling 2026-11-12): an end for a run that has ended already is answered as ended whatever
/// else the end holds or lacks: no state, a state that is none, a time before the begin. The same end for a run still
/// running is refused as invalid, in its own words.
#[test]
fn an_end_with_no_state_for_an_ended_run_is_answered_as_ended_and_for_a_running_one_as_invalid() {
    let w = world("Europe/London", 0);
    ensure(w.path(), at(T));
    let began = ms("2026-10-20T09:00:05Z");
    let run_ref = begin(w.path(), "context", began, HOST, at(T)).unwrap()["run"]["run_ref"].clone();
    let lacking = || {
        [
            json!({"event":"end","run_ref":run_ref,"at_unix_ms":began + 60_000}),
            json!({"event":"end","run_ref":run_ref,"at_unix_ms":began + 60_000,"state":"finished"}),
            json!({"event":"end","run_ref":run_ref,"at_unix_ms":began - 1,"state":"done"}),
        ]
    };
    for input in lacking() {
        let error = run(
            w.path(),
            "day_move_record",
            input.clone(),
            Some(HOST),
            at(T),
        )
        .expect_err("a running run's end that lacks its state or its time is refused");
        assert_eq!(
            error.kind(),
            io::ErrorKind::InvalidInput,
            "{input}: {error}"
        );
    }
    end(w.path(), &run_ref, began + 60_000, "stalled", HOST, at(T)).unwrap();
    for input in lacking() {
        let error = run(
            w.path(),
            "day_move_record",
            input.clone(),
            Some(HOST),
            at(T),
        )
        .expect_err("an ended run takes no end");
        assert_eq!(
            error.kind(),
            io::ErrorKind::AlreadyExists,
            "{input}: {error}"
        );
        let refusal = *action_result("central.day.move.record", Err(error))
            .error
            .expect("a refusal");
        assert_eq!(refusal.code, "run_ended_already", "{input}");
        assert_eq!(
            refusal.details.unwrap()["run"]["state"],
            "stalled",
            "{input}"
        );
    }
}

/// be2j.c3 (DLW build ruling 2026-11-11): an end given to a run that has ended
/// already answers with a code of its own and the run as it ended, so a caller
/// knows the run is finished without reading the Day again. Every other
/// conflict keeps the code it had: a move still running, a closed Day, and a
/// write on a Day whose record changed after it was read.
#[test]
fn an_end_for_an_ended_run_has_its_own_code_and_other_conflicts_keep_theirs() {
    let w = world("Europe/London", 0);
    let day = ensure(w.path(), at(T));
    let first = begin(w.path(), "context", ms("2026-10-20T09:00:05Z"), HOST, at(T)).unwrap();
    let run_ref = first["run"]["run_ref"].clone();
    end(
        w.path(),
        &run_ref,
        ms("2026-10-20T09:01:00Z"),
        "stalled",
        HOST,
        at(T),
    )
    .unwrap();
    let kept = fs::read_to_string(log_path(w.path(), "2026-10-20")).unwrap();
    const ACTION: &str = "central.day.move.record";
    const CONFLICT: &str = "stale_basis_or_identity_conflict";
    let refused = |result: io::Result<Value>| {
        let error = result.expect_err("the operation must refuse");
        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
        let result = action_result(ACTION, Err(error));
        assert_eq!(result.status, ResultStatus::VerificationFailure);
        assert_eq!(result.action.as_deref(), Some(ACTION));
        *result.error.expect("a refusal")
    };

    let again = refused(end(
        w.path(),
        &run_ref,
        ms("2026-10-20T09:02:00Z"),
        "done",
        HOST,
        at(T),
    ));
    assert_eq!(again.code, "run_ended_already");
    assert_eq!(
        again.message,
        format!("{} has ended already", run_ref.as_str().unwrap())
    );
    let details = again.details.unwrap();
    assert_eq!(details["run_ref"], run_ref);
    assert_eq!(details["run"]["run_ref"], run_ref);
    assert_eq!(
        details["run"]["state"], "stalled",
        "as it ended, not as asked"
    );
    assert_eq!(
        details["run"]["end"]["reason"],
        "The agent could not start."
    );
    assert_eq!(details["retry_policy_action"], "central.work.policy");
    assert_eq!(details["automatic_authority_widening"], false);
    assert_eq!(
        fs::read_to_string(log_path(w.path(), "2026-10-20")).unwrap(),
        kept,
        "nothing is recorded"
    );

    // A move still running refuses a second begin: a conflict, in the code it had.
    begin(w.path(), "tasks", ms("2026-10-20T09:10:00Z"), HOST, at(T)).unwrap();
    let running = refused(begin(
        w.path(),
        "tasks",
        ms("2026-10-20T09:11:00Z"),
        HOST,
        at(T),
    ));
    assert_eq!(running.code, CONFLICT);
    assert!(running.details.unwrap().get("run").is_none());

    // The Day's record changed after it was read: the write an end makes on
    // the revision it read is refused as a conflict, in the code it had.
    let relative = "Control/user/day/2026-10-20/moves.jsonl";
    let now = fs::read_to_string(log_path(w.path(), "2026-10-20")).unwrap();
    let changed = refused(
        crate::source_safety::replace(w.path(), relative, &source::revision(&kept), &now)
            .map(|()| Value::Null),
    );
    assert_eq!(changed.code, CONFLICT);

    // A closed Day refuses a begin: a conflict, in the code it had.
    run(
        w.path(),
        "day_lifecycle",
        json!({"day_ref":day["day_ref"],"expected_revision":day["revision"]["revision"],
            "expected_relations_revision":day["relations_revision"],"lifecycle":"closed"}),
        Some(HUMAN),
        at(T),
    )
    .unwrap();
    let closed = refused(begin(
        w.path(),
        "patterns",
        ms("2026-10-20T20:05:00Z"),
        HOST,
        at(T),
    ));
    assert_eq!(closed.code, CONFLICT);
}

// ---- be2m: words that hold a joiner are kept (DLW build ruling 2026-11-10) --------------------------------------

/// The sha256 of `joiner-cases.json`, the cases O-I keeps beside its own tests too, byte for byte: rows of text, whether
/// the Day keeps the words as given, what stood in the way when it does not, and the text as it is shown on one line. A
/// file that moves on one side without the other's sha256 changing fails here or there (be2m item 5).
const JOINER_CASES_SHA256: &str =
    "56d86905278fada2c13a081d5e7058a00864d17e0382c84ec2ce85f084c83bd4";

fn joiner_cases() -> Vec<Value> {
    let cases = include_str!("joiner-cases.json");
    assert_eq!(
        format!("{:x}", Sha256::digest(cases.as_bytes())),
        JOINER_CASES_SHA256,
        "the cases are the ones O-I holds"
    );
    serde_json::from_str(cases).unwrap()
}

/// A joiner that changes how the characters beside it are drawn is part of the words a person sees: a family emoji, a
/// technologist with a skin tone, a Persian word with its ZWNJ and a Devanagari conjunct are recorded as given. A joiner
/// that draws nothing where it stands is refused as before, and so is every other invisible character; the refusal
/// leads with what stood in the way, in a person's terms, one kind per case.
#[test]
fn a_records_words_keep_a_joiner_in_its_place_and_refused_words_name_what_stood_in_the_way() {
    let w = world("Europe/London", 0);
    ensure(w.path(), at(T));
    let at_ms = ms("2026-10-20T09:10:00Z");
    let cases = joiner_cases();
    let text = |case: &Value| case["text"].as_str().unwrap().to_owned();
    let list: Vec<Value> = cases
        .iter()
        .enumerate()
        .map(|(index, case)| {
            reached(
                &format!("saying:joiner-{index}"),
                "to-you",
                at_ms,
                &text(case),
            )
        })
        .collect();
    let mut results = Vec::new();
    for turn in list.chunks(32) {
        let answers = records(w.path(), turn.to_vec(), at(T)).unwrap();
        results.extend(answers["results"].as_array().unwrap().iter().cloned());
    }
    assert_eq!(results.len(), cases.len());
    let mut kinds = std::collections::BTreeSet::new();
    for (case, result) in cases.iter().zip(&results) {
        let name = case["name"].as_str().unwrap();
        if case["kept"] == true {
            assert_eq!(result["created"], true, "{name}: {result}");
            assert_eq!(result["item"]["words"], case["text"], "{name}: as given");
        } else {
            let kind = case["kind"].as_str().unwrap();
            kinds.insert(kind);
            let words = result["refused"].as_str().unwrap_or_default();
            assert!(
                words.starts_with(&format!(
                    "words hold {kind}: they hold no control character but line breaks and tabs"
                )),
                "{name}: {result}"
            );
            assert!(
                words.ends_with("but a joiner inside an emoji or a joined word"),
                "{name}: {result}"
            );
        }
    }
    assert_eq!(
        kinds.into_iter().collect::<Vec<_>>(),
        [
            "a bidi control or mark",
            "a blank filler",
            "a byte order mark",
            "a carriage return with no line break after it",
            "a control character",
            "a format character that draws nothing",
            "a joiner outside an emoji or a joined word",
            "a line or paragraph separator",
            "a soft hyphen",
            "a tag character",
            "a zero-width space",
        ],
        "one case at least for each kind"
    );
    // The Day reads back what it kept, and nothing else.
    let reading = read(w.path(), "day_field_read", json!({}), at(T));
    let mut read_back: Vec<String> = reading["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["words"].as_str().unwrap().to_owned())
        .collect();
    let mut kept: Vec<String> = cases
        .iter()
        .filter(|case| case["kept"] == true)
        .map(text)
        .collect();
    read_back.sort();
    kept.sort();
    assert_eq!(read_back, kept);
    assert!(
        kept.len() >= 20
            && kept
                .iter()
                .any(|words| words.contains("\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}"))
            && kept
                .iter()
                .any(|words| words.contains("\u{06CC}\u{200C}\u{062E}")),
        "the family emoji and the Persian word are among them"
    );
}

/// be2m.3 item 4 (DLW build ruling 2026-11-12): the cases' `shown` column against the form a reader of the Day is given
/// today, the label of an entry read as a part (`central.day.entry.part`, whose words `plain_text_words` gives). The
/// label still leaves every joiner out (it keeps one in its place once O-I shows a part label that holds one), so for
/// every case it is the column without its joiners, and the column itself where the column holds none.
#[test]
fn an_entrys_label_is_each_cases_shown_form_without_its_joiners() {
    let w = world("Europe/London", 0);
    ensure(w.path(), at(T));
    make_document(w.path(), TODAY, "day:c1-today", PROPOSAL, at(T));
    let cases = joiner_cases();
    let items: Vec<Value> = cases
        .iter()
        .enumerate()
        .map(|(index, case)| json!({"id":format!("j{index}"),"text":case["text"]}))
        .collect();
    set_field(
        w.path(),
        TODAY,
        "p1_must",
        Value::Array(items),
        "set:joiners",
        at(T),
    );
    let joiners = ['\u{200C}', '\u{200D}'];
    let mut with_a_joiner = 0;
    for (index, case) in cases.iter().enumerate() {
        let name = &case["name"];
        let read = part(
            w.path(),
            json!({"field_id":"p1_must","item_id":format!("j{index}")}),
            at(T),
        )
        .unwrap();
        let label = read["part"]["label"].as_str().unwrap();
        let column = case["shown"].as_str().unwrap();
        assert_eq!(label, column.replace(joiners, ""), "{name}");
        assert_eq!(label == column, !column.contains(joiners), "{name}");
        with_a_joiner += usize::from(column.contains(joiners));
    }
    assert_eq!(
        with_a_joiner, 38,
        "the cases shown with a joiner in its place: every kept one that holds a joiner, and the family beside a soft hyphen"
    );
}

/// Prose only: a name keeps the whole rule, so a sender's name or a star's name that holds a joiner is refused though
/// the same characters stand in a record's words; and a run's reason and summary, an agent's words the ruling does not
/// name, keep the whole rule too.
#[test]
fn a_name_and_a_runs_own_words_still_refuse_a_joiner_that_a_records_words_keep() {
    let w = world("Europe/London", 0);
    ensure(w.path(), at(T));
    let at_ms = ms("2026-10-20T09:10:00Z");
    let family = "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}";
    let persian = "\u{0645}\u{06CC}\u{200C}\u{062E}\u{0648}\u{0627}\u{0647}\u{0645}";
    let mut list = Vec::new();
    for (index, joined) in [family, persian].into_iter().enumerate() {
        let mut named = reached(&format!("saying:name-{index}"), "to-you", at_ms, joined);
        named["from"]["name"] = json!(joined);
        list.push(named);
        let mut starred = reached(&format!("saying:star-{index}"), "star", at_ms, joined);
        starred["star"]["name"] = json!(joined);
        list.push(starred);
        list.push(reached(
            &format!("saying:words-{index}"),
            "to-you",
            at_ms,
            joined,
        ));
    }
    let answers = records(w.path(), list, at(T)).unwrap();
    for result in answers["results"].as_array().unwrap() {
        let named = result["item_ref"].as_str().unwrap();
        let words = result["refused"].as_str().unwrap_or_default();
        if named.contains("name-") {
            assert!(
                words.starts_with("from.name stands on one line"),
                "{result}"
            );
        } else if named.contains("star-") {
            assert!(
                words.starts_with("star.name stands on one line"),
                "{result}"
            );
        } else {
            assert_eq!(result["created"], true, "{result}");
        }
    }
    let run_ref = begin(w.path(), "tasks", at_ms, HOST, at(T)).unwrap()["run"]["run_ref"].clone();
    for (state, reason, summary, refusal) in [
        (
            "done",
            None,
            format!("Three tasks for {family}."),
            "gave.summary holds no control character",
        ),
        (
            "stalled",
            Some(format!("Waiting on {persian}.")),
            "Three tasks.".to_owned(),
            "reason holds no control character",
        ),
    ] {
        let mut input = json!({"event":"end","run_ref":run_ref,"at_unix_ms":at_ms + 60_000,"state":state,
            "gave":{"summary":summary}});
        if let Some(reason) = reason {
            input["reason"] = json!(reason);
        }
        let refused = words_of(run(w.path(), "day_move_record", input, Some(AGENT), at(T)));
        assert!(refused.contains(refusal), "{refused}");
    }
}
