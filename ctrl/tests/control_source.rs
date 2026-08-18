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
fn control_open_keeps_agents_explicitly_mixed_after_the_governance_wiki_split() {
    let root = temporary_directory("roots").join("Central");
    initialize_central(&root).unwrap();
    for target in CONTROL_ROOTS {
        let result = execute(&root, "control.open", json!({ "target": target }));
        assert_eq!(result.status, ResultStatus::Success);
        let data = result.data.unwrap();
        assert_eq!(data["target"], target);
        assert_eq!(
            data["source_class"],
            if target == "agents" { "mixed" } else { "authored" }
        );
        assert_eq!(data["exists"], true);
        assert!(data["path"].as_str().unwrap().ends_with(&format!("Control/{target}")));
    }
}

#[test]
fn control_search_reads_human_source_but_not_agent_wiki_as_authored_source() {
    let root = temporary_directory("formats").join("Central");
    initialize_central(&root).unwrap();
    fs::write(root.join("Control/user/about.md"), "# About\nI prefer quiet launchers.\n").unwrap();
    fs::write(root.join("Control/machines/tools.json"), "{\"launcher\":\"Raycast launcher\"}\n").unwrap();
    fs::create_dir_all(root.join("Control/agents/governance/nested")).unwrap();
    fs::write(
        root.join("Control/agents/governance/nested/voice.notes"),
        "launcher guidance can remain plain prose\n",
    )
    .unwrap();
    fs::write(
        root.join("Control/agents/wiki/should-not-surface.txt"),
        "launcher is Agent-maintained Wiki knowledge, not human-authored governance\n",
    )
    .unwrap();

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
    assert!(matches.iter().any(|item| item["source_path"] == "Control/agents/governance/nested/voice.notes"));
    assert!(!matches.iter().any(|item| item["source_path"].as_str().unwrap().contains("agents/wiki")));
}

#[test]
fn pre_split_direct_agent_governance_files_remain_human_authored_by_provenance() {
    let root = temporary_directory("legacy-agents").join("Central");
    initialize_central(&root).unwrap();
    fs::write(
        root.join("Control/agents/legacy-style.txt"),
        "Legacy governance prefers compact evidence.\n",
    )
    .unwrap();
    let result = execute(&root, "control.search", json!({ "query": "compact" }));
    let data = result.data.unwrap();
    assert_eq!(data["matches"].as_array().unwrap().len(), 1);
    assert_eq!(data["matches"][0]["source_path"], "Control/agents/legacy-style.txt");
    assert_eq!(data["matches"][0]["source_class"], "authored");
}

#[test]
fn product_ground_is_ordinary_nested_user_source_not_a_fourth_control_root() {
    let root = temporary_directory("product-ground").join("Central");
    initialize_central(&root).unwrap();
    let product = root.join("Control/user/products/example");
    fs::create_dir_all(product.join("expressions")).unwrap();
    fs::create_dir_all(product.join("positions")).unwrap();
    fs::write(
        product.join("expressions/encounter.md"),
        "I want the encounter to remain directly manipulable by the person.\n",
    )
    .unwrap();
    fs::write(
        product.join("positions/INTERACTION.md"),
        "The primary interaction remains human-addressable.\n",
    )
    .unwrap();
    fs::write(
        product.join("VISION.md"),
        "The product should preserve human authorship while increasing agency.\n",
    )
    .unwrap();

    let opened = execute(&root, "control.open", json!({ "target": "user" }));
    assert_eq!(opened.status, ResultStatus::Success);
    assert_eq!(opened.data.unwrap()["source_class"], "authored");

    let result = execute(&root, "control.search", json!({ "query": "human" }));
    assert_eq!(result.status, ResultStatus::Success);
    let data = result.data.unwrap();
    let matches = data["matches"].as_array().unwrap();
    assert_eq!(matches.len(), 2);
    assert!(matches.iter().all(|item| item["target"] == "user"));
    assert!(matches.iter().all(|item| item["source_class"] == "authored"));
    assert!(matches.iter().any(|item| item["source_path"] == "Control/user/products/example/positions/INTERACTION.md"));
    assert!(matches.iter().any(|item| item["source_path"] == "Control/user/products/example/VISION.md"));
    assert_eq!(fs::read_dir(root.join(".central")).unwrap().count(), 0);
    assert_eq!(CONTROL_ROOTS, ["user", "agents", "machines"]);
}

#[test]
fn control_search_reports_unsupported_human_source_explicitly() {
    let root = temporary_directory("unsupported").join("Central");
    initialize_central(&root).unwrap();
    fs::write(root.join("Control/user/about.md"), "A searchable durable preference.\n").unwrap();
    fs::write(root.join("Control/agents/governance/archive.bin"), [0xff, 0xfe, 0x00, 0x80]).unwrap();

    let result = execute(&root, "control.search", json!({ "query": "durable" }));
    assert_eq!(result.status, ResultStatus::Success);
    let data = result.data.unwrap();
    assert_eq!(data["files_scanned"], 1);
    assert_eq!(data["matches"].as_array().unwrap().len(), 1);

    let skipped = data["skipped_sources"].as_array().unwrap();
    assert_eq!(skipped.len(), 1);
    assert_eq!(skipped[0]["target"], "agents");
    assert_eq!(skipped[0]["source_path"], "Control/agents/governance/archive.bin");
    assert_eq!(skipped[0]["source_class"], "authored");
    assert_eq!(skipped[0]["reason"], "unsupported_non_text_source");
}

#[test]
fn direct_governance_filesystem_edits_are_visible_without_import_or_generated_index() {
    let root = temporary_directory("direct-edit").join("Central");
    initialize_central(&root).unwrap();
    let source = root.join("Control/agents/governance/style");
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
fn retrieval_deny_marker_excludes_an_arbitrary_human_subtree() {
    let root = temporary_directory("private").join("Central");
    initialize_central(&root).unwrap();
    let private = root.join("Control/user/my-own-name-for-private-material");
    fs::create_dir_all(&private).unwrap();
    fs::write(private.join(".no-agent-retrieval"), "").unwrap();
    fs::write(private.join("note.md"), "This concealed-marker-text must not be retrieved.\n").unwrap();

    let result = execute(&root, "control.search", json!({ "query": "concealed-marker-text" }));
    let data = result.data.unwrap();
    assert!(data["matches"].as_array().unwrap().is_empty());
    assert_eq!(data["skipped_sources"].as_array().unwrap().len(), 1);
    assert_eq!(data["skipped_sources"][0]["reason"], "not_agent_readable");
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
