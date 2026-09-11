use central_ctrl::continuous_work::{execute_at, execute_with_token_at, source::Scope};
use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{fs, io, path::{Path, PathBuf}, process::Command, sync::atomic::{AtomicU64, Ordering}, time::{SystemTime, UNIX_EPOCH}};

static NEXT: AtomicU64 = AtomicU64::new(0);
const HUMAN: &str = "controlled-test-human-credential-not-a-real-secret";
const AGENT: &str = "controlled-test-agent-credential-not-a-real-secret";
struct World(PathBuf);
impl World { fn path(&self) -> &Path { &self.0 } }
impl Drop for World { fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); } }
fn world() -> World {
    let path = std::env::temp_dir().join(format!("central-native-caw-{}-{}-{}",std::process::id(),SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos(),NEXT.fetch_add(1,Ordering::Relaxed)));
    central_ctrl::initialize_central(&path).unwrap();
    let world = World(path);
    for name in ["one","two"] {
        let root = world.path().join(format!("Work/{name}"));
        fs::create_dir_all(root.join("ProjectCentral/user")).unwrap();
        fs::write(root.join("ProjectCentral/project.json"),serde_json::to_vec(&central_ctrl::ProjectCentralManifest::new(format!("test/{name}"))).unwrap()).unwrap();
        fs::create_dir_all(root.join("src")).unwrap();
    }
    let grants: Vec<Value> = [HUMAN, AGENT].iter().enumerate().map(|(index, secret)| {
        json!({"principal_ref":if index==0 {"human:test"} else {"agent:test"},"actor_kind":if index==0 {"human"} else {"agent"},"token_sha256":format!("{:x}",Sha256::digest(secret.as_bytes())),"scope_refs":["control:root","project:test/one","project:test/two"],"actions":["central.day.ensure","central.day.lifecycle","central.now.lifecycle","central.now.obligations"],"expires_at_unix_seconds":u64::MAX})
    }).collect();
    let policies = [
        ("placement.json","work-placement-policy",json!({"schema":"central.work-placement-policy/v1","scope_ref":"control:root","writable":[{"path":"Work/one","class":"repository"},{"path":"Work/two","class":"repository"}],"enforcement":"native-actions","required_coverage":["file-content"],"lease_seconds":300})),
        ("time.json","civil-time-policy",json!({"schema":"central.civil-time-policy/v1","scope_ref":"control:root","timezone":"Europe/London","day_boundary_minutes":240,"automatic_day_rollover":true})),
        ("authority.json","native-action-authority",json!({"schema":"central.native-action-authority/v1","scope_ref":"control:root","grants":grants})),
    ];
    let scope = Scope::resolve(world.path(),None).unwrap();
    let mut relations = vec![];
    for (name,role,value) in policies {
        let path = format!("Control/user/{name}");
        fs::write(world.path().join(&path),serde_json::to_vec_pretty(&value).unwrap()).unwrap();
        relations.push(json!({"ref":scope.source_ref(&path),"path":path,"roles":[role],"provenance":"human-adopted","standing":"architecture-contract","treatment":"projectcentral-user","recognition":"explicit-controlled-test-fixture-not-personal-adoption","recorded_at_unix_seconds":1}));
    }
    fs::create_dir_all(world.path().join("Control/relations")).unwrap();
    fs::write(world.path().join("Control/relations/source-relations.json"),serde_json::to_vec_pretty(&json!({"schema":"central.control.ground-relations/v1","project_id":"control:root","relations":relations})).unwrap()).unwrap();
    world
}
fn at(value: &str) -> u64 { DateTime::parse_from_rfc3339(value).unwrap().with_timezone(&Utc).timestamp() as u64 }
fn run(world: &World, operation: &str, input: &Value, token: &str, now: u64) -> io::Result<Value> {
    execute_with_token_at(world.path(),operation,input,Some(token),now)
}
fn allocate(world: &World, project: Option<&str>, now: u64) -> Value {
    let policy = execute_at(world.path(),"policy",&json!({"project":project}),now).unwrap();
    execute_at(world.path(),"allocate",&json!({"project":project,"task_ref":"task:unchanged-by-day","purpose":"independent NOW lifecycle","expected_policy_revision":policy["revision"]}),now).unwrap()
}
fn ensure(world: &World, project: Option<&str>, now: u64) -> Value {
    let time = execute_at(world.path(),"time_policy",&json!({"project":project}),now).unwrap();
    run(world,"day_ensure",&json!({"project":project,"expected_time_policy_revision":time["revision"]}),AGENT,now).unwrap()
}
fn lifecycle(world: &World, allocation: &Value, current_revision: &Value, state: &str, now: u64) -> Value {
    let policy = execute_at(world.path(),"policy",&json!({}),now).unwrap();
    run(world,"now_lifecycle",&json!({"now_ref":allocation["now_ref"],"expected_revision":current_revision,"expected_policy_revision":policy["revision"],"lifecycle":state}),AGENT,now).unwrap()
}

