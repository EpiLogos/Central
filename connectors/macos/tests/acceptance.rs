#[cfg(target_os = "macos")]
mod macos_acceptance {
    use central_connector_sdk::{
        run_machine_inspector_conformance, run_native_open_conformance,
        run_native_reveal_conformance, run_tag_store_conformance, Connector, ConnectorContext,
        ConnectorRegistry, MachineInspectorConformanceFixture, NativeTargetConformanceFixture,
        TagStoreConformanceFixture, MACHINE_INSPECTOR_PORT, NATIVE_OPEN_PORT,
        NATIVE_REVEAL_PORT, TAG_STORE_PORT,
    };
    use central_ctrl::{
        create_core_action_registry, initialize_central, ActionExecutionContext, ResultStatus,
        RootOptions,
    };
    use central_macos_connectors::MacOsNativeConnector;
    use central_reference_connectors::FilesystemWorkConnector;
    use serde_json::json;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temporary_directory(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "central-macos-{label}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn macos_registry() -> ConnectorRegistry {
        let mut connectors = ConnectorRegistry::default();
        connectors.register(FilesystemWorkConnector::new()).unwrap();
        connectors.register(MacOsNativeConnector::new()).unwrap();
        connectors
    }

    #[test]
    fn manifest_and_capability_probes_are_real_macos_eligible() {
        let connector = MacOsNativeConnector::new();
        assert_eq!(connector.manifest().mutation_scope, "externally-mutating");
        let context = ConnectorContext { platform: "macos".to_owned() };
        for port in [
            NATIVE_OPEN_PORT,
            NATIVE_REVEAL_PORT,
            TAG_STORE_PORT,
            MACHINE_INSPECTOR_PORT,
        ] {
            let probe = connector.probe(&port, &context);
            assert!(
                probe.available,
                "{} probe was unavailable: {:?}",
                port.id, probe.reason
            );
        }
    }

    #[test]
    fn native_open_and_reveal_pass_shared_conformance_on_macos() {
        let connector = MacOsNativeConnector::new();
        let directory = temporary_directory("native");
        let file = directory.join("reveal-me.txt");
        fs::write(&file, "central").unwrap();

        let open = run_native_open_conformance(
            &connector,
            &NativeTargetConformanceFixture {
                target: directory.clone(),
                platform: "macos".to_owned(),
            },
        )
        .unwrap();
        assert_eq!(open.port_id, NATIVE_OPEN_PORT.id);

        let reveal = run_native_reveal_conformance(
            &connector,
            &NativeTargetConformanceFixture {
                target: file,
                platform: "macos".to_owned(),
            },
        )
        .unwrap();
        assert_eq!(reveal.port_id, NATIVE_REVEAL_PORT.id);
    }

    #[test]
    fn finder_tags_round_trip_through_the_public_tag_store_contract() {
        let connector = MacOsNativeConnector::new();
        let directory = temporary_directory("tags");
        let file = directory.join("tag-me.txt");
        fs::write(&file, "central").unwrap();

        let report = run_tag_store_conformance(
            &connector,
            &TagStoreConformanceFixture {
                target: file,
                platform: "macos".to_owned(),
            },
        )
        .unwrap();
        assert_eq!(report.port_id, TAG_STORE_PORT.id);
    }

    #[test]
    fn machine_observation_passes_the_authoritative_shared_conformance_on_macos() {
        let connector = MacOsNativeConnector::new();
        let report = run_machine_inspector_conformance(
            &connector,
            &MachineInspectorConformanceFixture {
                platform: "macos".to_owned(),
                expected: None,
            },
        )
        .unwrap();
        assert_eq!(report.port_id, MACHINE_INSPECTOR_PORT.id);
    }

