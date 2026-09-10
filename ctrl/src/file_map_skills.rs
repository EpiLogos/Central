//! The owner publishes an exact, bounded skill-source tree. AIKit snapshots and
//! projects it; placement, standing, source refs and retrieval remain Central's.
use crate::file_map::*;
use base64::Engine;
use serde_json::{json,Value};
use std::{collections::BTreeMap,fs,io::{self,Read},path::Path};
use std::os::unix::fs::MetadataExt;

pub(crate) fn tree(root:&Path,input:&Value)->io::Result<Value>{
    let (scope,r)=resolve(root,input,text(input,"source_ref")?)?;
    if !r.allowed(&scope)? || r.resource_uri.is_some(){return Err(denied("Skill source is not available"));}
    let path=r.location(&scope)?;
    if !path.is_dir(){return Err(invalid("Skill tree requires a registered directory"));}
    let mut files=Vec::new();let mut skills=Vec::new();let mut total=0usize;let mut basis=Vec::new();
    let source_refs=resources(&scope)?.into_iter().map(|r|(r.path,r.source_ref)).collect::<BTreeMap<_,_>>();
    let mut stack=vec![path.clone()];
    while let Some(dir)=stack.pop(){
        if dir.join(crate::control::AGENT_RETRIEVAL_DENY_MARKER).exists(){return Err(denied("A skill subtree is excluded; the owner does not copy around its privacy marker"));}
        let mut entries=fs::read_dir(&dir)?.map(|e|e.map(|e|e.path())).collect::<io::Result<Vec<_>>>()?;entries.sort();
        for file in entries{
            if files.len()>4096{return Err(invalid("Skill tree exceeds the file-count bound"));}
            let meta=fs::symlink_metadata(&file)?;
            if meta.file_type().is_symlink(){return Err(denied("Skill tree contains a symlink; resolve its source relationship explicitly"));}
            if meta.is_dir(){
                if matches!(file.file_name().and_then(|v|v.to_str()),Some(".git"|".central")){continue;}
                stack.push(file);continue;
            }
            if !meta.is_file() || meta.nlink()!=1{return Err(denied("Skill tree contains special or multiply-linked material"));}
            let owner_root=r.external_root.as_deref().unwrap_or(&scope.root);
            let relative_owner=file.strip_prefix(owner_root).map_err(io::Error::other)?.to_str().ok_or_else(||invalid("Non UTF-8 skill path"))?;
            let relative_tree=file.strip_prefix(&path).map_err(io::Error::other)?.to_str().ok_or_else(||invalid("Non UTF-8 skill path"))?;
            let mut bytes=Vec::new();crate::file_mutation::open_native_file(owner_root,relative_owner)?.take(4*1024*1024+1).read_to_end(&mut bytes)?;
            total+=bytes.len();if bytes.len()>4*1024*1024||total>32*1024*1024{return Err(invalid("Skill source exceeds bounded material size"));}
            let revision=digest(&bytes);let mode=meta.mode()&0o777;
            let reference=source_refs.get(relative_owner).cloned().unwrap_or_else(||format!("{}/{}",r.source_ref,relative_tree));
            basis.push(format!("{relative_tree}\0{revision}\0{mode}"));
            files.push(json!({"path":relative_tree,"source_ref":reference,"revision":revision,"mode":mode,"encoding":"base64","content":base64::engine::general_purpose::STANDARD.encode(&bytes)}));
            if file.file_name().and_then(|s|s.to_str())==Some(crate::control_skills::SKILL_BODY){
                let skill_dir=file.parent().unwrap();let name=skill_dir.file_name().and_then(|s|s.to_str()).ok_or_else(||invalid("Non UTF-8 skill name"))?;
                let manifest=crate::control_skills::read_skill_manifest(skill_dir)?;
                if let Some(m)=&manifest{
                    if m.name!=name{return Err(invalid("Central skill name differs from its directory"));}
                    if m.standing==crate::control_skills::SkillStanding::Retired && m.retirement.as_ref().is_none_or(|r|r.retirement_reason.trim().is_empty()){return Err(invalid("Retirement requires its exact source record and reason"));}
                }
                skills.push(json!({"path":skill_dir.strip_prefix(&path).unwrap(),"manifest":manifest,"standing":manifest.as_ref().map(|m|m.standing.as_str()).unwrap_or("unresolved")}));
            }
        }
    }
    files.sort_by(|a,b|a["path"].as_str().cmp(&b["path"].as_str()));basis.sort();
    // Recheck every source revision and the live privacy boundary before return.
    for file in &files{
        let full=path.join(text(file,"path")?);let rel=full.strip_prefix(r.external_root.as_deref().unwrap_or(&scope.root)).unwrap().to_str().unwrap();
        checked(r.external_root.as_deref().unwrap_or(&scope.root),rel)?;
        if crate::source_horizon::content_revision(&full)?.revision!=text(file,"revision")? || full.ancestors().any(|p|p.join(crate::control::AGENT_RETRIEVAL_DENY_MARKER).exists()){return Err(conflict("Skill tree changed or was excluded while reading"));}
    }
    Ok(json!({"schema":SCHEMA,"world_ref":scope.world_ref,"source_ref":r.source_ref,"location":path,"revision":digest(basis.join("\n").as_bytes()),"files":files,"skills":skills,"projection_policy":{"standing_owner":"Central","materialisation_owner":"AIKit","retired_projects":false},"automatic_agent_or_model_invocation":false}))
}
