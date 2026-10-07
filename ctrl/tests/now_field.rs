//! The composed NOW field (`central.now.field`) — O-I #220 lane A: one
//! activity reading joining the NOW plane with the native material and
//! Gateway owners. The external owners are stubbed at PATH, so every
//! assertion here is about the composition law: declared joins, honest
//! unavailability, disclosed bounds — never invented work.

use central_ctrl::continuous_work::{execute_at, source::Scope};
use serde_json::{json, Value};
use std::{
    fs,
    io,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    sync::{Mutex, OnceLock},
};

static NEXT: AtomicU64 = AtomicU64::new(0);
static FIELD_ENV: OnceLock<Mutex<()>> = OnceLock::new();

struct World(PathBuf);

impl Drop for World {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn unique(prefix: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "{prefix}-{}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ))
}

impl World {
    fn new() -> Self {
        let path = unique("central-now-field");
        central_ctrl::initialize_central(&path).unwrap();
        let world = World(fs::canonicalize(&path).unwrap());
        fs::create_dir_all(world.root().join("Work/one")).unwrap();
        fs::create_dir_all(world.root().join("Control/machines")).unwrap();
        fs::write(
            world.root().join("Control/machines/current.json"),
            serde_json::to_vec_pretty(&json!({
                "schema":"central.machine","version":1,"role":"current","capabilities":[],
                "requirements":{"packages":[],"configurations":[],"services":[]},
                "bindings":[{"kind":"workcell","reference":"workcell:local"}]
            }))
            .unwrap(),
        )
        .unwrap();
        let scope = Scope::resolve(world.root(), None).unwrap();
        let policy_path = "Control/user/placement.json";
        fs::write(
            world.root().join(&policy_path),
            serde_json::to_vec_pretty(&json!({
                "schema":"central.work-placement-policy/v1","scope_ref":"control:root",
                "writable":[{"path":"Work/one","class":"repository"}],
                "protected":[],"enforcement":"native-actions",
                "required_coverage":["file-content"],"lease_seconds":300
            }))
            .unwrap(),
        )
        .unwrap();
        fs::create_dir_all(world.root().join("Control/relations")).unwrap();
        fs::write(
            world.root().join("Control/relations/source-relations.json"),
            serde_json::to_vec_pretty(&json!({
                "schema":"central.control.ground-relations/v1","project_id":"control:root",
                "relations":[{
                    "ref":scope.source_ref(policy_path),"path":policy_path,
                    "roles":["work-placement-policy"],"provenance":"human-adopted",
                    "standing":"architecture-contract","treatment":"projectcentral-user",
                    "recognition":"controlled-test-fixture-not-personal-adoption",
                    "recorded_at_unix_seconds":1
                }]
            }))
            .unwrap(),
        )
        .unwrap();
        world
    }

    fn root(&self) -> &Path {
        &self.0
    }

    fn policy_revision(&self) -> String {
        execute_at(self.root(), "policy", &json!({}), 100).unwrap()["revision"]
            .as_str()
            .unwrap()
            .to_string()
    }

    fn workcell_root(&self) -> Value {
        execute_at(
            self.root(),
            "workcell_root",
            &json!({"workcell_ref":"workcell:local"}),
            100,
        )
        .unwrap()
    }

    fn child(&self, parent: &Value, task: &str) -> Value {
        execute_at(
            self.root(),
            "allocate",
            &json!({
                "task_ref":task,"purpose":"bounded child work",
                "expected_policy_revision":self.policy_revision(),
                "parent_now_ref":parent["now_ref"],
                "workcell_ref":"workcell:local"
            }),
            100,
        )
        .unwrap()
    }

    fn field(&self, input: &Value) -> Value {
        execute_at(self.root(), "now_field", input, 1_000).unwrap()
    }
}

/// Serialise every env/PATH mutation in this file: tests here run in
/// parallel threads of one process, and subprocess resolution observes the
/// process-global PATH.
fn field_env() -> &'static Mutex<()> {
    FIELD_ENV.get_or_init(|| Mutex::new(()))
}

fn stub_owners(dir: &Path, scripts: &[(&str, &str)]) -> PathBuf {
    let bin = dir.join("bin");
    fs::create_dir_all(&bin).unwrap();
    for (name, body) in scripts {
        let path = bin.join(name);
        fs::write(&path, body).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        }
    }
    bin
}

fn with_path<T>(bin: &Path, run: impl FnOnce() -> T) -> T {
    let _guard = field_env().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let original = std::env::var("PATH").unwrap_or_default();
    std::env::set_var("PATH", format!("{}:{}", bin.display(), original));
    let result = run();
    std::env::set_var("PATH", original);
    result
}

