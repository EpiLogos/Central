use central_ctrl::action::{ActionAvailability, ActionDescriptor, ActionOutputDefinition};
use central_ctrl::{
    create_core_action_registry, initialize_central, inspect_central, resolve_central_root,
    ActionExecutionContext, ActionRegistry, CliEnvironment, ConnectorContext, ConnectorRegistry,
    MixedRootSignal, MutationClass, ResultStatus, RootOptions,
};
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn temporary_directory(label: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
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
        output: ActionOutputDefinition {
            output_type: "test".to_owned(),
        },
        mutation_class: MutationClass::ReadOnly,
        preview_supported: false,
        required_ports: Vec::new(),
        availability: ActionAvailability {
            available: true,
            reason: None,
        },
    }
}

fn execute(
    registry: &ActionRegistry,
    id: &str,
    root_options: &RootOptions,
) -> central_ctrl::ActionResult {
    let connectors = ConnectorRegistry::default();
    let connector_context = ConnectorContext {
        platform: "test".to_owned(),
    };
    let context = ActionExecutionContext {
        root_options,
        connectors: &connectors,
        connector_context: &connector_context,
    };
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
    })
    .unwrap();
    assert_eq!(root.path, explicit);

    let root = resolve_central_root(&RootOptions {
        explicit_root: None,
        configured_root: Some(configured.clone()),
        home: Some(home.clone()),
    })
    .unwrap();
    assert_eq!(root.path, configured);

    let root = resolve_central_root(&RootOptions {
        explicit_root: None,
        configured_root: None,
        home: Some(home.clone()),
    })
    .unwrap();
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
    assert_eq!(
        fs::read_dir(root.join("Control/agents/governance"))
            .unwrap()
            .count(),
        0
    );
    assert_eq!(
        fs::read_dir(root.join("Control/machines")).unwrap().count(),
        0
    );

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
    let options = RootOptions {
        explicit_root: Some(root.clone()),
        ..RootOptions::default()
    };

    let missing = execute(&registry, "central.doctor", &options);
    assert_eq!(missing.status, ResultStatus::InvalidCentralStructure);

    execute(&registry, "central.init", &options);
    assert_eq!(
        execute(&registry, "central.doctor", &options).status,
        ResultStatus::Success
    );

    let file_root = base.join("Central-file");
    fs::write(&file_root, "not a directory").unwrap();
    let file_options = RootOptions {
        explicit_root: Some(file_root),
        ..RootOptions::default()
    };
    assert_eq!(
        execute(&registry, "central.doctor", &file_options).status,
        ResultStatus::InvalidCentralStructure
    );
}

#[test]
fn doctor_diagnoses_a_personal_root_that_is_also_the_product_checkout() {
    let root = temporary_directory("mixed-root-checkout").join("Central");
    initialize_central(&root).unwrap();
    fs::write(root.join("Cargo.toml"), "[workspace]\n").unwrap();
    fs::create_dir_all(root.join("ctrl/src")).unwrap();
    fs::create_dir_all(root.join("crates/connector-sdk")).unwrap();

    let report = inspect_central(&root).unwrap();
    assert!(report.valid, "structure alone remains valid");
    assert!(report.mixed_root.detected);
    assert!(report
        .mixed_root
        .message
        .as_deref()
        .unwrap_or_default()
        .contains("personal root is also the Central product source checkout"));
    assert!(report
        .mixed_root
        .signals
        .contains(&MixedRootSignal::CargoManifest));
    assert!(report
        .mixed_root
        .signals
        .contains(&MixedRootSignal::CtrlSourceDirectory));
    assert!(report
        .mixed_root
        .signals
        .contains(&MixedRootSignal::ConnectorSdkCrate));
}

#[test]
fn doctor_does_not_flag_a_personal_root_with_one_generic_file() {
    let root = temporary_directory("mixed-root-plain").join("Central");
    initialize_central(&root).unwrap();
    fs::write(root.join("Cargo.toml"), "[workspace]\n").unwrap();

    let report = inspect_central(&root).unwrap();
    assert!(report.valid);
    assert!(!report.mixed_root.detected);
}

#[test]
fn doctor_flags_connectors_directory_with_the_product_remote() {
    let root = temporary_directory("mixed-root-remote").join("Central");
    initialize_central(&root).unwrap();
    fs::create_dir_all(root.join("connectors")).unwrap();
    fs::create_dir_all(root.join(".git")).unwrap();
    fs::write(
        root.join(".git/config"),
        "[remote \"origin\"]\n\turl = https://github.com/EpiLogos/Central\n",
    )
    .unwrap();

    let report = inspect_central(&root).unwrap();
    assert!(report.mixed_root.detected);
    assert!(report
        .mixed_root
        .signals
        .contains(&MixedRootSignal::ConnectorsDirectory));
    assert!(report
        .mixed_root
        .signals
        .contains(&MixedRootSignal::ProductRepositoryRemote));
}

