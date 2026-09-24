use super::*;
use std::fs;
use std::sync::{Arc, Barrier};
use std::thread;

pub(super) fn world() -> crate::TempDir {
    let temp = crate::tempdir().unwrap();
    for path in [
        "Control/user",
        "Control/relations",
        "Control/agents",
        "Work/one/ProjectCentral/user",
        "Work/two/ProjectCentral/user",
        "Work/external/.git",
    ] {
        fs::create_dir_all(temp.path().join(path)).unwrap();
    }
    for name in ["one", "two"] {
        fs::write(
            temp.path()
                .join(format!("Work/{name}/ProjectCentral/project.json")),
            serde_json::to_vec(&crate::projectcentral::ProjectCentralManifest::new(
                format!("test/{name}"),
            ))
            .unwrap(),
        )
        .unwrap();
    }
    let scope = Scope::resolve(temp.path(), None).unwrap();
    let policy_path = "Control/user/placement-policy.json";
    let policy = json!({
        "schema":placement::POLICY_SCHEMA,"scope_ref":"control:root",
        "authority_refs":[],"writable":[
            {"path":"Work/one","class":"repository"},
            {"path":"Work/two","class":"repository"},
            {"path":"Work/external","class":"repository"}
        ],"protected":[],"enforcement":"harness-interception",
        "required_coverage":["filesystem"],"lease_seconds":300,"expires_at_unix_seconds":9999
    });
    fs::write(
        temp.path().join(policy_path),
        source::encoded(&policy).unwrap(),
    )
    .unwrap();
    fs::write(temp.path().join(&scope.relations_path), source::encoded(&json!({
        "schema":scope.relations_schema,"project_id":scope.relations_id,"relations":[{
            "ref":scope.source_ref(policy_path),"path":policy_path,"roles":[placement::POLICY_ROLE],
            "provenance":"human-adopted","standing":"architecture-contract","treatment":"projectcentral-user",
            "recognition":"controlled-test-fixture-not-personal-adoption","recorded_at_unix_seconds":1
        }]
    })).unwrap()).unwrap();
    temp
}
pub(super) fn policy(root: &Path, project: Option<&str>) -> Value {
    execute_at(root, "policy", &json!({"project":project}), 100).unwrap()
}
fn request(root: &Path, project: Option<&str>, task: &str) -> Value {
    json!({"project":project,"task_ref":task,"purpose":"bounded implementation","participant_refs":["agent:test"],"source_refs":[],"expected_policy_revision":policy(root,project)["revision"]})
}
fn validation(allocation: &Value, destination: &str, project: Option<&str>) -> Value {
    json!({"project":project,"now_ref":allocation["now_ref"],"expected_now_revision":allocation["revision"]["revision"],"expected_policy_revision":allocation["policy"]["revision"],"destination":destination})
}

