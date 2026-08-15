use central_ctrl::{initialize_central, run_cli, CliEnvironment, ResultStatus};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn temporary_directory(label: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "central-cli-contract-{label}-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir_all(&path).unwrap();
    path
}

fn environment(root: &PathBuf) -> CliEnvironment {
    CliEnvironment {
        configured_root: Some(root.clone()),
        home: None,
    }
}

#[test]
fn generic_action_run_invokes_registered_actions_with_structured_input() {
    let root = temporary_directory("generic").join("Central");
    initialize_central(&root).unwrap();
    fs::create_dir_all(root.join("Work/example-project")).unwrap();

    let list = run_cli(
        &[
            "--json".to_owned(),
            "action".to_owned(),
            "run".to_owned(),
            "action.list".to_owned(),
        ],
        &environment(&root),
    );
    assert_eq!(list.result.status, ResultStatus::Success);
    assert_eq!(list.result.action.as_deref(), Some("action.list"));

    let search = run_cli(
        &[
            "--json".to_owned(),
            "action".to_owned(),
            "run".to_owned(),
            "work.search".to_owned(),
            r#"{"query":"example"}"#.to_owned(),
        ],
        &environment(&root),
    );
    assert_eq!(search.result.status, ResultStatus::Success);
    assert_eq!(search.result.action.as_deref(), Some("work.search"));
    assert_eq!(
        search.result.data.as_ref().unwrap()["matches"][0]["name"],
        "example-project"
    );
}

#[test]
fn work_reveal_has_a_stock_cli_projection_even_when_no_provider_is_available() {
    let root = temporary_directory("reveal").join("Central");
    initialize_central(&root).unwrap();
    fs::create_dir_all(root.join("Work/example-project")).unwrap();

    let result = run_cli(
        &[
            "--json".to_owned(),
            "work".to_owned(),
            "reveal".to_owned(),
            "example-project".to_owned(),
        ],
        &environment(&root),
    );
    assert_eq!(result.result.action.as_deref(), Some("work.reveal"));
    assert_eq!(result.result.status, ResultStatus::UnavailableCapability);
}

#[test]
fn generic_action_run_rejects_non_object_or_extra_input() {
    let root = temporary_directory("invalid").join("Central");
    initialize_central(&root).unwrap();

    let scalar = run_cli(
        &[
            "--json".to_owned(),
            "action".to_owned(),
            "run".to_owned(),
            "action.list".to_owned(),
            "[]".to_owned(),
        ],
        &environment(&root),
    );
    assert_eq!(scalar.result.status, ResultStatus::InvalidInput);
    assert!(scalar.output.contains("must be a JSON object"));

    let extra = run_cli(
        &[
            "--json".to_owned(),
            "action".to_owned(),
            "run".to_owned(),
            "action.list".to_owned(),
            "{}".to_owned(),
            "extra".to_owned(),
        ],
        &environment(&root),
    );
    assert_eq!(extra.result.status, ResultStatus::InvalidInput);
    assert!(extra.output.contains("at most one JSON object"));
}
