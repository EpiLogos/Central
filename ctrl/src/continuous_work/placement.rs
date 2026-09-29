use super::source::{self, conflict, denied, encoded, invalid, key, text, Scope};
use crate::source_horizon::SourceBinding;
use crate::source_safety::{reject_symlink_components, relative_member};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs;
use std::io;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

pub const POLICY_SCHEMA: &str = "central.work-placement-policy/v1";
pub const POLICY_ROLE: &str = "work-placement-policy";
pub const NOW_SCHEMA: &str = "central.now-clearing/v1";
/// Records that declare `work_refs` (lane ownership) carry the v2 schema so
/// older readers fail loudly on schema instead of confusingly on an unknown
/// field. Readers accept both; writers emit v2 only when `work_refs` exist.
pub const NOW_SCHEMA_V2: &str = "central.now-clearing/v2";

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
    /// Authored exclusions remain binding inside the native NOW aperture. This
    /// is separate from structural parent protection (for example ProjectCentral)
    /// whose one authorised exception is the allocated task T directory.
    pub explicit_protected_paths: Vec<PathBuf>,
    pub enforcement: String,
    pub required_coverage: Vec<String>,
    pub issued_at_unix_seconds: u64,
    pub expires_at_unix_seconds: u64,
    pub native_enforcement: String,
    pub outside_writes_prevented: bool,
}

pub(crate) fn anchor(root: &Path, absolute: &Path) -> io::Result<PathAnchor> {
    let relative = absolute
        .strip_prefix(root)
        .map_err(|_| denied("destination is outside Central"))?;
    reject_symlink_components(root, relative)?;
    let mut current = absolute;
    loop {
        match fs::symlink_metadata(current) {
            Ok(meta) => {
                if meta.file_type().is_symlink() {
                    return Err(denied(
                        "symlink destination requires reviewed material resolution",
                    ));
                }
                if !meta.is_dir() && !meta.is_file() {
                    return Err(denied("destination is not a regular file or directory"));
                }
                return Ok(PathAnchor {
                    path: absolute.into(),
                    existing_ancestor: current.into(),
                    device: meta.dev(),
                    inode: meta.ino(),
                    exists: current == absolute,
                });
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                current = current
                    .parent()
                    .ok_or_else(|| denied("destination has no existing Central ancestor"))?;
            }
            Err(e) => return Err(e),
        }
    }
}
fn absolute(scope: &Scope, raw: &str) -> io::Result<PathBuf> {
    if raw == "." && scope.project.is_some() {
        return Ok(scope.root.clone());
    }
    Ok(scope.root.join(relative_member(raw)?))
}

fn git_pointer(path: &Path, prefix: &str) -> io::Result<PathBuf> {
    if !fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_file()) {
        return Err(denied("worktree has an invalid native Git registration"));
    }
    let raw = fs::read_to_string(path)
        .map_err(|_| denied("worktree is missing its native Git registration"))?;
    let value = raw
        .strip_prefix(prefix)
        .map(str::trim)
        .filter(|value| !value.is_empty() && !value.contains(['\n', '\r']))
        .ok_or_else(|| denied("worktree has an invalid native Git registration"))?;
    let value = Path::new(value);
    Ok(if value.is_absolute() {
        value.to_path_buf()
    } else {
        path.parent()
            .ok_or_else(|| denied("worktree Git registration has no parent"))?
            .join(value)
    })
}

fn canonical_registration_path(root: &Path, path: &Path) -> io::Result<PathBuf> {
    let root = fs::canonicalize(root)
        .map_err(|_| denied("Central root is unavailable for worktree registration"))?;
    // macOS may spell the same filesystem root as /tmp and /private/tmp. Find
    // the outermost raw ancestor that is the Central root by filesystem
    // identity, then inspect every component below it without following links.
    let raw_root = path
        .ancestors()
        .filter(|ancestor| fs::canonicalize(ancestor).is_ok_and(|value| value == root))
        .last()
        .ok_or_else(|| denied("worktree Git registration leaves Central"))?;
    let relative = path
        .strip_prefix(raw_root)
        .map_err(|_| denied("worktree Git registration leaves Central"))?;
    let mut normal = root.clone();
    for component in relative.components() {
        match component {
            std::path::Component::Normal(component) => {
                normal.push(component);
                if fs::symlink_metadata(&normal)
                    .is_ok_and(|metadata| metadata.file_type().is_symlink())
                {
                    return Err(denied("worktree Git registration contains a symlink"));
                }
            }
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir if normal != root => {
                normal.pop();
            }
            _ => return Err(denied("worktree Git registration leaves Central")),
        }
    }
    let raw =
        fs::canonicalize(path).map_err(|_| denied("worktree Git registration is unavailable"))?;
    let normal =
        fs::canonicalize(normal).map_err(|_| denied("worktree Git registration is unavailable"))?;
    if raw != normal {
        return Err(denied(
            "worktree Git registration has ambiguous path traversal",
        ));
    }
    Ok(normal)
}

/// A root policy may explicitly grant one checkout under Central/worktrees,
/// but only when Git's two-way administrative registration proves that it is a
/// worktree of an ordinary Work repository already granted by the same policy.
fn validate_registered_worktree(
    scope: &Scope,
    policy: &PlacementPolicy,
    path: &Path,
) -> io::Result<PathBuf> {
    registered_worktree(scope, policy, path).map(|(common, _)| common)
}

