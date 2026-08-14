use central_ctrl::{
    create_core_action_registry, initialize_central, run_cli, ActionExecutionContext, CliEnvironment,
    ConnectorContext, ConnectorRegistry, MutationClass, ResultStatus, RootOptions,
    MACHINE_DECLARATION_SCHEMA, MACHINE_DECLARATION_VERSION,
};
use serde_json::json;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn temporary_directory(label: &str) -> PathBuf {
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let path = std::env::temp_dir().join(format!("central-machine-{label}-{}-{nonce}", std::process::id()));
    fs::create_dir_all(&path).unwrap();
    path
}

fn write_role(root: &PathBuf, role: &str, declaration: serde_json::Value) {
    fs::write(
        root.join("Control/machines").join(format!("{role}.json")),
        serde_json::to_string_pretty(&declaration).unwrap(),
    ).unwrap();
}

fn execute(root: &PathBuf, role: &str) -> central_ctrl::ActionResult {
    let registry = create_core_action_registry();
    let connectors = ConnectorRegistry::default();
    let connector_context = ConnectorContext { platform: "test".to_owned() };
    let root_options = RootOptions { explicit_root: Some(root.clone()), ..RootOptions::default() };
    let context = ActionExecutionContext { root_options: &root_options, connectors: &connectors, connector_context: &connector_context };
    registry.execute("machine.declaration", &json!({ "role": role }), &context)
}

fn workstation() -> serde_json::Value {
    json!({
        "schema": MACHINE_DECLARATION_SCHEMA,
        "version": MACHINE_DECLARATION_VERSION,
        "role": "primary-workstation",
        "capabilities": [
            "interactive-command-surface",
            "project-entry",
            "native-automation",
            "package-management",
            "portable-configuration"
        ],
        "requirements": {
            "packages": [
                {
                    "id": "git",
                    "state": "present",
                    "source": {
                        "kind": "file",
                        "reference": "Control/machines/packages/workstation.list"
                    }
                }
            ],
            "configurations": [
                {
                    "id": "shell-profile",
                    "state": "present",
                    "source": {
                        "kind": "control",
                        "reference": "Control/machines/configuration/shell-profile"
                    }
                }
            ],
            "services": []
        }
    })
}

fn server() -> serde_json::Value {
    json!({
        "schema": MACHINE_DECLARATION_SCHEMA,
        "version": MACHINE_DECLARATION_VERSION,
        "role": "home-server",
        "capabilities": ["remote-shell", "package-management", "portable-configuration"],
        "requirements": {
            "packages": [{ "id": "git", "state": "present" }],
            "configurations": [{
                "id": "remote-access-policy",
                "state": "present",
                "source": {
                    "kind": "control",
                    "reference": "Control/machines/configuration/remote-access"
                }
            }],
            "services": [{ "id": "ssh", "running": true, "enabled": true }]
        }
    })
}

#[test]
fn representative_workstation_declaration_is_versioned_authored_and_provider_neutral() {
    let root = temporary_directory("workstation").join("Central");
    initialize_central(&root).unwrap();
    write_role(&root, "primary-workstation", workstation());

    let result = execute(&root, "primary-workstation");
    assert_eq!(result.status, ResultStatus::Success);
    let data = result.data.unwrap();
    assert_eq!(data["declaration"]["schema"], MACHINE_DECLARATION_SCHEMA);
    assert_eq!(data["declaration"]["version"], MACHINE_DECLARATION_VERSION);
    assert_eq!(data["declaration"]["role"], "primary-workstation");
    assert_eq!(data["declaration"]["requirements"]["packages"][0]["id"], "git");
    assert_eq!(data["declaration"]["requirements"]["configurations"][0]["id"], "shell-profile");
    assert_eq!(data["source"]["source_class"], "authored");
    assert_eq!(data["source"]["path"], "Control/machines/primary-workstation.json");
    assert!(data["declaration"].get("provider").is_none());

    let descriptor = create_core_action_registry().get("machine.declaration").unwrap().clone();
    assert_eq!(descriptor.mutation_class, MutationClass::ReadOnly);
    assert!(descriptor.required_ports.is_empty());
}

#[test]
fn representative_server_declaration_can_require_service_running_and_enablement() {
    let root = temporary_directory("server").join("Central");
    initialize_central(&root).unwrap();
    write_role(&root, "home-server", server());
    let result = execute(&root, "home-server");
    assert_eq!(result.status, ResultStatus::Success);
    let data = result.data.unwrap();
    assert_eq!(data["declaration"]["role"], "home-server");
    assert_eq!(data["declaration"]["requirements"]["services"][0]["running"], true);
    assert_eq!(data["declaration"]["requirements"]["services"][0]["enabled"], true);
}

