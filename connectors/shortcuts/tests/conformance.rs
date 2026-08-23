#[cfg(unix)]
mod unix_tests {
    use central_connector_sdk::{
        run_automation_conformance, AutomationConformanceFixture, Connector, ConnectorContext,
        AUTOMATION_PORT,
    };
    use central_shortcuts_connector::{ShortcutsAutomationConnector, SHORTCUTS_CONNECTOR_ID};
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
            "central-shortcuts-{label}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn fixture_executable() -> PathBuf {
        let directory = temporary_directory("fixture");
        let executable = directory.join("shortcuts-fixture");
        fs::write(
            &executable,
            "#!/bin/sh\nif [ \"$1\" != \"run\" ]; then exit 20; fi\nif [ -z \"$2\" ]; then exit 21; fi\nprintf '%s\\n' \"$2\" > \"${CENTRAL_SHORTCUTS_LOG:?}\"\n",
        )
        .unwrap();
        let mut permissions = fs::metadata(&executable).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&executable, permissions).unwrap();
        executable
    }

    #[test]
    fn shared_conformance_exercises_the_shortcuts_provider_operation() {
        let executable = fixture_executable();
        let log = executable.parent().unwrap().join("invocation.log");
        std::env::set_var("CENTRAL_SHORTCUTS_LOG", &log);
        let connector = ShortcutsAutomationConnector::with_executable(executable);
        let report = run_automation_conformance(
            &connector,
            &AutomationConformanceFixture {
                platform: "macos".to_owned(),
                automation: "Central Fixture".to_owned(),
            },
        )
        .unwrap();
        assert_eq!(report.port_id, AUTOMATION_PORT.id);
        assert_eq!(report.connector.id, SHORTCUTS_CONNECTOR_ID);
        assert_eq!(fs::read_to_string(log).unwrap().trim(), "Central Fixture");
        std::env::remove_var("CENTRAL_SHORTCUTS_LOG");
    }

    #[test]
    fn connector_is_explicitly_ineligible_off_platform() {
        let connector = ShortcutsAutomationConnector::with_executable(fixture_executable());
        let probe = connector.probe(
            &AUTOMATION_PORT,
            &ConnectorContext {
                platform: "linux".to_owned(),
            },
        );
        assert!(!probe.available);
        assert!(probe.reason.unwrap().contains("does not support platform"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn real_macos_shortcuts_binary_is_probeable() {
        let connector = ShortcutsAutomationConnector::new();
        let probe = connector.probe(
            &AUTOMATION_PORT,
            &ConnectorContext {
                platform: "macos".to_owned(),
            },
        );
        assert!(probe.available, "{:?}", probe.reason);
        assert_eq!(connector.executable(), std::path::Path::new("/usr/bin/shortcuts"));
    }
}
