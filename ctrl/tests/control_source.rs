use central_ctrl::{ProcessContext, control, root, run};
use central_ctrl::result::ResultStatus;
use tempfile::tempdir;

fn args(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_string()).collect()
}

#[test]
fn control_open_targets_all_three_stable_source_roots() {
    let temp = tempdir().unwrap();
    let central = temp.path().join("Central");
    root::initialize(&central).unwrap();

    for target in ["user", "agents", "machines"] {
        let result = control::open(&central, target);
        assert_eq!(result.status, ResultStatus::Success);
        let data = result.data.unwrap();
        assert_eq!(data["source_class"], "authored");
        assert!(data["path"].as_str().unwrap().ends_with(target));
    }
}

#[test]
fn control_search_reads_live_source_and_sees_direct_edits() {
    let temp = tempdir().unwrap();
    let central = temp.path().join("Central");
    root::initialize(&central).unwrap();
    let source = central.join("Control/user/preferences.md");
    std::fs::write(&source, "Editor: Helix\n").unwrap();

    let first = control::search(&central, "helix");
    assert_eq!(first.status, ResultStatus::Success);
    assert_eq!(first.data.as_ref().unwrap()["matches"].as_array().unwrap().len(), 1);

    std::fs::write(&source, "Editor: Zed\n").unwrap();
    let stale_query = control::search(&central, "helix");
    assert!(stale_query.data.as_ref().unwrap()["matches"].as_array().unwrap().is_empty());
    let fresh_query = control::search(&central, "zed");
    assert_eq!(fresh_query.data.as_ref().unwrap()["matches"][0]["path"], "Control/user/preferences.md");
}

#[test]
fn derived_central_state_is_never_searched_as_authored_control() {
    let temp = tempdir().unwrap();
    let central = temp.path().join("Central");
    root::initialize(&central).unwrap();
    std::fs::create_dir(central.join(".central")).unwrap();
    std::fs::write(central.join(".central/observed.md"), "derived-only-marker\n").unwrap();
    std::fs::write(central.join("Control/agents/source.md"), "authored-marker\n").unwrap();

    let derived = control::search(&central, "derived-only-marker");
    assert!(derived.data.as_ref().unwrap()["matches"].as_array().unwrap().is_empty());
    let authored = control::search(&central, "authored-marker");
    assert_eq!(authored.data.as_ref().unwrap()["matches"][0]["path"], "Control/agents/source.md");
}

#[test]
fn unsupported_source_formats_are_reported_without_becoming_authority() {
    let temp = tempdir().unwrap();
    let central = temp.path().join("Central");
    root::initialize(&central).unwrap();
    std::fs::write(central.join("Control/user/preferences.md"), "needle\n").unwrap();
    std::fs::write(central.join("Control/user/archive.bin"), b"needle\x00binary").unwrap();

    let result = control::search(&central, "needle");
    assert_eq!(result.status, ResultStatus::Success);
    let data = result.data.unwrap();
    assert_eq!(data["matches"].as_array().unwrap().len(), 1);
    assert_eq!(data["skipped"][0]["path"], "Control/user/archive.bin");
    assert_eq!(data["skipped"][0]["reason"], "unsupported_format");
}

#[test]
fn missing_control_root_is_a_structured_failure() {
    let temp = tempdir().unwrap();
    let central = temp.path().join("Central");
    root::initialize(&central).unwrap();
    std::fs::remove_dir(central.join("Control/machines")).unwrap();

    let result = control::search(&central, "anything");
    assert_eq!(result.status, ResultStatus::InvalidCentralStructure);
    assert_eq!(result.data.unwrap()["missing_roots"][0]["root"], "machines");
}

#[test]
fn control_search_creates_no_index_database_or_derived_state() {
    let temp = tempdir().unwrap();
    let central = temp.path().join("Central");
    root::initialize(&central).unwrap();
    std::fs::write(central.join("Control/machines/server.json"), "{\"role\":\"server\"}\n").unwrap();

    let result = control::search(&central, "server");
    assert_eq!(result.status, ResultStatus::Success);
    assert!(!central.join(".central").exists());
}

#[test]
fn cli_projects_control_actions_without_parallel_semantics() {
    let temp = tempdir().unwrap();
    let central = temp.path().join("Central");
    root::initialize(&central).unwrap();
    std::fs::write(central.join("Control/user/preferences.txt"), "terminal preference\n").unwrap();
    let context = ProcessContext { configured_root: Some(central), home: None };

    let opened = run(args(&["control", "open", "user", "--json"]), context.clone());
    assert_eq!(opened.result.action, "control.open");
    assert_eq!(opened.result.status, ResultStatus::Success);

    let searched = run(args(&["control", "search", "terminal", "preference", "--json"]), context);
    assert_eq!(searched.result.action, "control.search");
    assert_eq!(searched.result.status, ResultStatus::Success);
    assert_eq!(searched.result.data.unwrap()["matches"].as_array().unwrap().len(), 1);
}
