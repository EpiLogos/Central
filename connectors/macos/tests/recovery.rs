#[cfg(unix)]
mod unix_tests {
    use central_chezmoi_connector::ChezmoiConnector;
    use central_connector_sdk::{
        CapabilityProbe, Connector, ConnectorContext, ConnectorManifest, ConnectorPortDeclaration,
        ConnectorRegistry, PortContract, PortError, StateChangePreview, StateChangeResult,
        SynchronizationRequest, Synchronizer, CONNECTOR_API_VERSION, SYNCHRONIZER_PORT,
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
    use std::sync::{Arc, Mutex};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temporary_directory(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "central-macos-recovery-{label}-{}-{nonce}",
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

    #[derive(Clone)]
    struct SyncState {
        changed: Arc<Mutex<bool>>,
        applies: Arc<Mutex<usize>>,
    }

    impl SyncState {
        fn new() -> Self {
            Self {
                changed: Arc::new(Mutex::new(true)),
                applies: Arc::new(Mutex::new(0)),
            }
        }
    }

    struct FixtureSynchronizer {
        manifest: ConnectorManifest,
        state: SyncState,
    }

    impl FixtureSynchronizer {
        fn new(state: SyncState) -> Self {
            Self {
                manifest: ConnectorManifest {
                    api_version: CONNECTOR_API_VERSION.to_owned(),
                    id: "fixture.macos-sync".to_owned(),
                    version: "0.1.0".to_owned(),
                    display_name: "macOS recovery synchronization fixture".to_owned(),
                    ports: vec![ConnectorPortDeclaration {
                        id: SYNCHRONIZER_PORT.id.to_owned(),
                        version: SYNCHRONIZER_PORT.version.to_owned(),
                    }],
                    platforms: vec!["macos".to_owned()],
                    entrypoint: "test:macos-recovery".to_owned(),
                    runtime_requirements: Vec::new(),
                    dependency_probes: Vec::new(),
                    configuration_requirements: Vec::new(),
                    mutation_scope: "externally-mutating".to_owned(),
                },
                state,
            }
        }
    }

    impl Synchronizer for FixtureSynchronizer {
        fn preview(&self, input: &SynchronizationRequest) -> Result<StateChangePreview, PortError> {
            assert_eq!(input.id, "central-source");
            let changed = *self.state.changed.lock().unwrap();
            Ok(StateChangePreview {
                changed,
                summary: if changed {
                    "macOS recovery source would synchronize".to_owned()
                } else {
                    "macOS recovery source is synchronized".to_owned()
                },
            })
        }

        fn apply(&self, input: &SynchronizationRequest) -> Result<StateChangeResult, PortError> {
            let changed = self.preview(input)?.changed;
            if changed {
                *self.state.changed.lock().unwrap() = false;
                *self.state.applies.lock().unwrap() += 1;
            }
            Ok(StateChangeResult {
                changed,
                summary: "macOS recovery source synchronized".to_owned(),
            })
        }
    }

    impl Connector for FixtureSynchronizer {
        fn manifest(&self) -> &ConnectorManifest {
            &self.manifest
        }

        fn probe(&self, _port: &PortContract, _context: &ConnectorContext) -> CapabilityProbe {
            CapabilityProbe::available()
        }

        fn synchronizer(&self) -> Option<&dyn Synchronizer> {
            Some(self)
        }
    }

    #[test]
    fn macos_provider_stack_recovers_sync_package_and_configuration_then_repeats_stably() {
        let fixture = temporary_directory("providers");
        let root = fixture.join("Central");
        let home = fixture.join("home");
        let source = fixture.join("source");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(&source).unwrap();
        fs::write(source.join("fixture.txt"), "authored recovery configuration\n").unwrap();
        initialize_central(&root).unwrap();

        let machine = json!({
            "schema": "central.machine",
            "version": 1,
            "role": "primary-workstation",
            "capabilities": [],
            "requirements": {
                "packages": [{ "id": "central-recovery-fixture", "state": "present" }],
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
            serde_json::to_string_pretty(&machine).unwrap(),
        )
        .unwrap();
        let recovery = json!({
            "schema": "central.recovery",
            "version": 1,
            "role": "primary-workstation",
            "synchronization": {
                "id": "central-source",
                "source": { "kind": "fixture", "reference": "fixture://macos" }
            }
        });
        fs::write(
            root.join("Control/machines/primary-workstation.recovery.json"),
            serde_json::to_string_pretty(&recovery).unwrap(),
        )
        .unwrap();

        let brew = fake_brew(&fixture);
        let chezmoi = fake_chezmoi(&fixture);
        let sync_state = SyncState::new();
        let mut connectors = ConnectorRegistry::default();
        connectors
            .register(MacOsNativeConnector::with_host_tools(
                brew.clone(),
                home.clone(),
            ))
            .unwrap();
        connectors
            .register(HomebrewConnector::with_executable(brew))
            .unwrap();
        connectors
            .register(ChezmoiConnector::with_paths(chezmoi, home.clone()))
            .unwrap();
        connectors
            .register(FixtureSynchronizer::new(sync_state.clone()))
            .unwrap();

        let connector_context = ConnectorContext {
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
        let input = json!({ "role": "primary-workstation" });

        let plan = actions.execute("central.recovery.plan", &input, &context);
        assert_eq!(plan.status, ResultStatus::Success);
        let plan = plan.data.unwrap();
        assert_eq!(plan["synchronization"]["status"], "changeable");
        assert_eq!(plan["machine"]["summary"]["changeable"], 2);

        let first = actions.execute("central.recover", &input, &context);
        assert_eq!(first.status, ResultStatus::Success);
        let first = first.data.unwrap();
        assert_eq!(first["outcome"], "complete");
        assert_eq!(first["synchronization"]["changed"], true);
        assert_eq!(first["machine_apply"]["operations"].as_array().unwrap().len(), 2);
        assert_eq!(first["verification"]["satisfied"], true);
        assert_eq!(
            fs::read_to_string(home.join("fixture.txt")).unwrap(),
            "authored recovery configuration\n"
        );
        assert_eq!(*sync_state.applies.lock().unwrap(), 1);

        let second = actions.execute("central.recover", &input, &context);
        assert_eq!(second.status, ResultStatus::Success);
        let second = second.data.unwrap();
        assert!(second["synchronization"].is_null());
        assert_eq!(second["machine_apply"]["operations"].as_array().unwrap().len(), 0);
        assert_eq!(second["verification"]["satisfied"], true);
        assert_eq!(*sync_state.applies.lock().unwrap(), 1);
    }
}
