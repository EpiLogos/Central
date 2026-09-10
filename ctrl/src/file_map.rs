//! Persistent file maps in the existing source-relations ground. Paths remain
//! locations; source refs remain identities; bkmr rows are derived bindings.
use crate::{action::*, result::{ActionResult, ResultStatus}, source_horizon as horizon};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::{BTreeMap, BTreeSet}, fs, io::{self, Read, Write}, path::{Component, Path, PathBuf}};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};

pub const SCHEMA: &str = "central.file-map/v1";
pub const STATE: &str = ".central/bkmr-bindings.json";
const LIMIT: usize = 10000;
const INDEX_BYTES: usize = 24000;
const MARK: &str = "\n[central-file-map/v1] ";
pub(crate) fn invalid(text: impl Into<String>) -> io::Error { io::Error::new(io::ErrorKind::InvalidInput, text.into()) }
pub(crate) fn conflict(text: impl Into<String>) -> io::Error { io::Error::new(io::ErrorKind::AlreadyExists, text.into()) }
pub(crate) fn denied(text: impl Into<String>) -> io::Error { io::Error::new(io::ErrorKind::PermissionDenied, text.into()) }
pub(crate) fn text<'a>(v: &'a Value, k: &str) -> io::Result<&'a str> { v[k].as_str().filter(|s| !s.is_empty()).ok_or_else(|| invalid(format!("{k} is required"))) }
pub(crate) fn relative(raw: &str) -> io::Result<&Path> {
    let p = Path::new(raw);
    if raw.is_empty() || !p.components().all(|c| matches!(c, Component::Normal(_))) { return Err(invalid("A location must have non-empty relative normal components")); }
    Ok(p)
}
pub(crate) fn checked(root: &Path, rel: &str) -> io::Result<PathBuf> {
    relative(rel)?;
    crate::projectcentral_flow::reject_symlink_components(root, Path::new(rel))?;
    Ok(root.join(rel))
}
pub(crate) fn safe_directories(root: &Path, rel: &str) -> io::Result<()> {
    let mut p = root.to_path_buf();
    for c in relative(rel)?.components() {
        p.push(c.as_os_str());
        match fs::create_dir(&p) { Ok(()) => fs::set_permissions(&p, fs::Permissions::from_mode(0o700))?, Err(e) if e.kind()==io::ErrorKind::AlreadyExists => {}, Err(e) => return Err(e) }
        if !fs::symlink_metadata(&p)?.file_type().is_dir() { return Err(denied("State parent is not a real directory")); }
    }
    Ok(())
}
pub(crate) fn atomic_json_bytes(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let parent = path.parent().ok_or_else(|| invalid("No record parent"))?;
    let tmp = parent.join(format!(".map-{}-{}",std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_err(io::Error::other)?.as_nanos()));
    let mut f = fs::OpenOptions::new().write(true).create_new(true).mode(0o600).custom_flags(libc::O_NOFOLLOW).open(&tmp)?;
    let result = (|| { f.write_all(bytes)?; f.sync_all()?; fs::rename(&tmp,path)?; fs::File::open(parent)?.sync_all() })();
    let _ = fs::remove_file(tmp); result
}
pub(crate) fn write_json(path: &Path, v: &impl Serialize) -> io::Result<()> { atomic_json_bytes(path, &serde_json::to_vec_pretty(v).map_err(io::Error::other)?) }
pub(crate) fn read_json(root: &Path, rel: &str) -> io::Result<Option<Value>> {
    let path = checked(root,rel)?;
    if !path.exists() { return Ok(None); }
    let f = crate::file_mutation::open_native_file(root, rel)?;
    if !f.metadata()?.is_file() { return Err(invalid("JSON source must be a regular file")); }
    let mut bytes = Vec::new(); f.take(8*1024*1024+1).read_to_end(&mut bytes)?;
    if bytes.len()>8*1024*1024 { return Err(invalid("JSON source exceeds bound")); }
    serde_json::from_slice(&bytes).map(Some).map_err(io::Error::other)
}
pub(crate) fn digest(bytes: &[u8]) -> String { crate::projectcentral_flow::content_revision_bytes(bytes) }
fn escape(s: &str) -> String { s.bytes().map(|b| if b.is_ascii_alphanumeric() || b"/-_.".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect() }

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Scope { pub world_ref: String, pub root: PathBuf, pub ground: String }
impl Scope {
    pub fn root(root: &Path) -> Self { Self { root: root.into(), world_ref: horizon::CONTROL_WORLD_REF.into(), ground: horizon::CONTROL_GROUND_RELATIONS_SOURCE.into() } }
    pub fn project(root: &Path) -> io::Result<Self> {
        let manifest = crate::projectcentral::read_project_manifest(root)?;
        if !manifest.validate().valid { return Err(invalid("Project manifest is invalid")); }
        Ok(Self { root: root.into(), world_ref: format!("project:{}",manifest.project_id), ground: horizon::GROUND_RELATIONS_SOURCE.into() })
    }
    pub fn ground(&self) -> io::Result<Value> {
        let value = read_json(&self.root, &self.ground)?.unwrap_or_else(|| json!({"schema":if self.world_ref==horizon::CONTROL_WORLD_REF { horizon::CONTROL_GROUND_RELATIONS_SCHEMA } else { horizon::GROUND_RELATIONS_SCHEMA },"project_id":self.world_ref.strip_prefix("project:").unwrap_or(&self.world_ref),"relations":[]}));
        let expected = if self.world_ref==horizon::CONTROL_WORLD_REF { horizon::CONTROL_GROUND_RELATIONS_SCHEMA } else { horizon::GROUND_RELATIONS_SCHEMA };
        if value["schema"]!=expected || value["project_id"]!=self.world_ref.strip_prefix("project:").unwrap_or(&self.world_ref) || !value["relations"].is_array() { return Err(invalid("Source relation ground schema/identity is invalid")); }
        if !value["file_map"].is_null() && value["file_map"]["schema"]!=SCHEMA { return Err(invalid("Unsupported file-map ground schema")); }
        Ok(value)
    }
    pub fn revision(&self) -> io::Result<String> {
        Ok(if checked(&self.root,&self.ground)?.exists() { digest(crate::source_safety::read(&self.root,&self.ground)?.as_bytes()) } else { "absent".into() })
    }
    pub(crate) fn save(&self, v: &Value) -> io::Result<()> {
        let parent = Path::new(&self.ground).parent().unwrap().to_str().ok_or_else(||invalid("Non UTF-8 ground"))?;
        safe_directories(&self.root,parent)?;
        checked(&self.root,&self.ground)?;
        write_json(&self.root.join(&self.ground),v)
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Resource {
    #[serde(rename="ref")] pub source_ref: String,
    pub path: String,
    #[serde(default)] pub roles: Vec<String>,
    pub provenance: String,
    pub standing: String,
    pub treatment: String,
    #[serde(default, skip_serializing_if="Option::is_none")] pub external_root: Option<PathBuf>,
    #[serde(default, skip_serializing_if="Option::is_none")] pub resource_uri: Option<String>,
    #[serde(default)] pub title: String,
    #[serde(default)] pub tags: Vec<String>,
    #[serde(default)] pub native_import: bool,
}
impl Resource {
    fn from_binding(b: horizon::SourceBinding) -> Self { Self { source_ref:b.source_ref,path:b.path,roles:b.roles,provenance:b.provenance,standing:b.standing,treatment:b.treatment,external_root:None,resource_uri:None,title:String::new(),tags:vec![],native_import:false } }
    pub(crate) fn location(&self, scope: &Scope) -> io::Result<PathBuf> {
        let root = self.external_root.as_deref().unwrap_or(&scope.root);
        if root.canonicalize()? != root { return Err(denied("Registered source root redirected")); }
        checked(root,&self.path)
    }
    pub(crate) fn allowed(&self,scope:&Scope) -> io::Result<bool> {
        if self.resource_uri.is_some() { return Ok(true); }
        let p=self.location(scope)?;
        let root=self.external_root.as_deref().unwrap_or(&scope.root);
        // Check ancestors too: a linked external resource cannot bypass a deny marker
        // simply by registering a narrower root below it.
        Ok(p.exists() && p.ancestors().all(|d| !d.join(crate::control::AGENT_RETRIEVAL_DENY_MARKER).exists()) && horizon::retrieval_allowed(root,&p))
    }
    pub(crate) fn revision(&self,scope:&Scope) -> io::Result<String> {
        if let Some(uri)=&self.resource_uri { return Ok(digest(uri.as_bytes())); }
        let p=self.location(scope)?;
        let m=fs::symlink_metadata(&p)?;
        if m.is_dir() { return Ok(format!("directory:{}:{}",m.dev(),m.ino())); }
        if !m.is_file() { return Err(invalid("Only files, directories and explicit URIs are mapped")); }
        Ok(horizon::content_revision(&p)?.revision)
    }
    fn content(&self, scope:&Scope) -> io::Result<String> {
        if self.resource_uri.is_some() || self.location(scope)?.is_dir() { return Ok(String::new()); }
        crate::source_safety::read(self.external_root.as_deref().unwrap_or(&scope.root),&self.path)
    }
    fn uri(&self,scope:&Scope) -> io::Result<String> {
        match &self.resource_uri { Some(uri)=>Ok(uri.clone()),None=>Ok(format!("file://{}",escape(self.location(scope)?.to_str().ok_or_else(||invalid("Non UTF-8 source"))?))) }
    }
}

pub(crate) fn resources(scope:&Scope) -> io::Result<Vec<Resource>> {
    let bindings = if scope.world_ref==horizon::CONTROL_WORLD_REF { horizon::control_source_bindings(&scope.root)? } else { horizon::project_source_bindings(&scope.root)? };
    let mut resources:BTreeMap<String,Resource> = bindings.into_iter().map(|b|(b.source_ref.clone(),Resource::from_binding(b))).collect();
    let ground=scope.ground()?;
    let mut declared_refs=BTreeSet::new();let mut declared_locations=BTreeSet::new();
    for v in ground["relations"].as_array().unwrap() {
        let r:Resource=serde_json::from_value(v.clone()).map_err(io::Error::other)?;
        relative(&r.path)?;
        if r.source_ref.trim().is_empty() || r.source_ref.contains(['\n','\r']) || !declared_refs.insert(r.source_ref.clone()) || !declared_locations.insert((r.external_root.clone(),r.path.clone())) {
            return Err(conflict("Duplicate or invalid authored source binding"));
        }
        resources.retain(|_,old| old.path!=r.path || old.external_root!=r.external_root);
        resources.insert(r.source_ref.clone(),r);
    }
    if resources.len()>LIMIT { return Err(invalid("Map source count exceeds bounded reading")); }
    Ok(resources.into_values().collect())
}
pub(crate) fn scopes(root:&Path,input:&Value) -> io::Result<Vec<Scope>> {
    if let Some(path)=input["project_path"].as_str() { return Ok(vec![Scope::project(&Path::new(path).canonicalize()?)?]); }
    if let Some(project)=input["project"].as_str() { return Ok(vec![Scope::project(&checked(root,&format!("Work/{}",relative(project)?.display()))?)?]); }
    if input["scope"].as_str()==Some("project") { return Ok(vec![Scope::project(root)?]); }
    let mut out=vec![Scope::root(root)];
    if input["scope"].as_str()!=Some("all") { return Ok(out); }
    let work=checked(root,"Work")?;
    if work.is_dir() {
        let mut paths=fs::read_dir(&work)?.map(|e|e.map(|e|e.path())).collect::<io::Result<Vec<_>>>()?; paths.sort();
        for p in paths { if fs::symlink_metadata(&p)?.is_dir() && p.join("ProjectCentral").join(crate::projectcentral::PROJECT_MANIFEST).is_file() { out.push(Scope::project(&p)?); } }
    }
    if let Some(extra)=out[0].ground()?["file_map"]["scopes"].as_array() {
        for item in extra {
            let p=Path::new(text(item,"path")?);let p=if p.is_absolute(){p.to_path_buf()}else{checked(root,p.to_str().unwrap())?};
            let s=Scope::project(&p)?;
            if s.world_ref!=text(item,"world_ref")? { return Err(conflict("Registered Project scope changed identity")); }
            if !out.iter().any(|old|old.root==s.root){out.push(s);}
        }
    }
    let mut ids=BTreeSet::new();
    if out.iter().any(|s|!ids.insert(s.world_ref.clone())) { return Err(conflict("Two placements claim the same World; select one explicitly")); }
    Ok(out)
}
pub(crate) fn resolve(root:&Path,input:&Value,reference:&str) -> io::Result<(Scope,Resource)> {
    let mut found=None;
    for s in scopes(root,input)? {
        for r in resources(&s)? { if r.source_ref==reference {
            if found.is_some(){return Err(conflict("SourceRef has conflicting owning scopes"));}
            found=Some((s.clone(),r));
        }}
    }
    found.ok_or_else(||io::Error::new(io::ErrorKind::NotFound,"SourceRef is not in the requested map"))
}
#[derive(Clone, Debug, Serialize, Deserialize)]
struct Binding { row:i64, revision:String, uri:String, #[serde(default)] imported:bool, #[serde(default)] description_prefix:String, #[serde(default)] declaration:String, #[serde(default)] declared_title:String, #[serde(default)] declared_tags:Vec<String> }
#[derive(Default,Serialize,Deserialize)]
struct IndexState { #[serde(default)] world_ref:String, #[serde(default)] entries:BTreeMap<String,Binding> }
fn state(s:&Scope)->io::Result<IndexState>{
    let state:IndexState=match read_json(&s.root,STATE)?{Some(v)=>serde_json::from_value(v).map_err(io::Error::other)?,None=>IndexState::default()};
    if !state.world_ref.is_empty() && state.world_ref!=s.world_ref{return Err(conflict("Index belongs to a different World"));}Ok(state)
}
fn marker(r:&Resource)->String{format!("{MARK}{}\n",r.source_ref)}
fn source_from_description(desc:&str)->Option<&str>{desc.rsplit_once(MARK)?.1.lines().next()}
fn native_name(content:&str)->Option<String>{
    content.lines().take(40).find_map(|line| {
        let line=line.trim().trim_start_matches('#').trim();
        line.strip_prefix("name:").map(|s|s.trim().trim_matches(['\'','"']).to_owned()).filter(|s|!s.is_empty())
    })
}
fn refresh(s:&Scope)->io::Result<Value>{
    let _lock=crate::source_safety::lock(&s.root,"bkmr.lock")?;
    let engine=crate::bkmr::Bkmr::new(&s.root);let version=engine.version()?;
    engine.prepare()?;
    let mut state=state(s)?;state.world_ref=s.world_ref.clone();
    let mut records=engine.records(None,&[],false,LIMIT+1)?;
    if records.len()>LIMIT{return Err(invalid("bkmr map exceeds supported record count"));}
    let current=resources(s)?;
    let mut changes=Vec::new();let mut gaps=Vec::new();
    // A revoked/missing source is removed only from rows bearing our exact binding.
    let allowed:BTreeSet<_>=current.iter().filter_map(|r|r.allowed(s).ok().filter(|v|*v).map(|_|r.source_ref.clone())).collect();
    for (reference,binding) in state.entries.clone(){
        if !allowed.contains(&reference){
            if let Some(row)=records.iter().find(|r|r.id==binding.row){
                if source_from_description(&row.description)!=Some(reference.as_str()){return Err(conflict("Owned bkmr row was replaced; refusing to delete someone else's record"));}
                engine.delete(row.id)?;records.retain(|r|r.id!=binding.row);
            }
            state.entries.remove(&reference);changes.push(json!({"ref":reference,"change":"withdrawn"}));
        }
    }
    for r in current {
        if !r.allowed(s)?{continue;}
        let revision=r.revision(s)?;let uri=r.uri(s)?;let declaration=digest(&serde_json::to_vec(&r).map_err(io::Error::other)?);
        let body=match r.content(s){Ok(body)=>body,Err(e) if matches!(e.kind(),io::ErrorKind::InvalidData)=>{gaps.push(json!({"ref":r.source_ref,"content_index":"metadata-only","reason":e.to_string()}));String::new()},Err(e)=>return Err(e)};
        let existing=records.iter().filter(|v|source_from_description(&v.description)==Some(r.source_ref.as_str())).collect::<Vec<_>>();
        if existing.len()>1{return Err(conflict("Duplicate native bindings for one SourceRef"));}
        let recovered=existing.first().copied().cloned();
        let previous=state.entries.get(&r.source_ref).cloned();
        if let Some(p)=&previous{if recovered.as_ref().map(|v|v.id)!=Some(p.row){return Err(conflict("Native row binding changed; explicit adoption is required"));}}
        if previous.as_ref().is_some_and(|b|b.revision==revision && b.uri==uri && b.declaration==declaration){continue;}
        let mut row=recovered;
        let mut imported=previous.as_ref().is_some_and(|b|b.imported);
        if r.native_import {
            if r.external_root.is_some() || r.resource_uri.is_some(){return Err(invalid("Native imports require a source inside its World"));}
            let name=native_name(&body).ok_or_else(||invalid("Native import requires bkmr-compatible name metadata"))?;
            let collision=records.iter().any(|v|v.title==name && row.as_ref().is_none_or(|old|old.id!=v.id));
            // bkmr also has unique-content constraints. A second physical source
            // must not become the first source merely because bytes coincide.
            let content_collision=records.iter().any(|v|body.contains(v.url.trim()) && !v.url.is_empty() && row.as_ref().is_none_or(|old|old.id!=v.id));
            if collision || content_collision {
                if imported{return Err(conflict("Native import would overwrite another source"));}
                gaps.push(json!({"ref":r.source_ref,"native_import":"uri-fallback","reason":"duplicate native title/content"}));
            }else{
                let prior=row.clone();
                if let Some(prior)=&prior {
                    // Importer matches by source name, not row/path, and ignores a
                    // path-only change. A transient metadata delta forces native
                    // file-path refresh; the exact human title/tags are restored.
                    engine.update(prior.id,&["--title".into(),name.clone(),"--tags".into(),"central-refresh-pending".into()])?;
                }
                engine.import(&r.path)?;
                records=engine.records(None,&[],false,LIMIT+1)?;
                row=records.iter().find(|v|v.title==name).cloned();
                if row.is_none(){return Err(invalid("bkmr skipped the requested native import; no success is inferred"));}
                if let Some(prior)=prior {
                    if row.as_ref().unwrap().id!=prior.id{return Err(conflict("Native import changed row identity unexpectedly"));}
                    engine.update(prior.id,&["--title".into(),prior.title.clone(),"--force".into(),"--tags".into(),prior.tags.join(",")])?;
                }
                imported=true;
            }
        }
        let prefix=row.as_ref().map(|v|v.description.split(MARK).next().unwrap_or("").to_owned()).unwrap_or_default();
        let description=format!("{}{}{}",prefix,marker(&r),if imported{String::new()}else{body.chars().take(INDEX_BYTES/4).collect::<String>()});
        if let Some(old)=&row {
            let mut fields=vec!["--description".into(),description];
            if !imported {fields.extend(["--url".into(),uri.clone()]);}
            if previous.as_ref().is_some_and(|p|p.declared_title!=r.title) && !r.title.is_empty() {fields.extend(["--title".into(),r.title.clone()]);}
            let mut tags=old.tags.clone();
            if let Some(p)=&previous{tags.retain(|t|!p.declared_tags.contains(t)||r.tags.contains(t));}
            for tag in &r.tags{if !tags.contains(tag){tags.push(tag.clone());}}
            fields.extend(["--force".into(),"--tags".into(),tags.join(",")]);
            engine.update(old.id,&fields)?;
        }else{
            if records.iter().any(|v|v.url==uri){return Err(conflict("An unowned bookmark already names this URI; adopt its native row explicitly"));}
            engine.add_uri(&uri,if r.title.is_empty(){&r.path}else{&r.title},&description,&r.tags)?;
        }
        records=engine.records(None,&[],false,LIMIT+1)?;
        let rows=records.iter().filter(|v|source_from_description(&v.description)==Some(r.source_ref.as_str())).collect::<Vec<_>>();
        if rows.len()!=1{return Err(conflict("Native mutation did not yield one exact source binding"));}
        if !r.allowed(s)? || r.revision(s)?!=revision {
            engine.delete(rows[0].id)?;
            return Err(conflict("Source/policy changed during indexing; indexed material was withdrawn"));
        }
        state.entries.insert(r.source_ref.clone(),Binding{row:rows[0].id,revision,uri,imported,description_prefix:prefix,declaration,declared_title:r.title.clone(),declared_tags:r.tags.clone()});
        write_json(&s.root.join(STATE),&state)?;
        changes.push(json!({"ref":r.source_ref,"change":"indexed","native_id":rows[0].id,"native_import":imported,"indexed_text_limit":INDEX_BYTES/4}));
    }
    write_json(&s.root.join(STATE),&state)?;
    Ok(json!({"world_ref":s.world_ref,"version":version,"changes":changes,"gaps":gaps,"indexed":state.entries.len()}))
}
fn reading(s:&Scope,r:&Resource,include_content:bool)->io::Result<Value>{
    if !r.allowed(s)?{return Err(denied("Source is missing or excluded from retrieval"));}
    let ground_revision=s.revision()?;
    let revision=r.revision(s)?;
    let content=if include_content {Some(r.content(s)?)}else{None};
    if !r.allowed(s)? || r.revision(s)?!=revision || s.revision()?!=ground_revision{return Err(conflict("Source or relation ground changed during reading"));}
    Ok(json!({"schema":SCHEMA,"world_ref":s.world_ref,"scope_root":s.root,"source":r,"revision":revision,"location":if r.resource_uri.is_some(){Value::Null}else{json!(r.location(s)?)},"uri":r.uri(s)?,"content":content,"automatic_agent_or_model_invocation":false}))
}
pub(crate) fn authorize(input:&Value)->io::Result<()> {
    text(input,"actor")?;
    if input["actor_kind"]!="human" || !input["agent_session_ref"].is_null(){return Err(denied("Durable relation/placement changes require the human's explicit operation; agent proposals do not author ground"));}
    Ok(())
}
pub(crate) fn cas(s:&Scope,input:&Value)->io::Result<()> {if s.revision()?!=text(input,"expected_revision")?{return Err(conflict("Source relation revision changed"));} Ok(())}
pub(crate) fn put_resource(entries:&mut Vec<Value>,r:&Resource)->io::Result<()> {
    let new=serde_json::to_value(r).map_err(io::Error::other)?;
    let mut prior=entries.iter().find(|v|v["ref"]==r.source_ref).cloned().unwrap_or(json!({}));
    prior.as_object_mut().ok_or_else(||invalid("Malformed source relation"))?.extend(new.as_object().unwrap().clone());
    entries.retain(|v|v["ref"]!=r.source_ref);entries.push(prior);Ok(())
}
pub(crate) fn map_extension(ground:&mut Value)->io::Result<&mut Value>{
    if ground["file_map"].is_null(){ground["file_map"]=json!({"schema":SCHEMA,"links":[],"scopes":[]});}
    if ground["file_map"]["schema"]!=SCHEMA{return Err(invalid("Unknown map ground schema"));}
    Ok(&mut ground["file_map"])
}
fn register_resource(root:&Path,s:&Scope,input:&Value)->io::Result<Value>{
    authorize(input)?;
    let _lock=crate::source_safety::lock(&s.root,"source-mutation.lock")?;cas(s,input)?;
    let path=text(input,"path")?;relative(path)?;
    if Path::new(path).components().any(|c|c.as_os_str()==".central" || c.as_os_str()==".git"){return Err(denied("Operational state is not authored map source"));}
    let external_root=input["external_root"].as_str().map(|v|Path::new(v).canonicalize()).transpose()?;
    let resource_uri=input["uri"].as_str().map(str::to_owned);
    let abs=external_root.as_deref().unwrap_or(&s.root).join(path);
    let own=resources(s)?.into_iter().find(|r|r.path==path && r.external_root==external_root);
    // Root registration does not mint a second source for a Project-owned file.
    for owner in scopes(root,&json!({"scope":"all"}))? {
        if owner.world_ref!=horizon::CONTROL_WORLD_REF && owner.root!=s.root && abs.starts_with(&owner.root) && abs!=owner.root {
            return Err(invalid(format!("Register this resource in its owning World {} at {}",owner.world_ref,owner.root.display())));
        }
    }
    let reference=own.as_ref().map(|r|r.source_ref.clone()).or_else(||input["source_ref"].as_str().map(str::to_owned)).unwrap_or_else(||horizon::source_ref(&s.world_ref,path));
    if let Some(requested)=input["source_ref"].as_str(){if requested!=reference{return Err(conflict("Existing source identity cannot be replaced by registration"));}}
    let mut r=own.unwrap_or(Resource{source_ref:reference,path:path.into(),roles:vec!["registered-file".into()],provenance:"unresolved".into(),standing:"unspecified".into(),treatment:"retain-native-in-place".into(),external_root,resource_uri,title:path.into(),tags:vec![],native_import:false});
    if let Some(title)=input["title"].as_str(){r.title=title.into();}
    if let Some(tags)=input.get("tags"){r.tags=serde_json::from_value(tags.clone()).map_err(io::Error::other)?;if r.tags.iter().any(|t|t.is_empty()||t.contains(',')||t.contains('\n')){return Err(invalid("Invalid bkmr tag"));}}
    if let Some(import)=input["native_import"].as_bool(){r.native_import=import;}
    if !r.allowed(s)?{return Err(denied("Cannot register an unreadable or excluded source"));}
    r.revision(s)?;
    let mut ground=s.ground()?;let entries=ground["relations"].as_array_mut().unwrap();
    if entries.iter().any(|v|v["ref"]==r.source_ref && v["path"]!=r.path){return Err(conflict("SourceRef already resolves another path; use the native move operation"));}
    put_resource(entries,&r)?;s.save(&ground)?;
    Ok(json!({"schema":SCHEMA,"world_ref":s.world_ref,"source":r,"ground_revision":s.revision()?}))
}
fn search(root:&Path,input:&Value)->io::Result<Value>{
    let query=text(input,"query")?;
    let tags:Vec<String>=serde_json::from_value(input.get("tags").cloned().unwrap_or(json!([]))).map_err(io::Error::other)?;
    let mode=input["mode"].as_str().unwrap_or("fulltext");
    // Indexing is deliberately offline/no-embed until an explicit embed campaign
    // has produced evidence. CLI availability is not index readiness.
    if mode!="fulltext"{return Err(invalid("Semantic/hybrid index readiness has not been established for this map; fulltext and tag search are available"));}
    let limit=input["limit"].as_u64().unwrap_or(50).min(1000) as usize;
    if limit==0{return Ok(json!({"schema":SCHEMA,"hits":[],"absences":[]}));}
    let restricted=input["source_refs"].as_array().map(|a|a.iter().filter_map(Value::as_str).collect::<BTreeSet<_>>());
    let mut hits=Vec::new();let mut absences=Vec::new();
    for scope in scopes(root,input)? {
        let state=state(&scope)?;
        let current=resources(&scope)?;
        let by_id:BTreeMap<_,_>=state.entries.iter().map(|(k,v)|(v.row,(k,v))).collect();
        // Query full native candidate set; permissions are checked against live
        // owner ground before any snippet/metadata crosses this public boundary.
        let rows=match crate::bkmr::Bkmr::new(&scope.root).records(Some(query),&tags,false,LIMIT+1){Ok(r)=>r,Err(e)=>{absences.push(json!({"world_ref":scope.world_ref,"reason":e.to_string()}));continue;}};
        if rows.len()>LIMIT{return Err(invalid("Native result count exceeds bounded search"));}
        for row in rows {
            let Some((reference,binding))=by_id.get(&row.id)else{continue;};
            if restricted.as_ref().is_some_and(|a|!a.contains(reference.as_str())){continue;}
            let Some(r)=current.iter().find(|r|&r.source_ref==*reference)else{continue;};
            if !r.allowed(&scope)?{continue;}
            if source_from_description(&row.description)!=Some(reference.as_str()){return Err(conflict("bkmr row/source binding mismatch"));}
            if r.revision(&scope)?!=binding.revision{absences.push(json!({"ref":reference,"reason":"source revision changed; refresh required"}));continue;}
            let source=reading(&scope,r,false)?;
            let snippet=if binding.imported {row.url.clone()}else{row.description.split_once(&marker(r)).map(|(_,v)|v.to_owned()).unwrap_or_default()};
            hits.push(json!({"ref":reference,"world_ref":scope.world_ref,"native_id":row.id,"title":row.title,"tags":row.tags,"snippet":snippet.chars().take(1000).collect::<String>(),"source":source,"mode":mode}));
        }
    }
    hits.truncate(limit);Ok(json!({"schema":SCHEMA,"hits":hits,"absences":absences}))
}
pub fn execute(root:&Path,op:&str,input:&Value)->io::Result<Value>{
    let root=root.canonicalize()?;
    if op=="search"{return search(&root,input);}
    if op=="skill-tree"{return crate::file_map_skills::tree(&root,input);}
    if op=="resolve" || op=="read" {
        let (s,r)=resolve(&root,input,text(input,"source_ref")?)?;
        let out=reading(&s,&r,op=="read")?;
        if let Some(expected)=input["expected_revision"].as_str(){if out["revision"]!=expected{return Err(conflict("Requested source revision is stale"));}}
        return Ok(out);
    }
    let selected=scopes(&root,input)?;
    if op=="inspect" {
        let mut values=Vec::new();for s in selected {
            let mut visible=Vec::new();let mut absent=Vec::new();
            for r in resources(&s)?{match reading(&s,&r,false){Ok(v)=>visible.push(v),Err(e)=>absent.push(json!({"ref":r.source_ref,"reason":e.to_string()}))}}
            values.push(json!({"world_ref":s.world_ref,"root":s.root,"ground_revision":s.revision()?,"database_present":s.root.join(crate::bkmr::DB).is_file(),"resources":visible,"absences":absent,"links":s.ground()?["file_map"]["links"],"capabilities":{"fulltext":s.root.join(crate::bkmr::DB).is_file(),"tags":true,"semantic":false,"hybrid":false,"reason":"No embedding readiness campaign has been run"}}));
        }return Ok(json!({"schema":SCHEMA,"scopes":values}));
    }
    if op=="refresh"{let results=selected.iter().map(refresh).collect::<io::Result<Vec<_>>>()?;return Ok(json!({"schema":SCHEMA,"scopes":results}));}
    if selected.len()!=1{return Err(invalid("Mutation requires one explicit scope"));}let s=&selected[0];
    if op=="register"{return register_resource(&root,s,input);}
    if op=="adopt" {
        authorize(input)?;let _l=crate::source_safety::lock(&s.root,"bkmr.lock")?;cas(s,input)?;
        let src=Path::new(text(input,"database")?);let dst=s.root.join(crate::bkmr::DB);
        if !src.is_absolute(){return Err(invalid("Adoption needs an explicit absolute database path"));}
        crate::bkmr::backup(src,&dst)?;
        crate::bkmr::backup(&dst,&s.root.join(".central/bkmr-adoption-backup.db"))?;
        crate::bkmr::Bkmr::new(&s.root).prepare()?;
        return Ok(json!({"schema":SCHEMA,"adopted":dst,"original_untouched":true,"backup":".central/bkmr-adoption-backup.db","source_bindings":"register explicitly; existing native rows are preserved"}));
    }
    crate::file_map_links::execute(&root,s,op,input)
}
fn action(op:&str,input:&Value,context:&ActionExecutionContext<'_>)->ActionResult{
    let id=format!("central.map.{op}");
    match crate::root::resolve_central_root(context.root_options).map_err(io::Error::other).and_then(|r|execute(&r.path,op,input)) {
        Ok(v)=>ActionResult::success(&id,v),
        Err(e)=>ActionResult::failure_coded(Some(&id),if e.kind()==io::ErrorKind::AlreadyExists{ResultStatus::VerificationFailure}else if e.kind()==io::ErrorKind::PermissionDenied{ResultStatus::UnavailableCapability}else{ResultStatus::InvalidInput},"central.map_operation_failed",e.to_string(),None),
    }
}
fn action_inputs(op:&str)->Vec<ActionInputDefinition> {
    let mut fields=vec![("scope","string",false),("project","string",false),("project_path","string",false)];
    match op {
        "read"|"resolve"|"skill-tree"=>fields.extend([("source_ref","string",true),("expected_revision","string",false)]),
        "search"=>fields.extend([("query","string",true),("tags","array",false),("mode","string",false),("limit","integer",false),("source_refs","array",false)]),
        "register"=>fields.extend([("path","string",true),("source_ref","string",false),("external_root","string",false),("uri","string",false),("title","string",false),("tags","array",false),("native_import","boolean",false)]),
        "link"=>fields.extend([("path","string",true),("source_ref","string",true),("owner","string",true)]),
        "unlink"=>fields.extend([("path","string",true),("owner","string",true)]),
        "adopt"=>fields.push(("database","string",true)),
        "scope"=>fields.push(("path","string",true)),
        "move.plan"=>fields.extend([("from","string",true),("to","string",true),("quiescent","boolean",false)]),
        "move.apply"|"move.rollback"=>fields.push(("plan","object",true)),_=>{}
    }
    if matches!(op,"register"|"link"|"unlink"|"adopt"|"scope") {fields.push(("expected_revision","string",true));}
    if matches!(op,"register"|"link"|"unlink"|"adopt"|"scope"|"repair"|"move.apply"|"move.rollback") {
        fields.extend([("actor","string",true),("actor_kind","string",true),("agent_session_ref","string",false)]);
    }
    fields.into_iter().map(|(name,kind,required)|ActionInputDefinition{name:name.into(),input_type:kind.into(),required,choices:None,selection:None}).collect()
}
pub fn register(registry:&mut ActionRegistry){
    type Handler=fn(&ActionRegistry,&Value,&ActionExecutionContext<'_>)->ActionResult;
    let ops:[(&str,Handler);15]=[
        ("skill-tree",|_,i,c|action("skill-tree",i,c)),("inspect",|_,i,c|action("inspect",i,c)),("register",|_,i,c|action("register",i,c)),("refresh",|_,i,c|action("refresh",i,c)),("search",|_,i,c|action("search",i,c)),("resolve",|_,i,c|action("resolve",i,c)),("read",|_,i,c|action("read",i,c)),("adopt",|_,i,c|action("adopt",i,c)),("link",|_,i,c|action("link",i,c)),("unlink",|_,i,c|action("unlink",i,c)),("repair",|_,i,c|action("repair",i,c)),("move.plan",|_,i,c|action("move.plan",i,c)),("move.apply",|_,i,c|action("move.apply",i,c)),("move.rollback",|_,i,c|action("move.rollback",i,c)),("scope",|_,i,c|action("scope",i,c)),
    ];
    for (op,handler) in ops {
        registry.register(ActionDescriptor{id:format!("central.map.{op}"),title:format!("File map {op}"),description:"Central-owned persistent bkmr map and source locations. Read/search never invokes an opener. Durable relations use explicit human authority and revision checks.".into(),inputs:action_inputs(op),output:ActionOutputDefinition{output_type:SCHEMA.into()},mutation_class:if matches!(op,"inspect"|"search"|"read"|"resolve"|"move.plan"|"skill-tree"){MutationClass::ReadOnly}else{MutationClass::LocallyMutating},preview_supported:false,required_ports:vec![],availability:ActionAvailability{available:true,reason:None}},handler).expect("unique map Action");
    }
}
