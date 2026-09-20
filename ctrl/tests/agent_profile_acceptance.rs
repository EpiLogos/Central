//! Controlled native source/credential tests. Test credentials and authored
//! authority fixtures exist only in disposable worlds, never production setup.
use central_ctrl::{agent_profile::{AgentProfile,AgentProfileScope},agent_profile_store::AgentProfileStore,agent_profile_acceptance::{execute_with_token_at,ACCEPT,REVIEW,ROSTER},continuous_work::source::Scope,world::WorldRef};
use serde_json::{json,Value};
use sha2::{Digest,Sha256};
use std::{fs,path::{Path,PathBuf},process::Command,sync::{Arc,Barrier},thread};
const HUMAN:&str="agent-profile-human-controlled-test-not-a-live-secret";
const AGENT:&str="agent-profile-agent-controlled-test-not-a-live-secret";
struct World(PathBuf);
impl Drop for World {fn drop(&mut self){let _=fs::remove_dir_all(&self.0);}}
fn world()->World {
 let path=std::env::temp_dir().join(format!("central-agent-acceptance-{}",uuid::Uuid::new_v4()));
 central_ctrl::initialize_central(&path).unwrap();
 let scope=Scope::resolve(&path,None).unwrap();
 let grants:Vec<_>=[(HUMAN,"human"),(AGENT,"agent")].into_iter().map(|(token,kind)|json!({"principal_ref":format!("{kind}:controlled-test"),"actor_kind":kind,"token_sha256":format!("{:x}",Sha256::digest(token.as_bytes())),"scope_refs":["control:root"],"actions":[ACCEPT],"expires_at_unix_seconds":200})).collect();
 fs::write(path.join("Control/user/agent-authority.json"),serde_json::to_vec(&json!({"schema":"central.native-action-authority/v1","scope_ref":"control:root","grants":grants})).unwrap()).unwrap();
 fs::create_dir_all(path.join("Control/relations")).unwrap();
 fs::write(path.join("Control/relations/source-relations.json"),serde_json::to_vec(&json!({"schema":"central.control.ground-relations/v1","project_id":"control:root","relations":[{"ref":scope.source_ref("Control/user/agent-authority.json"),"path":"Control/user/agent-authority.json","roles":["native-action-authority"],"provenance":"human-adopted","standing":"architecture-contract","treatment":"projectcentral-user","recognition":"controlled-test-only-not-personal-adoption","recorded_at_unix_seconds":1}]})).unwrap()).unwrap();
 World(path)
}
fn proposal(root:&Path)->AgentProfile {
 let mut profile=AgentProfile::propose_from_intent("agent-profile:test-creation","r1","agent:test-creation",AgentProfileScope::Personal,WorldRef::new("world:personal").unwrap(),"Read the explicitly selected project sources; return an attributed comparison.","agent-profile.express").unwrap();
 profile.name=Some("Source reader".into());profile.purpose=profile.intent_provenance.as_ref().map(|p|p.intent_expression.clone());
 AgentProfileStore::personal(root).save(&profile,None).unwrap(); profile
}
fn input(profile:&AgentProfile)->Value {json!({"scope":"root","profile_ref":profile.profile_ref})}
fn review(root:&Path,p:&AgentProfile)->Value {execute_with_token_at(root,REVIEW,&input(p),None,100).unwrap()}
fn acceptance(root:&Path,p:&AgentProfile)->Value {let r=review(root,p);json!({"scope":"root","profile_ref":p.profile_ref,"expected_revision":p.revision,"expected_content_digest":r["content_digest"]})}
#[test] fn authenticated_acceptance_is_separate_from_generated_source_and_execution() {
 let w=world();let p=proposal(&w.0);let path=AgentProfileStore::personal(&w.0).source_path(&p.profile_ref).unwrap();let bytes=fs::read(&path).unwrap();
 assert_eq!(review(&w.0,&p)["accepted"],false);
 let result=execute_with_token_at(&w.0,ACCEPT,&acceptance(&w.0,&p),Some(HUMAN),100).unwrap();
 assert_eq!(result["accepted"],true);assert_eq!(result["acceptance"]["principal_ref"],"human:controlled-test");
 assert_eq!(result["profile"]["intent_provenance"]["recognition"],"unrecognised");
 assert_eq!(result["profile"]["name"],"Source reader");assert_eq!(result["execution_authority_granted"],false);
 assert_eq!(fs::read(path).unwrap(),bytes);
 let roster=execute_with_token_at(&w.0,ROSTER,&json!({"scope":"root"}),None,300).unwrap();
 assert_eq!(roster["profiles"][0]["acceptance"],result["acceptance"]);
 assert!(!roster.to_string().contains(HUMAN));assert!(!roster.to_string().contains(AGENT));
}
#[test] fn missing_agent_expired_and_json_credential_cannot_accept() {
 let w=world();let p=proposal(&w.0);let value=acceptance(&w.0,&p);
 for (token,now) in [(None,100),(Some(AGENT),100),(Some(HUMAN),200)] {
  assert!(execute_with_token_at(&w.0,ACCEPT,&value,token,now).is_err());
 }
 let mut forged=value.clone();forged["actor_kind"]=json!("human");forged["token"]=json!(HUMAN);
 assert!(execute_with_token_at(&w.0,ACCEPT,&forged,None,100).is_err());
 assert_eq!(review(&w.0,&p)["accepted"],false);
}
#[test] fn stale_review_and_same_revision_external_edit_do_not_reuse_acceptance() {
 let w=world();let p=proposal(&w.0);let request=acceptance(&w.0,&p);
 let accepted=execute_with_token_at(&w.0,ACCEPT,&request,Some(HUMAN),100).unwrap();
 let path=AgentProfileStore::personal(&w.0).source_path(&p.profile_ref).unwrap();
 let mut edited=p.clone();edited.purpose=Some("A different purpose not reviewed".into());
 fs::write(path,serde_json::to_vec(&edited).unwrap()).unwrap();
 assert_eq!(review(&w.0,&p)["accepted"],false);
 assert!(execute_with_token_at(&w.0,ACCEPT,&request,Some(HUMAN),101).is_err());
 assert!(accepted["acceptance"]["acceptance_ref"].as_str().unwrap().starts_with("agent-profile-acceptance:"));
}
#[test] fn acceptance_is_idempotent_across_concurrent_native_callers() {
 let w=world();let p=proposal(&w.0);let value=acceptance(&w.0,&p);let barrier=Arc::new(Barrier::new(6));
 let handles:Vec<_>=(0..6).map(|i|{let root=w.0.clone();let input=value.clone();let b=barrier.clone();thread::spawn(move||{b.wait();execute_with_token_at(&root,ACCEPT,&input,Some(HUMAN),100+i).unwrap()})}).collect();
 let values:Vec<_>=handles.into_iter().map(|h|h.join().unwrap()).collect();
 for value in &values {assert_eq!(value["acceptance"],values[0]["acceptance"]);}
 assert_eq!(fs::read_dir(w.0.join(".central/agent-profile-acceptances")).unwrap().count(),1);
}
#[test] fn native_receipt_or_source_redirect_is_not_followed() {
 use std::os::unix::fs::symlink;
 let w=world();let p=proposal(&w.0);let request=acceptance(&w.0,&p);let external=w.0.join("outside");fs::create_dir(&external).unwrap();
 symlink(&external,w.0.join(".central/agent-profile-acceptances")).unwrap();
 assert!(execute_with_token_at(&w.0,ACCEPT,&request,Some(HUMAN),100).is_err());
 assert_eq!(fs::read_dir(external).unwrap().count(),0);
}
#[test] fn real_cli_expresses_name_and_skill_intent_but_cannot_self_accept() {
 let w=world();
 let input=json!({"scope":"root","world_ref":"world:personal","ratified_world_refs":["world:personal"],"name":"Named reader","intent_expression":"Read explicit sources","purpose":"Read explicit sources","skill_refs":["skill:source-reading"]});
 let output=Command::new(env!("CARGO_BIN_EXE_ctrl")).args(["--json","--root",w.0.to_str().unwrap(),"action","run","agent-profile.express",&input.to_string()]).env_remove("CENTRAL_NATIVE_TOKEN").output().unwrap();
 assert!(output.status.success(),"{}",String::from_utf8_lossy(&output.stderr));
 let result:Value=serde_json::from_slice(&output.stdout).unwrap();
 assert_eq!(result["data"]["profile"]["name"],"Named reader");
 assert_eq!(result["data"]["profile"]["skill_refs"],json!(["skill:source-reading"]));
 assert_eq!(result["data"]["human_recognised"],false);
 let mut invalid=input;invalid["intent_expression"]=json!(" silently trimmed ");
 let output=Command::new(env!("CARGO_BIN_EXE_ctrl")).args(["--json","--root",w.0.to_str().unwrap(),"action","run","agent-profile.express",&invalid.to_string()]).output().unwrap();
 assert!(!output.status.success());
}
