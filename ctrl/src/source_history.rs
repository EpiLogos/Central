use crate::action::{
    ActionAvailability, ActionDescriptor, ActionExecutionContext, ActionInputDefinition,
    ActionOutputDefinition, ActionRegistry, MutationClass,
};
use crate::result::{ActionResult, ResultStatus};
use crate::root::resolve_central_root;
use crate::source_horizon::{read_project_change_horizon, ObservedSource, SourceBinding};
use central_connector_sdk::{
    PortError, PortErrorCode, SourceCompareOutput, SourceCompareRequest, SourceHistory,
    SourceHistoryOutput, SourceHistoryRequest, SourceRevisionReadOutput, SourceRevisionReadRequest,
    SOURCE_HISTORY_PORT,
};
use serde::Serialize;
use serde_json::{json, to_value, Value};
use std::io;
use std::path::{Component, Path, PathBuf};

pub const CENTRAL_SOURCE_HISTORY_SCHEMA: &str = "central.source-history/v1";
pub const CENTRAL_SOURCE_RECOVERY_PREVIEW_SCHEMA: &str = "central.source-recovery-preview/v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CentralSourceHistory {
    pub schema: String,
    pub world_ref: String,
    pub source: SourceBinding,
    pub current_content_revision: String,
    pub provider: String,
    pub history: SourceHistoryOutput,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CentralSourceCompare {
    pub schema: String,
    pub world_ref: String,
    pub source: SourceBinding,
    pub current_content_revision: String,
    pub provider: String,
    pub comparison: SourceCompareOutput,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SourceRecoveryPreview {
    pub schema: String,
    pub world_ref: String,
    pub source: SourceBinding,
    pub current_content_revision: String,
    pub expected_content_revision: String,
    pub historical_revision: String,
    pub basis_matches_current: bool,
    pub historical_content: Option<SourceRevisionReadOutput>,
    pub requires_recognition: bool,
    pub mutation_performed: bool,
    pub reason: String,
}

pub fn read_source_history(
    project_root: &Path,
    source_ref: &str,
    limit: usize,
    provider: &dyn SourceHistory,
) -> io::Result<CentralSourceHistory> {
    let (world_ref, observed) = observed_source(project_root, source_ref)?;
    require_retrieval(&observed.binding)?;
    let history = provider
        .history(&SourceHistoryRequest {
            world_root: project_root.to_path_buf(),
            source_path: PathBuf::from(&observed.binding.path),
            limit: limit.max(1),
        })
        .map_err(port_io)?;
    Ok(CentralSourceHistory {
        schema: CENTRAL_SOURCE_HISTORY_SCHEMA.to_owned(),
        world_ref,
        source: observed.binding,
        current_content_revision: observed.revision.revision,
        provider: history.provider.clone(),
        history,
    })
}

pub fn compare_source_history(
    project_root: &Path,
    source_ref: &str,
    from_revision: &str,
    to_revision: &str,
    max_bytes: usize,
    provider: &dyn SourceHistory,
) -> io::Result<CentralSourceCompare> {
    let (world_ref, observed) = observed_source(project_root, source_ref)?;
    require_retrieval(&observed.binding)?;
    let comparison = provider
        .compare(&SourceCompareRequest {
            world_root: project_root.to_path_buf(),
            source_path: PathBuf::from(&observed.binding.path),
            from_revision: from_revision.to_owned(),
            to_revision: to_revision.to_owned(),
            max_bytes: max_bytes.max(1),
        })
        .map_err(port_io)?;
    Ok(CentralSourceCompare {
        schema: CENTRAL_SOURCE_HISTORY_SCHEMA.to_owned(),
        world_ref,
        source: observed.binding,
        current_content_revision: observed.revision.revision,
        provider: comparison.provider.clone(),
        comparison,
    })
}

pub fn preview_source_recovery(
    project_root: &Path,
    source_ref: &str,
    expected_content_revision: &str,
    historical_revision: &str,
    max_bytes: usize,
    provider: &dyn SourceHistory,
) -> io::Result<SourceRecoveryPreview> {
    let (world_ref, observed) = observed_source(project_root, source_ref)?;
    require_retrieval(&observed.binding)?;
    let basis_matches_current = observed.revision.revision == expected_content_revision;
    let requires_recognition = recognised_human_source(&observed.binding);
    let historical_content = if basis_matches_current {
        Some(
            provider
                .read_revision(&SourceRevisionReadRequest {
                    world_root: project_root.to_path_buf(),
                    source_path: PathBuf::from(&observed.binding.path),
                    revision: historical_revision.to_owned(),
                    max_bytes: max_bytes.max(1),
                })
                .map_err(port_io)?,
        )
    } else {
        None
    };
    Ok(SourceRecoveryPreview {
        schema: CENTRAL_SOURCE_RECOVERY_PREVIEW_SCHEMA.to_owned(),
        world_ref,
        source: observed.binding,
        current_content_revision: observed.revision.revision,
        expected_content_revision: expected_content_revision.to_owned(),
        historical_revision: historical_revision.to_owned(),
        basis_matches_current,
        historical_content,
        requires_recognition,
        mutation_performed: false,
        reason: if !basis_matches_current {
            "current source revision moved since the caller basis; recovery must be reconciled before any source mutation".to_owned()
        } else if requires_recognition {
            "historical material is available as a recovery candidate; recognised human source requires explicit human Recognition through the native owner mutation path".to_owned()
        } else {
            "historical material is available as a recovery candidate; this preview does not grant or infer source mutation authority".to_owned()
        },
    })
}

fn observed_source(project_root: &Path, source_ref: &str) -> io::Result<(String, ObservedSource)> {
    if source_ref.trim().is_empty() || source_ref != source_ref.trim() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "source_ref must be a non-empty stable Central source reference",
        ));
    }
    let horizon = read_project_change_horizon(project_root, None)?;
    let source = horizon
        .sources
        .into_iter()
        .find(|source| source.binding.source_ref == source_ref)
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "source_ref is not in the current Project horizon"))?;
    Ok((horizon.world_ref, source))
}