/// The registration proof of [`validate_registered_worktree`], also naming the
/// authorised `repository` grant (absolute path) whose common Git directory
/// the checkout shares.
fn registered_worktree(
    scope: &Scope,
    policy: &PlacementPolicy,
    path: &Path,
) -> io::Result<(PathBuf, PathBuf)> {
    if !path.is_dir() {
        return Err(denied("worktree grant must name an existing checkout"));
    }
    let marker = path.join(".git");
    if !marker.is_file() {
        return Err(denied(
            "worktree grant requires a registered linked Git checkout",
        ));
    }
    let admin =
        canonical_registration_path(&scope.central_root, &git_pointer(&marker, "gitdir: ")?)?;
    if !admin.is_dir() {
        return Err(denied("worktree Git administration is unavailable"));
    }
    let common = canonical_registration_path(
        &scope.central_root,
        &git_pointer(&admin.join("commondir"), "")?,
    )?;
    if !common.is_dir() {
        return Err(denied("worktree common Git directory is unavailable"));
    }
    let registered_marker = canonical_registration_path(
        &scope.central_root,
        &git_pointer(&admin.join("gitdir"), "")?,
    )?;
    let marker = canonical_registration_path(&scope.central_root, &marker)?;
    if registered_marker != marker || admin.parent() != Some(common.join("worktrees").as_path()) {
        return Err(denied(
            "worktree Git registration does not match this checkout",
        ));
    }

    let mut authorised_repository = None;
    for grant in &policy.writable {
        if grant.class != "repository" {
            continue;
        }
        let repository = absolute(scope, &grant.path)?;
        let relative = repository
            .strip_prefix(&scope.central_root)
            .map_err(|_| denied("repository grant outside Central"))?;
        if relative
            .components()
            .next()
            .and_then(|part| part.as_os_str().to_str())
            != Some("Work")
        {
            continue;
        }
        let Ok(repository_git) =
            canonical_registration_path(&scope.central_root, &repository.join(".git"))
        else {
            continue;
        };
        if repository_git == common {
            authorised_repository = Some(repository);
            break;
        }
    }
    let Some(repository) = authorised_repository else {
        return Err(denied(
            "worktree common Git directory is not an authorised Work repository",
        ));
    };
    Ok((common, repository))
}

/// One registered-worktree grant of the recognised root placement policy that
/// contains a path, resolved to the Work member of the `repository` grant it
/// is a linked checkout of. `member` is absent (with `reason`) when the grant
/// fails its registration proof — the grant still claims the path.
#[derive(Debug, Clone, Serialize)]
pub struct WorktreeClaim {
    pub grant: String,
    pub member: Option<String>,
    pub reason: Option<String>,
}

