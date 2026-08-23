use crate::action::{create_core_action_registry, ActionExecutionContext};
use crate::picker::{run_guided_action_picker, NullTerminalSurface, TerminalSurface};
use crate::projectcentral_ops::register_projectcentral_actions;
use crate::result::{ActionResult, ResultStatus};
use crate::root::RootOptions;
use central_connector_sdk::ConnectorContext;
use central_reference_connectors::create_default_connector_registry;
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

enum CommandTarget {
    Direct { action_id: String, input: Value },
    Guided,
}

struct ParsedCommand {
    structured: bool,
    explicit_root: Option<PathBuf>,
    target: CommandTarget,
}

fn parse_action_input(structured: bool, action_id: &str, raw: &str) -> Result<Value, (bool, String)> {
    let value: Value = serde_json::from_str(raw)
        .map_err(|error| (structured, format!("action run input must be a JSON object: {error}")))?;
    if !value.is_object() {
        return Err((structured, format!("action run input for {action_id} must be a JSON object.")));
    }
    Ok(value)
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
    if positional.as_slice() == ["pick"] {
        return Ok(ParsedCommand { structured, explicit_root, target: CommandTarget::Guided });
    }
    if positional.first().map(String::as_str) == Some("pick") {
        return Err((structured, "pick takes no positional input.".to_owned()));
    }

    let (action_id, input): (&str, Value) = match positional.as_slice() {
        [command] if command == "root" => ("central.root", json!({})),
        [command] if command == "init" => ("central.init", json!({})),
        [command] if command == "doctor" => ("central.doctor", json!({})),
        [command] if command == "actions" => ("action.list", json!({})),
        [domain, verb] if domain == "action" && verb == "list" => ("action.list", json!({})),
        [domain, verb, action] if domain == "action" && verb == "run" => {
            (action.as_str(), json!({}))
        }
        [domain, verb, action, raw] if domain == "action" && verb == "run" => {
            (action.as_str(), parse_action_input(structured, action, raw)?)
        }
        [domain, verb] if domain == "action" && verb == "run" => {
            return Err((structured, "action run requires an Action id.".to_owned()));
        }
        [domain, verb, ..] if domain == "action" && verb == "run" => {
            return Err((structured, "action run accepts one Action id and at most one JSON object argument.".to_owned()));
        }
        [domain, verb] if domain == "work" && verb == "list" => ("work.list", json!({})),
        [domain, verb, rest @ ..] if domain == "work" && verb == "search" && !rest.is_empty() => {
            ("work.search", json!({ "query": rest.join(" ") }))
        }
        [domain, verb, rest @ ..] if domain == "work" && verb == "open" && !rest.is_empty() => {
            ("work.open", json!({ "query": rest.join(" ") }))
        }
        [domain, verb, rest @ ..] if domain == "work" && verb == "reveal" && !rest.is_empty() => {
            ("work.reveal", json!({ "query": rest.join(" ") }))
        }
        [command, rest @ ..] if command == "open" && !rest.is_empty() => {
            ("work.open", json!({ "query": rest.join(" ") }))
        }
        [domain, verb] if domain == "work" && matches!(verb.as_str(), "search" | "open" | "reveal") => {
            return Err((structured, format!("work {verb} requires a query.")));
        }
        [command] if command == "open" => return Err((structured, "open requires a Work name or search.".to_owned())),
        [domain, verb, target] if domain == "control" && verb == "open" => ("control.open", json!({ "target": target })),
        [domain, verb, rest @ ..] if domain == "control" && verb == "search" && !rest.is_empty() => {
            ("control.search", json!({ "query": rest.join(" ") }))
        }
        [domain, verb] if domain == "control" && verb == "open" => {
            return Err((structured, "control open requires one Control root.".to_owned()));
        }
        [domain, verb] if domain == "control" && verb == "search" => {
            return Err((structured, "control search requires a query.".to_owned()));
        }
        [domain, verb] if domain == "machine" && matches!(verb.as_str(), "inspect" | "account") => {
            let action = if verb == "inspect" { "machine.inspect" } else { "machine.account" };
            (action, json!({}))
        },
        [domain, verb, role]
            if domain == "machine" && matches!(verb.as_str(), "declaration" | "plan" | "apply" | "verify") =>
        {
            let action = match verb.as_str() {
                "declaration" => "machine.declaration",
                "plan" => "machine.plan",
                "apply" => "machine.apply",
                "verify" => "machine.verify",
                _ => unreachable!(),
            };
            (action, json!({ "role": role }))
        }
        [domain, verb]
            if domain == "machine" && matches!(verb.as_str(), "declaration" | "plan" | "apply" | "verify") =>
        {
            return Err((structured, format!("machine {verb} requires a role.")));
        }
        [domain, verb, role] if domain == "recovery" && verb == "plan" => {
            ("central.recovery.plan", json!({ "role": role }))
        }
        [domain, verb] if domain == "recovery" && verb == "plan" => {
            return Err((structured, "recovery plan requires a role.".to_owned()));
        }
        [command, role] if command == "recover" => {
            ("central.recover", json!({ "role": role }))
        }
        [command] if command == "recover" => {
            return Err((structured, "recover requires a role.".to_owned()));
        }
        [canonical, rest @ ..] if canonical == "work.search" && !rest.is_empty() => {
            ("work.search", json!({ "query": rest.join(" ") }))
        }
        [canonical, rest @ ..] if canonical == "work.open" && !rest.is_empty() => {
            ("work.open", json!({ "query": rest.join(" ") }))
        }
        [canonical, rest @ ..] if canonical == "work.reveal" && !rest.is_empty() => {
            ("work.reveal", json!({ "query": rest.join(" ") }))
        }
        [canonical] if matches!(canonical.as_str(), "work.search" | "work.open" | "work.reveal") => {
            return Err((structured, format!("{canonical} requires a query.")));
        }
        [canonical, target] if canonical == "control.open" => ("control.open", json!({ "target": target })),
        [canonical, rest @ ..] if canonical == "control.search" && !rest.is_empty() => {
            ("control.search", json!({ "query": rest.join(" ") }))
        }
        [canonical] if canonical == "control.open" => {
            return Err((structured, "control.open requires one Control root.".to_owned()));
        }
        [canonical] if canonical == "control.search" => {
            return Err((structured, "control.search requires a query.".to_owned()));
        }
        [canonical, role]
            if matches!(
                canonical.as_str(),
                "machine.declaration" | "machine.plan" | "machine.apply" | "machine.verify" | "central.recovery.plan" | "central.recover"
            ) =>
        {
            (canonical.as_str(), json!({ "role": role }))
        }
        [canonical]
            if matches!(
                canonical.as_str(),
                "machine.declaration" | "machine.plan" | "machine.apply" | "machine.verify" | "central.recovery.plan" | "central.recover"
            ) =>
        {
            return Err((structured, format!("{canonical} requires a role.")));
        }
        [canonical]
            if matches!(
                canonical.as_str(),
                "central.root" | "central.init" | "central.doctor" | "action.list" | "machine.inspect" | "machine.account" | "work.list"
            ) =>
        {
            (canonical.as_str(), json!({}))
        }
        [unknown] => return Err((structured, format!("Unknown command: {unknown}"))),
        _ => return Err((structured, format!("Unexpected arguments: {}", positional[1..].join(" ")))),
    };

    Ok(ParsedCommand {
        structured,
        explicit_root,
        target: CommandTarget::Direct { action_id: action_id.to_owned(), input },
    })
}

