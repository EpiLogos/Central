//! `central.git.census` — the observed branch and worktree state of the
//! world's repositories, as one read-only read model.
//!
//! The census never fetches, prunes or cleans. It answers "what is open,
//! where, and who owns it" so agents can join existing lanes instead of
//! minting new ones, and so day closes can reconcile open work against
//! recorded returns. Attribution is declared, never inferred: a worktree is
//! an aikit task lane when it lives under `.aikit/tasks/`, and it is a
//! recorded lane when an active NOW clearing or project handoff carries a
//! matching `work_refs` entry. Everything else is `unattributed`.

use crate::action::{
    ActionAvailability, ActionDescriptor, ActionExecutionContext, ActionInputDefinition,
    ActionOutputDefinition, ActionRegistry, MutationClass,
};
use crate::result::{ActionResult, ResultStatus};
use crate::root::resolve_central_root;
use central_connector_sdk::{GitCensusRequest, GitRepoCensus, GitState, GIT_STATE_PORT};
use chrono::{DateTime, Utc};
use serde_json::{json, to_value, Value};
use std::collections::BTreeMap;
use std::io;
use std::path::{Component, Path};

pub const CENTRAL_GIT_CENSUS_SCHEMA: &str = "central.git-census/v1";

/// Directories never descended into during repository discovery.
const DISCOVERY_SKIP: [&str; 6] = [".git", "node_modules", "target", "dist", ".venv", ".cache"];
/// How deep under `Work/` nested repositories are still discovered.
const DISCOVERY_MAX_DEPTH: usize = 3;

/// A branch/worktree ownership reference carried by a NOW clearing
/// (`central.now-clearing/v1` `work_refs`) or a project handoff
/// (`central.project-now.handoff/v1` `work_refs`). Defined with the
/// clearing schema and shared here.
pub use crate::continuous_work::placement::WorkRef;

/// branch -> now_ref, per repository relative path.
type NowClaims = BTreeMap<String, BTreeMap<String, String>>;

/// Repositories to census, each relative to the Central root.
pub struct CensusScope {
    pub repos: Vec<String>,
}

/// Discover git repositories: every directory under `Work/` down to
/// `DISCOVERY_MAX_DEPTH` carrying a `.git` entry, plus `Control` itself.
pub fn discover_repos(central: &Path) -> Vec<String> {
    let mut found = Vec::new();
    if has_git(&central.join("Control")) {
        found.push("Control".to_owned());
    }
    let work = central.join("Work");
    walk(&work, &work, 1, &mut found);
    found.sort();
    found.dedup();
    found
}

fn walk(work: &Path, dir: &Path, depth: usize, found: &mut Vec<String>) {
    if depth > DISCOVERY_MAX_DEPTH {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !file_type.is_dir() || DISCOVERY_SKIP.contains(&name.as_ref()) {
            continue;
        }
        let path = entry.path();
        if has_git(&path) {
            if let Ok(relative) = path.strip_prefix(work.parent().unwrap_or(work)) {
                found.push(relative.to_string_lossy().into_owned());
            }
        }
        walk(work, &path, depth + 1, found);
    }
}

fn has_git(dir: &Path) -> bool {
    dir.join(".git").exists()
}

/// Lane attribution from `.aikit/tasks/<name>` membership.
fn aikit_lane(path: &Path) -> Option<String> {
    let components: Vec<_> = path
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect();
    let index = components.iter().position(|c| c == ".aikit")?;
    if components.get(index + 1).map(String::as_str) == Some("tasks") {
        components
            .get(index + 2)
            .map(|task| format!("aikit-task:{task}"))
    } else {
        None
    }
}