/// Read-only resolution of `path` (canonical, inside Central) through the
/// root placement policy's `worktree` grants. `Ok(None)` means no recognised
/// root placement policy is adopted; an unrecognised or malformed policy is an
/// error the caller reports as data.
pub(crate) fn worktree_claims(
    central_root: &Path,
    path: &Path,
    now: u64,
) -> io::Result<Option<Vec<WorktreeClaim>>> {
    let root = Scope::resolve(central_root, None)?;
    let Some((policy, _)) = policy_at(&root, now, false)? else {
        return Ok(None);
    };
    let mut claims = Vec::new();
    for grant in policy.writable.iter().filter(|g| g.class == "worktree") {
        let absolute = absolute(&root, &grant.path)?;
        let Ok(canonical) = fs::canonicalize(&absolute) else {
            continue;
        };
        if !path.starts_with(&canonical) {
            continue;
        }
        let claim = match registered_worktree(&root, &policy, &absolute) {
            Ok((_, repository)) => match repository
                .strip_prefix(&root.central_root)
                .ok()
                .and_then(|relative| relative.components().nth(1))
                .and_then(|part| part.as_os_str().to_str())
            {
                Some(member) => WorktreeClaim {
                    grant: grant.path.clone(),
                    member: Some(member.into()),
                    reason: None,
                },
                None => WorktreeClaim {
                    grant: grant.path.clone(),
                    member: None,
                    reason: Some("repository grant does not name a Work member".into()),
                },
            },
            Err(error) => WorktreeClaim {
                grant: grant.path.clone(),
                member: None,
                reason: Some(error.to_string()),
            },
        };
        claims.push(claim);
    }
    Ok(Some(claims))
}
fn recognised(source: &SourceBinding) -> bool {
    matches!(
        source.provenance.as_str(),
        "human-authored" | "human-adopted"
    ) && matches!(
        source.standing.as_str(),
        "authored-human-position" | "design-commitment" | "architecture-contract"
    ) && source.agent_retrieval_allowed
}
fn policy_at(
    scope: &Scope,
    now: u64,
    required: bool,
) -> io::Result<Option<(PlacementPolicy, PolicySource)>> {
    let (relations, _) = scope.relations()?;
    let candidates: Vec<_> = relations["relations"]
        .as_array()
        .ok_or_else(|| invalid("invalid relations"))?
        .iter()
        .filter(|r| {
            r["roles"]
                .as_array()
                .is_some_and(|roles| roles.iter().any(|role| role == POLICY_ROLE))
        })
        .collect();
    if candidates.len() > 1 {
        return Err(invalid(
            "multiple effective placement-policy relations require reconciliation",
        ));
    }
    let Some(relation) = candidates.first() else {
        return if required {
            Err(denied("no recognised root placement policy; record exact policy adoption through Central source operations"))
        } else {
            Ok(None)
        };
    };
    let reading = scope.read(text(relation, "ref")?)?;
    if !recognised(&reading.source)
        || relation["recognition"].as_str().is_none_or(|s| {
            s.trim().is_empty() || s == "owner-recorded-source-relation-not-human-recognition"
        })
    {
        return Err(denied(
            "placement-policy source is draft/unrecognised; no fallback or authority widening",
        ));
    }
    let policy: PlacementPolicy = serde_json::from_str(&reading.content)?;
    if policy.schema != POLICY_SCHEMA || policy.scope_ref != scope.world_ref {
        return Err(invalid("placement-policy schema or scope mismatch"));
    }
    if policy.lease_seconds == 0
        || policy.lease_seconds > 86400
        || policy
            .expires_at_unix_seconds
            .is_some_and(|expiry| now >= expiry)
    {
        return Err(denied(
            "placement policy is expired or has an invalid bounded lease",
        ));
    }
    if !matches!(
        policy.enforcement.as_str(),
        "native-actions" | "harness-interception" | "material-filesystem"
    ) || policy.required_coverage.is_empty()
        || policy.required_coverage.len() > 32
    {
        return Err(invalid(
            "placement policy requires a known enforcement level and explicit coverage",
        ));
    }
    let mut authorities = vec![SourceBasis {
        source_ref: reading.source.source_ref.clone(),
        revision: reading.revision.revision.clone(),
    }];
    for basis in &policy.authority_refs {
        let authority = scope.read(&basis.source_ref)?;
        if authority.revision.revision != basis.revision {
            return Err(conflict("placement authority source revision is stale"));
        }
        if !recognised(&authority.source) {
            return Err(denied("placement authority is not recognised human source"));
        }
        authorities.push(basis.clone());
    }
    Ok(Some((
        policy,
        PolicySource {
            source: reading.source,
            revision: reading.revision.revision,
            authority_refs: authorities,
        },
    )))
}
fn destinations(scope: &Scope, policy: &PlacementPolicy) -> io::Result<Vec<WritableDestination>> {
    if policy.writable.len() > 256 || policy.protected.len() > 256 {
        return Err(invalid(
            "placement policy exceeds bounded destination count",
        ));
    }
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
            let registered_worktree = parts.len() >= 3
                && parts[0].as_os_str() == "worktrees"
                && grant.class == "worktree";
            if registered_worktree {
                validate_registered_worktree(scope, policy, &path)?;
            } else if parts.len() < 2 || parts[0].as_os_str() != "Work" {
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
fn explicit_protection(scope: &Scope, policy: &PlacementPolicy) -> io::Result<Vec<PathBuf>> {
    policy
        .protected
        .iter()
        .map(|path| absolute(scope, path))
        .collect()
}
fn protection(scope: &Scope, policy: &PlacementPolicy) -> io::Result<Vec<PathBuf>> {
    let mut paths = vec![
        scope.root.join(".central"),
        scope.root.join(&scope.prefix).join("user"),
        scope.root.join(&scope.relations_path),
    ];
    if scope.project.is_none() {
        for grant in &policy.writable {
            if grant.class != "worktree" {
                continue;
            }
            let worktree = absolute(scope, &grant.path)?;
            let relative = worktree
                .strip_prefix(&scope.central_root)
                .map_err(|_| denied("worktree grant outside Central"))?;
            let parts = relative.components().collect::<Vec<_>>();
            if parts.len() >= 3 && parts[0].as_os_str() == "worktrees" {
                let common = validate_registered_worktree(scope, policy, &worktree)?;
                paths.extend([
                    worktree.join(".git"),
                    worktree.join(".central"),
                    worktree.join("ProjectCentral"),
                    common,
                ]);
            }
        }
    }
    paths.extend(explicit_protection(scope, policy)?);
    Ok(paths)
}
fn level(level: &str) -> u8 {
    match level {
        "material-filesystem" => 2,
        "harness-interception" => 1,
        _ => 0,
    }
}
pub fn effective_policy(scope: &Scope, now: u64) -> io::Result<EffectivePolicy> {
    let root = Scope::resolve(&scope.central_root, None)?;
    let (root_policy, root_source) =
        policy_at(&root, now, true)?.ok_or_else(|| denied("root policy unavailable"))?;
    if root_policy.parent_policy.is_some() {
        return Err(invalid(
            "root placement policy cannot name a Project parent",
        ));
    }
    let mut grants = destinations(&root, &root_policy)?;
    let mut protected = protection(&root, &root_policy)?;
    let mut explicit_protected = explicit_protection(&root, &root_policy)?;
    let mut enforcement = root_policy.enforcement.clone();
    let mut coverage = root_policy.required_coverage.clone();
    let mut expiry = now
        .saturating_add(root_policy.lease_seconds)
        .min(root_policy.expires_at_unix_seconds.unwrap_or(u64::MAX));
    let mut sources = vec![root_source];
    if scope.project.is_some() {
        grants.retain(|grant| grant.path.starts_with(&scope.root));
        protected.push(scope.root.join("ProjectCentral"));
        protected.push(scope.root.join(".central"));
        if let Some((local, local_source)) = policy_at(scope, now, false)? {
            let parent = local
                .parent_policy
                .as_ref()
                .ok_or_else(|| invalid("Project policy must pin its root policy basis"))?;
            if parent.source_ref != sources[0].source.source_ref
                || parent.revision != sources[0].revision
            {
                return Err(conflict(
                    "Project placement policy pins a stale root policy",
                ));
            }
            let local_grants = destinations(scope, &local)?;
            if local_grants
                .iter()
                .any(|g| !grants.iter().any(|parent| g.path.starts_with(&parent.path)))
            {
                return Err(denied("Project policy cannot widen its root grants"));
            }
            grants = local_grants;
            protected.extend(protection(scope, &local)?);
            explicit_protected.extend(explicit_protection(scope, &local)?);
            if level(&local.enforcement) > level(&enforcement) {
                enforcement = local.enforcement.clone();
            }
            coverage.extend(local.required_coverage.clone());
            expiry = expiry
                .min(now.saturating_add(local.lease_seconds))
                .min(local.expires_at_unix_seconds.unwrap_or(u64::MAX));
            sources.push(local_source);
        }
    }
    coverage.sort();
    coverage.dedup();
    protected.sort();
    protected.dedup();
    explicit_protected.sort();
    explicit_protected.dedup();
    let stable = json!({"scope":scope.world_ref,"sources":sources,"writable":grants,"protected":protected,"explicit_protected":explicit_protected,"enforcement":enforcement,"required_coverage":coverage});
    Ok(EffectivePolicy {
        schema: "central.effective-placement-policy/v1".into(),
        scope_ref: scope.world_ref.clone(),
        root_scope_ref: root.world_ref,
        revision: source::revision(&serde_json::to_string(&stable)?),
        sources,
        writable_destinations: grants,
        protected_paths: protected,
        explicit_protected_paths: explicit_protected,
        enforcement,
        required_coverage: coverage,
        issued_at_unix_seconds: now,
        expires_at_unix_seconds: expiry,
        native_enforcement:
            "only operations routed through Central; consumers enforce their actual coverage".into(),
        outside_writes_prevented: false,
    })
}
pub(crate) fn checked_policy(
    scope: &Scope,
    input: &Value,
    now: u64,
) -> io::Result<EffectivePolicy> {
    let policy = effective_policy(scope, now)?;
    if policy.revision != text(input, "expected_policy_revision")? {
        return Err(conflict(
            "effective placement policy changed; re-read central.work.policy before retry",
        ));
    }
    Ok(policy)
}

/// A declared lane-ownership reference: one NOW record claiming one branch
/// (optionally one worktree) of one repository. Shared by the clearing
/// schema (`work_refs`) and the project handoff schema; also consumed by the
/// git census read model for lane attribution. Declared by callers, never
/// inferred.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkRef {
    /// Repository path relative to the Central root (e.g. `Work/O-I`).
    pub repo: String,
    pub branch: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worktree_path: Option<String>,
}

/// Parse the optional `work_refs` input with plain bounds: non-empty
/// repo/branch strings, at most 64 entries.
pub(crate) fn work_refs_input(input: &Value) -> io::Result<Vec<WorkRef>> {
    let value = match input.get("work_refs") {
        None | Some(Value::Null) => return Ok(vec![]),
        Some(value @ Value::Array(_)) => value.clone(),
        Some(_) => return Err(invalid("work_refs must be an array")),
    };
    let refs: Vec<WorkRef> = serde_json::from_value(value)
        .map_err(|_| invalid("work_refs entries require non-empty repo and branch strings"))?;
    if refs.len() > 64 {
        return Err(invalid("work_refs carries at most 64 entries"));
    }
    for work_ref in &refs {
        if work_ref.repo.trim().is_empty() || work_ref.branch.trim().is_empty() {
            return Err(invalid(
                "work_refs entries require non-empty repo and branch strings",
            ));
        }
    }
    Ok(refs)
}

fn schema_for(record: &NowRecord) -> &'static str {
    if record.work_refs.is_empty() {
        NOW_SCHEMA
    } else {
        NOW_SCHEMA_V2
    }
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub work_refs: Vec<WorkRef>,
    /// Material placement: the Workcell this NOW is placed on
    /// (`workcell:<label>`). Declared by the caller, never inferred. Absent on
    /// every record written before the World-inhabitation horizon existed, and
    /// skipped when absent so those records stay byte-identical.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workcell_ref: Option<String>,
    /// The NOW this clearing hangs from: a NOW of the same scope or of the
    /// root scope. Present exactly when `horizon` is `child`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_now_ref: Option<String>,
    /// `workcell-root` (the one root NOW of a Workcell, root register only) or
    /// `child`. Absent means a standalone clearing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub horizon: Option<String>,
}
pub const HORIZON_WORKCELL_ROOT: &str = "workcell-root";
pub const HORIZON_CHILD: &str = "child";

