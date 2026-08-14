use central_ctrl::action::{ActionAvailability, ActionDescriptor, ActionOutputDefinition};
use central_ctrl::{
    create_core_action_registry, inspect_central, initialize_central, resolve_central_root, ActionRegistry,
    CliEnvironment, MutationClass, ResultStatus, RootOptions,
};
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn temporary_directory(label: &str) -> PathBuf {
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let path = std::env::temp_dir().join(format!("central-{label}-{}-{nonce}", std::process::id()));
    fs::create_dir_all(&path).unwrap();
    path
}

fn descriptor(id: &str) -> ActionDescriptor {
    ActionDescriptor {
        id: id.to_owned(),
        title: "Test".to_owned(),
        description: "Test Action".to_owned(),
        inputs: Vec::new(),
        output: ActionOutputDefinition { output_type: "test".to_owned() },
        mutation_class: MutationClass::ReadOnly,
        preview_supported: false,
        required_ports: Vec::new(),
        availability: ActionAvailability { available: true, reason: None },
    }
}

#[test]
fn root_discovery_prefers_explicit_then_configured_then_home_default() {
    let explicit = PathBuf::from("/explicit/Central");
    let configured = PathBuf::from("/configured/Central");
    let home = PathBuf::from("/home/person");
    let root = resolve_central_root(&RootOptions {
        explicit_root: Some(explicit.clone()),
        configured_root: Some(configured.clone()),
        home: Some(home.clone()),
    }).unwrap();
    assert_eq!(root.path, explicit);

    let root = resolve_central_root(&RootOptions { explicit_root: None, configured_root: Some(configured.clone()), home: Some(home.clone()) }).unwrap();
    assert_eq!(root.path, configured);

    let root = resolve_central_root(&RootOptions { explicit_root: None, configured_root: None, home: Some(home.clone()) }).unwrap();
    assert_eq!(root.path, home.join("Central"));
}

#[test]
fn initialization_creates_only_the_protocol_roots_and_is_repeatable() {
    let root = temporary_directory("init").join("Central");
    initialize_central(&root).unwrap();
    initialize_central(&root).unwrap();

    for relative in ["Control/user", "Control/agents", "Control/machines", ".central", "Work"] {
        assert!(root.join(relative).is_dir(), "missing {relative}");
    }
    for control in ["user", "agents", "machines"] {
        assert_eq!(fs::read_dir(root.join("Control").join(control)).unwrap().count(), 0);
    }
    assert!(inspect_central(&root).unwrap().valid);
}

#[test]
fn doctor_reports_missing_invalid_and_valid_structure() {
    let base = temporary_directory("doctor");
    let root = base.join("Central");
    let registry = create_core_action_registry();
    let options = RootOptions { explicit_root: Some(root.clone()), ..RootOptions::default() };

    let missing = registry.execute("central.doctor", &json!({}), &options);
    assert_eq!(missing.status, ResultStatus::InvalidCentralStructure);

    registry.execute("central.init", &json!({}), &options);
    assert_eq!(registry.execute("central.doctor", &json!({}), &options).status, ResultStatus::Success);

    let file_root = base.join("Central-file");
    fs::write(&file_root, "not a directory").unwrap();
    let file_options = RootOptions { explicit_root: Some(file_root), ..RootOptions::default() };
    assert_eq!(registry.execute("central.doctor", &json!({}), &file_options).status, ResultStatus::InvalidCentralStructure);
}

#[test]
fn registry_has_stable_ids_and_complete_descriptors() {
    let registry = create_core_action_registry();
    let ids = registry.list().into_iter().map(|action| action.id).collect::<Vec<_>>();
    assert_eq!(ids, vec!["action.list", "central.doctor", "central.init", "central.root"]);
    for id in ids {
        let action = registry.get(&id).unwrap();
        assert!(!action.title.is_empty());
        assert!(!action.description.is_empty());
        assert!(action.availability.available);
    }
}

fn panic_action(_registry: &ActionRegistry, _input: &serde_json::Value, _options: &RootOptions) -> central_ctrl::ActionResult {
    panic!("boom")
}

#[test]
fn registry_converts_unexpected_panics_to_structured_internal_failure() {
    let mut registry = ActionRegistry::default();
    registry.register(descriptor("test.fail"), panic_action).unwrap();
    let result = registry.execute("test.fail", &json!({}), &RootOptions::default());
    assert_eq!(result.status, ResultStatus::InternalFailure);
    assert_eq!(result.error.unwrap().code, "internal_failure");
}

#[test]
fn action_list_has_human_and_structured_cli_renderings() {
    let environment = CliEnvironment { configured_root: None, home: Some(temporary_directory("cli-home")) };
    let human = central_ctrl::run_cli(&["actions".to_owned()], &environment);
    assert_eq!(human.exit_code, 0);
    assert!(human.output.contains("action.list\tList Actions"));
    assert!(human.output.contains("central.doctor\tDiagnose Central"));

    let structured = central_ctrl::run_cli(&["--json".to_owned(), "action.list".to_owned()], &environment);
    assert_eq!(structured.exit_code, 0);
    let value: serde_json::Value = serde_json::from_str(&structured.output).unwrap();
    assert_eq!(value["status"], "success");
    assert!(value["data"]["actions"].as_array().unwrap().len() >= 4);
}

#[test]
fn structured_failures_distinguish_input_structure_and_internal_failure() {
    let environment = CliEnvironment { configured_root: None, home: Some(temporary_directory("failure-home")) };
    let invalid_input = central_ctrl::run_cli(&["--json".to_owned(), "no-such-command".to_owned()], &environment);
    assert_eq!(invalid_input.exit_code, 2);
    assert_eq!(invalid_input.result.status, ResultStatus::InvalidInput);

    let missing_root = temporary_directory("missing").join("Central");
    let invalid_structure = central_ctrl::run_cli(
        &["--json".to_owned(), "--root".to_owned(), missing_root.display().to_string(), "doctor".to_owned()],
        &environment,
    );
    assert_eq!(invalid_structure.exit_code, 3);
    assert_eq!(invalid_structure.result.status, ResultStatus::InvalidCentralStructure);
}

#[test]
fn binary_is_the_stable_development_entrypoint() {
    let root = temporary_directory("binary").join("Central");
    let binary = env!("CARGO_BIN_EXE_ctrl");
    let init = Command::new(binary).args(["--json", "--root", root.to_str().unwrap(), "init"]).output().unwrap();
    assert!(init.status.success(), "{}", String::from_utf8_lossy(&init.stderr));
    let payload: serde_json::Value = serde_json::from_slice(&init.stdout).unwrap();
    assert_eq!(payload["action"], "central.init");
    assert!(Path::new(root.to_str().unwrap()).join("Control/user").is_dir());
}
