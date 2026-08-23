use central_connector_sdk::{ConnectorContext, ConnectorRegistry};
use central_macos_connectors::MacOsNativeConnector;
use central_reference_connectors::create_default_connector_registry;
use central_shortcuts_connector::ShortcutsAutomationConnector;
use central_ctrl::{
    create_core_action_registry, register_automation_actions, run_cli_with_runtime,
    ActionExecutionContext, ActionRegistry, ActionResult, CliEnvironment, CliExecution,
    ResultStatus, RootOptions, TerminalSurface,
};
use serde_json::{json, Value};
use std::path::PathBuf;

pub fn create_macos_connector_registry() -> ConnectorRegistry {
    let mut registry = create_default_connector_registry();
    registry
        .register(MacOsNativeConnector::new())
        .expect("macOS native Connector manifest is valid");
    registry
        .register(ShortcutsAutomationConnector::new())
        .expect("Shortcuts Connector manifest is valid");
    registry
}

pub fn create_macos_action_registry() -> ActionRegistry {
    let mut registry = create_core_action_registry();
    central_ctrl::projectcentral_ops::register_projectcentral_actions(&mut registry);
    register_automation_actions(&mut registry);
    registry
}

#[derive(Debug)]
struct HostCommand {
    structured: bool,
    explicit_root: Option<PathBuf>,
    special: Option<SpecialCommand>,
}

#[derive(Debug)]
enum SpecialCommand {
    ActionList,
    ActionRun { action: String, input: Value },
    AutomationRun { automation: String },
}

fn parse_host_command(args: &[String]) -> Result<HostCommand, (bool, String)> {
    let mut structured = false;
    let mut explicit_root = None;
    let mut positional = Vec::new();
    let mut index = 0;
    while index < args.len() {
        let argument = &args[index];
        if argument == "--json" {
            structured = true;
        } else if argument == "--root" {
            index += 1;
            let Some(value) = args.get(index) else {
                return Err((structured, "--root requires a path.".to_owned()));
            };
            explicit_root = Some(PathBuf::from(value));
        } else if let Some(value) = argument.strip_prefix("--root=") {
            if value.is_empty() {
                return Err((structured, "--root requires a path.".to_owned()));
            }
            explicit_root = Some(PathBuf::from(value));
        } else if argument.starts_with("--") {
            return Ok(HostCommand {
                structured,
                explicit_root,
                special: None,
            });
        } else {
            positional.push(argument.clone());
        }
        index += 1;
    }

    let special = match positional.as_slice() {
        [command] if command == "actions" => Some(SpecialCommand::ActionList),
        [domain, verb] if domain == "action" && verb == "list" => Some(SpecialCommand::ActionList),
        [domain, verb, action] if domain == "action" && verb == "run" => {
            Some(SpecialCommand::ActionRun {
                action: action.clone(),
                input: json!({}),
            })
        }
        [domain, verb, action, input] if domain == "action" && verb == "run" => {
            let value = serde_json::from_str::<Value>(input).map_err(|error| {
                (
                    structured,
                    format!("action run input must be valid JSON: {error}"),
                )
            })?;
            if !value.is_object() {
                return Err((
                    structured,
                    "action run input must be a JSON object.".to_owned(),
                ));
            }
            Some(SpecialCommand::ActionRun {
                action: action.clone(),
                input: value,
            })
        }
        [domain, verb, automation] if domain == "automation" && verb == "run" => {
            Some(SpecialCommand::AutomationRun {
                automation: automation.clone(),
            })
        }
        _ => None,
    };

    Ok(HostCommand {
        structured,
        explicit_root,
        special,
    })
}

fn exit_code(result: &ActionResult) -> i32 {
    match result.status {
        ResultStatus::Success | ResultStatus::Cancelled => 0,
        ResultStatus::InvalidInput => 2,
        ResultStatus::InvalidCentralStructure => 3,
        ResultStatus::UnavailableCapability => 4,
        ResultStatus::ConnectorFailure => 5,
        ResultStatus::PartialCompletion => 6,
        ResultStatus::VerificationFailure => 7,
        ResultStatus::InternalFailure => 1,
    }
}

fn render_special(result: &ActionResult, structured: bool) -> String {
    if structured {
        return serde_json::to_string(result).expect("ActionResult serializes");
    }
    if !result.ok {
        return result
            .error
            .as_ref()
            .map(|error| format!("{}: {}", error.code, error.message))
            .unwrap_or_else(|| "Action failed.".to_owned());
    }
    match result.action.as_deref() {
        Some("action.list") => result
            .data
            .as_ref()
            .and_then(|data| data.get("actions"))
            .and_then(Value::as_array)
            .map(|actions| {
                actions
                    .iter()
                    .map(|action| {
                        format!(
                            "{}\t{}",
                            action.get("id").and_then(Value::as_str).unwrap_or_default(),
                            action
                                .get("title")
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_default(),
        _ => result
            .data
            .as_ref()
            .map(Value::to_string)
            .unwrap_or_default(),
    }
}

pub fn run_macos_cli_with_runtime(
    args: &[String],
    environment: &CliEnvironment,
    surface: &mut dyn TerminalSurface,
    connectors: &ConnectorRegistry,
    connector_context: &ConnectorContext,
) -> CliExecution {
    let parsed = match parse_host_command(args) {
        Ok(parsed) => parsed,
        Err((structured, message)) => {
            let result = ActionResult::failure(None, ResultStatus::InvalidInput, message, None);
            return CliExecution {
                exit_code: exit_code(&result),
                output: render_special(&result, structured),
                result,
            };
        }
    };

    let Some(command) = parsed.special else {
        return run_cli_with_runtime(args, environment, surface, connectors, connector_context);
    };

    let root_options = RootOptions {
        explicit_root: parsed.explicit_root,
        configured_root: environment.configured_root.clone(),
        home: environment.home.clone(),
    };
    let context = ActionExecutionContext {
        root_options: &root_options,
        connectors,
        connector_context,
    };
    let registry = create_macos_action_registry();
    let result = match command {
        SpecialCommand::ActionList => registry.execute("action.list", &json!({}), &context),
        SpecialCommand::ActionRun { action, input } => registry.execute(&action, &input, &context),
        SpecialCommand::AutomationRun { automation } => registry.execute(
            "automation.run",
            &json!({ "automation": automation }),
            &context,
        ),
    };
    CliExecution {
        exit_code: exit_code(&result),
        output: render_special(&result, parsed.structured),
        result,
    }
}

pub fn run_macos_cli(
    args: &[String],
    environment: &CliEnvironment,
    surface: &mut dyn TerminalSurface,
) -> CliExecution {
    let connectors = create_macos_connector_registry();
    let connector_context = ConnectorContext::current();
    run_macos_cli_with_runtime(
        args,
        environment,
        surface,
        &connectors,
        &connector_context,
    )
}
