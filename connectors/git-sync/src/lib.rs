use central_connector_sdk::{
    CapabilityProbe, Connector, ConnectorContext, ConnectorManifest, ConnectorPortDeclaration,
    PortContract, PortError, PortErrorCode, StateChangePreview, StateChangeResult,
    SynchronizationRequest, Synchronizer, CONNECTOR_API_VERSION, SYNCHRONIZER_PORT,
};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

pub const GIT_SYNCHRONIZER_CONNECTOR_ID: &str = "personal.git-sync";
const SOURCE_KIND: &str = "git";
const TARGET_ENV: &str = "CENTRAL_GIT_SYNC_TARGET";
const GIT_ENV: &str = "CENTRAL_GIT_EXECUTABLE";

pub struct GitSynchronizerConnector {
    manifest: ConnectorManifest,
    git: PathBuf,
    target: PathBuf,
}

impl GitSynchronizerConnector {
    pub fn new() -> Self {
        let git = std::env::var_os(GIT_ENV)
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("git"));
        let target = std::env::var_os(TARGET_ENV)
            .map(PathBuf::from)
            .unwrap_or_default();
        Self::with_paths(git, target)
    }

    pub fn with_paths(git: PathBuf, target: PathBuf) -> Self {
        Self {
            manifest: ConnectorManifest {
                api_version: CONNECTOR_API_VERSION.to_owned(),
                id: GIT_SYNCHRONIZER_CONNECTOR_ID.to_owned(),
                version: "0.1.0".to_owned(),
                display_name: "Git fast-forward synchronization".to_owned(),
                ports: vec![ConnectorPortDeclaration {
                    id: SYNCHRONIZER_PORT.id.to_owned(),
                    version: SYNCHRONIZER_PORT.version.to_owned(),
                }],
                platforms: vec!["*".to_owned()],
                entrypoint: "rust:central-git-sync-connector::GitSynchronizerConnector".to_owned(),
                runtime_requirements: vec!["git".to_owned()],
                dependency_probes: vec!["git --version".to_owned()],
                configuration_requirements: vec![format!(
                    "{TARGET_ENV} must name an existing Git working tree to synchronize."
                )],
                mutation_scope: "externally-mutating".to_owned(),
            },
            git,
            target,
        }
    }

    pub fn target(&self) -> &Path {
        &self.target
    }

    fn error(
        code: PortErrorCode,
        message: impl Into<String>,
        detail: impl Into<String>,
    ) -> PortError {
        let mut error = PortError::new(code, message);
        let detail = detail.into();
        if !detail.trim().is_empty() {
            error.provider_detail = Some(detail);
        }
        error
    }

    fn output_detail(output: &Output) -> String {
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

    fn command_output(&self, command: &mut Command, operation: &str) -> Result<Output, PortError> {
        command.output().map_err(|error| {
            let code = match error.kind() {
                std::io::ErrorKind::NotFound => PortErrorCode::MissingDependency,
                std::io::ErrorKind::PermissionDenied => PortErrorCode::PermissionFailure,
                _ => PortErrorCode::ProviderOperationFailed,
            };
            Self::error(
                code,
                format!("Git {operation} could not start."),
                error.to_string(),
            )
        })
    }

    fn git_available(&self) -> bool {
        Command::new(&self.git)
            .arg("--version")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    fn target_is_worktree(&self) -> bool {
        if self.target.as_os_str().is_empty() || !self.target.is_dir() {
            return false;
        }
        Command::new(&self.git)
            .arg("-C")
            .arg(&self.target)
            .args(["rev-parse", "--is-inside-work-tree"])
            .output()
            .map(|output| {
                output.status.success()
                    && String::from_utf8_lossy(&output.stdout).trim() == "true"
            })
            .unwrap_or(false)
    }

    fn require_target(&self) -> Result<(), PortError> {
        if self.target.as_os_str().is_empty() {
            return Err(PortError::new(
                PortErrorCode::InvalidConfiguration,
                format!("{TARGET_ENV} is required for Git synchronization."),
            ));
        }
        if !self.target.is_dir() {
            return Err(PortError::new(
                PortErrorCode::InvalidConfiguration,
                format!(
                    "Configured Git synchronization target does not exist: {}",
                    self.target.display()
                ),
            ));
        }
        if !self.target_is_worktree() {
            return Err(PortError::new(
                PortErrorCode::InvalidConfiguration,
                format!(
                    "Configured Git synchronization target is not a Git working tree: {}",
                    self.target.display()
                ),
            ));
        }
        Ok(())
    }

    fn source_reference<'a>(
        &self,
        input: &'a SynchronizationRequest,
    ) -> Result<&'a str, PortError> {
        if input.id.trim().is_empty() {
            return Err(PortError::new(
                PortErrorCode::InvalidInput,
                "Synchronization id must be non-empty.",
            ));
        }
        let source = input.source.as_ref().ok_or_else(|| {
            PortError::new(
                PortErrorCode::InvalidConfiguration,
                "Git synchronization requires an authored source reference.",
            )
        })?;
        if source.kind != SOURCE_KIND {
            return Err(PortError::new(
                PortErrorCode::InvalidConfiguration,
                format!(
                    "Git Synchronizer requires source.kind='{SOURCE_KIND}', not '{}'.",
                    source.kind
                ),
            ));
        }
        let reference = source.reference.trim();
        if reference.is_empty() {
            return Err(PortError::new(
                PortErrorCode::InvalidConfiguration,
                "Git synchronization source reference must be non-empty.",
            ));
        }
        Ok(reference)
    }

    fn local_head(&self) -> Result<String, PortError> {
        self.require_target()?;
        let output = self.command_output(
            Command::new(&self.git)
                .arg("-C")
                .arg(&self.target)
                .args(["rev-parse", "HEAD"]),
            "local revision inspection",
        )?;
        if !output.status.success() {
            return Err(Self::error(
                PortErrorCode::InvalidConfiguration,
                "Git synchronization target has no readable HEAD revision.",
                Self::output_detail(&output),
            ));
        }
        let revision = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        if revision.is_empty() {
            return Err(PortError::new(
                PortErrorCode::InvalidConfiguration,
                "Git synchronization target returned an empty HEAD revision.",
            ));
        }
        Ok(revision)
    }

    fn remote_head(&self, reference: &str) -> Result<String, PortError> {
        let output = self.command_output(
            Command::new(&self.git)
                .args([OsStr::new("ls-remote"), OsStr::new("--exit-code")])
                .arg(reference)
                .arg("HEAD"),
            "remote revision inspection",
        )?;
        if !output.status.success() {
            return Err(Self::error(
                PortErrorCode::ProviderOperationFailed,
                "Git synchronization source HEAD could not be resolved.",
                Self::output_detail(&output),
            ));
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        let revision = stdout
            .lines()
            .find_map(|line| line.split_whitespace().next())
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                PortError::new(
                    PortErrorCode::ProviderOperationFailed,
                    "Git synchronization source returned no HEAD revision.",
                )
            })?;
        Ok(revision.to_owned())
    }

    fn ensure_clean_worktree(&self) -> Result<(), PortError> {
        let output = self.command_output(
            Command::new(&self.git)
                .arg("-C")
                .arg(&self.target)
                .args(["status", "--porcelain=v1"]),
            "working-tree inspection",
        )?;
        if !output.status.success() {
            return Err(Self::error(
                PortErrorCode::ProviderOperationFailed,
                "Git working-tree status could not be inspected.",
                Self::output_detail(&output),
            ));
        }
        if !output.stdout.is_empty() {
            return Err(Self::error(
                PortErrorCode::InvalidConfiguration,
                "Git synchronization refuses to update a working tree with local changes.",
                String::from_utf8_lossy(&output.stdout).trim().to_owned(),
            ));
        }
        Ok(())
    }

    fn fast_forward(&self, reference: &str) -> Result<(), PortError> {
        let fetch = self.command_output(
            Command::new(&self.git)
                .arg("-C")
                .arg(&self.target)
                .args(["fetch", "--no-tags"])
                .arg(reference)
                .arg("HEAD"),
            "fetch",
        )?;
        if !fetch.status.success() {
            return Err(Self::error(
                PortErrorCode::ProviderOperationFailed,
                "Git synchronization fetch failed.",
                Self::output_detail(&fetch),
            ));
        }

        let merge = self.command_output(
            Command::new(&self.git)
                .arg("-C")
                .arg(&self.target)
                .args(["merge", "--ff-only", "FETCH_HEAD"]),
            "fast-forward merge",
        )?;
        if !merge.status.success() {
            return Err(Self::error(
                PortErrorCode::ProviderOperationFailed,
                "Git synchronization requires a clean fast-forward update; the target has divergent history or could not merge FETCH_HEAD.",
                Self::output_detail(&merge),
            ));
        }
        Ok(())
    }
}