const CANNED_CENSUS: &str = r#"{"schema":"workcell.place-census/v1","machine":"stub-host","ok":true,
  "workcell_ref":"workcell:local","panes":[{"classification":"harness","classification_evidence":"provider-foreground","harness_slug":"pi","pane_command":"pi","pane_dead":false,"pane_id":"w1:p1","pane_pid":11,"pane_tty":null,"place_key":"w1/w1:p1","process_start_marker":"stub","provider":"herdr","session_created":null,"session_name":null,"tab_id":"w1:t1","window_index":null,"window_name":null,"workspace_id":"w1","workspace_label":"declared child work"}],
  "providers":[{"detail":null,"malformed_rows":0,"pane_count":1,"provider":"herdr","status":"ok","version":"herdr 0.8.2"}],
  "place_reuse_findings":[],"summary":{"dead":0,"harness":1,"other":0,"panes":1,"self":0,"unobserved":0}}"#;

const CANNED_STATUS: &str = r#"{"ok":true,"health":"healthy","offers":0,"providers":1,
  "workcell_ref":"workcell:local","state_root":"/tmp/stub"}"#;

const CANNED_INSTANCES: &str = r#"{"ok":true,"workcell_ref":"workcell:local","instances":[
  {"consecutive_misses":0,"evidence_grade":"live-pid","executable":{"path":"/usr/bin/pi","sha256":"stub"},
   "harness_ref":"harness/pi","instance_ref":"instance:pi:stub","liveness":"live","observed_at":"unix:1000",
   "pids":[11],"schema":"workcell.harness-instance/v1","seams":[],"workcell_ref":"workcell:local"}]}"#;

const CANNED_GATEWAY_REMOTES: &str = r#"{"ok":true,"schema":1,"warnings":[],
  "data":{"remotes":[{"workcell_ref":"workcell:mac","token_location":"file:/tmp/mac.token","websocket_bind":"100.109.102.82:7788","websocket_path":"/"}]},"type":"remotes"}"#;

const CANNED_GATEWAY: &str = r#"{"ok":true,"schema":1,"warnings":[],
  "data":{"type":"status","status":{"version":"aikit.agency-gateway/v1","gateway_ref":"agency-gateway/stub",
  "binding_count":1,"stream_count":2,"delivery_receipt_count":3,"pending_delivery_count":0,
  "connector_count":1,"connector_health":[{"connector_ref":"gateway-connector/telegram/main","state":"connected","detail":"stub","provenance":["stub/v1"]}]}}}"#;

fn writing_stub() -> String {
    // Resolve the canned files relative to the stub's own directory so the
    // test controls content per case without depending on cwd.
    r#"#!/bin/sh
case "$1" in
  gateway) exec cat "$(dirname "$0")/../$1-$2.json" ;;
  *) exec cat "$(dirname "$0")/../$1.json" ;;
esac
"#
    .to_string()
}
const FAILING_STUB: &str = "#!/bin/sh\necho \"stub owner refused: controlled failure\" >&2\nexit 3\n";

#[test]
fn field_composes_declared_material_gateway_and_children() {
    let world = World::new();
    let root_now = world.workcell_root();
    let child = world.child(&root_now, "task:field-child");
    let dir = unique("central-now-field-stubs");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("places.json"), CANNED_CENSUS).unwrap();
    fs::write(dir.join("status.json"), CANNED_STATUS).unwrap();
    fs::write(dir.join("instances.json"), CANNED_INSTANCES).unwrap();
    fs::write(dir.join("gateway-status.json"), CANNED_GATEWAY).unwrap();
    let bin = stub_owners(&dir, &[("workcell", &writing_stub()), ("aikit", &writing_stub())]);
    let field = with_path(&bin, || world.field(&json!({})));
    assert_eq!(field["schema"], "central.now-field/v1");

    // The machine declaration joins the local readings to the Workcell root.
    assert_eq!(
        field["machine"]["declared_workcell_ref"],
        "workcell:local",
        "declared binding read from Control/machines/current.json"
    );
    assert!(
        field["machine"]["declaration_source_ref"]
            .as_str()
            .unwrap()
            .starts_with("central:source:control:root:Control/machines/current.json")
    );

    assert_eq!(field["workcells"].as_array().unwrap().len(), 1);
    let workcell = &field["workcells"][0];
    assert_eq!(workcell["workcell_ref"], "workcell:local");
    assert_eq!(workcell["root"]["now_ref"], root_now["now_ref"]);

    // Children hang from the root by the native parent relation, with liveness.
    let children = workcell["children"].as_array().unwrap();
    assert_eq!(children.len(), 1);
    assert_eq!(children[0]["task_ref"], "task:field-child");
    assert_eq!(children[0]["live"], true);
    assert_eq!(workcell["children_count"], 1);

    // Material is the owner's own reading, joined by declaration, with the
    // native cell ref retained verbatim beside the declared identity.
    let material = &workcell["material"];
    assert_eq!(material["available"], true);
    assert_eq!(material["observation_scope"], "local");
    assert_eq!(material["native_workcell_ref"], "workcell:local");
    assert!(material["declared_join"]["source_ref"].is_string());
    assert_eq!(
        material["census"]["reading"]["summary"]["panes"], 1,
        "the owner's census is carried, not rewritten"
    );
    assert_eq!(material["census"]["observed_at_unix_seconds"], 1_000);
    assert_eq!(material["instances"]["reading"]["instances"].as_array().unwrap().len(), 1);

    // Gateway status comes from the AIKit owner's own status reading.
    let gateway = &workcell["gateway"];
    assert_eq!(gateway["available"], true);
    assert_eq!(
        gateway["status"]["reading"]["data"]["status"]["gateway_ref"],
        "agency-gateway/stub"
    );

    // Temporal sources this fixture does not recognise are absent, and the
    // reading says so instead of inventing a Day or a timezone.
    assert!(field["time"]["policy"].is_null());
    assert!(field["time"]["day"].is_null());
    assert!(!field["time"]["note"].as_str().unwrap().is_empty());

    // The Day-rollover horizon carries the active roots and live children.
    let horizon = &field["horizon"];
    assert_eq!(horizon["carried"].as_array().unwrap().len(), 2);
}

