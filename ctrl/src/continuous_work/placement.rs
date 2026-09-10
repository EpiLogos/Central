use super::source::{self, conflict, denied, encoded, invalid, key, text, Scope};
use crate::projectcentral_flow::{relative_member, reject_symlink_components};
use crate::source_horizon::SourceBinding;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs;
use std::io;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

pub const POLICY_SCHEMA: &str = "central.work-placement-policy/v1";
pub const POLICY_ROLE: &str = "work-placement-policy";
pub const NOW_SCHEMA: &str = "central.now-clearing/v1";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SourceBasis {
    pub source_ref: String,
    pub revision: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Grant {
    /// Relative to the declaring World. A Project may use '.' for its own
    /// ordinary repository tree, never for its protected ProjectCentral sources.
    pub path: String,
    pub class: String,
    #[serde(default)]
    pub reason: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlacementPolicy {
    pub schema: String,
    pub scope_ref: String,
    #[serde(default)]
    pub parent_policy: Option<SourceBasis>,
    #[serde(default)]
    pub authority_refs: Vec<SourceBasis>,
    pub writable: Vec<Grant>,
    #[serde(default)]
    pub protected: Vec<String>,
    pub enforcement: String,
    pub required_coverage: Vec<String>,
    pub lease_seconds: u64,
    #[serde(default)]
    pub expires_at_unix_seconds: Option<u64>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PathAnchor {
    pub path: PathBuf,
    pub existing_ancestor: PathBuf,
    pub device: u64,
    pub inode: u64,
    pub exists: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WritableDestination {
    pub path: PathBuf,
    pub class: String,
    pub anchor: PathAnchor,
}
#[derive(Debug, Clone, Serialize)]
pub struct PolicySource {
    pub source: SourceBinding,
    pub revision: String,
    pub authority_refs: Vec<SourceBasis>,
}
#[derive(Debug, Clone, Serialize)]
pub struct EffectivePolicy {
    pub schema: String,
    pub scope_ref: String,
    pub root_scope_ref: String,
    pub revision: String,
    pub sources: Vec<PolicySource>,
    pub writable_destinations: Vec<WritableDestination>,
    pub protected_paths: Vec<PathBuf>,
    pub enforcement: String,
    pub required_coverage: Vec<String>,
    pub issued_at_unix_seconds: u64,
    pub expires_at_unix_seconds: u64,
    pub native_enforcement: String,
    pub outside_writes_prevented: bool,
}

pub(crate) fn anchor(root: &Path, absolute: &Path) -> io::Result<PathAnchor> {
    let relative = absolute.strip_prefix(root).map_err(|_| denied("destination is outside Central"))?;
    reject_symlink_components(root, relative)?;
    let mut current = absolute;
    loop {
        match fs::symlink_metadata(current) {
            Ok(meta) => {
                if meta.file_type().is_symlink() { return Err(denied("symlink destination requires reviewed material resolution")); }
                if !meta.is_dir() && !meta.is_file() { return Err(denied("destination is not a regular file or directory")); }
                return Ok(PathAnchor {
                    path: absolute.into(), existing_ancestor: current.into(),
                    device: meta.dev(), inode: meta.ino(), exists: current == absolute,
                });
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                current = current.parent().ok_or_else(|| denied("destination has no existing Central ancestor"))?;
            }
            Err(e) => return Err(e),
        }
    }
}
fn absolute(scope: &Scope, raw: &str) -> io::Result<PathBuf> {
    if raw == "." && scope.project.is_some() { return Ok(scope.root.clone()); }
    Ok(scope.root.join(relative_member(raw)?))
}
fn recognised(source: &SourceBinding) -> bool {
    matches!(source.provenance.as_str(), "human-authored" | "human-adopted")
        && matches!(source.standing.as_str(), "authored-human-position" | "design-commitment" | "architecture-contract")
        && source.agent_retrieval_allowed
}
fn policy_at(scope: &Scope, now: u64, required: bool) -> io::Result<Option<(PlacementPolicy, PolicySource)>> {
    let (relations, _) = scope.relations()?;
    let candidates: Vec<_> = relations["relations"].as_array().ok_or_else(|| invalid("invalid relations"))?
        .iter().filter(|r| r["roles"].as_array().is_some_and(|roles| roles.iter().any(|role| role == POLICY_ROLE))).collect();
    if candidates.len() > 1 { return Err(invalid("multiple effective placement-policy relations require reconciliation")); }
    let Some(relation) = candidates.first() else {
        return if required { Err(denied("no recognised root placement policy; record exact policy adoption through Central source operations")) } else { Ok(None) };
    };
    let reading = scope.read(text(relation, "ref")?)?;
    if !recognised(&reading.source) || relation["recognition"].as_str().is_none_or(|s| s.trim().is_empty() || s == "owner-recorded-source-relation-not-human-recognition") {
        return Err(denied("placement-policy source is draft/unrecognised; no fallback or authority widening"));
    }
    let policy: PlacementPolicy = serde_json::from_str(&reading.content)?;
    if policy.schema != POLICY_SCHEMA || policy.scope_ref != scope.world_ref {
        return Err(invalid("placement-policy schema or scope mismatch"));
    }
    if policy.lease_seconds == 0 || policy.lease_seconds > 86400
        || policy.expires_at_unix_seconds.is_some_and(|expiry| now >= expiry) {
        return Err(denied("placement policy is expired or has an invalid bounded lease"));
    }
    if !matches!(policy.enforcement.as_str(), "native-actions" | "harness-interception" | "material-filesystem")
        || policy.required_coverage.is_empty() || policy.required_coverage.len() > 32 {
        return Err(invalid("placement policy requires a known enforcement level and explicit coverage"));
    }
    let mut authorities = vec![SourceBasis { source_ref: reading.source.source_ref.clone(), revision: reading.revision.revision.clone() }];
    for basis in &policy.authority_refs {
        let authority = scope.read(&basis.source_ref)?;
        if authority.revision.revision != basis.revision { return Err(conflict("placement authority source revision is stale")); }
        if !recognised(&authority.source) { return Err(denied("placement authority is not recognised human source")); }
        authorities.push(basis.clone());
    }
    Ok(Some((policy, PolicySource { source: reading.source, revision: reading.revision.revision, authority_refs: authorities })))
}
fn destinations(scope: &Scope, policy: &PlacementPolicy) -> io::Result<Vec<WritableDestination>> {
    if policy.writable.len() > 256 || policy.protected.len() > 256 { return Err(invalid("placement policy exceeds bounded destination count")); }
    policy.writable.iter().map(|grant| {
        if !matches!(grant.class.as_str(), "repository" | "worktree" | "build-output" | "declared-exception") {
            return Err(invalid("unknown destination class"));
        }
        if grant.class == "declared-exception" && grant.reason.as_ref().is_none_or(|s| s.trim().is_empty()) {
            return Err(invalid("a declared exception requires its reviewed reason in the policy source"));
        }
        let path = absolute(scope, &grant.path)?;
        let relative = path.strip_prefix(&scope.central_root).map_err(|_| denied("grant outside Central"))?;
        if relative.components().any(|part| matches!(part.as_os_str().to_str(), Some(".central" | ".git" | "Control" | "ProjectCentral"))) {
            return Err(denied("ordinary engineering grants cannot bypass native source or metadata ownership"));
        }
        if scope.project.is_none() {
            let parts: Vec<_> = relative.components().collect();
            if parts.len() < 2 || parts[0].as_os_str() != "Work" {
                return Err(denied("root engineering grant must name a Work member, not Central/Work structural scratch"));
            }
            if parts.len() == 2 && path.is_file() && grant.class != "declared-exception" {
                return Err(denied("loose Work-root file requires an explicit reviewed exception"));
            }
            if parts.len() == 2 && !path.exists() && grant.class != "declared-exception" {
                return Err(denied("new structural Work entry requires native adoption/creation, not a write grant"));
            }
        }
        Ok(WritableDestination { anchor: anchor(&scope.central_root, &path)?, path, class: grant.class.clone() })
    }).collect()
}
fn protection(scope: &Scope, policy: &PlacementPolicy) -> io::Result<Vec<PathBuf>> {
    let mut paths = vec![scope.root.join(".central"), scope.root.join(&scope.prefix).join("user"), scope.root.join(&scope.relations_path)];
    for path in &policy.protected { paths.push(absolute(scope, path)?); }
    Ok(paths)
}
fn level(level: &str) -> u8 {
    match level { "material-filesystem" => 2, "harness-interception" => 1, _ => 0 }
}
pub fn effective_policy(scope: &Scope, now: u64) -> io::Result<EffectivePolicy> {
    let root = Scope::resolve(&scope.central_root, None)?;
    let (root_policy, root_source) = policy_at(&root, now, true)?.ok_or_else(|| denied("root policy unavailable"))?;
    if root_policy.parent_policy.is_some() { return Err(invalid("root placement policy cannot name a Project parent")); }
    let mut grants = destinations(&root, &root_policy)?;
    let mut protected = protection(&root, &root_policy)?;
    let mut enforcement = root_policy.enforcement.clone();
    let mut coverage = root_policy.required_coverage.clone();
    let mut expiry = now.saturating_add(root_policy.lease_seconds).min(root_policy.expires_at_unix_seconds.unwrap_or(u64::MAX));
    let mut sources = vec![root_source];
    if scope.project.is_some() {
        grants.retain(|grant| grant.path.starts_with(&scope.root));
        protected.push(scope.root.join("ProjectCentral"));
        protected.push(scope.root.join(".central"));
        if let Some((local, local_source)) = policy_at(scope, now, false)? {
            let parent = local.parent_policy.as_ref().ok_or_else(|| invalid("Project policy must pin its root policy basis"))?;
            if parent.source_ref != sources[0].source.source_ref || parent.revision != sources[0].revision {
                return Err(conflict("Project placement policy pins a stale root policy"));
            }
            let local_grants = destinations(scope, &local)?;
            if local_grants.iter().any(|g| !grants.iter().any(|parent| g.path.starts_with(&parent.path))) {
                return Err(denied("Project policy cannot widen its root grants"));
            }
            grants = local_grants;
            protected.extend(protection(scope, &local)?);
            if level(&local.enforcement) > level(&enforcement) { enforcement = local.enforcement.clone(); }
            coverage.extend(local.required_coverage.clone());
            expiry = expiry.min(now.saturating_add(local.lease_seconds)).min(local.expires_at_unix_seconds.unwrap_or(u64::MAX));
            sources.push(local_source);
        }
    }
    coverage.sort(); coverage.dedup(); protected.sort(); protected.dedup();
    let stable = json!({"scope":scope.world_ref,"sources":sources,"writable":grants,"protected":protected,"enforcement":enforcement,"required_coverage":coverage});
    Ok(EffectivePolicy {
        schema: "central.effective-placement-policy/v1".into(), scope_ref: scope.world_ref.clone(), root_scope_ref: root.world_ref,
        revision: source::revision(&serde_json::to_string(&stable)?), sources,
        writable_destinations: grants, protected_paths: protected, enforcement, required_coverage: coverage,
        issued_at_unix_seconds: now, expires_at_unix_seconds: expiry,
        native_enforcement: "only operations routed through Central; consumers enforce their actual coverage".into(), outside_writes_prevented: false,
    })
}
pub(crate) fn checked_policy(scope: &Scope, input: &Value, now: u64) -> io::Result<EffectivePolicy> {
    let policy = effective_policy(scope, now)?;
    if policy.revision != text(input, "expected_policy_revision")? { return Err(conflict("effective placement policy changed; re-read central.work.policy before retry")); }
    Ok(policy)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NowRecord {
    pub schema: String,
    pub now_ref: String,
    pub source_ref: String,
    pub scope_ref: String,
    pub task_ref: String,
    pub purpose: String,
    pub participant_refs: Vec<String>,
    pub source_refs: Vec<String>,
    pub policy_revision_at_allocation: String,
    pub created_at_unix_seconds: u64,
    pub lifecycle: String,
    pub obligations: Vec<String>,
    pub continuation_refs: Vec<String>,
    pub archive_ref: Option<String>,
}
fn refs(input: &Value, name: &str) -> io::Result<Vec<String>> {
    let values: Vec<String> = serde_json::from_value(input.get(name).cloned().unwrap_or_else(|| json!([])))?;
    if values.len() > 256 || values.iter().any(|v| v.trim().is_empty() || v.len() > 4096) { return Err(invalid("invalid or excessive relationship refs")); }
    Ok(values)
}
pub(crate) fn read_now(scope: &Scope, reference: &str) -> io::Result<(NowRecord, source::SourceReading)> {
    for binding in scope.bindings()?.into_iter().filter(|b| b.roles.iter().any(|r| r == "now-clearing")) {
        let source = scope.read(&binding.source_ref)?;
        let record: NowRecord = serde_json::from_str(&source.content)?;
        if record.schema != NOW_SCHEMA || record.scope_ref != scope.world_ref || record.source_ref != binding.source_ref {
            return Err(invalid("NOW source identity/schema mismatch"));
        }
        if record.now_ref == reference { return Ok((record, source)); }
    }
    Err(io::Error::new(io::ErrorKind::NotFound, "NOW ref is not allocated in this World"))
}
pub(crate) fn now_destination(scope: &Scope, source_path: &str) -> io::Result<PathBuf> {
    let parent = Path::new(source_path).parent().ok_or_else(|| invalid("NOW source parent missing"))?;
    if !parent.starts_with(format!("{}/agents/now", scope.prefix)) { return Err(denied("NOW is outside the active agent aperture; inspect migration/continuation before re-entry")); }
    Ok(scope.root.join(parent).join("T"))
}
pub(crate) fn allocation_reading(scope: &Scope, record: &NowRecord, source: &source::SourceReading, mut policy: EffectivePolicy, created: bool) -> io::Result<Value> {
    let destination = now_destination(scope, &source.source.path)?;
    crate::file_mutation::directory(&scope.root, destination.strip_prefix(&scope.root).map_err(io::Error::other)?)?;
    policy.protected_paths.push(scope.root.join(&source.source.path));
    policy.writable_destinations.push(WritableDestination { path: destination.clone(), class: "now-artifact".into(), anchor: anchor(&scope.central_root, &destination)? });
    Ok(json!({"schema":"central.now-allocation/v1","created":created,"now_ref":record.now_ref,"source":source.source,"revision":source.revision,"record":record,"writable_destination":destination,"artifact_namespace":"T","permitted_artifact_kinds":["plans","findings","coordination","tracking"],"policy":policy,"automatic_agent_or_model_invocation":false}))
}
pub fn allocate(scope: &Scope, input: &Value, now: u64) -> io::Result<Value> {
    let _locks = source::lock(scope)?;
    let policy = checked_policy(scope, input, now)?;
    let task = text(input, "task_ref")?;
    let purpose = text(input, "purpose")?;
    let participants = refs(input, "participant_refs")?;
    let source_refs = refs(input, "source_refs")?;
    let now_ref = format!("central:now:{}:{}", scope.world_ref, key(task));
    match read_now(scope, &now_ref) {
        Ok((record, reading)) => {
            if record.task_ref != task || record.purpose != purpose || record.participant_refs != participants || record.source_refs != source_refs {
                return Err(conflict("allocation id already has a different task/purpose/relationship basis"));
            }
            if record.lifecycle != "active" { return Err(denied("existing NOW is not active; use explicit lifecycle re-entry, not another allocation")); }
            return allocation_reading(scope, &record, &reading, policy, false);
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(e),
    }
    scope.reconcile(None, &[])?;
    let path = format!("{}/{}/now.json", scope.now_dir(), key(task));
    let mut record = NowRecord {
        schema: NOW_SCHEMA.into(), now_ref, source_ref: scope.source_ref(&path), scope_ref: scope.world_ref.clone(),
        task_ref: task.into(), purpose: purpose.into(), participant_refs: participants, source_refs,
        policy_revision_at_allocation: policy.revision.clone(), created_at_unix_seconds: now,
        lifecycle: "active".into(), obligations: vec![], continuation_refs: vec![], archive_ref: None,
    };
    let mut created = true;
    // A process may die after publishing now.json but before binding it. Resume
    // that exact source, including its original allocation time and policy basis.
    match crate::source_safety::read(&scope.root, &path) {
        Ok(raw) => {
            let previous: NowRecord = serde_json::from_str(&raw)?;
            if previous.schema != record.schema || previous.now_ref != record.now_ref || previous.source_ref != record.source_ref
                || previous.scope_ref != record.scope_ref || previous.task_ref != record.task_ref || previous.purpose != record.purpose
                || previous.participant_refs != record.participant_refs || previous.source_refs != record.source_refs || previous.lifecycle != "active" {
                return Err(conflict("unbound NOW bytes disagree with allocation; inspect recovery instead of overwriting"));
            }
            record = previous; created = false;
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(e),
    }
    let destination = now_destination(scope, &path)?;
    source::directories(&scope.root, destination.strip_prefix(&scope.root).map_err(io::Error::other)?)?;
    let reading = scope.create_agent_source(&path, &encoded(&record)?, "now-clearing", now)?;
    scope.reconcile(Some(("central.now.allocate", "native-operation", None)), std::slice::from_ref(&reading.source.source_ref))?;
    allocation_reading(scope, &record, &reading, policy, created)
}
pub fn validate(scope: &Scope, input: &Value, now: u64) -> io::Result<Value> {
    let _locks = source::lock(scope)?;
    let policy = checked_policy(scope, input, now)?;
    let (record, reading) = read_now(scope, text(input, "now_ref")?)?;
    if reading.revision.revision != text(input, "expected_now_revision")? { return Err(conflict("NOW source revision changed")); }
    let valid_now = now_destination(scope, &reading.source.path)?;
    let raw = text(input, "destination")?;
    let path = if Path::new(raw).is_absolute() {
        let relative = Path::new(raw).strip_prefix(&scope.central_root).map_err(|_| denied("destination outside Central"))?;
        scope.central_root.join(relative_member(&relative.to_string_lossy())?)
    } else { scope.central_root.join(relative_member(raw)?) };
    let current_anchor = anchor(&scope.central_root, &path)?;
    if let Some(basis) = input.get("expected_destination_anchor") {
        let basis: PathAnchor = serde_json::from_value(basis.clone())?;
        if basis != current_anchor { return Err(conflict("destination basis changed since preview")); }
    }
    // NOW is an allocation aperture, not an override of explicit source law.
    // Both engineering and artifact writes require an active task, and every
    // explicit protected path wins even inside the allocated T directory.
    let protected = policy.protected_paths.iter().any(|protected| path.starts_with(protected));
    let in_now = path.starts_with(&valid_now) && path != valid_now;
    let is_metadata = path.strip_prefix(&scope.central_root).map_err(io::Error::other)?.components()
        .any(|part| matches!(part.as_os_str().to_str(), Some(".git" | ".central" | "ProjectCentral" | "Control")));
    let ordinary = !is_metadata && policy.writable_destinations.iter().any(|grant| path.starts_with(&grant.path));
    let allowed = record.lifecycle == "active" && !protected && (in_now || ordinary);
    Ok(json!({"schema":"central.work-placement-validation/v1","allowed":allowed,"outcome":if allowed {"permitted"} else {"rejected"},"destination":path,"destination_anchor":current_anchor,"now_ref":record.now_ref,"now_revision":reading.revision.revision,"policy_revision":policy.revision,"expires_at_unix_seconds":policy.expires_at_unix_seconds,"required_enforcement":policy.enforcement,"required_coverage":policy.required_coverage,"valid_now_destination":valid_now,"retry_action":"central.work.validate","reason":if allowed {"authorised NOW artifact or ordinary repository/worktree/build write; native source authority remains separate"} else {"task is inactive, destination is outside this task's allowed writes, or explicit source/structural protection applies; re-enter the same NOW explicitly or use the native source/adoption operation"},"outside_writes_prevented":false}))
}