#[test]
fn doctor_action_reports_mixed_root_without_failing_structure() {
    let root = temporary_directory("mixed-root-action").join("Central");
    initialize_central(&root).unwrap();
    fs::write(root.join("Cargo.toml"), "[workspace]\n").unwrap();
    fs::create_dir_all(root.join("ctrl")).unwrap();

    let registry = create_core_action_registry();
    let options = RootOptions {
        explicit_root: Some(root.clone()),
        ..RootOptions::default()
    };
    let result = execute(&registry, "central.doctor", &options);
    assert_eq!(result.status, ResultStatus::Success);
    let data = result.data.expect("doctor data");
    assert_eq!(data["mixed_root"]["detected"], json!(true));
    assert!(data["mixed_root"]["message"]
        .as_str()
        .unwrap_or_default()
        .contains("product source checkout"));
}

#[test]
fn registry_has_stable_ids_and_complete_descriptors() {
    let registry = create_core_action_registry();
    let ids = registry
        .list()
        .into_iter()
        .map(|action| action.id)
        .collect::<Vec<_>>();
    assert_eq!(
        ids,
        vec![
            "action.list",
            "central.doctor",
            "central.files.history",
            "central.files.list",
            "central.files.read",
            "central.files.recovery_preview",
            "central.files.restore",
            "central.files.write",
            "central.init",
            "central.recognize",
            "central.recover",
            "central.recovery.plan",
            "central.root",
            "central.wiki.read",
            "central.world",
            "central.world.project",
            "central.world.reproject.apply",
            "central.world.reproject.plan",
            "control.index",
            "control.open",
            "control.search",
            "control.skills.inspect",
            "control.skills.restore",
            "control.skills.retire",
            "machine.account",
            "machine.adopt-current",
            "machine.apply",
            "machine.declaration",
            "machine.inspect",
            "machine.plan",
            "machine.verify",
            "work.list",
            "work.open",
            "work.reveal",
            "work.search",
        ]
    );
    for id in ids {
        let action = registry.get(&id).unwrap();
        assert!(!action.title.is_empty());
        assert!(!action.description.is_empty());
        assert!(action.availability.available);
    }
}

fn panic_action(
    _registry: &ActionRegistry,
    _input: &serde_json::Value,
    _context: &ActionExecutionContext<'_>,
) -> central_ctrl::ActionResult {
    panic!("boom")
}

#[test]
fn registry_converts_unexpected_panics_to_structured_internal_failure() {
    let mut registry = ActionRegistry::default();
    registry
        .register(descriptor("test.fail"), panic_action)
        .unwrap();
    let root_options = RootOptions::default();
    let connectors = ConnectorRegistry::default();
    let connector_context = ConnectorContext {
        platform: "test".to_owned(),
    };
    let context = ActionExecutionContext {
        root_options: &root_options,
        connectors: &connectors,
        connector_context: &connector_context,
    };
    let result = registry.execute("test.fail", &json!({}), &context);
    assert_eq!(result.status, ResultStatus::InternalFailure);
    assert_eq!(result.error.unwrap().code, "internal_failure");
}

