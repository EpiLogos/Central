#[cfg(unix)]
mod unix_tests {
    use central_connector_sdk::{
        run_package_manager_conformance, Connector, ConnectorContext,
        PackageManagerConformanceFixture, PackageStateRequest, PACKAGE_MANAGER_PORT,
    };
    use central_homebrew_connector::HomebrewConnector;
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
            "central-homebrew-{label}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn fake_brew() -> PathBuf {
        let directory = temporary_directory("fake");
        let executable = directory.join("brew");
        fs::write(
            &executable,
            r#"#!/bin/sh
set -eu
STATE="$0.state"
if [ "${1:-}" = "--version" ]; then
  echo "Homebrew fixture"
  exit 0
fi
if [ "${1:-}" = "list" ]; then
  id="${4:-}"
  if [ -f "$STATE" ] && [ "$(cat "$STATE")" = "$id" ]; then
    echo "$id 1.0"
    exit 0
  fi
  exit 1
fi
if [ "${1:-}" = "install" ]; then
  id="${3:-}"
  printf '%s' "$id" > "$STATE"
  exit 0
fi
if [ "${1:-}" = "uninstall" ]; then
  rm -f "$STATE"
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
    fn homebrew_passes_shared_package_manager_conformance() {
        let connector = HomebrewConnector::with_executable(fake_brew());
        let report = run_package_manager_conformance(
            &connector,
            &PackageManagerConformanceFixture {
                platform: "macos".to_owned(),
                request: PackageStateRequest {
                    id: "central-fixture".to_owned(),
                    present: true,
                    source: None,
                },
            },
        )
        .unwrap();
        assert_eq!(report.port_id, PACKAGE_MANAGER_PORT.id);
        assert_eq!(report.connector.id, "personal.homebrew");
        assert!(report.checks.iter().any(|check| check == "post-apply-preview"));
        assert!(report.checks.iter().any(|check| check == "idempotent-apply"));
    }

    #[test]
    fn homebrew_probe_is_explicit_about_platform_and_dependency() {
        let connector = HomebrewConnector::with_executable(fake_brew());
        let off_platform = connector.probe(
            &PACKAGE_MANAGER_PORT,
            &ConnectorContext {
                platform: "linux".to_owned(),
            },
        );
        assert!(!off_platform.available);
        assert!(off_platform.reason.unwrap().contains("does not support platform"));

        let missing = HomebrewConnector::with_executable(PathBuf::from("/definitely/missing/brew"));
        let missing_probe = missing.probe(
            &PACKAGE_MANAGER_PORT,
            &ConnectorContext {
                platform: "macos".to_owned(),
            },
        );
        assert!(!missing_probe.available);
        assert!(missing_probe.reason.unwrap().contains("unavailable"));
    }
}
