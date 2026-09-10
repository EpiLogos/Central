use central_ctrl::continuous_work::{execute_at,execute_with_token_at,source::Scope};
use serde_json::{json,Value};
use sha2::{Digest,Sha256};
use std::{collections::BTreeSet,fs,io,path::{Path,PathBuf},sync::{Arc,Barrier,atomic::{AtomicU64,Ordering}},thread,time::{SystemTime,UNIX_EPOCH}};

const HUMAN:&str="document-human-test-credential-not-a-real-secret";
const AGENT:&str="document-agent-test-credential-not-a-real-secret";
const OTHER:&str="document-other-test-credential-not-a-real-secret";
static NEXT:AtomicU64=AtomicU64::new(0);
struct World(PathBuf);
impl World {fn path(&self)->&Path {&self.0}}
impl Drop for World {fn drop(&mut self){let _=fs::remove_dir_all(&self.0);}}
fn call(root:&Path,op:&str,input:&Value,token:&str)->io::Result<Value>{execute_with_token_at(root,op,input,Some(token),100)}
fn world()->World{
    let path=std::env::temp_dir().join(format!("central-doc-receiving-{}-{}-{}",std::process::id(),SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos(),NEXT.fetch_add(1,Ordering::Relaxed)));
    central_ctrl::initialize_central(&path).unwrap();
    for name in ["one","two"]{
        let project=path.join(format!("Work/{name}/ProjectCentral"));fs::create_dir_all(project.join("user")).unwrap();
        fs::write(project.join("project.json"),serde_json::to_vec(&central_ctrl::ProjectCentralManifest::new(format!("test/{name}"))).unwrap()).unwrap();
    }
    let grants:Vec<Value>=[HUMAN,AGENT,OTHER].iter().enumerate().map(|(i,token)|json!({
        "principal_ref":["human:test","agent:test","agent:other"][i],"actor_kind":if i==0{"human"}else{"agent"},
        "token_sha256":format!("{:x}",Sha256::digest(token.as_bytes())),"scope_refs":["control:root","project:test/one","project:test/two"],
        "actions":["central.day.ensure","central.day.lifecycle","central.now.lifecycle","central.document.create","central.document.mutate","central.receiving.submit","central.receiving.review","central.receiving.include","central.receiving.recover"],"expires_at_unix_seconds":9999
    })).collect();
    let policies=[
        ("placement.json","work-placement-policy",json!({"schema":"central.work-placement-policy/v1","scope_ref":"control:root","writable":[{"path":"Work/one","class":"repository"},{"path":"Work/two","class":"repository"}],"enforcement":"native-actions","required_coverage":["file-content"],"lease_seconds":300})),
        ("time.json","civil-time-policy",json!({"schema":"central.civil-time-policy/v1","scope_ref":"control:root","timezone":"Europe/London","day_boundary_minutes":0,"automatic_day_rollover":true})),
        ("authority.json","native-action-authority",json!({"schema":"central.native-action-authority/v1","scope_ref":"control:root","grants":grants})),
    ];
    let scope=Scope::resolve(&path,None).unwrap();let mut relations=vec![];
    for(name,role,value)in policies{
        let source=format!("Control/user/{name}");fs::write(path.join(&source),serde_json::to_vec_pretty(&value).unwrap()).unwrap();
        relations.push(json!({"ref":scope.source_ref(&source),"path":source,"roles":[role],"provenance":"human-adopted","standing":"architecture-contract","treatment":"projectcentral-user","recognition":"controlled-test-fixture-not-personal-adoption","recorded_at_unix_seconds":1}));
    }
    fs::create_dir_all(path.join("Control/relations")).unwrap();
    fs::write(path.join("Control/relations/source-relations.json"),serde_json::to_vec_pretty(&json!({"schema":"central.control.ground-relations/v1","project_id":"control:root","relations":relations})).unwrap()).unwrap();
    World(path)
}
fn document(root:&Path,project:Option<&str>,kind:&str,id:&str)->Value{
    let policy=execute_at(root,"policy",&json!({"project":project}),100).unwrap();
    // These are explicitly supplied test data, not asserted original Day HTML keys.
    let mut input=json!({"project":project,"kind":kind,"document_id":id,"title":"","expected_policy_revision":policy["revision"],"template_payload":{"actual_supplied_test_field":"human value","unknown_nested":{"preserved":true}},"fields":[{"id":"test-field","label":"Supplied test field","template_pointer":"/actual_supplied_test_field"}]});
    if kind=="day"{
        let time=execute_at(root,"time_policy",&json!({"project":project}),100).unwrap();
        let day=call(root,"day_ensure",&json!({"project":project,"expected_time_policy_revision":time["revision"]}),HUMAN).unwrap();
        input["day_ref"]=day["day_ref"].clone();input["expected_revision"]=day["revision"]["revision"].clone();
    }
    call(root,"document_create",&input,HUMAN).unwrap()
}
fn read(root:&Path,doc:&Value,project:Option<&str>)->Value{
    execute_at(root,"document_read",&json!({"project":project,"source_ref":doc["source"]["ref"],"document_id":doc["document_id"]}),100).unwrap()
}
fn mutation(doc:&Value,project:Option<&str>,request:&str,operation:&str)->Value{
    json!({"project":project,"source_ref":doc["source"]["ref"],"document_id":doc["document_id"],"expected_revision":doc["revision"]["revision"],"request_id":request,"operation":operation})
}
fn proposal(doc:&Value,project:Option<&str>,producer:&str,entry:&str)->Value{
    json!({"project":project,"producer_key":producer,"source_ref":doc["source"]["ref"],"document_id":doc["document_id"],"expected_source_revision":doc["revision"]["revision"],"occurred_at_unix_seconds":42,"task_ref":"task:retained","session_ref":"session:finished","proposal":{"operation":"entry.add","entry_id":entry,"contribution_id":format!("{entry}:contribution"),"html":"<p>Reviewed contribution</p>"}})
}
fn review(root:&Path,received:&Value,doc:&Value,project:Option<&str>)->Value{
    call(root,"receiving_review",&json!({"project":project,"return_ref":received["return_ref"],"expected_return_revision":received["revision"],"expected_source_revision":doc["revision"]["revision"],"disposition":"accepted"}),HUMAN).unwrap()
}
fn inclusion(received:&Value,doc:&Value,project:Option<&str>)->Value{
    json!({"project":project,"return_ref":received["return_ref"],"expected_return_revision":received["revision"],"expected_source_revision":doc["revision"]["revision"]})
}

