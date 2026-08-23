#[cfg(unix)]
mod unix_tests {
    use central_chezmoi_connector::ChezmoiConnector;
    use central_connector_sdk::{
        run_scoped_machine_inspector_conformance, ConnectorRegistry, MachineInspectionInput,
        MachineInspectionOutput, ObservedConfiguration, ObservedPackage,
        ScopedMachineInspectorConformanceFixture,
    };
    use central_ctrl::{
        create_core_action_registry, initialize_central, ActionExecutionContext, ResultStatus,
        RootOptions,
    };
    use central_homebrew_connector::HomebrewConnector;
    use central_macos_connectors::MacOsNativeConnector;
    use serde_json::json;
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
            "central-macos-package-config-{label}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn make_executable(path: &PathBuf, body: &str) {
        fs::write(path, body).unwrap();
        let mut permissions = fs::metadata(path).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions).unwrap();
    }

    fn fake_brew(directory: &PathBuf) -> PathBuf {
        let executable = directory.join("brew");
        make_executable(
            &executable,
            r#"#!/bin/sh
set -eu
STATE="$0.state"
if [ "${1:-}" = "--version" ]; then echo "Homebrew fixture"; exit 0; fi
if [ "${1:-}" = "list" ]; then
  id="${4:-}"
  if [ -f "$STATE" ] && [ "$(cat "$STATE")" = "$id" ]; then echo "$id 1.0"; exit 0; fi
  exit 1
fi
if [ "${1:-}" = "install" ]; then printf '%s' "${3:-}" > "$STATE"; exit 0; fi
if [ "${1:-}" = "uninstall" ]; then rm -f "$STATE"; exit 0; fi
exit 64
"#,
        );
        executable
    }

    fn fake_chezmoi(directory: &PathBuf) -> PathBuf {
        let executable = directory.join("chezmoi");
        make_executable(
            &executable,
            r#"#!/bin/sh
set -eu
if [ "${1:-}" = "--version" ]; then echo "chezmoi fixture"; exit 0; fi
SOURCE=""
DEST=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    --source) SOURCE="$2"; shift 2 ;;
    --destination) DEST="$2"; shift 2 ;;
    --no-tty|--force) shift ;;
    diff|apply) CMD="$1"; shift; break ;;
    *) echo "unexpected argument: $1" >&2; exit 64 ;;
  esac
done
TARGET="${1:-}"
REL="${TARGET#"$DEST"/}"
SRC="$SOURCE/$REL"
DST="$DEST/$REL"
if [ "$CMD" = "diff" ]; then
  if [ ! -f "$DST" ] || ! cmp -s "$SRC" "$DST"; then echo "changed: $REL"; fi
  exit 0
