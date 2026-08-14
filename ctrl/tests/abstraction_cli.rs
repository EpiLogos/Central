use central_ctrl::reference_connectors::FILESYSTEM_WORK_DISCOVERY_ID;
use central_ctrl::result::ResultStatus;
use central_ctrl::root;
use central_ctrl::{ProcessContext, run};
use tempfile::tempdir;

fn args(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_string()).collect()
}

#[test]
fn work_list_cli_reports_canonical_action_and_selected_connector() {
    let temp = tempdir().unwrap();
    let central = temp.path().join("Central");
    root::initialize(&central).unwrap();
    std::fs::create_dir(central.join("Work/alpha")).unwrap();
    let output = run(
        args(&["work", "list", "--json"]),
        ProcessContext {
            configured_root: Some(central),
            home: None,
        },
    );
    assert_eq!(output.result.status, ResultStatus::Success);
    assert_eq!(output.exit_code, 0);
    let value: serde_json::Value = serde_json::from_str(&output.render()).unwrap();
    assert_eq!(value["action"], "work.list");
    assert_eq!(value["data"]["items"][0]["name"], "alpha");
    assert_eq!(
        value["data"]["diagnostics"]["selected_connector"],
        FILESYSTEM_WORK_DISCOVERY_ID
    );
}
