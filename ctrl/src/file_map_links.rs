//! Governed placements and replayable location changes for Central file maps.
//! Filesystem effects retain source identity and never replace a foreign entry.
use crate::file_map::*;
use serde::{Deserialize,Serialize};
use serde_json::{json,Value};
use std::{ffi::CString,fs,io,path::{Path,PathBuf}};
use std::os::unix::{fs::{MetadataExt,symlink},io::AsRawFd};

#[derive(Clone,Debug,Serialize,Deserialize)]
struct Link { path:String, source_ref:String, owner:String, text:String, dev:u64, ino:u64, #[serde(default)] staged:Option<String> }
fn links(ground:&Value)->io::Result<Vec<Link>> { serde_json::from_value(ground["file_map"].get("links").cloned().unwrap_or(json!([]))).map_err(io::Error::other) }
fn cstring(text:&std::ffi::OsStr)->io::Result<CString> { CString::new(text.as_encoded_bytes()).map_err(io::Error::other) }
fn rename_new(root:&Path,from:&str,to:&str)->io::Result<()> {
    let from=relative(from)?;let to=relative(to)?;
    let src=crate::file_mutation::directory(root,from.parent().unwrap())?;
    let dst=crate::file_mutation::directory(root,to.parent().unwrap())?;
    let a=cstring(from.file_name().unwrap())?;let b=cstring(to.file_name().unwrap())?;
    #[cfg(target_os="linux")]
    let rc=unsafe { libc::renameat2(src.as_raw_fd(),a.as_ptr(),dst.as_raw_fd(),b.as_ptr(),libc::RENAME_NOREPLACE) };
    #[cfg(target_os="macos")]
    let rc=unsafe { libc::renameatx_np(src.as_raw_fd(),a.as_ptr(),dst.as_raw_fd(),b.as_ptr(),libc::RENAME_EXCL) };
    #[cfg(not(any(target_os="linux",target_os="macos")))]
    let rc=-1;
    if rc!=0{return Err(io::Error::last_os_error());}
    src.sync_all()?;dst.sync_all()
}
fn link_is(root:&Path,path:&str,l:&Link)->io::Result<bool>{
    let p=relative(path)?;crate::file_mutation::directory(root,p.parent().unwrap())?;
    match fs::symlink_metadata(root.join(p)){
        Ok(m)=>Ok(m.file_type().is_symlink() && m.dev()==l.dev && m.ino()==l.ino && fs::read_link(root.join(p))?==Path::new(&l.text)),
        Err(e) if e.kind()==io::ErrorKind::NotFound=>Ok(false),Err(e)=>Err(e)
    }
}
fn require_owned(root:&Path,l:&Link)->io::Result<()> {
    if link_is(root,&l.path,l)?{Ok(())}else{Err(conflict(format!("Managed link {} is missing or replaced; no foreign entry is touched",l.path)))}
}
fn relative_target(parent:&Path,target:&Path)->io::Result<String>{
    let a=parent.components().collect::<Vec<_>>();let b=target.components().collect::<Vec<_>>();
    let shared=a.iter().zip(&b).take_while(|(a,b)|a==b).count();
    let mut p=PathBuf::new();for _ in shared..a.len(){p.push("..");}for c in &b[shared..]{p.push(c.as_os_str());}
    p.to_str().map(str::to_owned).ok_or_else(||invalid("Non UTF-8 link target"))
}
fn stage(s:&Scope,path:&str,reference:&str,owner:&str,target:&Path)->io::Result<Link>{
    let member=relative(path)?;crate::file_mutation::directory(&s.root,member.parent().unwrap())?;
    let target_text=relative_target(&s.root.join(member).parent().unwrap(),target)?;
    let name=format!(".central/link-{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_err(io::Error::other)?.as_nanos());
    // Create under owner state before publishing. The inode is durably recorded
    // before the destination changes, making an interrupted publication replayable.
    symlink(&target_text,s.root.join(&name))?;
    let m=fs::symlink_metadata(s.root.join(&name))?;
    Ok(Link{path:path.into(),source_ref:reference.into(),owner:owner.into(),text:target_text,dev:m.dev(),ino:m.ino(),staged:Some(name)})
}
fn publish(s:&Scope,link:&mut Link)->io::Result<()> {
    if let Some(staged)=link.staged.clone(){
        if !link_is(&s.root,&link.path,link)? {
            if !link_is(&s.root,&staged,link)?{return Err(conflict("Staged link identity changed; refusing recovery"));}
            rename_new(&s.root,&staged,&link.path)?;
        }
        link.staged=None;
    }
    require_owned(&s.root,link)
}
fn replace_ground_links(s:&Scope,ground:&mut Value,ls:&[Link])->io::Result<()> {
    map_extension(ground)?["links"]=serde_json::to_value(ls).map_err(io::Error::other)?;s.save(ground)
}
fn link(root:&Path,s:&Scope,i:&Value)->io::Result<Value>{
    authorize(i)?;let _lock=crate::source_safety::lock(&s.root,"source-mutation.lock")?;
    let mut ground=s.ground()?;let mut ls=links(&ground)?;
    let path=text(i,"path")?;relative(path)?;let reference=text(i,"source_ref")?;let owner=text(i,"owner")?;
    if let Some(l)=ls.iter_mut().find(|l|l.path==path){
        if l.source_ref!=reference || l.owner!=owner{return Err(conflict("Link already belongs to another binding"));}
        publish(s,l)?;replace_ground_links(s,&mut ground,&ls)?;
        return Ok(json!({"schema":SCHEMA,"links":ls,"replayed":true,"ground_revision":s.revision()?}));
    }
    cas(s,i)?;
    if Path::new(path).components().any(|c|matches!(c.as_os_str().to_str(),Some(".central"|".git"))){return Err(denied("A managed public link cannot replace internal state"));}
    let (ts,r)=resolve(root,&json!({"scope":"all"}),reference).or_else(|_|resolve(&s.root,&json!({"scope":"project"}),reference))?;
    if !r.allowed(&ts)? || r.resource_uri.is_some(){return Err(denied("Link target is not an accessible filesystem source"));}
    let target=r.location(&ts)?;
    if target==s.root.join(path) || s.root.join(path).starts_with(&target) && target.is_dir(){return Err(invalid("A link cannot point to itself or an enclosing directory"));}
    if fs::symlink_metadata(s.root.join(path)).is_ok(){return Err(conflict("Link destination already exists"));}
    ls.push(stage(s,path,reference,owner,&target)?);
    replace_ground_links(s,&mut ground,&ls)?;
    let last=ls.last_mut().unwrap();publish(s,last)?;replace_ground_links(s,&mut ground,&ls)?;
    Ok(json!({"schema":SCHEMA,"links":ls,"ground_revision":s.revision()?}))
}
fn repair(root:&Path,s:&Scope)->io::Result<Value>{
    let _lock=crate::source_safety::lock(&s.root,"source-mutation.lock")?;
    let mut ground=s.ground()?;let mut ls=links(&ground)?;let mut changed=Vec::new();
    for index in 0..ls.len(){
        publish(s,&mut ls[index])?;
        let (owner,r)=resolve(root,&json!({"scope":"all"}),&ls[index].source_ref).or_else(|_|resolve(&s.root,&json!({"scope":"project"}),&ls[index].source_ref))?;
        if !r.allowed(&owner)?{return Err(denied("Managed link target is no longer available"));}
        let target=r.location(&owner)?;let l=ls[index].clone();
        let new_text=relative_target(s.root.join(&l.path).parent().unwrap(),&target)?;
        if new_text==l.text{continue;}
        require_owned(&s.root,&l)?;
        let new=stage(s,&l.path,&l.source_ref,&l.owner,&target)?;
        // Retain the old link as a recovery entry until the replacement publishes.
        // The journal lives in .central, not human ground.
        let backup=format!(".central/link-old-{}",l.ino);
        let journal=json!({"old":l,"new":new,"backup":backup});
        write_json(&s.root.join(".central/bkmr-link-repair.json"),&journal)?;
        rename_new(&s.root,&l.path,&backup)?;
        ls[index]=new;
        replace_ground_links(s,&mut ground,&ls)?;
        publish(s,&mut ls[index])?;
        replace_ground_links(s,&mut ground,&ls)?;
        if link_is(&s.root,&backup,&l)?{fs::remove_file(s.root.join(&backup))?;}
        fs::remove_file(s.root.join(".central/bkmr-link-repair.json"))?;
        changed.push(l.path);
    }
    replace_ground_links(s,&mut ground,&ls)?;
    Ok(json!({"schema":SCHEMA,"repaired":changed,"links":ls,"ground_revision":s.revision()?}))
}
fn recover_repair(s:&Scope)->io::Result<()> {
    let _lock=crate::source_safety::lock(&s.root,"source-mutation.lock")?;
    let Some(j)=read_json(&s.root,".central/bkmr-link-repair.json")?else{return Ok(());};
    let old:Link=serde_json::from_value(j["old"].clone()).map_err(io::Error::other)?;
    let mut new:Link=serde_json::from_value(j["new"].clone()).map_err(io::Error::other)?;
    let backup=text(&j,"backup")?;
    if link_is(&s.root,&old.path,&old)?{rename_new(&s.root,&old.path,backup)?;}
    else if !link_is(&s.root,backup,&old)?{return Err(conflict("Link repair lost its original inode; manual review required"));}
    let mut ground=s.ground()?;let mut ls=links(&ground)?;
    let index=ls.iter().position(|l|l.path==new.path && l.owner==new.owner).ok_or_else(||conflict("Link owner changed during recovery"))?;
    ls[index]=new.clone();replace_ground_links(s,&mut ground,&ls)?;
    publish(s,&mut new)?;ls[index]=new;replace_ground_links(s,&mut ground,&ls)?;
    if link_is(&s.root,backup,&old)?{fs::remove_file(s.root.join(backup))?;}
    fs::remove_file(s.root.join(".central/bkmr-link-repair.json"))?;Ok(())
}
fn fingerprint(root:&Path,rel:&str)->io::Result<String>{
    fn walk(root:&Path,base:&Path,rel:&Path,out:&mut Vec<String>,n:&mut usize)->io::Result<()> {
        *n+=1;if *n>10000{return Err(invalid("Move tree exceeds bounded source scan"));}
        let path=root.join(rel);let m=fs::symlink_metadata(&path)?;
        if m.file_type().is_symlink() || (!m.is_file()&&!m.is_dir()) {return Err(denied("Move tree contains a link or special file; use its native owner"));}
        if m.nlink()>1 && m.is_file(){return Err(denied("Move would affect a hard-linked source"));}
        if m.is_dir(){
            if path.join(".git").exists() || path.join(crate::control::AGENT_RETRIEVAL_DENY_MARKER).exists(){return Err(denied("Repository or excluded subtree relocation requires its native migration owner"));}
            let mut files=fs::read_dir(&path)?.map(|e|e.map(|e|e.path())).collect::<io::Result<Vec<_>>>()?;files.sort();
            for p in files{walk(root,base,p.strip_prefix(root).unwrap(),out,n)?;}
        }else{
            let revision=crate::source_horizon::content_revision(&path)?.revision;
            out.push(format!("{}:{}:{}:{revision}",rel.strip_prefix(base).unwrap_or(rel).display(),m.dev(),m.ino()));
        }Ok(())
    }
    checked(root,rel)?;let mut out=vec![];walk(root,Path::new(rel),Path::new(rel),&mut out,&mut 0)?;
    Ok(digest(out.join("\n").as_bytes()))
}
fn move_plan(s:&Scope,i:&Value)->io::Result<Value>{
    let from=text(i,"from")?;let to=text(i,"to")?;relative(from)?;relative(to)?;
    for path in [from,to]{if Path::new(path).components().any(|c|matches!(c.as_os_str().to_str(),Some(".central"|".git"))) || path==s.ground {return Err(denied("Cannot relocate internal state or relation ground as an ordinary source"));}}
    if Path::new(to).starts_with(from) || Path::new(from).starts_with(to){return Err(invalid("Move endpoints must be disjoint"));}
    let origin=checked(&s.root,from)?;let destination=checked(&s.root,to)?;
    if origin.is_dir() && i["quiescent"]!=true{return Err(denied("Directory relocation requires explicit quiescent-session confirmation"));}
    crate::file_mutation::directory(&s.root,Path::new(to).parent().unwrap())?;
    if fs::symlink_metadata(destination).is_ok(){return Err(conflict("Move destination exists"));}
    if origin.ancestors().any(|p|p.join(crate::control::AGENT_RETRIEVAL_DENY_MARKER).exists()){return Err(denied("Source is excluded from retrieval/mutation"));}
    let before=s.ground()?;let mut after=before.clone();let rs=resources(s)?;let entries=after["relations"].as_array_mut().unwrap();
    for mut r in rs {
        if r.external_root.is_none() && r.resource_uri.is_none() && Path::new(&r.path).starts_with(from){
            let suffix=Path::new(&r.path).strip_prefix(from).unwrap();
            r.path=if suffix.as_os_str().is_empty(){to.to_owned()}else{Path::new(to).join(suffix).to_str().ok_or_else(||invalid("Non UTF-8 move"))?.into()};
            put_resource(entries,&r)?;
        }
    }
    // A moved file need not previously have been a Source. Freeze its current
    // canonical identity in the relation ground before publishing the move.
    if origin.is_file() && !entries.iter().any(|v|v["path"]==to){
        entries.push(json!({"ref":crate::source_horizon::source_ref(&s.world_ref,from),"path":to,"roles":["registered-file"],"provenance":"unresolved","standing":"unspecified","treatment":"retain-native-in-place"}));
    }
    let m=fs::symlink_metadata(&origin)?;
    let mut p=json!({"schema":"central.file-map-move/v1","world_ref":s.world_ref,"from":from,"to":to,"quiescent":i["quiescent"],"ground_revision":s.revision()?,"fingerprint":fingerprint(&s.root,from)?,"dev":m.dev(),"ino":m.ino(),"before":before,"after":after});
    p["id"]=json!(digest(serde_json::to_string(&p).map_err(io::Error::other)?.as_bytes()).replace([':', '/'],"_"));Ok(p)
}
fn semantic_ground(v:&Value)->Value {
    let mut v=v.clone();
    if let Some(ls)=v["file_map"]["links"].as_array_mut(){for l in ls{if let Some(o)=l.as_object_mut(){for k in ["dev","ino","text","staged"]{o.remove(k);}}}}
    v
}
fn move_apply(root:&Path,s:&Scope,i:&Value,rollback:bool)->io::Result<Value>{
    authorize(i)?;let plan=&i["plan"];let id=text(plan,"id")?;
    if !id.chars().all(|c|c.is_ascii_alphanumeric()||"_.-".contains(c)){return Err(invalid("Invalid move plan id"));}
    let journal_path=format!(".central/bkmr-move-{id}.json");
    {
        let _lock=crate::source_safety::lock(&s.root,"source-mutation.lock")?;
        let mut j=match read_json(&s.root,&journal_path)?{
            Some(j)=>{if j["plan"]!=*plan{return Err(conflict("Replay does not match the persisted plan"));}j},
            None=>{
                if rollback{return Err(invalid("Cannot roll back an unrecorded move"));}
                let fresh=move_plan(s,plan)?;
                if fresh!=*plan{return Err(conflict("Move plan is stale or altered"));}
                let j=json!({"plan":plan,"phase":"prepared"});write_json(&s.root.join(&journal_path),&j)?;j
            }
        };
        let phase=j["phase"].as_str().unwrap_or("");
        if phase=="rolled-back"{if rollback{return Ok(j);}return Err(conflict("A rolled-back operation needs a new plan"));}
        if phase=="complete"&&!rollback{return Ok(j);}
        let (from,to,before,after)=if rollback{(text(plan,"to")?,text(plan,"from")?,&plan["after"],&plan["before"])}else{(text(plan,"from")?,text(plan,"to")?,&plan["before"],&plan["after"])};
        let current=s.ground()?;
        if semantic_ground(&current)!=semantic_ground(before) && semantic_ground(&current)!=semantic_ground(after){return Err(conflict("Relation ground changed since the move; merge/review before recovery"));}
        let src=checked(&s.root,from)?;let dst=checked(&s.root,to)?;
        if src.exists(){
            let m=fs::symlink_metadata(&src)?;
            if m.dev()!=plan["dev"].as_u64().unwrap_or(0)||m.ino()!=plan["ino"].as_u64().unwrap_or(0)||fingerprint(&s.root,from)?!=text(plan,"fingerprint")?{return Err(conflict("Move source changed since its plan"));}
            if dst.exists(){return Err(conflict("Recovery destination exists"));}
            rename_new(&s.root,from,to)?;
        }else{
            let m=fs::symlink_metadata(&dst)?;
            if m.dev()!=plan["dev"].as_u64().unwrap_or(0)||m.ino()!=plan["ino"].as_u64().unwrap_or(0)||fingerprint(&s.root,to)?!=text(plan,"fingerprint")?{return Err(conflict("Neither move endpoint retains the planned source"));}
        }
        j["phase"]=json!(if rollback{"rollback-moved"}else{"moved"});write_json(&s.root.join(&journal_path),&j)?;
        let mut next=after.clone();
        if !current["file_map"]["links"].is_null(){map_extension(&mut next)?["links"]=current["file_map"]["links"].clone();}
        s.save(&next)?;
        j["phase"]=json!(if rollback{"rolled-back"}else{"bound"});write_json(&s.root.join(&journal_path),&j)?;
    }
    // Derived search and link effects can fail independently. The durable
    // moved/bound phase is retained, so retry does not perform the move twice.
    let scope_input=if s.world_ref==crate::source_horizon::CONTROL_WORLD_REF{json!({})}else{json!({"project_path":s.root})};
    crate::file_map::execute(root,"refresh",&scope_input)?;
    for owner in scopes(root,&json!({"scope":"all"}))? {recover_repair(&owner)?;if !links(&owner.ground()?)?.is_empty(){repair(root,&owner)?;}}
    let mut j=read_json(&s.root,&journal_path)?.unwrap();j["phase"]=json!(if rollback{"rolled-back"}else{"complete"});write_json(&s.root.join(&journal_path),&j)?;Ok(j)
}
pub(crate) fn execute(root:&Path,s:&Scope,op:&str,i:&Value)->io::Result<Value>{
    match op {
        "link"=>{recover_repair(s)?;link(root,s,i)},
        "repair"=>{authorize(i)?;recover_repair(s)?;repair(root,s)},
        "unlink"=>{
            authorize(i)?;let _lock=crate::source_safety::lock(&s.root,"source-mutation.lock")?;cas(s,i)?;
            let mut g=s.ground()?;let mut ls=links(&g)?;let path=text(i,"path")?;let owner=text(i,"owner")?;
            let l=ls.iter().find(|l|l.path==path && l.owner==owner).ok_or_else(||denied("No link belongs to this owner"))?.clone();require_owned(&s.root,&l)?;
            // Keep the inode in owner state so removal is recoverable.
            let backup=format!(".central/unlinked-{}",l.ino);rename_new(&s.root,path,&backup)?;
            ls.retain(|v|v.path!=path);replace_ground_links(s,&mut g,&ls)?;
            Ok(json!({"schema":SCHEMA,"removed":path,"retained_link":backup,"source_untouched":true,"ground_revision":s.revision()?}))
        },
        "move.plan"=>move_plan(s,i),
        "move.apply"=>move_apply(root,s,i,false),
        "move.rollback"=>move_apply(root,s,i,true),
        "scope"=>{
            authorize(i)?;let _lock=crate::source_safety::lock(&s.root,"source-mutation.lock")?;cas(s,i)?;
            if s.world_ref!=crate::source_horizon::CONTROL_WORLD_REF{return Err(invalid("Project federation is registered at the root map"));}
            let raw=text(i,"path")?;let path=Path::new(raw);let absolute=if path.is_absolute(){path.canonicalize()?}else{checked(&s.root,raw)?};
            let owner=Scope::project(&absolute)?;let mut g=s.ground()?;let ext=map_extension(&mut g)?;
            let mut scopes=ext["scopes"].as_array().cloned().unwrap_or_default();
            scopes.retain(|v|v["world_ref"]!=owner.world_ref);scopes.push(json!({"path":raw,"world_ref":owner.world_ref}));ext["scopes"]=json!(scopes);s.save(&g)?;
            Ok(json!({"schema":SCHEMA,"scope":owner,"ground_revision":s.revision()?}))
        },
        _=>Err(invalid("Unknown map operation"))
    }
}