impl Default for GitSynchronizerConnector {
    fn default() -> Self {
        Self::new()
    }
}

impl Synchronizer for GitSynchronizerConnector {
    fn preview(&self, input: &SynchronizationRequest) -> Result<StateChangePreview, PortError> {
        let reference = self.source_reference(input)?;
        let local = self.local_head()?;
        let remote = self.remote_head(reference)?;
        let changed = local != remote;
        Ok(StateChangePreview {
            changed,
            summary: if changed {
                format!(
                    "Git synchronization '{}' would fast-forward {} from {} to {}.",
                    input.id,
                    self.target.display(),
                    local,
                    remote
                )
            } else {
                format!(
                    "Git synchronization '{}' is already at source HEAD {}.",
                    input.id, local
                )
            },
        })
    }

    fn apply(&self, input: &SynchronizationRequest) -> Result<StateChangeResult, PortError> {
        let preview = self.preview(input)?;
        if !preview.changed {
            return Ok(StateChangeResult {
                changed: false,
                summary: preview.summary,
            });
        }

        let reference = self.source_reference(input)?;
        self.ensure_clean_worktree()?;
        self.fast_forward(reference)?;

        let after = self.preview(input)?;
        if after.changed {
            return Err(PortError::new(
                PortErrorCode::VerificationFailure,
                format!(
                    "Git synchronization '{}' remained out of date after a successful fast-forward operation.",
                    input.id
                ),
            ));
        }

        Ok(StateChangeResult {
            changed: true,
            summary: format!(
                "Git synchronization '{}' fast-forwarded {} and verified source HEAD.",
                input.id,
                self.target.display()
            ),
        })
    }
}

impl Connector for GitSynchronizerConnector {
    fn manifest(&self) -> &ConnectorManifest {
        &self.manifest
    }

    fn probe(&self, _port: &PortContract, _context: &ConnectorContext) -> CapabilityProbe {
        if !self.git_available() {
            return CapabilityProbe::unavailable(format!(
                "Git executable is unavailable; configure {GIT_ENV} when git is not on PATH."
            ));
        }
        if self.target.as_os_str().is_empty() {
            return CapabilityProbe::unavailable(format!(
                "Git synchronization target is not configured; set {TARGET_ENV}."
            ));
        }
        if !self.target_is_worktree() {
            return CapabilityProbe::unavailable(format!(
                "Configured Git synchronization target is not an available Git working tree: {}",
                self.target.display()
            ));
        }
        CapabilityProbe::available()
    }

    fn synchronizer(&self) -> Option<&dyn Synchronizer> {
        Some(self)
    }
}