/// Collect declared lane claims from every active root-register NOW clearing
/// and from the discovered projects' handoff records. Unparsable records are
/// skipped: attribution is declared evidence only, never guessed.
pub(crate) fn collect_now_claims(central: &Path, repos: &[String]) -> NowClaims {
    let mut claims = NowClaims::new();
    // Root-register clearings need ground relations; their absence only
    // means no clearing claims exist, not that the handoff scan should skip.
    if let Ok(scope) = crate::continuous_work::source::Scope::resolve(central, None) {
        let rows =
            crate::continuous_work::placement::list_now(&scope, &json!({})).unwrap_or_default();
        for row in rows {
            let Some(now_ref) = row.get("now_ref").and_then(Value::as_str) else {
                continue;
            };
            let active = row.get("lifecycle").and_then(Value::as_str) == Some("active");
            let Some(work_refs) = row.get("work_refs").and_then(Value::as_array) else {
                continue;
            };
            if !active {
                continue;
            }
            for reference in work_refs {
                if let Ok(work_ref) = serde_json::from_value::<WorkRef>(reference.clone()) {
                    claims
                        .entry(work_ref.repo)
                        .or_default()
                        .insert(work_ref.branch, now_ref.to_owned());
                }
            }
        }
    }
    for repo in repos {
        let project = repo.strip_prefix("Work/").unwrap_or(repo);
        let handoffs = central
            .join("Work")
            .join(project)
            .join("ProjectCentral/now/agents");
        let Ok(entries) = std::fs::read_dir(&handoffs) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let Ok(content) = std::fs::read_to_string(&path) else {
                continue;
            };
            let Ok(handoff) =
                serde_json::from_str::<crate::projectcentral_now::NowHandoff>(&content)
            else {
                continue;
            };
            if !matches!(handoff.status.as_str(), "active" | "waiting" | "carried") {
                continue;
            }
            for work_ref in &handoff.work_refs {
                claims
                    .entry(work_ref.repo.clone())
                    .or_default()
                    .insert(work_ref.branch.clone(), handoff.id.clone());
            }
        }
    }
    claims
}

fn lane_for(worktree_path: &Path, branch: Option<&str>, claims: &NowClaims, repo: &str) -> String {
    if let Some(task) = aikit_lane(worktree_path) {
        return task;
    }
    if let Some(branch) = branch {
        if let Some(claim) = claims.get(repo).and_then(|by_branch| by_branch.get(branch)) {
            return format!("now:{claim}");
        }
    }
    "unattributed".to_owned()
}

fn stale_days(last_commit_at: Option<&str>, now: DateTime<Utc>) -> Option<i64> {
    let committed = DateTime::parse_from_rfc3339(last_commit_at?).ok()?;
    Some((now.timestamp() - committed.timestamp()) / 86_400)
}

/// The rendered projections and the attention list over one assembled census.
pub struct CensusReadModel {
    pub document: Value,
    pub attention: Vec<Value>,
    pub summary: Value,
}

