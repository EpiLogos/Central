use central_ctrl::{
    create_core_action_registry, initialize_central, run_cli_with_runtime, ActionExecutionContext,
    CliEnvironment, ConnectorContext, ConnectorRegistry, MachineInspectionOutput, MutationClass,
    NullTerminalSurface, ResultStatus, RootOptions, StaticMachineInspectorConnector,
    MACHINE_ADOPTION_SCHEMA, MACHINE_DECLARATION_SCHEMA, MACHINE_DECLARATION_VERSION,
    MACHINE_INSPECTOR_PORT,
};
use serde_json::{json, Value};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn temporary_directory(label: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "central-adopt-{label}-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir_all(&path).unwrap();
    path
}

fn observation() -> MachineInspectionOutput {
    MachineInspectionOutput {
        platform: "test-os".to_owned(),
        architecture: "test-arch".to_owned(),
        capabilities: vec![
            "package-management".to_owned(),
            "portable-configuration".to_owned(),
        ],
        packages: Vec::new(),
        configurations: Vec::new(),
        services: Vec::new(),
    }
}

fn connectors() -> ConnectorRegistry {
    let mut connectors = ConnectorRegistry::default();
    connectors
        .register(StaticMachineInspectorConnector::new(observation()))
        .unwrap();
    connectors
}

fn execute(
    root: &PathBuf,
    registry_connectors: &ConnectorRegistry,
    action: &str,
    input: &Value,
) -> central_ctrl::ActionResult {
    let connector_context = ConnectorContext {
        platform: "test".to_owned(),
    };
    let root_options = RootOptions {
        explicit_root: Some(root.clone()),
        ..RootOptions::default()
    };
    let context = ActionExecutionContext {
        root_options: &root_options,
        connectors: registry_connectors,
        connector_context: &connector_context,
    };
    create_core_action_registry().execute(action, input, &context)
}

fn adopt(
    root: &PathBuf,
    connectors: &ConnectorRegistry,
    input: &Value,
) -> central_ctrl::ActionResult {
    execute(root, connectors, "machine.adopt-current", input)
}

fn read_declaration(root: &PathBuf) -> Value {
    let result = execute(
        root,
        &connectors(),
        "machine.declaration",
        &json!({ "role": "current" }),
    );
    assert_eq!(result.status, ResultStatus::Success);
    result.data.unwrap()
}

fn declaration_bytes(root: &PathBuf) -> String {
    fs::read_to_string(root.join("Control/machines/current.json")).unwrap()
}

#[test]
fn first_adoption_creates_declaration_with_workcell_binding_and_seeded_capabilities() {
    let root = temporary_directory("first").join("Central");
    initialize_central(&root).unwrap();

    let result = adopt(&root, &connectors(), &json!({}));
    assert_eq!(result.status, ResultStatus::Success);
    let data = result.data.unwrap();
    assert_eq!(data["schema"], MACHINE_ADOPTION_SCHEMA);
    assert_eq!(data["outcome"], "created");
    assert_eq!(data["role"], "current");
    assert_eq!(data["workcell_ref"], "workcell:local");
    assert_eq!(data["source"]["path"], "Control/machines/current.json");
    assert_eq!(data["source"]["source_class"], "authored");
    assert_eq!(data["declaration"]["schema"], MACHINE_DECLARATION_SCHEMA);
    assert_eq!(data["declaration"]["version"], MACHINE_DECLARATION_VERSION);
    assert_eq!(data["declaration"]["role"], "current");
    // Observed capabilities are seeded as the initial accepted intent.
    assert_eq!(
        data["declaration"]["capabilities"],
        json!(["package-management", "portable-configuration"])
    );
    assert_eq!(
        data["declaration"]["bindings"],
        json!([{ "kind": "workcell", "reference": "workcell:local" }])
    );
    // Observation evidence remains in the result, separate from intent.
    assert_eq!(data["observed"]["observation"]["platform"], "test-os");
    assert_eq!(data["observed"]["observation"]["architecture"], "test-arch");
    assert_eq!(data["observed"]["source"]["source_class"], "observed");

    // The file on disk is exactly what the canonical reader accepts.
    let read_back = read_declaration(&root);
    assert_eq!(read_back["declaration"], data["declaration"]);

    // An explicit role and Workcell reference address a different declaration.
    let other = adopt(
        &root,
        &connectors(),
        &json!({ "role": "dev-laptop", "workcell_ref": "workcell:remote" }),
    );
    assert_eq!(other.status, ResultStatus::Success);
    let other_data = other.data.unwrap();
    assert_eq!(other_data["outcome"], "created");
    assert_eq!(
        other_data["source"]["path"],
        "Control/machines/dev-laptop.json"
    );
    assert_eq!(
        other_data["declaration"]["bindings"],
        json!([{ "kind": "workcell", "reference": "workcell:remote" }])
    );
    fs::remove_dir_all(root.parent().unwrap()).unwrap();
}