#[test]
fn concurrent_same_task_allocation_publishes_one_identical_source() {
    let temp = world();
    let input = request(temp.path(), None, "task:concurrent");
    let barrier = Arc::new(Barrier::new(8));
    let handles: Vec<_> = (0..8)
        .map(|_| {
            let root = temp.path().to_path_buf();
            let input = input.clone();
            let barrier = barrier.clone();
            thread::spawn(move || {
                barrier.wait();
                execute_at(&root, "allocate", &input, 100).unwrap()
            })
        })
        .collect();
    let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    assert_eq!(results.iter().filter(|r| r["created"] == true).count(), 1);
    for result in &results {
        assert_eq!(result["now_ref"], results[0]["now_ref"]);
        assert_eq!(result["revision"], results[0]["revision"]);
        assert!(Path::new(result["writable_destination"].as_str().unwrap()).is_dir());
    }
    let scope = Scope::resolve(temp.path(), None).unwrap();
    assert_eq!(
        scope
            .bindings()
            .unwrap()
            .iter()
            .filter(|b| b.roles.iter().any(|r| r == "now-clearing"))
            .count(),
        1
    );
    let horizon = crate::source_horizon::reconcile_control_sources(temp.path()).unwrap();
    assert!(horizon
        .horizon
        .sources
        .iter()
        .any(|s| s.binding.source_ref == results[0]["source"]["ref"].as_str().unwrap()));
}
#[test]
fn distinct_tasks_and_root_two_projects_do_not_share_scratch_or_identity() {
    let temp = world();
    let mut refs = std::collections::BTreeSet::new();
    for project in [None, Some("one"), Some("two")] {
        for task in ["task:a", "task:b"] {
            let result = execute_at(
                temp.path(),
                "allocate",
                &request(temp.path(), project, task),
                100,
            )
            .unwrap();
            assert!(refs.insert(result["now_ref"].as_str().unwrap().to_owned()));
            let path = result["writable_destination"].as_str().unwrap();
            assert!(path.contains(if project.is_none() {
                "Control/agents/now/clearings/"
            } else {
                "ProjectCentral/agents/now/clearings/"
            }));
            assert!(!path.contains("ProjectCentral/now/user"));
        }
    }
}
#[test]
fn normal_repository_and_external_repository_writes_are_not_now_only() {
    let temp = world();
    let allocation = execute_at(
        temp.path(),
        "allocate",
        &request(temp.path(), None, "task:write"),
        100,
    )
    .unwrap();
    for path in [
        "Work/one/README.md",
        "Work/one/src/main.rs",
        "Work/one/target/output",
        "Work/external/README.md",
    ] {
        let result = execute_at(
            temp.path(),
            "validate",
            &validation(&allocation, path, None),
            100,
        )
        .unwrap();
        assert_eq!(result["allowed"], true, "{path}: {result}");
    }
    assert!(!temp.path().join("Work/external/ProjectCentral").exists());
    assert_eq!(allocation["policy"]["outside_writes_prevented"], false);
}
#[test]
fn loose_work_root_scratch_and_human_sources_are_rejected_with_usable_now() {
    let temp = world();
    let allocation = execute_at(
        temp.path(),
        "allocate",
        &request(temp.path(), None, "task:guard"),
        100,
    )
    .unwrap();
    for path in [
        "Work/README-draft.md",
        "Work/empty.diff",
        "Control/user/placement-policy.json",
        "Work/one/ProjectCentral/user/day.md",
        "Work/one/.git/config",
    ] {
        let result = execute_at(
            temp.path(),
            "validate",
            &validation(&allocation, path, None),
            100,
        )
        .unwrap();
        assert_eq!(result["allowed"], false, "{path}");
        assert_eq!(
            result["valid_now_destination"],
            allocation["writable_destination"]
        );
    }
    let target = format!(
        "{}/plan.md",
        allocation["writable_destination"].as_str().unwrap()
    );
    assert_eq!(
        execute_at(
            temp.path(),
            "validate",
            &validation(&allocation, &target, None),
            100
        )
        .unwrap()["allowed"],
        true
    );
}
#[test]
fn stale_policy_and_now_sources_refuse_before_any_widening() {
    let temp = world();
    let input = request(temp.path(), None, "task:stale");
    let allocation = execute_at(temp.path(), "allocate", &input, 100).unwrap();
    let now_path = temp
        .path()
        .join(allocation["source"]["path"].as_str().unwrap());
    let original = fs::read(&now_path).unwrap();
    let mut changed = String::from_utf8(original.clone()).unwrap();
    changed.push('\n');
    fs::write(&now_path, changed).unwrap();
    let error = execute_at(
        temp.path(),
        "validate",
        &validation(&allocation, "Work/one/README.md", None),
        100,
    )
    .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
    fs::write(&now_path, &original).unwrap();
    let path = temp.path().join("Control/user/placement-policy.json");
    let mut policy = fs::read_to_string(&path).unwrap();
    policy.push('\n');
    fs::write(path, policy).unwrap();
    assert_eq!(
        execute_at(temp.path(), "allocate", &input, 100)
            .unwrap_err()
            .kind(),
        io::ErrorKind::AlreadyExists
    );
    assert_eq!(fs::read(&now_path).unwrap(), original);
}
#[test]
fn draft_unreadable_and_expired_policy_never_fall_back_to_permissive_defaults() {
    let temp = world();
    let path = temp.path().join("Control/relations/source-relations.json");
    let original = fs::read_to_string(&path).unwrap();
    let mut relation: Value = serde_json::from_str(&original).unwrap();
    relation["relations"][0]["provenance"] = json!("generated-suggestion");
    fs::write(&path, source::encoded(&relation).unwrap()).unwrap();
    assert_eq!(
        execute_at(temp.path(), "policy", &json!({}), 100)
            .unwrap_err()
            .kind(),
        io::ErrorKind::PermissionDenied
    );
    fs::write(&path, original).unwrap();
    assert_eq!(
        execute_at(temp.path(), "policy", &json!({}), 10000)
            .unwrap_err()
            .kind(),
        io::ErrorKind::PermissionDenied
    );
    fs::write(
        temp.path().join("Control/user/placement-policy.json"),
        "{broken",
    )
    .unwrap();
    assert!(execute_at(temp.path(), "policy", &json!({}), 100).is_err());
    assert!(!temp.path().join("Control/agents/now/clearings").exists());
}
#[test]
fn project_policy_narrows_root_and_detects_a_stale_parent() {
    let temp = world();
    let scope = Scope::resolve(temp.path(), Some("one")).unwrap();
    let root = policy(temp.path(), None);
    let local = json!({"schema":placement::POLICY_SCHEMA,"scope_ref":scope.world_ref,"parent_policy":{"source_ref":root["sources"][0]["source"]["ref"],"revision":root["sources"][0]["revision"]},"authority_refs":[],"writable":[{"path":"src","class":"repository"}],"protected":[],"enforcement":"material-filesystem","required_coverage":["filesystem"],"lease_seconds":60});
    let path = "ProjectCentral/user/placement.json";
    fs::write(scope.root.join(path), source::encoded(&local).unwrap()).unwrap();
    fs::create_dir_all(scope.root.join("ProjectCentral/relations")).unwrap();
    fs::write(scope.root.join(&scope.relations_path),source::encoded(&json!({"schema":scope.relations_schema,"project_id":scope.relations_id,"relations":[{"ref":scope.source_ref(path),"path":path,"roles":[placement::POLICY_ROLE],"provenance":"human-adopted","standing":"architecture-contract","treatment":"projectcentral-user","recognition":"test-local-adoption","recorded_at_unix_seconds":1}]})).unwrap()).unwrap();
    let reading = policy(temp.path(), Some("one"));
    assert_eq!(reading["sources"].as_array().unwrap().len(), 2);
    assert_eq!(reading["enforcement"], "material-filesystem");
    assert_eq!(reading["expires_at_unix_seconds"], 160);
    let root_path = temp.path().join("Control/user/placement-policy.json");
    let mut changed = fs::read_to_string(&root_path).unwrap();
    changed.push('\n');
    fs::write(root_path, changed).unwrap();
    assert_eq!(
        execute_at(temp.path(), "policy", &json!({"project":"one"}), 100)
            .unwrap_err()
            .kind(),
        io::ErrorKind::AlreadyExists
    );
}
#[test]
fn symlink_and_changed_destination_anchor_are_refused() {
    let temp = world();
    let outside = crate::tempdir().unwrap();
    let allocation = execute_at(
        temp.path(),
        "allocate",
        &request(temp.path(), None, "task:paths"),
        100,
    )
    .unwrap();
    std::os::unix::fs::symlink(outside.path(), temp.path().join("Work/one/escape")).unwrap();
    assert!(execute_at(
        temp.path(),
        "validate",
        &validation(&allocation, "Work/one/escape/human.md", None),
        100
    )
    .is_err());
    let mut input = validation(&allocation, "Work/one/new/file.md", None);
    let preview = execute_at(temp.path(), "validate", &input, 100).unwrap();
    input["expected_destination_anchor"] = preview["destination_anchor"].clone();
    fs::create_dir_all(temp.path().join("Work/one/new")).unwrap();
    assert_eq!(
        execute_at(temp.path(), "validate", &input, 100)
            .unwrap_err()
            .kind(),
        io::ErrorKind::AlreadyExists
    );
    assert!(!outside.path().join("human.md").exists());
}
#[test]
fn interrupted_allocation_rebinds_original_source_without_changing_bytes() {
    let temp = world();
    let input = request(temp.path(), None, "task:recovery");
    let first = execute_at(temp.path(), "allocate", &input, 100).unwrap();
    let path = temp.path().join(first["source"]["path"].as_str().unwrap());
    let bytes = fs::read(&path).unwrap();
    let rel_path = temp.path().join("Control/relations/source-relations.json");
    let mut relations: Value = serde_json::from_slice(&fs::read(&rel_path).unwrap()).unwrap();
    relations["relations"]
        .as_array_mut()
        .unwrap()
        .retain(|r| r["ref"] != first["source"]["ref"]);
    fs::write(rel_path, source::encoded(&relations).unwrap()).unwrap();
    let resumed = execute_at(temp.path(), "allocate", &input, 101).unwrap();
    assert_eq!(resumed["now_ref"], first["now_ref"]);
    assert_eq!(resumed["revision"], first["revision"]);
    assert_eq!(resumed["record"]["created_at_unix_seconds"], 100);
    assert_eq!(fs::read(path).unwrap(), bytes);
}
#[test]
fn same_task_different_purpose_is_a_conflict_not_a_shared_scratch_overwrite() {
    let temp = world();
    let mut input = request(temp.path(), None, "task:identity");
    execute_at(temp.path(), "allocate", &input, 100).unwrap();
    input["purpose"] = json!("a different task intent");
    assert_eq!(
        execute_at(temp.path(), "allocate", &input, 100)
            .unwrap_err()
            .kind(),
        io::ErrorKind::AlreadyExists
    );
}
#[test]
fn now_listing_lists_allocations_and_filters_by_participant() {
    let temp = world();
    let root = temp.path();
    let alpha = execute_at(root, "allocate", &request(root, None, "task:alpha"), 100).unwrap();
    let mut beta_input = request(root, None, "task:beta");
    beta_input["participant_refs"] = serde_json::json!(["agent:beta", "agent:test"]);
    let beta = execute_at(root, "allocate", &beta_input, 100).unwrap();

    let all = execute_at(root, "now_list", &serde_json::json!({}), 100).unwrap();
    assert_eq!(all["schema"], "central.now-listing/v1");
    let records = all["records"].as_array().unwrap();
    assert_eq!(records.len(), 2);
    let now_refs: Vec<&str> = records
        .iter()
        .map(|r| r["now_ref"].as_str().unwrap())
        .collect();
    assert!(now_refs.contains(&alpha["now_ref"].as_str().unwrap()));
    assert!(now_refs.contains(&beta["now_ref"].as_str().unwrap()));
    let alpha_row = records
        .iter()
        .find(|r| r["now_ref"] == alpha["now_ref"])
        .unwrap();
    assert_eq!(alpha_row["task_ref"], "task:alpha");
    assert_eq!(
        alpha_row["participant_refs"],
        serde_json::json!(["agent:test"])
    );
    assert_eq!(
        alpha_row["revision"]["revision"],
        alpha["revision"]["revision"]
    );

    let filtered = execute_at(
        root,
        "now_list",
        &serde_json::json!({"participant_refs": ["agent:beta"]}),
        100,
    )
    .unwrap();
    let filtered_refs: Vec<&str> = filtered["records"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["now_ref"].as_str().unwrap())
        .collect();
    assert_eq!(filtered_refs, vec![beta["now_ref"].as_str().unwrap()]);

    let none = execute_at(
        root,
        "now_list",
        &serde_json::json!({"participant_refs": ["agent:nobody"]}),
        100,
    )
    .unwrap();
    assert_eq!(none["schema"], "central.now-listing/v1");
    assert_eq!(none["records"].as_array().unwrap().len(), 0);
}