    #[test]
    fn canonical_work_actions_invoke_the_macos_connector_through_native_ports() {
        let root = temporary_directory("actions").join("Central");
        initialize_central(&root).unwrap();
        let project = root.join("Work").join("native-project");
        fs::create_dir(&project).unwrap();

        let connectors = macos_registry();
        let connector_context = ConnectorContext { platform: "macos".to_owned() };
        let root_options = RootOptions {
            explicit_root: Some(root),
            ..RootOptions::default()
        };
        let context = ActionExecutionContext {
            root_options: &root_options,
            connectors: &connectors,
            connector_context: &connector_context,
        };
        let actions = create_core_action_registry();

        let open = actions.execute("work.open", &json!({ "query": "native-project" }), &context);
        assert_eq!(open.status, ResultStatus::Success);
        assert_eq!(
            open.data.as_ref().unwrap()["native"]["diagnostics"]["selected_connector"]["id"],
            "personal.macos-native"
        );
        assert_eq!(open.data.as_ref().unwrap()["native"]["port"], NATIVE_OPEN_PORT.id);

        let reveal = actions.execute("work.reveal", &json!({ "query": "native-project" }), &context);
        assert_eq!(reveal.status, ResultStatus::Success);
        assert_eq!(
            reveal.data.as_ref().unwrap()["native"]["diagnostics"]["selected_connector"]["id"],
            "personal.macos-native"
        );
        assert_eq!(reveal.data.as_ref().unwrap()["native"]["port"], NATIVE_REVEAL_PORT.id);
    }

    #[test]
    fn machine_inspect_and_plan_use_the_macos_inspector_without_mixing_authored_and_observed_state() {
        let root = temporary_directory("machine-plan").join("Central");
        initialize_central(&root).unwrap();
        let declaration = json!({
            "schema": "central.machine",
            "version": 1,
            "role": "mac-workstation",
            "capabilities": ["NativeOpen", "TagStore"],
            "requirements": {
                "packages": [],
                "configurations": [],
                "services": []
            }
        });
        fs::write(
            root.join("Control/machines/mac-workstation.json"),
            serde_json::to_string_pretty(&declaration).unwrap(),
        )
        .unwrap();

        let connectors = macos_registry();
        let connector_context = ConnectorContext { platform: "macos".to_owned() };
        let root_options = RootOptions {
            explicit_root: Some(root),
            ..RootOptions::default()
        };
        let context = ActionExecutionContext {
            root_options: &root_options,
            connectors: &connectors,
            connector_context: &connector_context,
        };
        let actions = create_core_action_registry();

        let inspect = actions.execute("machine.inspect", &json!({}), &context);
        assert_eq!(inspect.status, ResultStatus::Success);
        let inspection = inspect.data.unwrap();
        assert_eq!(inspection["observation"]["platform"], "macos");
        assert_eq!(inspection["source"]["source_class"], "observed");
        assert_eq!(inspection["source"]["connector"]["id"], "personal.macos-native");

        let plan = actions.execute("machine.plan", &json!({ "role": "mac-workstation" }), &context);
        assert_eq!(plan.status, ResultStatus::Success);
        let data = plan.data.unwrap();
        assert_eq!(data["summary"]["satisfied"], 2);
        assert_eq!(data["summary"]["missing"], 0);
        assert_eq!(data["summary"]["changeable"], 0);
        assert_eq!(data["summary"]["unsupported"], 0);
        assert_eq!(data["authored"]["source"]["source_class"], "authored");
        assert_eq!(data["observed"]["source"]["source_class"], "observed");
        assert_eq!(data["observed"]["source"]["connector"]["id"], "personal.macos-native");
    }
}

#[cfg(not(target_os = "macos"))]
#[test]
fn macos_connector_is_explicitly_ineligible_off_platform() {
    use central_connector_sdk::{Connector, ConnectorContext, NATIVE_OPEN_PORT};
    use central_macos_connectors::MacOsNativeConnector;

    let connector = MacOsNativeConnector::new();
    let probe = connector.probe(
        &NATIVE_OPEN_PORT,
        &ConnectorContext { platform: std::env::consts::OS.to_owned() },
    );
    assert!(!probe.available);
    assert!(probe.reason.unwrap().contains("does not support platform"));
}
