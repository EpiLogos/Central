use central_ctrl::CliEnvironment;
use serde_json::Value;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn temporary_root() -> PathBuf {
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    std::env::temp_dir().join(format!("central-ground-actions-{}-{nonce}", std::process::id()))
}

fn run(root: &PathBuf, action: &str, input: &str) -> central_ctrl::CliExecution {
    central_ctrl::run_cli(
        &[
            "--json".to_owned(),
            "--root".to_owned(),
            root.display().to_string(),
            "action".to_owned(),
            "run".to_owned(),
            action.to_owned(),
            input.to_owned(),
        ],
        &CliEnvironment::default(),
    )
}

#[test]
fn actions_establish_one_small_human_ground_without_generated_documents() {
    let root = temporary_root();
    let init = central_ctrl::run_cli(
        &[
            "--json".to_owned(),
            "--root".to_owned(),
            root.display().to_string(),
            "init".to_owned(),
        ],
        &CliEnvironment::default(),
    );
    assert_eq!(init.exit_code, 0, "{}", init.output);

    let project = root.join("Work/example");
    fs::create_dir_all(&project).unwrap();
    let pc = run(
        &root,
        "projectcentral.init",
        r#"{"project":"example","project_id":"example/project"}"#,
    );
    assert_eq!(pc.exit_code, 0, "{}", pc.output);
    assert!(project.join("ProjectCentral/user").is_dir());
    assert_eq!(fs::read_dir(project.join("ProjectCentral/user")).unwrap().count(), 0);
    assert!(!project.join("ProjectCentral/README.md").exists());

    let empty = run(&root, "projectcentral.ground.inspect", r#"{"project":"example"}"#);
    assert_eq!(empty.exit_code, 0, "{}", empty.output);
    let empty_value: Value = serde_json::from_str(&empty.output).unwrap();
    assert_eq!(empty_value["data"]["status"], "empty");

    fs::write(project.join("ProjectCentral/user/note.md"), "A small human-authored Project note.\n").unwrap();
    let unresolved = run(&root, "projectcentral.ground.inspect", r#"{"project":"example"}"#);
    let unresolved_value: Value = serde_json::from_str(&unresolved.output).unwrap();
    assert_eq!(unresolved_value["data"]["status"], "partial");
    assert_eq!(unresolved_value["data"]["recognised_sources"][0]["provenance"], "unresolved");

    let apply = run(
        &root,
        "projectcentral.ground.apply",
        r#"{"project":"example","source":"ProjectCentral/user/note.md","provenance":"human-authored","standing":"authored-human-position","treatment":"projectcentral-user","roles":"purpose","acceptance":"human-accepted"}"#,
    );
    assert_eq!(apply.exit_code, 0, "{}", apply.output);
    let apply_value: Value = serde_json::from_str(&apply.output).unwrap();
    assert_eq!(apply_value["data"]["ground_status"], "established");
    assert_eq!(apply_value["data"]["source_bytes_mutated"], false);
    assert_eq!(apply_value["data"]["source_path_mutated"], false);
    assert_eq!(fs::read_to_string(project.join("ProjectCentral/user/note.md")).unwrap(), "A small human-authored Project note.\n");
    assert!(project.join("ProjectCentral/agents/wiki/wiki.json").is_file());
    assert!(project.join("ProjectCentral/relations/source-relations.json").is_file());

    let _ = fs::remove_dir_all(root);
}

#[test]
fn apply_rejects_missing_explicit_human_acceptance() {
    let root = temporary_root();
    let init = central_ctrl::run_cli(
        &[
            "--json".to_owned(),
            "--root".to_owned(),
            root.display().to_string(),
            "init".to_owned(),
        ],
        &CliEnvironment::default(),
    );
    assert_eq!(init.exit_code, 0, "{}", init.output);
    let project = root.join("Work/example");
    fs::create_dir_all(&project).unwrap();
    assert_eq!(
        run(
            &root,
            "projectcentral.init",
            r#"{"project":"example","project_id":"example/project"}"#,
        )
        .exit_code,
        0
    );
    fs::write(project.join("ProjectCentral/user/note.md"), "draft\n").unwrap();

    let apply = run(
        &root,
        "projectcentral.ground.apply",
        r#"{"project":"example","source":"ProjectCentral/user/note.md","provenance":"human-authored","standing":"authored-human-position","treatment":"projectcentral-user","roles":"purpose","acceptance":"agent-assumed"}"#,
    );
    assert_eq!(apply.exit_code, 2);
    assert!(!project.join("ProjectCentral/relations/source-relations.json").exists());
    let _ = fs::remove_dir_all(root);
}