#[test]
fn v2_clearing_record_with_work_refs_deserializes_and_round_trips() {
    // Exactly the shape that broke the v1-only reader: a `central.now-clearing/v2`
    // record carrying `work_refs`. Before v2 support this failed to deserialize
    // with "unknown field `work_refs`", which blocked every root `central.now.*`
    // read and lifecycle operation for the whole World.
    let v2 = r#"{
        "schema": "central.now-clearing/v2",
        "now_ref": "central:now:control:root:test-v2",
        "source_ref": "central:source:control:root:Control/agents/now/clearings/test-v2/now.json",
        "scope_ref": "control:root",
        "task_ref": "control:task:test-v2",
        "purpose": "a lane-owning clearing",
        "participant_refs": [],
        "source_refs": [],
        "policy_revision_at_allocation": "central.content-fnv1a64/v1:2872:2aa797052872efe8",
        "created_at_unix_seconds": 1789672633,
        "lifecycle": "active",
        "obligations": [],
        "continuation_refs": [],
        "archive_ref": null,
        "work_refs": [
            {"repo": "Work/O-I", "branch": "techne/convergence", "worktree_path": "/x/Work/O-I/.agent-worktrees/techne"},
            {"repo": "Work/Actuation", "branch": "act-rust/vak-control"}
        ]
    }"#;
    let record: placement::NowRecord =
        serde_json::from_str(v2).expect("v2 clearing record must deserialize");
    assert_eq!(record.schema, placement::NOW_SCHEMA_V2);
    assert_eq!(record.work_refs.len(), 2);
    assert_eq!(record.work_refs[0].repo, "Work/O-I");
    assert_eq!(
        record.work_refs[0].worktree_path.as_deref(),
        Some("/x/Work/O-I/.agent-worktrees/techne")
    );
    assert_eq!(record.work_refs[1].branch, "act-rust/vak-control");
    assert!(record.work_refs[1].worktree_path.is_none());

    // Re-serialisation (what a lifecycle close does via `encoded(&record)`)
    // preserves the v2 schema and the lane claims verbatim.
    let reserialized = serde_json::to_string(&record).unwrap();
    assert!(reserialized.contains("central.now-clearing/v2"));
    assert!(reserialized.contains("act-rust/vak-control"));
    assert!(reserialized.contains("worktree_path"));

    // A v1 record (no `work_refs`) still reads, defaults to an empty lane set,
    // and re-emits without a `work_refs` key — older readers stay unaffected.
    let v1 = r#"{
        "schema": "central.now-clearing/v1",
        "now_ref": "central:now:control:root:test-v1",
        "source_ref": "central:source:control:root:Control/agents/now/clearings/test-v1/now.json",
        "scope_ref": "control:root",
        "task_ref": "control:task:test-v1",
        "purpose": "an ordinary clearing",
        "participant_refs": [],
        "source_refs": [],
        "policy_revision_at_allocation": "central.content-fnv1a64/v1:2872:2aa797052872efe8",
        "created_at_unix_seconds": 1789672633,
        "lifecycle": "active",
        "obligations": [],
        "continuation_refs": [],
        "archive_ref": null
    }"#;
    let v1_record: placement::NowRecord =
        serde_json::from_str(v1).expect("v1 clearing record must still deserialize");
    assert_eq!(v1_record.schema, placement::NOW_SCHEMA);
    assert!(v1_record.work_refs.is_empty());
    assert!(!serde_json::to_string(&v1_record)
        .unwrap()
        .contains("work_refs"));
}

