//! The projects-`main` desired-state policy on `central.machine`, and how its
//! drift verdict is surfaced through `machine.plan` / `machine.verify`.
//!
//! Central declares the projection target (authored intent). It never runs git
//! or computes the drift: repository/worktree state is AIKit's
//! (`aikit worktree project`, `aikit.worktree-projection/v1`). Central surfaces
//! the verdict only from a reading supplied to it. These tests pin all three
//! verdict shapes (Satisfied / Missing / Unsupported), the additive parse, and
//! the `--projection-reading <file>` CLI path.

use central_ctrl::{
    create_core_action_registry, initialize_central, run_cli, ActionExecutionContext,
    CliEnvironment, ConnectorContext, ConnectorRegistry, MachineInspectionOutput, ResultStatus,
    RootOptions, StaticMachineInspectorConnector,
};
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

fn temporary_directory(label: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "central-projection-{label}-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir_all(&path).unwrap();
    path
}

/// An empty host observation: the projection entry is computed purely from the
/// supplied reading, so no packages/configurations/services need observing.
fn empty_observation() -> MachineInspectionOutput {
    MachineInspectionOutput {
        platform: "test-os".to_owned(),
        architecture: "test-arch".to_owned(),
        capabilities: Vec::new(),
        packages: Vec::new(),
        configurations: Vec::new(),
        services: Vec::new(),
    }
}

fn write_current(root: &Path, declaration: Value) {
    fs::write(
        root.join("Control/machines/current.json"),
        serde_json::to_string_pretty(&declaration).unwrap(),
    )
    .unwrap();
}

fn run(root: &Path, action: &str, input: Value) -> central_ctrl::ActionResult {
    let mut connectors = ConnectorRegistry::default();
    connectors
        .register(StaticMachineInspectorConnector::new(empty_observation()))
        .unwrap();
    let connector_context = ConnectorContext {
        platform: "test".to_owned(),
    };
    let root_options = RootOptions {
        explicit_root: Some(root.to_path_buf()),
        ..RootOptions::default()
    };
    let context = ActionExecutionContext {
        root_options: &root_options,
        connectors: &connectors,
        connector_context: &connector_context,
    };
    create_core_action_registry().execute(action, &input, &context)
}

/// The real `current.json` shape (bindings + empty requirements), optionally with
/// a projection policy.
fn current_declaration(projection: Option<Value>) -> Value {
    let mut declaration = json!({
        "schema": "central.machine",
        "version": 1,
        "role": "current",
        "capabilities": [],
        "requirements": { "packages": [], "configurations": [], "services": [] },
        "bindings": [{ "kind": "workcell", "reference": "workcell:local" }]
    });
    if let Some(projection) = projection {
        declaration["projection"] = projection;
    }
    declaration
}

fn policy(target: &str, projects: Value) -> Value {
    json!({ "target": target, "projects": projects })
}

/// One checkout reading carrying the full `aikit.worktree-projection/v1` per-repo
/// shape — Central must tolerate every field it does not itself read.
fn repo_entry(key: &str, action: Value) -> Value {
    json!({
        "key": key,
        "project": format!("project:{key}"),
        "locator": format!("/Users/admin/Central/Work/{key}"),
        "target": "origin/main",
        "head": "deadbeefcafe0000feed",
        "target_revision": "feedface12340000beef",
        "branch": null,
        "detached": true,
        "clean": true,
        "divergence": { "relation": "up-to-date" },
        "action": action
    })
}

fn reading(target: &str, applied: bool, entries: Vec<Value>) -> Value {
    json!({
        "version": "aikit.worktree-projection/v1",
        "target": target,
        "applied": applied,
        "entries": entries
    })
}

fn already_projected() -> Value {
    json!({ "action": "already-projected" })
}

/// Carries `from`/`to` exactly as AIKit serialises them, to prove Central's
/// tag-only reader ignores the payload.
fn fast_forwarded() -> Value {
    json!({ "action": "fast-forwarded", "from": "2bcdc78aaaaa", "to": "a3b1169bbbbb" })
}

fn would_fast_forward() -> Value {
    json!({ "action": "would-fast-forward", "to": "a3b1169bbbbb" })
}

fn surfaced(reason: &str) -> Value {
    json!({ "action": "surfaced", "reason": reason })
}

