//! Durable returned work remains a proposal until an explicit owner action.
//! Declared acceptance fields never confer human source authority.
use crate::{
    action::*,
    continuous_work::{authority, source::Scope},
    result::{ActionResult, ResultStatus},
    root::resolve_central_root,
    source_horizon::SourceBinding,
    source_safety::content_revision_bytes,
    world_source::{
        enforce_write_authority, read_world_source, write_commissioned_return_source,
        write_world_source,
    },
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs, io,
    os::unix::fs::MetadataExt,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    basis_source: Option<SourceBinding>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    maintenance_authorization: Option<MaintenanceAuthorization>,
}

/// Historical operation attribution, never a deserializable native Principal
/// and never permission to resend a retained proposal.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct MaintenanceAuthorization {
    acceptance: String,
    principal_ref: String,
    actor_kind: String,
    scope_ref: String,
    authority_ref: String,
    authority_revision: String,
    action: String,
    expires_at_unix_seconds: u64,
}
impl MaintenanceAuthorization {
    fn observed(principal: &authority::Principal) -> Self {
        Self {
            acceptance: "commissioned-maintenance".into(),
            principal_ref: principal.principal_ref.clone(),
            actor_kind: principal.actor_kind.clone(),
            scope_ref: principal.scope_ref.clone(),
            authority_ref: principal.authority_ref.clone(),
            authority_revision: principal.authority_revision.clone(),
            action: "projectcentral.source.return_accept".into(),
            expires_at_unix_seconds: principal.expires_at_unix_seconds,
        }
    }
}
fn native_now() -> io::Result<u64> {
    Ok(std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(io::Error::other)?
        .as_secs())
}
fn require_current_scope(
    scope: &Scope,
    requested_central: &Path,
    root: &fs::File,
    project: &fs::File,
) -> io::Result<()> {
    let current = Scope::resolve(requested_central, scope.project.as_deref())?;
    let current_root = crate::file_mutation::directory(&current.central_root, Path::new(""))?;
    let current_project = crate::file_mutation::directory(&current.root, Path::new(""))?;
    let held_root = root.metadata()?;
    let held_project = project.metadata()?;
    let root_metadata = current_root.metadata()?;
    let project_metadata = current_project.metadata()?;
    if current.world_ref != scope.world_ref
        || current.central_root != scope.central_root
        || current.root != scope.root
        || (held_root.dev(), held_root.ino()) != (root_metadata.dev(), root_metadata.ino())
        || (held_project.dev(), held_project.ino())
            != (project_metadata.dev(), project_metadata.ino())
    {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "Source maintenance native root or Project affiliation changed",
        ));
    }
    Ok(())
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
    crate::source_safety::reject_symlink_components(project, Path::new(".central/source-returns"))?;
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
    if !matches!(
        r.schema.as_str(),
        "central.source-return/v1" | "central.source-return/v2"
    ) || r.return_ref != reference
        || (r.schema == "central.source-return/v2"
            && (r
                .basis_source
                .as_ref()
                .is_none_or(|binding| binding.source_ref != r.source_ref)
                || (matches!(r.status.as_str(), "applying" | "accepted")
                    && r.maintenance_authorization.is_none())))
    {
        return Err(invalid("Return identity mismatch"));
    }
    Ok(r)
}
fn save(
    project: &Path,
    dir: &Path,
    r: &SourceReturn,
    disposition: crate::file_mutation::RecordDisposition,
) -> io::Result<()> {
    crate::file_mutation::atomic_record(
        project,
        path(project, dir, &r.return_ref)?
            .strip_prefix(project)
            .map_err(io::Error::other)?,
        &serde_json::to_vec(r)?,
        disposition,
    )
}
fn reading(project: &Path, r: &SourceReturn) -> io::Result<Value> {
    let current = read_world_source(project, &r.source_ref)?;
    let refusal = if r.schema == "central.source-return/v2" {
        Some("Commissioned maintenance requires current authenticated exact Project/action authority; this proposal is not permission".to_owned())
    } else {
        enforce_write_authority(&current.source,"agent",Some(&r.agent_session_ref)).err().map(|e|format!("{e}; this Action context has no attested human principal and acceptance strings do not grant it"))
    };
    Ok(
        json!({"schema":if r.schema=="central.source-return/v2"{"central.source-return-reading/v2"}else{"central.source-return-reading/v1"},"proposal":r,"current":current,"basis_current":current.revision.revision==r.basis_revision,"acceptance":{"available":r.status=="pending"&&refusal.is_none(),"reason":refusal},"authored_source_mutated":false,"automatic_agent_or_model_invocation":false}),
    )
}
fn run(project: &Path, op: &str, input: &Value, central: &Path) -> io::Result<Value> {
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
        let maintenance_proposal =
            input.get("acceptance").and_then(Value::as_str) == Some("commissioned-maintenance");
        let r = SourceReturn {
            schema: if maintenance_proposal {
                "central.source-return/v2"
            } else {
                "central.source-return/v1"
            }
            .into(),
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
            basis_source: if maintenance_proposal {
                Some(current.source)
            } else {
                None
            },
            maintenance_authorization: None,
        };
        save(
            project,
            &dir,
            &r,
            crate::file_mutation::RecordDisposition::CreateNew,
        )?;
        return reading(project, &r).map_err(|error| {
            crate::file_mutation::record_owner_error_with_ref(
                error,
                "source_return.after_proposal_publication",
                &r.return_ref,
                Some(&r.source_ref),
                Some(&r.basis_revision),
                "source_not_changed_proposal_published",
            )
        });
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
    let mut recovered = false;
    if r.status == "applying" {
        let current = read_world_source(project, &r.source_ref)?;
        if r.maintenance_authorization.is_some() && r.basis_source.as_ref() != Some(&current.source)
        {
            return Err(io::Error::other(
                "Interrupted maintenance Source binding changed; retain uncertainty without resend",
            ));
        }
        let target = content_revision_bytes(r.proposed_content.as_bytes());
        if current.revision.revision == target
            && (r.maintenance_authorization.is_none() || current.content == r.proposed_content)
        {
            r.status = "accepted".into();
            r.result_revision = Some(target);
            save(
                project,
                &dir,
                &r,
                crate::file_mutation::RecordDisposition::ReplaceOrCreate,
            )
            .map_err(|error| {
                crate::file_mutation::record_owner_error_with_ref(
                    error,
                    "source_return.recovery_target_observed",
                    &r.return_ref,
                    Some(&r.source_ref),
                    r.result_revision.as_deref(),
                    "target_observed",
                )
            })?;
        } else if current.revision.revision == r.basis_revision
            && (r.maintenance_authorization.is_none() || current.content == r.basis_content)
        {
            r.status = "pending".into();
            save(
                project,
                &dir,
                &r,
                crate::file_mutation::RecordDisposition::ReplaceOrCreate,
            )
            .map_err(|error| {
                crate::file_mutation::record_owner_error_with_ref(
                    error,
                    "source_return.recovery_basis_observed",
                    &r.return_ref,
                    Some(&r.source_ref),
                    Some(&r.basis_revision),
                    "basis_observed_unchanged",
                )
            })?;
        } else {
            return Err(io::Error::other("Interrupted return application is unresolved: current source matches neither basis nor proposed revision; do not automatically resend"));
        }
        recovered = true;
    }
    if op == "return_read" {
        let result: io::Result<Value> = (|| {
            let mut value = reading(project, &r)?;
            if input.get("acceptance").and_then(Value::as_str) == Some("commissioned-maintenance") {
                let scope = Scope::resolve(central, Some(text(input, "project")?))?;
                let root = Scope::resolve(&scope.central_root, None)?;
                let (source, _, revision) =
                    authority::recognised_source(&root, authority::AUTHORITY_ROLE)?;
                value["commissioned_maintenance"] = json!({
                    "authority_ref":source.source.source_ref,"authority_revision":revision,
                    "action":"projectcentral.source.return_accept","scope_ref":scope.world_ref,
                    "requires_current_authenticated_principal":true,"available":false,
                    "note":"Source basis is not permission or physical human review"});
            }
            Ok(value)
        })();
        return result.map_err(|error| {
            if recovered {
                crate::file_mutation::record_owner_error_with_ref(
                    error,
                    "source_return.after_recovery_recording",
                    &r.return_ref,
                    Some(&r.source_ref),
                    r.result_revision.as_deref(),
                    "recovery_record_acknowledged",
                )
            } else {
                error
            }
        });
    }
    if r.status != "pending" {
        return Err(invalid("Return is not pending"));
    }
    if op == "return_reject" {
        r.status = "rejected".into();
        save(
            project,
            &dir,
            &r,
            crate::file_mutation::RecordDisposition::ReplaceOrCreate,
        )?;
        return reading(project, &r).map_err(|error| {
            crate::file_mutation::record_owner_error_with_ref(
                error,
                "source_return.after_rejection_recording",
                &r.return_ref,
                Some(&r.source_ref),
                Some(&r.basis_revision),
                "source_not_changed_rejection_record_acknowledged",
            )
        });
    }
    let acceptance = text(input, "acceptance")?;
    let maintenance = acceptance == "commissioned-maintenance";
    if !maintenance && acceptance != "human-accepted" {
        return Err(invalid("Explicit acceptance is required"));
    }
    if !maintenance
        && (r.schema == "central.source-return/v2" || r.maintenance_authorization.is_some())
    {
        return Err(invalid(
            "A retained maintenance attempt cannot become declared collaborative acceptance; make a fresh exact proposal",
        ));
    }
    let scope = if maintenance {
        if [
            "principal",
            "token",
            "authorizing_principal",
            "actor",
            "actor_kind",
            "agent_session_ref",
            "source_ref",
            "proposed_content",
        ]
        .iter()
        .any(|field| input.get(field).is_some())
        {
            return Err(invalid("Source acceptance cannot supply principal or replace immutable proposal attribution/content"));
        }
        let scope = Scope::resolve(central, Some(text(input, "project")?))?;
        if scope.root != fs::canonicalize(project)? {
            return Err(invalid(
                "Source Return and authenticated Project do not share an owner",
            ));
        }
        Some(scope)
    } else {
        None
    };
    let held = scope
        .as_ref()
        .map(|scope| -> io::Result<_> {
            Ok((
                crate::file_mutation::directory(&scope.central_root, Path::new(""))?,
                crate::file_mutation::directory(&scope.root, Path::new(""))?,
            ))
        })
        .transpose()?;
    let guards = scope
        .as_ref()
        .map(crate::continuous_work::source::lock)
        .transpose()?;
    let principal = if let Some(scope) = &scope {
        let expected_authority = text(input, "expected_authority_revision")?;
        if expected_authority.trim().is_empty() || expected_authority.len() > 4096 {
            return Err(invalid(
                "expected_authority_revision requires bounded nonempty text",
            ));
        }
        let (root, project) = held.as_ref().expect("held maintenance owner");
        require_current_scope(scope, central, root, project)?;
        let token = std::env::var("CENTRAL_NATIVE_TOKEN").ok();
        let principal = authority::authenticate(
            scope,
            token.as_deref(),
            "projectcentral.source.return_accept",
            Some(expected_authority),
            native_now()?,
        )?;
        principal.require_human()?;
        Some(principal)
    } else {
        None
    };
    let current = read_world_source(project, &r.source_ref)?;
    let expected = text(input, "expected_revision")?;
    if expected != r.basis_revision || current.revision.revision != r.basis_revision {
        return Ok(
            json!({"outcome":"conflict","proposal":r,"current":current,"authored_source_mutated":false}),
        );
    }
    // Source authority is derived from native binding. Never convert an Agent
    // return into a human actor merely because a caller supplied acceptance text.
    if maintenance {
        if r.schema != "central.source-return/v2" {
            return Err(invalid(
                "Maintenance requires a fresh explicitly selected v2 Source Return",
            ));
        }
        let captured = r.basis_source.as_ref().ok_or_else(|| invalid(
            "Legacy Return lacks an owner-captured binding; make a fresh exact proposal for maintenance"))?;
        if captured != &current.source || current.content != r.basis_content {
            return Ok(
                json!({"outcome":"conflict","proposal":r,"current":current,"authored_source_mutated":false}),
            );
        }
        if crate::world_source::natively_owned(captured)
            || !captured
                .roles
                .iter()
                .any(|role| role == "project-human-source-aperture")
        {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "Maintenance cannot replace native owner state or an unsupported Source role",
            ));
        }
    } else {
        enforce_write_authority(&current.source,"agent",Some(&r.agent_session_ref)).map_err(|e|io::Error::new(io::ErrorKind::PermissionDenied,format!("{e}; no attested human acceptance principal is present in native ActionExecutionContext")))?;
    }
    let accepted = text(input, "accepted_by_ref")?;
    if accepted.len() > 4096 {
        return Err(invalid("accepted_by_ref exceeds bounded size"));
    }
    if let Some(principal) = &principal {
        if accepted != principal.principal_ref {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "accepted_by_ref differs from the authenticated authorizing principal",
            ));
        }
        r.maintenance_authorization = Some(MaintenanceAuthorization::observed(principal));
    }
    r.accepted_by_ref = Some(accepted.into());
    r.status = "applying".into();
    save(
        project,
        &dir,
        &r,
        crate::file_mutation::RecordDisposition::ReplaceOrCreate,
    )?;
    // The retired Flow registry played no authority here: every retained
    // source applies as the ordinary world source it always was.
    let applied = if let Some(principal) = &principal {
        let scope = scope.as_ref().expect("authenticated maintenance scope");
        let (root, project) = held.as_ref().expect("held maintenance owner");
        require_current_scope(scope, central, root, project).and_then(|()| {
            write_commissioned_return_source(
                scope,
                principal,
                r.basis_source.as_ref().expect("validated captured binding"),
                &r.basis_revision,
                &r.basis_content,
                &r.proposed_content,
                &r.agent_session_ref,
                guards.as_ref().expect("held maintenance mutation guards"),
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
    }
    .map(|receipt| {
        let revision = receipt.revision.revision.clone();
        let mut observed =
            json!({"owner_operation":"projectcentral.source.write","source":receipt});
        if let Some(authorization) = &r.maintenance_authorization {
            observed["maintenance_authorization"] = json!(authorization);
        }
        (revision, observed)
    });
    match applied {
        Ok((revision, receipt)) => {
            r.status = "accepted".into();
            r.result_revision = Some(revision);
            save(
                project,
                &dir,
                &r,
                crate::file_mutation::RecordDisposition::ReplaceOrCreate,
            )
            .map_err(|error| {
                crate::file_mutation::record_owner_error_with_ref(
                    error,
                    "source_return.accepted_source_recording",
                    &r.return_ref,
                    Some(&r.source_ref),
                    r.result_revision.as_deref(),
                    "acknowledged",
                )
            })?;
            Ok(
                json!({"outcome":"accepted","proposal":r,"receipt":receipt,"authored_source_mutated":true}),
            )
        }
        Err(primary) => {
            // A later read/recording failure cannot erase the actual native
            // source-write failure or imply that its effect was undone.
            let current = match read_world_source(project, &r.source_ref) {
                Ok(current) => current,
                Err(secondary) => {
                    return Err(crate::file_mutation::record_owner_errors(
                        primary,
                        secondary,
                        "source_return.failed_write_observation",
                        Some(&r.source_ref),
                        None,
                        "unconfirmed",
                    ))
                }
            };
            if current.revision.revision == r.basis_revision
                && (r.maintenance_authorization.is_none()
                    || (r.basis_source.as_ref() == Some(&current.source)
                        && current.content == r.basis_content))
            {
                r.status = "pending".into();
                if let Err(secondary) = save(
                    project,
                    &dir,
                    &r,
                    crate::file_mutation::RecordDisposition::ReplaceOrCreate,
                ) {
                    return Err(crate::file_mutation::record_owner_errors(
                        primary,
                        secondary,
                        "source_return.failed_write_recording",
                        Some(&r.source_ref),
                        Some(&r.basis_revision),
                        "basis_observed_unchanged",
                    ));
                }
            }
            Err(primary)
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
        let rel = crate::source_safety::relative_member(project)?;
        crate::source_safety::reject_symlink_components(&root, &Path::new("Work").join(&rel))?;
        let project = root.join("Work").join(rel);
        let manifest = crate::projectcentral::read_project_manifest(&project)?;
        if !manifest.validate().valid {
            return Err(invalid("Invalid Project identity"));
        }
        run(&project, op, input, &root)
    })();
    match result {
        Ok(v) => ActionResult::success(&id, v),
        Err(e) if crate::file_mutation::record_failure_result(&id, &e).is_some() => {
            crate::file_mutation::record_failure_result(&id, &e)
                .expect("matched native record failure")
        }
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
                ("acceptance", false),
            ],
            "returns" => vec![("project", true), ("limit", false), ("before", false)],
            "return_accept" => vec![
                ("project", true),
                ("return_ref", true),
                ("expected_revision", true),
                ("acceptance", true),
                ("accepted_by_ref", true),
                ("expected_authority_revision", false),
            ],
            "return_read" => vec![
                ("project", true),
                ("return_ref", true),
                ("acceptance", false),
            ],
            _ => vec![("project", true), ("return_ref", true)],
        };
        registry.register(ActionDescriptor{id:format!("projectcentral.source.{op}"),title:format!("Source {op}"),description:"Native returned-work proposal, exact source basis and authority-preserving acceptance. Acceptance text never grants human source authority.".into(),inputs:fields.into_iter().map(|(name,required)|ActionInputDefinition{name:name.into(),input_type:match name{"evidence_refs"=>"array","limit"=>"integer",_=>"string"}.into(),required,choices:None,selection:None}).collect(),output:ActionOutputDefinition{output_type:"central-source-return".into()},mutation_class:MutationClass::LocallyMutating,preview_supported:false,required_ports:vec![],availability:ActionAvailability{available:true,reason:None}},handler).expect("Unique source return Action");
    }
}