#[test]
fn allocate_with_work_refs_emits_v2_and_now_read_and_list_surface_lane_claims() {
    let temp = world();
    let root = temp.path();
    let mut input = request(root, None, "task:lanes");
    input["work_refs"] = serde_json::json!([
        {"repo": "Work/one", "branch": "techne/lane-one"},
        {"repo": "Work/two", "branch": "techne/lane-two", "worktree_path": "Work/two/.aikit/tasks/lane-two"}
    ]);
    let allocated = execute_at(root, "allocate", &input, 100).unwrap();

    // The record on disk carries the v2 schema so a v1-only reader fails loudly
    // on schema rather than confusingly on an unknown field.
    let mut raw = String::new();
    for entry in fs::read_dir(root.join("Control/agents/now/clearings"))
        .unwrap()
        .flatten()
    {
        let path = entry.path().join("now.json");
        if let Ok(content) = fs::read_to_string(&path) {
            if content.contains("task:lanes") {
                raw = content;
                break;
            }
        }
    }
    assert!(raw.contains("central.now-clearing/v2"), "record: {raw}");

    // central.now.read returns the record with its lane claims intact.
    let read = execute_at(
        root,
        "now_read",
        &serde_json::json!({"now_ref": allocated["now_ref"]}),
        100,
    )
    .unwrap();
    assert_eq!(read["record"]["schema"], "central.now-clearing/v2");
    assert_eq!(read["record"]["work_refs"].as_array().unwrap().len(), 2);

    // central.now.list surfaces the declared lanes.
    let listed = execute_at(root, "now_list", &serde_json::json!({}), 100).unwrap();
    let row = listed["records"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["now_ref"] == allocated["now_ref"])
        .unwrap();
    assert_eq!(
        row["work_refs"],
        serde_json::json!([
            {"repo": "Work/one", "branch": "techne/lane-one"},
            {"repo": "Work/two", "branch": "techne/lane-two", "worktree_path": "Work/two/.aikit/tasks/lane-two"}
        ])
    );

    // Idempotent re-allocation reads the v2 record back through read_now; a
    // different lane basis conflicts rather than silently overwriting.
    execute_at(root, "allocate", &input, 101).unwrap();
    let mut changed = input.clone();
    changed["work_refs"] = serde_json::json!([{"repo": "Work/one", "branch": "other"}]);
    assert_eq!(
        execute_at(root, "allocate", &changed, 102)
            .unwrap_err()
            .kind(),
        std::io::ErrorKind::AlreadyExists
    );

    // A plain allocation without work_refs keeps the v1 schema and an empty
    // lane set in the listing.
    let plain = execute_at(root, "allocate", &request(root, None, "task:plain"), 103).unwrap();
    let plain_listed = execute_at(root, "now_list", &serde_json::json!({}), 103).unwrap();
    let plain_row = plain_listed["records"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["now_ref"] == plain["now_ref"])
        .unwrap();
    assert_eq!(plain_row["work_refs"], serde_json::json!([]));
}