fi
if [ "$CMD" = "apply" ]; then mkdir -p "$(dirname "$DST")"; cp "$SRC" "$DST"; exit 0; fi
exit 64
"#,
        );
        executable
    }

    fn write_role(root: &PathBuf, source: &PathBuf) {
        let declaration = json!({
            "schema": "central.machine",
            "version": 1,
            "role": "primary-workstation",
            "capabilities": [],
            "requirements": {
                "packages": [{ "id": "central-fixture", "state": "present" }],
                "configurations": [{
                    "id": "fixture.txt",
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
            root.join("Control/machines/primary-workstation.json"),
            serde_json::to_string_pretty(&declaration).unwrap(),
        )
        .unwrap();
    }

    fn registry(brew: PathBuf, chezmoi: PathBuf, home: PathBuf, include_managers: bool) -> ConnectorRegistry {
        let mut connectors = ConnectorRegistry::default();
        connectors
            .register(MacOsNativeConnector::with_host_tools(brew.clone(), home.clone()))
            .unwrap();
        if include_managers {
            connectors.register(HomebrewConnector::with_executable(brew)).unwrap();
            connectors
                .register(ChezmoiConnector::with_paths(chezmoi, home))
                .unwrap();
        }
        connectors
    }

    fn execute(
        root: &PathBuf,
        connectors: &ConnectorRegistry,
        action: &str,
    ) -> central_ctrl::ActionResult {
        let connector_context = central_connector_sdk::ConnectorContext {
            platform: "macos".to_owned(),
        };
        let root_options = RootOptions {
            explicit_root: Some(root.clone()),
            ..RootOptions::default()
        };
        let context = ActionExecutionContext {
            root_options: &root_options,
            connectors,
            connector_context: &connector_context,
        };
        create_core_action_registry().execute(
            action,
            &json!({ "role": "primary-workstation" }),
            &context,
        )
    }

    #[test]
    fn scoped_macos_machine_inspection_observes_requested_package_and_configuration_ids() {
        let fixture = temporary_directory("scoped");
        let home = fixture.join("home");
        fs::create_dir_all(&home).unwrap();
        fs::write(home.join("fixture.txt"), "present\n").unwrap();
        let brew = fake_brew(&fixture);
        fs::write(PathBuf::from(format!("{}.state", brew.display())), "central-fixture").unwrap();
        let connector = MacOsNativeConnector::with_host_tools(brew, home);
        let input = MachineInspectionInput {
            package_ids: vec!["central-fixture".to_owned()],
            configuration_ids: vec!["fixture.txt".to_owned()],
            service_ids: Vec::new(),
        };
        let expected = MachineInspectionOutput {
            platform: std::env::consts::OS.to_owned(),
            architecture: std::env::consts::ARCH.to_owned(),
            capabilities: vec![
                "MachineInspector".to_owned(),
                "NativeOpen".to_owned(),
                "NativeReveal".to_owned(),
                "TagStore".to_owned(),
            ],
            packages: vec![ObservedPackage {
                id: "central-fixture".to_owned(),
                present: true,
            }],
            configurations: vec![ObservedConfiguration {
                id: "fixture.txt".to_owned(),
                present: true,
            }],
            services: Vec::new(),
        };
        let report = run_scoped_machine_inspector_conformance(
            &connector,
            &ScopedMachineInspectorConformanceFixture {
                platform: "macos".to_owned(),
                input,
                expected,
            },
        )
        .unwrap();
        assert!(report
            .checks
            .iter()
            .any(|check| check == "requested-observations"));
    }

    #[test]
    fn canonical_machine_actions_plan_apply_verify_and_repeat_through_homebrew_and_chezmoi() {
        let fixture = temporary_directory("canonical");
        let root = fixture.join("Central");
        let home = fixture.join("home");
        let source = fixture.join("source");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(&source).unwrap();
        fs::write(source.join("fixture.txt"), "authored macOS configuration\n").unwrap();
        initialize_central(&root).unwrap();
        write_role(&root, &source);

        let brew = fake_brew(&fixture);
        let chezmoi = fake_chezmoi(&fixture);
        let connectors = registry(brew, chezmoi, home.clone(), true);

        let plan = execute(&root, &connectors, "machine.plan");
        assert_eq!(plan.status, ResultStatus::Success);
        let data = plan.data.unwrap();
        assert_eq!(data["summary"]["changeable"], 2);
        assert_eq!(data["summary"]["missing"], 0);
        let entries = data["entries"].as_array().unwrap();
        let package = entries.iter().find(|entry| entry["kind"] == "package").unwrap();
        assert_eq!(package["port"], "PackageManager");
        assert_eq!(package["connector"]["id"], "personal.homebrew");
        let configuration = entries
            .iter()
            .find(|entry| entry["kind"] == "configuration")
            .unwrap();
        assert_eq!(configuration["port"], "ConfigurationManager");
        assert_eq!(configuration["connector"]["id"], "personal.chezmoi");
        assert_eq!(data["authored"]["source"]["source_class"], "authored");
        assert_eq!(data["observed"]["source"]["source_class"], "observed");

        let apply = execute(&root, &connectors, "machine.apply");
        assert_eq!(apply.status, ResultStatus::Success);
        let data = apply.data.unwrap();
        assert_eq!(data["outcome"], "complete");
        assert_eq!(data["operations"].as_array().unwrap().len(), 2);
        assert_eq!(data["verification"]["satisfied"], true);
        assert_eq!(
            fs::read_to_string(home.join("fixture.txt")).unwrap(),
            "authored macOS configuration\n"
        );

        let verify = execute(&root, &connectors, "machine.verify");
        assert_eq!(verify.status, ResultStatus::Success);
        assert_eq!(verify.data.unwrap()["satisfied"], true);

        let repeated = execute(&root, &connectors, "machine.apply");
        assert_eq!(repeated.status, ResultStatus::Success);
        let repeated = repeated.data.unwrap();
        assert_eq!(repeated["operations"].as_array().unwrap().len(), 0);
        assert_eq!(repeated["verification"]["satisfied"], true);
    }

    #[test]
    fn removing_package_and_configuration_extensions_yields_clear_unavailable_capabilities() {
        let fixture = temporary_directory("removed");
        let root = fixture.join("Central");
        let home = fixture.join("home");
        let source = fixture.join("source");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(&source).unwrap();
        fs::write(source.join("fixture.txt"), "authored\n").unwrap();
        initialize_central(&root).unwrap();
        write_role(&root, &source);

        let brew = fake_brew(&fixture);
        let chezmoi = fake_chezmoi(&fixture);
        let connectors = registry(brew, chezmoi, home, false);

        let plan = execute(&root, &connectors, "machine.plan");
        assert_eq!(plan.status, ResultStatus::Success);
        let data = plan.data.unwrap();
        assert_eq!(data["summary"]["missing"], 2);
        let entries = data["entries"].as_array().unwrap();
        let package = entries.iter().find(|entry| entry["kind"] == "package").unwrap();
        assert!(package["reason"]
            .as_str()
            .unwrap()
            .contains("no eligible PackageManager Connector"));
        let configuration = entries
            .iter()
            .find(|entry| entry["kind"] == "configuration")
            .unwrap();
        assert!(configuration["reason"]
            .as_str()
            .unwrap()
            .contains("no eligible ConfigurationManager Connector"));

        let apply = execute(&root, &connectors, "machine.apply");
        assert_eq!(apply.status, ResultStatus::UnavailableCapability);
        assert_eq!(apply.action.as_deref(), Some("machine.apply"));
    }
}