#[test]
fn civil_day_rollover_does_not_close_writing_rewrite_tasks_or_clear_now() {
    let world = world();
    let before = at("2026-03-29T02:59:59Z");
    let after = at("2026-03-29T03:00:00Z");
    let now = allocate(&world,None,before);
    let original_now = fs::read(world.path().join(now["source"]["path"].as_str().unwrap())).unwrap();
    let day = ensure(&world,None,before);
    assert_eq!(day["temporal"]["civil_date"],"2026-03-28");
    let day_path = world.path().join(day["source"]["path"].as_str().unwrap());
    let human_bytes = "# Still writing\r\n- [ ] Unfinished task\r\nλ and human spacing  \r\n";
    fs::write(&day_path,human_bytes).unwrap();
    let next = ensure(&world,None,after);
    assert_eq!(next["temporal"]["civil_date"],"2026-03-29");
    assert_eq!(next["content"],"");
    assert_eq!(next["today_advanced"],true);
    for key in ["prior_writing_closed","open_editor_changed","tasks_carried_or_ticked","now_cleared_or_archived","automatic_agent_or_model_invocation"] { assert_eq!(next[key],false,"{key}"); }
    assert_eq!(fs::read_to_string(day_path).unwrap(),human_bytes);
    assert_eq!(fs::read(world.path().join(now["source"]["path"].as_str().unwrap())).unwrap(),original_now);
    let old = execute_at(world.path(),"day_read",&json!({"day_ref":day["day_ref"]}),after).unwrap();
    assert_eq!(old["temporal"]["lifecycle"],"open");
    let backward = ensure(&world,None,before);
    assert_eq!(backward["today_advanced"],false);
    assert_eq!(backward["today"]["day_ref"],next["day_ref"]);
}
#[test]
fn daylight_saving_repeated_hour_and_project_days_use_the_same_root_policy() {
    let world = world();
    for instant in ["2026-10-25T00:30:00Z","2026-10-25T01:30:00Z"] {
        let now = at(instant);
        let root = execute_at(world.path(),"time_policy",&json!({}),now).unwrap();
        for project in ["one","two"] {
            let local = execute_at(world.path(),"time_policy",&json!({"project":project}),now).unwrap();
            assert_eq!(local["revision"],root["revision"]);
            assert_eq!(local["civil_date"],"2026-10-24");
            let day = ensure(&world,Some(project),now);
            assert!(day["source"]["path"].as_str().unwrap().starts_with("ProjectCentral/user/day/"));
        }
    }
}
#[test]
fn human_label_does_not_authenticate_day_closure_and_agent_cannot_close_it() {
    let world = world(); let day = ensure(&world,None,100);
    let input = json!({"day_ref":day["day_ref"],"expected_revision":day["revision"]["revision"],"expected_relations_revision":day["relations_revision"],"lifecycle":"closed","actor_kind":"human","author":"H"});
    assert_eq!(execute_at(world.path(),"day_lifecycle",&input,100).unwrap_err().kind(),io::ErrorKind::PermissionDenied);
    assert_eq!(run(&world,"day_lifecycle",&input,AGENT,100).unwrap_err().kind(),io::ErrorKind::PermissionDenied);
    let closed = run(&world,"day_lifecycle",&input,HUMAN,100).unwrap();
    assert_eq!(closed["temporal"]["lifecycle"],"closed");
    assert_eq!(closed["revision"],day["revision"]);
    assert_eq!(run(&world,"day_lifecycle",&input,HUMAN,100).unwrap_err().kind(),io::ErrorKind::AlreadyExists);
}
#[test]
fn stale_time_policy_and_changed_authority_fail_without_source_effects() {
    let world = world(); let time = execute_at(world.path(),"time_policy",&json!({}),100).unwrap();
    let path = world.path().join("Control/user/time.json");
    let mut content = fs::read_to_string(&path).unwrap(); content.push('\n'); fs::write(path,content).unwrap();
    assert_eq!(run(&world,"day_ensure",&json!({"expected_time_policy_revision":time["revision"]}),AGENT,100).unwrap_err().kind(),io::ErrorKind::AlreadyExists);
    assert!(!world.path().join("Control/user/day").exists());
    let current = execute_at(world.path(),"time_policy",&json!({}),100).unwrap();
    assert_eq!(run(&world,"day_ensure",&json!({"expected_time_policy_revision":current["revision"],"expected_authority_revision":"stale"}),AGENT,100).unwrap_err().kind(),io::ErrorKind::AlreadyExists);
    assert!(!world.path().join("Control/user/day").exists());
}
#[test]
fn now_archive_reentry_keeps_identity_artifacts_and_native_history() {
    let world = world(); let allocation = allocate(&world,None,100);
    let artifact = Path::new(allocation["writable_destination"].as_str().unwrap()).join("retained.md");
    fs::write(&artifact,"retained working material").unwrap();
    let closed = lifecycle(&world,&allocation,&allocation["revision"]["revision"],"closed",101);
    let archived = lifecycle(&world,&allocation,&closed["revision"]["revision"],"archived",102);
    assert!(archived["record"]["archive_ref"].is_string());
    let resumed = lifecycle(&world,&allocation,&archived["revision"]["revision"],"active",103);
    assert_eq!(resumed["record"]["now_ref"],allocation["now_ref"]);
    assert_eq!(fs::read_to_string(artifact).unwrap(),"retained working material");
    let history = execute_at(world.path(),"source_history",&json!({"source_ref":allocation["source"]["ref"]}),103).unwrap();
    assert_eq!(history["entries"].as_array().unwrap().len(),3);
    assert_eq!(history["entries"][0]["actor"],"agent:test");
    assert_eq!(history["entries"][0]["previous_revision"],archived["revision"]["revision"]);
}
#[test]
fn pending_receiving_survives_closure_and_blocks_now_archive() {
    let world = world(); let allocation = allocate(&world,None,100);
    let closed = lifecycle(&world,&allocation,&allocation["revision"]["revision"],"closed",101);
    fs::create_dir_all(world.path().join(".central/source-returns/contributions")).unwrap();
    fs::write(world.path().join(".central/source-returns/contributions/pending.json"),serde_json::to_vec(&json!({"return_ref":"return:pending","now_ref":allocation["now_ref"],"status":"needs-review"})).unwrap()).unwrap();
    let policy = execute_at(world.path(),"policy",&json!({}),102).unwrap();
    let error = run(&world,"now_lifecycle",&json!({"now_ref":allocation["now_ref"],"expected_revision":closed["revision"]["revision"],"expected_policy_revision":policy["revision"],"lifecycle":"archived"}),AGENT,102).unwrap_err();
    assert_eq!(error.kind(),io::ErrorKind::AlreadyExists);
    let read = execute_at(world.path(),"now_read",&json!({"now_ref":allocation["now_ref"]}),102).unwrap();
    assert_eq!(read["record"]["lifecycle"],"closed");
    assert!(world.path().join(".central/source-returns/contributions/pending.json").exists());
}
#[test]
fn real_cli_binary_roundtrips_policy_allocation_source_and_rejection() {
    let world = world();
    let invoke = |action: &str,input: Value| {
        let output = Command::new(env!("CARGO_BIN_EXE_ctrl")).args(["--json","--root",world.path().to_str().unwrap(),"action","run",action,&input.to_string()]).env_remove("CENTRAL_NATIVE_TOKEN").output().unwrap();
        serde_json::from_slice::<Value>(&output.stdout).unwrap_or_else(|error| panic!("{error}: {} / {}",String::from_utf8_lossy(&output.stdout),String::from_utf8_lossy(&output.stderr)))
    };
    let policy = invoke("central.work.policy",json!({})); assert_eq!(policy["ok"],true);
    let request = json!({"task_ref":"task:real-native-binary","purpose":"public consumer boundary","expected_policy_revision":policy["data"]["revision"]});
    let first = invoke("central.now.allocate",request.clone()); assert_eq!(first["ok"],true,"{first}");
    let second = invoke("central.now.allocate",request); assert_eq!(second["ok"],true);
    assert_eq!(first["data"]["now_ref"],second["data"]["now_ref"]);
    assert_eq!(first["data"]["revision"],second["data"]["revision"]);
    let read = invoke("central.now.read",json!({"now_ref":first["data"]["now_ref"]}));
    assert_eq!(read["data"]["revision"],first["data"]["revision"]);
    for (destination,allowed) in [("Work/one/src/main.rs",true),("Work/root-scratch.diff",false)] {
        let response = invoke("central.work.validate",json!({"destination":destination,"now_ref":first["data"]["now_ref"],"expected_now_revision":first["data"]["revision"]["revision"],"expected_policy_revision":policy["data"]["revision"]}));
        assert_eq!(response["ok"],allowed,"{response}");
    }
    println!("native-consumer-policy={policy}");
    println!("native-consumer-allocation={first}");
}