#[test]
fn allocation_with_work_refs_carries_lane_claims_and_rejects_a_different_basis() {
    let temp = world();
    let root = temp.path();
    let mut input = request(root, None, "task:lanes");
    input["work_refs"] = serde_json::json!([
        {"repo": "Work/one", "branch": "techne/lane-one"},
        {"repo": "Work/two", "branch": "techne/lane-two", "worktree_path": "Work/two/.aikit/tasks/lane-two"}
    ]);
    let allocated = execute_at(root, "allocate", &input, 100).unwrap();

    // The listing carries the declared claims; v1 records (no work_refs)
    // remain exactly as before.
    let listed = execute_at(root, "now_list", &serde_json::json!({}), 100).unwrap();
    let row = listed["records"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["now_ref"] == allocated["now_ref"])
        .unwrap()
        .clone();
    assert_eq!(
        row["work_refs"],
        serde_json::json!([
            {"repo": "Work/one", "branch": "techne/lane-one"},
            {"repo": "Work/two", "branch": "techne/lane-two", "worktree_path": "Work/two/.aikit/tasks/lane-two"}
        ])
    );

    // The raw record carries the v2 schema so older readers fail loudly.
    let mut raw = String::new();
    for entry in fs::read_dir(root.join("Control/agents/now/clearings"))
        .unwrap()
        .flatten()
    {
        let path = entry.path().join("now.json");
        if let Ok(content) = fs::read_to_string(&path) {
            if content.contains("task:lanes") {
                raw = content;
                break;
            }
        }
    }
    assert!(raw.contains("central.now-clearing/v2"), "record: {raw}");
    assert!(raw.contains("techne/lane-one"));

    // Identical basis is an idempotent re-allocation; a different claim set
    // conflicts.
    execute_at(root, "allocate", &input, 101).unwrap();
    let mut changed = input.clone();
    changed["work_refs"] = serde_json::json!([{"repo": "Work/one", "branch": "other"}]);
    assert_eq!(
        execute_at(root, "allocate", &changed, 102)
            .unwrap_err()
            .kind(),
        std::io::ErrorKind::AlreadyExists
    );

    // The census attribution sees the clearing claim.
    let claims = crate::git_census::collect_now_claims(root, &["Work/one".to_owned()]);
    assert_eq!(
        claims
            .get("Work/one")
            .and_then(|by| by.get("techne/lane-one")),
        Some(&row["now_ref"].as_str().unwrap().to_owned())
    );

    // A plain allocation without work_refs keeps the v1 schema.
    let plain = execute_at(root, "allocate", &request(root, None, "task:plain"), 103).unwrap();
    assert!(!plain.is_null());
}