#[test]
fn action_list_has_human_and_structured_cli_renderings() {
    let environment = CliEnvironment {
        configured_root: None,
        home: Some(temporary_directory("cli-home")),
    };
    let human = central_ctrl::run_cli(&["actions".to_owned()], &environment);
    assert_eq!(human.exit_code, 0);
    assert!(human.output.contains("action.list\tList Actions"));
    assert!(human
        .output
        .contains("central.recovery.plan\tPlan Central recovery"));
    assert!(human.output.contains("central.world\tShow the world map"));
    assert!(human
        .output
        .contains("central.recover\tRecover Central machine state"));
    assert!(human
        .output
        .contains("machine.inspect\tInspect current machine"));
    assert!(human.output.contains("machine.plan\tPlan machine changes"));
    assert!(human.output.contains("machine.apply\tApply machine plan"));
    assert!(human
        .output
        .contains("machine.verify\tVerify machine declaration"));
    assert!(human
        .output
        .contains("machine.adopt-current\tAdopt current machine"));
    assert!(human
        .output
        .contains("machine.oi-suite-policy\tInspect machine O:I suite policy intent"));
    assert!(human.output.contains("work.open\tOpen Work item"));
    assert!(human.output.contains("work.reveal\tReveal Work item"));
    assert!(human
        .output
        .contains("projectcentral.inspect\tInspect ProjectCentral"));
    assert!(human
        .output
        .contains("projectcentral.doctor\tVerify ProjectCentral"));
    assert!(human
        .output
        .contains("projectcentral.init\tInitialize ProjectCentral"));
    assert!(human
        .output
        .contains("projectcentral.adopt.preview\tPreview Wiki adoption"));
    assert!(human
        .output
        .contains("projectcentral.adopt\tAdopt Wiki in place"));
    assert!(human
        .output
        .contains("projectcentral.migrate.preview\tPreview Wiki migration"));
    assert!(human
        .output
        .contains("projectcentral.migrate\tMigrate selected Wiki"));
    assert!(human
        .output
        .contains("projectcentral.ground.inspect\tInspect authored Project ground"));
    assert!(human
        .output
        .contains("projectcentral.ground.plan\tPlan authored Project ground"));
    assert!(human
        .output
        .contains("projectcentral.ground.apply\tApply accepted Project ground relation"));
    assert!(human
        .output
        .contains("projectcentral.change.horizon\tRead current Source Change Horizon"));
    assert!(human
        .output
        .contains("projectcentral.change.reconcile\tReconcile Project source revisions"));
    assert!(human
        .output
        .contains("projectcentral.change.ack\tAcknowledge Source Change cursor"));
    assert!(human
        .output
        .contains("projectcentral.now.inspect\tInspect Project NOW"));
    assert!(human
        .output
        .contains("projectcentral.now.init\tInitialize Project NOW"));
    assert!(human
        .output
        .contains("projectcentral.now.return\tWrite bounded Agent return"));
    assert!(human
        .output
        .contains("projectcentral.now.update\tUpdate NOW lifecycle"));
    assert!(human
        .output
        .contains("projectcentral.now.promote\tPromote NOW material"));
    assert!(human
        .output
        .contains("projectcentral.now.rollover\tClose DAY and roll NOW"));
    assert!(human
        .output
        .contains("projectcentral.flow.create\tCreate Project Flow"));
    assert!(human
        .output
        .contains("projectcentral.flow.write\tWrite Project Flow revision"));
    assert!(human
        .output
        .contains("projectcentral.flow.history\tRead Project Flow history"));
    assert!(human
        .output
        .contains("projectcentral.flow.now\tRead Project Flow NOW view"));
    assert!(human
        .output
        .contains("central.self.inspect\tInspect root self-description field"));
    assert!(human
        .output
        .contains("projectcentral.self.inspect\tInspect Project self-description field"));
    assert!(human.output.contains(
        "projectcentral.self.retain-tier\tRelate retained native source into Project tier"
    ));
    assert!(human
        .output
        .contains("agent-profile.list\tList Agent Profiles"));
    assert!(human
        .output
        .contains("agent-profile.read\tRead Agent Profile"));
    assert!(human
        .output
        .contains("agent-profile.save\tSave Agent Profile"));
    assert!(human
        .output
        .contains("agent-profile.remove\tRemove Agent Profile"));
    assert!(human
        .output
        .contains("agent-profile.propose\tPropose Agent Profile from Intent"));
    assert!(human
        .output
        .contains("control.skills.inspect\tInspect skill ground"));
    assert!(human
        .output
        .contains("control.skills.retire\tRetire a skill"));
    assert!(human
        .output
        .contains("control.skills.restore\tRestore a retired skill"));
    assert!(human
        .output
        .contains("central.remember\tRemember selection at Central root"));
    assert!(human
        .output
        .contains("projectcentral.remember\tRemember selection into Project"));

    let structured = central_ctrl::run_cli(
        &["--json".to_owned(), "action.list".to_owned()],
        &environment,
    );
    assert_eq!(structured.exit_code, 0);
    let value: serde_json::Value = serde_json::from_str(&structured.output).unwrap();
    assert_eq!(value["status"], "success");
    let actions = value["data"]["actions"].as_array().unwrap();
    let mut continuous = ActionRegistry::default();
    central_ctrl::continuous_work::register_actions(&mut continuous);
    assert_eq!(actions.len(), 129 + continuous.list().len());
    for descriptor in continuous.list() {
        let actual = actions
            .iter()
            .find(|action| action["id"] == descriptor.id)
            .unwrap_or_else(|| panic!("missing native continuous-work Action {}", descriptor.id));
        assert_eq!(*actual, serde_json::to_value(&descriptor).unwrap());
        assert!(human
            .output
            .contains(&format!("{}\t{}", descriptor.id, descriptor.title)));
    }
    let ids = actions
        .iter()
        .filter_map(|action| action["id"].as_str())
        .collect::<Vec<_>>();
    for id in [
        "central.agent-set.save",
        "central.agent-set.list",
        "central.agent-set.read",
        "central.agent-set.remove",
        "central.agent-set.resolve",
        "central.world-relations.save",
        "central.world-relations.list",
        "central.world-relations.read",
        "central.world-relations.remove",
        "central.world.effective-sources",
        "projectcentral.inspect",
        "projectcentral.doctor",
        "projectcentral.init",
        "projectcentral.adopt.preview",
        "projectcentral.adopt",
        "projectcentral.migrate.preview",
        "projectcentral.migrate",
        "projectcentral.ground.inspect",
        "projectcentral.ground.plan",
        "projectcentral.ground.apply",
        "projectcentral.change.horizon",
        "projectcentral.change.reconcile",
        "projectcentral.change.ack",
        "projectcentral.now.inspect",
        "projectcentral.now.init",
        "projectcentral.now.return",
        "projectcentral.now.update",
        "projectcentral.now.promote",
        "projectcentral.now.rollover",
        "projectcentral.flow.list",
        "projectcentral.flow.read",
        "projectcentral.flow.create",
        "projectcentral.flow.adopt",
        "projectcentral.flow.write",
        "projectcentral.flow.rename",
        "projectcentral.flow.lifecycle",
        "projectcentral.flow.history",
        "projectcentral.source.history",
        "projectcentral.source.compare",
        "projectcentral.source.recovery.preview",
        "projectcentral.source.read",
        "projectcentral.source.write",
        "central.self.inspect",
        "central.self.ensure",
        "central.self.source.create",
        "central.self.tier.relate",
        "central.self.ux.relate",
        "central.self.ex.relate",
        "central.self.resolve",
        "projectcentral.self.inspect",
        "projectcentral.self.ensure",
        "projectcentral.self.source.create",
        "projectcentral.self.tier.relate",
        "projectcentral.self.retain-tier",
        "projectcentral.self.ux.relate",
        "projectcentral.self.ex.relate",
        "projectcentral.self.resolve",
        "machine.oi-suite-policy",
        "machine.account",
        "agent-profile.list",
        "agent-profile.read",
        "agent-profile.save",
        "agent-profile.remove",
        "agent-profile.propose",
        "central.template.preview",
        "central.template.stamp",
        "control.engineering-ground.plan",
        "control.engineering-ground.render",
        "central.remember",
        "projectcentral.remember",
    ] {
        assert!(ids.contains(&id), "missing Action {id}");
    }
}

