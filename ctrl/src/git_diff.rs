//! Bounded GitState diff Action, owned by Central's existing Git integration.
use crate::{
    action::{
        ActionAvailability, ActionDescriptor, ActionExecutionContext, ActionInputDefinition,
        ActionOutputDefinition, ActionRegistry, MutationClass,
    },
    result::{ActionResult, ResultStatus},
    root::resolve_central_root,
};
use central_connector_sdk::{GitDiffRequest, GIT_STATE_PORT};
use serde_json::Value;
fn execute(
    _: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let action = "central.git.diff";
    let result = (|| -> Result<Value, String> {
        let central = resolve_central_root(context.root_options)?
            .path
            .canonicalize()
            .map_err(|e| e.to_string())?;
        let request: GitDiffRequest =
            serde_json::from_value(input.clone()).map_err(|e| e.to_string())?;
        let repo = if request.repo_root.is_absolute() {
            request.repo_root.clone()
        } else {
            central.join(&request.repo_root)
        }
        .canonicalize()
        .map_err(|e| e.to_string())?;
        if !repo.starts_with(&central) {
            return Err(
                "Repository must belong to this Central root; choose its registered local worktree"
                    .into(),
            );
        }
        let resolution = context
            .connectors
            .resolve(&GIT_STATE_PORT, context.connector_context);
        let connector = resolution
            .connector
            .ok_or("The GitState connector is unavailable")?;
        let provider = connector
            .git_state()
            .ok_or("The selected connector does not expose GitState")?;
        let reading = provider
            .diff(&GitDiffRequest {
                repo_root: repo,
                ..request
            })
            .map_err(|e| {
                format!(
                    "{}{}",
                    e.message,
                    e.provider_detail
                        .map(|s| format!(": {s}"))
                        .unwrap_or_default()
                )
            })?;
        serde_json::to_value(reading).map_err(|e| e.to_string())
    })();
    match result {
        Ok(reading) => ActionResult::success(action, reading),
        Err(error) => ActionResult::failure(Some(action), ResultStatus::InvalidInput, error, None),
    }
}
pub fn register(registry: &mut ActionRegistry) {
    let input = |name: &str, kind: &str, required: bool| ActionInputDefinition {
        name: name.into(),
        input_type: kind.into(),
        required,
        choices: None,
        selection: None,
    };
    registry.register(ActionDescriptor{id:"central.git.diff".into(),title:"Read repository changes".into(),description:"Bounded commit or working-tree diff through GitState. No fetch or Git mutation; exact revisions, file counts and truncation are disclosed.".into(),inputs:vec![input("repo_root","string",true),input("from","string",true),input("to","string",true),input("ignore_whitespace","boolean",false),input("max_bytes","integer",false),input("max_files","integer",false)],output:ActionOutputDefinition{output_type:"central.git-diff/v1".into()},mutation_class:MutationClass::ReadOnly,preview_supported:false,required_ports:vec![GIT_STATE_PORT.id.into()],availability:ActionAvailability{available:true,reason:None}},execute).expect("unique Git diff Action");
}
