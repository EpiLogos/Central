use central_connector_sdk::{
    run_synchronizer_conformance, Connector, ConnectorContext, PortErrorCode,
    ReconciliationSourceReference, SynchronizationRequest, Synchronizer,
    SynchronizerConformanceFixture,
};
use central_ctrl::{
    create_core_action_registry, initialize_central, ActionExecutionContext, ConnectorRegistry,
    InMemoryMachineConnector, MachineInspectionOutput, ResultStatus, RootOptions,
};
use central_git_sync_connector::{GitSynchronizerConnector, GIT_SYNCHRONIZER_CONNECTOR_ID};
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

fn temporary_directory(label: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "central-git-sync-{label}-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir_all(&path).unwrap();
    path
}

fn run(command: &mut Command, operation: &str) -> Output {
    let output = command
        .output()
        .unwrap_or_else(|error| panic!("{operation} could not start: {error}"));
    assert!(
        output.status.success(),
        "{operation} failed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn git(args: &[&str]) -> Output {
    run(Command::new("git").args(args), &format!("git {}", args.join(" ")))
}

fn git_at(repository: &Path, args: &[&str]) -> Output {
    run(
        Command::new("git").arg("-C").arg(repository).args(args),
        &format!("git -C {} {}", repository.display(), args.join(" ")),
    )
}

struct GitFixture {
    root: PathBuf,
    remote: PathBuf,
    seed: PathBuf,
    target: PathBuf,
}

impl GitFixture {
    fn changeable(label: &str) -> Self {
        let root = temporary_directory(label);
        let remote = root.join("remote.git");
        let seed = root.join("seed");
        let target = root.join("target");

        git(&["init", "--bare", remote.to_str().unwrap()]);
        git(&["init", seed.to_str().unwrap()]);
        git_at(&seed, &["config", "user.email", "central@example.invalid"]);
        git_at(&seed, &["config", "user.name", "Central acceptance"]);

        fs::write(seed.join("payload.txt"), "v1\n").unwrap();
        git_at(&seed, &["add", "payload.txt"]);
        git_at(&seed, &["commit", "-m", "initial"]);

        let branch = String::from_utf8_lossy(&git_at(&seed, &["rev-parse", "--abbrev-ref", "HEAD"]).stdout)
            .trim()
            .to_owned();
        git_at(&seed, &["remote", "add", "origin", remote.to_str().unwrap()]);
        git_at(&seed, &["push", "origin", "HEAD"]);
        git(&[
            "--git-dir",
            remote.to_str().unwrap(),
            "symbolic-ref",
            "HEAD",
            &format!("refs/heads/{branch}"),
        ]);
        git(&["clone", remote.to_str().unwrap(), target.to_str().unwrap()]);

        fs::write(seed.join("payload.txt"), "v2\n").unwrap();
        git_at(&seed, &["add", "payload.txt"]);
        git_at(&seed, &["commit", "-m", "advance remote"]);
        git_at(&seed, &["push", "origin", "HEAD"]);

        Self {
            root,
            remote,
            seed,
            target,
        }
    }

    fn request(&self) -> SynchronizationRequest {
        SynchronizationRequest {
            id: "central-authored-source".to_owned(),
            source: Some(ReconciliationSourceReference {
                kind: "git".to_owned(),
                reference: self.remote.to_string_lossy().into_owned(),
            }),
        }
    }

    fn connector(&self) -> GitSynchronizerConnector {
        GitSynchronizerConnector::with_paths(PathBuf::from("git"), self.target.clone())
    }
}

impl Drop for GitFixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn git_target_passes_public_synchronizer_conformance_with_real_mutation() {
    let fixture = GitFixture::changeable("conformance");
    let connector = fixture.connector();
    let report = run_synchronizer_conformance(
        &connector,
        &SynchronizerConformanceFixture {
            platform: std::env::consts::OS.to_owned(),
            request: fixture.request(),
        },
    )
    .unwrap();

    assert_eq!(report.port_id, "Synchronizer");
    assert_eq!(report.connector.id, GIT_SYNCHRONIZER_CONNECTOR_ID);
    assert!(report.checks.iter().any(|check| check == "fixture-precondition"));
    assert_eq!(fs::read_to_string(fixture.target.join("payload.txt")).unwrap(), "v2\n");
}

#[test]
fn git_apply_refuses_to_overwrite_a_dirty_target() {
    let fixture = GitFixture::changeable("dirty-target");
    fs::write(fixture.target.join("local-only.txt"), "do not overwrite\n").unwrap();
    let connector = fixture.connector();
    let request = fixture.request();

    assert!(connector.preview(&request).unwrap().changed);
    let error = connector.apply(&request).unwrap_err();
    assert_eq!(error.code, PortErrorCode::InvalidConfiguration);
    assert!(error.message.contains("local changes"));
    assert_eq!(fs::read_to_string(fixture.target.join("payload.txt")).unwrap(), "v1\n");
}

#[test]
fn canonical_recover_uses_the_public_git_synchronizer_against_the_real_target() {
    let fixture = GitFixture::changeable("canonical-action");
    let central_root = fixture.root.join("Central");
    initialize_central(&central_root).unwrap();

    let role = "fresh-session-host";
    let machine = json!({
        "schema": "central.machine",
        "version": 1,
        "role": role,
        "capabilities": [],
        "requirements": {
            "packages": [],
            "configurations": [],
            "services": []
        }
    });
    fs::write(
        central_root.join(format!("Control/machines/{role}.json")),
        serde_json::to_string_pretty(&machine).unwrap(),
    )
    .unwrap();
    let recovery = json!({
        "schema": "central.recovery",
        "version": 1,
        "role": role,
        "synchronization": {
            "id": "central-authored-source",
            "source": {
                "kind": "git",
                "reference": fixture.remote.to_string_lossy()
            }
        }
    });
    fs::write(
        central_root.join(format!("Control/machines/{role}.recovery.json")),
        serde_json::to_string_pretty(&recovery).unwrap(),
    )
    .unwrap();

    let mut connectors = ConnectorRegistry::default();
    connectors.register(fixture.connector()).unwrap();
    connectors
        .register(InMemoryMachineConnector::new(MachineInspectionOutput {
            platform: std::env::consts::OS.to_owned(),
            architecture: std::env::consts::ARCH.to_owned(),
            capabilities: Vec::new(),
            packages: Vec::new(),
            configurations: Vec::new(),
            services: Vec::new(),
        }))
        .unwrap();

    let connector_context = ConnectorContext {
        platform: std::env::consts::OS.to_owned(),
    };
    let root_options = RootOptions {
        explicit_root: Some(central_root.clone()),
        ..RootOptions::default()
    };
    let context = ActionExecutionContext {
        root_options: &root_options,
        connectors: &connectors,
        connector_context: &connector_context,
    };
    let registry = create_core_action_registry();

    let first = registry.execute("central.recover", &json!({ "role": role }), &context);
    assert_eq!(first.status, ResultStatus::Success);
    let data = first.data.unwrap();
    assert_eq!(data["outcome"], "complete");
    assert_eq!(
        data["initial_plan"]["synchronization"]["connector"]["id"],
        GIT_SYNCHRONIZER_CONNECTOR_ID
    );
    assert_eq!(data["synchronization"]["changed"], true);
    assert_eq!(fs::read_to_string(fixture.target.join("payload.txt")).unwrap(), "v2\n");

    let repeated = registry.execute("central.recover", &json!({ "role": role }), &context);
    assert_eq!(repeated.status, ResultStatus::Success);
    let repeated_data = repeated.data.unwrap();
    assert_eq!(repeated_data["outcome"], "complete");
    assert!(repeated_data["synchronization"].is_null());
    assert_eq!(fs::read_to_string(fixture.target.join("payload.txt")).unwrap(), "v2\n");
}