fn human_output(result: &ActionResult) -> String {
    if !result.ok {
        let error = result.error.as_ref().expect("failure has an error");
        return if result.status == ResultStatus::Cancelled {
            error.message.clone()
        } else {
            format!("{}: {}", error.code, error.message)
        };
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
            if let Some(mixed) = data.get("mixed_root") {
                if mixed.get("detected").and_then(Value::as_bool).unwrap_or(false) {
                    let message = mixed.get("message").and_then(Value::as_str).unwrap_or("Central personal root is also the Central product source checkout.");
                    lines.push(format!("warning: {message}"));
                }
            }
            lines.join("\n")
        }
        Some("central.recovery.plan") => crate::recovery::explain_recovery_plan(data),
        Some("central.recover") => crate::recovery::explain_recovery(data),
        Some("action.list") => data
            .get("actions")
            .and_then(Value::as_array)
            .map(|actions| actions.iter().map(|action| {
                let id = action.get("id").and_then(Value::as_str).unwrap_or_default();
                let title = action.get("title").and_then(Value::as_str).unwrap_or_default();
                format!("{id}\t{title}")
            }).collect::<Vec<_>>().join("\n"))
            .unwrap_or_default(),
        Some("control.open") => {
            let target = data.get("target").and_then(Value::as_str).unwrap_or_default();
            let path = data.get("path").and_then(Value::as_str).unwrap_or_default();
            format!("{target}\t{path}")
        }
        Some("control.search") => data
            .get("matches")
            .and_then(Value::as_array)
            .map(|matches| matches.iter().map(|item| {
                let path = item.get("source_path").and_then(Value::as_str).unwrap_or_default();
                let line = item.get("line").and_then(Value::as_u64).unwrap_or_default();
                let text = item.get("text").and_then(Value::as_str).unwrap_or_default();
                format!("{path}:{line}\t{text}")
            }).collect::<Vec<_>>().join("\n"))
            .unwrap_or_default(),
        Some("machine.declaration") => crate::machine::explain_machine_declaration(data),
        Some("machine.inspect") => crate::machine::explain_machine_inspection(data),
        Some("machine.account") => crate::machine_account::explain_account(data),
        Some("machine.plan") => crate::machine::explain_machine_plan(data),
        Some("machine.apply") => crate::machine::explain_machine_apply(data),
        Some("machine.verify") => crate::machine::explain_machine_verification(data),
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
        Some("work.search") => data
            .get("matches")
            .and_then(Value::as_array)
            .map(|matches| matches.iter().map(|item| {
                let name = item.get("name").and_then(Value::as_str).unwrap_or_default();
                let path = item.get("path").and_then(Value::as_str).unwrap_or_default();
                format!("{name}\t{path}")
            }).collect::<Vec<_>>().join("\n"))
            .unwrap_or_default(),
        Some("work.open") | Some("work.reveal") => {
            let name = data.get("item").and_then(|item| item.get("name")).and_then(Value::as_str).unwrap_or_default();
            let path = data.get("item").and_then(|item| item.get("path")).and_then(Value::as_str).unwrap_or_default();
            format!("{name}\t{path}")
        }
        _ => data.to_string(),
    }
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

pub fn run_cli_with_surface(
    args: &[String],
    environment: &CliEnvironment,
    surface: &mut dyn TerminalSurface,
) -> CliExecution {
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
    let mut registry = create_core_action_registry();
    register_projectcentral_actions(&mut registry);
    let result = match parsed.target {
        CommandTarget::Direct { action_id, input } => registry.execute(&action_id, &input, &context),
        CommandTarget::Guided => run_guided_action_picker(&registry, &context, surface),
    };
    let output = if parsed.structured {
        serde_json::to_string(&result).expect("ActionResult serializes")
    } else {
        human_output(&result)
    };
    CliExecution { exit_code: exit_code(&result), result, output }
}

pub fn run_cli(args: &[String], environment: &CliEnvironment) -> CliExecution {
    let mut surface = NullTerminalSurface;
    run_cli_with_surface(args, environment, &mut surface)
}
