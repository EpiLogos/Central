//! Native owner regressions for actual defects observed through the public CLI.
//! Controlled source declarations are fixture setup, never personal adoption.
use central_ctrl::continuous_work::{execute_at, execute_with_token_at, source::Scope};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{fs, path::{Path, PathBuf}, time::{SystemTime, UNIX_EPOCH}};

const AGENT: &str = "controlled-join-agent-not-a-personal-credential";
struct World(PathBuf);
impl Drop for World { fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); } }
impl World {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("central-joined-admission-{}-{}",std::process::id(),SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()));
        central_ctrl::initialize_central(&root).unwrap();
        // Scope::resolve canonicalises the world root (/var -> /private/var on
        // macOS); every path this fixture hands to public operations must live
        // under the canonical root or the membership checks refuse it.
        let root = fs::canonicalize(&root).unwrap();
        fs::create_dir_all(root.join("Work/existing-repository/src")).unwrap();
        let scope = Scope::resolve(&root,None).unwrap();
        let mut relations = vec![];
        for (name,role,body) in [
            ("placement.json","work-placement-policy",json!({"schema":"central.work-placement-policy/v1","scope_ref":"control:root","writable":[{"path":"Work/existing-repository","class":"repository"}],"enforcement":"material-filesystem","required_coverage":["file-content","file-creation"],"lease_seconds":300})),
            ("authority.json","native-action-authority",json!({"schema":"central.native-action-authority/v1","scope_ref":"control:root","grants":[{"principal_ref":"agent:join","actor_kind":"agent","token_sha256":format!("{:x}",Sha256::digest(AGENT.as_bytes())),"scope_refs":["control:root"],"actions":["central.now.lifecycle"],"expires_at_unix_seconds":u64::MAX}]})),
        ] {
            let path = format!("Control/user/{name}");
            fs::write(root.join(&path),serde_json::to_vec_pretty(&body).unwrap()).unwrap();
            relations.push(json!({"ref":scope.source_ref(&path),"path":path,"roles":[role],"provenance":"human-adopted","standing":"architecture-contract","treatment":"projectcentral-user","recognition":"controlled-test-source-not-personal-adoption","recorded_at_unix_seconds":1}));
        }
        fs::create_dir_all(root.join("Control/relations")).unwrap();
        fs::write(root.join("Control/relations/source-relations.json"),serde_json::to_vec_pretty(&json!({"schema":"central.control.ground-relations/v1","project_id":"control:root","relations":relations})).unwrap()).unwrap();
        Self(root)
    }
    fn run(&self, operation: &str, input: Value) -> Value {
        execute_at(&self.0,operation,&input,100).unwrap()
    }
    fn allocate(&self) -> Value {
        let policy = self.run("policy",json!({}));
        self.run("allocate",json!({"task_ref":"task:joined-admission","purpose":"prove the native write basis","participant_refs":["agent:join"],"expected_policy_revision":policy["revision"]}))
    }
    fn validate(&self, allocation: &Value, revision: &Value, path: &Path) -> Value {
        let policy = self.run("policy",json!({}));
        self.run("validate",json!({"now_ref":allocation["now_ref"],"expected_now_revision":revision,"expected_policy_revision":policy["revision"],"destination":path}))
    }
    fn lifecycle(&self, allocation: &Value, revision: &Value, lifecycle: &str) -> Value {
        let policy = self.run("policy",json!({}));
        execute_with_token_at(&self.0,"now_lifecycle",&json!({"now_ref":allocation["now_ref"],"expected_revision":revision,"expected_policy_revision":policy["revision"],"lifecycle":lifecycle}),Some(AGENT),100).unwrap()
    }
}

