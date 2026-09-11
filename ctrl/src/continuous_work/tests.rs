use super::*;
use std::fs;
use std::sync::{Arc, Barrier};
use std::thread;

pub(super) fn world() -> crate::TempDir {
    let temp = crate::tempdir().unwrap();
    for path in ["Control/user", "Control/relations", "Control/agents", "Work/one/ProjectCentral/user", "Work/two/ProjectCentral/user", "Work/external/.git"] {
        fs::create_dir_all(temp.path().join(path)).unwrap();
    }
    for name in ["one", "two"] {
        fs::write(temp.path().join(format!("Work/{name}/ProjectCentral/project.json")), serde_json::to_vec(&crate::projectcentral::ProjectCentralManifest::new(format!("test/{name}"))).unwrap()).unwrap();
    }
    let scope = Scope::resolve(temp.path(), None).unwrap();
    let policy_path = "Control/user/placement-policy.json";
    let policy = json!({
        "schema":placement::POLICY_SCHEMA,"scope_ref":"control:root",
        "authority_refs":[],"writable":[
            {"path":"Work/one","class":"repository"},
            {"path":"Work/two","class":"repository"},
            {"path":"Work/external","class":"repository"}
        ],"protected":[],"enforcement":"harness-interception",
        "required_coverage":["filesystem"],"lease_seconds":300,"expires_at_unix_seconds":9999
    });
    fs::write(temp.path().join(policy_path), source::encoded(&policy).unwrap()).unwrap();
    fs::write(temp.path().join(&scope.relations_path), source::encoded(&json!({
        "schema":scope.relations_schema,"project_id":scope.relations_id,"relations":[{
            "ref":scope.source_ref(policy_path),"path":policy_path,"roles":[placement::POLICY_ROLE],
            "provenance":"human-adopted","standing":"architecture-contract","treatment":"projectcentral-user",
            "recognition":"controlled-test-fixture-not-personal-adoption","recorded_at_unix_seconds":1
        }]
    })).unwrap()).unwrap();
    temp
}
fn policy(root: &Path, project: Option<&str>) -> Value {
    execute_at(root, "policy", &json!({"project":project}), 100).unwrap()
}
fn request(root: &Path, project: Option<&str>, task: &str) -> Value {
    json!({"project":project,"task_ref":task,"purpose":"bounded implementation","participant_refs":["agent:test"],"source_refs":[],"expected_policy_revision":policy(root,project)["revision"]})
}
fn validation(allocation: &Value, destination: &str, project: Option<&str>) -> Value {
    json!({"project":project,"now_ref":allocation["now_ref"],"expected_now_revision":allocation["revision"]["revision"],"expected_policy_revision":allocation["policy"]["revision"],"destination":destination})
}