#[test]
fn structured_failures_distinguish_input_structure_and_internal_failure() {
    let environment = CliEnvironment {
        configured_root: None,
        home: Some(temporary_directory("failure-home")),
    };
    let invalid_input = central_ctrl::run_cli(
        &["--json".to_owned(), "no-such-command".to_owned()],
        &environment,
    );
    assert_eq!(invalid_input.exit_code, 2);
    assert_eq!(invalid_input.result.status, ResultStatus::InvalidInput);

    let missing_root = temporary_directory("missing").join("Central");
    let invalid_structure = central_ctrl::run_cli(
        &[
            "--json".to_owned(),
            "--root".to_owned(),
            missing_root.display().to_string(),
            "doctor".to_owned(),
        ],
        &environment,
    );
    assert_eq!(invalid_structure.exit_code, 3);
    assert_eq!(
        invalid_structure.result.status,
        ResultStatus::InvalidCentralStructure
    );
}

#[test]
fn binary_is_the_stable_development_entrypoint() {
    let root = temporary_directory("binary").join("Central");
    let binary = env!("CARGO_BIN_EXE_ctrl");
    let init = Command::new(binary)
        .args(["--json", "--root", root.to_str().unwrap(), "init"])
        .output()
        .unwrap();
    assert!(
        init.status.success(),
        "{}",
        String::from_utf8_lossy(&init.stderr)
    );
    let payload: serde_json::Value = serde_json::from_slice(&init.stdout).unwrap();
    assert_eq!(payload["action"], "central.init");
    assert!(Path::new(root.to_str().unwrap())
        .join("Control/user")
        .is_dir());
    assert!(Path::new(root.to_str().unwrap())
        .join("Control/agents/governance")
        .is_dir());
    assert!(Path::new(root.to_str().unwrap())
        .join("Control/agents/wiki/wiki.json")
        .is_file());
    assert!(!Path::new(root.to_str().unwrap())
        .join("Control/self")
        .exists());
}
