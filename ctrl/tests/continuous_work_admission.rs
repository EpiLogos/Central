//! Native owner regressions for actual defects observed through the public CLI.
//! Controlled source declarations are fixture setup, never personal adoption.
use central_ctrl::continuous_work::{execute_at, execute_with_token_at, source::Scope};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

const AGENT: &str = "controlled-join-agent-not-a-personal-credential";
// macOS SystemTime::now() has ~microsecond real granularity, so pid+nanos alone
// collides when several tests in this process start inside the same microsecond
// under parallel load; the per-process sequence makes each root unique.
static WORLD_SEQ: AtomicU64 = AtomicU64::new(0);
struct World(PathBuf);
impl Drop for World {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
impl World {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "central-joined-admission-{}-{}-{}",
            std::process::id(),
            WORLD_SEQ.fetch_add(1, Ordering::Relaxed),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        central_ctrl::initialize_central(&root).unwrap();
        // Scope::resolve canonicalises the world root (/var -> /private/var on
        // macOS); every path this fixture hands to public operations must live
        // under the canonical root or the membership checks refuse it.
        let root = fs::canonicalize(&root).unwrap();
        fs::create_dir_all(root.join("Work/existing-repository/src")).unwrap();
        let scope = Scope::resolve(&root, None).unwrap();
        let mut relations = vec![];
        for (name, role, body) in [
            (
                "placement.json",
                "work-placement-policy",
                json!({"schema":"central.work-placement-policy/v1","scope_ref":"control:root","writable":[{"path":"Work/existing-repository","class":"repository"}],"enforcement":"material-filesystem","required_coverage":["file-content","file-creation"],"lease_seconds":300}),
            ),
            (
                "authority.json",
                "native-action-authority",
                json!({"schema":"central.native-action-authority/v1","scope_ref":"control:root","grants":[{"principal_ref":"agent:join","actor_kind":"agent","token_sha256":format!("{:x}",Sha256::digest(AGENT.as_bytes())),"scope_refs":["control:root"],"actions":["central.now.lifecycle"],"expires_at_unix_seconds":u64::MAX}]}),
            ),
        ] {
            let path = format!("Control/user/{name}");
            fs::write(root.join(&path), serde_json::to_vec_pretty(&body).unwrap()).unwrap();
            relations.push(json!({"ref":scope.source_ref(&path),"path":path,"roles":[role],"provenance":"human-adopted","standing":"architecture-contract","treatment":"projectcentral-user","recognition":"controlled-test-source-not-personal-adoption","recorded_at_unix_seconds":1}));
        }
        fs::create_dir_all(root.join("Control/relations")).unwrap();
        fs::write(root.join("Control/relations/source-relations.json"),serde_json::to_vec_pretty(&json!({"schema":"central.control.ground-relations/v1","project_id":"control:root","relations":relations})).unwrap()).unwrap();
        Self(root)
    }
    fn run(&self, operation: &str, input: Value) -> Value {
        execute_at(&self.0, operation, &input, 100).unwrap()
    }
    fn allocate(&self) -> Value {
        let policy = self.run("policy", json!({}));
        self.run("allocate",json!({"task_ref":"task:joined-admission","purpose":"prove the native write basis","participant_refs":["agent:join"],"expected_policy_revision":policy["revision"]}))
    }
    fn validate(&self, allocation: &Value, revision: &Value, path: &Path) -> Value {
        let policy = self.run("policy", json!({}));
        self.run("validate",json!({"now_ref":allocation["now_ref"],"expected_now_revision":revision,"expected_policy_revision":policy["revision"],"destination":path}))
    }
    fn lifecycle(&self, allocation: &Value, revision: &Value, lifecycle: &str) -> Value {
        let policy = self.run("policy", json!({}));
        execute_with_token_at(&self.0,"now_lifecycle",&json!({"now_ref":allocation["now_ref"],"expected_revision":revision,"expected_policy_revision":policy["revision"],"lifecycle":lifecycle}),Some(AGENT),100).unwrap()
    }
}

#[test]
fn every_task_write_requires_active_now_including_normal_repository_edits() {
    for inactive in ["quiescent", "closed"] {
        let world = World::new();
        let allocated = world.allocate();
        let source = world.0.join("Work/existing-repository/src/result.rs");
        let artifact =
            Path::new(allocated["writable_destination"].as_str().unwrap()).join("result.md");
        for path in [&source, &artifact] {
            assert_eq!(
                world.validate(&allocated, &allocated["revision"]["revision"], path)["allowed"],
                true
            );
        }
        let stopped = world.lifecycle(&allocated, &allocated["revision"]["revision"], inactive);
        for path in [&source, &artifact] {
            assert_eq!(
                world.validate(&allocated, &stopped["revision"]["revision"], path)["allowed"],
                false,
                "{inactive}: {}",
                path.display()
            );
        }
        let resumed = world.lifecycle(&allocated, &stopped["revision"]["revision"], "active");
        assert_eq!(resumed["record"]["now_ref"], allocated["now_ref"]);
        assert_eq!(
            world.validate(&allocated, &resumed["revision"]["revision"], &source)["allowed"],
            true
        );
    }
}