#[test]
fn field_marks_failed_owners_unavailable_not_empty() {
    let world = World::new();
    world.workcell_root();
    let dir = unique("central-now-field-failing");
    fs::create_dir_all(&dir).unwrap();
    // Only failing owner stubs are on PATH ahead of the machine's real
    // binaries: workcell exits 3 with its own words on stderr and aikit does
    // the same, so every owner reading must report its own failure.
    let bin = stub_owners(
        &dir,
        &[("workcell", FAILING_STUB), ("aikit", FAILING_STUB)],
    );
    let field = with_path(&bin, || {
        world.field(&json!({"workcell_refs":["workcell:local"]}))
    });

    let workcell = &field["workcells"][0];
    // The NOW plane is still read: unavailability is not absence of work.
    assert_eq!(workcell["root"]["task_ref"], "central:task:control:root:workcell-root:workcell:local");
    let material = &workcell["material"];
    assert_eq!(material["available"], false);
    let census = &material["census"];
    assert_eq!(census["available"], false);
    assert!(
        census["reason"]
            .as_str()
            .unwrap()
            .contains("controlled failure"),
        "the owner's own stderr words are carried: {}",
        census["reason"]
    );
    assert!(
        !census["native_owner_command"].as_str().unwrap().is_empty(),
        "the reading names exactly which owner call failed"
    );
    assert_eq!(census["observed_at_unix_seconds"], 1_000);
    let gateway = &workcell["gateway"];
    assert_eq!(gateway["available"], false);
    assert!(
        gateway["status"]["reason"]
            .as_str()
            .unwrap()
            .contains("controlled failure"),
        "gateway failure carries the owner's own words: {}",
        gateway["status"]["reason"]
    );
}

#[test]
fn field_truncates_census_rows_and_discloses_the_bound() {
    let world = World::new();
    world.workcell_root();
    let mut panes = Vec::new();
    for index in 0..170 {
        panes.push(json!({
            "classification":"other","classification_evidence":"ps-comm",
            "harness_slug":null,"pane_command":"bash","pane_dead":false,
            "pane_id":format!("%{index}"),"pane_pid":1000+index,"pane_tty":null,
            "place_key":format!("s/%{index}"),"process_start_marker":"stub",
            "provider":"tmux","session_created":null,"session_name":"s",
            "tab_id":null,"window_index":"0","window_name":"w",
            "workspace_id":null,"workspace_label":null
        }));
    }
    let census = json!({
        "schema":"workcell.place-census/v1","machine":"stub-host","ok":true,
        "workcell_ref":"workcell:local","panes":panes,"providers":[],
        "place_reuse_findings":[],
        "summary":{"dead":0,"harness":0,"other":170,"panes":170,"self":0,"unobserved":0}
    });
    let dir = unique("central-now-field-cap");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("places.json"), serde_json::to_vec(&census).unwrap()).unwrap();
    fs::write(dir.join("status.json"), CANNED_STATUS).unwrap();
    fs::write(dir.join("instances.json"), CANNED_INSTANCES).unwrap();
    fs::write(dir.join("gateway-status.json"), CANNED_GATEWAY).unwrap();
    let bin = stub_owners(&dir, &[("workcell", &writing_stub()), ("aikit", &writing_stub())]);
    let field = with_path(&bin, || world.field(&json!({})));
    let census = &field["workcells"][0]["material"]["census"];
    assert_eq!(
        census["reading"]["panes"].as_array().unwrap().len(),
        field["bounds"]["census_rows_max"].as_u64().unwrap() as usize,
        "rows are capped at the disclosed bound"
    );
    assert_eq!(census["truncated_rows"], 10);
}

