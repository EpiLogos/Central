#[cfg(target_os = "macos")]
mod primary_workstation_acceptance {
    use central_chezmoi_connector::ChezmoiConnector;
    use central_connector_sdk::{ConnectorRegistry, ReconciliationSourceReference};
    use central_ctrl::{
        create_core_action_registry, initialize_central, ActionExecutionContext, ResultStatus,
        RootOptions,
    };
    use central_homebrew_connector::HomebrewConnector;
    use central_macos_connectors::MacOsNativeConnector;
    use serde_json::json;
    use std::fs;
    use std::path::PathBuf;
    use std::process::Command;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temporary_directory(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "central-primary-workstation-{label}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn required_env(name: &str) -> String {
        std::env::var(name).unwrap_or_else(|_| panic!("{name} must be set for ignored primary-workstation acceptance"))
    }

    #[test]
    #[ignore = "requires the named primary macOS workstation, Homebrew, chezmoi, and an already-installed harmless formula"]
    fn real_primary_workstation_package_configuration_plan_apply_verify_is_harmless_and_stable() {
        let formula = required_env("CENTRAL_HOMEBREW_ACCEPTANCE_FORMULA");
        let brew = std::env::var_os("CENTRAL_BREW_EXECUTABLE")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("brew"));
        let chezmoi = std::env::var_os("CENTRAL_CHEZMOI_EXECUTABLE")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("chezmoi"));

        let installed = Command::new(&brew)
            .args(["list", "--formula", "--versions", formula.as_str()])
            .status()
            .expect("Homebrew must be runnable on the primary workstation");
        assert!(
            installed.success(),
            "CENTRAL_HOMEBREW_ACCEPTANCE_FORMULA must name an already-installed formula so acceptance does not install or remove personal software"
        );
        let chezmoi_available = Command::new(&chezmoi)
            .arg("--version")
            .status()
            .expect("chezmoi must be runnable on the primary workstation");
        assert!(chezmoi_available.success());

        let fixture = temporary_directory("package-config");
        let root = fixture.join("Central");
        let destination = fixture.join("destination");
        let source = fixture.join("source");
        fs::create_dir_all(&destination).unwrap();
        fs::create_dir_all(&source).unwrap();
        fs::write(
            source.join("central-acceptance.txt"),
            "Central primary-workstation chezmoi acceptance\n",
        )
        .unwrap();
        initialize_central(&root).unwrap();

        let target = destination.join("central-acceptance.txt");
        let declaration = json!({
            "schema": "central.machine",
            "version": 1,
            "role": "primary-workstation-acceptance",
            "capabilities": [],
            "requirements": {
                "packages": [{ "id": formula, "state": "present" }],
                "configurations": [{
                    "id": target.display().to_string(),
                    "state": "present",
                    "source": {
                        "kind": "chezmoi",
                        "reference": source.display().to_string()
                    }
                }],
                "services": []
            }
        });
        fs::write(
            root.join("Control/machines/primary-workstation-acceptance.json"),
            serde_json::to_string_pretty(&declaration).unwrap(),
        )
        .unwrap();

        let mut connectors = ConnectorRegistry::default();
        connectors
            .register(MacOsNativeConnector::with_host_tools(
                brew.clone(),
                destination.clone(),
            ))
            .unwrap();
        connectors
            .register(HomebrewConnector::with_executable(brew))
            .unwrap();
        connectors
            .register(ChezmoiConnector::with_paths(chezmoi, destination.clone()))
            .unwrap();

        let connector_context = central_connector_sdk::ConnectorContext {
            platform: "macos".to_owned(),
        };
        let root_options = RootOptions {
            explicit_root: Some(root.clone()),
            ..RootOptions::default()
        };
        let context = ActionExecutionContext {
            root_options: &root_options,
            connectors: &connectors,
            connector_context: &connector_context,
        };
        let actions = create_core_action_registry();
        let input = json!({ "role": "primary-workstation-acceptance" });

        let plan = actions.execute("machine.plan", &input, &context);
        assert_eq!(plan.status, ResultStatus::Success);
        let plan_data = plan.data.unwrap();
        let package = plan_data["entries"]
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["kind"] == "package")
            .unwrap();
        assert_eq!(package["status"], "satisfied");
        let configuration = plan_data["entries"]
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["kind"] == "configuration")
            .unwrap();
        assert_eq!(configuration["status"], "changeable");
        assert_eq!(configuration["port"], "ConfigurationManager");
        assert_eq!(configuration["connector"]["id"], "personal.chezmoi");

        let apply = actions.execute("machine.apply", &input, &context);
        assert_eq!(apply.status, ResultStatus::Success);
        let apply_data = apply.data.unwrap();
        assert_eq!(apply_data["outcome"], "complete");
        assert_eq!(apply_data["verification"]["satisfied"], true);
        assert_eq!(
            fs::read_to_string(&target).unwrap(),
            "Central primary-workstation chezmoi acceptance\n"
        );

        let verify = actions.execute("machine.verify", &input, &context);
        assert_eq!(verify.status, ResultStatus::Success);
        assert_eq!(verify.data.unwrap()["satisfied"], true);

        let repeated = actions.execute("machine.apply", &input, &context);
        assert_eq!(repeated.status, ResultStatus::Success);
        assert_eq!(
            repeated.data.unwrap()["operations"].as_array().unwrap().len(),
            0
        );

        let source_reference = ReconciliationSourceReference {
            kind: "chezmoi".to_owned(),
            reference: source.display().to_string(),
        };
        assert_eq!(source_reference.kind, "chezmoi");
        fs::remove_dir_all(fixture).unwrap();
    }
}

#[cfg(not(target_os = "macos"))]
#[test]
fn primary_workstation_acceptance_is_macos_only() {
    assert_ne!(std::env::consts::OS, "macos");
}
