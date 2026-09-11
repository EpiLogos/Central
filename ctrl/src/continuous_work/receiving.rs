//! Receiving extends the existing source-return owner and lock. Acceptance and
//! inclusion are separate native mutations; arrival never edits a human Day.
use super::{authority::{self,Principal}, documents::{self,ContributionAuthor}, source::{self,conflict,denied,invalid,text,Scope}};
use serde::{Deserialize,Serialize};
use serde_json::{json,Value};
use std::{fs,io,path::Path};

#[path = "receiving_aperture.rs"]
mod aperture;

const AREA: &str=".central/source-returns/contributions";
#[derive(Debug,Clone,Serialize,Deserialize)]
struct Review {
    reviewer_ref: String,
    authority_ref: String,
    authority_revision: String,
    disposition: String,
    source_revision: String,
    reviewed_at_unix_seconds: u64,
}
#[derive(Debug,Clone,Serialize,Deserialize)]
struct Received {
    schema: String,
    return_ref: String,
    scope_ref: String,
    sequence: u64,
    request_digest: String,
    source_ref: String,
    document_id: String,
    proposed_source_revision: String,
    proposal: Value,
    author: ContributionAuthor,
    authority_ref: String,
    authority_revision: String,
    occurred_at_unix_seconds: Option<u64>,
    received_at_unix_seconds: u64,
    now_ref: Option<String>,
    day_ref: Option<String>,
    task_ref: Option<String>,
    run_ref: Option<String>,
    session_ref: Option<String>,
    status: String,
    stale_at_arrival: bool,
    review: Option<Review>,
    inclusion_request: Option<Value>,
    applied_source_revision: Option<String>,
    last_error: Option<String>,
    #[serde(default)]
    acknowledgement: Option<Value>,
    #[serde(default)]
    summary: Option<String>,
    #[serde(default)]
    evidence_refs: Vec<String>,
    #[serde(default)]
    artifacts: Vec<Value>,
}
fn path(reference: &str) -> String {format!("{AREA}/{}.json",source::key(reference))}
fn read_record(scope: &Scope,reference: &str) -> io::Result<(Received,String)> {
    let raw=crate::source_safety::read(&scope.root,&path(reference))?;
    let record: Received=serde_json::from_str(&raw)?;
    if record.schema!="central.received-contribution/v1" || record.return_ref!=reference || record.scope_ref!=scope.world_ref {return Err(invalid("receiving identity/scope mismatch"));}
    Ok((record,source::revision(&raw)))
}
fn write(scope: &Scope,record: &Received) -> io::Result<String> {
    source::directories(&scope.root,Path::new(AREA))?;
    let raw=source::encoded(record)?;
    if raw.len()>crate::source_safety::MAX_SOURCE {return Err(invalid("receiving record exceeds native bounded persistence"));}
    crate::file_mutation::atomic_record(&scope.root.join(path(&record.return_ref)),raw.as_bytes())?;
    Ok(source::revision(&raw))
}
fn require_disclosure(scope: &Scope,record: &Received) -> io::Result<()> {
    scope.read(&record.source_ref)?;
    for artifact in &record.artifacts {scope.read(text(&artifact["source"],"ref")?)?;}
    Ok(())
}
fn response(record: &Received,revision: &str) -> Value {
    json!({"schema":"central.receiving-reading/v1","return_ref":record.return_ref,"revision":revision,"record":record,
        "source_changed_by_arrival_or_review":false,"included":record.status=="included","automatic_agent_or_model_invocation":false})
}
fn optional(input: &Value,key: &str) -> io::Result<Option<String>> {
    match input.get(key) {None|Some(Value::Null)=>Ok(None),Some(Value::String(value)) if !value.trim().is_empty()&&value.len()<=4096=>Ok(Some(value.clone())),_=>Err(invalid(format!("{key} requires bounded non-empty reference text")))}
}
fn next_sequence(scope: &Scope) -> io::Result<u64> {
    source::directories(&scope.root,Path::new(AREA))?;
    let cursor_path=format!("{AREA}/cursor.json");
    let previous=match crate::source_safety::read(&scope.root,&cursor_path) {
        Ok(raw)=>{
            let value: Value=serde_json::from_str(&raw)?;
            if value["schema"]!="central.receiving-cursor/v1" {return Err(invalid("invalid receiving cursor schema"));}
            value["sequence"].as_u64().ok_or_else(||invalid("invalid receiving sequence"))?
        }
        Err(error) if error.kind()==io::ErrorKind::NotFound=>0,
        Err(error)=>return Err(error),
    };
    let next=previous.checked_add(1).ok_or_else(||invalid("receiving sequence exhausted"))?;
    crate::file_mutation::atomic_record(&scope.root.join(cursor_path),&serde_json::to_vec(&json!({"schema":"central.receiving-cursor/v1","sequence":next}))?)?;
    Ok(next)
}
fn submit(scope: &Scope,input: &Value,principal: &Principal,now: u64) -> io::Result<Value> {
    let reference=format!("central:return:{}:{}",scope.world_ref,source::key(&format!("{}\n{}",principal.principal_ref,text(input,"producer_key")?)));
    let digest=source::key(&serde_json::to_string(input)?);
    match read_record(scope,&reference) {
        Ok((existing,revision))=>{
            if existing.request_digest!=digest || existing.author.principal_ref!=principal.principal_ref {return Err(conflict("Return producer key already has different content or attribution"));}
            require_disclosure(scope,&existing)?;
            return Ok(response(&existing,&revision));
        }
        Err(error) if error.kind()==io::ErrorKind::NotFound=>{},
        Err(error)=>return Err(error),
    }
    let (source,document,_)=documents::reading(scope,input)?;
    let proposal=input.get("proposal").cloned().filter(Value::is_object).ok_or_else(||invalid("proposal must be a native document operation object"))?;
    text(&proposal,"operation")?;
    for reserved in ["source_ref","document_id","expected_revision","request_id","project","actor_kind","author_ref","actor"] {
        if proposal.get(reserved).is_some() {return Err(invalid("proposal cannot supply owner-resolved identity, basis or authenticated attribution fields"));}
    }
    if serde_json::to_vec(&proposal)?.len()>512*1024 {return Err(invalid("proposal exceeds bounded receiving size"));}
    let expected=text(input,"expected_source_revision")?;
    let now_ref=optional(input,"now_ref")?;
    if let Some(reference)=&now_ref {super::placement::read_now(scope,reference)?;}
    let day_ref=optional(input,"day_ref")?;
    if let Some(reference)=&day_ref {super::temporal::day_read(scope,&json!({"day_ref":reference}))?;}
    let summary=match input.get("summary") {None|Some(Value::Null)=>None,Some(Value::String(s)) if s.len()<=8192=>Some(s.clone()),_=>return Err(invalid("summary must be bounded text"))};
    let evidence_refs: Vec<String>=serde_json::from_value(input.get("evidence_refs").cloned().unwrap_or(json!([])))?;
    if evidence_refs.len()>128 || evidence_refs.iter().any(|r|r.trim().is_empty()||r.len()>4096) {return Err(invalid("evidence refs exceed bounded receiving limits"));}
    let artifacts=retain_artifacts(scope,input,principal)?;
    if input.get("occurred_at_unix_seconds").is_some_and(|v|!v.is_null()&&v.as_u64().is_none()) {return Err(invalid("occurrence time must be an integer or absent"));}
    let stale=source.revision.revision!=expected;
    let record=Received {schema:"central.received-contribution/v1".into(),return_ref:reference,scope_ref:scope.world_ref.clone(),sequence:next_sequence(scope)?,request_digest:digest,
        source_ref:source.source.source_ref,document_id:text(&document,"document_id")?.into(),proposed_source_revision:expected.into(),proposal,
        author:principal.into(),authority_ref:principal.authority_ref.clone(),authority_revision:principal.authority_revision.clone(),
        occurred_at_unix_seconds:input.get("occurred_at_unix_seconds").and_then(Value::as_u64),received_at_unix_seconds:now,now_ref,day_ref,
        task_ref:optional(input,"task_ref")?,run_ref:optional(input,"run_ref")?,session_ref:optional(input,"session_ref")?,
        status:if stale {"needs-review"} else {"pending"}.into(),stale_at_arrival:stale,review:None,inclusion_request:None,applied_source_revision:None,last_error:None,acknowledgement:None,summary,evidence_refs,artifacts};
    let revision=write(scope,&record)?;
    Ok(response(&record,&revision))
}
/// Evidence is retained from the actual scoped SourceRef at the supplied
/// revision. It does not become included Day prose or adopted human ground.
fn retain_artifacts(scope: &Scope,input: &Value,principal: &Principal) -> io::Result<Vec<Value>> {
    let items=match input.get("artifacts") {None=>return Ok(vec![]),Some(v)=>v.as_array().ok_or_else(||invalid("artifacts must be an array"))?};
    if items.len()>16 {return Err(invalid("at most 16 explicit source artifacts per Return"));}
    let mut retained=Vec::new();let mut total=0;
    for item in items {
        let reading=scope.read(text(item,"source_ref")?)?;
        if reading.revision.revision!=text(item,"expected_revision")? {return Err(conflict("draft artifact changed before receiving"));}
        total+=reading.content.len();
        if total>512*1024 {return Err(invalid("retained draft evidence exceeds bounded receiving size"));}
        retained.push(json!({"source":reading.source,"revision":reading.revision,"content":reading.content,
            "content_sha256":source::key(&reading.content),"submitted_by":principal.principal_ref,
            "declared_original_producer_ref":optional(item,"producer_ref")?,
            "proposed_target_ref":optional(item,"proposed_target_ref")?,
            "standing":"retained-source-evidence-not-human-adoption"}));
    }
    Ok(retained)
}
fn checked(scope: &Scope,input: &Value) -> io::Result<Received> {
    let (record,revision)=read_record(scope,text(input,"return_ref")?)?;
    if revision!=text(input,"expected_return_revision")? {return Err(conflict("receiving record changed since review/selection"));}
    require_disclosure(scope,&record)?;
    Ok(record)
}
fn review(scope: &Scope,input: &Value,principal: &Principal,now: u64) -> io::Result<Value> {
    principal.require_human()?;
    let mut record=checked(scope,input)?;
    let disposition=text(input,"disposition")?;
    if disposition=="acknowledged" {
        record.acknowledgement=Some(json!({"principal_ref":principal.principal_ref,"recorded_at_unix_seconds":now,"authority_ref":principal.authority_ref,"authority_revision":principal.authority_revision}));
        let revision=write(scope,&record)?;return Ok(response(&record,&revision));
    }
    if disposition=="pending" {
        if !matches!(record.status.as_str(),"pending"|"needs-review"|"accepted"|"rejected") {return Err(conflict("pending cannot undo applied or uncertain effects"));}
        record.status="pending".into();record.review=None;
        let revision=write(scope,&record)?;return Ok(response(&record,&revision));
    }
    if !matches!(record.status.as_str(),"pending"|"needs-review"|"accepted"|"rejected") {return Err(conflict("Return has applied or uncertain effects; review cannot reset them"));}
    if !matches!(disposition,"accepted"|"rejected") {return Err(invalid("review disposition is accepted, rejected, pending or acknowledged; inclusion is separate"));}
    let source_revision=if disposition=="accepted" {
        let current=scope.read(&record.source_ref)?;
        if current.revision.revision!=text(input,"expected_source_revision")? {return Err(conflict("reviewed target source changed"));}
        current.revision.revision
    } else {record.proposed_source_revision.clone()};
    record.review=Some(Review {reviewer_ref:principal.principal_ref.clone(),authority_ref:principal.authority_ref.clone(),authority_revision:principal.authority_revision.clone(),disposition:disposition.into(),source_revision,reviewed_at_unix_seconds:now});
    record.status=disposition.into();
    let revision=write(scope,&record)?;
    Ok(response(&record,&revision))
}
fn include(scope: &Scope,input: &Value,principal: &Principal,now: u64,recover: bool) -> io::Result<Value> {
    principal.require_human()?;
    let mut record=checked(scope,input)?;
    if record.status=="included" {return Ok(response(&record,text(input,"expected_return_revision")?));}
    let review=record.review.as_ref().ok_or_else(||denied("unreviewed Return cannot be included"))?;
    if review.disposition!="accepted" || review.reviewer_ref!=principal.principal_ref {return Err(denied("inclusion requires the authenticated accepting reviewer or a new explicit review"));}
    let request=if recover {
        if !matches!(record.status.as_str(),"including"|"uncertain") {return Err(conflict("Return has no interrupted inclusion to recover"));}
        record.inclusion_request.clone().ok_or_else(||invalid("recorded inclusion intent missing"))?
    } else {
        if record.status!="accepted" {return Err(denied("only an explicitly accepted Return may begin inclusion"));}
        let current=scope.read(&record.source_ref)?;
        if current.revision.revision!=review.source_revision || current.revision.revision!=text(input,"expected_source_revision")? {return Err(conflict("source changed since explicit acceptance; re-review exact current basis"));}
        let mut request=record.proposal.clone();
        request["source_ref"]=json!(record.source_ref);
        request["document_id"]=json!(record.document_id);
        request["expected_revision"]=json!(review.source_revision);
        request["request_id"]=json!(format!("include:{}",record.return_ref));
        // Inclusion retains the original receipt/occurrence, not the later
        // review clock. None remains absent rather than a fabricated event.
        request["occurred_at_unix_seconds"]=json!(record.occurred_at_unix_seconds);
        request["received_at_unix_seconds"]=json!(record.received_at_unix_seconds);
        request
    };
    record.inclusion_request=Some(request.clone());
    record.status="including".into();record.last_error=None;
    write(scope,&record)?;
    let result=documents::mutate_reviewed(scope,&request,&record.author,principal,now);
    match result {
        Ok(result)=>{
            let applied=result["operation_receipt"]["revision"].as_str().or_else(||result["replayed_operation"]["applied_revision"].as_str())
                .ok_or_else(||invalid("native document did not return an actual applied revision"))?;
            record.status="included".into();record.applied_source_revision=Some(applied.into());record.last_error=None;
            let revision=write(scope,&record)?;
            let mut response=response(&record,&revision);
            response["document_result"]=result;
            Ok(response)
        }
        Err(error)=>{
            record.status="uncertain".into();record.last_error=Some(error.to_string());
            write(scope,&record)?;
            Err(io::Error::new(error.kind(),format!("inclusion not confirmed; inspect/recover {}: {error}",record.return_ref)))
        }
    }
}
fn list(scope: &Scope,input: &Value) -> io::Result<Value> {
    let after=input.get("after").and_then(Value::as_u64).unwrap_or(0);
    let limit=input.get("limit").and_then(Value::as_u64).unwrap_or(50);
    if limit==0 || limit>200 {return Err(invalid("receiving page limit must be 1..200"));}
    let entries=match fs::read_dir(scope.root.join(AREA)) {
        Ok(entries)=>entries,
        Err(error) if error.kind()==io::ErrorKind::NotFound=>return Ok(json!({"schema":"central.receiving-page/v1","scope_ref":scope.world_ref,"returns":[],"more":false,"next_after":null})),
        Err(error)=>return Err(error),
    };
    let mut records=Vec::new();
    let mut withheld=0u64;
    for entry in entries {
        let entry=entry?;
        let name=entry.file_name().to_string_lossy().into_owned();
        if name=="cursor.json" || !name.ends_with(".json") {continue;}
        let raw=crate::source_safety::read(&scope.root,&format!("{AREA}/{name}"))?;
        let record: Received=serde_json::from_str(&raw)?;
        if record.schema!="central.received-contribution/v1" || record.scope_ref!=scope.world_ref {return Err(invalid("unexpected record in receiving owner store"));}
        if record.sequence<=after {continue;}
        if require_disclosure(scope,&record).is_err() {withheld+=1;continue;}
        records.push(json!({"return_ref":record.return_ref,"revision":source::revision(&raw),"sequence":record.sequence,"status":record.status,
            "source_ref":record.source_ref,"document_id":record.document_id,"author":record.author,"summary":record.summary,"acknowledgement":record.acknowledgement,
            "occurred_at_unix_seconds":record.occurred_at_unix_seconds,"received_at_unix_seconds":record.received_at_unix_seconds,
            "now_ref":record.now_ref,"day_ref":record.day_ref,"task_ref":record.task_ref,"run_ref":record.run_ref,"session_ref":record.session_ref}));
    }
    records.sort_by_key(|record|record["sequence"].as_u64().unwrap_or(0));
    let more=records.len()>limit as usize;records.truncate(limit as usize);
    let next=if more {records.last().map(|record|record["sequence"].clone())} else {None};
    Ok(json!({"schema":"central.receiving-page/v1","scope_ref":scope.world_ref,"returns":records,"more":more,"next_after":next,"withheld_unavailable_sources":withheld,"proposal_bodies_in_page":false}))
}
/// Called before the generic source lock. This preserves the legacy owner's
/// return-lock → source-lock ordering and cannot deadlock against v1 acceptance.
pub(crate) fn dispatch(scope: &Scope,operation: &str,input: &Value,token: Option<&str>,now: u64) -> io::Result<Value> {
    if operation=="receiving_list" && input.get("projects").is_some() {return aperture::read(scope,input,token,now);}
    if operation=="receiving_list" && input.get("cursor").is_some() {return Err(invalid("root aperture cursor requires its explicit projects selection"));}
    let _receiving=crate::source_safety::lock(&scope.root,"source-return.lock")?;
    let _sources=source::lock(scope)?;
    match operation {
        "receiving_list"=>return list(scope,input),
        "receiving_read"=>{
            let (record,revision)=read_record(scope,text(input,"return_ref")?)?;
            require_disclosure(scope,&record)?;
            return Ok(response(&record,&revision));
        }
        _=>{},
    }
    let action=match operation {
        "receiving_submit"=>"central.receiving.submit",
        "receiving_review"=>"central.receiving.review",
        "receiving_include"=>"central.receiving.include",
        "receiving_recover"=>"central.receiving.recover",
        _=>return Err(invalid("unknown native receiving operation")),
    };
    let principal=authority::authenticate(scope,token,action,input.get("expected_authority_revision").and_then(Value::as_str),now)?;
    match operation {
        "receiving_submit"=>submit(scope,input,&principal,now),
        "receiving_review"=>review(scope,input,&principal,now),
        "receiving_include"=>include(scope,input,&principal,now,false),
        _=>include(scope,input,&principal,now,true),
    }
}
