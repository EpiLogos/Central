use central_ctrl::{
    create_core_action_registry, initialize_central, run_cli, ActionExecutionContext, CliEnvironment,
    ConnectorContext, ConnectorRegistry, ResultStatus, RootOptions, CONTROL_ROOTS,
};
use serde_json::json;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn temporary_directory(label: &str) -> PathBuf {
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let path = std::env::temp_dir().join(format!("central-control-{label}-{}-{nonce}", std::process::id()));
    fs::create_dir_all(&path).unwrap();
    path
}

fn execute(root: &PathBuf, action: &str, input: serde_json::Value) -> central_ctrl::ActionResult {
    let registry = create_core_action_registry();
    let connectors = ConnectorRegistry::default();
    let connector_context = ConnectorContext { platform: "test".to_owned() };
    let root_options = RootOptions { explicit_root: Some(root.clone()), ..RootOptions::default() };
    let context = ActionExecutionContext { root_options: &root_options, connectors: &connectors, connector_context: &connector_context };
    registry.execute(action, &input, &context)
}

#[test]
fn control_open_locates_all_three_stable_roots_as_authored_source() {
    let root = temporary_directory("roots").join("Central");
    initialize_central(&root).unwrap();
    for target in CONTROL_ROOTS {
        let result = execute(&root, "control.open", json!({ "target": target }));
        assert_eq!(result.status, ResultStatus::Success);
        let data = result.data.unwrap();
        assert_eq!(data["target"], target);
        assert_eq!(data["source_class"], "authored");
        assert_eq!(data["exists"], true);
        assert!(data["path"].as_str().unwrap().ends_with(&format!("Control/{target}")));
    }
}

#[test]
fn control_search_reads_multiple_human_readable_formats_without_schema() {
    let root = temporary_directory("formats").join("Central");
    initialize_central(&root).unwrap();
    fs::write(root.join("Control/user/about.md"), "# About\nI prefer quiet launchers.\n").unwrap();
    fs::write(root.join("Control/machines/tools.json"), "{\"launcher\":\"Raycast launcher\"}\n").unwrap();
    fs::create_dir_all(root.join("Control/agents/nested")).unwrap();
    fs::write(root.join("Control/agents/nested/voice.notes"), "launcher guidance can remain plain prose\n").unwrap();

    let result = execute(&root, "control.search", json!({ "query": "launcher" }));
    assert_eq!(result.status, ResultStatus::Success);
    let data = result.data.unwrap();
    let matches = data["matches"].as_array().unwrap();
    assert_eq!(matches.len(), 3);
    assert_eq!(data["files_scanned"], 3);
    assert!(data["skipped_sources"].as_array().unwrap().is_empty());
    assert!(matches.iter().all(|item| item["source_class"] == "authored"));
    assert!(matches.iter().any(|item| item["source_path"] == "Control/user/about.md"));
    assert!(matches.iter().any(|item| item["source_path"] == "Control/machines/tools.json"));
    assert!(matches.iter().any(|item| item["source_path"] == "Control/agents/nested/voice.notes"));
}

#[test]
fn control_search_reports_unsupported_non_text_sources_explicitly() {
    let root = temporary_directory("unsupported").join("Central");
    initialize_central(&root).unwrap();
    fs::write(root.join("Control/user/about.md"), "A searchable durable preference.\n").unwrap();
    fs::write(root.join("Control/agents/archive.bin"), [0xff, 0xfe, 0x00, 0x80]).unwrap();

    let result = execute(&root, "control.search", json!({ "query": "durable" }));
    assert_eq!(result.status, ResultStatus::Success);
    let data = result.data.unwrap();
    assert_eq!(data["files_scanned"], 1);
    assert_eq!(data["matches"].as_array().unwrap().len(), 1);

    let skipped = data["skipped_sources"].as_array().unwrap();
    assert_eq!(skipped.len(), 1);
    assert_eq!(skipped[0]["target"], "agents");
    assert_eq!(skipped[0]["source_path"], "Control/agents/archive.bin");
    assert_eq!(skipped[0]["source_class"], "authored");
    assert_eq!(skipped[0]["reason"], "unsupported_non_text_source");
}

#[test]
fn direct_filesystem_edits_are_visible_immediately_without_import_or_generated_index() {
    let root = temporary_directory("direct-edit").join("Central");
    initialize_central(&root).unwrap();
    let source = root.join("Control/agents/style");
    fs::write(&source, "Prefer concise technical prose.\n").unwrap();

    let first = execute(&root, "control.search", json!({ "query": "concise" }));
    assert_eq!(first.data.unwrap()["matches"].as_array().unwrap().len(), 1);

    fs::write(&source, "Prefer spacious explanatory prose.\n").unwrap();
    let old = execute(&root, "control.search", json!({ "query": "concise" }));
    assert!(old.data.unwrap()["matches"].as_array().unwrap().is_empty());
    let changed = execute(&root, "control.search", json!({ "query": "spacious" }));
    assert_eq!(changed.data.unwrap()["matches"].as_array().unwrap().len(), 1);
    assert_eq!(fs::read_dir(root.join(".central")).unwrap().count(), 0);
}

#[test]
fn control_actions_diagnose_invalid_target_and_missing_source_root() {
    let root = temporary_directory("invalid").join("Central");
    initialize_central(&root).unwrap();
    let invalid = execute(&root, "control.open", json!({ "target": "projects" }));
    assert_eq!(invalid.status, ResultStatus::InvalidInput);

    fs::remove_dir(root.join("Control/machines")).unwrap();
    let missing = execute(&root, "control.open", json!({ "target": "machines" }));
    assert_eq!(missing.status, ResultStatus::InvalidCentralStructure);
    let search = execute(&root, "control.search", json!({ "query": "anything" }));
    assert_eq!(search.status, ResultStatus::InvalidCentralStructure);
}

#[test]
fn cli_projects_control_open_and_search_over_the_same_actions() {
    let root = temporary_directory("cli").join("Central");
    initialize_central(&root).unwrap();
    fs::write(root.join("Control/user/note.txt"), "A durable preference for terminal clarity.\n").unwrap();
    let environment = CliEnvironment { configured_root: None, home: None };

    let open = run_cli(&[
        "--root".to_owned(), root.display().to_string(), "control".to_owned(), "open".to_owned(), "user".to_owned(),
    ], &environment);
    assert_eq!(open.exit_code, 0);
    assert!(open.output.contains("user"));
    assert!(open.output.contains("Control/user"));

    let search = run_cli(&[
        "--json".to_owned(), "--root".to_owned(), root.display().to_string(),
        "control".to_owned(), "search".to_owned(), "terminal".to_owned(), "clarity".to_owned(),
    ], &environment);
    assert_eq!(search.exit_code, 0);
    let payload: serde_json::Value = serde_json::from_str(&search.output).unwrap();
    assert_eq!(payload["action"], "control.search");
    assert_eq!(payload["data"]["matches"][0]["source_path"], "Control/user/note.txt");
}