fn projection_of(plan: &Value) -> Value {
    plan["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["kind"] == "projection")
        .expect("a projection entry")
        .clone()
}

#[test]
fn a_declaration_without_projection_parses_and_stays_projection_free() {
    // Proves the field is additive: an existing current.json that lacks it
    // deserialises cleanly, serialises without the key, and produces no
    // projection plan entry — existing machine behaviour is untouched.
    let root = temporary_directory("additive-absent").join("Central");
    initialize_central(&root).unwrap();
    write_current(&root, current_declaration(None));

    let declaration = run(&root, "machine.declaration", json!({ "role": "current" }));
    assert_eq!(declaration.status, ResultStatus::Success);
    let data = declaration.data.unwrap();
    assert!(data["declaration"].get("projection").is_none());

    let plan = run(&root, "machine.plan", json!({ "role": "current" }));
    assert_eq!(plan.status, ResultStatus::Success);
    let plan = plan.data.unwrap();
    assert!(plan["entries"]
        .as_array()
        .unwrap()
        .iter()
        .all(|entry| entry["kind"] != "projection"));
    assert_eq!(plan["summary"]["unsupported"], 0);
    assert_eq!(plan["summary"]["missing"], 0);
}

#[test]
fn a_projection_policy_parses_and_reads_back() {
    let root = temporary_directory("additive-present").join("Central");
    initialize_central(&root).unwrap();
    write_current(
        &root,
        current_declaration(Some(policy("origin/main", json!([])))),
    );

    let declaration = run(&root, "machine.declaration", json!({ "role": "current" }));
    assert_eq!(declaration.status, ResultStatus::Success);
    let data = declaration.data.unwrap();
    assert_eq!(data["declaration"]["projection"]["target"], "origin/main");
    assert_eq!(
        data["declaration"]["projection"]["projects"]
            .as_array()
            .unwrap()
            .len(),
        0
    );
}

#[test]
fn without_a_reading_the_projection_is_unsupported_and_names_the_aikit_verifier() {
    // The ownership-honest absent case: Central does not run git, so with no
    // AIKit reading the drift verdict is deliberately Unsupported and points at
    // who computes it — exactly as empty capability slots are Unsupported when
    // no reconciliation Port observes them.
    let root = temporary_directory("absent-reading").join("Central");
    initialize_central(&root).unwrap();
    write_current(
        &root,
        current_declaration(Some(policy("origin/main", json!([])))),
    );

    let plan = run(&root, "machine.plan", json!({ "role": "current" }));
    assert_eq!(plan.status, ResultStatus::Success);
    let data = plan.data.unwrap();
    let entry = projection_of(&data);
    assert_eq!(entry["kind"], "projection");
    assert_eq!(entry["id"], "worktree-projection");
    assert_eq!(entry["status"], "unsupported");
    assert_eq!(entry["observed"]["owner"], "aikit");
    assert!(entry["reason"]
        .as_str()
        .unwrap()
        .contains("aikit worktree project --json"));
    assert_eq!(data["summary"]["unsupported"], 1);

    // verify refuses: the projection cannot be confirmed without the reading.
    let verify = run(&root, "machine.verify", json!({ "role": "current" }));
    assert_eq!(verify.status, ResultStatus::VerificationFailure);
    let details = verify.error.unwrap().details.unwrap();
    assert_eq!(details["satisfied"], false);
}

#[test]
fn every_covered_checkout_projected_is_satisfied() {
    let root = temporary_directory("satisfied").join("Central");
    initialize_central(&root).unwrap();
    write_current(
        &root,
        current_declaration(Some(policy("origin/main", json!([])))),
    );

    // Whole suite (empty `projects`): every checkout already-projected or
    // fast-forwarded. The fast-forwarded entry carries from/to to prove parse.
    let observed = reading(
        "origin/main",
        false,
        vec![
            repo_entry("o-i", already_projected()),
            repo_entry("ql-mef", fast_forwarded()),
            repo_entry("central", already_projected()),
        ],
    );

    let plan = run(
        &root,
        "machine.plan",
        json!({ "role": "current", "projection_reading": observed.clone() }),
    );
    assert_eq!(plan.status, ResultStatus::Success);
    let data = plan.data.unwrap();
    let entry = projection_of(&data);
    assert_eq!(entry["status"], "satisfied");
    assert_eq!(entry["observed"]["covered"], 3);
    assert_eq!(entry["observed"]["projected"], 3);
    assert_eq!(data["summary"]["satisfied"], 1);
    assert_eq!(data["summary"]["missing"], 0);
    assert_eq!(data["summary"]["unsupported"], 0);

    let verify = run(
        &root,
        "machine.verify",
        json!({ "role": "current", "projection_reading": observed }),
    );
    assert_eq!(verify.status, ResultStatus::Success);
    assert_eq!(verify.data.unwrap()["satisfied"], true);
}

#[test]
fn drift_is_surfaced_as_missing_naming_the_repos_and_the_owner_command() {
    // Not-projected checkouts are Missing (not Changeable): Central owns no
    // git-reconciliation Port, so `machine.apply` has nothing to run — the
    // repair is `aikit worktree project --apply`. Central surfaces, never fixes.
    let root = temporary_directory("drift").join("Central");
    initialize_central(&root).unwrap();
    write_current(
        &root,
        current_declaration(Some(policy("origin/main", json!([])))),
    );

    let observed = reading(
        "origin/main",
        false,
        vec![
            repo_entry("o-i", already_projected()),
            repo_entry("ql-mef", would_fast_forward()),
            repo_entry(
                "central",
                surfaced("3 local commit(s) not on origin/main; push or open a PR"),
            ),
        ],
    );

    let plan = run(
        &root,
        "machine.plan",
        json!({ "role": "current", "projection_reading": observed.clone() }),
    );
    assert_eq!(plan.status, ResultStatus::Success);
    let data = plan.data.unwrap();
    let entry = projection_of(&data);
    assert_eq!(entry["status"], "missing");

    let not_projected = entry["observed"]["not_projected"].as_array().unwrap();
    let keys: Vec<&str> = not_projected
        .iter()
        .map(|item| item["key"].as_str().unwrap())
        .collect();
    assert!(keys.contains(&"ql-mef"));
    assert!(keys.contains(&"central"));
    assert!(!keys.contains(&"o-i"));
    assert!(entry["reason"]
        .as_str()
        .unwrap()
        .contains("aikit worktree project --apply"));

    // The surfaced reason is carried through unchanged.
    let central = not_projected
        .iter()
        .find(|item| item["key"] == "central")
        .unwrap();
    assert_eq!(central["state"], "surfaced");
    assert!(central["detail"].as_str().unwrap().contains("local commit"));
    assert_eq!(data["summary"]["missing"], 1);
    assert_eq!(data["summary"]["satisfied"], 0);

    let verify = run(
        &root,
        "machine.verify",
        json!({ "role": "current", "projection_reading": observed }),
    );
    assert_eq!(verify.status, ResultStatus::VerificationFailure);
}

#[test]
fn a_scoped_policy_ignores_drift_outside_its_named_projects() {
    let root = temporary_directory("scoped").join("Central");
    initialize_central(&root).unwrap();
    write_current(
        &root,
        current_declaration(Some(policy("origin/main", json!(["o-i", "central"])))),
    );

    // ql-mef is surfaced drift, but the policy does not cover it.
    let observed = reading(
        "origin/main",
        true,
        vec![
            repo_entry("o-i", already_projected()),
            repo_entry("central", fast_forwarded()),
            repo_entry(
                "ql-mef",
                surfaced("diverged from origin/main; reconcile by hand"),
            ),
        ],
    );

    let plan = run(
        &root,
        "machine.plan",
        json!({ "role": "current", "projection_reading": observed }),
    );
    let data = plan.data.unwrap();
    let entry = projection_of(&data);
    assert_eq!(entry["status"], "satisfied");
    assert_eq!(entry["observed"]["covered"], 2);
}

#[test]
fn a_covered_project_absent_from_the_reading_is_missing() {
    let root = temporary_directory("absent-project").join("Central");
    initialize_central(&root).unwrap();
    write_current(
        &root,
        current_declaration(Some(policy("origin/main", json!(["o-i", "workcell"])))),
    );

    // The reading carries o-i (projected) but never mentions workcell.
    let observed = reading(
        "origin/main",
        false,
        vec![repo_entry("o-i", already_projected())],
    );

    let plan = run(
        &root,
        "machine.plan",
        json!({ "role": "current", "projection_reading": observed }),
    );
    let data = plan.data.unwrap();
    let entry = projection_of(&data);
    assert_eq!(entry["status"], "missing");
    let absent: Vec<&str> = entry["observed"]["absent_projects"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_str().unwrap())
        .collect();
    assert_eq!(absent, vec!["workcell"]);
    assert!(entry["reason"]
        .as_str()
        .unwrap()
        .contains("absent from the reading"));
}

#[test]
fn a_reading_against_another_target_does_not_verify_the_policy() {
    let root = temporary_directory("target-mismatch").join("Central");
    initialize_central(&root).unwrap();
    write_current(
        &root,
        current_declaration(Some(policy("origin/main", json!([])))),
    );

    let observed = reading(
        "origin/release",
        false,
        vec![repo_entry("o-i", already_projected())],
    );
    let plan = run(
        &root,
        "machine.plan",
        json!({ "role": "current", "projection_reading": observed }),
    );
    let data = plan.data.unwrap();
    let entry = projection_of(&data);
    assert_eq!(entry["status"], "missing");
    assert_eq!(entry["observed"]["issue"], "target-mismatch");
    assert!(entry["reason"].as_str().unwrap().contains("origin/release"));
}

#[test]
fn a_structurally_invalid_reading_is_rejected() {
    let root = temporary_directory("invalid-reading").join("Central");
    initialize_central(&root).unwrap();
    write_current(
        &root,
        current_declaration(Some(policy("origin/main", json!([])))),
    );

    // Missing the required per-entry `action` tag.
    let bad = json!({
        "version": "aikit.worktree-projection/v1",
        "target": "origin/main",
        "entries": [{ "key": "o-i" }]
    });
    let plan = run(
        &root,
        "machine.plan",
        json!({ "role": "current", "projection_reading": bad }),
    );
    assert_eq!(plan.status, ResultStatus::InvalidInput);
    assert!(plan
        .error
        .unwrap()
        .message
        .contains("aikit.worktree-projection/v1"));
}

#[test]
fn cli_reads_the_projection_reading_from_a_file() {
    let base = temporary_directory("cli");
    let root = base.join("Central");
    initialize_central(&root).unwrap();
    write_current(
        &root,
        current_declaration(Some(policy("origin/main", json!([])))),
    );

    let reading_path = base.join("projection.json");
    let observed = reading(
        "origin/main",
        false,
        vec![
            repo_entry("o-i", already_projected()),
            repo_entry("central", already_projected()),
        ],
    );
    fs::write(
        &reading_path,
        serde_json::to_string_pretty(&observed).unwrap(),
    )
    .unwrap();

    let environment = CliEnvironment {
        configured_root: None,
        home: None,
    };

    // `machine plan current --projection-reading <file>` reads the file and
    // surfaces the satisfied verdict.
    let plan = run_cli(
        &[
            "--json".to_owned(),
            "--root".to_owned(),
            root.display().to_string(),
            "machine".to_owned(),
            "plan".to_owned(),
            "current".to_owned(),
            "--projection-reading".to_owned(),
            reading_path.display().to_string(),
        ],
        &environment,
    );
    assert_eq!(plan.exit_code, 0);
    let value: Value = serde_json::from_str(&plan.output).unwrap();
    assert_eq!(value["action"], "machine.plan");
    let entry = value["data"]["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["kind"] == "projection")
        .unwrap();
    assert_eq!(entry["status"], "satisfied");

    // `machine verify current --projection-reading <file>` passes.
    let verify = run_cli(
        &[
            "--json".to_owned(),
            "--root".to_owned(),
            root.display().to_string(),
            "machine".to_owned(),
            "verify".to_owned(),
            "current".to_owned(),
            "--projection-reading".to_owned(),
            reading_path.display().to_string(),
        ],
        &environment,
    );
    assert_eq!(verify.exit_code, 0);
}

#[test]
fn the_projection_reading_flag_is_rejected_on_other_commands() {
    let environment = CliEnvironment {
        configured_root: None,
        home: None,
    };
    // The flag is checked before the file is read, so a non-existent path still
    // reports the scope error rather than a read failure.
    let result = run_cli(
        &[
            "machine".to_owned(),
            "declaration".to_owned(),
            "current".to_owned(),
            "--projection-reading".to_owned(),
            "/nonexistent/projection.json".to_owned(),
        ],
        &environment,
    );
    assert_ne!(result.exit_code, 0);
    assert!(result
        .output
        .contains("--projection-reading applies only to"));
}
