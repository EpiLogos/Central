//! Native regression cases for the continuation of #151/#152. All Worlds and
//! credentials are disposable. No private Control or installed profile is read.
use central_ctrl::continuous_work::{execute_at, execute_with_token_at, source::Scope};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, fs, io, path::{Path, PathBuf}, process::Command,
    sync::{Arc, Barrier, atomic::{AtomicU64, Ordering}}, thread, time::{SystemTime, UNIX_EPOCH}};

const HUMAN: &str = "continuation-human-fixture-not-a-personal-credential";
const AGENT: &str = "continuation-agent-fixture-not-a-personal-credential";
static NEXT: AtomicU64 = AtomicU64::new(0);
struct World(PathBuf);
impl World { fn path(&self) -> &Path { &self.0 } }
impl Drop for World { fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); } }
fn call(root: &Path, op: &str, input: &Value, token: &str) -> io::Result<Value> {
    execute_with_token_at(root, op, input, Some(token), 100)
}
fn cli(root: &Path, action: &str, input: &Value, token: &str) -> Value {
    let out = Command::new(env!("CARGO_BIN_EXE_ctrl"))
        .args(["--json", "--root", root.to_str().unwrap(), "action", "run", action, &input.to_string()])
        .env("CENTRAL_NATIVE_TOKEN", token).env_remove("CENTRAL_ROOT").output().unwrap();
    let value: Value = serde_json::from_slice(&out.stdout).unwrap_or_else(|_| panic!("{}", String::from_utf8_lossy(&out.stderr)));
    assert!(out.status.success() && value["ok"] == true, "{value}");
    value["data"].clone()
}
fn world() -> World {
    let path = std::env::temp_dir().join(format!("central-continuation-{}-{}-{}", std::process::id(), SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos(), NEXT.fetch_add(1,Ordering::Relaxed)));
    central_ctrl::initialize_central(&path).unwrap();
    for name in ["one","two"] {
        let project=path.join(format!("Work/{name}/ProjectCentral"));
        fs::create_dir_all(project.join("user")).unwrap();
        fs::write(project.join("project.json"),serde_json::to_vec(&central_ctrl::ProjectCentralManifest::new(format!("test/{name}"))).unwrap()).unwrap();
    }
    let grants: Vec<_> = [(HUMAN,"human:test","human"),(AGENT,"agent:test","agent")].into_iter().map(|(token,principal,kind)| json!({
        "principal_ref":principal,"actor_kind":kind,"token_sha256":format!("{:x}",Sha256::digest(token.as_bytes())),
        "scope_refs":["control:root","project:test/one","project:test/two"],
        "actions":["central.day.ensure","central.day.lifecycle","central.now.lifecycle","central.document.create","central.document.mutate","central.receiving.submit","central.receiving.list","central.receiving.review","central.receiving.include","central.receiving.recover"],"expires_at_unix_seconds":9999999999u64
    })).collect();
    let policies = [
        ("placement.json","work-placement-policy",json!({"schema":"central.work-placement-policy/v1","scope_ref":"control:root","writable":[{"path":"Work/one","class":"repository"},{"path":"Work/two","class":"repository"}],"enforcement":"native-actions","required_coverage":["file-content"],"lease_seconds":300})),
        ("time.json","civil-time-policy",json!({"schema":"central.civil-time-policy/v1","scope_ref":"control:root","timezone":"Europe/London","day_boundary_minutes":0,"automatic_day_rollover":true})),
        ("authority.json","native-action-authority",json!({"schema":"central.native-action-authority/v1","scope_ref":"control:root","grants":grants})),
    ];
    let scope=Scope::resolve(&path,None).unwrap(); let mut relations=vec![];
    for (name,role,value) in policies {
        let source=format!("Control/user/{name}");
        fs::write(path.join(&source),serde_json::to_vec_pretty(&value).unwrap()).unwrap();
        relations.push(json!({"ref":scope.source_ref(&source),"path":source,"roles":[role],"provenance":"human-adopted","standing":"architecture-contract","treatment":"projectcentral-user","recognition":"controlled-fixture-only"}));
    }
    fs::create_dir_all(path.join("Control/relations")).unwrap();
    fs::write(path.join("Control/relations/source-relations.json"),serde_json::to_vec_pretty(&json!({"schema":"central.control.ground-relations/v1","project_id":"control:root","relations":relations})).unwrap()).unwrap();
    World(path)
}
fn doc(root: &Path, project: Option<&str>, id: &str) -> Value {
    let policy=execute_at(root,"policy",&json!({"project":project}),100).unwrap();
    call(root,"document_create",&json!({"project":project,"kind":"flow","document_id":id,"title":"A retained document","expected_policy_revision":policy["revision"],
        "template_payload":{"supplied_test_key":"Literal human text","unknown":{"keep":[1,2,3]}},
        "fields":[{"id":"fixture","label":"Supplied literal fixture","template_pointer":"/supplied_test_key"}]}),HUMAN).unwrap()
}
fn input(doc: &Value, project: Option<&str>, request: &str, operation: &str) -> Value {
    json!({"project":project,"source_ref":doc["source"]["ref"],"document_id":doc["document_id"],"expected_revision":doc["revision"]["revision"],"request_id":request,"operation":operation})
}
fn read(root: &Path, doc: &Value, project: Option<&str>) -> Value {
    execute_at(root,"document_read",&json!({"project":project,"source_ref":doc["source"]["ref"],"document_id":doc["document_id"]}),100).unwrap()
}
fn add(root: &Path, doc: &Value, project: Option<&str>, id: &str) -> Value {
    let mut i=input(doc,project,id,"entry.add"); i["entry_id"]=json!(id); i["contribution_id"]=json!(format!("{id}:part")); i["html"]=json!("<p>Original passage</p>");
    call(root,"document_mutate",&i,AGENT).unwrap()
}
fn submit_input(doc: &Value, project: Option<&str>, key: &str) -> Value {
    json!({"project":project,"producer_key":key,"source_ref":doc["source"]["ref"],"document_id":doc["document_id"],"expected_source_revision":doc["revision"]["revision"],"occurred_at_unix_seconds":42,"summary":"What changed; still awaiting human review.","proposal":{"operation":"entry.add","entry_id":key,"contribution_id":format!("{key}:part"),"html":"<p>Returned work</p>"}})
}
fn patch_external(root: &Path, doc: &Value, project: Option<&str>) -> (Value, String) {
    let scope=Scope::resolve(root,project).unwrap();
    let path=scope.root.join(doc["source"]["path"].as_str().unwrap());
    let mut value:Value=serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    value["contributions"][0]["html"]=json!("<p>Human's external revision</p>");
    let raw=serde_json::to_string_pretty(&value).unwrap(); fs::write(&path,&raw).unwrap();
    (read(root,doc,project), raw)
}
fn contains_snapshot(dir: &Path, raw: &str) -> bool {
    fs::read_dir(dir).unwrap().filter_map(Result::ok).any(|entry| {
        let path=entry.path();
        if path.is_dir() {contains_snapshot(&path,raw)} else {
            fs::read(&path).ok().and_then(|b|serde_json::from_slice::<Value>(&b).ok()).is_some_and(|v|v["content"]==raw)
        }
    })
}

