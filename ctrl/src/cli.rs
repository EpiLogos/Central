use std::path::PathBuf;

use serde_json::json;

use crate::{
    action::ActionRegistry,
    control,
    result::{ActionResult, FailureCode, ResultStatus},
    root::{self, RootContext, RootError},
    runtime::Runtime,
    work,
};

#[derive(Debug, Clone, Default)]
pub struct ProcessContext {
    pub configured_root: Option<PathBuf>,
    pub home: Option<PathBuf>,
}

impl ProcessContext {
    pub fn from_process() -> Self {
        Self {
            configured_root: std::env::var_os("CENTRAL_ROOT").map(PathBuf::from),
            home: std::env::var_os("HOME").map(PathBuf::from),
        }
    }
}

#[derive(Debug, Clone)]
pub struct CommandOutput {
    pub result: ActionResult,
    pub human: String,
    pub json: bool,
    pub exit_code: i32,
}

impl CommandOutput {
    pub fn render(&self) -> String {
        if self.json {
            serde_json::to_string_pretty(&self.result).unwrap_or_else(|error| {
                format!("{{\"status\":\"internal_failure\",\"error\":\"{error}\"}}")
            })
        } else {
            self.human.clone()
        }
    }
}

#[derive(Debug)]
struct ParsedArgs {
    json: bool,
    explicit_root: Option<PathBuf>,
    command: Vec<String>,
}

pub fn run(args: Vec<String>, context: ProcessContext) -> CommandOutput {
    let parsed = match parse_args(&args) {
        Ok(parsed) => parsed,
        Err(message) => {
            return invalid_input("ctrl", message, args.iter().any(|arg| arg == "--json"));
        }
    };

    let runtime = Runtime::default();
    match parsed.command.as_slice() {
        [command] if command == "central.root" || command == "root" => {
            with_root("central.root", &parsed, &context, |root| {
                let display = root.display().to_string();
                success(
                    "central.root",
                    json!({ "root": display }),
                    display,
                    parsed.json,
                )
            })
        }
        [command] if command == "central.init" || command == "init" => with_root(
            "central.init",
            &parsed,
            &context,
            |root| match root::initialize(&root) {
                Ok(report) => {
                    let human = format!("Central initialized at {}", report.root);
                    success(
                        "central.init",
                        serde_json::to_value(report).expect("InitReport serializes"),
                        human,
                        parsed.json,
                    )
                }
                Err(error) => internal_failure("central.init", error.to_string(), parsed.json),
            },
        ),
        [command] if command == "central.doctor" || command == "doctor" => with_root(
            "central.doctor",
            &parsed,
            &context,
            |root| match root::doctor(&root) {
                Ok(report) if report.valid => {
                    let human = format!("Central structure is valid at {}", report.root);
                    success(
                        "central.doctor",
                        serde_json::to_value(report).expect("DoctorReport serializes"),
                        human,
                        parsed.json,
                    )
                }
                Ok(report) => {
                    let mut problems = Vec::new();
                    if !report.missing.is_empty() {
                        problems.push(format!("missing: {}", report.missing.join(", ")));
                    }
                    if !report.invalid.is_empty() {
                        problems.push(format!("not directories: {}", report.invalid.join(", ")));
                    }
                    let human = format!(
                        "Central structure is invalid at {} ({})",
                        report.root,
                        problems.join("; ")
                    );
                    failure_with_data(
                        "central.doctor",
                        ResultStatus::InvalidCentralStructure,
                        FailureCode::InvalidCentralStructure,
                        "basic Central structure is invalid",
                        serde_json::to_value(report).expect("DoctorReport serializes"),
                        human,
                        parsed.json,
                    )
                }
                Err(error) => internal_failure("central.doctor", error.to_string(), parsed.json),
            },
        ),
        [command] if command == "action.list" => action_list(&runtime.actions, parsed.json),
        [domain, command] if domain == "action" && command == "list" => {
            action_list(&runtime.actions, parsed.json)
        }
        [command] if command == "work.list" => work_list(&parsed, &context, &runtime),
        [domain, command] if domain == "work" && command == "list" => {
            work_list(&parsed, &context, &runtime)
        }
        [command, target] if command == "control.open" => {
            control_open(&parsed, &context, target)
        }
        [domain, command, target] if domain == "control" && command == "open" => {
            control_open(&parsed, &context, target)
        }
        [command, query @ ..] if command == "control.search" => {
            control_search(&parsed, &context, query)
        }
        [domain, command, query @ ..] if domain == "control" && command == "search" => {
            control_search(&parsed, &context, query)
        }
        _ => invalid_input(
            "ctrl",
            "unknown command; use root, init, doctor, action list, work list, control open, or control search",
            parsed.json,
        ),
    }
}

fn action_list(registry: &ActionRegistry, json_output: bool) -> CommandOutput {
    let actions = registry.descriptors();
    let human = actions
        .iter()
        .map(|action| format!("{}\t{} — {}", action.id, action.title, action.description))
        .collect::<Vec<_>>()
        .join("\n");
    success(
        "action.list",
        json!({ "actions": actions }),
        human,
        json_output,
    )
}

fn work_list(parsed: &ParsedArgs, context: &ProcessContext, runtime: &Runtime) -> CommandOutput {
    with_root("work.list", parsed, context, |root| {
        let result = work::list(&runtime.connectors, &runtime.environment, &root);
        let human = work_list_human(&result);
        output_from_result(result, human, parsed.json)
    })
}

