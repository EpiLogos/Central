//! The Day's turn by Central's civil clock and the receipts laid into a Day (day_turn.rs; O-I #1005). Every world here is a
//! disposable temporary Central root with its own civil-time policy, placement policy, Day form and credentials; none of it
//! touches a real ground.
use super::*;
use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use std::fs;

const HUMAN: &str = "day-turn-human-token-not-a-live-credential";
const FORM_PATH: &str = "Control/forms/ql-daily-die.html";
const FIRST: &str = "central:day:control:root:2026-10-20";
const SECOND: &str = "central:day:control:root:2026-10-21";
const PROPOSAL: &str = "<p>Begin with the harbour map.</p>";
const PAGE: &str = "central:path:root:Control/user/flows/harbour.html";

fn at(rfc3339: &str) -> u64 {
    DateTime::parse_from_rfc3339(rfc3339)
        .unwrap()
        .with_timezone(&Utc)
        .timestamp() as u64
}

/// The Day form as Setup keeps it in a new ground: its inert ql-doc island, a daily payload with its field map.
fn day_form() -> String {
    let payload = json!({"meta":{"type":"daily","template":"ql-daily v0.1","date":null,"timezone":null,"uuid":null,"face":0},
        "fields":{"p0_quick_thoughts":"","p1_must":[],"p5_synthesis":""},"body":{"p5_tomorrow":""},"capture":[]});
    format!(
        "<!doctype html><html><body><script>window.ignored = true</script>\n<script id=\"ql-doc\" type=\"application/json\">{payload}</script></body></html>"
    )
}