#[test]
fn explicit_protected_artifact_and_directory_win_inside_the_task_now() {
    let world = World::new();
    let allocation = world.allocate();
    let now = PathBuf::from(allocation["writable_destination"].as_str().unwrap());
    let protected = now.join("human-source.txt");
    fs::write(&protected, "untouched human bytes\r\n").unwrap();
    let policy_path = world.0.join("Control/user/placement.json");
    let mut policy: Value = serde_json::from_slice(&fs::read(&policy_path).unwrap()).unwrap();
    policy["protected"] = json!([
        protected.strip_prefix(&world.0).unwrap(),
        now.join("private").strip_prefix(&world.0).unwrap()
    ]);
    fs::write(policy_path, serde_json::to_vec_pretty(&policy).unwrap()).unwrap();
    for path in [&protected, &now.join("private/future-source.txt")] {
        assert_eq!(
            world.validate(&allocation, &allocation["revision"]["revision"], path)["allowed"],
            false
        );
    }
    assert_eq!(
        world.validate(
            &allocation,
            &allocation["revision"]["revision"],
            &now.join("allowed.txt")
        )["allowed"],
        true
    );
    assert_eq!(
        fs::read_to_string(protected).unwrap(),
        "untouched human bytes\r\n"
    );
}

#[test]
fn placement_readback_is_explicit_current_and_does_not_publish_source_changes() {
    let world = World::new();
    let allocation = world.allocate();
    let source_path = world.0.join(allocation["source"]["path"].as_str().unwrap());
    let source_before = fs::read(&source_path).unwrap();
    let relation_path = world.0.join("Control/relations/source-relations.json");
    let relations_before = fs::read(&relation_path).unwrap();
    let reading = execute_at(
        &world.0,
        "now_read",
        &json!({"now_ref":allocation["now_ref"],"with_placement":true}),
        105,
    )
    .unwrap();
    assert_eq!(reading["schema"], "central.now-reading/v1");
    assert_eq!(reading["placement_included"], true);
    assert_eq!(reading["record"], allocation["record"]);
    assert_eq!(reading["source"], allocation["source"]);
    assert_eq!(reading["revision"], allocation["revision"]);
    assert_eq!(
        reading["writable_destination"],
        allocation["writable_destination"]
    );
    assert_eq!(
        reading["policy"]["revision"],
        allocation["policy"]["revision"]
    );
    assert_eq!(reading["policy"]["issued_at_unix_seconds"], 105);
    assert!(reading.get("created").is_none());
    assert_eq!(fs::read(&source_path).unwrap(), source_before);
    assert_eq!(fs::read(&relation_path).unwrap(), relations_before);
}

#[test]
fn historical_read_does_not_require_current_policy_and_cannot_mint_missing_material() {
    let world = World::new();
    let allocation = world.allocate();
    fs::write(
        world.0.join("Control/user/placement.json"),
        "not a readable current policy",
    )
    .unwrap();
    let history = world.run("now_read", json!({"now_ref":allocation["now_ref"]}));
    assert_eq!(history["record"], allocation["record"]);
    assert!(history.get("policy").is_none());
    assert!(execute_at(
        &world.0,
        "now_read",
        &json!({"now_ref":allocation["now_ref"],"with_placement":true}),
        100
    )
    .is_err());
    assert!(execute_at(
        &world.0,
        "now_read",
        &json!({"now_ref":allocation["now_ref"],"with_placement":"true"}),
        100
    )
    .is_err());
    let separate = World::new();
    let other = separate.allocate();
    let destination = PathBuf::from(other["writable_destination"].as_str().unwrap());
    fs::remove_dir(&destination).unwrap();
    assert!(execute_at(
        &separate.0,
        "now_read",
        &json!({"now_ref":other["now_ref"],"with_placement":true}),
        100
    )
    .is_err());
    assert!(!destination.exists());
}

