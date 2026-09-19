//! The `GitState` port: observed branch and worktree state of one repository.
//!
//! Read-only by contract. Every fact here comes from a real `git` query the
//! way the last fetch left it; nothing in this module fetches, prunes, locks
//! or writes. Lane ownership is deliberately absent — attribution is Central's
//! read-model concern, not the provider's.

use central_connector_sdk::{
    GitBranchObservation, GitCensusRequest, GitRepoCensus, GitState, GitWorktreeObservation,
    PortError, PortErrorCode,
};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Changed paths reported per worktree before the list is truncated.
const DIRTY_PATH_CAP: usize = 500;

fn error(code: PortErrorCode, message: impl Into<String>, detail: impl Into<String>) -> PortError {
    let mut error = PortError::new(code, message);
    let detail = detail.into();
    if !detail.trim().is_empty() {
        error.provider_detail = Some(detail);
    }
    error
}

fn output_detail(output: &std::process::Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    if !stderr.is_empty() {
        return stderr;
    }
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if !stdout.is_empty() {
        return stdout;
    }
    format!("exit status: {}", output.status)
}

/// Run `git -C <repo> <args>` and return stdout. `git` failures become
/// provider errors carrying the command's own stderr.
fn git_out(git: &Path, repo: &Path, args: &[&str]) -> Result<String, PortError> {
    let output = Command::new(git)
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .map_err(|e| {
            let code = match e.kind() {
                std::io::ErrorKind::NotFound => PortErrorCode::MissingDependency,
                std::io::ErrorKind::PermissionDenied => PortErrorCode::PermissionFailure,
                _ => PortErrorCode::ProviderOperationFailed,
            };
            error(code, "Git census could not start git.", e.to_string())
        })?;
    if !output.status.success() {
        let detail = output_detail(&output);
        let code = if detail.contains("not a git repository") {
            PortErrorCode::InvalidConfiguration
        } else {
            PortErrorCode::ProviderOperationFailed
        };
        return Err(error(
            code,
            format!("Git census `git {}` failed.", args.join(" ")),
            detail,
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Run a git query whose failure is ordinary (absent config key, missing ref)
/// and yield stdout only on success.
fn git_optional(git: &Path, repo: &Path, args: &[&str]) -> Option<String> {
    Command::new(git)
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
        .filter(|value| !value.is_empty())
}

/// Parse `%(upstream:track,nobracket)`: `ahead 3`, `behind 2`,
/// `ahead 1, behind 2`, or `gone`.
fn parse_track(track: &str) -> (Option<u64>, Option<u64>, bool) {
    let mut ahead = None;
    let mut behind = None;
    let mut gone = false;
    for part in track.split(',') {
        let part = part.trim();
        if let Some(value) = part.strip_prefix("ahead ") {
            ahead = value.parse().ok();
        } else if let Some(value) = part.strip_prefix("behind ") {
            behind = value.parse().ok();
        } else if part == "gone" {
            gone = true;
        }
    }
    (ahead, behind, gone)
}

struct BranchRow {
    name: String,
    tip: String,
    upstream: Option<String>,
    ahead: Option<u64>,
    behind: Option<u64>,
    last_commit_at: Option<String>,
}

/// One for-each-ref record, fields separated by NUL:
/// objectname, refname, upstream, track, committerdate.
fn parse_branch_row(line: &str) -> Option<BranchRow> {
    let fields: Vec<&str> = line.split('\0').collect();
    if fields.len() < 5 {
        return None;
    }
    let name = fields[1].strip_prefix("refs/heads/")?.to_owned();
    let (ahead, behind, _gone) = parse_track(fields[3]);
    Some(BranchRow {
        name,
        tip: fields[0].to_owned(),
        upstream: fields[2].strip_prefix("refs/remotes/").map(str::to_owned),
        ahead,
        behind,
        last_commit_at: if fields[4].is_empty() {
            None
        } else {
            Some(fields[4].to_owned())
        },
    })
}

/// Parse `git worktree list --porcelain`: records separated by blank lines
/// with `worktree`, `HEAD`, `branch`, `bare`, `detached`, `locked`, `prunable`
/// fields.
fn parse_worktree_block(block: &str) -> Option<(PathBuf, Option<String>, bool, bool, bool, bool)> {
    let mut path = None;
    let mut branch = None;
    let mut detached = false;
    let mut bare = false;
    let mut locked = false;
    let mut prunable = false;
    for line in block.lines() {
        if let Some(value) = line.strip_prefix("worktree ") {
            path = Some(PathBuf::from(value));
        } else if let Some(value) = line.strip_prefix("branch ") {
            branch = value.strip_prefix("refs/heads/").map(str::to_owned);
        } else if line == "detached" {
            detached = true;
        } else if line == "bare" {
            bare = true;
        } else if line.starts_with("locked") {
            locked = true;
        } else if line.starts_with("prunable") {
            prunable = true;
        }
    }
    Some((path?, branch, detached, bare, locked, prunable))
}

pub(crate) fn census(git: &Path, input: &GitCensusRequest) -> Result<GitRepoCensus, PortError> {
    let repo = input.repo_root.as_path();
    // Identity checks first: an absent repository is a configuration error;
    // an existing one without commits is a valid, empty census.
    let git_dir = git_optional(git, repo, &["rev-parse", "--git-dir"]);
    let Some(_) = git_dir else {
        return Err(error(
            PortErrorCode::InvalidConfiguration,
            format!(
                "Git census target is not a Git repository: {}",
                repo.display()
            ),
            "`git rev-parse --git-dir` failed",
        ));
    };
    let unborn = git_optional(git, repo, &["rev-parse", "--verify", "HEAD"]).is_none();
    let bare = git_optional(git, repo, &["rev-parse", "--is-bare-repository"])
        .is_some_and(|value| value == "true");

    let head_branch = git_optional(git, repo, &["rev-parse", "--abbrev-ref", "HEAD"])
        .filter(|value| value != "HEAD");
    let remote = git_optional(git, repo, &["config", "--get", "remote.origin.url"]);
    let default_branch = git_optional(
        git,
        repo,
        &["symbolic-ref", "--short", "refs/remotes/origin/HEAD"],
    )
    .map(|value| value.strip_prefix("origin/").unwrap_or(&value).to_owned())
    .or_else(|| {
        head_branch
            .as_deref()
            .filter(|name| *name == "main" || *name == "master")
            .map(str::to_owned)
    });

    // Branches: one for-each-ref call carries tip, upstream, track and date.
    let branch_output = git_out(
        git,
        repo,
        &[
            "for-each-ref",
            "refs/heads",
            "--format=%(objectname)%00%(refname)%00%(upstream)%00%(upstream:track,nobracket)%00%(committerdate:iso-strict)",
        ],
    )?;
    let mut branches: Vec<BranchRow> = branch_output
        .lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(parse_branch_row)
        .collect();
    branches.sort_by(|left, right| left.name.cmp(&right.name));

    // Local-only tips: one exact set-algebra call over the last-fetched refs.
    let local_tip_output = if unborn {
        String::new()
    } else {
        git_out(
            git,
            repo,
            &["rev-list", "--branches", "--not", "--remotes", "--no-walk"],
        )
        .unwrap_or_default()
    };
    let local_tips: std::collections::BTreeSet<&str> = local_tip_output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();

    // Worktrees: porcelain listing is the register of record.
    let worktree_output = git_out(git, repo, &["worktree", "list", "--porcelain"])?;
    let mut worktrees = Vec::new();
    let mut checked_out: BTreeMap<String, Vec<PathBuf>> = BTreeMap::new();
    for block in worktree_output.split("\n\n") {
        if block.trim().is_empty() {
            continue;
        }
        let Some((path, branch, detached, bare, locked, prunable)) = parse_worktree_block(block)
        else {
            continue;
        };
        if let Some(branch) = &branch {
            checked_out
                .entry(branch.clone())
                .or_default()
                .push(path.clone());
        }
        let directory_exists = path.is_dir();
        let dirty_files = if directory_exists && !bare {
            git_optional(git, &path, &["status", "--porcelain"])
                .map(|status| status.lines().count() as u64)
        } else {
            None
        };
        let last_commit_at = if unborn || prunable || !directory_exists {
            None
        } else if let Some(branch) = &branch {
            branches
                .iter()
                .find(|row| &row.name == branch)
                .and_then(|row| row.last_commit_at.clone())
        } else {
            git_optional(git, &path, &["log", "-1", "--format=%cI"])
        };
        let (mut ahead, mut behind) = (None, None);
        if let Some(branch) = &branch {
            if let Some(row) = branches.iter().find(|row| &row.name == branch) {
                ahead = row.ahead;
                behind = row.behind;
            }
        }
        worktrees.push(GitWorktreeObservation {
            path,
            branch,
            detached,
            bare,
            locked,
            prunable: prunable || !directory_exists,
            dirty_files,
            ahead,
            behind,
            last_commit_at,
        });
    }
    worktrees.sort_by(|left, right| left.path.cmp(&right.path));

    let branch_observations: Vec<GitBranchObservation> = branches
        .iter()
        .map(|row| GitBranchObservation {
            name: row.name.clone(),
            upstream: row.upstream.clone(),
            ahead: row.ahead,
            behind: row.behind,
            last_commit_at: row.last_commit_at.clone(),
            checked_out_in: checked_out.get(&row.name).cloned().unwrap_or_default(),
            local_only: local_tips.contains(row.tip.as_str()),
        })
        .collect();
    let unmerged_tips: Vec<String> = branch_observations
        .iter()
        .filter(|branch| branch.local_only)
        .map(|branch| branch.name.clone())
        .collect();

    let dirty_paths = if input.include_paths && !unborn {
        // Changed paths are a main-worktree concern; run status directly on
        // the requested repository root.
        git_optional(git, repo, &["status", "--porcelain"]).map(|status| {
            let paths: Vec<String> = status
                .lines()
                .map(|line| line.get(3..).unwrap_or(line).to_owned())
                .collect();
            if paths.len() > DIRTY_PATH_CAP {
                paths.into_iter().take(DIRTY_PATH_CAP).collect()
            } else {
                paths
            }
        })
    } else {
        None
    };

    Ok(GitRepoCensus {
        repo_root: repo.to_path_buf(),
        unborn,
        bare,
        head_branch,
        remote,
        default_branch,
        worktrees,
        branches: branch_observations,
        unmerged_tips,
        dirty_paths,
    })
}

impl GitState for super::GitSynchronizerConnector {
    fn census(&self, input: &GitCensusRequest) -> Result<GitRepoCensus, PortError> {
        census(self.git_path(), input)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::process::Command as ProcessCommand;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_root(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "central-git-state-{label}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn git(repo: &Path, args: &[&str]) {
        let output = ProcessCommand::new("git")
            .arg("-C")
            .arg(repo)
            .args(args)
            .output()
            .unwrap_or_else(|error| panic!("git {args:?} could not start: {error}"));
        assert!(
            output.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn commit_all(repo: &Path, message: &str) {
        git(repo, &["add", "-A"]);
        git(repo, &["commit", "--allow-empty", "-m", message]);
    }

    #[test]
    fn refuses_a_directory_that_is_not_a_repository() {
        let root = temp_root("not-repo");
        let result = census(
            Path::new("git"),
            &GitCensusRequest {
                repo_root: root.clone(),
                include_paths: false,
                stale_days: 14,
            },
        )
        .unwrap_err();
        assert_eq!(result.code, PortErrorCode::InvalidConfiguration);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn observes_worktrees_branches_and_local_only_tips() {
        let root = temp_root("repo");
        let repo = root.join("repo");
        fs::create_dir_all(&repo).unwrap();
        git(&repo, &["init", "--initial-branch=main"]);
        git(&repo, &["config", "user.email", "census@example.invalid"]);
        git(&repo, &["config", "user.name", "Census"]);
        commit_all(&repo, "initial");
        git(&repo, &["branch", "parked"]);
        git(&repo, &["worktree", "add", "-b", "lane/one", "../lane-one"]);
        let lane = root.join("lane-one");
        // A local commit on the lane that no remote has ever seen.
        fs::write(lane.join("note.txt"), "x").unwrap();
        git(&lane, &["add", "-A"]);
        git(&lane, &["commit", "-m", "lane work"]);
        // Dirt in the main worktree.
        fs::write(repo.join("dirty.txt"), "uncommitted").unwrap();
        // git canonicalizes paths (macOS /var -> /private/var), so compare
        // canonical forms.
        let repo = fs::canonicalize(&repo).unwrap();
        let lane = fs::canonicalize(&lane).unwrap();

        let result = census(
            Path::new("git"),
            &GitCensusRequest {
                repo_root: repo.clone(),
                include_paths: true,
                stale_days: 14,
            },
        )
        .unwrap();

        assert!(!result.unborn);
        assert_eq!(result.head_branch.as_deref(), Some("main"));
        assert_eq!(result.default_branch.as_deref(), Some("main"));
        assert_eq!(result.worktrees.len(), 2);

        let main = result
            .worktrees
            .iter()
            .find(|worktree| worktree.path == repo)
            .unwrap();
        assert_eq!(main.branch.as_deref(), Some("main"));
        assert_eq!(main.dirty_files, Some(1));

        let lane_tree = result
            .worktrees
            .iter()
            .find(|worktree| worktree.path == lane)
            .unwrap();
        assert_eq!(lane_tree.branch.as_deref(), Some("lane/one"));

        let parked = result
            .branches
            .iter()
            .find(|branch| branch.name == "parked")
            .unwrap();
        assert!(
            parked.local_only,
            "no remote exists, so every tip is local-only"
        );
        assert!(parked.checked_out_in.is_empty());

        let lane_branch = result
            .branches
            .iter()
            .find(|branch| branch.name == "lane/one")
            .unwrap();
        assert!(lane_branch.local_only);
        assert_eq!(lane_branch.checked_out_in, vec![lane.clone()]);

        assert!(result.unmerged_tips.contains(&"lane/one".to_owned()));
        assert_eq!(
            result.dirty_paths.as_ref().map(|paths| paths.as_slice()),
            Some(&["dirty.txt".to_owned()][..])
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn reports_an_unborn_repository_as_empty_not_failed() {
        let root = temp_root("unborn");
        let repo = root.join("fresh");
        fs::create_dir_all(&repo).unwrap();
        git(&repo, &["init", "--initial-branch=main"]);
        let result = census(
            Path::new("git"),
            &GitCensusRequest {
                repo_root: repo.clone(),
                include_paths: false,
                stale_days: 14,
            },
        )
        .unwrap();
        assert!(result.unborn);
        assert!(result.unmerged_tips.is_empty());
        assert_eq!(result.worktrees.len(), 1);
        let _ = fs::remove_dir_all(root);
    }
}