/// The deterministic task identity of a Workcell's root NOW.
pub fn workcell_root_task_ref(workcell_ref: &str) -> String {
    format!("central:task:control:root:workcell-root:{workcell_ref}")
}
/// The NOW ref a task allocates in a scope (pure function of scope + task).
pub fn now_ref_for(world_ref: &str, task_ref: &str) -> String {
    format!("central:now:{world_ref}:{}", key(task_ref))
}
fn workcell_ref_input(input: &Value) -> io::Result<Option<String>> {
    match input.get("workcell_ref") {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => {
            validate_workcell_ref(value)?;
            Ok(Some(value.clone()))
        }
        Some(_) => Err(invalid("workcell_ref must be a string")),
    }
}
pub(crate) fn validate_workcell_ref(value: &str) -> io::Result<()> {
    let label = value.strip_prefix("workcell:").unwrap_or_default();
    if label.is_empty()
        || value.len() > 256
        || value
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || c == '\0')
    {
        return Err(invalid(
            "workcell_ref must be workcell:<label> with a non-empty label and no whitespace",
        ));
    }
    Ok(())
}
/// Horizon shape law, checked on every read so a malformed record fails as an
/// identity fault instead of silently reading as standalone.
fn check_horizon(record: &NowRecord) -> io::Result<()> {
    let fault = |detail: &str| {
        Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "NOW horizon is malformed at {}: {detail}",
                record.source_ref
            ),
        ))
    };
    if let Some(workcell) = &record.workcell_ref {
        if validate_workcell_ref(workcell).is_err() {
            return fault("workcell_ref is not workcell:<label>");
        }
    }
    match record.horizon.as_deref() {
        None if record.parent_now_ref.is_some() => fault("parent_now_ref requires horizon child"),
        None => Ok(()),
        Some(HORIZON_CHILD) if record.parent_now_ref.is_none() => {
            fault("a child NOW requires parent_now_ref")
        }
        Some(HORIZON_CHILD) => Ok(()),
        Some(HORIZON_WORKCELL_ROOT)
            if record.parent_now_ref.is_some()
                || record.workcell_ref.is_none()
                || record.scope_ref != crate::source_horizon::CONTROL_WORLD_REF =>
        {
            fault("a workcell-root NOW is a root-register clearing with workcell_ref and no parent")
        }
        Some(HORIZON_WORKCELL_ROOT) => Ok(()),
        Some(_) => fault("horizon is workcell-root or child"),
    }
}
fn refs(input: &Value, name: &str) -> io::Result<Vec<String>> {
    let values: Vec<String> =
        serde_json::from_value(input.get(name).cloned().unwrap_or_else(|| json!([])))?;
    if values.len() > 256 || values.iter().any(|v| v.trim().is_empty() || v.len() > 4096) {
        return Err(invalid("invalid or excessive relationship refs"));
    }
    Ok(values)
}
/// Every clearing bound with the now-clearing role in this scope, parsed and
/// identity-checked. One NOW ref bound more than once yields its first binding
/// only. Every bound source must parse and co-refer, or the scan fails.
pub(crate) fn scan_now(scope: &Scope) -> io::Result<Vec<(NowRecord, source::SourceReading)>> {
    let mut seen = std::collections::BTreeSet::new();
    let mut records = Vec::new();
    for binding in scope
        .bindings()?
        .into_iter()
        .filter(|b| b.roles.iter().any(|r| r == "now-clearing"))
    {
        let source = scope.read_bound(binding.clone())?;
        let record: NowRecord = serde_json::from_str(&source.content).map_err(|error| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "now-clearing record {} failed to parse: {error}",
                    binding.source_ref
                ),
            )
        })?;
        if !matches!(record.schema.as_str(), NOW_SCHEMA | NOW_SCHEMA_V2)
            || record.scope_ref != scope.world_ref
            || record.source_ref != binding.source_ref
        {
            return Err(invalid(format!(
                "NOW source identity/schema mismatch at {}",
                binding.source_ref
            )));
        }
        check_horizon(&record)?;
        if seen.insert(record.now_ref.clone()) {
            records.push((record, source));
        }
    }
    Ok(records)
}
pub(crate) fn read_now(
    scope: &Scope,
    reference: &str,
) -> io::Result<(NowRecord, source::SourceReading)> {
    scan_now(scope)?
        .into_iter()
        .find(|(record, _)| record.now_ref == reference)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "NOW ref is not allocated in this World",
            )
        })
}
/// The listing row of one clearing. Horizon fields appear only on records
/// that carry them, so a listing of standalone clearings is unchanged.
fn now_row(record: &NowRecord, source: &source::SourceReading) -> Value {
    let mut row = json!({
        "now_ref": record.now_ref,
        "source_ref": record.source_ref,
        "scope_ref": record.scope_ref,
        "task_ref": record.task_ref,
        "purpose": record.purpose,
        "participant_refs": record.participant_refs,
        "source_refs": record.source_refs,
        "lifecycle": record.lifecycle,
        "created_at_unix_seconds": record.created_at_unix_seconds,
        "work_refs": record.work_refs,
        "revision": source.revision,
    });
    for (name, value) in [
        ("workcell_ref", &record.workcell_ref),
        ("parent_now_ref", &record.parent_now_ref),
        ("horizon", &record.horizon),
    ] {
        if let Some(value) = value {
            row[name] = json!(value);
        }
    }
    row
}
/// List the World's allocated NOW clearings, optionally filtered to records
/// carrying any of the given participant refs. An empty filter lists every
/// allocated clearing. Same identity law as `read_now`: every source bound
/// with the now-clearing role must parse and co-refer, or the listing fails.
/// A now_ref bound more than once lists once — the first binding, exactly
/// what `read_now` would return.
pub(crate) fn list_now(scope: &Scope, input: &Value) -> io::Result<Vec<Value>> {
    let participant_filter = refs(input, "participant_refs")?;
    let mut rows: Vec<Value> = scan_now(scope)?
        .iter()
        .filter(|(record, _)| {
            participant_filter.is_empty()
                || record
                    .participant_refs
                    .iter()
                    .any(|p| participant_filter.contains(p))
        })
        .map(|(record, source)| now_row(record, source))
        .collect();
    rows.sort_by(|a, b| {
        a["now_ref"]
            .as_str()
            .unwrap_or("")
            .cmp(b["now_ref"].as_str().unwrap_or(""))
    });
    Ok(rows)
}
pub(crate) fn now_destination(scope: &Scope, source_path: &str) -> io::Result<PathBuf> {
    let parent = Path::new(source_path)
        .parent()
        .ok_or_else(|| invalid("NOW source parent missing"))?;
    if !parent.starts_with(format!("{}/agents/now", scope.prefix)) {
        return Err(denied("NOW is outside the active agent aperture; inspect migration/continuation before re-entry"));
    }
    Ok(scope.root.join(parent).join("T"))
}
/// Refuse a wholly excluded clearing before creating/rebinding source or T.
/// A protected descendant does not erase the entire aperture; validation still
/// excludes that descendant and ambiguous mutations of its parents.
fn check_allocation_protection(
    scope: &Scope,
    source_path: &str,
    policy: &EffectivePolicy,
) -> io::Result<()> {
    let source_path_absolute = scope.root.join(source_path);
    let destination = now_destination(scope, source_path)?;
    if policy.explicit_protected_paths.iter().any(|protected| {
        source_path_absolute.starts_with(protected) || destination.starts_with(protected)
    }) {
        return Err(denied("NOW allocation conflicts with an explicit protected source/destination; resolve the current owner policy before retry"));
    }
    Ok(())
}
pub(super) fn allocation_reading(
    scope: &Scope,
    record: &NowRecord,
    source: &source::SourceReading,
    mut policy: EffectivePolicy,
    created: bool,
) -> io::Result<Value> {
    check_allocation_protection(scope, &source.source.path, &policy)?;
    let destination = now_destination(scope, &source.source.path)?;
    crate::file_mutation::directory(
        &scope.root,
        destination
            .strip_prefix(&scope.root)
            .map_err(io::Error::other)?,
    )?;
    policy
        .protected_paths
        .push(scope.root.join(&source.source.path));
    policy.writable_destinations.push(WritableDestination {
        path: destination.clone(),
        class: "now-artifact".into(),
        anchor: anchor(&scope.central_root, &destination)?,
    });
    Ok(
        json!({"schema":"central.now-allocation/v1","created":created,"now_ref":record.now_ref,"source":source.source,"revision":source.revision,"record":record,"writable_destination":destination,"artifact_namespace":"T","permitted_artifact_kinds":["plans","findings","coordination","tracking"],"policy":policy,"automatic_agent_or_model_invocation":false}),
    )
}
/// The allocation basis of one clearing: everything its identity and
/// idempotent replay are compared on.
struct AllocationSpec {
    task: String,
    purpose: String,
    participants: Vec<String>,
    source_refs: Vec<String>,
    work_refs: Vec<WorkRef>,
    workcell_ref: Option<String>,
    parent_now_ref: Option<String>,
    horizon: Option<String>,
}
impl AllocationSpec {
    fn matches(&self, record: &NowRecord) -> bool {
        record.task_ref == self.task
            && record.purpose == self.purpose
            && record.participant_refs == self.participants
            && record.source_refs == self.source_refs
            && record.work_refs == self.work_refs
            && record.workcell_ref == self.workcell_ref
            && record.parent_now_ref == self.parent_now_ref
            && record.horizon == self.horizon
    }
}
pub fn allocate(scope: &Scope, input: &Value, now: u64) -> io::Result<Value> {
    let _locks = source::lock(scope)?;
    let policy = checked_policy(scope, input, now)?;
    let task = text(input, "task_ref")?;
    let parent_now_ref = match input.get("parent_now_ref") {
        None | Some(Value::Null) => None,
        Some(_) => Some(text(input, "parent_now_ref")?.to_owned()),
    };
    let spec = AllocationSpec {
        task: task.into(),
        purpose: text(input, "purpose")?.into(),
        participants: refs(input, "participant_refs")?,
        source_refs: refs(input, "source_refs")?,
        work_refs: work_refs_input(input)?,
        workcell_ref: workcell_ref_input(input)?,
        horizon: parent_now_ref.as_ref().map(|_| HORIZON_CHILD.to_owned()),
        parent_now_ref,
    };
    if let Some(parent) = &spec.parent_now_ref {
        check_parent(scope, parent, &now_ref_for(&scope.world_ref, &spec.task))?;
    }
    allocate_locked(scope, policy, spec, now)
}
/// A parent NOW must already be allocated in this scope or in the root scope.
/// The scope is read from the ref itself, so a ref naming another Project's
/// NOW is refused rather than searched for.
fn check_parent(scope: &Scope, parent: &str, own: &str) -> io::Result<()> {
    if parent == own {
        return Err(invalid("a NOW cannot be its own parent_now_ref"));
    }
    let in_world = |world: &str| {
        parent
            .strip_prefix(&format!("central:now:{world}:"))
            .is_some_and(|key| key.len() == 64 && key.bytes().all(|b| b.is_ascii_hexdigit()))
    };
    let parent_scope = if in_world(&scope.world_ref) {
        scope.clone()
    } else if in_world(crate::source_horizon::CONTROL_WORLD_REF) {
        Scope::resolve(&scope.central_root, None)?
    } else {
        return Err(invalid(format!(
            "parent_now_ref {parent} is not a NOW of {} or of the root scope; a child NOW hangs only from its own scope or the root register",
            scope.world_ref
        )));
    };
    match read_now(&parent_scope, parent) {
        Ok(_) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!(
                "parent_now_ref {parent} is not allocated in {}; nothing was allocated. Allocate or re-read the parent (central.now.list) before allocating the child",
                parent_scope.world_ref
            ),
        )),
        Err(e) => Err(e),
    }
}
/// `central.now.workcell-root`: idempotently ensure the one root NOW of a
/// Workcell in the root register. Its task identity is a pure function of the
/// Workcell ref, so every caller converges on the same clearing.
pub fn workcell_root(scope: &Scope, input: &Value, now: u64) -> io::Result<Value> {
    if scope.project.is_some() {
        return Err(invalid(
            "central.now.workcell-root runs in the root register only; omit project",
        ));
    }
    let workcell_ref = workcell_ref_input(input)?
        .ok_or_else(|| invalid("workcell_ref requires workcell:<label>"))?;
    let _locks = source::lock(scope)?;
    let policy = match input.get("expected_policy_revision") {
        None | Some(Value::Null) => effective_policy(scope, now)?,
        Some(_) => checked_policy(scope, input, now)?,
    };
    let spec = AllocationSpec {
        task: workcell_root_task_ref(&workcell_ref),
        purpose: format!(
            "Workcell root NOW for {workcell_ref}: the live material horizon that child NOWs placed on this Workcell hang from."
        ),
        participants: vec![],
        source_refs: vec![],
        work_refs: vec![],
        workcell_ref: Some(workcell_ref),
        parent_now_ref: None,
        horizon: Some(HORIZON_WORKCELL_ROOT.into()),
    };
    allocate_locked(scope, policy, spec, now)
}
fn allocate_locked(
    scope: &Scope,
    policy: EffectivePolicy,
    spec: AllocationSpec,
    now: u64,
) -> io::Result<Value> {
    let now_ref = now_ref_for(&scope.world_ref, &spec.task);
    match read_now(scope, &now_ref) {
        Ok((record, reading)) => {
            if !spec.matches(&record) {
                return Err(conflict(
                    "allocation id already has a different task/purpose/relationship basis",
                ));
            }
            if record.lifecycle != "active" {
                return Err(denied("existing NOW is not active; use explicit lifecycle re-entry, not another allocation"));
            }
            return allocation_reading(scope, &record, &reading, policy, false);
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(e),
    }
    let path = format!("{}/{}/now.json", scope.now_dir(), key(&spec.task));
    check_allocation_protection(scope, &path, &policy)?;
    scope.reconcile(None, &[])?;
    let mut record = NowRecord {
        schema: if spec.work_refs.is_empty() {
            NOW_SCHEMA.into()
        } else {
            NOW_SCHEMA_V2.into()
        },
        now_ref,
        source_ref: scope.source_ref(&path),
        scope_ref: scope.world_ref.clone(),
        task_ref: spec.task.clone(),
        purpose: spec.purpose.clone(),
        participant_refs: spec.participants.clone(),
        source_refs: spec.source_refs.clone(),
        policy_revision_at_allocation: policy.revision.clone(),
        created_at_unix_seconds: now,
        lifecycle: "active".into(),
        obligations: vec![],
        continuation_refs: vec![],
        archive_ref: None,
        work_refs: spec.work_refs.clone(),
        workcell_ref: spec.workcell_ref.clone(),
        parent_now_ref: spec.parent_now_ref.clone(),
        horizon: spec.horizon.clone(),
    };
    let mut created = true;
    // A process may die after publishing now.json but before binding it. Resume
    // that exact source, including its original allocation time and policy basis.
    match crate::source_safety::read(&scope.root, &path) {
        Ok(raw) => {
            let previous: NowRecord = serde_json::from_str(&raw)?;
            if previous.schema != schema_for(&record)
                || previous.now_ref != record.now_ref
                || previous.source_ref != record.source_ref
                || previous.scope_ref != record.scope_ref
                || !spec.matches(&previous)
                || previous.lifecycle != "active"
            {
                return Err(conflict("unbound NOW bytes disagree with allocation; inspect recovery instead of overwriting"));
            }
            record = previous;
            created = false;
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(e),
    }
    let destination = now_destination(scope, &path)?;
    source::directories(
        &scope.root,
        destination
            .strip_prefix(&scope.root)
            .map_err(io::Error::other)?,
    )?;
    let reading = scope.create_agent_source(&path, &encoded(&record)?, "now-clearing", now)?;
    scope.reconcile(
        Some(("central.now.allocate", "native-operation", None)),
        std::slice::from_ref(&reading.source.source_ref),
    )?;
    allocation_reading(scope, &record, &reading, policy, created)
}
/// `central.now.children`: every clearing whose parent_now_ref is this NOW,
/// uncapped. A root NOW's children may live in the root register or in any
/// Project; a Project NOW's children live only in its own Project. A Project
/// whose clearings cannot be read is named in `unscanned`, never skipped
/// silently.
pub fn children(scope: &Scope, input: &Value) -> io::Result<Value> {
    let (parent, parent_source) = read_now(scope, text(input, "now_ref")?)?;
    let mut scopes = vec![(scope.project.clone(), Ok(scope.clone()))];
    if scope.project.is_none() {
        let mut members: Vec<String> = fs::read_dir(scope.central_root.join("Work"))?
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.path().join("ProjectCentral/project.json").is_file())
            .filter_map(|entry| entry.file_name().to_str().map(str::to_owned))
            .filter(|name| !name.starts_with('.'))
            .collect();
        members.sort();
        for member in members {
            let resolved = Scope::resolve(&scope.central_root, Some(&member));
            scopes.push((Some(member), resolved));
        }
    }
    let mut rows = Vec::new();
    let mut unscanned = Vec::new();
    for (project, resolved) in scopes {
        let records = resolved
            .and_then(|child_scope| scan_now(&child_scope).map(|records| (child_scope, records)));
        match records {
            Ok((child_scope, records)) => {
                for (record, source) in records {
                    if record.parent_now_ref.as_deref() != Some(parent.now_ref.as_str()) {
                        continue;
                    }
                    let mut row = now_row(&record, &source);
                    row["project"] = json!(child_scope.project);
                    row["live"] = json!(record.lifecycle == "active");
                    rows.push(row);
                }
            }
            Err(error) => unscanned.push(json!({"project": project, "reason": error.to_string()})),
        }
    }
    rows.sort_by(|a, b| {
        (a["scope_ref"].as_str(), a["now_ref"].as_str())
            .cmp(&(b["scope_ref"].as_str(), b["now_ref"].as_str()))
    });
    Ok(json!({
        "schema": "central.now-children/v1",
        "now_ref": parent.now_ref,
        "parent": now_row(&parent, &parent_source),
        "count": rows.len(),
        "children": rows,
        "unscanned": unscanned,
        "automatic_agent_or_model_invocation": false,
    }))
}
/// The live NOW horizon of one scope as a Day boundary sees it: every clearing
/// that carries a horizon (Workcell root or child). Active ones carry across
/// the boundary; quiescent ones are released from the live horizon and stay
/// retained. Closed and archived clearings left the horizon through their own
/// explicit lifecycle act and are not re-reported. Read-only: a Day boundary
/// never closes, completes or archives a clearing.
pub fn horizon_reading(scope: &Scope) -> io::Result<Value> {
    let mut carried = Vec::new();
    let mut released = Vec::new();
    for (record, _) in scan_now(scope)? {
        let Some(horizon) = record.horizon.as_deref() else {
            continue;
        };
        let entry = json!({
            "now_ref": record.now_ref,
            "horizon": horizon,
            "lifecycle": record.lifecycle,
            "workcell_ref": record.workcell_ref,
            "parent_now_ref": record.parent_now_ref,
        });
        match record.lifecycle.as_str() {
            "active" => carried.push(entry),
            "quiescent" => released.push(entry),
            _ => {}
        }
    }
    Ok(json!({
        "scope_ref": scope.world_ref,
        "carried": carried,
        "released": released,
        "clearings_closed_completed_or_archived": false,
    }))
}
pub fn validate(scope: &Scope, input: &Value, now: u64) -> io::Result<Value> {
    let _locks = source::lock(scope)?;
    let policy = checked_policy(scope, input, now)?;
    let (record, reading) = read_now(scope, text(input, "now_ref")?)?;
    if reading.revision.revision != text(input, "expected_now_revision")? {
        return Err(conflict("NOW source revision changed"));
    }
    let valid_now = now_destination(scope, &reading.source.path)?;
    let raw = text(input, "destination")?;
    let path = if Path::new(raw).is_absolute() {
        let relative = Path::new(raw)
            .strip_prefix(&scope.central_root)
            .map_err(|_| denied("destination outside Central"))?;
        scope
            .central_root
            .join(relative_member(&relative.to_string_lossy())?)
    } else {
        scope.central_root.join(relative_member(raw)?)
    };
    let current_anchor = anchor(&scope.central_root, &path)?;
    if let Some(basis) = input.get("expected_destination_anchor") {
        let basis: PathAnchor = serde_json::from_value(basis.clone())?;
        if basis != current_anchor {
            return Err(conflict("destination basis changed since preview"));
        }
    }
    // This public operation does not distinguish a content write from a recursive
    // remove/rename. An ancestor of an explicit protected object cannot receive
    // an ambiguous approval. A sibling remains writable.
    let explicitly_protected = policy
        .explicit_protected_paths
        .iter()
        .any(|protected| path.starts_with(protected) || protected.starts_with(&path));
    let in_now = path.starts_with(&valid_now) && path != valid_now;
    let is_metadata = path
        .strip_prefix(&scope.central_root)
        .map_err(io::Error::other)?
        .components()
        .any(|part| {
            matches!(
                part.as_os_str().to_str(),
                Some(".git" | ".central" | "ProjectCentral" | "Control")
            )
        });
    let ordinary = !is_metadata
        && policy
            .writable_destinations
            .iter()
            .any(|grant| path.starts_with(&grant.path))
        && !policy
            .protected_paths
            .iter()
            .any(|protected| path.starts_with(protected) || protected.starts_with(&path));
    // An inactive task cannot borrow the Project grant to continue effects while
    // its NOW is quiescent/closed/archived. Explicit re-entry retains its identity.
    let active = record.lifecycle == "active";
    let allowed = active && !explicitly_protected && (in_now || ordinary);
    let reason = if !active {
        "task NOW is not active; use explicit lifecycle re-entry before any task write"
    } else if explicitly_protected {
        "destination intersects explicit protected source ground; an allocated NOW never overrides it"
    } else if allowed {
        "authorised NOW artifact or ordinary repository/worktree/build write; native source authority remains separate"
    } else {
        "destination is outside this task's allowed writes or is protected structural/source ground; use the allocated NOW T destination or the native source/adoption operation"
    };
    Ok(
        json!({"schema":"central.work-placement-validation/v1","allowed":allowed,"outcome":if allowed {"permitted"} else {"rejected"},"destination":path,"destination_anchor":current_anchor,"now_ref":record.now_ref,"now_lifecycle":record.lifecycle,"now_revision":reading.revision.revision,"policy_revision":policy.revision,"expires_at_unix_seconds":policy.expires_at_unix_seconds,"required_enforcement":policy.enforcement,"required_coverage":policy.required_coverage,"valid_now_destination":valid_now,"retry_action":"central.work.validate","reason":reason,"outside_writes_prevented":false}),
    )
}
