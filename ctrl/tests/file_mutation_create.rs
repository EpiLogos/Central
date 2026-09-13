//! The create door of `central.files.write`: an absent ordinary file is
//! created only as a flow instance under `Control/user/flows/`, only with an
//! empty expected revision, seeded into the same CAS journal as every other
//! commit. Real registry, real filesystem; no backend substitutes.

use central_ctrl::{
    create_core_action_registry, ActionExecutionContext, ActionRegistry, CliEnvironment,
    ConnectorContext, ConnectorRegistry, ResultStatus, RootOptions,
};
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

fn temporary_ground(label: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!("central-{label}-{}-{nonce}", std::process::id()));
    fs::create_dir_all(&path).unwrap();
    let root = fs::canonicalize(&path).unwrap();
    let registry = create_core_action_registry();
    let options = RootOptions {
        explicit_root: Some(root.clone()),
        ..RootOptions::default()
    };
    let result = run(&registry, &options, "central.init", &json!({}));
    assert_eq!(result.status, ResultStatus::Success, "status={status:?} error={error:?}", status = result.status, error = result.error);
    let _ = &result;
    root
}

fn run(
    registry: &ActionRegistry,
    root_options: &RootOptions,
    id: &str,
    input: &serde_json::Value,
) -> central_ctrl::ActionResult {
    let connectors = ConnectorRegistry::default();
    let connector_context = ConnectorContext {
        platform: "test".to_owned(),
    };
    let context = ActionExecutionContext {
        root_options,
        connectors: &connectors,
        connector_context: &connector_context,
    };
    registry.execute(id, input, &context)
}

fn flow_location(root: &Path, name: &str) -> serde_json::Value {
    let path = format!("Control/user/flows/{name}");
    json!({
        "schema": "central.path-ref/v1",
        "ref": format!("central:path:{}:{path}", root.display()),
        "root": root.display().to_string(),
        "path": path,
    })
}

#[test]
fn an_absent_flow_instance_is_created_with_empty_expected_revision() {
    let root = temporary_ground("flow-create");
    let registry = create_core_action_registry();
    let options = RootOptions {
        explicit_root: Some(root.clone()),
        ..RootOptions::default()
    };
    let result = run(
        &registry,
        &options,
        "central.files.write",
        &json!({
            "location": flow_location(&root, "flow-2026-09-13-1200.html"),
            "expected_revision": "",
            "content": "<!doctype html><html><body>flow instance</body></html>",
            "actor": "human:desktop",
            "actor_kind": "human",
        }),
    );
    assert_eq!(result.status, ResultStatus::Success, "status={status:?} error={error:?}", status = result.status, error = result.error);
    let _ = &result;
    let data = result.data.expect("created data");
    assert_eq!(data["outcome"], "created");
    assert_eq!(data["changed"], json!(true));
    let revision = data["revision"].as_str().expect("revision").to_owned();
    let on_disk =
        fs::read_to_string(root.join("Control/user/flows/flow-2026-09-13-1200.html")).unwrap();
    assert_eq!(on_disk, "<!doctype html><html><body>flow instance</body></html>");

    // The created file is an ordinary CAS file from here: a revision-checked
    // write lands, a stale basis conflicts, and history records the creation
    // as the first event with no previous revision.
    let loc = flow_location(&root, "flow-2026-09-13-1200.html");
    let second = run(
        &registry,
        &options,
        "central.files.write",
        &json!({
            "location": loc,
            "expected_revision": revision,
            "content": "<!doctype html><html><body>flow instance, revised</body></html>",
            "actor": "human:desktop",
            "actor_kind": "human",
        }),
    );
    assert_eq!(second.status, ResultStatus::Success, "full={second:?}");
    assert_eq!(second.data.expect("d")["outcome"], "written");

    let stale = run(
        &registry,
        &options,
        "central.files.write",
        &json!({
            "location": loc,
            "expected_revision": "stale-revision",
            "content": "x",
            "actor": "human:desktop",
            "actor_kind": "human",
        }),
    );
    assert_eq!(stale.status, ResultStatus::Success);
    assert_eq!(stale.data.expect("d")["outcome"], "conflict");

    let history = run(
        &registry,
        &options,
        "central.files.history",
        &json!({"location": loc}),
    );
    assert_eq!(history.status, ResultStatus::Success);
    let entries = history.data.expect("d")["entries"]
        .as_array()
        .expect("entries")
        .clone();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0]["cursor"], json!(2));
    let creation = entries.last().expect("creation event");
    assert_eq!(creation["cursor"], json!(1));
    assert_eq!(creation["previous_revision"], json!(""));
}

