use central_ctrl::action::{ActionAvailability, ActionDescriptor, ActionOutputDefinition};
use central_ctrl::{
    create_core_action_registry, inspect_central, initialize_central, resolve_central_root,
    ActionExecutionContext, ActionRegistry, CliEnvironment, ConnectorContext, ConnectorRegistry,
    MutationClass, ResultStatus, RootOptions,
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

fn execute(registry: &ActionRegistry, id: &str, root_options: &RootOptions) -> central_ctrl::ActionResult {
    let connectors = ConnectorRegistry::default();
    let connector_context = ConnectorContext { platform: "test".to_owned() };
    let context = ActionExecutionContext { root_options, connectors: &connectors, connector_context: &connector_context };
    registry.execute(id, &json!({}), &context)
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
fn initialization_creates_recursive_control_roots_and_root_wiki_and_is_repeatable() {
    let root = temporary_directory("init").join("Central");
    initialize_central(&root).unwrap();
    initialize_central(&root).unwrap();

    for relative in [
        "Control/user",
        "Control/agents/governance",
        "Control/agents/wiki",
        "Control/machines",
        ".central",
        "Work",
    ] {
        assert!(root.join(relative).is_dir(), "missing {relative}");
    }
    assert_eq!(fs::read_dir(root.join("Control/user")).unwrap().count(), 0);
    assert_eq!(fs::read_dir(root.join("Control/agents/governance")).unwrap().count(), 0);
    assert_eq!(fs::read_dir(root.join("Control/machines")).unwrap().count(), 0);

    let root_wiki = root.join("Control/agents/wiki/wiki.json");
    assert!(root_wiki.is_file());
    let wiki: serde_json::Value = serde_json::from_slice(&fs::read(root_wiki).unwrap()).unwrap();
    assert_eq!(wiki["objects"][0]["profile"], "okf-wiki/v1");
    assert_eq!(wiki["objects"][0]["object"], "space");
    assert_eq!(wiki["objects"][0]["ref"], "central:wiki:root");
    assert!(inspect_central(&root).unwrap().valid);
}

#[test]
fn doctor_reports_missing_invalid_and_valid_structure() {
    let base = temporary_directory("doctor");
    let root = base.join("Central");
    let registry = create_core_action_registry();
    let options = RootOptions { explicit_root: Some(root.clone()), ..RootOptions::default() };

    let missing = execute(&registry, "central.doctor", &options);
    assert_eq!(missing.status, ResultStatus::InvalidCentralStructure);

    execute(&registry, "central.init", &options);
    assert_eq!(execute(&registry, "central.doctor", &options).status, ResultStatus::Success);

    let file_root = base.join("Central-file");
    fs::write(&file_root, "not a directory").unwrap();
    let file_options = RootOptions { explicit_root: Some(file_root), ..RootOptions::default() };
    assert_eq!(execute(&registry, "central.doctor", &file_options).status, ResultStatus::InvalidCentralStructure);
}

#[test]
fn registry_has_stable_ids_and_complete_descriptors() {
    let registry = create_core_action_registry();
    let ids = registry.list().into_iter().map(|action| action.id).collect::<Vec<_>>();
    assert_eq!(ids, vec![
        "action.list",
        "central.doctor",
        "central.init",
        "central.recover",
        "central.recovery.plan",
        "central.root",
        "control.open",
        "control.search",
        "machine.apply",
        "machine.declaration",
        "machine.inspect",
        "machine.plan",
        "machine.verify",
        "work.list",
        "work.open",
        "work.reveal",
        "work.search",
    ]);
    for id in ids {
        let action = registry.get(&id).unwrap();
        assert!(!action.title.is_empty());
        assert!(!action.description.is_empty());
        assert!(action.availability.available);
    }
}

fn panic_action(_registry: &ActionRegistry, _input: &serde_json::Value, _context: &ActionExecutionContext<'_>) -> central_ctrl::ActionResult {
    panic!("boom")
}

#[test]
fn registry_converts_unexpected_panics_to_structured_internal_failure() {
    let mut registry = ActionRegistry::default();
    registry.register(descriptor("test.fail"), panic_action).unwrap();
    let root_options = RootOptions::default();
    let connectors = ConnectorRegistry::default();
    let connector_context = ConnectorContext { platform: "test".to_owned() };
    let context = ActionExecutionContext { root_options: &root_options, connectors: &connectors, connector_context: &connector_context };
    let result = registry.execute("test.fail", &json!({}), &context);
    assert_eq!(result.status, ResultStatus::InternalFailure);
    assert_eq!(result.error.unwrap().code, "internal_failure");
}

#[test]
fn action_list_has_human_and_structured_cli_renderings() {
    let environment = CliEnvironment { configured_root: None, home: Some(temporary_directory("cli-home")) };
    let human = central_ctrl::run_cli(&["actions".to_owned()], &environment);
    assert_eq!(human.exit_code, 0);
    assert!(human.output.contains("action.list\tList Actions"));
    assert!(human.output.contains("central.recovery.plan\tPlan Central recovery"));
    assert!(human.output.contains("central.recover\tRecover Central machine state"));
    assert!(human.output.contains("machine.inspect\tInspect current machine"));
    assert!(human.output.contains("machine.plan\tPlan machine changes"));
    assert!(human.output.contains("machine.apply\tApply machine plan"));
    assert!(human.output.contains("machine.verify\tVerify machine declaration"));
    assert!(human.output.contains("work.open\tOpen Work item"));
    assert!(human.output.contains("work.reveal\tReveal Work item"));
    assert!(human.output.contains("projectcentral.inspect\tInspect ProjectCentral"));
    assert!(human.output.contains("projectcentral.doctor\tVerify ProjectCentral"));
    assert!(human.output.contains("projectcentral.init\tInitialize ProjectCentral"));
    assert!(human.output.contains("projectcentral.adopt.preview\tPreview Wiki adoption"));
    assert!(human.output.contains("projectcentral.adopt\tAdopt Wiki in place"));
    assert!(human.output.contains("projectcentral.migrate.preview\tPreview Wiki migration"));
    assert!(human.output.contains("projectcentral.migrate\tMigrate selected Wiki"));

    let structured = central_ctrl::run_cli(&["--json".to_owned(), "action.list".to_owned()], &environment);
    assert_eq!(structured.exit_code, 0);
    let value: serde_json::Value = serde_json::from_str(&structured.output).unwrap();
    assert_eq!(value["status"], "success");
    let actions = value["data"]["actions"].as_array().unwrap();
    assert_eq!(actions.len(), 24);
    let ids = actions.iter().filter_map(|action| action["id"].as_str()).collect::<Vec<_>>();
    for id in [
        "projectcentral.inspect",
        "projectcentral.doctor",
        "projectcentral.init",
        "projectcentral.adopt.preview",
        "projectcentral.adopt",
        "projectcentral.migrate.preview",
        "projectcentral.migrate",
    ] {
        assert!(ids.contains(&id), "missing ProjectCentral Action {id}");
    }
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
    assert!(Path::new(root.to_str().unwrap()).join("Control/agents/governance").is_dir());
    assert!(Path::new(root.to_str().unwrap()).join("Control/agents/wiki/wiki.json").is_file());
}