#[test]
fn native_external_reconciliation_protects_human_bytes_and_snapshots_the_exact_basis() {
    let w=world();
    for project in [None,Some("one"),Some("two")] {
        let d=add(w.path(),&doc(w.path(),project,"external"),project,"first");
        let (external,raw)=patch_external(w.path(),&d,project);
        let mut i=input(&external,project,"reconcile","external.reconcile");
        i["expected_native_revision"]=external["last_native_revision"].clone();
        assert_eq!(call(w.path(),"document_mutate",&i,AGENT).unwrap_err().kind(),io::ErrorKind::PermissionDenied);
        let mut stale=i.clone();stale["expected_native_revision"]=json!("stale");
        assert_eq!(call(w.path(),"document_mutate",&stale,HUMAN).unwrap_err().kind(),io::ErrorKind::AlreadyExists);
        let saved=cli(w.path(),"central.document.mutate",&i,HUMAN);
        assert_eq!(saved["unreviewed_external_revision"],false);
        assert_eq!(saved["document"]["contributions"][0]["html"],"<p>Human's external revision</p>");
        assert_eq!(saved["document"]["contributions"][0]["human_touched"],true);
        assert_eq!(saved["document"]["contributions"][0]["locked"],true);
        let scope=Scope::resolve(w.path(),project).unwrap();
        assert!(contains_snapshot(&scope.root.join(".central/file-history"),&raw));
        let replay=cli(w.path(),"central.document.mutate",&i,HUMAN);
        assert_eq!(replay["revision"],saved["revision"]);
        let mut edit=input(&saved,project,"agent-after-reconcile","contribution.patch");
        edit["contribution_id"]=json!("first:part");edit["html"]=json!("overwrite");
        assert_eq!(call(w.path(),"document_mutate",&edit,AGENT).unwrap_err().kind(),io::ErrorKind::PermissionDenied);
    }
}
#[test]
fn interrupted_reconciliation_recovers_metadata_without_reapplying_or_losing_external_bytes() {
    let w=world();let d=add(w.path(),&doc(w.path(),None,"recovery"),None,"entry");
    let (external,_)=patch_external(w.path(),&d,None);
    let mut i=input(&external,None,"interrupted","external.reconcile");i["expected_native_revision"]=external["last_native_revision"].clone();
    let saved=call(w.path(),"document_mutate",&i,HUMAN).unwrap();
    for entry in fs::read_dir(w.path().join(".central/source-returns/document-mutations")).unwrap() {
        let path=entry.unwrap().path();let mut intent:Value=serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        if intent["request_key"].as_str().unwrap().ends_with(":interrupted") {intent["status"]=json!("prepared");fs::write(path,serde_json::to_vec(&intent).unwrap()).unwrap();}
    }
    let path=w.path().join("Control/relations/source-relations.json");
    let mut relations:Value=serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    for relation in relations["relations"].as_array_mut().unwrap() {
        if relation["ref"]==d["source"]["ref"] {relation["document"]["last_native_revision"]=external["last_native_revision"].clone();}
    }
    fs::write(path,serde_json::to_vec(&relations).unwrap()).unwrap();
    let recovered=cli(w.path(),"central.document.mutate",&i,HUMAN);
    assert_eq!(recovered["revision"],saved["revision"]);
    assert_eq!(recovered["unreviewed_external_revision"],false);
    assert_eq!(recovered["document"]["external_reconciliations"].as_array().unwrap().len(),1);
}
#[test]
fn ordered_entries_notes_replies_and_stale_anchors_survive_native_html_roundtrip() {
    let w=world();let mut d=add(w.path(),&doc(w.path(),None,"roundtrip"),None,"last");
    let mut i=input(&d,None,"insert","entry.insert");i["entry_id"]=json!("blank");i["before_entry_id"]=json!("last");
    d=call(w.path(),"document_mutate",&i,HUMAN).unwrap();
    let mut n=input(&d,None,"note","note.add");n["note_id"]=json!("note:1");n["timing"]=json!("During");n["html"]=json!("<p>During note</p>");
    n["anchor"]=json!({"kind":"contribution","target_id":"last:part","original_text":"Original passage"});
    d=call(w.path(),"document_mutate",&n,HUMAN).unwrap();
    let mut reply=input(&d,None,"note-reply","note.add");reply["note_id"]=json!("note:2");reply["timing"]=json!("After");reply["parent_note_id"]=json!("note:1");reply["html"]=json!("An after reply");
    d=call(w.path(),"document_mutate",&reply,HUMAN).unwrap();
    let mut patch=input(&d,None,"target-changed","contribution.patch");patch["contribution_id"]=json!("last:part");patch["html"]=json!("<p>Changed passage</p>");
    d=call(w.path(),"document_mutate",&patch,HUMAN).unwrap();
    assert_eq!(d["document"]["notes"][0]["anchor"]["status"],"needs-review");
    assert_eq!(d["document"]["notes"][0]["anchor"]["original_text"],"Original passage");
    let exported=cli(w.path(),"central.document.export",&json!({"source_ref":d["source"]["ref"],"document_id":d["document_id"]}),HUMAN);
    let html=exported["html"].as_str().unwrap();
    assert!(html.contains("Literal human text") && html.contains("During note") && html.contains("Stale anchor"));
    assert!(html.find("data-entry-id=\"blank\"").unwrap()<html.find("data-entry-id=\"last\"").unwrap());
    let path=w.path().join("retained-copy.html");fs::write(&path,html).unwrap();
    let mut restore=input(&d,None,"restore","portable.restore");restore["html"]=json!(fs::read_to_string(path).unwrap());
    let restored=cli(w.path(),"central.document.mutate",&restore,HUMAN);
    for key in ["entries","notes","template_payload","fields","document_id"] {assert_eq!(restored["document"][key],d["document"][key],"{key}");}
    assert_eq!(restored["document"]["contributions"],d["document"]["contributions"]);
    let replay=cli(w.path(),"central.document.mutate",&restore,HUMAN);assert_eq!(replay["revision"],restored["revision"]);
}
#[test]
fn portable_payload_cannot_grant_authority_change_identity_or_lose_template_keys() {
    let w=world();let d=add(w.path(),&doc(w.path(),None,"safety"),None,"entry");
    let exported=execute_at(w.path(),"document_export",&json!({"source_ref":d["source"]["ref"],"document_id":d["document_id"]}),100).unwrap();
    let mut restore=input(&d,None,"restore","portable.restore");restore["value"]=exported["snapshot"].clone();
    assert_eq!(call(w.path(),"document_mutate",&restore,AGENT).unwrap_err().kind(),io::ErrorKind::PermissionDenied);
    for key in ["document_id","date","kind"] {
        let mut altered=restore.clone();altered["value"]["document"][key]=json!("other");
        assert!(call(w.path(),"document_mutate",&altered,HUMAN).is_err());
    }
    let mut lost=restore.clone();lost["value"]["document"]["template_payload"]=json!({});
    assert!(call(w.path(),"document_mutate",&lost,HUMAN).is_err());
    let mut duplicate=restore.clone();let part=duplicate["value"]["document"]["contributions"][0].clone();
    duplicate["value"]["document"]["contributions"].as_array_mut().unwrap().push(part);
    assert!(call(w.path(),"document_mutate",&duplicate,HUMAN).is_err());
    restore["value"]["document"]["contributions"][0]["author_ref"]=json!("human:forged");
    restore["value"]["document"]["contributions"][0]["html"]=json!("<p>safe<script>bad()</script><img src='https://invalid/track'></p>");
    let saved=call(w.path(),"document_mutate",&restore,HUMAN).unwrap();
    let part=&saved["document"]["contributions"][0];
    assert_eq!(part["author_ref"],"agent:test");assert_eq!(part["locked"],true);
    assert!(!part["html"].as_str().unwrap().contains("<script"));assert!(!part["html"].as_str().unwrap().contains("<img"));
    let mut stale=restore;stale["request_id"]=json!("stale-restore");assert_eq!(call(w.path(),"document_mutate",&stale,HUMAN).unwrap_err().kind(),io::ErrorKind::AlreadyExists);
}
#[test]
fn root_receiving_uses_owner_refs_fair_cursors_and_rechecks_grants() {
    let w=world();let mut expected=BTreeSet::new();
    for project in [None,Some("one"),Some("two")] {
        let d=doc(w.path(),project,"same-local-doc-id");
        for n in 0..3 {
            let returned=call(w.path(),"receiving_submit",&submit_input(&d,project,&format!("return:{n}")),AGENT).unwrap();
            expected.insert(returned["return_ref"].as_str().unwrap().to_owned());
        }
    }
    let mut request=json!({"projects":["one","two"],"limit":1});
    assert_eq!(execute_at(w.path(),"receiving_list",&request,100).unwrap_err().kind(),io::ErrorKind::PermissionDenied);
    let mut actual=BTreeSet::new();let mut first_scopes=vec![];
    loop {
        let page=cli(w.path(),"central.receiving.list",&request,HUMAN);
        for value in page["returns"].as_array().unwrap() {
            assert!(value.get("proposal").is_none());
            assert_eq!(value["open"]["action"],"central.receiving.read");
            assert!(actual.insert(value["return_ref"].as_str().unwrap().to_owned()));
            if first_scopes.len()<3 {first_scopes.push(value["scope_ref"].clone());}
        }
        request["cursor"]=page["cursor"].clone();if page["more"]==false {break;}
    }
    assert_eq!(actual,expected);assert_eq!(first_scopes,json!(["control:root","project:test/one","project:test/two"]).as_array().unwrap().clone());
    let mut wrong=request.clone();wrong["projects"]=json!(["one"]);assert!(call(w.path(),"receiving_list",&wrong,HUMAN).is_err());
    let path=w.path().join("Control/user/authority.json");let mut grants:Value=serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    grants["grants"][0]["scope_refs"]=json!(["control:root","project:test/one"]);fs::write(path,serde_json::to_vec(&grants).unwrap()).unwrap();
    assert_eq!(call(w.path(),"receiving_list",&request,HUMAN).unwrap_err().kind(),io::ErrorKind::PermissionDenied);
}
#[test]
fn private_project_sources_and_revoked_artifacts_are_not_exposed_by_root_receiving() {
    let w=world();let d=doc(w.path(),Some("one"),"private");
    call(w.path(),"receiving_submit",&submit_input(&d,Some("one"),"private-return"),AGENT).unwrap();
    let scope=Scope::resolve(w.path(),Some("one")).unwrap();let path=scope.root.join(d["source"]["path"].as_str().unwrap());
    fs::write(path.parent().unwrap().join(".no-agent-retrieval"),"").unwrap();
    let page=call(w.path(),"receiving_list",&json!({"projects":["one","two"]}),HUMAN).unwrap();
    assert!(page["returns"].as_array().unwrap().is_empty());assert_eq!(page["withheld_unavailable_sources"],1);
}
#[test]
fn draft_to_inbox_retains_original_bytes_and_separates_acknowledgement_review_and_inclusion() {
    let w=world();let d=doc(w.path(),None,"draft-inbox");let scope=Scope::resolve(w.path(),None).unwrap();
    let original="# Proposed README\n\nExact draft bytes, not Day prose.\n";
    let policy=execute_at(w.path(),"policy",&json!({}),100).unwrap();
    let allocated=execute_at(w.path(),"allocate",&json!({"task_ref":"task:draft","purpose":"Preserve draft Return","expected_policy_revision":policy["revision"]}),100).unwrap();
    let draft=PathBuf::from(allocated["writable_destination"].as_str().unwrap()).join("README.md");
    let relative=draft.strip_prefix(&scope.root).unwrap().to_str().unwrap();
    let artifact=scope.create_agent_source(relative,original,"generated-draft",100).unwrap();
    let mut request=submit_input(&d,None,"draft-return");
    request["now_ref"]=allocated["now_ref"].clone();
    request["evidence_refs"]=json!(["factory:attempt:test"]);
    request["artifacts"]=json!([{"source_ref":artifact.source.source_ref,"expected_revision":artifact.revision.revision,"producer_ref":"agent:original-producer","proposed_target_ref":"project:readme"}]);
    let before=fs::read(w.path().join(d["source"]["path"].as_str().unwrap())).unwrap();
    let received=cli(w.path(),"central.receiving.submit",&request,AGENT);
    assert_eq!(received["record"]["artifacts"][0]["content"],original);assert_eq!(received["record"]["status"],"pending");
    assert_eq!(fs::read(w.path().join(d["source"]["path"].as_str().unwrap())).unwrap(),before);
    let closed=call(w.path(),"now_lifecycle",&json!({"now_ref":allocated["now_ref"],"expected_revision":allocated["revision"]["revision"],"expected_policy_revision":policy["revision"],"lifecycle":"closed"}),HUMAN).unwrap();
    let archive_input=json!({"now_ref":allocated["now_ref"],"expected_revision":closed["revision"]["revision"],"expected_policy_revision":policy["revision"],"lifecycle":"archived"});
    let blocked=call(w.path(),"now_lifecycle",&archive_input,HUMAN).unwrap_err();
    assert_eq!(blocked.kind(),io::ErrorKind::AlreadyExists);assert!(blocked.to_string().contains("outstanding receiving obligations"));
    let mut review=json!({"return_ref":received["return_ref"],"expected_return_revision":received["revision"],"expected_source_revision":d["revision"]["revision"],"disposition":"accepted"});
    let accepted=cli(w.path(),"central.receiving.review",&review,HUMAN);
    review["expected_return_revision"]=accepted["revision"].clone();review["disposition"]=json!("acknowledged");
    let ack=cli(w.path(),"central.receiving.review",&review,HUMAN);assert_eq!(ack["record"]["status"],"accepted");assert!(ack["record"]["acknowledgement"].is_object());
    review["expected_return_revision"]=ack["revision"].clone();review["disposition"]=json!("pending");
    let pending=cli(w.path(),"central.receiving.review",&review,HUMAN);assert_eq!(pending["record"]["status"],"pending");assert!(pending["record"]["review"].is_null());
    review["expected_return_revision"]=pending["revision"].clone();review["disposition"]=json!("accepted");
    let accepted=cli(w.path(),"central.receiving.review",&review,HUMAN);
    let included=cli(w.path(),"central.receiving.include",&json!({"return_ref":received["return_ref"],"expected_return_revision":accepted["revision"],"expected_source_revision":d["revision"]["revision"]}),HUMAN);
    assert_eq!(included["record"]["status"],"included");
    assert_eq!(included["document_result"]["document"]["contributions"][0]["occurred_at_unix_seconds"],42);
    assert_eq!(included["document_result"]["document"]["contributions"][0]["received_at_unix_seconds"],received["record"]["received_at_unix_seconds"]);
    let archived=call(w.path(),"now_lifecycle",&archive_input,HUMAN).unwrap();
    let reentered=call(w.path(),"now_lifecycle",&json!({"now_ref":allocated["now_ref"],"expected_revision":archived["revision"]["revision"],"expected_policy_revision":policy["revision"],"lifecycle":"active"}),HUMAN).unwrap();
    assert_eq!(reentered["record"]["lifecycle"],"active");assert_eq!(fs::read_to_string(&draft).unwrap(),original);
    fs::write(draft.parent().unwrap().join(".no-agent-retrieval"),"").unwrap();
    let page=call(w.path(),"receiving_list",&json!({"projects":[]}),HUMAN).unwrap();assert!(page["returns"].as_array().unwrap().is_empty());
    assert!(call(w.path(),"receiving_read",&json!({"return_ref":received["return_ref"]}),HUMAN).is_err());
}
#[test]
fn concurrent_native_receivers_across_root_and_projects_have_one_ref_per_producer() {
    let w=world();let barrier=Arc::new(Barrier::new(6));let mut requests=vec![];
    for project in [None,Some("one"),Some("two")] {
        let d=doc(w.path(),project,"concurrent");let request=submit_input(&d,project,"same-producer");requests.push(request.clone());requests.push(request);
    }
    let threads:Vec<_>=requests.into_iter().map(|request| {let root=w.path().to_owned();let barrier=barrier.clone();thread::spawn(move||{barrier.wait();cli(&root,"central.receiving.submit",&request,AGENT)})}).collect();
    let returned:Vec<_>=threads.into_iter().map(|t|t.join().unwrap()).collect();
    let refs:BTreeSet<_>=returned.iter().map(|v|v["return_ref"].as_str().unwrap()).collect();assert_eq!(refs.len(),3);
    let page=cli(w.path(),"central.receiving.list",&json!({"projects":["one","two"]}),HUMAN);assert_eq!(page["returns"].as_array().unwrap().len(),3);
}
