#[cfg(unix)]
mod unix_tests {
    use central_connector_sdk::{ConnectorContext, ConnectorRegistry};
    use central_reference_connectors::FilesystemWorkConnector;
    use central_shortcuts_connector::ShortcutsAutomationConnector;
    use central_ctrl::{
        create_core_action_registry, initialize_central, run_cli_with_runtime, CliEnvironment,
        NullTerminalSurface,
    };
    use central_macos_host::{create_macos_action_registry, run_macos_cli_with_runtime};
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temporary_directory(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "central-macos-host-{label}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn shortcut_fixture() -> (PathBuf, PathBuf) {
        let directory = temporary_directory("shortcut");
        let executable = directory.join("shortcuts-fixture");
        let log = directory.join("shortcut.log");
        fs::write(
            &executable,
            format!(
                "#!/bin/sh\nif [ \"$1\" != \"run\" ]; then exit 20; fi\nprintf '%s\\n' \"$2\" > '{}'\n",
                log.display()
            ),
        )
        .unwrap();
        let mut permissions = fs::metadata(&executable).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&executable, permissions).unwrap();
        (executable, log)
    }

    fn connectors_with_shortcuts(executable: PathBuf) -> ConnectorRegistry {
        let mut connectors = ConnectorRegistry::default();
        connectors.register(FilesystemWorkConnector::new()).unwrap();
        connectors
            .register(ShortcutsAutomationConnector::with_executable(executable))
            .unwrap();
        connectors
    }

    #[test]
    fn macos_action_registry_extends_but_does_not_mutate_core_identity() {
        let core = create_core_action_registry();
        assert_eq!(core.list().len(), 15);
        assert!(core.get("automation.run").is_none());

        let macos = create_macos_action_registry();
        assert_eq!(macos.list().len(), 16);
        let automation = macos.get("automation.run").unwrap();
        assert_eq!(automation.required_ports, vec!["Automation"]);
        assert_eq!(automation.mutation_class.as_str(), "externally-mutating");
    }

    #[test]
    fn ctrl_macos_action_run_calls_shortcuts_through_the_automation_port() {
        let (executable, log) = shortcut_fixture();
        let connectors = connectors_with_shortcuts(executable);
        let context = ConnectorContext {
            platform: "macos".to_owned(),
        };
        let environment = CliEnvironment {
            configured_root: None,
            home: Some(temporary_directory("home")),
        };
        let mut surface = NullTerminalSurface;
        let execution = run_macos_cli_with_runtime(
            &[
                "--json".to_owned(),
                "action".to_owned(),
                "run".to_owned(),
                "automation.run".to_owned(),
                r#"{"automation":"Central Fixture"}"#.to_owned(),
            ],
            &environment,
            &mut surface,
            &connectors,
            &context,
        );
        assert_eq!(execution.exit_code, 0, "{}", execution.output);
        let value: serde_json::Value = serde_json::from_str(&execution.output).unwrap();
        assert_eq!(value["action"], "automation.run");
        assert_eq!(value["data"]["port"], "Automation");
        assert_eq!(value["data"]["connector"]["id"], "personal.macos-shortcuts");
        assert_eq!(fs::read_to_string(log).unwrap().trim(), "Central Fixture");
    }

    #[test]
    fn generic_action_run_and_existing_cli_project_the_same_canonical_action_result() {
        let root = temporary_directory("equivalence").join("Central");
        initialize_central(&root).unwrap();
        fs::create_dir(root.join("Work/project-alpha")).unwrap();
        let mut connectors = ConnectorRegistry::default();
        connectors.register(FilesystemWorkConnector::new()).unwrap();
        let context = ConnectorContext {
            platform: "macos".to_owned(),
        };
        let environment = CliEnvironment {
            configured_root: None,
            home: None,
        };

        let mut generic_surface = NullTerminalSurface;
        let generic = run_macos_cli_with_runtime(
            &[
                "--json".to_owned(),
                "--root".to_owned(),
                root.display().to_string(),
                "action".to_owned(),
                "run".to_owned(),
                "work.search".to_owned(),
                r#"{"query":"alpha"}"#.to_owned(),
            ],
            &environment,
            &mut generic_surface,
            &connectors,
            &context,
        );

        let mut direct_surface = NullTerminalSurface;
        let direct = run_cli_with_runtime(
            &[
                "--json".to_owned(),
                "--root".to_owned(),
                root.display().to_string(),
                "work".to_owned(),
                "search".to_owned(),
                "alpha".to_owned(),
            ],
            &environment,
            &mut direct_surface,
            &connectors,
            &context,
        );

        assert_eq!(generic.exit_code, 0);
        assert_eq!(direct.exit_code, 0);
        let generic_value: serde_json::Value = serde_json::from_str(&generic.output).unwrap();
        let direct_value: serde_json::Value = serde_json::from_str(&direct.output).unwrap();
        assert_eq!(generic_value, direct_value);
    }

    #[test]
    fn removing_personal_surfaces_leaves_existing_cli_behavior_unchanged() {
        let root = temporary_directory("delegation").join("Central");
        initialize_central(&root).unwrap();
        fs::create_dir(root.join("Work/project-a")).unwrap();
        let mut connectors = ConnectorRegistry::default();
        connectors.register(FilesystemWorkConnector::new()).unwrap();
        let context = ConnectorContext {
            platform: "macos".to_owned(),
        };
        let environment = CliEnvironment {
            configured_root: None,
            home: None,
        };
        let args = vec![
            "--json".to_owned(),
            "--root".to_owned(),
            root.display().to_string(),
            "work".to_owned(),
            "list".to_owned(),
        ];

        let mut host_surface = NullTerminalSurface;
        let host = run_macos_cli_with_runtime(
            &args,
            &environment,
            &mut host_surface,
            &connectors,
            &context,
        );
        let mut core_surface = NullTerminalSurface;
        let core = run_cli_with_runtime(
            &args,
            &environment,
            &mut core_surface,
            &connectors,
            &context,
        );
        assert_eq!(host.result, core.result);
        assert_eq!(host.output, core.output);
    }
}
