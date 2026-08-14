use crate::action::{create_core_action_registry, ActionExecutionContext};
use crate::connector::ConnectorContext;
use crate::reference::create_default_connector_registry;
use crate::result::{ActionResult, ResultStatus};
use crate::root::RootOptions;
use serde_json::{json, Value};
use std::env;
use std::path::PathBuf;

#[derive(Debug, Clone, Default)]
pub struct CliEnvironment {
    pub configured_root: Option<PathBuf>,
    pub home: Option<PathBuf>,
}

impl CliEnvironment {
    pub fn from_process() -> Self {
        let configured_root = env::var_os("CENTRAL_ROOT").filter(|value| !value.is_empty()).map(PathBuf::from);
        let home = env::var_os("HOME")
            .filter(|value| !value.is_empty())
            .or_else(|| env::var_os("USERPROFILE").filter(|value| !value.is_empty()))
            .map(PathBuf::from);
        Self { configured_root, home }
    }
}

#[derive(Debug, Clone)]
pub struct CliExecution {
    pub result: ActionResult,
    pub output: String,
    pub exit_code: i32,
}

struct ParsedCommand {
    structured: bool,
    explicit_root: Option<PathBuf>,
    action_id: String,
}

fn parse_args(args: &[String]) -> Result<ParsedCommand, (bool, String)> {
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
            if value.starts_with("--") {
                return Err((structured, "--root requires a path.".to_owned()));
            }
            explicit_root = Some(PathBuf::from(value));
        } else if let Some(value) = argument.strip_prefix("--root=") {
            if value.is_empty() {
                return Err((structured, "--root requires a path.".to_owned()));
            }
            explicit_root = Some(PathBuf::from(value));
        } else if argument.starts_with("--") {
            return Err((structured, format!("Unknown option: {argument}")));
        } else {
            positional.push(argument.clone());
        }
        index += 1;
    }

    if positional.is_empty() {
        return Err((structured, "An Action or command is required.".to_owned()));
    }

    let action_id: &str = match positional.as_slice() {
        [command] if command == "root" => "central.root",
        [command] if command == "init" => "central.init",
        [command] if command == "doctor" => "central.doctor",
        [command] if command == "actions" => "action.list",
        [domain, verb] if domain == "action" && verb == "list" => "action.list",
        [domain, verb] if domain == "work" && verb == "list" => "work.list",
        [canonical] if matches!(canonical.as_str(), "central.root" | "central.init" | "central.doctor" | "action.list" | "work.list") => canonical.as_str(),
        [unknown] => return Err((structured, format!("Unknown command: {unknown}"))),
        _ => return Err((structured, format!("Unexpected arguments: {}", positional[1..].join(" ")))),
    };

    Ok(ParsedCommand { structured, explicit_root, action_id: action_id.to_owned() })
}

fn human_output(result: &ActionResult) -> String {
    if !result.ok {
        let error = result.error.as_ref().expect("failure has an error");
        return format!("{}: {}", error.code, error.message);
    }

    let Some(data) = result.data.as_ref() else {
        return String::new();
    };
    match result.action.as_deref() {
        Some("central.root") => {
            let path = data.get("path").and_then(Value::as_str).unwrap_or_default();
            let source = data.get("source").and_then(Value::as_str).unwrap_or("unknown");
            format!("{path} ({source})")
        }
        Some("central.init") => {
            let root = data.get("root").and_then(Value::as_str).unwrap_or_default();
            format!("Initialized Central at {root}")
        }
        Some("central.doctor") => {
            let root = data.get("root").and_then(Value::as_str).unwrap_or_default();
            let valid = data.get("valid").and_then(Value::as_bool).unwrap_or(false);
            let mut lines = vec![format!("Central root: {root}"), format!("Valid: {}", if valid { "yes" } else { "no" })];
            if let Some(checks) = data.get("checks").and_then(Value::as_array) {
                for check in checks {
                    let path = check.get("path").and_then(Value::as_str).unwrap_or_default();
                    let ok = check.get("valid").and_then(Value::as_bool).unwrap_or(false);
                    lines.push(format!("{}  {path}", if ok { "ok" } else { "missing" }));
                }
            }
            lines.join("\n")
        }
        Some("action.list") => data
            .get("actions")
            .and_then(Value::as_array)
            .map(|actions| actions.iter().map(|action| {
                let id = action.get("id").and_then(Value::as_str).unwrap_or_default();
                let title = action.get("title").and_then(Value::as_str).unwrap_or_default();
                format!("{id}\t{title}")
            }).collect::<Vec<_>>().join("\n"))
            .unwrap_or_default(),
        Some("work.list") => {
            let selected = data
                .get("diagnostics")
                .and_then(|diagnostics| diagnostics.get("selected_connector"))
                .and_then(|connector| connector.get("id"))
                .and_then(Value::as_str)
                .unwrap_or("none");
            let mut lines = vec![format!("Connector: {selected}")];
            if let Some(items) = data.get("items").and_then(Value::as_array) {
                for item in items {
                    let name = item.get("name").and_then(Value::as_str).unwrap_or_default();
                    let path = item.get("path").and_then(Value::as_str).unwrap_or_default();
                    lines.push(format!("{name}\t{path}"));
                }
            }
            lines.join("\n")
        }
        _ => data.to_string(),
    }
}

fn exit_code(result: &ActionResult) -> i32 {
    match result.status {
        ResultStatus::Success => 0,
        ResultStatus::InvalidInput => 2,
        ResultStatus::InvalidCentralStructure => 3,
        ResultStatus::UnavailableCapability => 4,
        ResultStatus::ConnectorFailure => 5,
        ResultStatus::InternalFailure => 1,
    }
}

pub fn run_cli(args: &[String], environment: &CliEnvironment) -> CliExecution {
    let parsed = match parse_args(args) {
        Ok(parsed) => parsed,
        Err((structured, message)) => {
            let result = ActionResult::failure(None, ResultStatus::InvalidInput, message, None);
            let output = if structured { serde_json::to_string(&result).expect("ActionResult serializes") } else { human_output(&result) };
            return CliExecution { exit_code: exit_code(&result), result, output };
        }
    };

    let root_options = RootOptions {
        explicit_root: parsed.explicit_root,
        configured_root: environment.configured_root.clone(),
        home: environment.home.clone(),
    };
    let connectors = create_default_connector_registry();
    let connector_context = ConnectorContext::current();
    let context = ActionExecutionContext {
        root_options: &root_options,
        connectors: &connectors,
        connector_context: &connector_context,
    };
    let registry = create_core_action_registry();
    let result = registry.execute(&parsed.action_id, &json!({}), &context);
    let output = if parsed.structured {
        serde_json::to_string(&result).expect("ActionResult serializes")
    } else {
        human_output(&result)
    };
    CliExecution { exit_code: exit_code(&result), result, output }
}
