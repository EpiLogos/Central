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
    assert_eq!(
        result.status,
        ResultStatus::Success,
        "status={status:?} error={error:?}",
        status = result.status,
        error = result.error
    );
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
    assert_eq!(
        result.status,
        ResultStatus::Success,
        "status={status:?} error={error:?}",
        status = result.status,
        error = result.error
    );
    let _ = &result;
    let data = result.data.expect("created data");
    assert_eq!(data["outcome"], "created");
    assert_eq!(data["changed"], json!(true));
    let revision = data["revision"].as_str().expect("revision").to_owned();
    let on_disk =
        fs::read_to_string(root.join("Control/user/flows/flow-2026-09-13-1200.html")).unwrap();
    assert_eq!(
        on_disk,
        "<!doctype html><html><body>flow instance</body></html>"
    );

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

    // Outside Control/user/flows: absent ordinary files are not created. The refusal is Central's own words, never the
    // filesystem's bare "No such file or directory": an agent running the Action by its id learns which Action makes a new
    // file, and protected ground refuses as it does for a file that exists.
    let work = mk("Work/Bare/new.txt", "");
    assert_eq!(work.status, ResultStatus::InvalidInput, "{:?}", work.data);
    let words = work
        .error
        .as_ref()
        .map(|e| e.message.clone())
        .unwrap_or_default();
    assert!(
        words.contains("Work/Bare/new.txt does not exist")
            && words.contains("central.files.create"),
        "{words}"
    );
    for path in [
        "Control/agents/now/flows/flow-x.md",
        "Control/user/notes.txt",
    ] {
        let protected = mk(path, "");
        assert_eq!(
            protected.status,
            ResultStatus::UnavailableCapability,
            "{path}"
        );
        let words = protected
            .error
            .as_ref()
            .map(|e| e.message.clone())
            .unwrap_or_default();
        assert!(words.starts_with("Protected ground"), "{path}: {words}");
    }

    // Inside flows but claiming a basis for a file that does not exist.
    let claimed = mk("Control/user/flows/flow-2026-09-13-1300.html", "rev/1");
    // A create with a claimed basis is a conflict, not a silent mint.
    assert_eq!(
        claimed.status,
        ResultStatus::VerificationFailure,
        "{:?}",
        claimed.data
    );

    // The refusals created nothing.
    assert!(!root.join("Work/Bare/new.txt").exists());
    assert!(!root.join("Control/agents/now/flows/flow-x.md").exists());
    assert!(!root.join("Control/user/notes.txt").exists());
    assert!(!root
        .join("Control/user/flows/flow-2026-09-13-1300.html")
        .exists());
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
    assert_eq!(
        fs::read_to_string(root.join("Control/user/flows/flow-2026-09-13-1400.html")).unwrap(),
        "first"
    );
    // CliEnvironment remains constructible in the test's own right.
    let _ = std::marker::PhantomData::<CliEnvironment>;
}

/// The root's change horizon (its owner Action, projectcentral.change.horizon): the changes of one path, in the order recorded.
fn horizon_changes(options: &RootOptions, path: &str) -> Vec<serde_json::Value> {
    let mut registry = create_core_action_registry();
    central_ctrl::projectcentral_ops::register_projectcentral_actions(&mut registry);
    let read = run(
        &registry,
        options,
        "projectcentral.change.horizon",
        &json!({}),
    );
    assert_eq!(read.status, ResultStatus::Success, "{:?}", read.error);
    read.data.unwrap()["changes"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|change| change["source_path"] == path)
        .cloned()
        .collect()
}