#[test]
fn a_machine_role_directory_coexists_with_the_role_declaration_file() {
    let root = temporary_directory("coexist").join("Central");
    initialize_central(&root).unwrap();
    // The live ground carries machine-scoped authored material below
    // Control/machines/current/ (adopted skills, machine configuration).
    let skill_dir = root.join("Control/machines/current/skills/brandkit");
    fs::create_dir_all(&skill_dir).unwrap();
    fs::write(skill_dir.join("SKILL.md"), "body\n").unwrap();

    let result = adopt(&root, &connectors(), &json!({}));
    assert_eq!(result.status, ResultStatus::Success);
    assert_eq!(
        result.data.unwrap()["source"]["path"],
        "Control/machines/current.json"
    );
    // The directory ground is untouched and both forms coexist.
    assert!(root.join("Control/machines/current.json").is_file());
    assert_eq!(
        fs::read_to_string(skill_dir.join("SKILL.md")).unwrap(),
        "body\n"
    );
    assert!(read_declaration(&root)["declaration"]["role"] == "current");
    fs::remove_dir_all(root.parent().unwrap()).unwrap();
}

#[test]
fn repeated_adoption_is_a_no_op_success_with_identical_ground() {
    let root = temporary_directory("repeat").join("Central");
    initialize_central(&root).unwrap();

    let first = adopt(&root, &connectors(), &json!({}));
    assert_eq!(first.status, ResultStatus::Success);
    assert_eq!(first.data.unwrap()["outcome"], "created");
    let after_first = declaration_bytes(&root);

    let second = adopt(
        &root,
        &connectors(),
        &json!({ "role": "current", "workcell_ref": "workcell:local" }),
    );
    assert_eq!(second.status, ResultStatus::Success);
    let data = second.data.unwrap();
    assert_eq!(data["outcome"], "unchanged");
    assert_eq!(data["workcell_ref"], "workcell:local");
    assert_eq!(declaration_bytes(&root), after_first);
    fs::remove_dir_all(root.parent().unwrap()).unwrap();
}

#[test]
fn a_different_existing_workcell_binding_is_surfaced_as_a_conflict() {
    let root = temporary_directory("conflict").join("Central");
    initialize_central(&root).unwrap();
    assert_eq!(
        adopt(&root, &connectors(), &json!({})).status,
        ResultStatus::Success
    );
    let before = declaration_bytes(&root);

    let conflict = adopt(
        &root,
        &connectors(),
        &json!({ "workcell_ref": "workcell:remote" }),
    );
    assert_eq!(conflict.status, ResultStatus::InvalidInput);
    let error = conflict.error.unwrap();
    let details = error.details.unwrap();
    assert_eq!(details["code"], "workcell_binding_conflict");
    assert_eq!(details["role"], "current");
    assert_eq!(details["requested_workcell_ref"], "workcell:remote");
    assert_eq!(details["existing_workcell_refs"], json!(["workcell:local"]));
    assert_eq!(details["path"], "Control/machines/current.json");
    assert!(error.message.contains("workcell:local"));
    // The authored ground is left exactly as it was.
    assert_eq!(declaration_bytes(&root), before);
    fs::remove_dir_all(root.parent().unwrap()).unwrap();
}

