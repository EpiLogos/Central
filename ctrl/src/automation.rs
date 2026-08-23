use crate::action::{
    ActionAvailability, ActionDescriptor, ActionExecutionContext, ActionInputDefinition,
    ActionOutputDefinition, ActionRegistry, MutationClass,
};
use crate::result::{ActionResult, ResultStatus};
use central_connector_sdk::{AutomationRunInput, ConnectorSummary, AUTOMATION_PORT};
use serde_json::{json, Value};

fn required_automation(input: &Value) -> Result<String, ActionResult> {
    let Some(automation) = input
        .get("automation")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Err(ActionResult::failure(
            Some("automation.run"),
            ResultStatus::InvalidInput,
            "automation.run requires automation.",
            None,
        ));
    };
    Ok(automation.to_owned())
}

fn run_automation(
    _registry: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let automation = match required_automation(input) {
        Ok(automation) => automation,
        Err(result) => return result,
    };
    let resolution = context
        .connectors
        .resolve(&AUTOMATION_PORT, context.connector_context);
    let diagnostics = resolution.diagnostics.clone();
    let Some(connector) = resolution.connector else {
        return ActionResult::failure(
            Some("automation.run"),
            ResultStatus::UnavailableCapability,
            format!("No eligible Connector implements {}.", AUTOMATION_PORT.id),
            Some(json!({
                "port": AUTOMATION_PORT.id,
                "diagnostics": diagnostics,
            })),
        );
    };
    let Some(implementation) = connector.automation() else {
        return ActionResult::failure(
            Some("automation.run"),
            ResultStatus::ConnectorFailure,
            format!(
                "Selected Connector does not expose {} implementation.",
                AUTOMATION_PORT.id
            ),
            Some(json!({
                "port": AUTOMATION_PORT.id,
                "connector": ConnectorSummary::from_connector(connector),
                "diagnostics": diagnostics,
            })),
        );
    };

    match implementation.run(&AutomationRunInput {
        automation: automation.clone(),
    }) {
        Ok(output) => ActionResult::success(
            "automation.run",
            json!({
                "automation": output.automation,
                "port": AUTOMATION_PORT.id,
                "connector": ConnectorSummary::from_connector(connector),
                "diagnostics": diagnostics,
            }),
        ),
        Err(error) => ActionResult::failure(
            Some("automation.run"),
            ResultStatus::ConnectorFailure,
            format!("Connector failed while executing {}.", AUTOMATION_PORT.id),
            Some(json!({
                "automation": automation,
                "port": AUTOMATION_PORT.id,
                "connector": ConnectorSummary::from_connector(connector),
                "provider_error": error,
                "diagnostics": diagnostics,
            })),
        ),
    }
}

pub fn register_automation_actions(registry: &mut ActionRegistry) {
    registry
        .register(
            ActionDescriptor {
                id: "automation.run".to_owned(),
                title: "Run automation".to_owned(),
                description: "Invoke a named native automation through the provider-neutral Automation Port."
                    .to_owned(),
                inputs: vec![ActionInputDefinition {
                    name: "automation".to_owned(),
                    input_type: "string".to_owned(),
                    required: true,
                    choices: None,
                    selection: None,
                }],
                output: ActionOutputDefinition {
                    output_type: "automation-invocation".to_owned(),
                },
                mutation_class: MutationClass::ExternallyMutating,
                preview_supported: false,
                required_ports: vec![AUTOMATION_PORT.id.to_owned()],
                availability: ActionAvailability {
                    available: true,
                    reason: None,
                },
            },
            run_automation,
        )
        .expect("automation Action id is valid");
}
