use central_ctrl::action::ActionRegistry;
use central_ctrl::result::ResultStatus;
use central_ctrl::root::{self, REQUIRED_DIRS, RootContext};
use central_ctrl::{ProcessContext, run};
use std::fs;
use tempfile::tempdir;

fn args(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_string()).collect()
}

#[test]
fn root_discovery_defaults_to_home_central() {
    let home = tempdir().unwrap();
    let root = root::resolve_root(&RootContext {
        home: Some(home.path().to_path_buf()),
        ..RootContext::default()
    })
    .unwrap();
    assert_eq!(root, home.path().join("Central"));
}

#[test]
fn configured_root_overrides_default_home_location() {
    let home = tempdir().unwrap();
    let configured = tempdir().unwrap();
    let root = root::resolve_root(&RootContext {
        configured_root: Some(configured.path().to_path_buf()),
        home: Some(home.path().to_path_buf()),
        ..RootContext::default()
    })
    .unwrap();
    assert_eq!(root, configured.path());
}

#[test]
fn explicit_root_overrides_configured_root() {
    let home = tempdir().unwrap();
    let configured = tempdir().unwrap();
    let explicit = tempdir().unwrap();
    let output = run(
        args(&[
            "--root",
            explicit.path().to_str().unwrap(),
            "root",
            "--json",
        ]),
        ProcessContext {
            configured_root: Some(configured.path().to_path_buf()),
            home: Some(home.path().to_path_buf()),
        },
    );
    assert_eq!(output.result.status, ResultStatus::Success);
    assert_eq!(
        output.result.data.unwrap()["root"],
        explicit.path().display().to_string()
    );
}

#[test]
fn initialization_is_idempotent() {
    let temp = tempdir().unwrap();
    let root = temp.path().join("Central");
    let first = root::initialize(&root).unwrap();
    let second = root::initialize(&root).unwrap();
    assert_eq!(first, second);
    for relative in REQUIRED_DIRS {
        assert!(root.join(relative).is_dir());
    }
    assert!(!root.join("Control/user/profile.json").exists());
}

#[test]
fn doctor_reports_invalid_then_valid_structure() {
    let temp = tempdir().unwrap();
    let root = temp.path().join("Central");
    let context = ProcessContext {
        configured_root: Some(root.clone()),
        home: None,
    };
    let invalid = run(args(&["doctor", "--json"]), context.clone());
    assert_eq!(invalid.result.status, ResultStatus::InvalidCentralStructure);
    assert_eq!(invalid.exit_code, 3);
    root::initialize(&root).unwrap();
    let valid = run(args(&["central.doctor", "--json"]), context);
    assert_eq!(valid.result.status, ResultStatus::Success);
    assert_eq!(valid.result.data.unwrap()["valid"], true);
}

#[test]
fn foundation_action_ids_remain_stable() {
    let registry = ActionRegistry::core();
    for id in [
        "action.list",
        "central.doctor",
        "central.init",
        "central.root",
    ] {
        let action = registry
            .get(id)
            .unwrap_or_else(|| panic!("missing Action {id}"));
        assert!(!action.title.is_empty());
        assert!(!action.description.is_empty());
        assert!(!action.output_definition.description.is_empty());
    }
}

#[test]
fn action_list_has_human_and_structured_renderings() {
    let human = run(args(&["action", "list"]), ProcessContext::default());
    assert!(human.render().contains("central.doctor"));
    let structured = run(args(&["action.list", "--json"]), ProcessContext::default());
    let rendered: serde_json::Value = serde_json::from_str(&structured.render()).unwrap();
    assert_eq!(rendered["action"], "action.list");
    assert_eq!(rendered["status"], "success");
    let ids = rendered["data"]["actions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|action| action["id"].as_str().unwrap())
        .collect::<Vec<_>>();
    for id in [
        "action.list",
        "central.doctor",
        "central.init",
        "central.root",
    ] {
        assert!(ids.contains(&id));
    }
}

#[test]
fn invalid_input_is_structured() {
    let output = run(args(&["unknown", "--json"]), ProcessContext::default());
    assert_eq!(output.result.status, ResultStatus::InvalidInput);
    assert_eq!(output.exit_code, 2);
    let rendered: serde_json::Value = serde_json::from_str(&output.render()).unwrap();
    assert_eq!(rendered["error"]["code"], "invalid_input");
}

#[test]
fn filesystem_error_is_reported_as_internal_failure() {
    let temp = tempdir().unwrap();
    let root = temp.path().join("Central");
    fs::write(&root, "file").unwrap();
    let output = run(
        args(&["init", "--json"]),
        ProcessContext {
            configured_root: Some(root),
            home: None,
        },
    );
    assert_eq!(output.result.status, ResultStatus::InternalFailure);
    assert_eq!(output.exit_code, 1);
}
