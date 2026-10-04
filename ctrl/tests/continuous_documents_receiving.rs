use central_ctrl::continuous_work::{execute_at, execute_with_token_at, source::Scope};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs, io,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Barrier,
    },
    thread,
    time::{SystemTime, UNIX_EPOCH},
};

const HUMAN: &str = "document-human-test-credential-not-a-real-secret";
const AGENT: &str = "document-agent-test-credential-not-a-real-secret";
const OTHER: &str = "document-other-test-credential-not-a-real-secret";
static NEXT: AtomicU64 = AtomicU64::new(0);
struct World(PathBuf);
impl World {
    fn path(&self) -> &Path {
        &self.0
    }
}
impl Drop for World {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn call(root: &Path, op: &str, input: &Value, token: &str) -> io::Result<Value> {
    execute_with_token_at(root, op, input, Some(token), 100)
}
fn world() -> World {
    let path = std::env::temp_dir().join(format!(
        "central-doc-receiving-{}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    configure_world(&path);
    World(path)
}
fn configure_world(path: &Path) {
    central_ctrl::initialize_central(path).unwrap();
    for name in ["one", "two"] {
        let project = path.join(format!("Work/{name}/ProjectCentral"));
        fs::create_dir_all(project.join("user")).unwrap();
        fs::write(
            project.join("project.json"),
            serde_json::to_vec(&central_ctrl::ProjectCentralManifest::new(format!(
                "test/{name}"
            )))
            .unwrap(),
        )
        .unwrap();
    }
    let grants:Vec<Value>=[
        (HUMAN,"human:test","human"),
        (AGENT,"agent:test","agent"),
        (OTHER,"agent:other","agent"),
    ].into_iter().map(|(token,principal_ref,actor_kind)|json!({
        "principal_ref":principal_ref,"actor_kind":actor_kind,
        "token_sha256":format!("{:x}",Sha256::digest(token.as_bytes())),"scope_refs":["control:root","project:test/one","project:test/two"],
        "actions":["central.day.ensure","central.day.lifecycle","central.now.lifecycle","central.document.create","central.document.mutate","central.receiving.submit","central.receiving.review","central.receiving.include","central.receiving.recover"],"expires_at_unix_seconds":9999
    })).collect();
    let policies = [
        (
            "placement.json",
            "work-placement-policy",
            json!({"schema":"central.work-placement-policy/v1","scope_ref":"control:root","writable":[{"path":"Work/one","class":"repository"},{"path":"Work/two","class":"repository"}],"enforcement":"native-actions","required_coverage":["file-content"],"lease_seconds":300}),
        ),
        (
            "time.json",
            "civil-time-policy",
            json!({"schema":"central.civil-time-policy/v1","scope_ref":"control:root","timezone":"Europe/London","day_boundary_minutes":0,"automatic_day_rollover":true}),
        ),
        (
            "authority.json",
            "native-action-authority",
            json!({"schema":"central.native-action-authority/v1","scope_ref":"control:root","grants":grants}),
        ),
    ];
    let scope = Scope::resolve(path, None).unwrap();
    let mut relations = vec![];
    for (name, role, value) in policies {
        let source = format!("Control/user/{name}");
        fs::write(
            path.join(&source),
            serde_json::to_vec_pretty(&value).unwrap(),
        )
        .unwrap();
        relations.push(json!({"ref":scope.source_ref(&source),"path":source,"roles":[role],"provenance":"human-adopted","standing":"architecture-contract","treatment":"projectcentral-user","recognition":"controlled-test-fixture-not-personal-adoption","recorded_at_unix_seconds":1}));
    }
    fs::create_dir_all(path.join("Control/relations")).unwrap();
    fs::write(path.join("Control/relations/source-relations.json"),serde_json::to_vec_pretty(&json!({"schema":"central.control.ground-relations/v1","project_id":"control:root","relations":relations})).unwrap()).unwrap();
}

fn document(root: &Path, project: Option<&str>, kind: &str, id: &str) -> Value {
    let policy = execute_at(root, "policy", &json!({"project":project}), 100).unwrap();
    // These are explicitly supplied test data, not asserted original Day HTML keys.
    let mut input = json!({"project":project,"kind":kind,"document_id":id,"title":"","expected_policy_revision":policy["revision"],"template_payload":{"actual_supplied_test_field":"human value","unknown_nested":{"preserved":true}},"fields":[{"id":"test-field","label":"Supplied test field","template_pointer":"/actual_supplied_test_field"}]});
    if kind == "day" {
        let time = execute_at(root, "time_policy", &json!({"project":project}), 100).unwrap();
        let day = call(
            root,
            "day_ensure",
            &json!({"project":project,"expected_time_policy_revision":time["revision"]}),
            HUMAN,
        )
        .unwrap();
        input["day_ref"] = day["day_ref"].clone();
        input["expected_revision"] = day["revision"]["revision"].clone();
    }
    call(root, "document_create", &input, HUMAN).unwrap()
}
fn read(root: &Path, doc: &Value, project: Option<&str>) -> Value {
    execute_at(root,"document_read",&json!({"project":project,"source_ref":doc["source"]["ref"],"document_id":doc["document_id"]}),100).unwrap()
}
fn mutation(doc: &Value, project: Option<&str>, request: &str, operation: &str) -> Value {
    json!({"project":project,"source_ref":doc["source"]["ref"],"document_id":doc["document_id"],"expected_revision":doc["revision"]["revision"],"request_id":request,"operation":operation})
}
fn proposal(doc: &Value, project: Option<&str>, producer: &str, entry: &str) -> Value {
    json!({"project":project,"producer_key":producer,"source_ref":doc["source"]["ref"],"document_id":doc["document_id"],"expected_source_revision":doc["revision"]["revision"],"occurred_at_unix_seconds":42,"task_ref":"task:retained","session_ref":"session:finished","proposal":{"operation":"entry.add","entry_id":entry,"contribution_id":format!("{entry}:contribution"),"html":"<p>Reviewed contribution</p>"}})
}
fn review(root: &Path, received: &Value, doc: &Value, project: Option<&str>) -> Value {
    call(root,"receiving_review",&json!({"project":project,"return_ref":received["return_ref"],"expected_return_revision":received["revision"],"expected_source_revision":doc["revision"]["revision"],"disposition":"accepted"}),HUMAN).unwrap()
}
fn inclusion(received: &Value, doc: &Value, project: Option<&str>) -> Value {
    json!({"project":project,"return_ref":received["return_ref"],"expected_return_revision":received["revision"],"expected_source_revision":doc["revision"]["revision"]})
}

#[test]
fn shared_flow_and_dialogue_use_the_existing_register_and_protect_human_edits() {
    let world = world();
    for (project, kind) in [(None, "flow"), (Some("one"), "dialogue")] {
        let doc = document(world.path(), project, kind, "doc:shared");
        assert_eq!(doc["document"]["title"], "");
        let mut add = mutation(&doc, project, "add:one", "entry.add");
        add["entry_id"] = json!("entry:one");
        add["contribution_id"] = json!("part:one");
        add["html"] = json!("<p>Agent writes <strong>this</strong></p>");
        let added = call(world.path(), "document_mutate", &add, AGENT).unwrap();
        assert_eq!(
            added["document"]["contributions"][0]["author_ref"],
            "agent:test"
        );
        let mut patch = mutation(&added, project, "patch:other", "contribution.patch");
        patch["contribution_id"] = json!("part:one");
        patch["html"] = json!("not mine");
        patch["actor_kind"] = json!("human");
        assert_eq!(
            call(world.path(), "document_mutate", &patch, OTHER)
                .unwrap_err()
                .kind(),
            io::ErrorKind::PermissionDenied
        );
        patch["request_id"] = json!("human:touch");
        let touched = call(world.path(), "document_mutate", &patch, HUMAN).unwrap();
        assert_eq!(
            touched["document"]["contributions"][0]["human_touched"],
            true
        );
        assert_eq!(touched["document"]["contributions"][0]["locked"], true);
        patch["expected_revision"] = touched["revision"]["revision"].clone();
        patch["request_id"] = json!("agent:after-human");
        assert_eq!(
            call(world.path(), "document_mutate", &patch, AGENT)
                .unwrap_err()
                .kind(),
            io::ErrorKind::PermissionDenied
        );
        assert_eq!(
            read(world.path(), &doc, project)["revision"],
            touched["revision"]
        );
    }
}
#[test]
fn document_operations_replay_exactly_and_external_human_bytes_block_agent_overwrite() {
    let world = world();
    let doc = document(world.path(), None, "flow", "doc:replay");
    let mut add = mutation(&doc, None, "add:replay", "entry.add");
    add["entry_id"] = json!("entry:one");
    add["contribution_id"] = json!("part:one");
    add["html"] = json!("one");
    let first = call(world.path(), "document_mutate", &add, AGENT).unwrap();
    let replay = call(world.path(), "document_mutate", &add, AGENT).unwrap();
    assert_eq!(first["revision"], replay["revision"]);
    assert_eq!(
        replay["document"]["contributions"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let path = world.path().join(doc["source"]["path"].as_str().unwrap());
    let mut actual = fs::read_to_string(&path).unwrap();
    actual.push('\n');
    fs::write(&path, &actual).unwrap();
    let external = read(world.path(), &doc, None);
    assert_eq!(external["unreviewed_external_revision"], true);
    let mut patch = mutation(&external, None, "external:patch", "contribution.patch");
    patch["contribution_id"] = json!("part:one");
    patch["html"] = json!("overwrite");
    assert_eq!(
        call(world.path(), "document_mutate", &patch, AGENT)
            .unwrap_err()
            .kind(),
        io::ErrorKind::PermissionDenied
    );
    assert_eq!(fs::read_to_string(path).unwrap(), actual);
}
#[test]
fn human_day_rejects_direct_agent_changes_preserves_supplied_fields_and_exports_inert_snapshot() {
    let world = world();
    let doc = document(world.path(), None, "day", "doc:day");
    assert_eq!(
        doc["document"]["template_payload"]["unknown_nested"]["preserved"],
        true
    );
    let mut append = mutation(&doc, None, "day:agent", "field.append");
    append["field_id"] = json!("test-field");
    append["contribution_id"] = json!("part:day");
    append["html"]=json!("<p>text<script>bad()</script><img src='https://example.invalid/track' onerror='bad()'><strong>safe</strong></p>");
    assert_eq!(
        call(world.path(), "document_mutate", &append, AGENT)
            .unwrap_err()
            .kind(),
        io::ErrorKind::PermissionDenied
    );
    append["request_id"] = json!("day:human");
    let saved = call(world.path(), "document_mutate", &append, HUMAN).unwrap();
    let html = saved["document"]["contributions"][0]["html"]
        .as_str()
        .unwrap();
    for unsafe_text in ["<script", "<img", "onerror", "https://"] {
        assert!(!html.contains(unsafe_text));
    }
    assert!(html.contains("<strong>safe</strong>"));
    let export = execute_at(
        world.path(),
        "document_export",
        &json!({"source_ref":doc["source"]["ref"],"document_id":doc["document_id"]}),
        100,
    )
    .unwrap();
    assert_eq!(export["executable_scripts"], false);
    assert_eq!(export["automatic_network_or_model_calls"], false);
    assert_eq!(
        export["snapshot"]["document"]["template_payload"],
        doc["document"]["template_payload"]
    );
    assert_eq!(
        export["snapshot"]["source_authority"],
        "retained-snapshot-not-live-authority"
    );
}
#[test]
fn missing_template_pointer_is_rejected_instead_of_manufacturing_a_fixture_key() {
    let world = world();
    let policy = execute_at(world.path(), "policy", &json!({}), 100).unwrap();
    let request = json!({"kind":"flow","document_id":"doc:missing","expected_policy_revision":policy["revision"],"template_payload":{"present":"value"},"fields":[{"id":"missing","label":"Missing","template_pointer":"/not-present"}]});
    assert_eq!(
        call(world.path(), "document_create", &request, HUMAN)
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidInput
    );
    assert!(!world.path().join(".central/flows.json").exists());
}
#[test]
fn legacy_declared_human_whole_file_write_cannot_bypass_document_ownership() {
    let world = world();
    let doc = document(world.path(), Some("one"), "flow", "doc:protected");
    let root = world.path().join("Work/one");
    let error = central_ctrl::write_world_source(
        &root,
        doc["source"]["ref"].as_str().unwrap(),
        doc["revision"]["revision"].as_str().unwrap(),
        "destroy human contributions",
        "H",
        "human",
        None,
    )
    .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
    assert_eq!(
        read(world.path(), &doc, Some("one"))["revision"],
        doc["revision"]
    );
}
#[test]
fn concurrent_distinct_receiving_and_same_producer_replay_have_no_lost_returns() {
    let world = world();
    let doc = document(world.path(), None, "day", "doc:concurrent");
    let before = fs::read(world.path().join(doc["source"]["path"].as_str().unwrap())).unwrap();
    let barrier = Arc::new(Barrier::new(8));
    let threads: Vec<_> = (0..8)
        .map(|i| {
            let root = world.path().to_path_buf();
            let input = proposal(&doc, None, &format!("producer:{i}"), &format!("entry:{i}"));
            let barrier = barrier.clone();
            thread::spawn(move || {
                barrier.wait();
                call(&root, "receiving_submit", &input, AGENT).unwrap()
            })
        })
        .collect();
    let results: Vec<_> = threads.into_iter().map(|t| t.join().unwrap()).collect();
    let refs: BTreeSet<_> = results
        .iter()
        .map(|r| r["return_ref"].as_str().unwrap())
        .collect();
    assert_eq!(refs.len(), 8);
    let sequences: BTreeSet<_> = results
        .iter()
        .map(|r| r["record"]["sequence"].as_u64().unwrap())
        .collect();
    assert_eq!(sequences.len(), 8);
    let original = results
        .iter()
        .find(|r| r["record"]["proposal"]["entry_id"] == "entry:0")
        .unwrap();
    let replay = call(
        world.path(),
        "receiving_submit",
        &proposal(&doc, None, "producer:0", "entry:0"),
        AGENT,
    )
    .unwrap();
    assert_eq!(replay["return_ref"], original["return_ref"]);
    assert_eq!(replay["revision"], original["revision"]);
    let first = execute_at(world.path(), "receiving_list", &json!({"limit":3}), 100).unwrap();
    assert_eq!(first["more"], true);
    assert_eq!(first["returns"].as_array().unwrap().len(), 3);
    let second = execute_at(
        world.path(),
        "receiving_list",
        &json!({"after":first["next_after"],"limit":20}),
        100,
    )
    .unwrap();
    assert_eq!(second["returns"].as_array().unwrap().len(), 5);
    assert_eq!(
        fs::read(world.path().join(doc["source"]["path"].as_str().unwrap())).unwrap(),
        before
    );
    assert_eq!(original["record"]["occurred_at_unix_seconds"], 42);
    assert_eq!(original["record"]["received_at_unix_seconds"], 100);
}
#[test]
fn concurrent_reviewed_inclusion_is_cas_safe_then_explicit_rereview_retains_both_contributions() {
    let world = world();
    let doc = document(world.path(), Some("one"), "day", "doc:review");
    let first = call(
        world.path(),
        "receiving_submit",
        &proposal(&doc, Some("one"), "producer:a", "entry:a"),
        AGENT,
    )
    .unwrap();
    let second = call(
        world.path(),
        "receiving_submit",
        &proposal(&doc, Some("one"), "producer:b", "entry:b"),
        OTHER,
    )
    .unwrap();
    let accepted = [
        review(world.path(), &first, &doc, Some("one")),
        review(world.path(), &second, &doc, Some("one")),
    ];
    assert_eq!(
        read(world.path(), &doc, Some("one"))["revision"],
        doc["revision"]
    );
    let barrier = Arc::new(Barrier::new(2));
    let threads: Vec<_> = accepted
        .iter()
        .map(|received| {
            let root = world.path().to_path_buf();
            let input = inclusion(received, &doc, Some("one"));
            let barrier = barrier.clone();
            thread::spawn(move || {
                barrier.wait();
                call(&root, "receiving_include", &input, HUMAN)
            })
        })
        .collect();
    let results: Vec<_> = threads.into_iter().map(|t| t.join().unwrap()).collect();
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
    assert_eq!(results.iter().filter(|r| r.is_err()).count(), 1);
    let lost = results.iter().position(Result::is_err).unwrap();
    let current = read(world.path(), &doc, Some("one"));
    let retained = execute_at(
        world.path(),
        "receiving_read",
        &json!({"project":"one","return_ref":accepted[lost]["return_ref"]}),
        100,
    )
    .unwrap();
    assert_eq!(retained["record"]["status"], "accepted");
    let rereview = review(world.path(), &retained, &current, Some("one"));
    let included = call(
        world.path(),
        "receiving_include",
        &inclusion(&rereview, &current, Some("one")),
        HUMAN,
    )
    .unwrap();
    assert_eq!(included["included"], true);
    let final_doc = read(world.path(), &doc, Some("one"));
    assert_eq!(
        final_doc["document"]["contributions"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    let authors: BTreeSet<_> = final_doc["document"]["contributions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["author_ref"].as_str().unwrap())
        .collect();
    assert_eq!(authors, BTreeSet::from(["agent:test", "agent:other"]));
}
#[test]
fn interrupted_inclusion_acknowledgement_recovers_without_duplicate_source_effect() {
    let world = world();
    let doc = document(world.path(), None, "day", "doc:lost-ack");
    let received = call(
        world.path(),
        "receiving_submit",
        &proposal(&doc, None, "producer:lost", "entry:lost"),
        AGENT,
    )
    .unwrap();
    let accepted = review(world.path(), &received, &doc, None);
    let included = call(
        world.path(),
        "receiving_include",
        &inclusion(&accepted, &doc, None),
        HUMAN,
    )
    .unwrap();
    let before = read(world.path(), &doc, None);
    // Recreate the actual persisted pre-ack phase after the independently
    // committed document effect, retaining the owner's real inclusion request.
    let area = world.path().join(".central/source-returns/contributions");
    let mut found = false;
    for entry in fs::read_dir(&area).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        let mut actual: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        if actual["return_ref"] == included["return_ref"] {
            assert!(actual["inclusion_request"].is_object());
            actual["status"] = json!("including");
            actual["applied_source_revision"] = Value::Null;
            fs::write(path, serde_json::to_vec_pretty(&actual).unwrap()).unwrap();
            found = true;
        }
    }
    assert!(found);
    let pending = execute_at(
        world.path(),
        "receiving_read",
        &json!({"return_ref":included["return_ref"]}),
        100,
    )
    .unwrap();
    let recovered = call(
        world.path(),
        "receiving_recover",
        &json!({"return_ref":pending["return_ref"],"expected_return_revision":pending["revision"]}),
        HUMAN,
    )
    .unwrap();
    assert_eq!(recovered["included"], true);
    assert_eq!(
        recovered["record"]["applied_source_revision"],
        included["record"]["applied_source_revision"]
    );
    let after = read(world.path(), &doc, None);
    assert_eq!(before["revision"], after["revision"]);
    assert_eq!(
        after["document"]["contributions"].as_array().unwrap().len(),
        1
    );
}
#[test]
fn uncommitted_inclusion_whose_basis_moved_returns_to_review_then_includes_at_new_basis() {
    use std::os::unix::fs::PermissionsExt;
    let world = world();
    let doc = document(world.path(), None, "day", "doc:moved-basis");
    let received = call(
        world.path(),
        "receiving_submit",
        &proposal(&doc, None, "producer:moved", "entry:moved"),
        AGENT,
    )
    .unwrap();
    let accepted = review(world.path(), &received, &doc, None);
    // The owner writes the intent, then the source write itself fails.
    let target = world.path().join(doc["source"]["path"].as_str().unwrap());
    fs::set_permissions(&target, fs::Permissions::from_mode(0o444)).unwrap();
    let failed = call(
        world.path(),
        "receiving_include",
        &inclusion(&accepted, &doc, None),
        HUMAN,
    )
    .unwrap_err();
    assert!(failed.to_string().contains("inclusion not confirmed"));
    let uncertain = execute_at(
        world.path(),
        "receiving_read",
        &json!({"return_ref":received["return_ref"]}),
        100,
    )
    .unwrap();
    assert_eq!(uncertain["record"]["status"], "uncertain");
    // The document advances past the recorded intent's basis.
    fs::set_permissions(&target, fs::Permissions::from_mode(0o644)).unwrap();
    let mut add = mutation(&doc, None, "human:advance", "entry.add");
    add["entry_id"] = json!("entry:human");
    add["contribution_id"] = json!("entry:human:contribution");
    add["html"] = json!("<p>Human moved on</p>");
    let advanced = call(world.path(), "document_mutate", &add, HUMAN).unwrap();
    assert_ne!(advanced["revision"], doc["revision"]);
    // Recovery establishes the intent never committed: back to review, error kept.
    let recovered = call(
        world.path(),
        "receiving_recover",
        &json!({"return_ref":uncertain["return_ref"],"expected_return_revision":uncertain["revision"]}),
        HUMAN,
    )
    .unwrap();
    assert_eq!(recovered["included"], false);
    assert_eq!(recovered["record"]["status"], "needs-review");
    assert_eq!(recovered["recovery"]["intent_committed"], false);
    assert!(recovered["record"]["last_error"]
        .as_str()
        .unwrap()
        .contains("later/unrelated revision"));
    assert!(recovered["record"]["inclusion_request"].is_null());
    assert_eq!(
        recovered["record"]["abandoned_inclusion_requests"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let unchanged = read(world.path(), &doc, None);
    assert_eq!(unchanged["revision"], advanced["revision"]);
    let contributions = |reading: &Value| -> Vec<String> {
        reading["document"]["contributions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["html"].as_str().unwrap().to_owned())
            .collect()
    };
    assert_eq!(contributions(&unchanged), ["<p>Human moved on</p>"]);
    // The abandoned intent is never finished, even by a repeated recover.
    assert_eq!(
        call(
            world.path(),
            "receiving_recover",
            &json!({"return_ref":recovered["return_ref"],"expected_return_revision":recovered["revision"]}),
            HUMAN,
        )
        .unwrap_err()
        .kind(),
        io::ErrorKind::AlreadyExists
    );
    // Explicit re-review at the new basis, then inclusion.
    let rereview = review(world.path(), &recovered, &unchanged, None);
    let included = call(
        world.path(),
        "receiving_include",
        &inclusion(&rereview, &unchanged, None),
        HUMAN,
    )
    .unwrap();
    assert_eq!(included["included"], true);
    assert!(included["record"]["last_error"].is_null());
    let after = read(world.path(), &doc, None);
    assert_eq!(
        included["record"]["applied_source_revision"],
        after["revision"]["revision"]
    );
    let mut expected = contributions(&after);
    expected.sort();
    assert_eq!(
        expected,
        ["<p>Human moved on</p>", "<p>Reviewed contribution</p>"]
    );
}
#[test]
fn committed_inclusion_superseded_by_a_later_revision_still_recovers_to_included() {
    let world = world();
    let doc = document(world.path(), None, "day", "doc:committed-moved");
    let received = call(
        world.path(),
        "receiving_submit",
        &proposal(&doc, None, "producer:committed", "entry:committed"),
        AGENT,
    )
    .unwrap();
    let accepted = review(world.path(), &received, &doc, None);
    let included = call(
        world.path(),
        "receiving_include",
        &inclusion(&accepted, &doc, None),
        HUMAN,
    )
    .unwrap();
    // Interrupt after the source commit but before either acknowledgement.
    let rewind = |area: &str, matches: &dyn Fn(&Value) -> bool, edit: &dyn Fn(&mut Value)| {
        let mut found = 0;
        for entry in fs::read_dir(world.path().join(area)).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().and_then(|s| s.to_str()) != Some("json") {
                continue;
            }
            let mut actual: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
            if matches(&actual) {
                edit(&mut actual);
                fs::write(path, serde_json::to_vec_pretty(&actual).unwrap()).unwrap();
                found += 1;
            }
        }
        assert_eq!(found, 1);
    };
    rewind(
        ".central/source-returns/contributions",
        &|v| v["return_ref"] == included["return_ref"],
        &|v| {
            v["status"] = json!("including");
            v["applied_source_revision"] = Value::Null;
        },
    );
    rewind(
        ".central/source-returns/document-mutations",
        &|v| v["next_revision"] == included["record"]["applied_source_revision"],
        &|v| v["status"] = json!("prepared"),
    );
    let committed = read(world.path(), &doc, None);
    let mut add = mutation(&committed, None, "human:after", "entry.add");
    add["entry_id"] = json!("entry:after");
    add["contribution_id"] = json!("entry:after:contribution");
    add["html"] = json!("<p>Later human entry</p>");
    let advanced = call(world.path(), "document_mutate", &add, HUMAN).unwrap();
    let pending = execute_at(
        world.path(),
        "receiving_read",
        &json!({"return_ref":included["return_ref"]}),
        100,
    )
    .unwrap();
    let recovered = call(
        world.path(),
        "receiving_recover",
        &json!({"return_ref":pending["return_ref"],"expected_return_revision":pending["revision"]}),
        HUMAN,
    )
    .unwrap();
    assert_eq!(recovered["included"], true);
    assert_eq!(
        recovered["record"]["applied_source_revision"],
        included["record"]["applied_source_revision"]
    );
    let after = read(world.path(), &doc, None);
    assert_eq!(after["revision"], advanced["revision"]);
    assert_eq!(
        after["document"]["contributions"].as_array().unwrap().len(),
        2
    );
}
#[test]
fn receiving_rejects_fake_human_review_and_stale_return_revision_without_source_effects() {
    let world = world();
    let doc = document(world.path(), None, "day", "doc:auth");
    let received = call(
        world.path(),
        "receiving_submit",
        &proposal(&doc, None, "producer:auth", "entry:auth"),
        AGENT,
    )
    .unwrap();
    let mut input = json!({"return_ref":received["return_ref"],"expected_return_revision":received["revision"],"expected_source_revision":doc["revision"]["revision"],"disposition":"accepted","actor_kind":"human","author":"H"});
    assert_eq!(
        call(world.path(), "receiving_review", &input, AGENT)
            .unwrap_err()
            .kind(),
        io::ErrorKind::PermissionDenied
    );
    assert_eq!(
        execute_at(world.path(), "receiving_review", &input, 100)
            .unwrap_err()
            .kind(),
        io::ErrorKind::PermissionDenied
    );
    input["expected_return_revision"] = json!("stale");
    assert_eq!(
        call(world.path(), "receiving_review", &input, HUMAN)
            .unwrap_err()
            .kind(),
        io::ErrorKind::AlreadyExists
    );
    assert_eq!(read(world.path(), &doc, None)["revision"], doc["revision"]);
}

fn allocate_now(root: &Path) -> Value {
    let policy = execute_at(root, "policy", &json!({}), 100).unwrap();
    execute_at(root,"allocate",&json!({"task_ref":"task:asks-the-owner","purpose":"work that needs the owner's decision","expected_policy_revision":policy["revision"]}),100).unwrap()
}
fn now_transition(root: &Path, now: &Value, lifecycle: &str) -> io::Result<Value> {
    let policy = execute_at(root, "policy", &json!({}), 100).unwrap();
    let current = execute_at(root, "now_read", &json!({"now_ref":now["now_ref"]}), 100).unwrap();
    call(
        root,
        "now_lifecycle",
        &json!({"now_ref":now["now_ref"],"expected_revision":current["revision"]["revision"],"expected_policy_revision":policy["revision"],"lifecycle":lifecycle}),
        AGENT,
    )
}
fn proposal_request(now: &Value) -> Value {
    json!({"producer_key":"inquiry:verify-shader-receipt","now_ref":now["now_ref"],
        "summary":"The producer's green receipt cannot verify the crash it claims to cover.",
        "evidence_refs":["central:path:/world:T/native-mac-shader-failure.json","central:path:/world:T/metal/receipt.json"],
        "request":{"kind":"proposal","subject":"Commission independent verification of the Mac shader uniform limit",
            "body":"The receipt has no timestamp, no per-check results and no hash of the crashed bundle.",
            "proposed_owner_ref":"factory","proposal_ref":"factory:commission:independent-verification-native-mac-shader-uniform-limit"}})
}
fn open_page(root: &Path) -> Value {
    execute_at(root, "receiving_list", &json!({"open":true}), 100).unwrap()
}

#[test]
fn agent_request_reaches_the_owner_and_the_decision_returns_to_its_now() {
    let world = world();
    let root = world.path();
    let now = allocate_now(root);

    // An Agent request is answerable from a NOW; without one it is refused.
    let mut orphan = proposal_request(&now);
    orphan.as_object_mut().unwrap().remove("now_ref");
    let error = call(root, "receiving_submit", &orphan, AGENT).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::InvalidInput);

    let received = call(root, "receiving_submit", &proposal_request(&now), AGENT).unwrap();
    let record = &received["record"];
    assert_eq!(record["kind"], "request");
    assert_eq!(record["status"], "pending");
    assert!(
        record.get("source_ref").is_none(),
        "a request targets no document"
    );
    assert_eq!(record["author"]["principal_ref"], "agent:test");
    assert_eq!(record["evidence_refs"].as_array().unwrap().len(), 2);
    // Exact replay is the same Return, not a second one.
    let replay = call(root, "receiving_submit", &proposal_request(&now), AGENT).unwrap();
    assert_eq!(replay["return_ref"], received["return_ref"]);

    let page = open_page(root);
    assert_eq!(page["open_total"], 1);
    assert_eq!(
        page["returns"][0]["request"]["subject"],
        "Commission independent verification of the Mac shader uniform limit"
    );
    assert_eq!(
        page["returns"][0]["request"]["proposed_owner_ref"],
        "factory"
    );
    assert_eq!(page["returns"][0]["settled"], false);

    // Only the owner decides.
    let decide = json!({"return_ref":received["return_ref"],"expected_return_revision":received["revision"],"disposition":"accepted","note":"Yes — verification only, no patch."});
    let error = call(root, "receiving_review", &decide, AGENT).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
    let accepted = call(root, "receiving_review", &decide, HUMAN).unwrap();
    assert_eq!(accepted["record"]["status"], "accepted");
    assert_eq!(
        accepted["record"]["review"]["note"],
        "Yes — verification only, no patch."
    );

    // Accepted with a named owner still waits for that owner: the Inbox keeps
    // it and the asking NOW cannot archive.
    assert_eq!(open_page(root)["open_total"], 1);
    now_transition(root, &now, "closed").unwrap();
    let error = now_transition(root, &now, "archived").unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);

    // The accepting human records the proposed owner's realisation.
    let mut realise = json!({"return_ref":received["return_ref"],"expected_return_revision":accepted["revision"],
        "realisation_ref":"run:01M3JJG94Q2RW4J32PYEM27QR7","realisation_owner_ref":"actuation"});
    let error = call(root, "receiving_include", &realise, HUMAN).unwrap_err();
    assert_eq!(
        error.kind(),
        io::ErrorKind::PermissionDenied,
        "only the proposed owner realises it"
    );
    realise["realisation_owner_ref"] = json!("factory");
    let error = call(root, "receiving_recover", &realise, HUMAN).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    let realised = call(root, "receiving_include", &realise, HUMAN).unwrap();
    assert_eq!(realised["record"]["status"], "included");
    assert_eq!(
        realised["record"]["realisation"]["ref"],
        "run:01M3JJG94Q2RW4J32PYEM27QR7"
    );
    assert_eq!(
        realised["record"]["realisation"]["recorded_by"],
        "human:test"
    );
    let mut again = realise.clone();
    again["expected_return_revision"] = realised["revision"].clone();
    assert_eq!(
        call(root, "receiving_include", &again, HUMAN).unwrap()["revision"],
        realised["revision"]
    );
    again["realisation_ref"] = json!("run:someone-else");
    assert_eq!(
        call(root, "receiving_include", &again, HUMAN)
            .unwrap_err()
            .kind(),
        io::ErrorKind::AlreadyExists
    );

    // The Agent reads the owner's decision where it works: its NOW.
    let reading = execute_at(root, "now_read", &json!({"now_ref":now["now_ref"]}), 100).unwrap();
    let returned = reading["returns"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["return_ref"] == received["return_ref"])
        .unwrap();
    assert_eq!(returned["kind"], "request");
    assert_eq!(returned["settled"], true);
    assert_eq!(returned["decision"]["disposition"], "accepted");
    assert_eq!(
        returned["decision"]["note"],
        "Yes — verification only, no patch."
    );
    assert_eq!(
        returned["realisation"]["ref"],
        "run:01M3JJG94Q2RW4J32PYEM27QR7"
    );

    assert_eq!(open_page(root)["open_total"], 0);
    assert!(open_page(root)["returns"].as_array().unwrap().is_empty());
    now_transition(root, &now, "archived").unwrap();
}

#[test]
fn questions_are_answered_and_ownerless_proposals_settle_on_acceptance() {
    let world = world();
    let root = world.path();
    let now = allocate_now(root);
    let question = call(root,"receiving_submit",&json!({"producer_key":"ask:placement","now_ref":now["now_ref"],
        "request":{"kind":"question","subject":"Should the verifier run on Omarchy or wait for the Mac?","options":["Omarchy now","Wait for the Mac"]}}),AGENT).unwrap();
    let base = json!({"return_ref":question["return_ref"],"expected_return_revision":question["revision"]});

    // Seen is not decided, and later is an explicit state.
    let mut seen = base.clone();
    seen["disposition"] = json!("acknowledged");
    let seen = call(root, "receiving_review", &seen, HUMAN).unwrap();
    assert_eq!(seen["record"]["status"], "pending");
    assert_eq!(
        seen["record"]["acknowledgement"]["principal_ref"],
        "human:test"
    );
    let mut answer = json!({"return_ref":question["return_ref"],"expected_return_revision":seen["revision"],"disposition":"accepted"});
    assert_eq!(
        call(root, "receiving_review", &answer, HUMAN)
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidInput
    );
    answer["disposition"] = json!("answered");
    assert_eq!(
        call(root, "receiving_review", &answer, HUMAN)
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidInput
    );
    answer["answer"] = json!("Omarchy now; state the hardware limit.");
    let answered = call(root, "receiving_review", &answer, HUMAN).unwrap();
    assert_eq!(answered["record"]["status"], "answered");
    assert_eq!(
        answered["record"]["review"]["answer"],
        "Omarchy now; state the hardware limit."
    );
    let reading = execute_at(root, "now_read", &json!({"now_ref":now["now_ref"]}), 100).unwrap();
    assert_eq!(
        reading["returns"][0]["decision"]["answer"],
        "Omarchy now; state the hardware limit."
    );
    assert_eq!(reading["returns"][0]["settled"], true);

    let proposal = call(root,"receiving_submit",&json!({"producer_key":"propose:rename","now_ref":now["now_ref"],
        "request":{"kind":"proposal","subject":"Rename the reception module to match the M-tree canon"}}),AGENT).unwrap();
    let accepted = call(root,"receiving_review",&json!({"return_ref":proposal["return_ref"],"expected_return_revision":proposal["revision"],"disposition":"accepted"}),HUMAN).unwrap();
    assert_eq!(accepted["record"]["status"], "accepted");
    assert_eq!(
        open_page(root)["open_total"],
        0,
        "an ownerless accepted proposal is settled"
    );
    let error = call(root,"receiving_include",&json!({"return_ref":proposal["return_ref"],"expected_return_revision":accepted["revision"],"realisation_ref":"run:x","realisation_owner_ref":"factory"}),HUMAN).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    // Leaving it pending again reopens it.
    let reopened = call(root,"receiving_review",&json!({"return_ref":proposal["return_ref"],"expected_return_revision":accepted["revision"],"disposition":"pending"}),HUMAN).unwrap();
    assert_eq!(reopened["record"]["status"], "pending");
    assert!(reopened["record"]["review"].is_null());
    assert_eq!(open_page(root)["open_total"], 1);
}

#[test]
fn request_and_contribution_shapes_stay_distinct_and_attribution_is_not_borrowed() {
    let world = world();
    let root = world.path();
    let now = allocate_now(root);
    let doc = document(root, None, "flow", "doc:shapes");

    let mut mixed = proposal_request(&now);
    mixed["source_ref"] = doc["source"]["ref"].clone();
    assert_eq!(
        call(root, "receiving_submit", &mixed, AGENT)
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidInput
    );
    let mut unknown = proposal_request(&now);
    unknown["request"]["priority"] = json!("urgent");
    assert_eq!(
        call(root, "receiving_submit", &unknown, AGENT)
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidInput
    );
    let mut options = proposal_request(&now);
    options["request"]["options"] = json!(["a"]);
    assert_eq!(
        call(root, "receiving_submit", &options, AGENT)
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidInput
    );

    // An operation the document owner cannot apply is refused at arrival,
    // not retained to fail at inclusion.
    let mut unincludable = proposal(&doc, None, "shape:unsupported", "entry:x");
    unincludable["proposal"]["operation"] = json!("append-to-field");
    assert_eq!(
        call(root, "receiving_submit", &unincludable, AGENT)
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidInput
    );
    let mut human_only = proposal(&doc, None, "shape:title", "entry:y");
    human_only["proposal"] = json!({"operation":"title.set","value":"Agent title"});
    assert_eq!(
        call(root, "receiving_submit", &human_only, AGENT)
            .unwrap_err()
            .kind(),
        io::ErrorKind::PermissionDenied
    );

    // An Agent credential speaks only for itself; a carrier may declare whose
    // work it delivers, and both stay on the record.
    let mut borrowed = proposal_request(&now);
    borrowed["declared_producer"] =
        json!({"ref":"agent:someone-else","actor_kind":"agent","attribution":"verified"});
    assert_eq!(
        call(root, "receiving_submit", &borrowed, AGENT)
            .unwrap_err()
            .kind(),
        io::ErrorKind::PermissionDenied
    );
    let mut carried = proposal_request(&now);
    carried["producer_key"] = json!("carried:epii");
    carried["declared_producer"] =
        json!({"ref":"agent-session/nara-epii","actor_kind":"agent","attribution":"claimed"});
    let carried = call(root, "receiving_submit", &carried, HUMAN).unwrap();
    assert_eq!(carried["record"]["author"]["principal_ref"], "human:test");
    assert_eq!(
        carried["record"]["declared_producer"]["ref"],
        "agent-session/nara-epii"
    );
    assert_eq!(
        carried["record"]["declared_producer"]["attribution"],
        "claimed"
    );
    let page = open_page(root);
    assert_eq!(
        page["returns"][0]["declared_producer"]["ref"],
        "agent-session/nara-epii"
    );
}

/// New lookup proofs use only native task scratch. Existing fixtures keep their
/// own standing; no test allocates a personal World or new development checkout.
struct ReceiptLookupWorld(PathBuf);
impl ReceiptLookupWorld {
    fn path(&self) -> &Path {
        &self.0
    }
}
impl Drop for ReceiptLookupWorld {
    fn drop(&mut self) {
        if let Err(error) = fs::remove_dir_all(&self.0) {
            let message = format!(
                "receiving lookup fixture cleanup {}: {error}",
                self.0.display()
            );
            if std::thread::panicking() {
                eprintln!("{message}");
            } else {
                panic!("{message}");
            }
        }
    }
}
fn receipt_lookup_world() -> ReceiptLookupWorld {
    let scratch = Path::new(env!("CARGO_MANIFEST_DIR")).join("../ProjectCentral/now/tmp");
    fs::create_dir_all(&scratch).unwrap();
    let path = scratch.join(format!(
        "receiving-lookup-{}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&path).unwrap();
    let world = ReceiptLookupWorld(path);
    configure_world(world.path());
    world
}
fn lookup_request(original: &Value, guarded: bool) -> Value {
    let mut input = json!({"project":original.get("project").cloned().unwrap_or(Value::Null),
        "producer_key":original["producer_key"]});
    if guarded {
        input["original_request"] = original.clone();
    }
    input
}
fn receiving_snapshot(root: &Path) -> std::collections::BTreeMap<PathBuf, Vec<u8>> {
    fs::read_dir(root.join(".central/source-returns/contributions"))
        .unwrap()
        .map(|entry| {
            let path = entry.unwrap().path();
            let bytes = fs::read(&path).unwrap();
            (path, bytes)
        })
        .collect()
}

#[test]
fn receipt_lookup_confirms_original_publication_without_another_arrival() {
    let world = receipt_lookup_world();
    let doc = document(world.path(), None, "day", "doc:lookup");
    let input = proposal(&doc, None, "lookup:original", "entry:lookup");
    let submitted = call(world.path(), "receiving_submit", &input, AGENT).unwrap();
    let original_record = submitted["record"].clone();
    let reference = submitted["return_ref"].clone();
    drop(submitted); // API acknowledgement discarded; CLI transport proof is separate.
    let before = receiving_snapshot(world.path());
    let document_before = read(world.path(), &doc, None);
    let recovered = call(
        world.path(),
        "receiving_read",
        &lookup_request(&input, true),
        AGENT,
    )
    .unwrap();
    assert_eq!(recovered["return_ref"], reference);
    assert_eq!(recovered["record"], original_record);
    assert_eq!(recovered["lookup"]["original_request_verified"], true);
    let by_ref = execute_at(
        world.path(),
        "receiving_read",
        &json!({"return_ref":reference}),
        100,
    )
    .unwrap();
    assert!(by_ref.get("lookup").is_none());
    assert_eq!(by_ref["record"], recovered["record"]);
    assert_eq!(by_ref["revision"], recovered["revision"]);
    assert_eq!(receiving_snapshot(world.path()), before);
    assert_eq!(read(world.path(), &doc, None), document_before);
}

#[test]
fn receipt_lookup_original_request_guard_preserves_exact_input_and_conflicts() {
    let world = receipt_lookup_world();
    let doc = document(world.path(), None, "day", "doc:guard");
    let input = proposal(&doc, None, "lookup:guard", "entry:guard");
    call(world.path(), "receiving_submit", &input, AGENT).unwrap();
    let before = receiving_snapshot(world.path());
    let unguarded = call(
        world.path(),
        "receiving_read",
        &lookup_request(&input, false),
        AGENT,
    )
    .unwrap();
    assert_eq!(unguarded["lookup"]["original_request_verified"], false);
    for (field, value) in [
        ("occurred_at_unix_seconds", json!(43)),
        ("expected_authority_revision", json!("old-different")),
    ] {
        let mut changed = lookup_request(&input, true);
        changed["original_request"][field] = value;
        assert_eq!(
            call(world.path(), "receiving_read", &changed, AGENT)
                .unwrap_err()
                .kind(),
            io::ErrorKind::AlreadyExists
        );
    }
    let mut different_key = lookup_request(&input, true);
    different_key["original_request"]["producer_key"] = json!("different");
    assert_eq!(
        call(world.path(), "receiving_read", &different_key, AGENT)
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidInput
    );
    assert_eq!(receiving_snapshot(world.path()), before);
}

#[test]
fn receipt_lookup_owner_rejects_malformed_selectors_and_expectation_types() {
    let world = receipt_lookup_world();
    for input in [
        json!({}),
        json!({"return_ref":"r","producer_key":"k"}),
        json!({"producer_key":7}),
        json!({"producer_key":"k","original_request":[]}),
        json!({"producer_key":"k","original_request":{}}),
        json!({"producer_key":"k","original_request":{"producer_key":false}}),
        json!({"producer_key":"k","expected_authority_revision":false}),
        json!({"producer_key":"k","expected_authority_revision":""}),
        json!({"return_ref":"r","original_request":{}}),
    ] {
        assert_eq!(
            call(world.path(), "receiving_read", &input, AGENT)
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidInput
        );
    }
    let doc = document(world.path(), None, "day", "doc:null");
    let original = proposal(&doc, None, "lookup:null", "entry:null");
    let submitted = call(world.path(), "receiving_submit", &original, AGENT).unwrap();
    let mut input = lookup_request(&original, false);
    input["return_ref"] = Value::Null;
    input["original_request"] = Value::Null;
    input["expected_authority_revision"] = Value::Null;
    assert_eq!(
        call(world.path(), "receiving_read", &input, AGENT).unwrap()["return_ref"],
        submitted["return_ref"]
    );
}

#[test]
fn receipt_lookup_uses_actual_principal_scope_and_current_authority() {
    let world = receipt_lookup_world();
    let root_doc = document(world.path(), None, "day", "doc:principals");
    let project_doc = document(world.path(), Some("one"), "day", "doc:scope");
    let original = proposal(&root_doc, None, "shared-key", "entry:agent");
    let ours = call(world.path(), "receiving_submit", &original, AGENT).unwrap();
    let other = call(world.path(), "receiving_submit", &original, OTHER).unwrap();
    let scoped = proposal(&project_doc, Some("one"), "shared-key", "entry:project");
    let in_project = call(world.path(), "receiving_submit", &scoped, AGENT).unwrap();
    assert_ne!(ours["return_ref"], other["return_ref"]);
    assert_ne!(ours["return_ref"], in_project["return_ref"]);
    let mut lookup = lookup_request(&original, true);
    lookup["actor"] = json!("agent:other");
    lookup["declared_producer"] =
        json!({"ref":"agent:other","actor_kind":"agent","attribution":"verified"});
    assert_eq!(
        call(world.path(), "receiving_read", &lookup, AGENT).unwrap()["return_ref"],
        ours["return_ref"]
    );
    assert_eq!(
        call(world.path(), "receiving_read", &lookup, OTHER).unwrap()["return_ref"],
        other["return_ref"]
    );
    assert_eq!(
        call(
            world.path(),
            "receiving_read",
            &lookup_request(&scoped, true),
            AGENT
        )
        .unwrap()["return_ref"],
        in_project["return_ref"]
    );
    assert_eq!(
        execute_at(world.path(), "receiving_read", &lookup, 100)
            .unwrap_err()
            .kind(),
        io::ErrorKind::PermissionDenied
    );
    assert_eq!(
        execute_with_token_at(world.path(), "receiving_read", &lookup, Some(AGENT), 10000)
            .unwrap_err()
            .kind(),
        io::ErrorKind::PermissionDenied
    );
    lookup["expected_authority_revision"] = json!("stale-current-authority");
    assert_eq!(
        call(world.path(), "receiving_read", &lookup, AGENT)
            .unwrap_err()
            .kind(),
        io::ErrorKind::AlreadyExists
    );
}

#[test]
fn receipt_lookup_observes_current_target_withdrawal_and_reopening() {
    let world = receipt_lookup_world();
    let doc = document(world.path(), None, "day", "doc:withdrawal");
    let original = proposal(&doc, None, "lookup:withdrawal", "entry:withdrawal");
    let submitted = call(world.path(), "receiving_submit", &original, AGENT).unwrap();
    let before = receiving_snapshot(world.path());
    let source_path = world.path().join(doc["source"]["path"].as_str().unwrap());
    let source_before = fs::read(&source_path).unwrap();
    let marker = source_path.parent().unwrap().join(".no-agent-retrieval");
    assert!(!marker.exists());
    fs::write(&marker, b"controlled fixture withdrawal").unwrap();
    assert_eq!(
        call(
            world.path(),
            "receiving_read",
            &lookup_request(&original, true),
            AGENT
        )
        .unwrap_err()
        .kind(),
        io::ErrorKind::PermissionDenied
    );
    fs::remove_file(marker).unwrap();
    let reopened = call(
        world.path(),
        "receiving_read",
        &lookup_request(&original, true),
        AGENT,
    )
    .unwrap();
    assert_eq!(reopened["return_ref"], submitted["return_ref"]);
    assert_eq!(receiving_snapshot(world.path()), before);
    assert_eq!(fs::read(source_path).unwrap(), source_before);
}

#[test]
fn receipt_lookup_actual_record_eacces_is_error_not_absence_or_resend() {
    use std::os::unix::fs::PermissionsExt;
    assert_ne!(
        unsafe { libc::geteuid() },
        0,
        "requires actual nonroot EACCES; unavailable prerequisite must not pass"
    );
    let world = receipt_lookup_world();
    let doc = document(world.path(), None, "day", "doc:record-permission");
    let original = proposal(&doc, None, "lookup:permission", "entry:permission");
    call(world.path(), "receiving_submit", &original, AGENT).unwrap();
    let before = receiving_snapshot(world.path());
    let paths: Vec<_> = before
        .keys()
        .filter(|path| path.file_name().unwrap() != "cursor.json")
        .collect();
    assert_eq!(paths.len(), 1);
    let path = paths[0];
    let mode = fs::metadata(path).unwrap().permissions();
    fs::set_permissions(path, fs::Permissions::from_mode(0o000)).unwrap();
    let error = call(
        world.path(),
        "receiving_read",
        &lookup_request(&original, true),
        AGENT,
    )
    .unwrap_err();
    fs::set_permissions(path, mode).unwrap();
    assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
    assert_eq!(error.raw_os_error(), Some(libc::EACCES));
    assert_eq!(receiving_snapshot(world.path()), before);
    assert!(call(
        world.path(),
        "receiving_read",
        &lookup_request(&original, true),
        AGENT
    )
    .is_ok());
}

#[test]
fn receipt_lookup_reports_actual_included_state_without_recovering_or_reviewing() {
    let world = receipt_lookup_world();
    let doc = document(world.path(), None, "day", "doc:included-lookup");
    let original = proposal(&doc, None, "lookup:included", "entry:included");
    let submitted = call(world.path(), "receiving_submit", &original, AGENT).unwrap();
    let accepted = review(world.path(), &submitted, &doc, None);
    let included = call(
        world.path(),
        "receiving_include",
        &inclusion(&accepted, &doc, None),
        HUMAN,
    )
    .unwrap();
    let before = receiving_snapshot(world.path());
    let document_before = read(world.path(), &doc, None);
    let lookup = call(
        world.path(),
        "receiving_read",
        &lookup_request(&original, true),
        AGENT,
    )
    .unwrap();
    assert_eq!(lookup["record"], included["record"]);
    assert_eq!(lookup["included"], true);
    assert_eq!(lookup["record"]["occurred_at_unix_seconds"], 42);
    assert_eq!(
        lookup["record"]["received_at_unix_seconds"],
        submitted["record"]["received_at_unix_seconds"]
    );
    assert_eq!(receiving_snapshot(world.path()), before);
    assert_eq!(read(world.path(), &doc, None), document_before);
}

#[test]
fn receipt_lookup_preserves_genuine_uncertain_inclusion_without_recovering() {
    use std::os::unix::fs::PermissionsExt;
    assert_ne!(
        unsafe { libc::geteuid() },
        0,
        "requires actual nonroot readonly target failure; unavailable prerequisite must not pass"
    );
    let world = receipt_lookup_world();
    let doc = document(world.path(), None, "day", "doc:uncertain-lookup");
    let original = proposal(&doc, None, "lookup:uncertain", "entry:uncertain");
    let submitted = call(world.path(), "receiving_submit", &original, AGENT).unwrap();
    let accepted = review(world.path(), &submitted, &doc, None);
    let target = world.path().join(doc["source"]["path"].as_str().unwrap());
    let source_before = fs::read(&target).unwrap();
    let original_permissions = fs::metadata(&target).unwrap().permissions();
    let original_mode = original_permissions.mode();
    // Restore this fixture-owned source on both normal return and assertion
    // unwind. A cleanup failure cannot become a green permission proof.
    struct RestorePermissions {
        path: PathBuf,
        permissions: fs::Permissions,
    }
    impl Drop for RestorePermissions {
        fn drop(&mut self) {
            if let Err(error) = fs::set_permissions(&self.path, self.permissions.clone()) {
                let message = format!(
                    "restore receiving lookup fixture source {}: {error}",
                    self.path.display()
                );
                if std::thread::panicking() {
                    eprintln!("{message}");
                } else {
                    panic!("{message}");
                }
            }
        }
    }
    let restore = RestorePermissions {
        path: target.clone(),
        permissions: original_permissions,
    };
    fs::set_permissions(&target, fs::Permissions::from_mode(0o444)).unwrap();
    let failed = call(
        world.path(),
        "receiving_include",
        &inclusion(&accepted, &doc, None),
        HUMAN,
    )
    .unwrap_err();
    assert_eq!(failed.kind(), io::ErrorKind::PermissionDenied);
    // Only the actual native owner has written the phase and retained intent.
    // There is no hand-written status, receiving.recover or subsequent review.
    let uncertain = execute_at(
        world.path(),
        "receiving_read",
        &json!({"return_ref":submitted["return_ref"]}),
        100,
    )
    .unwrap();
    assert_eq!(uncertain["record"]["status"], "uncertain");
    assert!(uncertain["record"]["inclusion_request"].is_object());
    assert_eq!(uncertain["included"], false);
    assert!(uncertain["record"]["applied_source_revision"].is_null());
    let area_before = receiving_snapshot(world.path());
    let lookup = call(
        world.path(),
        "receiving_read",
        &lookup_request(&original, true),
        AGENT,
    )
    .unwrap();
    assert_eq!(lookup["lookup"]["original_request_verified"], true);
    assert_eq!(lookup["return_ref"], uncertain["return_ref"]);
    assert_eq!(lookup["record"], uncertain["record"]);
    assert_eq!(lookup["revision"], uncertain["revision"]);
    assert_eq!(lookup["record"]["status"], "uncertain");
    assert_eq!(lookup["included"], false);
    assert_eq!(lookup["record"]["occurred_at_unix_seconds"], 42);
    assert_eq!(
        lookup["record"]["received_at_unix_seconds"],
        submitted["record"]["received_at_unix_seconds"]
    );
    assert_eq!(receiving_snapshot(world.path()), area_before);
    assert_eq!(fs::read(&target).unwrap(), source_before);
    assert_eq!(
        fs::metadata(&target).unwrap().permissions().mode() & 0o777,
        0o444
    );
    drop(restore);
    assert_eq!(
        fs::metadata(&target).unwrap().permissions().mode(),
        original_mode
    );
    assert_eq!(fs::read(target).unwrap(), source_before);
}

// These contextual-cause regressions exercise the actual native document and
// Receiving owners. The readonly-file preflight is distinct from real OS
// refusal to create the document publisher's staging file in its parent.
struct RestoreReceivingFixtureMode {
    path: PathBuf,
    identity: (u64, u64),
    permissions: fs::Permissions,
}
impl RestoreReceivingFixtureMode {
    fn retain(path: &Path) -> Self {
        use std::os::unix::fs::MetadataExt;
        let metadata = fs::symlink_metadata(path).unwrap();
        assert!(!metadata.file_type().is_symlink());
        Self {
            path: path.into(),
            identity: (metadata.dev(), metadata.ino()),
            permissions: metadata.permissions(),
        }
    }
}
impl Drop for RestoreReceivingFixtureMode {
    fn drop(&mut self) {
        use std::os::unix::fs::MetadataExt;
        let result = (|| {
            let metadata = fs::symlink_metadata(&self.path)?;
            if metadata.file_type().is_symlink()
                || (metadata.dev(), metadata.ino()) != self.identity
            {
                return Err(io::Error::other(
                    "owned permission fixture changed physical affiliation",
                ));
            }
            fs::set_permissions(&self.path, self.permissions.clone())
        })();
        if let Err(error) = result {
            let message = format!(
                "restore receiving cause fixture {}: {error}",
                self.path.display()
            );
            if std::thread::panicking() {
                eprintln!("{message}");
            } else {
                panic!("{message}");
            }
        }
    }
}
fn original_inclusion_io(error: &io::Error) -> &io::Error {
    std::error::Error::source(error)
        .and_then(|cause| cause.downcast_ref::<io::Error>())
        .expect("Receiving contextual error retains the actual original document IO")
}
fn assert_actual_uncertain_lookup(
    root: &Path,
    document: &Value,
    original: &Value,
    submitted: &Value,
    failed: &io::Error,
) {
    let cause = original_inclusion_io(failed);
    assert_eq!(failed.kind(), cause.kind());
    assert_eq!(
        failed.to_string(),
        format!(
            "inclusion not confirmed; inspect/recover {}: {cause}",
            submitted["return_ref"].as_str().unwrap()
        )
    );
    // The context is a custom io::Error; the OS code belongs to the preserved
    // original cause, never an errno reconstructed from a kind or message.
    assert_eq!(failed.raw_os_error(), None);
    let uncertain = execute_at(
        root,
        "receiving_read",
        &json!({"return_ref":submitted["return_ref"]}),
        100,
    )
    .unwrap();
    assert_eq!(uncertain["record"]["status"], "uncertain");
    assert!(uncertain["record"]["inclusion_request"].is_object());
    assert_eq!(uncertain["included"], false);
    assert!(uncertain["record"]["applied_source_revision"].is_null());
    assert_eq!(uncertain["record"]["last_error"], cause.to_string());
    for field in [
        "schema",
        "return_ref",
        "scope_ref",
        "sequence",
        "request_digest",
        "kind",
        "source_ref",
        "document_id",
        "proposal",
        "author",
        "authority_ref",
        "authority_revision",
        "occurred_at_unix_seconds",
        "received_at_unix_seconds",
    ] {
        assert_eq!(
            uncertain["record"][field], submitted["record"][field],
            "{field}"
        );
    }
    assert_eq!(uncertain["record"]["source_ref"], document["source"]["ref"]);
    assert_eq!(uncertain["record"]["occurred_at_unix_seconds"], 42);
    let area_before = receiving_snapshot(root);
    let lookup = call(
        root,
        "receiving_read",
        &lookup_request(original, true),
        AGENT,
    )
    .unwrap();
    assert_eq!(lookup["lookup"]["selector"], "authenticated_producer_key");
    assert_eq!(lookup["lookup"]["original_request_verified"], true);
    assert_eq!(lookup["return_ref"], uncertain["return_ref"]);
    assert_eq!(lookup["record"], uncertain["record"]);
    assert_eq!(lookup["revision"], uncertain["revision"]);
    assert_eq!(lookup["included"], false);
    assert_eq!(receiving_snapshot(root), area_before);
    assert_eq!(read(root, document, None)["document"], document["document"]);
}

#[test]
fn receiving_include_retains_actual_parent_directory_os_cause_and_uncertain_receipt() {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    assert_ne!(
        unsafe { libc::geteuid() },
        0,
        "requires actual nonroot directory EACCES; unavailable prerequisite must not pass"
    );
    let world = receipt_lookup_world();
    let doc = document(world.path(), None, "flow", "doc:directory-cause");
    let original = proposal(&doc, None, "cause:directory", "entry:directory-cause");
    let submitted = call(world.path(), "receiving_submit", &original, AGENT).unwrap();
    let accepted = review(world.path(), &submitted, &doc, None);
    let target = world.path().join(doc["source"]["path"].as_str().unwrap());
    let parent = target.parent().unwrap();
    let source_before = fs::read(&target).unwrap();
    let source_metadata = fs::symlink_metadata(&target).unwrap();
    assert!(
        !source_metadata.permissions().readonly(),
        "actual source stays writable"
    );
    let source_identity = (source_metadata.dev(), source_metadata.ino());
    let restore = RestoreReceivingFixtureMode::retain(parent);
    let parent_identity = restore.identity;
    let parent_mode = restore.permissions.mode();
    let receiving_area = world.path().join(".central/source-returns/contributions");
    let document_area = world
        .path()
        .join(".central/source-returns/document-mutations");
    assert!(parent.starts_with(world.path()));
    assert_ne!(parent, world.path());
    assert!(!receiving_area.starts_with(parent));
    assert!(!document_area.starts_with(parent));
    fs::set_permissions(parent, fs::Permissions::from_mode(0o555)).unwrap();
    let oracle_path = parent.join(".receiving-cause-unused-os-oracle");
    assert!(!oracle_path.exists());
    let oracle = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&oracle_path)
        .expect_err("the real owned parent must refuse new file creation");
    assert_eq!(oracle.kind(), io::ErrorKind::PermissionDenied);
    assert_eq!(oracle.raw_os_error(), Some(libc::EACCES));
    assert!(!oracle_path.exists());
    let failed = call(
        world.path(),
        "receiving_include",
        &inclusion(&accepted, &doc, None),
        HUMAN,
    )
    .unwrap_err();
    let cause = original_inclusion_io(&failed);
    assert_eq!(cause.kind(), oracle.kind());
    assert_eq!(cause.raw_os_error(), oracle.raw_os_error());
    assert_actual_uncertain_lookup(world.path(), &doc, &original, &submitted, &failed);
    // No test writes an including/uncertain status. The native document owner
    // must have retained its prepared intent before the actual OS refusal.
    let intents: Vec<Value> = fs::read_dir(&document_area)
        .unwrap()
        .map(|entry| serde_json::from_slice(&fs::read(entry.unwrap().path()).unwrap()).unwrap())
        .collect();
    assert_eq!(intents.len(), 1);
    assert_eq!(intents[0]["status"], "prepared");
    assert_eq!(intents[0]["source_ref"], doc["source"]["ref"]);
    assert_eq!(intents[0]["previous_revision"], doc["revision"]["revision"]);
    assert_eq!(fs::read(&target).unwrap(), source_before);
    let current_source = fs::symlink_metadata(&target).unwrap();
    assert_eq!(
        (current_source.dev(), current_source.ino()),
        source_identity
    );
    assert_eq!(
        current_source.permissions().mode(),
        source_metadata.permissions().mode()
    );
    assert_eq!(
        fs::metadata(parent).unwrap().permissions().mode() & 0o777,
        0o555
    );
    drop(restore);
    let current_parent = fs::symlink_metadata(parent).unwrap();
    assert_eq!(
        (current_parent.dev(), current_parent.ino()),
        parent_identity
    );
    assert_eq!(current_parent.permissions().mode(), parent_mode);
    assert_eq!(fs::read(&target).unwrap(), source_before);
}

#[test]
fn receiving_include_retains_actual_custom_preflight_cause_without_fabricating_errno() {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    assert_ne!(
        unsafe { libc::geteuid() },
        0,
        "requires actual nonroot readonly fixture; unavailable prerequisite must not pass"
    );
    let world = receipt_lookup_world();
    let doc = document(world.path(), None, "flow", "doc:preflight-cause");
    let original = proposal(&doc, None, "cause:preflight", "entry:preflight-cause");
    let submitted = call(world.path(), "receiving_submit", &original, AGENT).unwrap();
    let accepted = review(world.path(), &submitted, &doc, None);
    let target = world.path().join(doc["source"]["path"].as_str().unwrap());
    let source_before = fs::read(&target).unwrap();
    let restore = RestoreReceivingFixtureMode::retain(&target);
    let identity = restore.identity;
    let mode = restore.permissions.mode();
    fs::set_permissions(&target, fs::Permissions::from_mode(0o444)).unwrap();
    let failed = call(
        world.path(),
        "receiving_include",
        &inclusion(&accepted, &doc, None),
        HUMAN,
    )
    .unwrap_err();
    let cause = original_inclusion_io(&failed);
    assert_eq!(cause.kind(), io::ErrorKind::PermissionDenied);
    assert_eq!(
        cause.raw_os_error(),
        None,
        "custom native writable-link refusal is not an OS EACCES receipt"
    );
    assert_actual_uncertain_lookup(world.path(), &doc, &original, &submitted, &failed);
    assert_eq!(fs::read(&target).unwrap(), source_before);
    let current = fs::symlink_metadata(&target).unwrap();
    assert_eq!((current.dev(), current.ino()), identity);
    assert_eq!(current.permissions().mode() & 0o777, 0o444);
    drop(restore);
    assert_eq!(fs::metadata(&target).unwrap().permissions().mode(), mode);
    assert_eq!(fs::read(target).unwrap(), source_before);
}

#[test]
fn receiving_inclusion_error_debug_retains_native_cause_without_proposed_body_or_credential() {
    use std::os::unix::fs::PermissionsExt;
    assert_ne!(
        unsafe { libc::geteuid() },
        0,
        "requires actual readonly native source failure; unavailable execution must not pass"
    );
    let world = receipt_lookup_world();
    let doc = document(world.path(), None, "flow", "doc:private-error-debug");
    let mut original = proposal(
        &doc,
        None,
        "debug:private-owner-error",
        "entry:private-owner-error",
    );
    let private_body = "<p>actual body selected for inclusion error privacy regression</p>";
    original["proposal"]["html"] = json!(private_body);
    let submitted = call(world.path(), "receiving_submit", &original, AGENT).unwrap();
    let accepted = review(world.path(), &submitted, &doc, None);
    let target = world.path().join(doc["source"]["path"].as_str().unwrap());
    let before = fs::read(&target).unwrap();
    let restore = RestoreReceivingFixtureMode::retain(&target);
    fs::set_permissions(&target, fs::Permissions::from_mode(0o444)).unwrap();
    let error = call(
        world.path(),
        "receiving_include",
        &inclusion(&accepted, &doc, None),
        HUMAN,
    )
    .unwrap_err();
    let cause = original_inclusion_io(&error);
    assert_eq!(cause.kind(), io::ErrorKind::PermissionDenied);
    assert_eq!(
        cause.raw_os_error(),
        None,
        "actual custom preflight must not acquire a fabricated OS code"
    );
    let debug = format!("{error:?}");
    for private in [private_body, HUMAN, AGENT] {
        assert!(!debug.contains(private));
    }
    assert_actual_uncertain_lookup(world.path(), &doc, &original, &submitted, &error);
    assert_eq!(fs::read(&target).unwrap(), before);
    drop(restore);
}
