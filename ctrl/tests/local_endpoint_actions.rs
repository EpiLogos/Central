use central_ctrl::CliEnvironment;
use serde_json::{json, Value};
use std::fs;
use std::net::{Ipv4Addr, TcpListener};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static NEXT_TEMP_ROOT: AtomicU64 = AtomicU64::new(0);

fn temporary_root() -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let sequence = NEXT_TEMP_ROOT.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "central-local-endpoints-actions-{}-{nonce}-{sequence}",
        std::process::id()
    ))
}

fn run(root: &PathBuf, action: &str, input: Value) -> central_ctrl::CliExecution {
    central_ctrl::run_cli(
        &[
            "--json".to_owned(),
            "--root".to_owned(),
            root.display().to_string(),
            "action".to_owned(),
            "run".to_owned(),
            action.to_owned(),
            input.to_string(),
        ],
        &CliEnvironment::default(),
    )
}

fn init(root: &PathBuf) {
    let root_init = central_ctrl::run_cli(
        &[
            "--json".to_owned(),
            "--root".to_owned(),
            root.display().to_string(),
            "init".to_owned(),
        ],
        &CliEnvironment::default(),
    );
    assert_eq!(root_init.exit_code, 0, "{}", root_init.output);

    for (project, project_id) in [("one", "example/one"), ("two", "example/two")] {
        fs::create_dir_all(root.join("Work").join(project)).unwrap();
        let result = run(
            root,
            "projectcentral.init",
            json!({ "project": project, "project_id": project_id }),
        );
        assert_eq!(result.exit_code, 0, "{}", result.output);
    }
}

#[test]
fn actions_record_ports_observe_occupancy_and_guard_cross_project_collisions() {
    let root = temporary_root();
    init(&root);

    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let port = listener.local_addr().unwrap().port();

    let first = run(
        &root,
        "projectcentral.local-endpoints.set",
        json!({
            "project": "one",
            "id": "db",
            "kind": "database",
            "service": "postgres",
            "description": "Local development database",
            "port": port
        }),
    );
    assert_eq!(first.exit_code, 0, "{}", first.output);
    assert!(root
        .join("Work/one/ProjectCentral/local-endpoints.json")
        .is_file());

    let project_inspect = run(
        &root,
        "projectcentral.local-endpoints.inspect",
        json!({ "project": "one" }),
    );
    assert_eq!(project_inspect.exit_code, 0, "{}", project_inspect.output);
    let project_value: Value = serde_json::from_str(&project_inspect.output).unwrap();
    assert_eq!(
        project_value["data"]["endpoints"][0]["observation"]["status"],
        "occupied"
    );

    let rejected = run(
        &root,
        "projectcentral.local-endpoints.set",
        json!({
            "project": "two",
            "id": "api",
            "kind": "http",
            "port": port
        }),
    );
    assert_eq!(rejected.exit_code, 7, "{}", rejected.output);
    let rejected_value: Value = serde_json::from_str(&rejected.output).unwrap();
    assert_eq!(rejected_value["status"], "verification_failure");

    let intentional = run(
        &root,
        "projectcentral.local-endpoints.set",
        json!({
            "project": "two",
            "id": "api",
            "kind": "http",
            "port": port,
            "allow_conflict": true
        }),
    );
    assert_eq!(intentional.exit_code, 0, "{}", intentional.output);

    let central = run(&root, "central.local-endpoints.inspect", json!({}));
    assert_eq!(central.exit_code, 0, "{}", central.output);
    let central_value: Value = serde_json::from_str(&central.output).unwrap();
    assert_eq!(central_value["data"]["endpoints"].as_array().unwrap().len(), 2);
    assert_eq!(central_value["data"]["collisions"].as_array().unwrap().len(), 1);
    assert_eq!(central_value["data"]["collisions"][0]["port"], port);

    let refreshed = run(&root, "central.local-endpoints.refresh", json!({}));
    assert_eq!(refreshed.exit_code, 0, "{}", refreshed.output);
    assert!(root.join(".central/local-endpoints.json").is_file());

    drop(listener);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn suggest_returns_an_undeclared_bindable_port_in_the_requested_range() {
    let root = temporary_root();
    init(&root);

    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let candidate = listener.local_addr().unwrap().port();
    drop(listener);

    let suggested = run(
        &root,
        "central.local-endpoints.suggest",
        json!({ "start": candidate, "end": candidate }),
    );
    assert_eq!(suggested.exit_code, 0, "{}", suggested.output);
    let value: Value = serde_json::from_str(&suggested.output).unwrap();
    assert_eq!(value["data"]["port"], candidate);
    assert_eq!(value["data"]["scope"], "localhost");
    assert_eq!(value["data"]["protocol"], "tcp");

    let _ = fs::remove_dir_all(root);
}