#[test]
fn shared_flow_and_dialogue_use_the_existing_register_and_protect_human_edits(){
    let world=world();
    for(project,kind)in[(None,"flow"),(Some("one"),"dialogue")]{
        let doc=document(world.path(),project,kind,"doc:shared");
        let root=Scope::resolve(world.path(),project).unwrap().root;
        let register:Value=serde_json::from_slice(&fs::read(root.join(".central/flows.json")).unwrap()).unwrap();
        assert_eq!(register["flows"].as_array().unwrap().len(),1);
        assert_eq!(register["flows"][0]["source_ref"],doc["source"]["ref"]);
        assert_eq!(doc["document"]["title"],"");
        let mut add=mutation(&doc,project,"add:one","entry.add");add["entry_id"]=json!("entry:one");add["contribution_id"]=json!("part:one");add["html"]=json!("<p>Agent writes <strong>this</strong></p>");
        let added=call(world.path(),"document_mutate",&add,AGENT).unwrap();
        assert_eq!(added["document"]["contributions"][0]["author_ref"],"agent:test");
        let mut patch=mutation(&added,project,"patch:other","contribution.patch");patch["contribution_id"]=json!("part:one");patch["html"]=json!("not mine");patch["actor_kind"]=json!("human");
        assert_eq!(call(world.path(),"document_mutate",&patch,OTHER).unwrap_err().kind(),io::ErrorKind::PermissionDenied);
        patch["request_id"]=json!("human:touch");
        let touched=call(world.path(),"document_mutate",&patch,HUMAN).unwrap();
        assert_eq!(touched["document"]["contributions"][0]["human_touched"],true);
        assert_eq!(touched["document"]["contributions"][0]["locked"],true);
        patch["expected_revision"]=touched["revision"]["revision"].clone();patch["request_id"]=json!("agent:after-human");
        assert_eq!(call(world.path(),"document_mutate",&patch,AGENT).unwrap_err().kind(),io::ErrorKind::PermissionDenied);
        assert_eq!(read(world.path(),&doc,project)["revision"],touched["revision"]);
    }
}
#[test]
fn document_operations_replay_exactly_and_external_human_bytes_block_agent_overwrite(){
    let world=world();let doc=document(world.path(),None,"flow","doc:replay");
    let mut add=mutation(&doc,None,"add:replay","entry.add");add["entry_id"]=json!("entry:one");add["contribution_id"]=json!("part:one");add["html"]=json!("one");
    let first=call(world.path(),"document_mutate",&add,AGENT).unwrap();
    let replay=call(world.path(),"document_mutate",&add,AGENT).unwrap();
    assert_eq!(first["revision"],replay["revision"]);assert_eq!(replay["document"]["contributions"].as_array().unwrap().len(),1);
    let path=world.path().join(doc["source"]["path"].as_str().unwrap());
    let mut actual=fs::read_to_string(&path).unwrap();actual.push('\n');fs::write(&path,&actual).unwrap();
    let external=read(world.path(),&doc,None);assert_eq!(external["unreviewed_external_revision"],true);
    let mut patch=mutation(&external,None,"external:patch","contribution.patch");patch["contribution_id"]=json!("part:one");patch["html"]=json!("overwrite");
    assert_eq!(call(world.path(),"document_mutate",&patch,AGENT).unwrap_err().kind(),io::ErrorKind::PermissionDenied);
    assert_eq!(fs::read_to_string(path).unwrap(),actual);
}
#[test]
fn human_day_rejects_direct_agent_changes_preserves_supplied_fields_and_exports_inert_snapshot(){
    let world=world();let doc=document(world.path(),None,"day","doc:day");
    assert_eq!(doc["document"]["template_payload"]["unknown_nested"]["preserved"],true);
    let mut append=mutation(&doc,None,"day:agent","field.append");append["field_id"]=json!("test-field");append["contribution_id"]=json!("part:day");append["html"]=json!("<p>text<script>bad()</script><img src='https://example.invalid/track' onerror='bad()'><strong>safe</strong></p>");
    assert_eq!(call(world.path(),"document_mutate",&append,AGENT).unwrap_err().kind(),io::ErrorKind::PermissionDenied);
    append["request_id"]=json!("day:human");let saved=call(world.path(),"document_mutate",&append,HUMAN).unwrap();
    let html=saved["document"]["contributions"][0]["html"].as_str().unwrap();
    for unsafe_text in ["<script","<img","onerror","https://"]{assert!(!html.contains(unsafe_text));}
    assert!(html.contains("<strong>safe</strong>"));
    let export=execute_at(world.path(),"document_export",&json!({"source_ref":doc["source"]["ref"],"document_id":doc["document_id"]}),100).unwrap();
    assert_eq!(export["executable_scripts"],false);assert_eq!(export["automatic_network_or_model_calls"],false);
    assert_eq!(export["snapshot"]["document"]["template_payload"],doc["document"]["template_payload"]);
    assert_eq!(export["snapshot"]["source_authority"],"retained-snapshot-not-live-authority");
}
#[test]
fn missing_template_pointer_is_rejected_instead_of_manufacturing_a_fixture_key(){
    let world=world();let policy=execute_at(world.path(),"policy",&json!({}),100).unwrap();
    let request=json!({"kind":"flow","document_id":"doc:missing","expected_policy_revision":policy["revision"],"template_payload":{"present":"value"},"fields":[{"id":"missing","label":"Missing","template_pointer":"/not-present"}]});
    assert_eq!(call(world.path(),"document_create",&request,HUMAN).unwrap_err().kind(),io::ErrorKind::InvalidInput);
    assert!(!world.path().join(".central/flows.json").exists());
}
#[test]
fn legacy_declared_human_whole_file_write_cannot_bypass_document_ownership(){
    let world=world();let doc=document(world.path(),Some("one"),"flow","doc:protected");
    let root=world.path().join("Work/one");
    let error=central_ctrl::write_world_source(&root,doc["source"]["ref"].as_str().unwrap(),doc["revision"]["revision"].as_str().unwrap(),"destroy human contributions","H","human",None).unwrap_err();
    assert_eq!(error.kind(),io::ErrorKind::PermissionDenied);
    assert_eq!(read(world.path(),&doc,Some("one"))["revision"],doc["revision"]);
}
#[test]
fn concurrent_distinct_receiving_and_same_producer_replay_have_no_lost_returns(){
    let world=world();let doc=document(world.path(),None,"day","doc:concurrent");
    let before=fs::read(world.path().join(doc["source"]["path"].as_str().unwrap())).unwrap();
    let barrier=Arc::new(Barrier::new(8));
    let threads:Vec<_>=(0..8).map(|i|{let root=world.path().to_path_buf();let input=proposal(&doc,None,&format!("producer:{i}"),&format!("entry:{i}"));let barrier=barrier.clone();thread::spawn(move||{barrier.wait();call(&root,"receiving_submit",&input,AGENT).unwrap()})}).collect();
    let results:Vec<_>=threads.into_iter().map(|t|t.join().unwrap()).collect();
    let refs:BTreeSet<_>=results.iter().map(|r|r["return_ref"].as_str().unwrap()).collect();assert_eq!(refs.len(),8);
    let sequences:BTreeSet<_>=results.iter().map(|r|r["record"]["sequence"].as_u64().unwrap()).collect();assert_eq!(sequences.len(),8);
    let original=results.iter().find(|r|r["record"]["proposal"]["entry_id"]=="entry:0").unwrap();
    let replay=call(world.path(),"receiving_submit",&proposal(&doc,None,"producer:0","entry:0"),AGENT).unwrap();assert_eq!(replay["return_ref"],original["return_ref"]);assert_eq!(replay["revision"],original["revision"]);
    let first=execute_at(world.path(),"receiving_list",&json!({"limit":3}),100).unwrap();assert_eq!(first["more"],true);assert_eq!(first["returns"].as_array().unwrap().len(),3);
    let second=execute_at(world.path(),"receiving_list",&json!({"after":first["next_after"],"limit":20}),100).unwrap();assert_eq!(second["returns"].as_array().unwrap().len(),5);
    assert_eq!(fs::read(world.path().join(doc["source"]["path"].as_str().unwrap())).unwrap(),before);
    assert_eq!(original["record"]["occurred_at_unix_seconds"],42);assert_eq!(original["record"]["received_at_unix_seconds"],100);
}
#[test]
fn concurrent_reviewed_inclusion_is_cas_safe_then_explicit_rereview_retains_both_contributions(){
    let world=world();let doc=document(world.path(),Some("one"),"day","doc:review");
    let first=call(world.path(),"receiving_submit",&proposal(&doc,Some("one"),"producer:a","entry:a"),AGENT).unwrap();
    let second=call(world.path(),"receiving_submit",&proposal(&doc,Some("one"),"producer:b","entry:b"),OTHER).unwrap();
    let accepted=[review(world.path(),&first,&doc,Some("one")),review(world.path(),&second,&doc,Some("one"))];
    assert_eq!(read(world.path(),&doc,Some("one"))["revision"],doc["revision"]);
    let barrier=Arc::new(Barrier::new(2));
    let threads:Vec<_>=accepted.iter().map(|received|{let root=world.path().to_path_buf();let input=inclusion(received,&doc,Some("one"));let barrier=barrier.clone();thread::spawn(move||{barrier.wait();call(&root,"receiving_include",&input,HUMAN)})}).collect();
    let results:Vec<_>=threads.into_iter().map(|t|t.join().unwrap()).collect();assert_eq!(results.iter().filter(|r|r.is_ok()).count(),1);assert_eq!(results.iter().filter(|r|r.is_err()).count(),1);
    let lost=results.iter().position(Result::is_err).unwrap();let current=read(world.path(),&doc,Some("one"));
    let retained=execute_at(world.path(),"receiving_read",&json!({"project":"one","return_ref":accepted[lost]["return_ref"]}),100).unwrap();
    assert_eq!(retained["record"]["status"],"accepted");
    let rereview=review(world.path(),&retained,&current,Some("one"));
    let included=call(world.path(),"receiving_include",&inclusion(&rereview,&current,Some("one")),HUMAN).unwrap();assert_eq!(included["included"],true);
    let final_doc=read(world.path(),&doc,Some("one"));assert_eq!(final_doc["document"]["contributions"].as_array().unwrap().len(),2);
    let authors:BTreeSet<_>=final_doc["document"]["contributions"].as_array().unwrap().iter().map(|c|c["author_ref"].as_str().unwrap()).collect();assert_eq!(authors,BTreeSet::from(["agent:test","agent:other"]));
}
#[test]
fn interrupted_inclusion_acknowledgement_recovers_without_duplicate_source_effect(){
    let world=world();let doc=document(world.path(),None,"day","doc:lost-ack");
    let received=call(world.path(),"receiving_submit",&proposal(&doc,None,"producer:lost","entry:lost"),AGENT).unwrap();
    let accepted=review(world.path(),&received,&doc,None);
    let included=call(world.path(),"receiving_include",&inclusion(&accepted,&doc,None),HUMAN).unwrap();
    let before=read(world.path(),&doc,None);
    // Recreate the actual persisted pre-ack phase after the independently
    // committed document effect, retaining the owner's real inclusion request.
    let area=world.path().join(".central/source-returns/contributions");
    let mut found=false;
    for entry in fs::read_dir(&area).unwrap(){let path=entry.unwrap().path();if path.extension().and_then(|s|s.to_str())!=Some("json"){continue;}let mut actual:Value=serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();if actual["return_ref"]==included["return_ref"]{assert!(actual["inclusion_request"].is_object());actual["status"]=json!("including");actual["applied_source_revision"]=Value::Null;fs::write(path,serde_json::to_vec_pretty(&actual).unwrap()).unwrap();found=true;}}
    assert!(found);
    let pending=execute_at(world.path(),"receiving_read",&json!({"return_ref":included["return_ref"]}),100).unwrap();
    let recovered=call(world.path(),"receiving_recover",&json!({"return_ref":pending["return_ref"],"expected_return_revision":pending["revision"]}),HUMAN).unwrap();
    assert_eq!(recovered["included"],true);assert_eq!(recovered["record"]["applied_source_revision"],included["record"]["applied_source_revision"]);
    let after=read(world.path(),&doc,None);assert_eq!(before["revision"],after["revision"]);assert_eq!(after["document"]["contributions"].as_array().unwrap().len(),1);
}
#[test]
fn receiving_rejects_fake_human_review_and_stale_return_revision_without_source_effects(){
    let world=world();let doc=document(world.path(),None,"day","doc:auth");
    let received=call(world.path(),"receiving_submit",&proposal(&doc,None,"producer:auth","entry:auth"),AGENT).unwrap();
    let mut input=json!({"return_ref":received["return_ref"],"expected_return_revision":received["revision"],"expected_source_revision":doc["revision"]["revision"],"disposition":"accepted","actor_kind":"human","author":"H"});
    assert_eq!(call(world.path(),"receiving_review",&input,AGENT).unwrap_err().kind(),io::ErrorKind::PermissionDenied);
    assert_eq!(execute_at(world.path(),"receiving_review",&input,100).unwrap_err().kind(),io::ErrorKind::PermissionDenied);
    input["expected_return_revision"]=json!("stale");assert_eq!(call(world.path(),"receiving_review",&input,HUMAN).unwrap_err().kind(),io::ErrorKind::AlreadyExists);
    assert_eq!(read(world.path(),&doc,None)["revision"],doc["revision"]);
}
