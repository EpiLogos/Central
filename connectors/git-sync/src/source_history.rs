use central_connector_sdk::{
    PortError, PortErrorCode, SourceCompareOutput, SourceCompareRequest, SourceHistory,
    SourceHistoryEntry, SourceHistoryOutput, SourceHistoryRequest, SourceRevisionReadOutput,
    SourceRevisionReadRequest,
};
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Output};

use super::{GitSynchronizerConnector, GIT_SYNCHRONIZER_CONNECTOR_ID};

impl SourceHistory for GitSynchronizerConnector {
    fn history(&self, input: &SourceHistoryRequest) -> Result<SourceHistoryOutput, PortError> {
        if input.limit == 0 {
            return Ok(SourceHistoryOutput {
                provider: GIT_SYNCHRONIZER_CONNECTOR_ID.to_owned(),
                source_path: input.source_path.clone(),
                entries: Vec::new(),
            });
        }
        let repo = resolve_repo_path(self, &input.world_root, &input.source_path)?;
        let output = checked(
            self,
            &repo.repository_root,
            vec![
                "log".to_owned(),
                format!("-n{}", input.limit),
                "--format=%H%x1f%P%x1f%s%x1f%an%x1f%aI%x1e".to_owned(),
                "--".to_owned(),
                repo.repo_relative.clone(),
            ],
            "source history",
        )?;
        Ok(SourceHistoryOutput {
            provider: GIT_SYNCHRONIZER_CONNECTOR_ID.to_owned(),
            source_path: input.source_path.clone(),
            entries: parse_history(&output),
        })
    }

    fn compare(&self, input: &SourceCompareRequest) -> Result<SourceCompareOutput, PortError> {
        require_revision(&input.from_revision, "from_revision")?;
        require_revision(&input.to_revision, "to_revision")?;
        let repo = resolve_repo_path(self, &input.world_root, &input.source_path)?;
        let output = self.command_output(
            Command::new(&self.git)
                .arg("-C")
                .arg(&repo.repository_root)
                .args(["diff", "--no-ext-diff", "--binary"])
                .arg(&input.from_revision)
                .arg(&input.to_revision)
                .arg("--")
                .arg(&repo.repo_relative),
            "source comparison",
        )?;
        if !output.status.success() {
            return Err(git_error("Git source comparison failed.", output));
        }
        let max = input.max_bytes.max(1);
        let truncated = output.stdout.len() > max;
        let patch = String::from_utf8_lossy(&output.stdout[..output.stdout.len().min(max)]).to_string();
        Ok(SourceCompareOutput {
            provider: GIT_SYNCHRONIZER_CONNECTOR_ID.to_owned(),
            source_path: input.source_path.clone(),
            from_revision: input.from_revision.clone(),
            to_revision: input.to_revision.clone(),
            patch,
            truncated,
        })
    }

    fn read_revision(
        &self,
        input: &SourceRevisionReadRequest,
    ) -> Result<SourceRevisionReadOutput, PortError> {
        require_revision(&input.revision, "revision")?;
        let repo = resolve_repo_path(self, &input.world_root, &input.source_path)?;
        let object = format!("{}:{}", input.revision, repo.repo_relative);
        let output = self.command_output(
            Command::new(&self.git)
                .arg("-C")
                .arg(&repo.repository_root)
                .args(["show", "--no-textconv", "--format="])
                .arg(object),
            "historical source read",
        )?;
        if !output.status.success() {
            return Err(git_error("Git historical source revision could not be read.", output));
        }
        let max = input.max_bytes.max(1);
        let truncated = output.stdout.len() > max;
        Ok(SourceRevisionReadOutput {
            provider: GIT_SYNCHRONIZER_CONNECTOR_ID.to_owned(),
            source_path: input.source_path.clone(),
            revision: input.revision.clone(),
            content: output.stdout[..output.stdout.len().min(max)].to_vec(),
            truncated,
        })
    }
}

struct RepoPath {
    repository_root: PathBuf,
    repo_relative: String,
}

fn resolve_repo_path(
    provider: &GitSynchronizerConnector,
    world_root: &Path,
    source_path: &Path,
) -> Result<RepoPath, PortError> {
    if !world_root.is_dir() {
        return Err(PortError::new(
            PortErrorCode::InvalidInput,
            format!("Source history world root does not exist: {}", world_root.display()),
        ));
    }
    if source_path.as_os_str().is_empty()
        || source_path.is_absolute()
        || !source_path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
    {
        return Err(PortError::new(
            PortErrorCode::InvalidInput,
            "Source history path must be a non-empty relative path inside the supplied world root.",
        ));
    }
    let repository_root = checked(
        provider,
        world_root,
        vec!["rev-parse".to_owned(), "--show-toplevel".to_owned()],
        "repository inspection",
    )?;
    let repository_root = PathBuf::from(repository_root.trim());
    let canonical_repo = repository_root.canonicalize().map_err(|error| {
        PortError::new(
            PortErrorCode::ProviderOperationFailed,
            format!("Git repository root could not be canonicalized: {error}"),
        )
    })?;
    let canonical_world = world_root.canonicalize().map_err(|error| {
        PortError::new(
            PortErrorCode::InvalidInput,
            format!("Source history world root could not be canonicalized: {error}"),
        )
    })?;
    if !canonical_world.starts_with(&canonical_repo) {
        return Err(PortError::new(
            PortErrorCode::InvalidInput,
            "Source history world root is not inside the resolved Git repository.",
        ));
    }
    let absolute_source = canonical_world.join(source_path);
    let repo_relative = absolute_source.strip_prefix(&canonical_repo).map_err(|_| {
        PortError::new(
            PortErrorCode::InvalidInput,
            "Source history path escaped the resolved Git repository.",
        )
    })?;
    let repo_relative = normalize(repo_relative)?;
    Ok(RepoPath {
        repository_root: canonical_repo,
        repo_relative,
    })
}

