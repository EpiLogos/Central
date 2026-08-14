#[cfg(unix)]
mod unix_tests {
    use central_chezmoi_connector::ChezmoiConnector;
    use central_connector_sdk::{
        run_configuration_manager_conformance, ConfigurationManager, ConfigurationManagerConformanceFixture,
        ConfigurationStateRequest, Connector, ConnectorContext, ReconciliationSourceReference,
        CONFIGURATION_MANAGER_PORT,
    };
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
            "central-chezmoi-{label}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn fake_chezmoi() -> PathBuf {
        let directory = temporary_directory("fake");
        let executable = directory.join("chezmoi");
        fs::write(
            &executable,
            r#"#!/bin/sh
set -eu
if [ "${1:-}" = "--version" ]; then
  echo "chezmoi fixture"
  exit 0
fi
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
  if [ ! -f "$DST" ] || ! cmp -s "$SRC" "$DST"; then
    echo "changed: $REL"
  fi
  exit 0
fi
if [ "$CMD" = "apply" ]; then
  mkdir -p "$(dirname "$DST")"
  cp "$SRC" "$DST"
  exit 0
fi
exit 64
"#,
        )
        .unwrap();
        let mut permissions = fs::metadata(&executable).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&executable, permissions).unwrap();
        executable
    }

    #[test]
    fn chezmoi_passes_shared_configuration_manager_conformance() {
        let destination = temporary_directory("destination");
        let source = temporary_directory("source");
        fs::write(source.join("fixture.txt"), "authored configuration\n").unwrap();
        let connector = ChezmoiConnector::with_paths(fake_chezmoi(), destination.clone());
        let request = ConfigurationStateRequest {
            id: destination.join("fixture.txt").display().to_string(),
            present: true,
            source: Some(ReconciliationSourceReference {
                kind: "chezmoi".to_owned(),
                reference: source.display().to_string(),
            }),
        };

        let report = run_configuration_manager_conformance(
            &connector,
            &ConfigurationManagerConformanceFixture {
                platform: "macos".to_owned(),
                request: request.clone(),
            },
        )
        .unwrap();
        assert_eq!(report.port_id, CONFIGURATION_MANAGER_PORT.id);
        assert_eq!(report.connector.id, "personal.chezmoi");
        assert_eq!(
            fs::read_to_string(destination.join("fixture.txt")).unwrap(),
            "authored configuration\n"
        );
        assert!(!connector.preview(&request).unwrap().changed);
    }

    #[test]
    fn chezmoi_rejects_wrong_source_kind_and_destination_escape() {
        let destination = temporary_directory("safety-destination");
        let source = temporary_directory("safety-source");
        let connector = ChezmoiConnector::with_paths(fake_chezmoi(), destination.clone());

        let wrong_source = ConfigurationStateRequest {
            id: destination.join("fixture.txt").display().to_string(),
            present: true,
            source: Some(ReconciliationSourceReference {
                kind: "file".to_owned(),
                reference: source.display().to_string(),
            }),
        };
        let error = connector.preview(&wrong_source).unwrap_err();
        assert!(error.message.contains("source kind"));

        let escaping = ConfigurationStateRequest {
            id: "../outside".to_owned(),
            present: false,
            source: None,
        };
        let error = connector.preview(&escaping).unwrap_err();
        assert!(error.message.contains("escape"));
    }

    #[test]
    fn chezmoi_probe_reports_platform_and_dependency_ineligibility() {
        let destination = temporary_directory("probe");
        let connector = ChezmoiConnector::with_paths(fake_chezmoi(), destination.clone());
        let off_platform = connector.probe(
            &CONFIGURATION_MANAGER_PORT,
            &ConnectorContext {
                platform: "linux".to_owned(),
            },
        );
        assert!(!off_platform.available);
        assert!(off_platform.reason.unwrap().contains("does not support platform"));

        let missing = ChezmoiConnector::with_paths(
            PathBuf::from("/definitely/missing/chezmoi"),
            destination,
        );
        let missing_probe = missing.probe(
            &CONFIGURATION_MANAGER_PORT,
            &ConnectorContext {
                platform: "macos".to_owned(),
            },
        );
        assert!(!missing_probe.available);
        assert!(missing_probe.reason.unwrap().contains("unavailable"));
    }
}