#[test]
fn field_names_requested_roots_that_do_not_exist() {
    let world = World::new();
    world.workcell_root();
    let field = world.field(&json!({"workcell_refs":["workcell:absent"]}));
    assert_eq!(
        field["missing_workcell_roots"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|v| v.as_str() == Some("workcell:absent"))
            .count(),
        1
    );
    assert_eq!(field["workcells"].as_array().unwrap().len(), 0);
}

#[test]
fn field_refuses_malformed_workcell_filters() {
    let world = World::new();
    let error = execute_at(
        world.root(),
        "now_field",
        &json!({"workcell_refs":["not-a-workcell-ref"]}),
        1_000,
    )
    .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    let error = execute_at(
        world.root(),
        "now_field",
        &json!({"workcell_refs":[42]}),
        1_000,
    )
    .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
}

#[test]
fn field_includes_declared_remote_gateways_without_mirroring_their_ground() {
    let world = World::new();
    world.workcell_root();
    let dir = unique("central-now-field-remote");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("places.json"), CANNED_CENSUS).unwrap();
    fs::write(dir.join("status.json"), CANNED_STATUS).unwrap();
    fs::write(dir.join("instances.json"), CANNED_INSTANCES).unwrap();
    fs::write(dir.join("gateway-status.json"), CANNED_GATEWAY).unwrap();
    fs::write(dir.join("gateway-remote.json"), CANNED_GATEWAY_REMOTES).unwrap();
    let bin = stub_owners(&dir, &[("workcell", &writing_stub()), ("aikit", &writing_stub())]);

    let field = with_path(&bin, || world.field(&json!({})));
    let blocks = field["workcells"].as_array().unwrap();
    let mac = blocks
        .iter()
        .find(|block| block["workcell_ref"] == "workcell:mac")
        .expect("the declared remote appears beside the local root");
    // A remote block is one availability reading, not a mirror: no root NOW,
    // no children, and its material explicitly lives on its own ground.
    assert!(mac["root"].is_null());
    assert_eq!(mac["children_count"], 0);
    assert_eq!(mac["material"]["available"], false);
    assert_eq!(mac["material"]["observation_scope"], "remote-ground");
    // The Gateway behind the declared remote is the addressable part, with
    // the routing provenance the owner's own command carries.
    assert_eq!(mac["gateway"]["available"], true);
    assert_eq!(mac["gateway"]["observation_scope"], "remote-declared");
    assert_eq!(
        mac["gateway"]["status"]["reading"]["data"]["status"]["gateway_ref"],
        "agency-gateway/stub"
    );
    assert_eq!(
        mac["remote_declaration"]["websocket_bind"],
        "100.109.102.82:7788"
    );
    assert!(
        blocks.iter().any(|block| block["workcell_ref"] == "workcell:local"),
        "the local root is still present beside the remote"
    );

    // Omitting the gateway reading omits the remote blocks entirely: nothing
    // is observed that the caller did not ask for.
    let field = with_path(&bin, || world.field(&json!({"include_gateway": false})));
    assert!(
        !field["workcells"]
            .as_array()
            .unwrap()
            .iter()
            .any(|block| block["workcell_ref"] == "workcell:mac")
    );
}

#[test]
fn field_names_unreadable_remote_declarations_instead_of_dropping_them() {
    let world = World::new();
    world.workcell_root();
    let dir = unique("central-now-field-remote-dead");
    fs::create_dir_all(&dir).unwrap();
    // Only the failing stub is on PATH: the remote list cannot be read.
    let bin = stub_owners(&dir, &[("aikit", FAILING_STUB)]);
    let field = with_path(&bin, || world.field(&json!({})));
    let unavailable = &field["gateway_remotes_unavailable"];
    assert_eq!(unavailable["available"], false);
    assert!(
        unavailable["reason"].as_str().unwrap().contains("controlled failure"),
        "the remote-declaration failure is named: {}",
        unavailable["reason"]
    );
    assert!(
        !field["workcells"].as_array().unwrap().iter().any(|block| block["workcell_ref"] == "workcell:mac"),
        "no remote block is invented when the declaration list cannot be read"
    );
}
