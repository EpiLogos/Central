//! Native Plural Flow contribution append (O:I #558). Every append, replay and
//! sequence case of the shared conformance file runs through the real
//! operation against a real Flow file; the cases' expectations are independent
//! of this implementation (they are the same cases O:I's own form runs). The
//! concurrency and recovery tests below are what only a native owner can show.
use central_ctrl::continuous_work::source::Scope;
use central_ctrl::file_mutation::{flow_append_with_token, FlowAppendError};
use central_ctrl::{
    create_core_action_registry, ActionExecutionContext, ConnectorContext, ConnectorRegistry,
    ResultStatus, RootOptions,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Barrier,
    },
    thread,
    time::{SystemTime, UNIX_EPOCH},
};

const SERVICE: &str = "flow-service-test-credential-not-a-real-secret";
const ANN: &str = "flow-ann-test-credential-not-a-real-secret";
const MALLORY: &str = "flow-mallory-test-credential-not-a-real-secret";
static NEXT: AtomicU64 = AtomicU64::new(0);

struct World(PathBuf);
impl Drop for World {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn world() -> World {
    let path = std::env::temp_dir().join(format!(
        "central-flow-append-{}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    central_ctrl::initialize_central(&path).unwrap();
    let grant = |token: &str, principal: &str, kind: &str| {
        json!({
            "principal_ref": principal, "actor_kind": kind,
            "token_sha256": format!("{:x}", Sha256::digest(token.as_bytes())),
            "scope_refs": ["control:root"], "actions": ["central.flow.append"],
            "expires_at_unix_seconds": u64::MAX / 2,
        })
    };
    let authority = json!({
        "schema": "central.native-action-authority/v1", "scope_ref": "control:root",
        "grants": [grant(SERVICE, "native-service:aikit-encounter", "native-service"),
                   grant(ANN, "human:ann", "human"), grant(MALLORY, "human:mallory", "human")],
    });
    let scope = Scope::resolve(&path, None).unwrap();
    let source = "Control/user/authority.json";
    fs::write(
        path.join(source),
        serde_json::to_vec_pretty(&authority).unwrap(),
    )
    .unwrap();
    fs::create_dir_all(path.join("Control/relations")).unwrap();
    fs::write(
        path.join("Control/relations/source-relations.json"),
        serde_json::to_vec_pretty(&json!({
            "schema": "central.control.ground-relations/v1", "project_id": "control:root",
            "relations": [{"ref": scope.source_ref(source), "path": source, "roles": ["native-action-authority"],
                "provenance": "human-adopted", "standing": "architecture-contract", "treatment": "projectcentral-user",
                "recognition": "controlled-test-fixture-not-personal-adoption", "recorded_at_unix_seconds": 1}],
        }))
        .unwrap(),
    )
    .unwrap();
    fs::create_dir_all(path.join("Control/user/flows")).unwrap();
    World(path.canonicalize().unwrap())
}
fn location(root: &Path, name: &str) -> Value {
    let path = format!("Control/user/flows/{name}");
    json!({"schema": "central.path-ref/v1", "ref": format!("central:path:{}:{path}", root.display()),
           "root": root.display().to_string(), "path": path})
}
const SHELL_OPEN: &str = "<!doctype html><html><head><title>Flow</title></head><body><main id=\"app\"></main>\n<script type=\"application/json\" id=\"ql-doc\">";
const SHELL_CLOSE: &str = "</script>\n<script>/* page */</script></body></html>";
fn flow_html(doc: &Value) -> String {
    format!(
        "{SHELL_OPEN}{}{SHELL_CLOSE}",
        serde_json::to_string(doc).unwrap()
    )
}
fn island(html: &str) -> Value {
    let start = html.find(SHELL_OPEN).expect("island") + SHELL_OPEN.len();
    let end = html[start..].find("</script>").unwrap() + start;
    serde_json::from_str(&html[start..end]).unwrap()
}
fn call(root: &Path, action: &str, input: Value) -> central_ctrl::ActionResult {
    let registry = create_core_action_registry();
    let options = RootOptions {
        explicit_root: Some(root.into()),
        ..RootOptions::default()
    };
    let connectors = ConnectorRegistry::default();
    let connector_context = ConnectorContext {
        platform: "test".into(),
    };
    registry.execute(
        action,
        &input,
        &ActionExecutionContext {
            root_options: &options,
            connectors: &connectors,
            connector_context: &connector_context,
        },
    )
}
fn place(root: &Path, name: &str, doc: &Value) -> Value {
    let loc = location(root, name);
    let created = call(
        root,
        "central.files.write",
        json!({
        "location": loc, "expected_revision": "", "content": flow_html(doc),
        "actor": "human:desktop", "actor_kind": "human"}),
    );
    assert_eq!(created.status, ResultStatus::Success, "{created:?}");
    loc
}
fn read_doc(root: &Path, loc: &Value) -> Value {
    let read = call(root, "central.files.read", json!({"location": loc}));
    assert_eq!(read.status, ResultStatus::Success, "{read:?}");
    island(read.data.unwrap()["content"].as_str().unwrap())
}

/// A case caller in the owner's terms: the body says who it claims to be; only
/// a host-held credential authenticates it.
fn input_for(loc: &Value, caller: &Value, request: &Value) -> (Value, Option<&'static str>) {
    let kind = caller["kind"].as_str().unwrap();
    let authenticated = caller["authenticated"].as_bool().unwrap_or(false);
    let mut input = json!({
        "location": loc, "operation_ref": request["operationRef"], "author_key": request["authorKey"],
        "html": request["html"], "at": request["at"],
    });
    for (from, to) in [
        ("addressees", "addressees"),
        ("audience", "audience"),
        ("intent", "intent"),
        ("relations", "relations"),
        ("basisRevision", "basis_revision"),
        ("attribution", "attribution"),
        ("entryId", "entry_id"),
    ] {
        if let Some(value) = request.get(from) {
            input[to] = value.clone();
        }
    }
    let token = match kind {
        "agent" => {
            input["actor"] = caller["ref"].clone();
            input["actor_kind"] = json!("agent");
            input["agent_session_ref"] = caller["session"].clone();
            if let Some(v) = caller.get("agent") {
                input["agent_ref"] = v.clone();
            }
            if let Some(v) = caller.get("generation") {
                input["generation"] = v.clone();
            }
            if let Some(v) = caller.get("workcell") {
                input["workcell"] = v.clone();
            }
            authenticated.then_some(SERVICE)
        }
        "human" => {
            let reference = caller
                .get("ref")
                .and_then(Value::as_str)
                .unwrap_or("human:desktop");
            input["actor"] = json!(reference);
            input["actor_kind"] = json!("human");
            if authenticated {
                Some(if reference == "human:ann" {
                    ANN
                } else {
                    MALLORY
                })
            } else {
                None
            }
        }
        _ => {
            input["actor"] = json!("system");
            input["actor_kind"] = json!("system");
            None
        }
    };
    (input, token)
}
fn run(root: &Path, input: &Value, token: Option<&str>) -> Result<Value, (String, String)> {
    match flow_append_with_token(root, input, token, 100) {
        Ok(done) => Ok(done),
        Err(FlowAppendError::Refused { code, message }) => Err((code, message)),
        Err(FlowAppendError::Io(error)) => panic!("owner condition, not a refusal: {error}"),
    }
}

fn cases() -> Vec<Value> {
    let raw = fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/plural-flow-cases.json"
    ))
    .unwrap();
    serde_json::from_str::<Value>(&raw).unwrap()["cases"]
        .as_array()
        .unwrap()
        .clone()
}
fn doc_of(case: &Value) -> Value {
    let mut doc = case["doc"].clone();
    for (path, value) in case
        .get("mutate")
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
    {
        let mut parts: Vec<&str> = path.split('.').collect();
        let last = parts.pop().unwrap();
        let mut at = &mut doc;
        for part in parts {
            at = &mut at[part];
        }
        at[last] = value.clone();
    }
    doc
}

#[test]
fn every_shared_conformance_case_holds_natively() {
    let mut ran = 0;
    for case in cases() {
        let name = case["name"].as_str().unwrap().to_string();
        let op = case["op"].as_str().unwrap();
        if !matches!(op, "append" | "replay" | "sequence") {
            continue;
        }
        ran += 1;
        let w = world();
        let loc = place(&w.0, "flow-case.html", &doc_of(&case));
        let expect = &case["expect"];
        match op {
            "append" => {
                let (input, token) = input_for(&loc, &case["caller"], &case["request"]);
                let result = run(&w.0, &input, token);
                if let Some(code) = expect["refusal"].as_str() {
                    let refused = result.expect_err(&format!("{name}: expected refusal {code}"));
                    assert_eq!(refused.0, code, "{name}: {}", refused.1);
                    // A refusal never writes: the document is byte-identical.
                    assert_eq!(
                        read_doc(&w.0, &loc),
                        doc_of(&case),
                        "{name}: refusal must not mutate"
                    );
                    continue;
                }
                let done = result.unwrap_or_else(|e| panic!("{name}: {e:?}"));
                assert_eq!(done["outcome"], expect["outcome"], "{name}");
                let entry = &done["entry"];
                if let Some(author) = expect["author"].as_str() {
                    assert_eq!(entry["authorKey"], author, "{name}");
                }
                if let Some(basis) = expect["attribution"].as_str() {
                    assert_eq!(entry["attribution"]["basis"], basis, "{name}");
                }
                if let Some(session) = expect["session"].as_str() {
                    assert_eq!(entry["attribution"]["session"], session, "{name}");
                }
                if let Some(reply) = expect["replyTo"].as_str() {
                    assert_eq!(entry["replyTo"]["entryId"], reply, "{name}");
                }
                if let Some(basis) = expect.get("basisRevision") {
                    assert_eq!(&entry["basisRevision"], basis, "{name}");
                }
                if let Some(list) = expect["converges"].as_array() {
                    let got: Vec<&Value> = entry["relations"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .filter(|r| r["type"] == "converge")
                        .map(|r| &r["entryId"])
                        .collect();
                    assert_eq!(got, list.iter().collect::<Vec<_>>(), "{name}");
                }
                let after = read_doc(&w.0, &loc);
                let before = doc_of(&case);
                assert_eq!(
                    after["meta"]["revision"].as_i64().unwrap(),
                    before["meta"]["revision"].as_i64().unwrap() + 1,
                    "{name}"
                );
                let kept = before["entries"].as_array().unwrap().len();
                assert_eq!(
                    after["entries"].as_array().unwrap()[..kept],
                    before["entries"].as_array().unwrap()[..],
                    "{name}: earlier entries survive"
                );
                for collection in ["notes", "packet", "media", "journal"] {
                    assert_eq!(
                        after[collection], before[collection],
                        "{name}: {collection}"
                    );
                }
                for (key, reference) in expect["binds"].as_object().into_iter().flatten() {
                    let held = after["meta"]["participants"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .find(|p| &p["key"] == key)
                        .unwrap();
                    assert_eq!(held["binding"]["ref"], *reference, "{name}");
                }
                for key in expect["noBinding"].as_array().into_iter().flatten() {
                    let held = after["meta"]["participants"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .find(|p| &p["key"] == key)
                        .unwrap();
                    assert_ne!(
                        held["binding"]["basis"], "verified",
                        "{name}: an unauthenticated caller binds no one"
                    );
                }
            }
            "replay" => {
                let (input, token) = input_for(&loc, &case["caller"], &case["request"]);
                run(&w.0, &input, token).unwrap_or_else(|e| panic!("{name}: first {e:?}"));
                let mut again_request = case["request"].clone();
                for (key, value) in case["replayRequest"].as_object().unwrap() {
                    again_request[key] = value.clone();
                }
                let (again, token) = input_for(&loc, &case["caller"], &again_request);
                let second = run(&w.0, &again, token);
                match expect["second"].as_str().unwrap() {
                    "recovered" => {
                        let second = second.unwrap_or_else(|e| panic!("{name}: {e:?}"));
                        assert_eq!(second["outcome"], "recovered", "{name}");
                        let after = read_doc(&w.0, &loc);
                        assert_eq!(
                            after["entries"].as_array().unwrap().len() as u64,
                            expect["entries"].as_u64().unwrap(),
                            "{name}"
                        );
                        assert_eq!(
                            after["meta"]["revision"].as_u64().unwrap(),
                            expect["revision"].as_u64().unwrap(),
                            "{name}"
                        );
                    }
                    refused => assert_eq!(
                        format!("refusal:{}", second.expect_err(&name).0),
                        refused,
                        "{name}"
                    ),
                }
            }
            _ => {
                for step in case["steps"].as_array().unwrap() {
                    let (input, token) = input_for(&loc, &step["caller"], &step["request"]);
                    run(&w.0, &input, token).unwrap_or_else(|e| panic!("{name}: {e:?}"));
                }
                let after = read_doc(&w.0, &loc);
                assert_eq!(
                    after["entries"].as_array().unwrap().len() as u64,
                    expect["entries"].as_u64().unwrap(),
                    "{name}"
                );
                assert_eq!(
                    after["meta"]["revision"].as_u64().unwrap(),
                    expect["revision"].as_u64().unwrap(),
                    "{name}"
                );
                for (operation, parent) in expect["replyParents"].as_object().unwrap() {
                    let entry = after["entries"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .find(|e| e["request"]["ref"] == *operation)
                        .unwrap();
                    assert_eq!(entry["replyTo"]["entryId"], *parent, "{name}");
                }
            }
        }
    }
    assert!(
        ran >= 20,
        "the shared append/replay/sequence cases ran ({ran})"
    );
}

fn base_case() -> Value {
    cases()
        .into_iter()
        .find(|c| c["name"] == "same-initial-distinct-keys")
        .unwrap()
}
fn ada() -> Value {
    json!({"kind": "agent", "ref": "agent-session/ada-1", "session": "agent-session/ada-1", "agent": "agent/ada", "authenticated": true})
}
fn ash() -> Value {
    json!({"kind": "agent", "ref": "agent-session/ash-1", "session": "agent-session/ash-1", "agent": "agent/ash", "authenticated": true})
}
fn req(op: &str, key: &str, html: &str) -> Value {
    json!({"operationRef": op, "authorKey": key, "html": html, "at": "2026-09-30T09:00:00.000Z", "relations": [{"type": "reply", "entryId": "e-q"}]})
}

#[test]
fn concurrent_agent_appends_both_survive_an_external_human_edit() {
    let w = world();
    let loc = place(&w.0, "flow-race.html", &doc_of(&base_case()));
    // Several independent appends race, each through the real CAS; the loser of
    // any round rereads the winner's bytes and appends again, so none is lost.
    let barrier = Arc::new(Barrier::new(6));
    let handles: Vec<_> = (0..6)
        .map(|i| {
            let (root, loc, barrier) = (w.0.clone(), loc.clone(), barrier.clone());
            thread::spawn(move || {
                let (caller, key) = if i % 2 == 0 {
                    (ada(), "p-ada")
                } else {
                    (ash(), "p-ash")
                };
                let (input, token) = input_for(
                    &loc,
                    &caller,
                    &req(&format!("op-race-{i}"), key, &format!("<p>answer {i}</p>")),
                );
                barrier.wait();
                run(&root, &input, token).unwrap_or_else(|e| panic!("append {i}: {e:?}"))
            })
        })
        .collect();
    for handle in handles {
        assert_eq!(handle.join().unwrap()["outcome"], "appended");
    }
    let after = read_doc(&w.0, &loc);
    let entries = after["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 7, "the question plus six distinct answers");
    let mut bodies: Vec<&str> = entries[1..]
        .iter()
        .map(|e| e["html"].as_str().unwrap())
        .collect();
    bodies.sort();
    assert_eq!(
        bodies,
        (0..6)
            .map(|i| format!("<p>answer {i}</p>"))
            .collect::<Vec<_>>()
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
    );
    assert_eq!(after["meta"]["revision"], 6);
    // A human edit of the whole file after the appends is ordinary CAS, and
    // the next append builds on it without overwriting it.
    let mut edited = after.clone();
    edited["entries"][0]["html"] = json!("<p>What does the passage actually claim?</p>");
    let current = call(&w.0, "central.files.read", json!({"location": loc}))
        .data
        .unwrap();
    let html = current["content"].as_str().unwrap();
    let wrote = call(
        &w.0,
        "central.files.write",
        json!({
        "location": loc, "expected_revision": current["revision"], "content": html.replace(&serde_json::to_string(&after).unwrap(), &serde_json::to_string(&edited).unwrap()),
        "actor": "human:desktop", "actor_kind": "human"}),
    );
    assert_eq!(wrote.status, ResultStatus::Success, "{wrote:?}");
    let (input, token) = input_for(
        &loc,
        &ada(),
        &req("op-after-edit", "p-ada", "<p>late answer</p>"),
    );
    run(&w.0, &input, token).unwrap();
    let final_doc = read_doc(&w.0, &loc);
    assert_eq!(
        final_doc["entries"][0]["html"], "<p>What does the passage actually claim?</p>",
        "the current human bytes survive"
    );
    assert_eq!(final_doc["entries"].as_array().unwrap().len(), 8);
}

#[test]
fn a_replay_after_an_ambiguous_outcome_recovers_the_same_entry() {
    // The caller cannot tell whether the append landed (process died after the
    // commit): resending the same operation recovers it, never duplicates it.
    let w = world();
    let loc = place(&w.0, "flow-replay.html", &doc_of(&base_case()));
    let (input, token) = input_for(&loc, &ada(), &req("op-ambiguous", "p-ada", "<p>once</p>"));
    let first = run(&w.0, &input, token).unwrap();
    let revision = read_doc(&w.0, &loc)["meta"]["revision"].clone();
    let again = run(&w.0, &input, token).unwrap();
    assert_eq!(first["outcome"], "appended");
    assert_eq!(again["outcome"], "recovered");
    assert_eq!(again["entry"]["id"], first["entry"]["id"]);
    let after = read_doc(&w.0, &loc);
    assert_eq!(after["entries"].as_array().unwrap().len(), 2);
    assert_eq!(after["meta"]["revision"], revision);
    let (changed, token) = input_for(
        &loc,
        &ada(),
        &req("op-ambiguous", "p-ada", "<p>different</p>"),
    );
    assert_eq!(
        run(&w.0, &changed, token).unwrap_err().0,
        "request-conflict"
    );
}

#[test]
fn the_registered_action_is_unauthenticated_by_default_and_never_verifies() {
    let w = world();
    let loc = place(&w.0, "flow-action.html", &doc_of(&base_case()));
    let mut caller = ada();
    caller["authenticated"] = json!(false);
    let (input, _) = input_for(
        &loc,
        &caller,
        &req("op-action", "p-ada", "<p>declared only</p>"),
    );
    let result = call(&w.0, "central.flow.append", input);
    assert_eq!(result.status, ResultStatus::Success, "{result:?}");
    let data = result.data.unwrap();
    assert_eq!(data["outcome"], "appended");
    assert_eq!(data["entry"]["attribution"]["basis"], "declared");
    let refused = call(
        &w.0,
        "central.flow.append",
        json!({
        "location": loc, "operation_ref": "op-bad", "author_key": "p-nobody", "html": "<p>x</p>", "at": "2026-09-30T09:00:00.000Z",
        "actor": "human:desktop", "actor_kind": "human"}),
    );
    assert_eq!(refused.status, ResultStatus::InvalidInput);
    let error = refused.error.unwrap();
    assert_eq!(error.code, "unknown-author");
    assert_eq!(error.details.unwrap()["written"], false);
}

#[test]
fn append_is_bounded_to_flow_instances_and_keeps_the_page_shell() {
    let w = world();
    let loc = place(&w.0, "flow-shell.html", &doc_of(&base_case()));
    let read = call(&w.0, "central.files.read", json!({"location": loc}))
        .data
        .unwrap();
    let before = read["content"].as_str().unwrap().to_string();
    let (input, token) = input_for(
        &loc,
        &ada(),
        &req(
            "op-shell",
            "p-ada",
            "<p>it is <!-- a --> comment</p></script>",
        ),
    );
    run(&w.0, &input, token).unwrap();
    let after = call(&w.0, "central.files.read", json!({"location": loc}))
        .data
        .unwrap();
    let html = after["content"].as_str().unwrap();
    assert!(
        html.starts_with(SHELL_OPEN)
            && html.contains("<main id=\"app\">")
            && html.ends_with(SHELL_CLOSE),
        "the page shell is untouched"
    );
    assert_eq!(
        &before[..before.find("<script type=\"application/json\"").unwrap()],
        &html[..html.find("<script type=\"application/json\"").unwrap()]
    );
    assert_eq!(
        island(html)["entries"][1]["html"],
        "<p>it is <!-- a --> comment</p></script>",
        "script and comment text survive the island escaping"
    );
    let outside = json!({"schema": "central.path-ref/v1", "ref": format!("central:path:{}:notes/x.html", w.0.display()), "root": w.0.display().to_string(), "path": "notes/x.html"});
    let (mut input, token) = input_for(&outside, &ada(), &req("op-out", "p-ada", "<p>x</p>"));
    input["location"] = outside;
    assert!(
        matches!(
            flow_append_with_token(&w.0, &input, token, 100),
            Err(FlowAppendError::Io(_))
        ),
        "a file outside Control/user/flows is refused"
    );
}
