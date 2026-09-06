//! Durable returned work remains a proposal until an explicit owner action.
//! Declared acceptance fields never confer human source authority.
use crate::{
    action::*,
    projectcentral_flow::content_revision_bytes,
    result::{ActionResult, ResultStatus},
    root::resolve_central_root,
    world_source::{enforce_write_authority, read_world_source, write_world_source},
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs, io,
    path::{Path, PathBuf},
};
#[derive(Debug, Clone, Serialize, Deserialize)]
struct SourceReturn {
    schema: String,
    return_ref: String,
    source_ref: String,
    basis_revision: String,
    basis_content: String,
    proposed_content: String,
    reason: String,
    evidence_refs: Vec<String>,
    agent_session_ref: String,
    status: String,
    accepted_by_ref: Option<String>,
    result_revision: Option<String>,
}
fn invalid(m: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, m)
}
fn text<'a>(i: &'a Value, k: &str) -> io::Result<&'a str> {
    i.get(k)
        .and_then(Value::as_str)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| invalid(&format!("{k} is required")))
}
fn directory(project: &Path) -> io::Result<PathBuf> {
    let dir = project.join(".central/source-returns");
    match fs::create_dir(&dir) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(e),
    };
    crate::projectcentral_flow::reject_symlink_components(
        project,
        Path::new(".central/source-returns"),
    )?;
    Ok(dir)
}
fn prefix(project: &Path) -> io::Result<String> {
    Ok(format!(
        "central:return:project:{}:",
        crate::projectcentral::read_project_manifest(project)?.project_id
    ))
}
fn path(project: &Path, dir: &Path, reference: &str) -> io::Result<PathBuf> {
    let prefix = prefix(project)?;
    let id = reference
        .strip_prefix(&prefix)
        .filter(|id| !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit() || b == b'-'))
        .ok_or_else(|| invalid("ReturnRef does not belong to this Project"))?;
    Ok(dir.join(format!("return-{id}.json")))
}
fn load(project: &Path, dir: &Path, reference: &str) -> io::Result<SourceReturn> {
    use std::io::Read;
    let p = path(project, dir, reference)?;
    let rel = p
        .strip_prefix(project)
        .map_err(io::Error::other)?
        .to_str()
        .ok_or_else(|| invalid("Invalid return path"))?;
    let r: SourceReturn = serde_json::from_reader(
        crate::file_mutation::open_native_file(project, rel)?
            .take((crate::source_safety::MAX_SOURCE * 12 + 65536) as u64),
    )?;
    if r.schema != "central.source-return/v1" || r.return_ref != reference {
        return Err(invalid("Return identity mismatch"));
    }
    Ok(r)
}
fn save(project: &Path, dir: &Path, r: &SourceReturn) -> io::Result<()> {
    crate::file_mutation::atomic_record(
        &path(project, dir, &r.return_ref)?,
        &serde_json::to_vec(r)?,
    )
}
fn reading(project: &Path, r: &SourceReturn) -> io::Result<Value> {
    let current = read_world_source(project, &r.source_ref)?;
    let refusal=enforce_write_authority(&current.source,"agent",Some(&r.agent_session_ref)).err().map(|e|format!("{e}; this Action context has no attested human principal and acceptance strings do not grant it"));
    Ok(
        json!({"schema":"central.source-return-reading/v1","proposal":r,"current":current,"basis_current":current.revision.revision==r.basis_revision,"acceptance":{"available":r.status=="pending"&&refusal.is_none(),"reason":refusal},"authored_source_mutated":false,"automatic_agent_or_model_invocation":false}),
    )
}
fn run(project: &Path, op: &str, input: &Value) -> io::Result<Value> {
    let _lock = crate::source_safety::lock(project, "source-return.lock")?;
    let dir = directory(project)?;
    if op == "return" {
        let source = text(input, "source_ref")?;
        let basis = text(input, "expected_revision")?;
        let content = input
            .get("proposed_content")
            .and_then(Value::as_str)
            .ok_or_else(|| invalid("proposed_content is required"))?;
        if content.len() > crate::source_safety::MAX_SOURCE || content.contains('\0') {
            return Err(invalid(
                "Return content must be bounded UTF-8 text without NUL",
            ));
        }
        let current = read_world_source(project, source)?;
        if current.revision.revision != basis {
            return Ok(
                json!({"outcome":"conflict","current":current,"authored_source_mutated":false}),
            );
        }
        let reason = text(input, "reason")?;
        let session = text(input, "agent_session_ref")?;
        if reason.len() > 16384 || session.len() > 4096 {
            return Err(invalid("Return provenance exceeds bounded size"));
        }
        let evidence: Vec<String> =
            serde_json::from_value(input.get("evidence_refs").cloned().unwrap_or(json!([])))?;
        if evidence.len() > 256 || evidence.iter().any(|r| r.len() > 4096) {
            return Err(invalid("Return evidence refs exceed bounded size"));
        }
        let id = format!(
            "{}-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(io::Error::other)?
                .as_nanos(),
            std::process::id()
        );
        let r = SourceReturn {
            schema: "central.source-return/v1".into(),
            return_ref: format!("{}{id}", prefix(project)?),
            source_ref: source.into(),
            basis_revision: basis.into(),
            basis_content: current.content,
            proposed_content: content.into(),
            reason: reason.into(),
            evidence_refs: evidence,
            agent_session_ref: session.into(),
            status: "pending".into(),
            accepted_by_ref: None,
            result_revision: None,
        };
        save(project, &dir, &r)?;
        return reading(project, &r);
    }
    if op == "returns" {
        let limit = input
            .get("limit")
            .map(|v| {
                v.as_u64()
                    .ok_or_else(|| invalid("limit must be an integer"))
            })
            .transpose()?
            .unwrap_or(20);
        if limit == 0 || limit > 100 {
            return Err(invalid("limit must be 1..100"));
        }
        let before = input.get("before").and_then(Value::as_str);
        let mut selected = BTreeMap::new();
        for e in fs::read_dir(&dir)? {
            let e = e?;
            let name = e.file_name().to_string_lossy().into_owned();
            if name.starts_with("return-")
                && name.ends_with(".json")
                && before.is_none_or(|b| name.as_str() < b)
            {
                selected.insert(name, e.path());
                if selected.len() > limit as usize + 1 {
                    selected.pop_first();
                }
            }
        }
        let more = selected.len() > limit as usize;
        let mut entries = Vec::new();
        let mut next = None;
        for (name, _) in selected.into_iter().rev().take(limit as usize) {
            let id = name
                .strip_prefix("return-")
                .unwrap()
                .strip_suffix(".json")
                .unwrap();
            let r = load(project, &dir, &format!("{}{id}", prefix(project)?))?;
            let current = read_world_source(project, &r.source_ref)?;
            entries.push(json!({"return_ref":r.return_ref,"source_ref":r.source_ref,"basis_revision":r.basis_revision,"status":r.status,"agent_session_ref":r.agent_session_ref,"current_revision":current.revision.revision}));
            next = Some(name);
        }
        return Ok(
            json!({"schema":"central.source-returns/v1","entries":entries,"more":more,"next_before":if more{next}else{None},"authored_source_mutated":false}),
        );
    }
    let reference = text(input, "return_ref")?;
    let mut r = load(project, &dir, reference)?;
    if r.status == "applying" {
        let current = read_world_source(project, &r.source_ref)?;
        let target = content_revision_bytes(r.proposed_content.as_bytes());
        if current.revision.revision == target {
            r.status = "accepted".into();
            r.result_revision = Some(target);
            save(project, &dir, &r)?;
        } else if current.revision.revision == r.basis_revision {
            r.status = "pending".into();
            save(project, &dir, &r)?;
        } else {
            return Err(io::Error::other("Interrupted return application is unresolved: current source matches neither basis nor proposed revision; do not automatically resend"));
        }
    }
    if op == "return_read" {
        return reading(project, &r);
    }
    if r.status != "pending" {
        return Err(invalid("Return is not pending"));
    }
    if op == "return_reject" {
        r.status = "rejected".into();
        save(project, &dir, &r)?;
        return reading(project, &r);
    }
    if text(input, "acceptance")? != "human-accepted" {
        return Err(invalid("Explicit acceptance is required"));
    }
    let current = read_world_source(project, &r.source_ref)?;
    let expected = text(input, "expected_revision")?;
    if expected != r.basis_revision || current.revision.revision != r.basis_revision {
        return Ok(
            json!({"outcome":"conflict","proposal":r,"current":current,"authored_source_mutated":false}),
        );
    }
    // Source authority is derived from native binding. Never convert an Agent
    // return into a human actor merely because a caller supplied acceptance text.
    enforce_write_authority(&current.source,"agent",Some(&r.agent_session_ref)).map_err(|e|io::Error::new(io::ErrorKind::PermissionDenied,format!("{e}; no attested human acceptance principal is present in native ActionExecutionContext")))?;
    let accepted = text(input, "accepted_by_ref")?;
    if accepted.len() > 4096 {
        return Err(invalid("accepted_by_ref exceeds bounded size"));
    }
    r.accepted_by_ref = Some(accepted.into());
    r.status = "applying".into();
    save(project, &dir, &r)?;
    let flow = crate::projectcentral_flow::registered_flow_records(project)?
        .into_iter()
        .find(|f| f.source_ref == r.source_ref);
    let applied = if let Some(flow) = flow {
        crate::projectcentral_flow::write_flow(
            project,
            &flow.flow_ref,
            &r.basis_revision,
            &r.proposed_content,
            &r.agent_session_ref,
            "agent",
            Some(r.agent_session_ref.clone()),
        )
        .map(|receipt| {
            (
                receipt.current_revision.clone(),
                json!({"owner_operation":"projectcentral.flow.write","flow":receipt}),
            )
        })
    } else {
        write_world_source(
            project,
            &r.source_ref,
            &r.basis_revision,
            &r.proposed_content,
            &r.agent_session_ref,
            "agent",
            Some(r.agent_session_ref.clone()),
        )
        .map(|receipt| {
            (
                receipt.revision.revision.clone(),
                json!({"owner_operation":"projectcentral.source.write","source":receipt}),
            )
        })
    };
    match applied {
        Ok((revision, receipt)) => {
            r.status = "accepted".into();
            r.result_revision = Some(revision);
            save(project, &dir, &r).map_err(|e| {
                io::Error::other(format!(
                    "Source was applied but return receipt failed: {e}; do not resend"
                ))
            })?;
            Ok(
                json!({"outcome":"accepted","proposal":r,"receipt":receipt,"authored_source_mutated":true}),
            )
        }
        Err(e) => {
            let current = read_world_source(project, &r.source_ref)?;
            if current.revision.revision == r.basis_revision {
                r.status = "pending".into();
                save(project, &dir, &r)?;
            }
            Err(e)
        }
    }
}
fn action(op: &str, input: &Value, context: &ActionExecutionContext<'_>) -> ActionResult {
    let id = format!("projectcentral.source.{op}");
    let result = (|| {
        let root = resolve_central_root(context.root_options)
            .map_err(io::Error::other)?
            .path;
        let project = text(input, "project")?;
        let rel = crate::projectcentral_flow::relative_member(project)?;
        crate::projectcentral_flow::reject_symlink_components(
            &root,
            &Path::new("Work").join(&rel),
        )?;
        let project = root.join("Work").join(rel);
        let manifest = crate::projectcentral::read_project_manifest(&project)?;
        if !manifest.validate().valid {
            return Err(invalid("Invalid Project identity"));
        }
        run(&project, op, input)
    })();
    match result {
        Ok(v) => ActionResult::success(&id, v),
        Err(e) => ActionResult::failure(
            Some(&id),
            match e.kind() {
                io::ErrorKind::PermissionDenied => ResultStatus::UnavailableCapability,
                io::ErrorKind::InvalidInput | io::ErrorKind::NotFound => ResultStatus::InvalidInput,
                io::ErrorKind::AlreadyExists | io::ErrorKind::InvalidData => {
                    ResultStatus::VerificationFailure
                }
                _ => ResultStatus::InternalFailure,
            },
            e.to_string(),
            Some(
                json!({"source_mutation":"unconfirmed","outcome":if e.kind()==io::ErrorKind::PermissionDenied{"refused"}else{"error"}}),
            ),
        ),
    }
}
pub(crate) fn register(registry: &mut ActionRegistry) {
    type Handler = fn(&ActionRegistry, &Value, &ActionExecutionContext<'_>) -> ActionResult;
    for (op, handler) in [
        ("return", (|_, i, c| action("return", i, c)) as Handler),
        ("returns", (|_, i, c| action("returns", i, c)) as Handler),
        (
            "return_read",
            (|_, i, c| action("return_read", i, c)) as Handler,
        ),
        (
            "return_accept",
            (|_, i, c| action("return_accept", i, c)) as Handler,
        ),
        (
            "return_reject",
            (|_, i, c| action("return_reject", i, c)) as Handler,
        ),
    ] {
        let fields = match op {
            "return" => vec![
                ("project", true),
                ("source_ref", true),
                ("expected_revision", true),
                ("proposed_content", true),
                ("reason", true),
                ("evidence_refs", false),
                ("agent_session_ref", true),
            ],
            "returns" => vec![("project", true), ("limit", false), ("before", false)],
            "return_accept" => vec![
                ("project", true),
                ("return_ref", true),
                ("expected_revision", true),
                ("acceptance", true),
                ("accepted_by_ref", true),
            ],
            _ => vec![("project", true), ("return_ref", true)],
        };
        registry.register(ActionDescriptor{id:format!("projectcentral.source.{op}"),title:format!("Source {op}"),description:"Native returned-work proposal, exact source basis and authority-preserving acceptance. Acceptance text never grants human source authority.".into(),inputs:fields.into_iter().map(|(name,required)|ActionInputDefinition{name:name.into(),input_type:match name{"evidence_refs"=>"array","limit"=>"integer",_=>"string"}.into(),required,choices:None,selection:None}).collect(),output:ActionOutputDefinition{output_type:"central-source-return".into()},mutation_class:MutationClass::LocallyMutating,preview_supported:false,required_ports:vec![],availability:ActionAvailability{available:true,reason:None}},handler).expect("Unique source return Action");
    }
}
