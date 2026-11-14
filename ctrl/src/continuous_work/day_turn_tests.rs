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
        "scope_refs":["control:root"],"actions":["central.document.create","central.document.mutate","central.day.settings.write","central.day.lifecycle","central.day.close-carry",
            "central.receiving.review","central.receiving.include","central.day.receipt.record"],
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

/// A Central location for `path` under the world's root, as Central's file route gives it.
fn location(root: &Path, path: &str) -> Value {
    let central = Scope::resolve(root, None).unwrap().central_root;
    serde_json::to_value(crate::files::CentralPathRef::new(&central, path.into()).unwrap()).unwrap()
}
/// Save the person's Day settings on their own credential (`central.day.settings.write`).
fn save(root: &Path, settings: Value, now: u64) -> io::Result<Value> {
    let current = execute_at(root, "day_settings_read", &json!({}), now).unwrap();
    let mut input = settings;
    input["expected_revision"] = current["revision"].clone();
    execute_with_token_at(root, "day_settings_write", &input, Some(HUMAN), now)
}
/// The revision Central's file route gives the file at `path` as it stands: what the person reads before they save it.
fn revision_of(root: &Path, path: &str) -> String {
    let central = Scope::resolve(root, None).unwrap().central_root;
    crate::files::read_file(
        &central,
        &serde_json::from_value(location(root, path)).unwrap(),
        crate::files::FileEncoding::Utf8,
    )
    .unwrap()
    .revision
}
/// The Day form at `path` as the person saves it, at the revision they read.
fn form_at(root: &Path, path: &str) -> Value {
    json!({"path": path, "revision": revision_of(root, path)})
}
/// The person saves their Day form (`day_form`) in the Day's settings.
fn save_form(root: &Path, now: u64) {
    save(root, json!({"day_form": form_at(root, FORM_PATH)}), now).unwrap();
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
fn the_turn_opens_today_and_makes_its_page_from_the_day_form_the_person_saved() {
    let w = world("UTC", true);
    let now = at("2026-10-20T09:00:00Z");
    // With no Day form saved, today's Day opens and says it wants its page.
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
    // The person saves their Day form: the page is made from the form's own island, dated by Central and bound to the form where
    // it stands.
    save_form(w.path(), now);
    let made = turn(w.path(), json!({}), now).unwrap();
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
    save_form(w.path(), now);
    turn(w.path(), json!({}), now).unwrap();
    let relations = relations_bytes(w.path());
    let day = read(w.path(), json!({}), now);
    let again = turn(w.path(), json!({}), now + 600).unwrap();
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
    save_form(w.path(), now);
    let kept = turn(w.path(), json!({}), now).unwrap();
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
    save_form(w.path(), first);
    // The person saved Close at Day start: the consent the clock closes under.
    save(
        w.path(),
        json!({"moves":{"close":{"runs_at":{"kind":"day_start"}}}}),
        first,
    )
    .unwrap();
    // The next civil day, a second after the turn.
    let second = at("2026-10-21T00:00:01Z");
    let turned = turn(w.path(), json!({}), second).unwrap();
    assert_eq!(turned["day_ref"], SECOND);
    assert_eq!(turned["today"]["day_ref"], SECOND);
    assert_eq!(turned["turn"]["opened"], true);
    assert_eq!(turned["turn"]["document"]["state"], "made");
    let closed = turned["turn"]["closed"].as_array().unwrap();
    assert_eq!(closed.len(), 1);
    assert_eq!(closed[0]["day_ref"], FIRST);
    assert_eq!(closed[0]["civil_date"], "2026-10-20");
    // The clock writes no one's words into a Day: face 5's proposal waits for the person.
    assert_eq!(closed[0]["carry"]["state"], "waits");
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
    let again = turn(w.path(), json!({}), second + 60).unwrap();
    assert_eq!(again["turn"]["closed"], json!([]));
    assert_eq!(relations_bytes(w.path()), relations);
}

#[test]
fn with_the_close_routine_off_the_earlier_day_closes_and_carries_nothing() {
    let w = world("UTC", true);
    let first = at("2026-10-20T09:00:00Z");
    turn(w.path(), json!({}), first).unwrap();
    person_document(w.path(), FIRST, PROPOSAL, first);
    save(
        w.path(),
        json!({"close_routine":false,"moves":{"close":{"runs_at":{"kind":"day_start"}}}}),
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

/// The person's own close of the Day `day_ref` with its close routine (`central.day.close-carry`), on its revisions as they stand.
fn close_carry(root: &Path, day_ref: &str, now: u64) -> io::Result<Value> {
    let day = read(root, json!({"day_ref": day_ref}), now);
    execute_with_token_at(
        root,
        "day_close_carry",
        &json!({"day_ref":day_ref,"expected_revision":day["revision"]["revision"],
            "expected_relations_revision":day["relations_revision"]}),
        Some(HUMAN),
        now,
    )
}

/// A world where the person saved Close at Day start and no Day form, and the Day of 2026-10-20 holds their page with a proposal
/// on face 5; turned at the next Day's start.
fn turned_without_a_day_form() -> (crate::TempDir, u64, Value) {
    let w = world("UTC", true);
    let first = at("2026-10-20T09:00:00Z");
    turn(w.path(), json!({}), first).unwrap();
    person_document(w.path(), FIRST, PROPOSAL, first);
    save(
        w.path(),
        json!({"moves":{"close":{"runs_at":{"kind":"day_start"}}}}),
        first,
    )
    .unwrap();
    let second = at("2026-10-21T00:00:01Z");
    let turned = turn(w.path(), json!({}), second).unwrap();
    (w, second, turned)
}

#[test]
fn with_no_day_form_saved_the_clock_leaves_a_day_whose_proposal_cannot_wait_open_and_the_persons_close_still_works(
) {
    let (w, second, turned) = turned_without_a_day_form();
    assert_eq!(turned["turn"]["document"]["state"], "wants-form");
    // The clock closes a Day only when it can file the close's carry: face 5's proposal has no page of today's to wait on.
    assert_eq!(turned["turn"]["closed"], json!([]), "{turned}");
    let open = turned["turn"]["stays_open"].as_array().unwrap();
    assert_eq!(open.len(), 1, "{turned}");
    assert_eq!(open[0]["day_ref"], FIRST);
    let why = open[0]["why"].as_str().unwrap();
    assert!(
        why.contains("No Day form is saved") && why.contains("until you close it"),
        "{why}"
    );
    let earlier = read(w.path(), json!({"day_ref": FIRST}), second);
    assert_eq!(earlier["temporal"]["lifecycle"], "open");
    assert_eq!(earlier["stays_open"], why);
    assert_eq!(
        earlier["document"]["document"]["template_payload"]["body"]["p5_tomorrow"],
        PROPOSAL
    );
    // Nothing was filed, and asked again the clock leaves it as it is.
    let listed = execute_at(w.path(), "receiving_list", &json!({}), second).unwrap();
    assert_eq!(listed["returns"], json!([]), "{listed}");
    let relations = relations_bytes(w.path());
    let again = turn(w.path(), json!({}), second + 60).unwrap();
    assert_eq!(again["turn"]["closed"], json!([]));
    assert_eq!(relations_bytes(w.path()), relations);
    // The person's own close still works: theirs, its proposal kept on the Day's face 5.
    let done = close_carry(w.path(), FIRST, second + 120).unwrap();
    assert_eq!(done["carry"]["state"], "not_carried", "{done}");
    let closed = read(w.path(), json!({"day_ref": FIRST}), second + 120);
    assert_eq!(closed["temporal"]["lifecycle"], "closed");
    assert_eq!(closed["temporal"]["lifecycle_actor_ref"], "human:test");
    assert!(closed.get("stays_open").is_none(), "{closed}");
    assert_eq!(
        closed["document"]["document"]["template_payload"]["body"]["p5_tomorrow"],
        PROPOSAL
    );
}

#[test]
fn once_the_person_saves_a_day_form_the_clock_closes_the_day_its_proposal_waiting() {
    let (w, second, _) = turned_without_a_day_form();
    save_form(w.path(), second + 60);
    let turned = turn(w.path(), json!({}), second + 120).unwrap();
    assert_eq!(turned["turn"]["document"]["state"], "made");
    assert_eq!(turned["turn"]["stays_open"], json!([]), "{turned}");
    let closed = turned["turn"]["closed"].as_array().unwrap();
    assert_eq!(closed.len(), 1, "{turned}");
    assert_eq!(closed[0]["day_ref"], FIRST);
    assert_eq!(closed[0]["carry"]["state"], "waits");
    let earlier = read(w.path(), json!({"day_ref": FIRST}), second + 120);
    assert_eq!(earlier["temporal"]["lifecycle"], "closed");
    assert_eq!(
        earlier["temporal"]["lifecycle_actor_ref"],
        "central:civil-clock"
    );
    let listed = execute_at(w.path(), "receiving_list", &json!({}), second + 120).unwrap();
    assert_eq!(listed["returns"].as_array().unwrap().len(), 1, "{listed}");
}

#[test]
fn a_page_of_todays_that_maps_no_face_zero_leaves_the_day_open_and_says_so() {
    let w = world("UTC", true);
    let first = at("2026-10-20T09:00:00Z");
    turn(w.path(), json!({}), first).unwrap();
    person_document(w.path(), FIRST, PROPOSAL, first);
    // A Day form whose field map holds no face 0.
    fs::write(
        w.path().join(FORM_PATH),
        day_form().replace("\"p0_quick_thoughts\":\"\",", ""),
    )
    .unwrap();
    save_form(w.path(), first);
    save(
        w.path(),
        json!({"moves":{"close":{"runs_at":{"kind":"day_start"}}}}),
        first,
    )
    .unwrap();
    let second = at("2026-10-21T00:00:01Z");
    let turned = turn(w.path(), json!({}), second).unwrap();
    assert_eq!(turned["turn"]["document"]["state"], "made");
    assert_eq!(turned["turn"]["closed"], json!([]), "{turned}");
    let why = turned["turn"]["stays_open"][0]["why"].as_str().unwrap();
    assert!(
        why.contains("maps no field at /fields/p0_quick_thoughts")
            && why.contains("until you close it"),
        "{why}"
    );
    assert_eq!(
        read(w.path(), json!({"day_ref": FIRST}), second)["stays_open"],
        why
    );
}

/// Lay a receipt on the person's credential (`central.day.receipt.record`).
fn lay(root: &Path, input: &Value, now: u64) -> io::Result<Value> {
    execute_with_token_at(root, "day_receipt_record", input, Some(HUMAN), now)
}

/// The person's own close or reopening of the Day `day_ref` (`central.day.lifecycle`), on its revisions as they stand.
fn lifecycle(root: &Path, day_ref: &str, state: &str, now: u64) -> Value {
    let day = read(root, json!({"day_ref": day_ref}), now);
    execute_with_token_at(
        root,
        "day_lifecycle",
        &json!({"day_ref":day_ref,"expected_revision":day["revision"]["revision"],
            "expected_relations_revision":day["relations_revision"],"lifecycle":state}),
        Some(HUMAN),
        now,
    )
    .unwrap()
}

#[test]
fn where_close_is_off_the_earlier_day_stays_open_until_the_person_closes_it_and_its_reading_says_so(
) {
    let w = world("UTC", true);
    let first = at("2026-10-20T09:00:00Z");
    turn(w.path(), json!({}), first).unwrap();
    person_document(w.path(), FIRST, PROPOSAL, first);
    save(
        w.path(),
        json!({"moves":{"close":{"runs_at":{"kind":"off"}}}}),
        first,
    )
    .unwrap();
    let second = at("2026-10-21T00:00:01Z");
    let turned = turn(w.path(), json!({}), second).unwrap();
    assert_eq!(turned["day_ref"], SECOND);
    assert_eq!(turned["turn"]["closed"], json!([]));
    let open = turned["turn"]["stays_open"].as_array().unwrap();
    assert_eq!(open.len(), 1);
    assert_eq!(open[0]["day_ref"], FIRST);
    let why = open[0]["why"].as_str().unwrap();
    assert!(
        why.contains("Close is Off") && why.contains("until you close it"),
        "{why}"
    );
    let earlier = read(w.path(), json!({"day_ref": FIRST}), second);
    assert_eq!(earlier["temporal"]["lifecycle"], "open");
    assert_eq!(earlier["stays_open"], why);
    // Asked again, the clock leaves it as it is.
    let relations = relations_bytes(w.path());
    let again = turn(w.path(), json!({}), second + 60).unwrap();
    assert_eq!(again["turn"]["closed"], json!([]));
    assert_eq!(relations_bytes(w.path()), relations);
    // The person closes it: it is closed by them, and its reading no longer says it stays open.
    let closed = lifecycle(w.path(), FIRST, "closed", second + 120);
    assert_eq!(closed["temporal"]["lifecycle"], "closed");
    assert_eq!(closed["temporal"]["lifecycle_actor_ref"], "human:test");
    assert!(closed.get("stays_open").is_none(), "{closed}");
}

#[test]
fn on_the_days_defaults_the_clock_closes_no_day() {
    let w = world("UTC", true);
    turn(w.path(), json!({}), at("2026-10-20T09:00:00Z")).unwrap();
    let second = at("2026-10-21T00:00:01Z");
    let turned = turn(w.path(), json!({}), second).unwrap();
    assert_eq!(turned["turn"]["closed"], json!([]));
    let why = turned["turn"]["stays_open"][0]["why"].as_str().unwrap();
    assert!(why.contains("saved Close at Day start"), "{why}");
    assert_eq!(
        read(w.path(), json!({"day_ref": FIRST}), second)["temporal"]["lifecycle"],
        "open"
    );
}

#[test]
fn a_day_the_person_reopened_after_its_turn_is_not_closed_again_by_the_clock() {
    let w = world("UTC", true);
    let first = at("2026-10-20T09:00:00Z");
    save(
        w.path(),
        json!({"moves":{"close":{"runs_at":{"kind":"day_start"}}}}),
        first,
    )
    .unwrap();
    turn(w.path(), json!({}), first).unwrap();
    let second = at("2026-10-21T00:00:01Z");
    let turned = turn(w.path(), json!({}), second).unwrap();
    assert_eq!(turned["turn"]["closed"][0]["day_ref"], FIRST);
    let closed = read(w.path(), json!({"day_ref": FIRST}), second);
    assert_eq!(closed["temporal"]["lifecycle"], "closed");
    assert_eq!(
        closed["temporal"]["lifecycle_actor_ref"],
        "central:civil-clock"
    );
    // The person opens it again after its turn: the clock leaves it open, and its reading says why.
    let reopened = lifecycle(w.path(), FIRST, "open", second + 60);
    assert_eq!(reopened["temporal"]["lifecycle"], "open");
    let again = turn(w.path(), json!({}), second + 120).unwrap();
    assert_eq!(again["turn"]["closed"], json!([]));
    let why = again["turn"]["stays_open"][0]["why"].as_str().unwrap();
    assert!(why.contains("again after its turn"), "{why}");
    let earlier = read(w.path(), json!({"day_ref": FIRST}), second + 120);
    assert_eq!(earlier["temporal"]["lifecycle"], "open");
    assert_eq!(earlier["temporal"]["lifecycle_actor_ref"], "human:test");
    assert_eq!(earlier["stays_open"], why);
}

#[test]
fn the_clocks_close_files_the_carry_in_receiving_where_approvals_keeps_or_drops_it() {
    let w = world("UTC", true);
    let first = at("2026-10-20T09:00:00Z");
    turn(w.path(), json!({}), first).unwrap();
    person_document(w.path(), FIRST, PROPOSAL, first);
    save(
        w.path(),
        json!({"day_form": form_at(w.path(), FORM_PATH), "moves":{"close":{"runs_at":{"kind":"day_start"}}}}),
        first,
    )
    .unwrap();
    let second = at("2026-10-21T00:00:01Z");
    let turned = turn(w.path(), json!({}), second).unwrap();
    let carry = &turned["turn"]["closed"][0]["carry"];
    assert_eq!(carry["state"], "waits", "{carry}");
    assert!(
        carry["reason"].as_str().unwrap().contains("Approvals"),
        "{carry}"
    );
    let return_ref = carry["return_ref"].as_str().expect("the carry's Return");
    // It waits in receiving: one Return, pending, by the clock, proposing face 5's words for today's face 0.
    let listed = execute_at(w.path(), "receiving_list", &json!({}), second).unwrap();
    let returns = listed["returns"].as_array().unwrap();
    assert_eq!(returns.len(), 1, "{listed}");
    assert_eq!(returns[0]["return_ref"], return_ref);
    assert_eq!(returns[0]["status"], "pending");
    assert_eq!(returns[0]["author"]["principal_ref"], "central:civil-clock");
    assert_eq!(returns[0]["day_ref"], SECOND);
    let held = execute_at(
        w.path(),
        "receiving_read",
        &json!({"return_ref": return_ref}),
        second,
    )
    .unwrap();
    let proposal = &held["record"]["proposal"];
    assert_eq!(proposal["operation"], "field.append");
    assert_eq!(proposal["field_id"], "p0_quick_thoughts");
    assert_eq!(proposal["html"], PROPOSAL);
    assert_eq!(
        held["record"]["document_id"],
        format!("document:{SECOND}").as_str()
    );
    // Today's face 0 holds nothing until the person keeps it, and asked again the clock files nothing more.
    let today = read(w.path(), json!({}), second + 60);
    assert!(today["document"]["document"]["contributions"]
        .as_array()
        .is_none_or(Vec::is_empty));
    turn(w.path(), json!({}), second + 60).unwrap();
    let listed = execute_at(w.path(), "receiving_list", &json!({}), second + 60).unwrap();
    assert_eq!(listed["returns"].as_array().unwrap().len(), 1);
    // The person keeps it: accepted and included, today's face 0 holds the proposal, the clock's, kept by them.
    let basis = today["revision"]["revision"].clone();
    let reviewed = execute_with_token_at(
        w.path(),
        "receiving_review",
        &json!({"return_ref": return_ref, "expected_return_revision": held["revision"], "disposition": "accepted",
            "expected_source_revision": basis}),
        Some(HUMAN),
        second + 120,
    )
    .unwrap();
    execute_with_token_at(
        w.path(),
        "receiving_include",
        &json!({"return_ref": return_ref, "expected_return_revision": reviewed["revision"],
            "expected_source_revision": basis}),
        Some(HUMAN),
        second + 180,
    )
    .unwrap();
    let today = read(w.path(), json!({}), second + 180);
    let contributions = today["document"]["document"]["contributions"]
        .as_array()
        .unwrap();
    assert_eq!(contributions.len(), 1, "{today}");
    assert_eq!(contributions[0]["field_id"], "p0_quick_thoughts");
    assert_eq!(contributions[0]["html"], PROPOSAL);
}

/// A Day form anyone may write: in Work/sources, where the placement policy lets work write.
const PLANTED: &str = "Work/sources/planted.html";

#[test]
fn the_turn_makes_the_page_only_from_the_saved_day_form_never_from_one_offered_in_its_input() {
    let w = world("UTC", true);
    let now = at("2026-10-20T09:00:00Z");
    fs::write(
        w.path().join(PLANTED),
        day_form().replace("ql-daily v0.1", "planted"),
    )
    .unwrap();
    // A form offered in the turn's input is refused in words, and nothing opens or is made from it.
    let relations = relations_bytes(w.path());
    let refused = turn(w.path(), json!({"form": location(w.path(), PLANTED)}), now).unwrap_err();
    assert_eq!(refused.kind(), io::ErrorKind::InvalidInput, "{refused}");
    assert!(refused.to_string().contains("day_form"), "{refused}");
    assert_eq!(relations_bytes(w.path()), relations);
    // With none saved, today's Day opens and says where its page comes from.
    let opened = turn(w.path(), json!({}), now).unwrap();
    assert_eq!(opened["turn"]["document"]["state"], "wants-form");
    assert!(opened["turn"]["document"]["why"]
        .as_str()
        .unwrap()
        .contains("day_form"));
    // The person saves their form at the revision they read: the reading shows it as theirs, and the page is made from it alone.
    let saved = save(
        w.path(),
        json!({"day_form": form_at(w.path(), FORM_PATH)}),
        now,
    )
    .unwrap();
    assert_eq!(saved["settings"]["day_form"]["path"], FORM_PATH);
    assert_eq!(
        saved["settings"]["day_form"]["revision"],
        revision_of(w.path(), FORM_PATH)
    );
    assert_eq!(saved["standing"]["day_form"], "saved");
    let made = turn(w.path(), json!({}), now).unwrap();
    assert_eq!(made["turn"]["document"]["state"], "made");
    let payload = &made["document"]["document"]["template_payload"];
    assert_eq!(payload["_oi_form_source"]["location"]["path"], FORM_PATH);
    assert_eq!(payload["meta"]["template"], "ql-daily v0.1");
}

#[test]
fn the_day_form_the_person_saves_is_a_day_form_in_their_world() {
    let w = world("UTC", true);
    let now = at("2026-10-20T09:00:00Z");
    fs::write(
        w.path().join("Control/forms/plain.html"),
        "<!doctype html><p>No data island.</p>",
    )
    .unwrap();
    fs::write(
        w.path().join("Control/forms/flow.html"),
        day_form().replace("\"daily\"", "\"flow\""),
    )
    .unwrap();
    let read_as = |path: &str| json!({"path": path, "revision": "a revision read"});
    for (form, words) in [
        (form_at(w.path(), "Control/forms/plain.html"), "ql-doc"),
        (form_at(w.path(), "Control/forms/flow.html"), "no Day form"),
        (read_as("Control/forms/absent.html"), ""),
        (read_as("../outside.html"), "day_form"),
        (read_as("/etc/hosts"), "day_form"),
        // A path alone, or a form without the revision the person read, pins nothing.
        (json!(FORM_PATH), "day_form"),
        (json!({"path": FORM_PATH}), "day_form"),
        (json!({"path": FORM_PATH, "revision": ""}), "day_form"),
        // The form as it stood when read, since changed: the person reads it again.
        (read_as(FORM_PATH), "changed since you read it"),
    ] {
        let refused = save(w.path(), json!({"day_form": form}), now).unwrap_err();
        assert!(
            refused.to_string().contains(words),
            "{form}: {refused} should say {words}"
        );
    }
    let settings = execute_at(w.path(), "day_settings_read", &json!({}), now).unwrap();
    assert_eq!(settings["saved"], false);
    assert_eq!(settings["settings"]["day_form"], Value::Null);
    assert_eq!(settings["standing"]["day_form"], "default");
}

#[test]
fn a_day_form_changed_since_the_person_saved_it_makes_no_page_until_they_save_it_again() {
    let w = world("UTC", true);
    let now = at("2026-10-20T09:00:00Z");
    // The person saves a form that stands where work may write, as they read it.
    fs::write(w.path().join(PLANTED), day_form()).unwrap();
    let read = form_at(w.path(), PLANTED);
    save(w.path(), json!({"day_form": read}), now).unwrap();
    // Another hand writes words into it afterwards.
    let planted = day_form().replace(
        "\"p0_quick_thoughts\":\"\"",
        "\"p0_quick_thoughts\":\"Words no person wrote\"",
    );
    assert_ne!(planted, day_form());
    fs::write(w.path().join(PLANTED), &planted).unwrap();
    // Saving it again on the revision read before is refused: the person reads it again.
    let stale = save(w.path(), json!({"day_form": read}), now).unwrap_err();
    assert_eq!(stale.kind(), io::ErrorKind::AlreadyExists, "{stale}");
    assert!(
        stale.to_string().contains("changed since you read it"),
        "{stale}"
    );
    // The clock makes no page from the form as it stands now, and says why.
    let turned = turn(w.path(), json!({}), now).unwrap();
    assert_eq!(
        turned["turn"]["document"]["state"], "wants-form",
        "{turned}"
    );
    let why = turned["turn"]["document"]["why"].as_str().unwrap();
    assert!(
        why.contains(PLANTED)
            && why.contains("changed since you saved it")
            && why.contains("save it again"),
        "{why}"
    );
    assert_eq!(turned["document_state"], "uninitialised");
    // Nor may the clock make today's page bound to the form as it stands now.
    let scope = Scope::resolve(w.path(), None).unwrap();
    let clock = authority::civil_clock(&scope, &temporal::time_policy(&scope, now).unwrap());
    let policy = execute_at(w.path(), "policy", &json!({}), now).unwrap();
    let refused = documents::create(
        &scope,
        &json!({"kind":"day","day_ref":FIRST,"document_id":format!("document:{FIRST}"),
            "expected_revision":turned["revision"]["revision"],"expected_policy_revision":policy["revision"],
            "template_payload":{"meta":{"type":"daily"},"fields":{"p0_quick_thoughts":""},
                "_oi_form_source":{"schema":"oi.original-day-form/v1","location":location(w.path(), PLANTED),
                    "revision":revision_of(w.path(), PLANTED)}},
            "fields":[]}),
        &clock,
        now,
    )
    .unwrap_err();
    assert_eq!(refused.kind(), io::ErrorKind::PermissionDenied, "{refused}");
    // The person reads it and saves it again: today's page is made from the form as they read it.
    save(
        w.path(),
        json!({"day_form": form_at(w.path(), PLANTED)}),
        now,
    )
    .unwrap();
    let made = turn(w.path(), json!({}), now).unwrap();
    assert_eq!(made["turn"]["document"]["state"], "made", "{made}");
    assert_eq!(
        made["document"]["document"]["template_payload"]["_oi_form_source"]["revision"],
        revision_of(w.path(), PLANTED)
    );
}

#[test]
fn the_clock_makes_no_day_document_but_todays_page_from_the_saved_form() {
    let w = world("UTC", true);
    let now = at("2026-10-20T09:00:00Z");
    turn(w.path(), json!({}), now).unwrap();
    save_form(w.path(), now);
    fs::write(w.path().join(PLANTED), day_form()).unwrap();
    let scope = Scope::resolve(w.path(), None).unwrap();
    let clock = authority::civil_clock(&scope, &temporal::time_policy(&scope, now).unwrap());
    let day = read(w.path(), json!({"day_ref": FIRST}), now);
    let policy = execute_at(w.path(), "policy", &json!({}), now).unwrap();
    let bound = |path: &str| {
        json!({"meta":{"type":"daily"},"fields":{"p0_quick_thoughts":""},
            "_oi_form_source":{"schema":"oi.original-day-form/v1","location":location(w.path(), path),"revision":revision_of(w.path(), path)}})
    };
    let create = |day_ref: &str, document_id: &str, payload: Value, at: u64| {
        documents::create(
            &scope,
            &json!({"kind":"day","day_ref":day_ref,"document_id":document_id,"expected_revision":day["revision"]["revision"],
                "expected_policy_revision":policy["revision"],"template_payload":payload,"fields":[]}),
            &clock,
            at,
        )
    };
    for (refused, why) in [
        (
            create(FIRST, "document:chosen", bound(FORM_PATH), now),
            "another document",
        ),
        (
            create(FIRST, &format!("document:{FIRST}"), bound(PLANTED), now),
            "another form",
        ),
        (
            create(
                FIRST,
                &format!("document:{FIRST}"),
                json!({"meta":{"type":"daily"},"fields":{}}),
                now,
            ),
            "no form",
        ),
        (
            create(
                FIRST,
                &format!("document:{FIRST}"),
                bound(FORM_PATH),
                at("2026-10-21T09:00:00Z"),
            ),
            "a Day that is not today",
        ),
    ] {
        let refused = refused.expect_err(why);
        assert_eq!(
            refused.kind(),
            io::ErrorKind::PermissionDenied,
            "{why}: {refused}"
        );
    }
    assert_eq!(
        read(w.path(), json!({"day_ref": FIRST}), now)["document_state"],
        "uninitialised"
    );
    // Today's page from the saved form is the one the clock makes.
    create(FIRST, &format!("document:{FIRST}"), bound(FORM_PATH), now).unwrap();
    assert_eq!(
        read(w.path(), json!({"day_ref": FIRST}), now)["document_state"],
        "ready"
    );
}

#[test]
fn a_receipt_is_laid_once_into_the_day_it_happened_on_and_read_back_with_it() {
    let w = world("UTC", true);
    let now = at("2026-10-20T09:00:00Z");
    turn(w.path(), json!({}), now).unwrap();
    let input = json!({"receipt_id":"receipt:files.write:1","act":"central.files.write","owner":"Central",
        "subject":"Control/user/flows/harbour.html","source":PAGE,"receipt":{"revision":"r2"}});
    let laid = lay(w.path(), &input, now).unwrap();
    assert_eq!(laid["recorded"], true);
    assert_eq!(laid["civil_date"], "2026-10-20");
    assert_eq!(laid["entry"]["schema"], "central.day-receipt/v1");
    assert_eq!(laid["entry"]["occurred_at_unix_seconds"], now);
    assert_eq!(laid["entry"]["recorded_by"], "human:test");
    // The same receipt is laid once.
    let again = lay(w.path(), &input, now + 5).unwrap();
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
fn a_receipt_is_laid_only_on_a_credential_that_grants_it() {
    let w = world("UTC", true);
    let now = at("2026-10-20T09:00:00Z");
    turn(w.path(), json!({}), now).unwrap();
    let input = json!({"receipt_id":"receipt:unproven","act":"central.files.write","owner":"Central","subject":"s"});
    for token in [None, Some("someone-elses-token-not-a-live-credential-0123")] {
        let refused =
            execute_with_token_at(w.path(), "day_receipt_record", &input, token, now).unwrap_err();
        assert_eq!(refused.kind(), io::ErrorKind::PermissionDenied, "{refused}");
    }
    assert_eq!(read(w.path(), json!({}), now)["receipts"], json!([]));
    assert!(!w
        .path()
        .join("Control/user/day/2026-10-20/receipts.jsonl")
        .exists());
}

#[test]
fn a_receipt_keeps_the_time_it_happened_within_todays_day() {
    let w = world("UTC", true);
    let happened = at("2026-10-20T08:00:00Z");
    let now = at("2026-10-20T09:00:00Z");
    turn(w.path(), json!({}), now).unwrap();
    let laid = lay(
        w.path(),
        &json!({"receipt_id":"receipt:later","act":"aikit.encounter.prompt","owner":"AIKit","subject":"agent-session:a",
            "occurred_at_unix_seconds":happened}),
        now,
    )
    .unwrap();
    assert_eq!(laid["civil_date"], "2026-10-20");
    assert_eq!(laid["entry"]["occurred_at_unix_seconds"], happened);
    assert_eq!(laid["entry"]["recorded_at_unix_seconds"], now);
    assert_eq!(
        read(w.path(), json!({"day_ref":FIRST}), now)["receipts"][0]["receipt_id"],
        "receipt:later"
    );
}

#[test]
fn a_receipt_out_of_its_bounds_is_refused_in_words() {
    let w = world("UTC", true);
    let now = at("2026-10-21T00:00:30Z");
    turn(w.path(), json!({}), at("2026-10-20T23:59:00Z")).unwrap();
    turn(w.path(), json!({}), now).unwrap();
    let receipt = |occurred: u64| json!({"receipt_id":"r","act":"a","owner":"Central","subject":"s","occurred_at_unix_seconds":occurred});
    for (input, words) in [
        // Before today's Day opened: yesterday's last minute, and the epoch.
        (receipt(at("2026-10-20T23:59:59Z")), "today's Day"),
        (receipt(0), "today's Day"),
        // After now, and after today's Day turns.
        (receipt(now + 3600), "no later than now"),
        (receipt(at("2026-10-22T00:00:00Z")), "no later than now"),
        (
            json!({"receipt_id":"r","owner":"Central","subject":"s"}),
            "act is text of at most 256 bytes",
        ),
        (
            json!({"receipt_id":"r","act":"a","owner":"Central","subject":"s","receipt":{"bulk":"x".repeat(17 * 1024)}}),
            "receipt is at most 16 KiB of JSON",
        ),
    ] {
        let refused = lay(w.path(), &input, now).unwrap_err();
        assert_eq!(refused.kind(), io::ErrorKind::InvalidInput, "{refused}");
        assert!(
            refused.to_string().contains(words),
            "{refused} should say {words}"
        );
    }
    // Nothing was laid in either Day, nor in a Day of 1970.
    assert_eq!(
        read(w.path(), json!({"day_ref":FIRST}), now)["receipts"],
        json!([])
    );
    assert_eq!(read(w.path(), json!({}), now)["receipts"], json!([]));
    assert!(!w.path().join("Control/user/day/1970-01-01").exists());
}

#[test]
fn the_turn_and_the_receipt_stand_in_centrals_action_field() {
    // The receipt is an authenticated Action, and the grant Setup gives (at the root and at each project) holds it.
    assert!(authority::AUTHENTICATED_ACTIONS.contains(&"central.day.receipt.record"));
    assert!(authority::PROJECT_AUTHENTICATED_ACTIONS.contains(&"central.day.receipt.record"));
    assert!(crate::ground_founding::setup_root_actions().contains(&"central.day.receipt.record"));
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