fn require_retrieval(binding: &SourceBinding) -> io::Result<()> {
    if !binding.agent_retrieval_allowed {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "source history is not disclosed through this application seam because the current source binding is excluded from Agent retrieval",
        ));
    }
    Ok(())
}

fn recognised_human_source(binding: &SourceBinding) -> bool {
    matches!(binding.provenance.as_str(), "human-authored" | "human-adopted")
}

fn port_io(error: PortError) -> io::Error {
    let kind = match error.code {
        PortErrorCode::InvalidInput | PortErrorCode::InvalidConfiguration => io::ErrorKind::InvalidInput,
        PortErrorCode::PermissionFailure => io::ErrorKind::PermissionDenied,
        PortErrorCode::MissingDependency | PortErrorCode::CapabilityUnavailable => io::ErrorKind::NotFound,
        _ => io::ErrorKind::Other,
    };
    io::Error::new(kind, error.message)
}

fn project_root(action: &str, input: &Value, context: &ActionExecutionContext<'_>) -> Result<PathBuf, ActionResult> {
    let project = required(input, "project", action)?;
    if Path::new(&project).is_absolute()
        || !Path::new(&project)
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
    {
        return Err(ActionResult::failure(
            Some(action),
            ResultStatus::InvalidInput,
            "project must be a single Central/Work member name.",
            None,
        ));
    }
    let central = resolve_central_root(context.root_options).map_err(|message| {
        ActionResult::failure(Some(action), ResultStatus::InvalidInput, message, None)
    })?;
    let root = central.path.join("Work").join(project);
    if !root.is_dir() {
        return Err(ActionResult::failure(
            Some(action),
            ResultStatus::InvalidInput,
            format!("Project root does not exist: {}", root.display()),
            None,
        ));
    }
    Ok(root)
}

fn required(input: &Value, field: &str, action: &str) -> Result<String, ActionResult> {
    input
        .get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| {
            ActionResult::failure(
                Some(action),
                ResultStatus::InvalidInput,
                format!("{action} requires {field}."),
                None,
            )
        })
}

fn number(input: &Value, field: &str, default: usize) -> usize {
    input
        .get(field)
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .unwrap_or(default)
}

