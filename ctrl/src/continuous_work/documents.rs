//! Native document payloads live in existing Day/Flow SourceRefs. Template data
//! is retained as supplied; absent original HTML keys are never fabricated.
use super::{authority::Principal, history, placement, source::{self, conflict, denied, encoded, invalid, text, Scope, SourceReading}, temporal};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::{BTreeSet, HashSet}, io, path::Path};

#[path = "document_extensions.rs"]
mod extensions;

pub const SCHEMA: &str = "central.contribution-document/v1";
pub const ROLE: &str = "protected-contribution-document";
const INTENTS: &str = ".central/source-returns/document-mutations";

fn optional_text<'a>(input: &'a Value,key: &str) -> io::Result<Option<&'a str>> {
    match input.get(key) {None|Some(Value::Null)=>Ok(None),Some(Value::String(value)) if value.len()<=4096=>Ok(Some(value)),_=>Err(invalid(format!("{key} must be bounded text or absent")))}
}
fn clean(content: &str) -> io::Result<String> {
    if content.len()>256*1024 || content.contains('\0') {return Err(invalid("a contribution must be bounded UTF-8 text without NUL"));}
    // No URL-bearing elements or attributes: this native text/rich-text cut
    // cannot silently load a remote resource or pretend to support media yet.
    let tags: HashSet<&str> = ["p","br","em","strong","b","i","u","s","blockquote","pre","code","ul","ol","li","h1","h2","h3","h4","hr","table","thead","tbody","tr","th","td"].into_iter().collect();
    Ok(ammonia::Builder::default().tags(tags).generic_attributes(HashSet::new()).tag_attributes(std::collections::HashMap::new()).clean(content).to_string())
}
fn parse(reading: &SourceReading) -> io::Result<Value> {
    let value: Value=serde_json::from_str(&reading.content)?;
    if value["schema"]!=SCHEMA || value["document_id"].as_str().is_none_or(|s|s.trim().is_empty())
        || !matches!(value["kind"].as_str(),Some("day"|"flow"|"dialogue")) {
        return Err(invalid("source is not a native contribution document; do not infer an original HTML template schema"));
    }
    for key in ["fields","entries","contributions","operations"] {
        if value[key].as_array().is_none() {return Err(invalid(format!("native document lacks its {key} array")));}
    }
    let mut ids=BTreeSet::new();
    for collection in ["fields","entries","contributions"] {
        for entry in value[collection].as_array().unwrap() {
            let id=text(entry,"id")?;
            if !ids.insert((collection,id)) {return Err(invalid("duplicate document-local identity requires explicit reconciliation"));}
        }
    }
    extensions::validate(&value)?;
    Ok(value)
}
fn relation(scope: &Scope,reference: &str) -> io::Result<Value> {
    let (relations,_)=scope.relations()?;
    relations["relations"].as_array().ok_or_else(||invalid("invalid source relations"))?.iter()
        .find(|entry|entry["ref"]==reference).cloned().ok_or_else(||invalid("document requires an explicit source relation"))
}
fn metadata(scope: &Scope,reading: &SourceReading,document: &Value) -> io::Result<Value> {
    let entry=relation(scope,&reading.source.source_ref)?;
    let meta=&entry["document"];
    if meta["document_id"]!=document["document_id"] || meta["kind"]!=document["kind"] {
        return Err(conflict("document identity differs from its native source relation"));
    }
    Ok(meta.clone())
}
fn retain_metadata(scope: &Scope,reading: &SourceReading,document: &Value,previous_revision: Option<&str>) -> io::Result<()> {
    let (mut relations,basis)=scope.relations()?;
    let entry=relations["relations"].as_array_mut().ok_or_else(||invalid("invalid source relations"))?.iter_mut()
        .find(|entry|entry["ref"]==reading.source.source_ref).ok_or_else(||invalid("document source relation disappeared"))?;
    if entry["path"]!=reading.source.path {return Err(conflict("document placement changed during mutation"));}
    if let Some(previous)=previous_revision {
        let recorded=entry["document"]["last_native_revision"].as_str();
        if recorded!=Some(previous) && recorded!=Some(reading.revision.revision.as_str()) {return Err(conflict("document metadata changed during mutation recovery"));}
    } else if entry.get("document").is_some() && entry["document"]["document_id"]!=document["document_id"] {
        return Err(conflict("source already owns a different DocumentId"));
    }
    let roles=entry["roles"].as_array_mut().ok_or_else(||invalid("source roles must be an array"))?;
    if !roles.iter().any(|role|role==ROLE) {roles.push(json!(ROLE));}
    entry["document"]=json!({"schema":"central.document-source-relation/v1","document_id":document["document_id"],"kind":document["kind"],"last_native_revision":reading.revision.revision,"source_media_type":"application/json","template_fidelity":document["template_fidelity"]});
    temporal::save_relations(scope,&relations,&basis)
}
pub(crate) fn reading(scope: &Scope,input: &Value) -> io::Result<(SourceReading,Value,Value)> {
    let source=scope.read(text(input,"source_ref")?)?;
    let document=parse(&source)?;
    if let Some(id)=input.get("document_id") {if id!=&document["document_id"] {return Err(conflict("DocumentId does not match SourceRef"));}}
    let meta=metadata(scope,&source,&document)?;
    Ok((source,document,meta))
}
fn result(source: &SourceReading,document: Value,meta: &Value) -> Value {
    json!({"schema":"central.document-reading/v1","source":source.source,"revision":source.revision,
        "document_id":document["document_id"],"document":document,
        "unreviewed_external_revision":meta["last_native_revision"]!=source.revision.revision,
        "last_native_revision":meta["last_native_revision"],
        "source_authority":"live-native-source","automatic_agent_or_model_invocation":false,
        "media_support":"text and sanitised rich text; embedded image/audio admission is not implemented in this cut"})
}
pub fn read(scope: &Scope,input: &Value) -> io::Result<Value> {
    let (source,document,meta)=reading(scope,input)?;
    Ok(result(&source,document,&meta))
}
fn fields(input: &Value,payload: &Value) -> io::Result<Vec<Value>> {
    let fields=input.get("fields").and_then(Value::as_array).cloned().unwrap_or_default();
    if fields.len()>128 {return Err(invalid("at most 128 explicit document fields"));}
    let mut ids=BTreeSet::new();
    for field in &fields {
        if !ids.insert(text(field,"id")?) {return Err(invalid("duplicate native field id"));}
        text(field,"label")?;
        if let Some(pointer)=optional_text(field,"template_pointer")? {
            if payload.pointer(pointer).is_none() {return Err(invalid("field mapping names a missing actual template payload key; no key is manufactured"));}
        }
    }
    Ok(fields)
}
pub fn create(scope: &Scope,input: &Value,principal: &Principal,now: u64) -> io::Result<Value> {
    placement::checked_policy(scope,input,now)?;
    let id=text(input,"document_id")?;
    let kind=text(input,"kind")?;
    if !matches!(kind,"day"|"flow"|"dialogue") {return Err(invalid("document kind is day, flow or dialogue"));}
    if kind=="day" {principal.require_human()?;}
    let digest=source::key(&format!("{}\n{}",principal.principal_ref,serde_json::to_string(input)?));
    for binding in scope.bindings()? {
        if binding.roles.iter().any(|role|role==ROLE) {
            let existing=scope.read(&binding.source_ref)?;
            let document=parse(&existing)?;
            if document["document_id"]==id {
                if document["creation_digest"]!=digest {return Err(conflict("DocumentId already has another creation intent"));}
                return read(scope,&json!({"source_ref":binding.source_ref,"document_id":id}));
            }
        }
    }
    let time=temporal::time_policy(scope,now)?;
    let payload=input.get("template_payload").cloned().unwrap_or_else(||json!({}));
    let explicit_fields=fields(input,&payload)?;
    let mut document=json!({"schema":SCHEMA,"document_id":id,"kind":kind,"title":optional_text(input,"title")?.unwrap_or(""),
        "date":time.civil_date,"created_at_unix_seconds":now,"saved_at_unix_seconds":now,"created_by":principal.principal_ref,
        "creation_digest":digest,"template_payload":payload,"fields":explicit_fields,"entries":[],"contributions":[],"operations":[],
        "template_fidelity":"native-document-with-exact-supplied-payload-not-original-HTML-fidelity","lifecycle":"open"});
    let source=if kind=="day" {
        let day=temporal::day_read(scope,&json!({"day_ref":text(input,"day_ref")?}))?;
        let current=scope.read(day["source"]["ref"].as_str().ok_or_else(||invalid("Day source identity missing"))?)?;
        if current.revision.revision!=text(input,"expected_revision")? {return Err(conflict("Day changed before native document initialisation"));}
        if !current.content.is_empty() {return Err(denied("non-empty human Day bytes are not implicitly converted; retain/import them with explicit reviewed source migration"));}
        document["day_ref"]=day["day_ref"].clone();
        history::replace(scope,&current,&encoded(&document)?,&principal.principal_ref,&principal.actor_kind,now)?
    } else {
        let path=format!("{}/agents/now/flows/{}.json",scope.prefix,source::key(id));
        // Resume an interrupted publication from its complete original payload.
        match crate::source_safety::read(&scope.root,&path) {
            Ok(raw)=>{
                let existing: Value=serde_json::from_str(&raw)?;
                if existing["schema"]!=SCHEMA || existing["document_id"]!=id || existing["creation_digest"]!=digest {return Err(conflict("unbound document path has different bytes/creation identity"));}
                document=existing;
            }
            Err(error) if error.kind()==io::ErrorKind::NotFound=>{},
            Err(error)=>return Err(error),
        }
        let flow=crate::projectcentral_flow::publish_native_document(scope,id,&path,&encoded(&document)?,optional_text(input,"title")?,principal,document["created_at_unix_seconds"].as_u64().unwrap_or(now))?;
        let binding=crate::source_horizon::SourceBinding {source_ref:flow.source_ref,path:flow.path,roles:vec!["flow-transcript".into(),ROLE.into()],provenance:"agent-maintained".into(),standing:"current-development-state".into(),treatment:"generated-derived".into(),agent_retrieval_allowed:true};
        scope.bind(&binding,now)?;
        scope.read(&binding.source_ref)?
    };
    retain_metadata(scope,&source,&document,None)?;
    read(scope,&json!({"source_ref":source.source.source_ref,"document_id":id}))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ContributionAuthor {
    pub principal_ref: String,
    pub actor_kind: String,
}
impl From<&Principal> for ContributionAuthor {
    fn from(principal: &Principal) -> Self {Self {principal_ref:principal.principal_ref.clone(),actor_kind:principal.actor_kind.clone()}}
}
fn array_mut<'a>(document: &'a mut Value,key: &str) -> io::Result<&'a mut Vec<Value>> {document[key].as_array_mut().ok_or_else(||invalid("invalid document collection"))}
fn append(document: &mut Value,input: &Value,author: &ContributionAuthor,reviewer: Option<&Principal>,now: u64,entry: Option<String>,field: Option<String>) -> io::Result<()> {
    let id=text(input,"contribution_id")?;
    if document["contributions"].as_array().unwrap().iter().any(|c|c["id"]==id) {return Err(conflict("ContributionId already exists"));}
    let html=clean(input.get("html").and_then(Value::as_str).ok_or_else(||invalid("contribution html required"))?)?;
    array_mut(document,"contributions")?.push(json!({"id":id,"entry_id":entry,"field_id":field,"html":html,
        "author_ref":author.principal_ref,"actor_kind":author.actor_kind,"display_role":if author.actor_kind=="human" {"F"} else {"H"},
        "occurred_at_unix_seconds":input.get("occurred_at_unix_seconds").cloned().unwrap_or(json!(now)),"received_at_unix_seconds":input.get("received_at_unix_seconds").and_then(Value::as_u64).unwrap_or(now),
        "locked":author.actor_kind=="human","human_touched":author.actor_kind=="human","removed":false,
        "reviewed_by":reviewer.map(|p|p.principal_ref.clone())}));
    Ok(())
}
fn edit(document: &mut Value,input: &Value,author: &ContributionAuthor,reviewer: Option<&Principal>,now: u64) -> io::Result<()> {
    let human=author.actor_kind=="human";
    let reviewed=reviewer.is_some_and(Principal::is_human);
    if document["kind"]=="day" && !human && !reviewed {return Err(denied("human Day contributions require explicit receiving review; direct Agent mutation is not adoption"));}
    if document["lifecycle"]!="open" {return Err(denied("document is closed; reopen it explicitly before contributing"));}
    match text(input,"operation")? {
        "entry.add"=>{
            let id=text(input,"entry_id")?;
            if document["entries"].as_array().unwrap().iter().any(|e|e["id"]==id) {return Err(conflict("EntryId already exists"));}
            let reply=optional_text(input,"reply_to")?;
            if reply.is_some_and(|reference|!document["entries"].as_array().unwrap().iter().any(|e|e["id"]==reference)) {return Err(invalid("reply anchor is not an existing document-local EntryId"));}
            array_mut(document,"entries")?.push(json!({"id":id,"reply_to":reply,"author_ref":author.principal_ref,"occurred_at_unix_seconds":input.get("occurred_at_unix_seconds").cloned().unwrap_or(json!(now)),"received_at_unix_seconds":input.get("received_at_unix_seconds").and_then(Value::as_u64).unwrap_or(now)}));
            if input.get("html").is_some() {append(document,input,author,reviewer,now,Some(id.into()),None)?;}
        }
        "entry.append"=>{
            let id=text(input,"entry_id")?;
            if !document["entries"].as_array().unwrap().iter().any(|e|e["id"]==id) {return Err(invalid("EntryId is not in this document"));}
            append(document,input,author,reviewer,now,Some(id.into()),None)?;
        }
        "field.append"=>{
            let id=text(input,"field_id")?;
            if !document["fields"].as_array().unwrap().iter().any(|e|e["id"]==id) {return Err(invalid("field_id is not an actual supplied document field"));}
            append(document,input,author,reviewer,now,None,Some(id.into()))?;
        }
        "contribution.patch"|"contribution.remove"=>{
            let id=text(input,"contribution_id")?;
            let contribution=array_mut(document,"contributions")?.iter_mut().find(|c|c["id"]==id).ok_or_else(||invalid("ContributionId is not in this document"))?;
            if !human && (contribution["author_ref"]!=author.principal_ref || contribution["locked"]==true || contribution["human_touched"]==true) {
                return Err(denied("Agent may edit only its own unlocked, untouched contribution; review does not transfer human authorship"));
            }
            if contribution["removed"]==true {return Err(conflict("contribution is already removed"));}
            if input["operation"]=="contribution.remove" {contribution["removed"]=json!(true);contribution["html"]=json!("");}
            else {contribution["html"]=json!(clean(input.get("html").and_then(Value::as_str).ok_or_else(||invalid("patch html required"))?)?);}
            if human {contribution["human_touched"]=json!(true);contribution["locked"]=json!(true);}
            contribution["edited_by"]=json!(author.principal_ref);contribution["edited_at_unix_seconds"]=json!(now);
        }
        "field.set"=>{
            if !human {return Err(denied("changing the human field value is not an Agent contribution"));}
            let id=text(input,"field_id")?;
            let field=document["fields"].as_array().unwrap().iter().find(|field|field["id"]==id).ok_or_else(||invalid("field_id not found"))?;
            let pointer=text(field,"template_pointer")?.to_owned();
            let destination=document["template_payload"].pointer_mut(&pointer).ok_or_else(||invalid("actual template field no longer exists"))?;
            *destination=input.get("value").cloned().ok_or_else(||invalid("field value required"))?;
        }
        "title.set"|"summary.set"|"lifecycle.set"=>{
            if !human {return Err(denied("human document metadata requires authenticated human mutation"));}
            let key=match input["operation"].as_str().unwrap() {"title.set"=>"title","summary.set"=>"summary",_=>"lifecycle"};
            let value=input.get("value").and_then(Value::as_str).ok_or_else(||invalid("metadata value must be text"))?;
            if value.len()>65536 {return Err(invalid("metadata value exceeds bounded source field size"));}
            if key=="lifecycle" && !matches!(value,"open"|"closed") {return Err(invalid("document lifecycle is open or closed"));}
            document[key]=json!(value);
        }
        _=>extensions::edit(document,input,author,reviewer,now)?,
    }
    Ok(())
}
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Intent {
    schema: String,
    scope_ref: String,
    source_ref: String,
    source_path: String,
    document_id: String,
    request_key: String,
    digest: String,
    actor: ContributionAuthor,
    reviewer_ref: Option<String>,
    previous_revision: String,
    #[serde(default)]
    previous_metadata_revision: Option<String>,
    next_revision: String,
    content: String,
    operation_at_unix_seconds: u64,
    status: String,
}
fn intent_path(key: &str) -> String {format!("{INTENTS}/{}.json",source::key(key))}
fn save_intent(scope: &Scope,intent: &Intent) -> io::Result<()> {
    source::directories(&scope.root,Path::new(INTENTS))?;
    let bytes=serde_json::to_vec_pretty(intent)?;
    if bytes.len()>crate::source_safety::MAX_SOURCE {return Err(invalid("document recovery intent exceeds current native source size limit"));}
    crate::file_mutation::atomic_record(&scope.root.join(intent_path(&intent.request_key)),&bytes)
}
fn finish(scope: &Scope,intent: &mut Intent) -> io::Result<Value> {
    let current=scope.read(&intent.source_ref)?;
    if current.source.path!=intent.source_path {return Err(conflict("document moved after mutation intent; explicit source recovery required"));}
    let document: Value=serde_json::from_str(&intent.content)?;
    if current.revision.revision==intent.previous_revision {
        if document["kind"]=="day" {
            history::replace(scope,&current,&intent.content,&intent.actor.principal_ref,&intent.actor.actor_kind,intent.operation_at_unix_seconds)?;
        } else {
            crate::projectcentral_flow::write_native_document(scope,&current,&intent.content,&intent.actor.principal_ref,&intent.actor.actor_kind,intent.operation_at_unix_seconds)?;
        }
    } else if current.revision.revision!=intent.next_revision {
        return Err(conflict("document has a later/unrelated revision; interrupted mutation does not overwrite it"));
    }
    let committed=scope.read(&intent.source_ref)?;
    if committed.revision.revision!=intent.next_revision {return Err(conflict("native document commit did not match its intended revision"));}
    retain_metadata(scope,&committed,&document,Some(intent.previous_metadata_revision.as_deref().unwrap_or(&intent.previous_revision)))?;
    intent.status="committed".into();save_intent(scope,intent)?;
    let mut response=read(scope,&json!({"source_ref":intent.source_ref,"document_id":intent.document_id}))?;
    response["operation_receipt"]=json!({"request_key":intent.request_key,"status":intent.status,"actor":intent.actor,"reviewed_by":intent.reviewer_ref,"previous_revision":intent.previous_revision,"revision":intent.next_revision});
    Ok(response)
}
pub fn mutate(scope: &Scope,input: &Value,principal: &Principal,now: u64) -> io::Result<Value> {
    mutate_as(scope,input,&ContributionAuthor::from(principal),None,now)
}
pub(crate) fn mutate_reviewed(scope: &Scope,input: &Value,author: &ContributionAuthor,reviewer: &Principal,now: u64) -> io::Result<Value> {
    reviewer.require_human()?;
    mutate_as(scope,input,author,Some(reviewer),now)
}
fn mutate_as(scope: &Scope,input: &Value,author: &ContributionAuthor,reviewer: Option<&Principal>,now: u64) -> io::Result<Value> {
    for key in ["occurred_at_unix_seconds","received_at_unix_seconds"] {
        if input.get(key).is_some_and(|v| !v.is_null() && v.as_u64().is_none()) {
            return Err(invalid(format!("{key} must be an integer or absent")));
        }
    }
    let reference=text(input,"source_ref")?;
    let id=text(input,"document_id")?;
    let request_key=format!("{}:{}:{}:{}",scope.world_ref,reference,author.principal_ref,text(input,"request_id")?);
    let digest=source::key(&serde_json::to_string(input)?);
    match crate::source_safety::read(&scope.root,&intent_path(&request_key)) {
        Ok(raw)=>{
            let mut intent: Intent=serde_json::from_str(&raw)?;
            if intent.schema!="central.document-mutation/v1" || intent.scope_ref!=scope.world_ref || intent.source_ref!=reference || intent.document_id!=id
                || intent.request_key!=request_key || intent.digest!=digest || intent.actor.principal_ref!=author.principal_ref
                || intent.reviewer_ref!=reviewer.map(|p|p.principal_ref.clone()) {return Err(conflict("document operation identity has different input or reviewed authority"));}
            if intent.status=="committed" {
                let mut response=read(scope,input)?;
                response["replayed_operation"]=json!({"request_key":request_key,"applied_revision":intent.next_revision,"current_source_not_rewritten":true});
                return Ok(response);
            }
            return finish(scope,&mut intent);
        }
        Err(error) if error.kind()==io::ErrorKind::NotFound=>{},
        Err(error)=>return Err(error),
    }
    let (current,mut document,meta)=reading(scope,input)?;
    if current.revision.revision!=text(input,"expected_revision")? {return Err(conflict("document source revision changed"));}
    let external = meta["last_native_revision"]!=current.revision.revision;
    if input["operation"]=="external.reconcile" {
        if !external {return Err(conflict("document has no external revision to reconcile"));}
        if meta["last_native_revision"]!=text(input,"expected_native_revision")? {return Err(conflict("last native document basis changed"));}
        extensions::reconcile(&mut document,input,author,now)?;
    } else {
        if external {return Err(denied("external human/source edit is protected; reconcile its exact retained revision before further native contribution mutation"));}
        // Reopening is explicit; restoration never silently changes lifecycle.
        if input["operation"]=="lifecycle.set" && input["value"]=="open" && author.actor_kind=="human" {document["lifecycle"]=json!("open");}
        edit(&mut document,input,author,reviewer,now)?;
    }
    extensions::refresh_anchors(&mut document)?;
    extensions::validate(&document)?;
    document["saved_at_unix_seconds"]=json!(now);
    array_mut(&mut document,"operations")?.push(json!({"request_key":request_key,"digest":digest,"actor_ref":author.principal_ref,"reviewed_by":reviewer.map(|p|p.principal_ref.clone()),"recorded_at_unix_seconds":now}));
    let content=encoded(&document)?;
    let mut intent=Intent {schema:"central.document-mutation/v1".into(),scope_ref:scope.world_ref.clone(),source_ref:reference.into(),source_path:current.source.path,document_id:id.into(),request_key,digest,actor:author.clone(),reviewer_ref:reviewer.map(|p|p.principal_ref.clone()),previous_revision:current.revision.revision,previous_metadata_revision:meta["last_native_revision"].as_str().map(str::to_owned),next_revision:source::revision(&content),content,operation_at_unix_seconds:now,status:"prepared".into()};
    save_intent(scope,&intent)?;
    finish(scope,&mut intent)
}
pub fn export(scope: &Scope,input: &Value) -> io::Result<Value> {
    extensions::export(scope,input)
}