#[test]
fn every_task_write_requires_active_now_including_normal_repository_edits() {
    for inactive in ["quiescent","closed"] {
        let world = World::new();
        let allocated = world.allocate();
        let source = world.0.join("Work/existing-repository/src/result.rs");
        let artifact = Path::new(allocated["writable_destination"].as_str().unwrap()).join("result.md");
        for path in [&source,&artifact] {
            assert_eq!(world.validate(&allocated,&allocated["revision"]["revision"],path)["allowed"],true);
        }
        let stopped = world.lifecycle(&allocated,&allocated["revision"]["revision"],inactive);
        for path in [&source,&artifact] {
            assert_eq!(world.validate(&allocated,&stopped["revision"]["revision"],path)["allowed"],false,"{inactive}: {}",path.display());
        }
        let resumed = world.lifecycle(&allocated,&stopped["revision"]["revision"],"active");
        assert_eq!(resumed["record"]["now_ref"],allocated["now_ref"]);
        assert_eq!(world.validate(&allocated,&resumed["revision"]["revision"],&source)["allowed"],true);
    }
}

#[test]
fn explicit_protected_artifact_and_directory_win_inside_the_task_now() {
    let world = World::new();
    let allocation = world.allocate();
    let now = PathBuf::from(allocation["writable_destination"].as_str().unwrap());
    let protected = now.join("human-source.txt");
    fs::write(&protected,"untouched human bytes\r\n").unwrap();
    let policy_path = world.0.join("Control/user/placement.json");
    let mut policy: Value = serde_json::from_slice(&fs::read(&policy_path).unwrap()).unwrap();
    policy["protected"] = json!([protected.strip_prefix(&world.0).unwrap(),now.join("private").strip_prefix(&world.0).unwrap()]);
    fs::write(policy_path,serde_json::to_vec_pretty(&policy).unwrap()).unwrap();
    for path in [&protected,&now.join("private/future-source.txt")] {
        assert_eq!(world.validate(&allocation,&allocation["revision"]["revision"],path)["allowed"],false);
    }
    assert_eq!(world.validate(&allocation,&allocation["revision"]["revision"],&now.join("allowed.txt"))["allowed"],true);
    assert_eq!(fs::read_to_string(protected).unwrap(),"untouched human bytes\r\n");
}

#[test]
fn placement_readback_is_explicit_current_and_does_not_publish_source_changes() {
    let world = World::new();
    let allocation = world.allocate();
    let source_path = world.0.join(allocation["source"]["path"].as_str().unwrap());
    let source_before = fs::read(&source_path).unwrap();
    let relation_path = world.0.join("Control/relations/source-relations.json");
    let relations_before = fs::read(&relation_path).unwrap();
    let reading = execute_at(&world.0,"now_read",&json!({"now_ref":allocation["now_ref"],"with_placement":true}),105).unwrap();
    assert_eq!(reading["schema"],"central.now-reading/v1");
    assert_eq!(reading["placement_included"],true);
    assert_eq!(reading["record"],allocation["record"]);
    assert_eq!(reading["source"],allocation["source"]);
    assert_eq!(reading["revision"],allocation["revision"]);
    assert_eq!(reading["writable_destination"],allocation["writable_destination"]);
    assert_eq!(reading["policy"]["revision"],allocation["policy"]["revision"]);
    assert_eq!(reading["policy"]["issued_at_unix_seconds"],105);
    assert!(reading.get("created").is_none());
    assert_eq!(fs::read(&source_path).unwrap(),source_before);
    assert_eq!(fs::read(&relation_path).unwrap(),relations_before);
}

#[test]
fn historical_read_does_not_require_current_policy_and_cannot_mint_missing_material() {
    let world = World::new();
    let allocation = world.allocate();
    fs::write(world.0.join("Control/user/placement.json"),"not a readable current policy").unwrap();
    let history = world.run("now_read",json!({"now_ref":allocation["now_ref"]}));
    assert_eq!(history["record"],allocation["record"]);
    assert!(history.get("policy").is_none());
    assert!(execute_at(&world.0,"now_read",&json!({"now_ref":allocation["now_ref"],"with_placement":true}),100).is_err());
    assert!(execute_at(&world.0,"now_read",&json!({"now_ref":allocation["now_ref"],"with_placement":"true"}),100).is_err());
    let separate = World::new();
    let other = separate.allocate();
    let destination = PathBuf::from(other["writable_destination"].as_str().unwrap());
    fs::remove_dir(&destination).unwrap();
    assert!(execute_at(&separate.0,"now_read",&json!({"now_ref":other["now_ref"],"with_placement":true}),100).is_err());
    assert!(!destination.exists());
}