fn resolve_provider<'a>(
    action: &str,
    context: &'a ActionExecutionContext<'a>,
) -> Result<&'a dyn SourceHistory, ActionResult> {
    let resolution = context
        .connectors
        .resolve(&SOURCE_HISTORY_PORT, context.connector_context);
    let diagnostics = to_value(&resolution.diagnostics).expect("Connector diagnostics serialise");
    let Some(connector) = resolution.connector else {
        return Err(ActionResult::failure(
            Some(action),
            ResultStatus::UnavailableCapability,
            format!("No eligible Connector implements {}.", SOURCE_HISTORY_PORT.id),
            Some(json!({ "port": SOURCE_HISTORY_PORT.id, "diagnostics": diagnostics })),
        ));
    };
    connector.source_history().ok_or_else(|| {
        ActionResult::failure(
            Some(action),
            ResultStatus::UnavailableCapability,
            "Resolved SourceHistory Connector did not expose its declared Port implementation.",
            Some(json!({ "port": SOURCE_HISTORY_PORT.id, "diagnostics": diagnostics })),
        )
    })
}

fn action_failure(action: &str, error: io::Error) -> ActionResult {
    let status = match error.kind() {
        io::ErrorKind::InvalidInput => ResultStatus::InvalidInput,
        io::ErrorKind::PermissionDenied => ResultStatus::UnavailableCapability,
        io::ErrorKind::NotFound => ResultStatus::UnavailableCapability,
        _ => ResultStatus::ConnectorFailure,
    };
    ActionResult::failure(Some(action), status, error.to_string(), None)
}

fn history_action(_: &ActionRegistry, input: &Value, context: &ActionExecutionContext<'_>) -> ActionResult {
    let action = "projectcentral.source.history";
    let root = match project_root(action, input, context) { Ok(value) => value, Err(result) => return result };
    let source_ref = match required(input, "source_ref", action) { Ok(value) => value, Err(result) => return result };
    let provider = match resolve_provider(action, context) { Ok(value) => value, Err(result) => return result };
    read_source_history(&root, &source_ref, number(input, "limit", 20).min(200), provider)
        .map(|value| ActionResult::success(action, to_value(value).expect("history serialises")))
        .unwrap_or_else(|error| action_failure(action, error))
}

fn compare_action(_: &ActionRegistry, input: &Value, context: &ActionExecutionContext<'_>) -> ActionResult {
    let action = "projectcentral.source.compare";
    let root = match project_root(action, input, context) { Ok(value) => value, Err(result) => return result };
    let source_ref = match required(input, "source_ref", action) { Ok(value) => value, Err(result) => return result };
    let from = match required(input, "from_revision", action) { Ok(value) => value, Err(result) => return result };
    let to = match required(input, "to_revision", action) { Ok(value) => value, Err(result) => return result };
    let provider = match resolve_provider(action, context) { Ok(value) => value, Err(result) => return result };
    compare_source_history(&root, &source_ref, &from, &to, number(input, "max_bytes", 131_072).min(1_048_576), provider)
        .map(|value| ActionResult::success(action, to_value(value).expect("comparison serialises")))
        .unwrap_or_else(|error| action_failure(action, error))
}