#[test]
fn allocation_states_the_evidence_cache_and_t_bounds_without_creating_the_cache() {
    let world = World::new();
    let allocated = world.allocate();
    let t = PathBuf::from(allocated["writable_destination"].as_str().unwrap());
    let clearing_id = t.parent().unwrap().file_name().unwrap().to_str().unwrap();
    let cache = PathBuf::from(allocated["evidence_cache"].as_str().unwrap());
    assert!(
        cache.ends_with(Path::new("Library/Caches/central/evidence").join(clearing_id)),
        "{}",
        cache.display()
    );
    assert!(
        !cache.starts_with(&world.0),
        "the evidence cache is never inside ground"
    );
    let bounds = &allocated["artifact_bounds"];
    assert_eq!(bounds["max_file_bytes"], 52_428_800);
    assert!(bounds["refused_kinds"]["archive_suffixes"]
        .as_array()
        .unwrap()
        .contains(&json!(".zip")));
    assert_eq!(
        bounds["bulky_evidence"],
        "download to evidence_cache; record finding, source and sha256 in T"
    );
}

#[test]
fn read_reports_t_payloads_and_close_tombstones_them_keeping_findings() {
    let world = World::new();
    let allocated = world.allocate();
    let t = PathBuf::from(allocated["writable_destination"].as_str().unwrap());
    fs::write(t.join("findings.md"), "# finding\nsource: run 42\n").unwrap();
    fs::write(t.join("shot.png"), [0x89u8, b'P', b'N', b'G']).unwrap();
    fs::write(t.join("ci-artifacts.ZIP"), b"zip bytes").unwrap();
    let big = fs::File::create(t.join("bundle.bin")).unwrap();
    big.set_len(52_428_800).unwrap();
    drop(big);
    fs::create_dir_all(t.join("app/node_modules/pkg")).unwrap();
    fs::write(t.join("app/node_modules/pkg/index.js"), "x").unwrap();
    fs::create_dir_all(t.join("build/target/debug")).unwrap();
    fs::write(
        t.join("build/target/CACHEDIR.TAG"),
        "Signature: 8a477f597d28d172789f06886806bc55\n",
    )
    .unwrap();
    fs::write(t.join("build/target/debug/ctrl"), b"binary").unwrap();
    // A symlinked directory is never followed: its payload is not T's.
    let outside = world.0.join("Work/existing-repository/outside");
    fs::create_dir_all(&outside).unwrap();
    fs::write(outside.join("elsewhere.zip"), b"not ours").unwrap();
    std::os::unix::fs::symlink(&outside, t.join("linked")).unwrap();

    let read = world.run("now_read", json!({"now_ref":allocated["now_ref"]}));
    let violations = &read["t_payload_violations"];
    assert_eq!(violations["count"], 4, "{violations}");
    assert_eq!(violations["largest"][0]["kind"], "large-file");
    assert!(violations["total_bytes"].as_u64().unwrap() >= 52_428_800);
    assert!(!violations.to_string().contains("elsewhere.zip"));

    let closed = world.lifecycle(&allocated, &allocated["revision"]["revision"], "closed");
    assert_eq!(closed["artifacts_deleted"], true);
    assert_eq!(closed["payloads_tombstoned"].as_array().unwrap().len(), 4);
    assert_eq!(closed["payload_tend_failures"], json!([]));
    for kept in ["findings.md", "shot.png"] {
        assert!(t.join(kept).is_file(), "{kept} is source, not payload");
    }
    for gone in [
        "ci-artifacts.ZIP",
        "bundle.bin",
        "app/node_modules",
        "build/target",
    ] {
        assert!(!t.join(gone).exists(), "{gone} should be tombstoned");
        let stone: Value =
            serde_json::from_slice(&fs::read(t.join(format!("{gone}.swept.json"))).unwrap())
                .unwrap();
        assert_eq!(stone["schema"], "central.swept-payload/v1");
    }
    let zip: Value =
        serde_json::from_slice(&fs::read(t.join("ci-artifacts.ZIP.swept.json")).unwrap()).unwrap();
    assert_eq!(zip["name"], "ci-artifacts.ZIP");
    assert_eq!(zip["bytes"], 9);
    assert_eq!(zip["sha256"], format!("{:x}", Sha256::digest(b"zip bytes")));
    let tree: Value =
        serde_json::from_slice(&fs::read(t.join("build/target.swept.json")).unwrap()).unwrap();
    assert_eq!(tree["sha256"], Value::Null);
    assert_eq!(tree["bytes"], 50, "CACHEDIR.TAG (44) + debug/ctrl (6)");
    assert!(outside.join("elsewhere.zip").is_file());
    assert!(fs::symlink_metadata(t.join("linked"))
        .unwrap()
        .file_type()
        .is_symlink());

    let reread = world.run("now_read", json!({"now_ref":allocated["now_ref"]}));
    assert_eq!(reread["t_payload_violations"]["count"], 0);
}