#[test]
fn adoption_round_trips_through_declaration_plan_and_verify() {
    let root = temporary_directory("roundtrip").join("Central");
    initialize_central(&root).unwrap();
    assert_eq!(
        adopt(&root, &connectors(), &json!({})).status,
        ResultStatus::Success
    );

    let declaration = read_declaration(&root);
    assert_eq!(
        declaration["declaration"]["bindings"],
        json!([{ "kind": "workcell", "reference": "workcell:local" }])
    );

    let plan = execute(
        &root,
        &connectors(),
        "machine.plan",
        &json!({ "role": "current" }),
    );
    assert_eq!(plan.status, ResultStatus::Success);
    let plan_data = plan.data.unwrap();
    assert_eq!(
        plan_data["authored"]["declaration"]["bindings"],
        json!([{ "kind": "workcell", "reference": "workcell:local" }])
    );
    assert_eq!(plan_data["summary"]["satisfied"], 2);
    assert_eq!(plan_data["summary"]["missing"], 0);
    assert_eq!(plan_data["summary"]["unsupported"], 0);

    let verify = execute(
        &root,
        &connectors(),
        "machine.verify",
        &json!({ "role": "current" }),
    );
    assert_eq!(verify.status, ResultStatus::Success);
    assert_eq!(verify.data.unwrap()["satisfied"], true);
    fs::remove_dir_all(root.parent().unwrap()).unwrap();
}

fn legacy_declaration() -> Value {
    json!({
        "schema": MACHINE_DECLARATION_SCHEMA,
        "version": MACHINE_DECLARATION_VERSION,
        "role": "current",
        "capabilities": ["interactive-command-surface"],
        "requirements": {
            "packages": [{ "id": "git", "state": "present" }],
            "configurations": [],
            "services": []
        },
        "owner_note": "authored metadata adoption must preserve"
    })
}

#[test]
fn existing_declarations_without_bindings_stay_compatible_and_gain_the_binding() {
    let root = temporary_directory("legacy").join("Central");
    initialize_central(&root).unwrap();
    fs::write(
        root.join("Control/machines/current.json"),
        serde_json::to_string_pretty(&legacy_declaration()).unwrap(),
    )
    .unwrap();

    // The pre-bindings form still parses identically through the reader.
    let before = read_declaration(&root);
    assert!(before["declaration"].get("bindings").is_none());
    assert_eq!(
        before["declaration"]["capabilities"],
        json!(["interactive-command-surface"])
    );

    let result = adopt(
        &root,
        &connectors(),
        &json!({ "workcell_ref": "workcell:local" }),
    );
    assert_eq!(result.status, ResultStatus::Success);
    let data = result.data.unwrap();
    assert_eq!(data["outcome"], "bound");
    assert_eq!(
        data["declaration"]["bindings"],
        json!([{ "kind": "workcell", "reference": "workcell:local" }])
    );
    // Authored intent and unknown authored fields survive the binding write.
    assert_eq!(
        data["declaration"]["capabilities"],
        json!(["interactive-command-surface"])
    );
    assert_eq!(
        data["declaration"]["requirements"]["packages"][0]["id"],
        "git"
    );
    let disk: Value = serde_json::from_str(&declaration_bytes(&root)).unwrap();
    assert_eq!(
        disk["owner_note"],
        "authored metadata adoption must preserve"
    );
    assert_eq!(
        disk["bindings"],
        json!([{ "kind": "workcell", "reference": "workcell:local" }])
    );
    fs::remove_dir_all(root.parent().unwrap()).unwrap();
}

#[test]
fn adoption_requires_a_machine_inspector_connector() {
    let root = temporary_directory("no-inspector").join("Central");
    initialize_central(&root).unwrap();
    let empty = ConnectorRegistry::default();

    let result = adopt(&root, &empty, &json!({}));
    assert_eq!(result.status, ResultStatus::UnavailableCapability);
    assert!(!root.join("Control/machines/current.json").exists());
    fs::remove_dir_all(root.parent().unwrap()).unwrap();
}