#[test]
fn allocate_advertises_work_refs_in_its_declared_inputs() {
    // The 2026-09-19 root allocate incident was diagnosable only from a raw
    // serde message: the action accepted work_refs at runtime while its
    // advertised contract omitted it. The declared inputs are the surface
    // other agents read; they must carry the field.
    let mut registry = crate::action::ActionRegistry::default();
    register_actions(&mut registry);
    let descriptor = registry.get("central.now.allocate").unwrap();
    let work_refs = descriptor
        .inputs
        .iter()
        .find(|input| input.name == "work_refs")
        .expect("central.now.allocate must advertise work_refs");
    assert_eq!(work_refs.input_type, "array");
    assert!(!work_refs.required);
}

#[test]
fn a_corrupt_bound_clearing_names_itself_in_the_listing_failure() {
    // When one bound record fails to parse, the failure must name the
    // offending record — a bare serde message left the 2026-09-19 incident
    // pointing at the caller's request instead of the stale record.
    let temp = world();
    let root = temp.path();
    let first = execute_at(root, "allocate", &request(root, None, "task:first"), 100).unwrap();
    let second = execute_at(root, "allocate", &request(root, None, "task:second"), 101).unwrap();
    let second_source = second["record"]["source_ref"].as_str().unwrap().to_owned();
    let second_path = second_source
        .split("central:source:control:root:")
        .nth(1)
        .unwrap()
        .to_owned();
    fs::write(
        root.join(&second_path),
        "{\"schema\":\"central.now-clearing/v1\"}",
    )
    .unwrap();

    let error = execute_at(root, "now_list", &serde_json::json!({}), 102).unwrap_err();
    let message = error.to_string();
    assert!(
        message.contains("failed to parse"),
        "message must name the parse failure: {message}"
    );
    assert!(
        message.contains(&second_source),
        "message must name the offending record: {message}"
    );

    // Repairing the corrupt bytes restores the listing without touching the
    // first allocation.
    fs::write(
        root.join(&second_path),
        source::encoded(&serde_json::json!({
            "schema":placement::NOW_SCHEMA,
            "now_ref":second["record"]["now_ref"],
            "source_ref":second_source,
            "scope_ref":second["record"]["scope_ref"],
            "task_ref":"task:second",
            "purpose":"bounded implementation",
            "participant_refs":["agent:test"],
            "source_refs":[],
            "policy_revision_at_allocation":first["record"]["policy_revision_at_allocation"],
            "created_at_unix_seconds":101,
            "lifecycle":"active",
            "obligations":[],
            "continuation_refs":[],
            "archive_ref":null
        }))
        .unwrap(),
    )
    .unwrap();
    let listed = execute_at(root, "now_list", &serde_json::json!({}), 103).unwrap();
    assert_eq!(listed["records"].as_array().unwrap().len(), 2);
}

