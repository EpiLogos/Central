use central_ctrl::{
    create_core_action_registry, initialize_central, ActionExecutionContext, ConnectorContext,
    ConnectorRegistry, FilesystemWorkConnector, ResultStatus, RootOptions, StaticWorkConnector,
    WorkItem, WORK_DISCOVERY_PORT,
};
use serde_json::json;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn temporary_directory(label: &str) -> PathBuf {
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let path = std::env::temp_dir().join(format!("central-{label}-{}-{nonce}", std::process::id()));
    fs::create_dir_all(&path).unwrap();
    path
}

fn registered(first_filesystem: bool) -> ConnectorRegistry {
    let mut registry = ConnectorRegistry::default();
    if first_filesystem {
        registry.register(FilesystemWorkConnector::new()).unwrap();
        registry.register(StaticWorkConnector::new(Vec::new())).unwrap();
    } else {
        registry.register(StaticWorkConnector::new(Vec::new())).unwrap();
        registry.register(FilesystemWorkConnector::new()).unwrap();
    }
    registry
}

#[test]
fn two_valid_reference_connectors_resolve_stably_independent_of_registration_order() {
    let context = ConnectorContext { platform: "linux".to_owned() };
    let first = registered(true);
    let second = registered(false);
    let a = first.resolve(&WORK_DISCOVERY_PORT, &context);
    let b = second.resolve(&WORK_DISCOVERY_PORT, &context);

    assert_eq!(a.diagnostics.eligible.iter().map(|item| item.id.as_str()).collect::<Vec<_>>(), vec![
        "reference.work-filesystem",
        "reference.work-static",
    ]);
    assert_eq!(a.diagnostics.selected_connector.as_ref().unwrap().id, "reference.work-filesystem");
    assert_eq!(b.diagnostics.selected_connector.as_ref().unwrap().id, "reference.work-filesystem");
}

#[test]
fn work_list_action_depends_on_port_and_reports_selected_connector() {
    let root = temporary_directory("work-list").join("Central");
    initialize_central(&root).unwrap();
    fs::create_dir(root.join("Work").join("zeta")).unwrap();
    fs::create_dir(root.join("Work").join("alpha")).unwrap();
    fs::write(root.join("Work").join("ordinary-file.txt"), "not a Work directory").unwrap();

    let registry = create_core_action_registry();
    assert_eq!(registry.get("work.list").unwrap().required_ports, vec![WORK_DISCOVERY_PORT.id]);
    let connectors = registered(false);
    let connector_context = ConnectorContext { platform: "linux".to_owned() };
    let root_options = RootOptions { explicit_root: Some(root.clone()), ..RootOptions::default() };
    let context = ActionExecutionContext { root_options: &root_options, connectors: &connectors, connector_context: &connector_context };
    let result = registry.execute("work.list", &json!({}), &context);

    assert_eq!(result.status, ResultStatus::Success);
    let data = result.data.unwrap();
    assert_eq!(data["diagnostics"]["selected_connector"]["id"], "reference.work-filesystem");
    assert_eq!(data["items"].as_array().unwrap().iter().map(|item| item["name"].as_str().unwrap()).collect::<Vec<_>>(), vec!["alpha", "zeta"]);
}

#[test]
fn work_list_returns_structured_unavailable_capability_when_no_connector_is_eligible() {
    let root = temporary_directory("unavailable").join("Central");
    initialize_central(&root).unwrap();
    let registry = create_core_action_registry();
    let connectors = ConnectorRegistry::default();
    let connector_context = ConnectorContext { platform: "linux".to_owned() };
    let root_options = RootOptions { explicit_root: Some(root), ..RootOptions::default() };
    let context = ActionExecutionContext { root_options: &root_options, connectors: &connectors, connector_context: &connector_context };
    let result = registry.execute("work.list", &json!({}), &context);

    assert_eq!(result.status, ResultStatus::UnavailableCapability);
    let error = result.error.unwrap();
    assert_eq!(error.code, "unavailable_capability");
    assert_eq!(error.details.unwrap()["port"], WORK_DISCOVERY_PORT.id);
}

#[test]
fn cli_exposes_the_same_action_and_connector_diagnostics() {
    let root = temporary_directory("work-cli").join("Central");
    initialize_central(&root).unwrap();
    fs::create_dir(root.join("Work").join("project-a")).unwrap();
    let environment = central_ctrl::CliEnvironment { configured_root: None, home: None };
    let execution = central_ctrl::run_cli(
        &["--json".to_owned(), "--root".to_owned(), root.display().to_string(), "work.list".to_owned()],
        &environment,
    );
    assert_eq!(execution.exit_code, 0);
    let value: serde_json::Value = serde_json::from_str(&execution.output).unwrap();
    assert_eq!(value["action"], "work.list");
    assert_eq!(value["data"]["diagnostics"]["selected_connector"]["id"], "reference.work-filesystem");
    assert_eq!(value["data"]["items"][0]["name"], "project-a");
}

#[test]
fn static_reference_connector_uses_the_same_typed_port_contract() {
    let item = WorkItem { name: "fixture".to_owned(), path: PathBuf::from("/fixture") };
    let mut connectors = ConnectorRegistry::default();
    connectors.register(StaticWorkConnector::new(vec![item])).unwrap();
    let context = ConnectorContext { platform: "linux".to_owned() };
    let resolution = connectors.resolve(&WORK_DISCOVERY_PORT, &context);
    let implementation = resolution.connector.unwrap().work_discovery().unwrap();
    let output = implementation.list(&central_ctrl::WorkDiscoveryInput { work_root: PathBuf::from("/ignored") }).unwrap();
    assert_eq!(output.items[0].name, "fixture");
}
