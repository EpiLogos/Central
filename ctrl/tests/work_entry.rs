use central_ctrl::{
    create_core_action_registry, initialize_central, run_cli, ActionExecutionContext, CliEnvironment,
    ConnectorContext, ConnectorRegistry, FilesystemWorkConnector, ResultStatus, RootOptions,
    WORK_DISCOVERY_PORT,
};
use serde_json::json;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn temporary_directory(label: &str) -> PathBuf {
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let path = std::env::temp_dir().join(format!("central-work-{label}-{}-{nonce}", std::process::id()));
    fs::create_dir_all(&path).unwrap();
    path
}

fn execute(root: &PathBuf, action: &str, input: serde_json::Value) -> central_ctrl::ActionResult {
    let registry = create_core_action_registry();
    let mut connectors = ConnectorRegistry::default();
    connectors.register(FilesystemWorkConnector::new()).unwrap();
    let connector_context = ConnectorContext { platform: "linux".to_owned() };
    let root_options = RootOptions { explicit_root: Some(root.clone()), ..RootOptions::default() };
    let context = ActionExecutionContext { root_options: &root_options, connectors: &connectors, connector_context: &connector_context };
    registry.execute(action, &input, &context)
}

#[test]
fn all_work_entry_actions_depend_on_the_same_public_discovery_port() {
    let registry = create_core_action_registry();
    for id in ["work.list", "work.search", "work.open"] {
        assert_eq!(registry.get(id).unwrap().required_ports, vec![WORK_DISCOVERY_PORT.id]);
    }
}

#[test]
fn search_and_open_operate_on_ordinary_directories_without_project_metadata() {
    let root = temporary_directory("ordinary").join("Central");
    initialize_central(&root).unwrap();
    fs::create_dir(root.join("Work").join("alpha-notes")).unwrap();
    fs::create_dir(root.join("Work").join("beta")).unwrap();

    let search = execute(&root, "work.search", json!({ "query": "alpha" }));
    assert_eq!(search.status, ResultStatus::Success);
    assert_eq!(search.data.as_ref().unwrap()["matches"][0]["name"], "alpha-notes");

    let open = execute(&root, "work.open", json!({ "query": "alpha" }));
    assert_eq!(open.status, ResultStatus::Success);
    assert_eq!(open.data.as_ref().unwrap()["item"]["name"], "alpha-notes");
    assert_eq!(open.data.as_ref().unwrap()["match"], "search");
    assert_eq!(fs::read_dir(root.join(".central")).unwrap().count(), 0);
    assert_eq!(fs::read_dir(root.join("Work").join("alpha-notes")).unwrap().count(), 0);
}

#[test]
fn exact_name_wins_and_new_directories_are_visible_immediately() {
    let root = temporary_directory("exact").join("Central");
    initialize_central(&root).unwrap();
    fs::create_dir(root.join("Work").join("alpha")).unwrap();
    fs::create_dir(root.join("Work").join("alpha-tools")).unwrap();

    let exact = execute(&root, "work.open", json!({ "query": "alpha" }));
    assert_eq!(exact.status, ResultStatus::Success);
    assert_eq!(exact.data.as_ref().unwrap()["item"]["name"], "alpha");
    assert_eq!(exact.data.as_ref().unwrap()["match"], "exact");

    fs::create_dir(root.join("Work").join("gamma")).unwrap();
    let later = execute(&root, "work.open", json!({ "query": "gamma" }));
    assert_eq!(later.status, ResultStatus::Success);
    assert_eq!(later.data.as_ref().unwrap()["item"]["name"], "gamma");
}

#[test]
fn ambiguous_and_missing_work_selection_return_structured_invalid_input() {
    let root = temporary_directory("failure").join("Central");
    initialize_central(&root).unwrap();
    fs::create_dir(root.join("Work").join("alpha-one")).unwrap();
    fs::create_dir(root.join("Work").join("alpha-two")).unwrap();

    let ambiguous = execute(&root, "work.open", json!({ "query": "alpha" }));
    assert_eq!(ambiguous.status, ResultStatus::InvalidInput);
    assert_eq!(ambiguous.error.unwrap().details.unwrap()["matches"].as_array().unwrap().len(), 2);

    let missing = execute(&root, "work.open", json!({ "query": "omega" }));
    assert_eq!(missing.status, ResultStatus::InvalidInput);
    assert_eq!(missing.error.unwrap().details.unwrap()["matches"].as_array().unwrap().len(), 0);
}

#[test]
fn explicit_cli_aliases_project_the_same_canonical_work_action() {
    let root = temporary_directory("cli").join("Central");
    initialize_central(&root).unwrap();
    fs::create_dir(root.join("Work").join("project-a")).unwrap();
    let environment = CliEnvironment { configured_root: None, home: None };

    let canonical = run_cli(&[
        "--json".to_owned(), "--root".to_owned(), root.display().to_string(),
        "work.open".to_owned(), "project-a".to_owned(),
    ], &environment);
    let alias = run_cli(&[
        "--json".to_owned(), "--root".to_owned(), root.display().to_string(),
        "open".to_owned(), "project-a".to_owned(),
    ], &environment);
    assert_eq!(canonical.exit_code, 0);
    assert_eq!(alias.exit_code, 0);
    let canonical_value: serde_json::Value = serde_json::from_str(&canonical.output).unwrap();
    let alias_value: serde_json::Value = serde_json::from_str(&alias.output).unwrap();
    assert_eq!(canonical_value["action"], "work.open");
    assert_eq!(alias_value["action"], "work.open");
    assert_eq!(canonical_value["data"]["item"], alias_value["data"]["item"]);
}