/// A world whose civil-time policy turns the Day by itself (`automatic`), in `timezone` at boundary 0, keeping the Day form.
fn world(timezone: &str, automatic: bool) -> crate::TempDir {
    let temp = crate::tempdir().unwrap();
    for path in [
        "Control/user",
        "Control/relations",
        "Control/agents",
        "Control/forms",
        "Work/sources",
    ] {
        fs::create_dir_all(temp.path().join(path)).unwrap();
    }
    fs::write(temp.path().join(FORM_PATH), day_form()).unwrap();
    let scope = Scope::resolve(temp.path(), None).unwrap();
    let grant = json!({"principal_ref":"human:test","actor_kind":"human","token_sha256":format!("{:x}",Sha256::digest(HUMAN.as_bytes())),
        "scope_refs":["control:root"],"actions":["central.document.create","central.document.mutate","central.day.settings.write"],
        "expires_at_unix_seconds":4_000_000_000u64});
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
            json!({"schema":"central.civil-time-policy/v1","scope_ref":"control:root","timezone":timezone,"day_boundary_minutes":0,"automatic_day_rollover":automatic}),
        ),
        (
            "authority.json",
            "native-action-authority",
            json!({"schema":"central.native-action-authority/v1","scope_ref":"control:root","grants":[grant]}),
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

/// The Day form's location, as Central's file route gives it.
fn form(root: &Path) -> Value {
    let central = Scope::resolve(root, None).unwrap().central_root;
    serde_json::to_value(crate::files::CentralPathRef::new(&central, FORM_PATH.into()).unwrap())
        .unwrap()
}
fn turn(root: &Path, input: Value, now: u64) -> io::Result<Value> {
    execute_at(root, "day_turn", &input, now)
}
fn read(root: &Path, input: Value, now: u64) -> Value {
    execute_at(root, "day_read", &input, now).unwrap()
}
fn relations_bytes(root: &Path) -> Vec<u8> {
    fs::read(root.join("Control/relations/source-relations.json")).unwrap()
}

/// The person's own Day document on the Day `day_ref` (`central.document.create`), its face 5 holding `proposal` and its set
/// holding one row with one page.
fn person_document(root: &Path, day_ref: &str, proposal: &str, now: u64) {
    let day = read(root, json!({"day_ref":day_ref}), now);
    let policy = execute_at(root, "policy", &json!({}), now).unwrap();
    let payload = json!({"meta":{"type":"daily","template":"ql-daily v0.1",
            "set":{"schema":"oi.day-set/v1","tracks":[{"world":"central:world:root","name":"Yours"}],
                "rows":[{"id":"row:harbour","name":"Harbour","pages":[PAGE]}]}},
        "fields":{"p0_quick_thoughts":""},"body":{"p5_tomorrow":proposal},"capture":[]});
    execute_with_token_at(
        root,
        "document_create",
        &json!({"kind":"day","document_id":"document:person","day_ref":day_ref,"expected_revision":day["revision"]["revision"],
            "expected_policy_revision":policy["revision"],"template_payload":payload,
            "fields":[{"id":"p0_quick_thoughts","label":"p0_quick_thoughts","template_pointer":"/fields/p0_quick_thoughts"}]}),
        Some(HUMAN),
        now,
    )
    .unwrap();
}

#[test]
fn the_turn_opens_today_and_makes_its_page_from_the_day_form() {
    let w = world("UTC", true);
    let now = at("2026-10-20T09:00:00Z");
    // Asked with no form, today's Day opens and says it wants its page.
    let first = turn(w.path(), json!({}), now).unwrap();
    assert_eq!(first["schema"], "central.day-reading/v1");
    assert_eq!(first["day_ref"], FIRST);
    assert_eq!(first["temporal"]["lifecycle"], "open");
    assert_eq!(first["today"]["day_ref"], FIRST);
    assert_eq!(first["document_state"], "uninitialised");
    assert_eq!(first["turn"]["schema"], "central.day-turn/v1");
    assert_eq!(first["turn"]["by"], "central:civil-clock");
    assert_eq!(first["turn"]["opened"], true);
    assert_eq!(first["turn"]["document"]["state"], "wants-form");
    assert_eq!(first["automatic_agent_or_model_invocation"], false);
    // Its bounds are Central's: the civil date's first instant and the instant it turns.
    assert_eq!(
        first["bounds"]["opens_at_unix_seconds"],
        at("2026-10-20T00:00:00Z")
    );
    assert_eq!(
        first["bounds"]["turns_at_unix_seconds"],
        at("2026-10-21T00:00:00Z")
    );
    // Offered the form, it makes the page from the form's own island, dated by Central and bound to the form where it stands.
    let made = turn(w.path(), json!({"form":form(w.path())}), now).unwrap();
    assert_eq!(made["turn"]["opened"], false);
    assert_eq!(made["turn"]["document"]["state"], "made");
    assert_eq!(made["document_state"], "ready");
    let payload = &made["document"]["document"]["template_payload"];
    assert_eq!(payload["meta"]["date"], "2026-10-20");
    assert_eq!(payload["meta"]["timezone"], "UTC");
    assert_eq!(
        payload["meta"]["uuid"],
        format!("document:{FIRST}").as_str()
    );
    assert_eq!(
        payload["_oi_form_source"]["schema"],
        "oi.original-day-form/v1"
    );
    assert_eq!(payload["_oi_form_source"]["location"]["path"], FORM_PATH);
    let fields: Vec<&str> = made["document"]["document"]["fields"]
        .as_array()
        .unwrap()
        .iter()
        .map(|field| field["template_pointer"].as_str().unwrap())
        .collect();
    assert!(fields.contains(&"/fields/p0_quick_thoughts"));
    assert!(fields.contains(&"/body"));
    // The page made is the Day's own document, made by the clock, not by a person.
    assert_eq!(made["document"]["document"]["kind"], "day");
    assert_eq!(made["document"]["document"]["day_ref"], FIRST);
}

#[test]
fn asked_again_with_nothing_to_turn_the_turn_writes_nothing() {
    let w = world("UTC", true);
    let now = at("2026-10-20T09:00:00Z");
    turn(w.path(), json!({"form":form(w.path())}), now).unwrap();
    let relations = relations_bytes(w.path());
    let day = read(w.path(), json!({}), now);
    let again = turn(w.path(), json!({"form":form(w.path())}), now + 600).unwrap();
    assert_eq!(again["turn"]["opened"], false);
    assert_eq!(again["turn"]["document"]["state"], "stood");
    assert_eq!(again["turn"]["closed"], json!([]));
    assert_eq!(
        relations_bytes(w.path()),
        relations,
        "the relations are not rewritten"
    );
    assert_eq!(
        again["revision"], day["revision"],
        "the Day's source is not rewritten"
    );
}

#[test]
fn a_policy_that_keeps_the_day_from_turning_by_itself_is_refused_in_words_and_nothing_opens() {
    let w = world("UTC", false);
    let relations = relations_bytes(w.path());
    let refused = turn(w.path(), json!({}), at("2026-10-20T09:00:00Z")).unwrap_err();
    assert_eq!(refused.kind(), io::ErrorKind::PermissionDenied);
    assert!(refused
        .to_string()
        .contains("automatic_day_rollover is off"));
    assert_eq!(relations_bytes(w.path()), relations);
}

#[test]
fn writing_that_is_no_day_document_is_kept_and_no_page_is_made_over_it() {
    let w = world("UTC", true);
    let now = at("2026-10-20T09:00:00Z");
    let opened = turn(w.path(), json!({}), now).unwrap();
    let path = opened["source"]["path"].as_str().unwrap().to_owned();
    fs::write(w.path().join(&path), "The person's own words.\n").unwrap();
    let kept = turn(w.path(), json!({"form":form(w.path())}), now).unwrap();
    assert_eq!(kept["turn"]["document"]["state"], "kept");
    assert_eq!(kept["document_state"], "uninitialised");
    assert_eq!(
        fs::read_to_string(w.path().join(&path)).unwrap(),
        "The person's own words.\n"
    );
}

#[test]
fn at_its_turn_the_earlier_day_closes_its_proposal_waiting_and_its_set_carried_into_today() {
    let w = world("UTC", true);
    let first = at("2026-10-20T09:00:00Z");
    turn(w.path(), json!({}), first).unwrap();
    person_document(w.path(), FIRST, PROPOSAL, first);
    // The next civil day, a second after the turn.
    let second = at("2026-10-21T00:00:01Z");
    let turned = turn(w.path(), json!({"form":form(w.path())}), second).unwrap();
    assert_eq!(turned["day_ref"], SECOND);
    assert_eq!(turned["today"]["day_ref"], SECOND);
    assert_eq!(turned["turn"]["opened"], true);
    assert_eq!(turned["turn"]["document"]["state"], "made");
    let closed = turned["turn"]["closed"].as_array().unwrap();
    assert_eq!(closed.len(), 1);
    assert_eq!(closed[0]["day_ref"], FIRST);
    assert_eq!(closed[0]["civil_date"], "2026-10-20");
    // The clock writes no one's words into a Day: face 5's proposal waits for the person, said in words.
    assert_eq!(closed[0]["carry"]["state"], "waits");
    assert!(closed[0]["carry"]["reason"]
        .as_str()
        .unwrap()
        .contains("waits for you"));
    // The earlier Day is closed by the clock and kept, its writing intact.
    let earlier = read(w.path(), json!({"day_ref":FIRST}), second);
    assert_eq!(earlier["temporal"]["lifecycle"], "closed");
    assert_eq!(
        earlier["temporal"]["lifecycle_actor_ref"],
        "central:civil-clock"
    );
    assert_eq!(
        earlier["document"]["document"]["template_payload"]["body"]["p5_tomorrow"],
        PROPOSAL
    );
    // Today's face 0 holds nothing the clock wrote.
    let today = read(w.path(), json!({}), second);
    assert!(today["document"]["document"]["contributions"]
        .as_array()
        .is_none_or(Vec::is_empty));
    // The set carried: its worlds as they were, a row whose page no longer plays dropped, the earlier Day in its lineage.
    let set = &today["document"]["document"]["template_payload"]["meta"]["set"];
    assert_eq!(set["schema"], "oi.day-set/v1");
    assert_eq!(
        set["tracks"],
        json!([{"world":"central:world:root","name":"Yours"}])
    );
    assert_eq!(set["rows"], json!([]));
    assert_eq!(set["lineage"], json!([FIRST]));
    // Asked again, nothing more turns.
    let relations = relations_bytes(w.path());
    let again = turn(w.path(), json!({"form":form(w.path())}), second + 60).unwrap();
    assert_eq!(again["turn"]["closed"], json!([]));
    assert_eq!(relations_bytes(w.path()), relations);
}

#[test]
fn with_the_close_routine_off_the_earlier_day_closes_and_carries_nothing() {
    let w = world("UTC", true);
    let first = at("2026-10-20T09:00:00Z");
    turn(w.path(), json!({}), first).unwrap();
    person_document(w.path(), FIRST, PROPOSAL, first);
    let settings = execute_at(w.path(), "day_settings_read", &json!({}), first).unwrap();
    execute_with_token_at(
        w.path(),
        "day_settings_write",
        &json!({"expected_revision":settings["revision"],"close_routine":false}),
        Some(HUMAN),
        first,
    )
    .unwrap();
    let turned = turn(w.path(), json!({}), at("2026-10-21T00:00:01Z")).unwrap();
    let closed = turned["turn"]["closed"].as_array().unwrap();
    assert_eq!(closed.len(), 1);
    assert_eq!(closed[0]["carry"]["state"], "off");
    let earlier = read(
        w.path(),
        json!({"day_ref":FIRST}),
        at("2026-10-21T00:00:01Z"),
    );
    assert_eq!(earlier["temporal"]["lifecycle"], "closed");
}

#[test]
fn a_receipt_is_laid_once_into_the_day_it_happened_on_and_read_back_with_it() {
    let w = world("UTC", true);
    let now = at("2026-10-20T09:00:00Z");
    turn(w.path(), json!({}), now).unwrap();
    let input = json!({"receipt_id":"receipt:files.write:1","act":"central.files.write","owner":"Central",
        "subject":"Control/user/flows/harbour.html","source":PAGE,"receipt":{"revision":"r2"}});
    let laid = execute_at(w.path(), "day_receipt_record", &input, now).unwrap();
    assert_eq!(laid["recorded"], true);
    assert_eq!(laid["civil_date"], "2026-10-20");
    assert_eq!(laid["entry"]["schema"], "central.day-receipt/v1");
    assert_eq!(laid["entry"]["occurred_at_unix_seconds"], now);
    // The same receipt is laid once.
    let again = execute_at(w.path(), "day_receipt_record", &input, now + 5).unwrap();
    assert_eq!(again["recorded"], false);
    let day = read(w.path(), json!({}), now + 5);
    let receipts = day["receipts"].as_array().unwrap();
    assert_eq!(receipts.len(), 1);
    assert_eq!(receipts[0]["act"], "central.files.write");
    assert_eq!(receipts[0]["owner"], "Central");
    assert_eq!(receipts[0]["subject"], "Control/user/flows/harbour.html");
    assert_eq!(receipts[0]["receipt"]["revision"], "r2");
}

#[test]
fn a_late_receipt_keeps_the_time_it_happened_and_lands_in_its_own_day() {
    let w = world("UTC", true);
    let happened = at("2026-10-20T23:59:00Z");
    turn(w.path(), json!({}), happened).unwrap();
    let now = at("2026-10-21T08:00:00Z");
    turn(w.path(), json!({}), now).unwrap();
    let laid = execute_at(
        w.path(),
        "day_receipt_record",
        &json!({"receipt_id":"receipt:late","act":"aikit.encounter.prompt","owner":"AIKit","subject":"agent-session:a",
            "occurred_at_unix_seconds":happened}),
        now,
    )
    .unwrap();
    assert_eq!(laid["civil_date"], "2026-10-20");
    assert_eq!(laid["entry"]["occurred_at_unix_seconds"], happened);
    assert_eq!(laid["entry"]["recorded_at_unix_seconds"], now);
    assert_eq!(
        read(w.path(), json!({"day_ref":FIRST}), now)["receipts"][0]["receipt_id"],
        "receipt:late"
    );
    assert_eq!(read(w.path(), json!({}), now)["receipts"], json!([]));
}

#[test]
fn a_receipt_out_of_its_bounds_is_refused_in_words() {
    let w = world("UTC", true);
    let now = at("2026-10-20T09:00:00Z");
    for (input, words) in [
        (
            json!({"receipt_id":"r","owner":"Central","subject":"s"}),
            "act is text of at most 256 bytes",
        ),
        (
            json!({"receipt_id":"r","act":"a","owner":"Central","subject":"s","occurred_at_unix_seconds":now + 3600}),
            "occurred_at_unix_seconds is an instant no later than now",
        ),
        (
            json!({"receipt_id":"r","act":"a","owner":"Central","subject":"s","receipt":{"bulk":"x".repeat(17 * 1024)}}),
            "receipt is at most 16 KiB of JSON",
        ),
    ] {
        let refused = execute_at(w.path(), "day_receipt_record", &input, now).unwrap_err();
        assert!(
            refused.to_string().contains(words),
            "{refused} should say {words}"
        );
    }
}

#[test]
fn the_turn_and_the_receipt_stand_in_centrals_action_field() {
    let registry = crate::cli::create_runtime_action_registry();
    for (id, class) in [
        ("central.day.turn", "locally-mutating"),
        ("central.day.receipt.record", "locally-mutating"),
    ] {
        let descriptor = registry
            .get(id)
            .unwrap_or_else(|| panic!("{id} is not registered"));
        assert_eq!(descriptor.mutation_class.as_str(), class, "{id}");
    }
}