#[test]
fn cli_reads_the_same_declaration_in_structured_and_human_forms() {
    let root = temporary_directory("cli").join("Central");
    initialize_central(&root).unwrap();
    write_role(&root, "home-server", server());
    let environment = CliEnvironment { configured_root: None, home: None };

    let structured = run_cli(&[
        "--json".to_owned(), "--root".to_owned(), root.display().to_string(),
        "machine".to_owned(), "declaration".to_owned(), "home-server".to_owned(),
    ], &environment);
    assert_eq!(structured.exit_code, 0);
    let value: serde_json::Value = serde_json::from_str(&structured.output).unwrap();
    assert_eq!(value["action"], "machine.declaration");
    assert_eq!(value["data"]["declaration"]["schema"], MACHINE_DECLARATION_SCHEMA);

    let human = run_cli(&[
        "--root".to_owned(), root.display().to_string(),
        "machine.declaration".to_owned(), "home-server".to_owned(),
    ], &environment);
    assert_eq!(human.exit_code, 0);
    assert!(human.output.contains("Machine role: home-server"));
    assert!(human.output.contains("Declaration: central.machine v1"));
    assert!(human.output.contains("remote-shell"));
    assert!(human.output.contains("remote-access-policy: present"));
    assert!(human.output.contains("ssh: running=true, enabled=true"));
}

#[test]
fn unsupported_schema_and_version_have_precise_diagnostics() {
    let root = temporary_directory("version").join("Central");
    initialize_central(&root).unwrap();
    let mut declaration = workstation();
    declaration["schema"] = json!("other.machine");
    write_role(&root, "primary-workstation", declaration);
    let schema = execute(&root, "primary-workstation");
    assert_eq!(schema.status, ResultStatus::InvalidInput);
    let details = schema.error.unwrap().details.unwrap();
    assert_eq!(details["code"], "unsupported_schema");
    assert_eq!(details["field"], "schema");

    let mut declaration = workstation();
    declaration["version"] = json!(2);
    write_role(&root, "primary-workstation", declaration);
    let version = execute(&root, "primary-workstation");
    let details = version.error.unwrap().details.unwrap();
    assert_eq!(details["code"], "unsupported_version");
    assert_eq!(details["field"], "version");
}

#[test]
fn malformed_and_semantically_invalid_declarations_report_source_and_field() {
    let root = temporary_directory("invalid").join("Central");
    initialize_central(&root).unwrap();
    fs::write(root.join("Control/machines/home-server.json"), "{not-json").unwrap();
    let malformed = execute(&root, "home-server");
    let details = malformed.error.unwrap().details.unwrap();
    assert_eq!(details["code"], "invalid_json");
    assert_eq!(details["path"], "Control/machines/home-server.json");

    let mut declaration = server();
    declaration["requirements"]["services"][0] = json!({ "id": "ssh" });
    write_role(&root, "home-server", declaration);
    let invalid = execute(&root, "home-server");
    let details = invalid.error.unwrap().details.unwrap();
    assert_eq!(details["code"], "invalid_service_requirement");
    assert_eq!(details["field"], "requirements.services[0]");
}

#[test]
fn role_binding_rejects_traversal_mismatch_and_missing_declarations() {
    let root = temporary_directory("roles").join("Central");
    initialize_central(&root).unwrap();

    let traversal = execute(&root, "../user");
    assert_eq!(traversal.error.unwrap().details.unwrap()["code"], "invalid_role");

    let mut declaration = server();
    declaration["role"] = json!("other-server");
    write_role(&root, "home-server", declaration);
    let mismatch = execute(&root, "home-server");
    assert_eq!(mismatch.error.unwrap().details.unwrap()["code"], "role_mismatch");

    let missing = execute(&root, "portable-laptop");
    let details = missing.error.unwrap().details.unwrap();
    assert_eq!(details["code"], "missing_declaration");
    assert_eq!(details["path"], "Control/machines/portable-laptop.json");
}

#[test]
fn direct_authored_edits_are_visible_immediately_without_generated_machine_state() {
    let root = temporary_directory("edit").join("Central");
    initialize_central(&root).unwrap();
    let mut declaration = workstation();
    declaration["capabilities"] = json!(["project-entry"]);
    write_role(&root, "primary-workstation", declaration);
    assert_eq!(
        execute(&root, "primary-workstation").data.unwrap()["declaration"]["capabilities"].as_array().unwrap().len(),
        1
    );

    let mut declaration = workstation();
    declaration["capabilities"] = json!(["project-entry", "native-automation"]);
    write_role(&root, "primary-workstation", declaration);
    assert_eq!(
        execute(&root, "primary-workstation").data.unwrap()["declaration"]["capabilities"].as_array().unwrap().len(),
        2
    );
    assert_eq!(fs::read_dir(root.join(".central")).unwrap().count(), 0);
}