#[test]
fn an_agent_sessions_page_writes_are_horizon_changes_that_name_its_session() {
    // A page of your world's Flows carrier is a source of the root's change horizon. An agent session that creates it and writes it
    // through the owner's doors declares itself (actor_kind agent, agent_session_ref); the horizon's change for each write carries
    // that declaration, as every owner-Action write's does (O-I #746). An edit made outside the doors in between stays its own
    // change and names no one: a write is attributed only the change it made.
    let root = temporary_ground("flow-agent-horizon");
    let registry = create_core_action_registry();
    let options = RootOptions {
        explicit_root: Some(root.clone()),
        ..RootOptions::default()
    };
    let session = "agent-session:746-ring";
    let path = "Control/user/flows/made-by-an-agent.md";
    fs::create_dir_all(root.join("Control/user/flows")).unwrap();
    horizon_changes(&options, path);
    let parent = json!({
        "schema": "central.path-ref/v1",
        "ref": format!("central:path:{}:Control/user/flows", root.display()),
        "root": root.display().to_string(),
        "path": "Control/user/flows",
    });
    let created = run(
        &registry,
        &options,
        "central.files.create",
        &json!({"parent": parent, "name": "made-by-an-agent.md", "content": "# one\n", "expected_absent": true,
                "operation_ref": "op:746-create", "actor": session, "actor_kind": "agent", "agent_session_ref": session}),
    );
    assert_eq!(created.status, ResultStatus::Success, "{:?}", created.error);
    fs::write(root.join(path), "# one, edited outside\n").unwrap();
    let outside = horizon_changes(&options, path);
    let current = outside.last().unwrap()["after_revision"].clone();
    let written = run(
        &registry,
        &options,
        "central.files.write",
        &json!({"location": flow_location(&root, "made-by-an-agent.md"), "expected_revision": current,
                "content": "# two\n", "actor": session, "actor_kind": "agent", "agent_session_ref": session}),
    );
    assert_eq!(written.status, ResultStatus::Success, "{:?}", written.error);
    assert_eq!(written.data.unwrap()["outcome"], "written");
    let changes = horizon_changes(&options, path);
    let seen: Vec<_> = changes
        .iter()
        .map(|c| {
            (
                c["kind"].clone(),
                c["actor_kind"].clone(),
                c["agent_session_ref"].clone(),
            )
        })
        .collect();
    assert_eq!(
        seen,
        vec![
            (json!("added"), json!("agent"), json!(session)),
            (
                json!("modified"),
                serde_json::Value::Null,
                serde_json::Value::Null
            ),
            (json!("modified"), json!("agent"), json!(session)),
        ],
        "each write names the session it was made in; the outside edit names no one: {changes:#?}"
    );
}

/// A world folder a person names freely ("My World", "DLW Worlds") puts a space in the ground's path. Central's own listing escapes
/// the root in every location ref it hands out (files.rs CentralPathRef); a Flow created beside that listing — the ref the shell builds
/// from it, as Start a Flow and New › Flow do — must be accepted by the create door, not refused as "another root" (O-I map #706,
/// 12 Nov empty-start walk).
#[test]
fn a_flow_is_created_from_the_listing_ref_where_the_ground_path_has_a_space() {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir()
        .join(format!("central-spaced-{}-{nonce}", std::process::id()))
        .join("My World");
    fs::create_dir_all(&path).unwrap();
    let root = fs::canonicalize(&path).unwrap();
    let registry = create_core_action_registry();
    let options = RootOptions {
        explicit_root: Some(root.clone()),
        ..RootOptions::default()
    };
    let init = run(&registry, &options, "central.init", &json!({}));
    assert_eq!(init.status, ResultStatus::Success, "init: {:?}", init.error);
    let listed = run(
        &registry,
        &options,
        "central.files.list",
        &json!({"path": "Control/user"}),
    );
    assert_eq!(
        listed.status,
        ResultStatus::Success,
        "list: {:?}",
        listed.error
    );
    let base = listed.data.expect("listing")["location"].clone();
    let base_ref = base["ref"].as_str().expect("listing ref");
    assert!(
        base_ref.contains("%20"),
        "the listing escapes the space in the root: {base_ref}"
    );
    let name = "flow-2026-11-12-2216.html";
    let location = json!({
        "schema": "central.path-ref/v1",
        "ref": format!("{base_ref}/flows/{name}"),
        "root": base["root"],
        "path": format!("{}/flows/{name}", base["path"].as_str().unwrap()),
    });
    let created = run(
        &registry,
        &options,
        "central.files.write",
        &json!({
            "location": location,
            "expected_revision": "",
            "content": "<!doctype html><html><body>a first entry</body></html>",
            "actor": "human:desktop",
            "actor_kind": "human",
        }),
    );
    assert_eq!(
        created.status,
        ResultStatus::Success,
        "the create door accepts the listing's own ref: {:?}",
        created.error
    );
    assert_eq!(created.data.expect("created")["outcome"], "created");
    assert!(root.join("Control/user/flows").join(name).is_file());
}