#[test]
fn creation_is_refused_outside_the_user_flows_area_and_with_a_claimed_basis() {
    let root = temporary_ground("flow-create-refusals");
    let registry = create_core_action_registry();
    let options = RootOptions {
        explicit_root: Some(root.clone()),
        ..RootOptions::default()
    };
    let mk = |path: &str, expected: &str| {
        run(
            &registry,
            &options,
            "central.files.write",
            &json!({
                "location": {
                    "schema": "central.path-ref/v1",
                    "ref": format!("central:path:{}:{path}", root.display()),
                    "root": root.display().to_string(),
                    "path": path,
                },
                "expected_revision": expected,
                "content": "x",
                "actor": "human:desktop",
                "actor_kind": "human",
            }),
        )
    };

    // Outside Control/user/flows: absent ordinary files are not created.
    let work = mk("Work/Bare/new.txt", "");
    assert_eq!(work.status, ResultStatus::InvalidInput, "{:?}", work.data);
    let protected = mk("Control/agents/now/flows/flow-x.md", "");
    assert_eq!(protected.status, ResultStatus::InvalidInput);
    let other_user = mk("Control/user/notes.txt", "");
    assert_eq!(other_user.status, ResultStatus::InvalidInput);

    // Inside flows but claiming a basis for a file that does not exist.
    let claimed = mk("Control/user/flows/flow-2026-09-13-1300.html", "rev/1");
    // A create with a claimed basis is a conflict, not a silent mint.
    assert_eq!(claimed.status, ResultStatus::VerificationFailure, "{:?}", claimed.data);

    // The refusals created nothing.
    assert!(!root.join("Work/Bare/new.txt").exists());
    assert!(!root.join("Control/agents/now/flows/flow-x.md").exists());
    assert!(!root.join("Control/user/notes.txt").exists());
    assert!(!root.join("Control/user/flows/flow-2026-09-13-1300.html").exists());
}

#[test]
fn a_second_creation_of_the_same_instance_is_a_conflict_not_an_overwrite() {
    let root = temporary_ground("flow-create-collision");
    let registry = create_core_action_registry();
    let options = RootOptions {
        explicit_root: Some(root.clone()),
        ..RootOptions::default()
    };
    let loc = flow_location(&root, "flow-2026-09-13-1400.html");
    let first = run(
        &registry,
        &options,
        "central.files.write",
        &json!({
            "location": loc,
            "expected_revision": "",
            "content": "first",
            "actor": "human:desktop",
            "actor_kind": "human",
        }),
    );
    assert_eq!(first.status, ResultStatus::Success, "{:?}", first.data);
    // An existing file no longer takes the create door: an empty expected
    // revision against existing bytes is a stale basis, not a mint.
    let again = run(
        &registry,
        &options,
        "central.files.write",
        &json!({
            "location": loc,
            "expected_revision": "",
            "content": "second",
            "actor": "human:desktop",
            "actor_kind": "human",
        }),
    );
    assert_eq!(again.status, ResultStatus::Success);
    assert_eq!(again.data.expect("d")["outcome"], "conflict");
    assert_eq!(fs::read_to_string(root.join("Control/user/flows/flow-2026-09-13-1400.html")).unwrap(), "first");
    // CliEnvironment remains constructible in the test's own right.
    let _ = std::marker::PhantomData::<CliEnvironment>;
}