#[test]
fn now_read_composes_receiving_returns_keyed_to_the_now() {
    let temp = world();
    let root = temp.path();
    let allocated =
        execute_at(root, "allocate", &request(root, None, "task:returns"), 100).unwrap();
    let now_ref = allocated["now_ref"].as_str().unwrap().to_owned();

    // A receiving Return lands on disk under this NOW exactly as
    // central.receiving.submit persists it, carrying the run/session/day refs
    // its producer stamped.
    let area = root.join(".central/source-returns");
    fs::create_dir_all(&area).unwrap();
    fs::write(
        area.join("run-return.json"),
        serde_json::to_string(&serde_json::json!({
            "schema": "central.received-return/v1",
            "return_ref": "return:run-x",
            "now_ref": now_ref,
            "status": "pending",
            "run_ref": "run:x",
            "session_ref": "ses:x",
            "day_ref": "day:2026-09-19"
        }))
        .unwrap(),
    )
    .unwrap();

    // central.now.read composes that Return into the reading's `returns` field
    // — the read-model join, end to end through the real action, not a fixture.
    let read = execute_at(
        root,
        "now_read",
        &serde_json::json!({"now_ref": now_ref}),
        100,
    )
    .unwrap();
    let returns = read["returns"]
        .as_array()
        .expect("now-reading composes its receiving returns");
    assert_eq!(returns.len(), 1);
    assert_eq!(returns[0]["return_ref"], "return:run-x");
    assert_eq!(returns[0]["settled"], false);
    assert_eq!(returns[0]["run_ref"], "run:x");
    assert_eq!(returns[0]["session_ref"], "ses:x");
    assert_eq!(returns[0]["day_ref"], "day:2026-09-19");
}