fn normalize(path: &Path) -> Result<String, PortError> {
    let mut parts = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(value) => parts.push(value.to_string_lossy().to_string()),
            _ => {
                return Err(PortError::new(
                    PortErrorCode::InvalidInput,
                    "Source history path contains a non-normal component.",
                ))
            }
        }
    }
    if parts.is_empty() {
        return Err(PortError::new(
            PortErrorCode::InvalidInput,
            "Source history path resolved to the repository root rather than a source.",
        ));
    }
    Ok(parts.join("/"))
}

fn checked(
    provider: &GitSynchronizerConnector,
    cwd: &Path,
    args: Vec<String>,
    operation: &str,
) -> Result<String, PortError> {
    let output = provider.command_output(
        Command::new(&provider.git).arg("-C").arg(cwd).args(args),
        operation,
    )?;
    if !output.status.success() {
        return Err(git_error(format!("Git {operation} failed."), output));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn require_revision(value: &str, field: &str) -> Result<(), PortError> {
    if value.trim().is_empty() || value != value.trim() || value.starts_with('-') {
        return Err(PortError::new(
            PortErrorCode::InvalidInput,
            format!("{field} must be a non-empty Git revision expression and may not begin with '-'."),
        ));
    }
    Ok(())
}

fn git_error(message: impl Into<String>, output: Output) -> PortError {
    let detail = GitSynchronizerConnector::output_detail(&output);
    GitSynchronizerConnector::error(PortErrorCode::ProviderOperationFailed, message, detail)
}

fn parse_history(raw: &str) -> Vec<SourceHistoryEntry> {
    raw.split('\u{1e}')
        .filter_map(|record| {
            let record = record.trim();
            if record.is_empty() {
                return None;
            }
            let mut fields = record.split('\u{1f}');
            let revision = fields.next()?.to_owned();
            let parents = fields
                .next()
                .unwrap_or_default()
                .split_whitespace()
                .map(str::to_owned)
                .collect();
            let subject = fields.next().unwrap_or_default().to_owned();
            let author = fields.next().filter(|value| !value.is_empty()).map(str::to_owned);
            let authored_at = fields.next().filter(|value| !value.is_empty()).map(str::to_owned);
            Some(SourceHistoryEntry {
                revision,
                parents,
                subject,
                author,
                authored_at,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn git_source_history_is_bounded_and_source_scoped() {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let root = std::env::temp_dir().join(format!("central-git-history-{}-{nonce}", std::process::id()));
        fs::create_dir_all(root.join("ProjectCentral/user")).unwrap();
        run(&root, ["init", "-q"]);
        run(&root, ["config", "user.name", "Central Test"]);
        run(&root, ["config", "user.email", "central@example.invalid"]);
        let source = root.join("ProjectCentral/user/intent.md");
        fs::write(&source, "one\n").unwrap();
        run(&root, ["add", "."]);
        run(&root, ["commit", "-qm", "one"]);
        let first = rev(&root);
        fs::write(&source, "two\n").unwrap();
        run(&root, ["add", "."]);
        run(&root, ["commit", "-qm", "two"]);
        let second = rev(&root);

        let provider = GitSynchronizerConnector::with_paths(PathBuf::from("git"), root.clone());
        let history = provider.history(&SourceHistoryRequest {
            world_root: root.clone(),
            source_path: PathBuf::from("ProjectCentral/user/intent.md"),
            limit: 8,
        }).unwrap();
        assert_eq!(history.entries.len(), 2);
        assert_eq!(history.entries[0].revision, second);

        let compared = provider.compare(&SourceCompareRequest {
            world_root: root.clone(),
            source_path: PathBuf::from("ProjectCentral/user/intent.md"),
            from_revision: first.clone(),
            to_revision: second.clone(),
            max_bytes: 4096,
        }).unwrap();
        assert!(compared.patch.contains("-one"));
        assert!(compared.patch.contains("+two"));
        assert!(!compared.truncated);

        let prior = provider.read_revision(&SourceRevisionReadRequest {
            world_root: root.clone(),
            source_path: PathBuf::from("ProjectCentral/user/intent.md"),
            revision: first,
            max_bytes: 1024,
        }).unwrap();
        assert_eq!(prior.content, b"one\n");
        assert!(!prior.truncated);

        let _ = fs::remove_dir_all(root);
    }

    fn run<const N: usize>(cwd: &Path, args: [&str; N]) {
        let status = Command::new("git").arg("-C").arg(cwd).args(args).status().unwrap();
        assert!(status.success());
    }

    fn rev(cwd: &Path) -> String {
        String::from_utf8(
            Command::new("git").arg("-C").arg(cwd).args(["rev-parse", "HEAD"]).output().unwrap().stdout,
        ).unwrap().trim().to_owned()
    }
}