#[test]
fn concurrent_same_task_allocation_publishes_one_identical_source() {
    let temp = world();
    let input = request(temp.path(), None, "task:concurrent");
    let barrier = Arc::new(Barrier::new(8));
    let handles: Vec<_> = (0..8).map(|_| {
        let root = temp.path().to_path_buf(); let input = input.clone(); let barrier = barrier.clone();
        thread::spawn(move || { barrier.wait(); execute_at(&root,"allocate",&input,100).unwrap() })
    }).collect();
    let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    assert_eq!(results.iter().filter(|r| r["created"] == true).count(), 1);
    for result in &results {
        assert_eq!(result["now_ref"], results[0]["now_ref"]);
        assert_eq!(result["revision"], results[0]["revision"]);
        assert!(Path::new(result["writable_destination"].as_str().unwrap()).is_dir());
    }
    let scope = Scope::resolve(temp.path(),None).unwrap();
    assert_eq!(scope.bindings().unwrap().iter().filter(|b| b.roles.iter().any(|r| r=="now-clearing")).count(),1);
    let horizon = crate::source_horizon::reconcile_control_sources(temp.path()).unwrap();
    assert!(horizon.horizon.sources.iter().any(|s| s.binding.source_ref == results[0]["source"]["ref"].as_str().unwrap()));
}
#[test]
fn distinct_tasks_and_root_two_projects_do_not_share_scratch_or_identity() {
    let temp = world();
    let mut refs = std::collections::BTreeSet::new();
    for project in [None, Some("one"), Some("two")] {
        for task in ["task:a", "task:b"] {
            let result = execute_at(temp.path(),"allocate",&request(temp.path(),project,task),100).unwrap();
            assert!(refs.insert(result["now_ref"].as_str().unwrap().to_owned()));
            let path = result["writable_destination"].as_str().unwrap();
            assert!(path.contains(if project.is_none() {"Control/agents/now/clearings/"} else {"ProjectCentral/agents/now/clearings/"}));
            assert!(!path.contains("ProjectCentral/now/user"));
        }
    }
}
#[test]
fn normal_repository_and_external_repository_writes_are_not_now_only() {
    let temp = world();
    let allocation = execute_at(temp.path(),"allocate",&request(temp.path(),None,"task:write"),100).unwrap();
    for path in ["Work/one/README.md", "Work/one/src/main.rs", "Work/one/target/output", "Work/external/README.md"] {
        let result = execute_at(temp.path(),"validate",&validation(&allocation,path,None),100).unwrap();
        assert_eq!(result["allowed"],true,"{path}: {result}");
    }
    assert!(!temp.path().join("Work/external/ProjectCentral").exists());
    assert_eq!(allocation["policy"]["outside_writes_prevented"],false);
}
#[test]
fn loose_work_root_scratch_and_human_sources_are_rejected_with_usable_now() {
    let temp = world();
    let allocation = execute_at(temp.path(),"allocate",&request(temp.path(),None,"task:guard"),100).unwrap();
    for path in ["Work/README-draft.md","Work/empty.diff","Control/user/placement-policy.json","Work/one/ProjectCentral/user/day.md","Work/one/.git/config"] {
        let result = execute_at(temp.path(),"validate",&validation(&allocation,path,None),100).unwrap();
        assert_eq!(result["allowed"],false,"{path}");
        assert_eq!(result["valid_now_destination"],allocation["writable_destination"]);
    }
    let target = format!("{}/plan.md",allocation["writable_destination"].as_str().unwrap());
    assert_eq!(execute_at(temp.path(),"validate",&validation(&allocation,&target,None),100).unwrap()["allowed"],true);
}
#[test]
fn stale_policy_and_now_sources_refuse_before_any_widening() {
    let temp = world();
    let input = request(temp.path(),None,"task:stale");
    let allocation = execute_at(temp.path(),"allocate",&input,100).unwrap();
    let now_path = temp.path().join(allocation["source"]["path"].as_str().unwrap());
    let original = fs::read(&now_path).unwrap();
    let mut changed = String::from_utf8(original.clone()).unwrap(); changed.push('\n');
    fs::write(&now_path,changed).unwrap();
    let error = execute_at(temp.path(),"validate",&validation(&allocation,"Work/one/README.md",None),100).unwrap_err();
    assert_eq!(error.kind(),io::ErrorKind::AlreadyExists);
    fs::write(&now_path,&original).unwrap();
    let path = temp.path().join("Control/user/placement-policy.json");
    let mut policy = fs::read_to_string(&path).unwrap(); policy.push('\n'); fs::write(path,policy).unwrap();
    assert_eq!(execute_at(temp.path(),"allocate",&input,100).unwrap_err().kind(),io::ErrorKind::AlreadyExists);
    assert_eq!(fs::read(&now_path).unwrap(),original);
}
#[test]
fn draft_unreadable_and_expired_policy_never_fall_back_to_permissive_defaults() {
    let temp = world();
    let path = temp.path().join("Control/relations/source-relations.json");
    let original = fs::read_to_string(&path).unwrap();
    let mut relation: Value = serde_json::from_str(&original).unwrap();
    relation["relations"][0]["provenance"] = json!("generated-suggestion");
    fs::write(&path,source::encoded(&relation).unwrap()).unwrap();
    assert_eq!(execute_at(temp.path(),"policy",&json!({}),100).unwrap_err().kind(),io::ErrorKind::PermissionDenied);
    fs::write(&path,original).unwrap();
    assert_eq!(execute_at(temp.path(),"policy",&json!({}),10000).unwrap_err().kind(),io::ErrorKind::PermissionDenied);
    fs::write(temp.path().join("Control/user/placement-policy.json"),"{broken").unwrap();
    assert!(execute_at(temp.path(),"policy",&json!({}),100).is_err());
    assert!(!temp.path().join("Control/agents/now/clearings").exists());
}
#[test]
fn project_policy_narrows_root_and_detects_a_stale_parent() {
    let temp = world();
    let scope = Scope::resolve(temp.path(),Some("one")).unwrap();
    let root = policy(temp.path(),None);
    let local = json!({"schema":placement::POLICY_SCHEMA,"scope_ref":scope.world_ref,"parent_policy":{"source_ref":root["sources"][0]["source"]["ref"],"revision":root["sources"][0]["revision"]},"authority_refs":[],"writable":[{"path":"src","class":"repository"}],"protected":[],"enforcement":"material-filesystem","required_coverage":["filesystem"],"lease_seconds":60});
    let path = "ProjectCentral/user/placement.json";
    fs::write(scope.root.join(path),source::encoded(&local).unwrap()).unwrap();
    fs::create_dir_all(scope.root.join("ProjectCentral/relations")).unwrap();
    fs::write(scope.root.join(&scope.relations_path),source::encoded(&json!({"schema":scope.relations_schema,"project_id":scope.relations_id,"relations":[{"ref":scope.source_ref(path),"path":path,"roles":[placement::POLICY_ROLE],"provenance":"human-adopted","standing":"architecture-contract","treatment":"projectcentral-user","recognition":"test-local-adoption","recorded_at_unix_seconds":1}]})).unwrap()).unwrap();
    let reading = policy(temp.path(),Some("one"));
    assert_eq!(reading["sources"].as_array().unwrap().len(),2);
    assert_eq!(reading["enforcement"],"material-filesystem");
    assert_eq!(reading["expires_at_unix_seconds"],160);
    let root_path = temp.path().join("Control/user/placement-policy.json");
    let mut changed = fs::read_to_string(&root_path).unwrap(); changed.push('\n'); fs::write(root_path,changed).unwrap();
    assert_eq!(execute_at(temp.path(),"policy",&json!({"project":"one"}),100).unwrap_err().kind(),io::ErrorKind::AlreadyExists);
}
#[test]
fn symlink_and_changed_destination_anchor_are_refused() {
    let temp = world(); let outside = crate::tempdir().unwrap();
    let allocation = execute_at(temp.path(),"allocate",&request(temp.path(),None,"task:paths"),100).unwrap();
    std::os::unix::fs::symlink(outside.path(),temp.path().join("Work/one/escape")).unwrap();
    assert!(execute_at(temp.path(),"validate",&validation(&allocation,"Work/one/escape/human.md",None),100).is_err());
    let mut input = validation(&allocation,"Work/one/new/file.md",None);
    let preview = execute_at(temp.path(),"validate",&input,100).unwrap();
    input["expected_destination_anchor"] = preview["destination_anchor"].clone();
    fs::create_dir_all(temp.path().join("Work/one/new")).unwrap();
    assert_eq!(execute_at(temp.path(),"validate",&input,100).unwrap_err().kind(),io::ErrorKind::AlreadyExists);
    assert!(!outside.path().join("human.md").exists());
}
#[test]
fn interrupted_allocation_rebinds_original_source_without_changing_bytes() {
    let temp = world(); let input = request(temp.path(),None,"task:recovery");
    let first = execute_at(temp.path(),"allocate",&input,100).unwrap();
    let path = temp.path().join(first["source"]["path"].as_str().unwrap());
    let bytes = fs::read(&path).unwrap();
    let rel_path = temp.path().join("Control/relations/source-relations.json");
    let mut relations: Value = serde_json::from_slice(&fs::read(&rel_path).unwrap()).unwrap();
    relations["relations"].as_array_mut().unwrap().retain(|r| r["ref"] != first["source"]["ref"]);
    fs::write(rel_path,source::encoded(&relations).unwrap()).unwrap();
    let resumed = execute_at(temp.path(),"allocate",&input,101).unwrap();
    assert_eq!(resumed["now_ref"],first["now_ref"]);
    assert_eq!(resumed["revision"],first["revision"]);
    assert_eq!(resumed["record"]["created_at_unix_seconds"],100);
    assert_eq!(fs::read(path).unwrap(),bytes);
}
#[test]
fn same_task_different_purpose_is_a_conflict_not_a_shared_scratch_overwrite() {
    let temp = world(); let mut input = request(temp.path(),None,"task:identity");
    execute_at(temp.path(),"allocate",&input,100).unwrap();
    input["purpose"] = json!("a different task intent");
    assert_eq!(execute_at(temp.path(),"allocate",&input,100).unwrap_err().kind(),io::ErrorKind::AlreadyExists);
}