pub fn assemble(
    central: &Path,
    scope: CensusScope,
    provider: &dyn GitState,
    include_paths: bool,
    stale_days_limit: u32,
) -> io::Result<CensusReadModel> {
    let claims = collect_now_claims(central, &scope.repos);
    let now = Utc::now();
    let mut repos = Vec::new();
    let mut attention = Vec::new();
    for repo in &scope.repos {
        let absolute = central.join(repo);
        let request = GitCensusRequest {
            repo_root: absolute.clone(),
            include_paths,
            stale_days: stale_days_limit,
        };
        match provider.census(&request) {
            Ok(census) => {
                record_attention(
                    repo,
                    &census,
                    &claims,
                    stale_days_limit,
                    now,
                    &mut attention,
                );
                repos.push(render_repo(repo, census, &claims));
            }
            Err(error) => {
                repos.push(json!({
                    "repo": repo,
                    "error": error.to_string(),
                }));
            }
        }
    }
    let summary = json!({
        "repos_censused": repos.iter().filter(|r| r.get("error").is_none()).count(),
        "repos_errored": repos.iter().filter(|r| r.get("error").is_some()).count(),
        "worktrees": repos.iter().filter_map(|r| r.get("worktrees")?.as_array()).map(Vec::len).sum::<usize>(),
        "branches": repos.iter().filter_map(|r| r.get("branches")?.as_array()).map(Vec::len).sum::<usize>(),
        "local_only_branches": attention.iter().filter(|a| a["kind"] == "local_only_branch").count(),
        "unattributed_worktrees": attention.iter().filter(|a| a["kind"] == "unattributed_worktree").count(),
        "attention_items": attention.len(),
    });
    let document = json!({
        "schema": CENTRAL_GIT_CENSUS_SCHEMA,
        "generated_at": now.to_rfc3339(),
        "scope": scope.repos,
        "repos": repos,
        "attention": attention,
        "summary": summary,
    });
    // The document above carries everything; callers that want typed access
    // re-read it from `document` rather than holding a parallel copy.
    let attention = document["attention"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let summary = document["summary"].clone();
    Ok(CensusReadModel {
        document,
        attention,
        summary,
    })
}

fn record_attention(
    repo: &str,
    census: &GitRepoCensus,
    claims: &NowClaims,
    stale_days_limit: u32,
    now: DateTime<Utc>,
    attention: &mut Vec<Value>,
) {
    for worktree in &census.worktrees {
        if worktree.prunable {
            attention.push(json!({
                "kind": "prunable_worktree", "repo": repo,
                "path": worktree.path.to_string_lossy(),
            }));
        }
        if worktree.detached {
            attention.push(json!({
                "kind": "detached_head", "repo": repo,
                "path": worktree.path.to_string_lossy(),
            }));
        }
        let is_repo_root = worktree.path == census.repo_root;
        let unattributed =
            lane_for(&worktree.path, worktree.branch.as_deref(), claims, repo) == "unattributed";
        if !is_repo_root && unattributed {
            attention.push(json!({
                "kind": "unattributed_worktree", "repo": repo,
                "path": worktree.path.to_string_lossy(),
                "branch": worktree.branch,
            }));
        }
    }
    let checked_out: Vec<&str> = census
        .worktrees
        .iter()
        .filter_map(|worktree| worktree.branch.as_deref())
        .collect();
    for branch in &census.branches {
        if branch.local_only {
            attention.push(json!({
                "kind": "local_only_branch", "repo": repo,
                "branch": branch.name,
                "checked_out": !branch.checked_out_in.is_empty(),
            }));
        }
        let parked =
            branch.checked_out_in.is_empty() && !checked_out.contains(&branch.name.as_str());
        if parked && stale_days_limit > 0 {
            if let Some(days) = stale_days(branch.last_commit_at.as_deref(), now) {
                if days >= stale_days_limit as i64 {
                    attention.push(json!({
                        "kind": "stale_branch", "repo": repo,
                        "branch": branch.name, "days_idle": days,
                    }));
                }
            }
        }
    }
}

/// One repository entry: the port observation with `repo` and per-worktree
/// `lane` attribution folded in.
fn render_repo(repo: &str, census: GitRepoCensus, claims: &NowClaims) -> Value {
    let mut value = to_value(&census).expect("census serialises");
    let object = value.as_object_mut().expect("census is an object");
    object.insert("repo".to_owned(), json!(repo));
    if let Some(worktrees) = object.get_mut("worktrees").and_then(Value::as_array_mut) {
        for (worktree, observation) in worktrees.iter_mut().zip(&census.worktrees) {
            let lane = lane_for(
                &observation.path,
                observation.branch.as_deref(),
                claims,
                repo,
            );
            if let Some(entry) = worktree.as_object_mut() {
                entry.insert("lane".to_owned(), json!(lane));
            }
        }
    }
    value
}

fn render_list(document: &Value) -> String {
    let mut lines = Vec::new();
    for repo in document["repos"].as_array().unwrap_or(&vec![]) {
        let name = repo["repo"].as_str().unwrap_or("?");
        if let Some(error) = repo.get("error").and_then(Value::as_str) {
            lines.push(format!("{name}: error ({error})"));
            continue;
        }
        let worktrees = repo["worktrees"].as_array().map(Vec::len).unwrap_or(0);
        let branches = repo["branches"].as_array().map(Vec::len).unwrap_or(0);
        let local_only = repo["unmerged_tips"].as_array().map(Vec::len).unwrap_or(0);
        let dirty = repo["worktrees"]
            .as_array()
            .map(|trees| {
                trees
                    .iter()
                    .filter_map(|t| t["dirty_files"].as_u64())
                    .sum::<u64>()
            })
            .unwrap_or(0);
        lines.push(format!(
            "{name}: worktrees={worktrees} branches={branches} local_only={local_only} dirty={dirty}"
        ));
        for worktree in repo["worktrees"].as_array().unwrap_or(&vec![]) {
            lines.push(format!(
                "  wt {} branch={} ahead={} behind={} dirty={} lane={}",
                worktree["path"].as_str().unwrap_or("?"),
                worktree["branch"].as_str().unwrap_or("(detached)"),
                worktree["ahead"]
                    .as_u64()
                    .map(|v| v.to_string())
                    .unwrap_or("-".into()),
                worktree["behind"]
                    .as_u64()
                    .map(|v| v.to_string())
                    .unwrap_or("-".into()),
                worktree["dirty_files"]
                    .as_u64()
                    .map(|v| v.to_string())
                    .unwrap_or("-".into()),
                worktree["lane"].as_str().unwrap_or("?"),
            ));
        }
        for branch in repo["branches"].as_array().unwrap_or(&vec![]) {
            if branch["local_only"].as_bool() != Some(true) {
                continue;
            }
            lines.push(format!(
                "  br {} upstream={} ahead={} behind={} LOCAL-ONLY",
                branch["name"].as_str().unwrap_or("?"),
                branch["upstream"].as_str().unwrap_or("(none)"),
                branch["ahead"]
                    .as_u64()
                    .map(|v| v.to_string())
                    .unwrap_or("-".into()),
                branch["behind"]
                    .as_u64()
                    .map(|v| v.to_string())
                    .unwrap_or("-".into()),
            ));
        }
    }
    if let Some(attention) = document["attention"].as_array() {
        if !attention.is_empty() {
            lines.push("attention:".to_owned());
            for item in attention {
                lines.push(format!(
                    "  ! {} {}",
                    item["kind"].as_str().unwrap_or("?"),
                    item
                ));
            }
        }
    }
    lines.join("\n")
}

fn render_tree(document: &Value) -> String {
    let mut lines = vec![format!(
        "git census {}",
        document["generated_at"].as_str().unwrap_or("")
    )];
    for repo in document["repos"].as_array().unwrap_or(&vec![]) {
        let name = repo["repo"].as_str().unwrap_or("?");
        if repo.get("error").is_some() {
            lines.push(format!("  {name} (error)"));
            continue;
        }
        lines.push(format!(
            "  {name}  remote={} default={}",
            repo["remote"].as_str().unwrap_or("(none)"),
            repo["default_branch"].as_str().unwrap_or("?"),
        ));
        for worktree in repo["worktrees"].as_array().unwrap_or(&vec![]) {
            let path = worktree["path"].as_str().unwrap_or("?");
            let leaf = path.rsplit('/').next().unwrap_or(path);
            lines.push(format!(
                "    wt {leaf} @ {} ahead={} behind={} dirty={} lane={}",
                worktree["branch"].as_str().unwrap_or("(detached)"),
                worktree["ahead"]
                    .as_u64()
                    .map(|v| v.to_string())
                    .unwrap_or("-".into()),
                worktree["behind"]
                    .as_u64()
                    .map(|v| v.to_string())
                    .unwrap_or("-".into()),
                worktree["dirty_files"]
                    .as_u64()
                    .map(|v| v.to_string())
                    .unwrap_or("-".into()),
                worktree["lane"].as_str().unwrap_or("?"),
            ));
        }
        for branch in repo["branches"].as_array().unwrap_or(&vec![]) {
            if !branch["checked_out_in"]
                .as_array()
                .unwrap_or(&vec![])
                .is_empty()
            {
                continue;
            }
            let marker = if branch["local_only"].as_bool() == Some(true) {
                " LOCAL-ONLY"
            } else {
                ""
            };
            lines.push(format!(
                "    br {} upstream={}{}",
                branch["name"].as_str().unwrap_or("?"),
                branch["upstream"].as_str().unwrap_or("(none)"),
                marker,
            ));
        }
    }
    lines.join("\n")
}

fn mermaid_id(value: &str) -> String {
    value
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect()
}

fn render_graph(document: &Value) -> String {
    let mut lines = vec!["flowchart LR".to_owned()];
    for repo in document["repos"].as_array().unwrap_or(&vec![]) {
        let name = repo["repo"].as_str().unwrap_or("?");
        if repo.get("error").is_some() {
            continue;
        }
        let id = mermaid_id(name);
        lines.push(format!("  subgraph {id}[\"{name}\"]"));
        for (index, worktree) in repo["worktrees"]
            .as_array()
            .unwrap_or(&vec![])
            .iter()
            .enumerate()
        {
            let path = worktree["path"].as_str().unwrap_or("?");
            let leaf = path.rsplit('/').next().unwrap_or(path);
            let branch = worktree["branch"].as_str().unwrap_or("(detached)");
            let dirty = worktree["dirty_files"].as_u64().unwrap_or(0);
            lines.push(format!(
                "    {id}_wt{index}[\"wt {leaf} @ {branch} (dirty {dirty})\"]"
            ));
        }
        for (index, branch) in repo["branches"]
            .as_array()
            .unwrap_or(&vec![])
            .iter()
            .enumerate()
        {
            if branch["local_only"].as_bool() != Some(true) {
                continue;
            }
            let name_branch = branch["name"].as_str().unwrap_or("?");
            lines.push(format!(
                "    {id}_br{index}[\"br {name_branch} LOCAL-ONLY\"]"
            ));
        }
        lines.push("  end".to_owned());
        for (index, worktree) in repo["worktrees"]
            .as_array()
            .unwrap_or(&vec![])
            .iter()
            .enumerate()
        {
            let branch = worktree["branch"].as_str();
            if let Some(branch) = branch {
                if let Some((position, _)) = repo["branches"]
                    .as_array()
                    .unwrap_or(&vec![])
                    .iter()
                    .enumerate()
                    .find(|(_, candidate)| candidate["name"].as_str() == Some(branch))
                {
                    lines.push(format!(
                        "  {id}_wt{index} -->|checked out| {id}_br{position}"
                    ));
                }
            }
        }
        if let Some(remote) = repo["remote"].as_str() {
            let remote_id = format!("{id}_remote");
            lines.push(format!("  {remote_id}[\"origin {remote}\"]"));
            lines.push(format!("  {id} --> {remote_id}"));
        }
    }
    lines.join("\n")
}

pub fn render_format(format: &str, document: &Value) -> Option<String> {
    match format {
        "list" => Some(render_list(document)),
        "tree" => Some(render_tree(document)),
        "graph" => Some(render_graph(document)),
        _ => None,
    }
}

fn census_action(
    _: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let action = "central.git.census";
    let central = match resolve_central_root(context.root_options) {
        Ok(root) => root.path,
        Err(message) => {
            return ActionResult::failure(Some(action), ResultStatus::InvalidInput, message, None)
        }
    };
    let project = input
        .get("project")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let repos = match project {
        Some(project) => {
            let valid = Path::new(project).is_absolute()
                || !Path::new(project)
                    .components()
                    .all(|component| matches!(component, Component::Normal(_)));
            if valid {
                return ActionResult::failure(
                    Some(action),
                    ResultStatus::InvalidInput,
                    "project must be a single Work member name.",
                    None,
                );
            }
            let root = central.join("Work").join(project);
            if !has_git(&root) {
                return ActionResult::failure(
                    Some(action),
                    ResultStatus::InvalidInput,
                    format!("Not a git repository: {}", root.display()),
                    None,
                );
            }
            vec![format!("Work/{project}")]
        }
        None => discover_repos(&central),
    };
    let format = input
        .get("format")
        .and_then(Value::as_str)
        .unwrap_or("json");
    let include_paths = input
        .get("include_paths")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let stale_days = input
        .get("stale_days")
        .and_then(Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
        .unwrap_or(14);

    let resolution = context
        .connectors
        .resolve(&GIT_STATE_PORT, context.connector_context);
    let Some(connector) = resolution.connector else {
        let diagnostics = to_value(&resolution.diagnostics).expect("diagnostics serialise");
        return ActionResult::failure(
            Some(action),
            ResultStatus::UnavailableCapability,
            format!("No eligible Connector implements {}.", GIT_STATE_PORT.id),
            Some(json!({ "port": GIT_STATE_PORT.id, "diagnostics": diagnostics })),
        );
    };
    let Some(provider) = connector.git_state() else {
        return ActionResult::failure(
            Some(action),
            ResultStatus::UnavailableCapability,
            "Resolved GitState Connector did not expose its declared Port implementation.",
            None,
        );
    };

    let model = assemble(
        &central,
        CensusScope { repos },
        provider,
        include_paths,
        stale_days,
    );
    match model {
        Ok(model) => {
            if format == "json" {
                ActionResult::success(action, model.document)
            } else if let Some(render) = render_format(format, &model.document) {
                ActionResult::success(
                    action,
                    json!({
                        "schema": CENTRAL_GIT_CENSUS_SCHEMA,
                        "format": format,
                        "summary": model.summary,
                        "render": render,
                    }),
                )
            } else {
                ActionResult::failure(
                    Some(action),
                    ResultStatus::InvalidInput,
                    format!("Unknown census format: {format}; use json, list, tree or graph."),
                    None,
                )
            }
        }
        Err(error) => ActionResult::failure(
            Some(action),
            ResultStatus::InternalFailure,
            error.to_string(),
            None,
        ),
    }
}

fn text_input(name: &str, required: bool) -> ActionInputDefinition {
    ActionInputDefinition {
        name: name.to_owned(),
        input_type: "string".to_owned(),
        required,
        choices: None,
        selection: None,
    }
}

pub fn register_git_actions(registry: &mut ActionRegistry) {
    registry
        .register(
            ActionDescriptor {
                id: "central.git.census".to_owned(),
                title: "Census open branches and worktrees".to_owned(),
                description: "Read the observed branch and worktree state of every Work repository (or one project) with lane attribution and an attention list: local-only branches, unattributed or prunable worktrees, detached heads, stale branches. Strictly read-only; remote refs are those of the last fetch.".to_owned(),
                inputs: vec![
                    text_input("project", false),
                    text_input("format", false),
                    ActionInputDefinition {
                        name: "include_paths".to_owned(),
                        input_type: "boolean".to_owned(),
                        required: false,
                        choices: None,
                        selection: None,
                    },
                    ActionInputDefinition {
                        name: "stale_days".to_owned(),
                        input_type: "integer".to_owned(),
                        required: false,
                        choices: None,
                        selection: None,
                    },
                ],
                output: ActionOutputDefinition {
                    output_type: "central.git-census/v1".to_owned(),
                },
                mutation_class: MutationClass::ReadOnly,
                preview_supported: false,
                required_ports: vec![GIT_STATE_PORT.id.to_owned()],
                availability: ActionAvailability {
                    available: true,
                    reason: None,
                },
            },
            census_action,
        )
        .expect("git census Action id is valid");
}

#[cfg(test)]
mod tests {
    use super::*;
    use central_connector_sdk::{GitBranchObservation, GitWorktreeObservation, PortError};
    use std::path::PathBuf;

    struct FakeGitState {
        census: GitRepoCensus,
    }

    impl GitState for FakeGitState {
        fn census(&self, _input: &GitCensusRequest) -> Result<GitRepoCensus, PortError> {
            Ok(self.census.clone())
        }
    }

    fn fixture_census() -> GitRepoCensus {
        let root = PathBuf::from("/central/Work/O-I");
        GitRepoCensus {
            repo_root: root.clone(),
            unborn: false,
            bare: false,
            head_branch: Some("main".into()),
            remote: Some("git@example:O-I.git".into()),
            default_branch: Some("main".into()),
            worktrees: vec![
                GitWorktreeObservation {
                    path: root.clone(),
                    branch: Some("main".into()),
                    detached: false,
                    bare: false,
                    locked: false,
                    prunable: false,
                    dirty_files: Some(2),
                    ahead: Some(1),
                    behind: None,
                    last_commit_at: None,
                },
                GitWorktreeObservation {
                    path: root.join(".aikit/tasks/nara-walk"),
                    branch: Some("thread4/nara".into()),
                    detached: false,
                    bare: false,
                    locked: false,
                    prunable: false,
                    dirty_files: Some(0),
                    ahead: None,
                    behind: None,
                    last_commit_at: None,
                },
                GitWorktreeObservation {
                    path: root.join("mystery-lane"),
                    branch: Some("old-experiment".into()),
                    detached: false,
                    bare: false,
                    locked: false,
                    prunable: false,
                    dirty_files: Some(7),
                    ahead: None,
                    behind: None,
                    last_commit_at: None,
                },
            ],
            branches: vec![
                GitBranchObservation {
                    name: "main".into(),
                    upstream: Some("origin/main".into()),
                    ahead: Some(1),
                    behind: None,
                    last_commit_at: None,
                    checked_out_in: vec![root.clone()],
                    local_only: false,
                },
                GitBranchObservation {
                    name: "old-experiment".into(),
                    upstream: None,
                    ahead: None,
                    behind: None,
                    last_commit_at: None,
                    checked_out_in: vec![root.join("mystery-lane")],
                    local_only: true,
                },
                GitBranchObservation {
                    name: "parked".into(),
                    upstream: None,
                    ahead: None,
                    behind: None,
                    last_commit_at: None,
                    checked_out_in: vec![],
                    local_only: true,
                },
            ],
            unmerged_tips: vec!["old-experiment".into(), "parked".into()],
            dirty_paths: None,
        }
    }

    #[test]
    fn attributes_aikit_lanes_and_marks_the_rest_unattributed() {
        let provider = FakeGitState {
            census: fixture_census(),
        };
        let central = PathBuf::from("/central");
        let model = assemble(
            &central,
            CensusScope {
                repos: vec!["Work/O-I".into()],
            },
            &provider,
            false,
            14,
        )
        .unwrap();
        let repo = &model.document["repos"][0];
        let lanes: Vec<&str> = repo["worktrees"]
            .as_array()
            .unwrap()
            .iter()
            .map(|w| w["lane"].as_str().unwrap())
            .collect();
        assert_eq!(
            lanes,
            ["unattributed", "aikit-task:nara-walk", "unattributed"]
        );
        let kinds: Vec<&str> = model
            .attention
            .iter()
            .map(|item| item["kind"].as_str().unwrap())
            .collect();
        assert!(kinds.contains(&"unattributed_worktree"));
        assert_eq!(
            kinds.iter().filter(|k| **k == "local_only_branch").count(),
            2
        );
    }

    #[test]
    fn a_project_handoff_work_ref_attributes_its_lane() {
        let provider = FakeGitState {
            census: fixture_census(),
        };
        // The handoff record is a plain project file, so no ground relations
        // are needed to prove the attribution path.
        let central =
            std::env::temp_dir().join(format!("central-git-census-claims-{}", std::process::id()));
        let agents = central.join("Work/O-I/ProjectCentral/now/agents");
        std::fs::create_dir_all(&agents).unwrap();
        let handoff = json!({
            "schema": "central.project-now.handoff/v1",
            "id": "lane-claim-test",
            "provenance": "agent-authored-bounded-return",
            "actor": "test-session",
            "kind": "handoff",
            "recorded_at_unix_seconds": 1,
            "subject": "old experiment lane",
            "result": "recorded",
            "status": "active",
            "work_refs": [{"repo": "Work/O-I", "branch": "old-experiment"}]
        });
        std::fs::write(agents.join("lane-claim-test.json"), handoff.to_string()).unwrap();
        let model = assemble(
            &central,
            CensusScope {
                repos: vec!["Work/O-I".into()],
            },
            &provider,
            false,
            14,
        )
        .unwrap();
        let lanes: Vec<String> = model.document["repos"][0]["worktrees"]
            .as_array()
            .unwrap()
            .iter()
            .map(|w| w["lane"].as_str().unwrap().to_owned())
            .collect();
        assert!(lanes.iter().any(|lane| lane == "now:lane-claim-test"));
        assert!(!model
            .attention
            .iter()
            .any(|item| item["kind"] == "unattributed_worktree"));
        let _ = std::fs::remove_dir_all(&central);
    }

    #[test]
    fn renders_list_tree_and_graph() {
        let provider = FakeGitState {
            census: fixture_census(),
        };
        let central = PathBuf::from("/central");
        let model = assemble(
            &central,
            CensusScope {
                repos: vec!["Work/O-I".into()],
            },
            &provider,
            false,
            14,
        )
        .unwrap();
        let list = render_format("list", &model.document).unwrap();
        assert!(list.contains("Work/O-I: worktrees=3 branches=3 local_only=2"));
        assert!(list.contains("aikit-task:nara-walk"));
        let tree = render_format("tree", &model.document).unwrap();
        assert!(tree.contains("wt nara-walk @ thread4/nara"));
        assert!(tree.contains("br parked upstream=(none) LOCAL-ONLY"));
        let graph = render_format("graph", &model.document).unwrap();
        assert!(graph.starts_with("flowchart LR"));
        assert!(graph.contains("LOCAL-ONLY"));
        assert!(render_format("yaml", &model.document).is_none());
    }

    #[test]
    fn discovers_repos_under_work_and_control() {
        let central =
            std::env::temp_dir().join(format!("central-git-census-disc-{}", std::process::id()));
        for repo in [
            "Work/O-I",
            "Work/O-I/nested",
            "Work/ai-kit",
            "Control",
            "Work/deep/a/b",
            "Work/deep/a/b/c",
        ] {
            std::fs::create_dir_all(central.join(repo).join(".git")).unwrap();
        }
        std::fs::create_dir_all(central.join("Work/plain")).unwrap();
        let found = discover_repos(&central);
        // Work/deep/a/b is exactly at the discovery limit; Work/deep/a/b/c is
        // one below it and must not appear.
        assert_eq!(
            found,
            vec![
                "Control".to_owned(),
                "Work/O-I".to_owned(),
                "Work/O-I/nested".to_owned(),
                "Work/ai-kit".to_owned(),
                "Work/deep/a/b".to_owned(),
            ]
        );
        let _ = std::fs::remove_dir_all(&central);
    }
}