/// A clearing written before the World-inhabitation horizon existed carries
/// no workcell_ref / parent_now_ref / horizon. Reading it and writing it back
/// (as every lifecycle/obligation transition does) must reproduce the exact
/// bytes, for both the v1 and the v2 (lane work_refs) shape.
#[test]
fn pre_horizon_v1_and_v2_clearings_round_trip_byte_identical() {
    let v1 = r#"{
  "schema": "central.now-clearing/v1",
  "now_ref": "central:now:control:root:0000000000000000000000000000000000000000000000000000000000000001",
  "source_ref": "central:source:control:root:Control/agents/now/clearings/0000000000000000000000000000000000000000000000000000000000000001/now.json",
  "scope_ref": "control:root",
  "task_ref": "task:pre-horizon",
  "purpose": "written before horizons existed",
  "participant_refs": [
    "agent:test"
  ],
  "source_refs": [],
  "policy_revision_at_allocation": "central.content-fnv1a64/v1:10:0000000000000000",
  "created_at_unix_seconds": 100,
  "lifecycle": "active",
  "obligations": [],
  "continuation_refs": [],
  "archive_ref": null
}
"#;
    let v2 = r#"{
  "schema": "central.now-clearing/v2",
  "now_ref": "central:now:project:test/one:0000000000000000000000000000000000000000000000000000000000000002",
  "source_ref": "central:source:project:test/one:ProjectCentral/agents/now/clearings/0000000000000000000000000000000000000000000000000000000000000002/now.json",
  "scope_ref": "project:test/one",
  "task_ref": "task:pre-horizon-lane",
  "purpose": "lane owner written before horizons existed",
  "participant_refs": [],
  "source_refs": [],
  "policy_revision_at_allocation": "central.content-fnv1a64/v1:10:0000000000000000",
  "created_at_unix_seconds": 100,
  "lifecycle": "quiescent",
  "obligations": [],
  "continuation_refs": [],
  "archive_ref": null,
  "work_refs": [
    {
      "repo": "Work/one",
      "branch": "topic",
      "worktree_path": "worktrees/env-2/one"
    }
  ]
}
"#;
    for raw in [v1, v2] {
        let record: placement::NowRecord = serde_json::from_str(raw).unwrap();
        assert!(record.workcell_ref.is_none() && record.parent_now_ref.is_none());
        assert!(record.horizon.is_none());
        assert_eq!(source::encoded(&record).unwrap(), raw);
    }
    // A clearing allocated today without horizon inputs is still exactly the
    // pre-horizon shape: no new key appears on disk.
    let temp = world();
    let allocation = execute_at(
        temp.path(),
        "allocate",
        &request(temp.path(), Some("one"), "task:standalone"),
        100,
    )
    .unwrap();
    let path = allocation["source"]["path"].as_str().unwrap();
    let bytes = fs::read_to_string(temp.path().join("Work/one").join(path)).unwrap();
    for key in ["workcell_ref", "parent_now_ref", "horizon"] {
        assert!(!bytes.contains(key), "{key} leaked into {bytes}");
    }
    let record: placement::NowRecord = serde_json::from_str(&bytes).unwrap();
    assert_eq!(source::encoded(&record).unwrap(), bytes);
    assert_eq!(record.schema, placement::NOW_SCHEMA);
}