#[test]
fn invalid_roles_and_missing_machine_ground_refuse_without_writing() {
    let root = temporary_directory("invalid").join("Central");
    initialize_central(&root).unwrap();

    let traversal = adopt(&root, &connectors(), &json!({ "role": "../user" }));
    assert_eq!(traversal.status, ResultStatus::InvalidInput);
    assert_eq!(
        traversal.error.unwrap().details.unwrap()["code"],
        "invalid_role"
    );

    let uninitialised = temporary_directory("bare");
    let missing_root = adopt(&uninitialised, &connectors(), &json!({}));
    assert_eq!(missing_root.status, ResultStatus::InvalidCentralStructure);
    assert!(missing_root
        .error
        .unwrap()
        .message
        .contains("Central machine source root is missing"));
    fs::remove_dir_all(root.parent().unwrap()).unwrap();
    fs::remove_dir_all(&uninitialised).unwrap();
}

#[test]
fn the_action_descriptor_declares_its_mutation_class_and_required_port() {
    let descriptor = create_core_action_registry()
        .get("machine.adopt-current")
        .unwrap()
        .clone();
    assert_eq!(descriptor.mutation_class, MutationClass::LocallyMutating);
    assert!(!descriptor.preview_supported);
    assert_eq!(descriptor.required_ports, vec![MACHINE_INSPECTOR_PORT.id]);
    let role = descriptor
        .inputs
        .iter()
        .find(|input| input.name == "role")
        .unwrap();
    let workcell_ref = descriptor
        .inputs
        .iter()
        .find(|input| input.name == "workcell_ref")
        .unwrap();
    assert!(!role.required);
    assert!(!workcell_ref.required);
}

#[test]
fn cli_adopts_the_current_machine_in_structured_and_human_forms() {
    let root = temporary_directory("cli").join("Central");
    initialize_central(&root).unwrap();
    let environment = CliEnvironment {
        configured_root: None,
        home: None,
    };
    let mut surface = NullTerminalSurface;

    let structured = run_cli_with_runtime(
        &[
            "--json".to_owned(),
            "--root".to_owned(),
            root.display().to_string(),
            "machine".to_owned(),
            "adopt-current".to_owned(),
        ],
        &environment,
        &mut surface,
        &connectors(),
        &ConnectorContext {
            platform: "test".to_owned(),
        },
    );
    assert_eq!(structured.exit_code, 0);
    let value: Value = serde_json::from_str(&structured.output).unwrap();
    assert_eq!(value["action"], "machine.adopt-current");
    assert_eq!(value["data"]["outcome"], "created");

    let human = run_cli_with_runtime(
        &[
            "--root".to_owned(),
            root.display().to_string(),
            "machine.adopt-current".to_owned(),
        ],
        &environment,
        &mut surface,
        &connectors(),
        &ConnectorContext {
            platform: "test".to_owned(),
        },
    );
    assert_eq!(human.exit_code, 0);
    assert!(human.output.contains("Machine adoption: unchanged"));
    assert!(human.output.contains("Workcell binding: workcell:local"));
    assert!(human
        .output
        .contains("Source: Control/machines/current.json [authored]"));
    assert!(human.output.contains("Observed host: test-os/test-arch"));

    // The human declaration surface renders the binding.
    let declaration = run_cli_with_runtime(
        &[
            "--root".to_owned(),
            root.display().to_string(),
            "machine".to_owned(),
            "declaration".to_owned(),
            "current".to_owned(),
        ],
        &environment,
        &mut surface,
        &connectors(),
        &ConnectorContext {
            platform: "test".to_owned(),
        },
    );
    assert_eq!(declaration.exit_code, 0);
    assert!(declaration.output.contains("Bindings:"));
    assert!(declaration.output.contains("- workcell: workcell:local"));
    fs::remove_dir_all(root.parent().unwrap()).unwrap();
}