fn control_open(parsed: &ParsedArgs, context: &ProcessContext, target: &str) -> CommandOutput {
    with_root("control.open", parsed, context, |root| {
        let result = control::open(&root, target);
        let human = result
            .data
            .as_ref()
            .and_then(|data| data["path"].as_str())
            .map(|path| path.to_string())
            .or_else(|| result.error.as_ref().map(|error| error.message.clone()))
            .unwrap_or_else(|| "control.open returned no path".into());
        output_from_result(result, human, parsed.json)
    })
}

fn control_search(
    parsed: &ParsedArgs,
    context: &ProcessContext,
    query_parts: &[String],
) -> CommandOutput {
    let query = query_parts.join(" ");
    with_root("control.search", parsed, context, |root| {
        let result = control::search(&root, &query);
        let human = control_search_human(&result);
        output_from_result(result, human, parsed.json)
    })
}

fn work_list_human(result: &ActionResult) -> String {
    if result.status == ResultStatus::Success {
        if let Some(data) = result.data.as_ref() {
            let names = data["items"]
                .as_array()
                .map(|items| {
                    items
                        .iter()
                        .filter_map(|item| item["name"].as_str())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let selected = data["diagnostics"]["selected_connector"]
                .as_str()
                .unwrap_or("none");
            if names.is_empty() {
                return format!("No Work items.\nConnector: {selected}");
            }
            return format!("{}\nConnector: {selected}", names.join("\n"));
        }
    }

    result
        .error
        .as_ref()
        .map(|error| error.message.clone())
        .unwrap_or_else(|| format!("work.list returned {:?}", result.status))
}

fn control_search_human(result: &ActionResult) -> String {
    if result.status == ResultStatus::Success {
        if let Some(data) = result.data.as_ref() {
            let matches = data["matches"].as_array().cloned().unwrap_or_default();
            if matches.is_empty() {
                return "No Control matches.".into();
            }
            return matches
                .iter()
                .map(|entry| {
                    format!(
                        "{}:{} {}",
                        entry["path"].as_str().unwrap_or("?"),
                        entry["line"].as_u64().unwrap_or(0),
                        entry["text"].as_str().unwrap_or("")
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
        }
    }

    result
        .error
        .as_ref()
        .map(|error| error.message.clone())
        .unwrap_or_else(|| format!("control.search returned {:?}", result.status))
}

fn output_from_result(result: ActionResult, human: String, json_output: bool) -> CommandOutput {
    let exit_code = match &result.status {
        ResultStatus::Success | ResultStatus::Cancelled => 0,
        ResultStatus::InternalFailure => 1,
        ResultStatus::InvalidInput => 2,
        ResultStatus::InvalidCentralStructure => 3,
        ResultStatus::UnavailableCapability => 4,
        ResultStatus::ConnectorFailure => 5,
        ResultStatus::PolicyRefusal => 6,
        ResultStatus::Partial => 7,
    };
    CommandOutput {
        result,
        human,
        json: json_output,
        exit_code,
    }
}

fn with_root<F>(
    action: &str,
    parsed: &ParsedArgs,
    context: &ProcessContext,
    operation: F,
) -> CommandOutput
where
    F: FnOnce(PathBuf) -> CommandOutput,
{
    let root_context = RootContext {
        explicit_root: parsed.explicit_root.clone(),
        configured_root: context.configured_root.clone(),
        home: context.home.clone(),
    };
    match root::resolve_root(&root_context) {
        Ok(root) => operation(root),
        Err(RootError::InvalidInput(message)) => invalid_input(action, message, parsed.json),
        Err(RootError::Io(error)) => internal_failure(action, error.to_string(), parsed.json),
    }
}

fn parse_args(args: &[String]) -> Result<ParsedArgs, String> {
    let mut json = false;
    let mut explicit_root = None;
    let mut command = Vec::new();
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--json" => json = true,
            "--root" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| "--root requires a path".to_string())?;
                if value.is_empty() {
                    return Err("--root requires a non-empty path".into());
                }
                explicit_root = Some(PathBuf::from(value.as_str()));
            }
            option if option.starts_with("--") => return Err(format!("unknown option: {option}")),
            value => command.push(value.to_string()),
        }
        index += 1;
    }

    Ok(ParsedArgs {
        json,
        explicit_root,
        command,
    })
}

fn success(
    action: &str,
    data: serde_json::Value,
    human: String,
    json_output: bool,
) -> CommandOutput {
    output_from_result(ActionResult::success(action, data), human, json_output)
}

fn invalid_input(action: &str, message: impl Into<String>, json_output: bool) -> CommandOutput {
    let message = message.into();
    output_from_result(
        ActionResult::failure(
            action,
            ResultStatus::InvalidInput,
            FailureCode::InvalidInput,
            message.clone(),
        ),
        format!("Invalid input: {message}"),
        json_output,
    )
}

fn internal_failure(action: &str, message: impl Into<String>, json_output: bool) -> CommandOutput {
    let message = message.into();
    output_from_result(
        ActionResult::failure(
            action,
            ResultStatus::InternalFailure,
            FailureCode::InternalFailure,
            message.clone(),
        ),
        format!("Internal failure: {message}"),
        json_output,
    )
}

fn failure_with_data(
    action: &str,
    status: ResultStatus,
    code: FailureCode,
    message: &str,
    data: serde_json::Value,
    human: String,
    json_output: bool,
) -> CommandOutput {
    output_from_result(
        ActionResult::failure_with_data(action, status, code, message, data),
        human,
        json_output,
    )
}
