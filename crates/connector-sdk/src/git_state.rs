use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::{PortContract, PortError, PortOperationContract};

pub const GIT_STATE_OPERATIONS: [PortOperationContract; 1] = [PortOperationContract {
    name: "census",
    input_type: "GitCensusRequest",
    output_type: "GitRepoCensus",
    mutation_class: "read-only",
    preview_required: false,
    idempotent: true,
}];

pub const GIT_STATE_PORT: PortContract = PortContract {
    id: "GitState",
    version: "1.0.0",
    purpose: "Read the observed branch and worktree state of one Git repository without fetching, pruning or otherwise changing any Git state.",
    operations: &GIT_STATE_OPERATIONS,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitCensusRequest {
    pub repo_root: PathBuf,
    #[serde(default)]
    pub include_paths: bool,
    /// Branches with no commit newer than this many days and no worktree are
    /// reported stale. Zero disables staleness (reported as `stale_days: 0`).
    #[serde(default)]
    pub stale_days: u32,
}

/// One observed worktree of the repository. Every field is what real `git`
/// reported; the provider never infers ownership or lane identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitWorktreeObservation {
    pub path: PathBuf,
    /// The checked-out branch, absent when detached or bare.
    #[serde(default)]
    pub branch: Option<String>,
    #[serde(default)]
    pub detached: bool,
    #[serde(default)]
    pub bare: bool,
    #[serde(default)]
    pub locked: bool,
    /// The registered worktree directory is missing or stale on disk.
    #[serde(default)]
    pub prunable: bool,
    /// Changed-file count from `git status --porcelain`; absent when the
    /// worktree directory does not exist (prunable) or is bare.
    #[serde(default)]
    pub dirty_files: Option<u64>,
    #[serde(default)]
    pub ahead: Option<u64>,
    #[serde(default)]
    pub behind: Option<u64>,
    /// Final-commit time of the checked-out branch (ISO-8601), absent when
    /// unborn, detached on an unreachable commit, or prunable.
    #[serde(default)]
    pub last_commit_at: Option<String>,
}

/// One observed local branch of the repository.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitBranchObservation {
    pub name: String,
    /// Full short upstream name (e.g. `origin/main`), absent when untracked.
    #[serde(default)]
    pub upstream: Option<String>,
    #[serde(default)]
    pub ahead: Option<u64>,
    #[serde(default)]
    pub behind: Option<u64>,
    #[serde(default)]
    pub last_commit_at: Option<String>,
    /// Worktree paths with this branch checked out (may be empty).
    #[serde(default)]
    pub checked_out_in: Vec<PathBuf>,
    /// The branch tip is contained in no remote ref under the last-fetched
    /// remote refs — work that exists only on this machine.
    #[serde(default)]
    pub local_only: bool,
}

/// The observed state of one repository. Read-only by contract: an
/// implementation must not fetch, prune, lock, or otherwise mutate Git state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitRepoCensus {
    pub repo_root: PathBuf,
    /// The repository has no commits yet; branch/worktree observations may
    /// still be present but are mostly empty.
    #[serde(default)]
    pub unborn: bool,
    #[serde(default)]
    pub bare: bool,
    #[serde(default)]
    pub head_branch: Option<String>,
    #[serde(default)]
    pub remote: Option<String>,
    #[serde(default)]
    pub default_branch: Option<String>,
    pub worktrees: Vec<GitWorktreeObservation>,
    pub branches: Vec<GitBranchObservation>,
    /// Names of branches whose tip is reachable from no remote ref.
    #[serde(default)]
    pub unmerged_tips: Vec<String>,
    /// Changed paths when the request asked for them; absent otherwise.
    #[serde(default)]
    pub dirty_paths: Option<Vec<String>>,
}

pub trait GitState: Send + Sync {
    fn census(&self, input: &GitCensusRequest) -> Result<GitRepoCensus, PortError>;
}
