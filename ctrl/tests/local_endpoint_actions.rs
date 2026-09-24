use central_ctrl::CliEnvironment;
use serde_json::{json, Value};
use std::fs;
use std::net::{Ipv4Addr, TcpListener};
use std::path::{Path, PathBuf};
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

fn run(root: &Path, action: &str, input: Value) -> central_ctrl::CliExecution {
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

fn init(root: &Path) {
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
    assert_eq!(
        central_value["data"]["endpoints"].as_array().unwrap().len(),
        2
    );
    assert_eq!(
        central_value["data"]["collisions"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
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

#[test]
fn scope_aware_set_inspect_collision_and_suggest_across_interfaces() {
    let root = temporary_root();
    init(&root);

    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let port = listener.local_addr().unwrap().port();

    let unknown_scope = run(
        &root,
        "projectcentral.local-endpoints.set",
        json!({
            "project": "one",
            "id": "bad",
            "kind": "http",
            "port": port,
            "scope": "multicast"
        }),
    );
    assert_eq!(unknown_scope.exit_code, 2, "{}", unknown_scope.output);

    // A tailnet-scoped and a localhost-scoped declaration of the same port
    // coexist: the set refuses only same-scope overlap.
    let set_tailnet = run(
        &root,
        "projectcentral.local-endpoints.set",
        json!({
            "project": "one",
            "id": "stdb",
            "kind": "database",
            "port": port,
            "scope": "tailnet"
        }),
    );
    assert_eq!(set_tailnet.exit_code, 0, "{}", set_tailnet.output);
    let set_value: Value = serde_json::from_str(&set_tailnet.output).unwrap();
    assert_eq!(
        set_value["data"]["inspection"]["endpoints"][0]["declaration"]["scope"],
        "tailnet"
    );

    let set_localhost = run(
        &root,
        "projectcentral.local-endpoints.set",
        json!({
            "project": "one",
            "id": "web",
            "kind": "http",
            "port": port,
            "scope": "localhost"
        }),
    );
    assert_eq!(set_localhost.exit_code, 0, "{}", set_localhost.output);
    let local_value: Value = serde_json::from_str(&set_localhost.output).unwrap();
    assert_eq!(
        local_value["data"]["same_port_other_scopes"][0]["scope"],
        "tailnet"
    );
    assert_eq!(
        local_value["data"]["same_port_other_scopes_note"],
        "the same protocol/port under a different scope is not a collision"
    );

    // The loopback listener occupies the localhost scope; the tailnet scope
    // is a different interface story and is not reported occupied.
    let inspect = run(
        &root,
        "projectcentral.local-endpoints.inspect",
        json!({ "project": "one" }),
    );
    assert_eq!(inspect.exit_code, 0, "{}", inspect.output);
    let inspect_value: Value = serde_json::from_str(&inspect.output).unwrap();
    let endpoints = inspect_value["data"]["endpoints"].as_array().unwrap();
    for endpoint in endpoints {
        match endpoint["declaration"]["scope"].as_str().unwrap() {
            "localhost" => assert_eq!(endpoint["observation"]["status"], "occupied"),
            "tailnet" => assert_ne!(endpoint["observation"]["status"], "occupied"),
            other => panic!("unexpected scope {other}"),
        }
    }

    // Central-wide: same port under different scopes is not a collision.
    let central = run(&root, "central.local-endpoints.inspect", json!({}));
    assert_eq!(central.exit_code, 0, "{}", central.output);
    let central_value: Value = serde_json::from_str(&central.output).unwrap();
    assert_eq!(
        central_value["data"]["collisions"]
            .as_array()
            .unwrap()
            .len(),
        0
    );

    drop(listener);

    // A port held on the wildcard binding is occupied on every interface,
    // so suggestion refuses it even though loopback alone would look free.
    let blocker = TcpListener::bind((std::net::Ipv4Addr::UNSPECIFIED, 0)).unwrap();
    let blocked_port = blocker.local_addr().unwrap().port();
    let refused = run(
        &root,
        "central.local-endpoints.suggest",
        json!({ "start": blocked_port, "end": blocked_port }),
    );
    assert_eq!(refused.exit_code, 2, "{}", refused.output);
    drop(blocker);

    let free = run(
        &root,
        "central.local-endpoints.suggest",
        json!({ "start": blocked_port, "end": blocked_port }),
    );
    assert_eq!(free.exit_code, 0, "{}", free.output);
    let free_value: Value = serde_json::from_str(&free.output).unwrap();
    assert_eq!(free_value["data"]["port"], blocked_port);
    assert_eq!(free_value["data"]["scope"], "localhost");
    let free_scopes = free_value["data"]["free_in"]
        .as_array()
        .unwrap()
        .iter()
        .map(|scope| scope["scope"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert!(free_scopes.contains(&"localhost"));
    assert!(free_scopes.contains(&"any"));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn v1_declaration_source_stays_readable_and_occupies_as_localhost() {
    let root = temporary_root();
    init(&root);

    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let port = listener.local_addr().unwrap().port();

    fs::write(
        root.join("Work/one/ProjectCentral/local-endpoints.json"),
        format!(
            r#"{{"schema":"central.project.local-endpoints/v1","endpoints":[{{"id":"legacy","kind":"http","port":{port}}}]}}"#
        ),
    )
    .unwrap();

    let inspect = run(
        &root,
        "projectcentral.local-endpoints.inspect",
        json!({ "project": "one" }),
    );
    assert_eq!(inspect.exit_code, 0, "{}", inspect.output);
    let inspect_value: Value = serde_json::from_str(&inspect.output).unwrap();
    let endpoint = &inspect_value["data"]["endpoints"][0];
    assert_eq!(endpoint["declaration"]["scope"], "localhost");
    assert_eq!(endpoint["observation"]["status"], "occupied");

    // A mutation normalises the v1 source onto v2.
    let updated = run(
        &root,
        "projectcentral.local-endpoints.set",
        json!({
            "project": "one",
            "id": "legacy",
            "kind": "http",
            "service": "legacy service",
            "port": port
        }),
    );
    assert_eq!(updated.exit_code, 0, "{}", updated.output);
    let written =
        fs::read_to_string(root.join("Work/one/ProjectCentral/local-endpoints.json")).unwrap();
    assert!(written.contains("central.project.local-endpoints/v2"));

    drop(listener);
    let _ = fs::remove_dir_all(root);
}