fn recovery_preview_action(_: &ActionRegistry, input: &Value, context: &ActionExecutionContext<'_>) -> ActionResult {
    let action = "projectcentral.source.recovery.preview";
    let root = match project_root(action, input, context) { Ok(value) => value, Err(result) => return result };
    let source_ref = match required(input, "source_ref", action) { Ok(value) => value, Err(result) => return result };
    let expected = match required(input, "expected_content_revision", action) { Ok(value) => value, Err(result) => return result };
    let historical = match required(input, "historical_revision", action) { Ok(value) => value, Err(result) => return result };
    let provider = match resolve_provider(action, context) { Ok(value) => value, Err(result) => return result };
    preview_source_recovery(&root, &source_ref, &expected, &historical, number(input, "max_bytes", 131_072).min(1_048_576), provider)
        .map(|value| ActionResult::success(action, to_value(value).expect("recovery preview serialises")))
        .unwrap_or_else(|error| action_failure(action, error))
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

fn descriptor(id: &str, title: &str, description: &str, inputs: Vec<ActionInputDefinition>, output: &str) -> ActionDescriptor {
    ActionDescriptor {
        id: id.to_owned(),
        title: title.to_owned(),
        description: description.to_owned(),
        inputs,
        output: ActionOutputDefinition { output_type: output.to_owned() },
        mutation_class: MutationClass::ReadOnly,
        preview_supported: false,
        required_ports: vec![SOURCE_HISTORY_PORT.id.to_owned()],
        availability: ActionAvailability { available: true, reason: None },
    }
}

pub fn register_source_history_actions(registry: &mut ActionRegistry) {
    let definitions = [
        (
            descriptor(
                "projectcentral.source.history",
                "Read native source history",
                "Read bounded provider history for one current Central SourceRef without changing SourceRef identity or source authority.",
                vec![text_input("project", true), text_input("source_ref", true), text_input("limit", false)],
                "central-source-history",
            ),
            history_action as fn(&ActionRegistry, &Value, &ActionExecutionContext<'_>) -> ActionResult,
        ),
        (
            descriptor(
                "projectcentral.source.compare",
                "Compare source revisions",
                "Compare two provider revisions for one current Central SourceRef while preserving Central privacy and semantic source identity.",
                vec![
                    text_input("project", true),
                    text_input("source_ref", true),
                    text_input("from_revision", true),
                    text_input("to_revision", true),
                    text_input("max_bytes", false),
                ],
                "central-source-comparison",
            ),
            compare_action,
        ),
        (
            descriptor(
                "projectcentral.source.recovery.preview",
                "Preview source recovery",
                "Read a historical candidate only when the caller basis still matches current source state; this never mutates source and never infers mutation authority.",
                vec![
                    text_input("project", true),
                    text_input("source_ref", true),
                    text_input("expected_content_revision", true),
                    text_input("historical_revision", true),
                    text_input("max_bytes", false),
                ],
                "central-source-recovery-preview",
            ),
            recovery_preview_action,
        ),
    ];
    for (descriptor, handler) in definitions {
        registry.register(descriptor, handler).expect("Source history Action ids are valid");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use central_connector_sdk::{
        SourceHistoryEntry, SourceHistoryOutput, SourceCompareOutput, SourceRevisionReadOutput,
    };
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct FakeHistory;
    impl SourceHistory for FakeHistory {
        fn history(&self, input: &SourceHistoryRequest) -> Result<SourceHistoryOutput, PortError> {
            Ok(SourceHistoryOutput {
                provider: "fake".into(),
                source_path: input.source_path.clone(),
                entries: vec![SourceHistoryEntry {
                    revision: "git-a".into(), parents: vec![], subject: "initial".into(), author: None, authored_at: None,
                }],
            })
        }
        fn compare(&self, input: &SourceCompareRequest) -> Result<SourceCompareOutput, PortError> {
            Ok(SourceCompareOutput {
                provider: "fake".into(), source_path: input.source_path.clone(),
                from_revision: input.from_revision.clone(), to_revision: input.to_revision.clone(),
                patch: "diff".into(), truncated: false,
            })
        }
        fn read_revision(&self, input: &SourceRevisionReadRequest) -> Result<SourceRevisionReadOutput, PortError> {
            Ok(SourceRevisionReadOutput {
                provider: "fake".into(), source_path: input.source_path.clone(), revision: input.revision.clone(),
                content: b"prior\n".to_vec(), truncated: false,
            })
        }
    }

    #[test]
    fn recovery_preview_refuses_to_read_candidate_after_basis_moves() {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let root = std::env::temp_dir().join(format!("central-source-history-{nonce}"));
        fs::create_dir_all(root.join("ProjectCentral/user")).unwrap();
        crate::projectcentral_ops::initialize_projectcentral(&root, &root, "example/history").unwrap();
        let source = root.join("ProjectCentral/user/intent.md");
        fs::write(&source, "one\n").unwrap();
        let horizon = read_project_change_horizon(&root, None).unwrap();
        let observed = horizon.sources.iter().find(|source| source.binding.path.ends_with("intent.md")).unwrap();
        let source_ref = observed.binding.source_ref.clone();
        let basis = observed.revision.revision.clone();
        fs::write(&source, "two\n").unwrap();

        let preview = preview_source_recovery(&root, &source_ref, &basis, "git-a", 1024, &FakeHistory).unwrap();
        assert!(!preview.basis_matches_current);
        assert!(preview.historical_content.is_none());
        assert!(!preview.mutation_performed);
        let _ = fs::remove_dir_all(root);
    }
}
