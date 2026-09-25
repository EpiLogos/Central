use crate::action::{create_core_action_registry, ActionExecutionContext};
use crate::agent_profile_actions::register_agent_profile_actions;
use crate::picker::{run_guided_action_picker, NullTerminalSurface, TerminalSurface};
use crate::projectcentral_ops::register_projectcentral_actions;
use crate::result::{ActionResult, ResultStatus};
use crate::root::RootOptions;
use central_connector_sdk::{ConnectorContext, ConnectorRegistry};
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
        let configured_root = env::var_os("CENTRAL_ROOT")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from);
        let home = env::var_os("HOME")
            .filter(|value| !value.is_empty())
            .or_else(|| env::var_os("USERPROFILE").filter(|value| !value.is_empty()))
            .map(PathBuf::from);
        Self {
            configured_root,
            home,
        }
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

/// Shared bounded Action JSON transport for core and native host surfaces.
pub fn parse_action_input(
    structured: bool,
    action_id: &str,
    raw: &str,
) -> Result<Value, (bool, String)> {
    // Explicit '-' carries the same JSON-object input on stdin. Large source
    // and original-form payloads must not depend on OS argv limits. This is
    // only transport: registry, authority and per-operation bounds still apply.
    let from_stdin;
    let raw = if raw == "-" {
        use std::io::Read;
        const MAX_ACTION_INPUT: u64 = 16 * 1024 * 1024;
        let mut bytes = Vec::new();
        std::io::stdin()
            .take(MAX_ACTION_INPUT + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| (structured, format!("read action JSON from stdin: {error}")))?;
        if bytes.len() as u64 > MAX_ACTION_INPUT {
            return Err((
                structured,
                "action stdin JSON exceeds the 16 MiB input bound".to_owned(),
            ));
        }
        from_stdin = String::from_utf8(bytes)
            .map_err(|_| (structured, "action stdin JSON must be UTF-8".to_owned()))?;
        from_stdin.as_str()
    } else {
        raw
    };
    let value: Value = serde_json::from_str(raw).map_err(|error| {
        (
            structured,
            format!("action run input must be a JSON object: {error}"),
        )
    })?;
    if !value.is_object() {
        return Err((
            structured,
            format!("action run input for {action_id} must be a JSON object."),
        ));
    }
    Ok(value)
}

/// Index of the first positional token (skipping `--json` / `--root <path>`),
/// or `None` when every argument is a flag.
fn first_positional(args: &[String]) -> Option<usize> {
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--json" => index += 1,
            "--root" => {
                args.get(index + 1)?;
                index += 2;
            }
            argument if argument.starts_with("--root=") => index += 1,
            _ => return Some(index),
        }
    }
    None
}

/// True when the invocation addresses the Configuration Plane family, so
/// usage failures answer with `oi.config-error/v1` instead of an envelope.
fn config_family_involved(args: &[String]) -> bool {
    matches!(
        first_positional(args).map(|index| args[index].as_str()),
        Some("config") | Some("config-contribution")
    )
}

/// The frozen four-verb owner mutation transport plus the contribution
/// command (09-CONFIGURATION-PLANE.md §4/§6):
///
/// ```text
/// ctrl config-contribution --json
/// ctrl config validate --json --setting <ref> [--scope <kind:ref>]
///           (--value <json> | --value-file <path|->)
/// ctrl config plan     --json ...   (same value flags as validate)
/// ctrl config apply    --json (--plan-file <path|->) [--changeset <id>]
/// ctrl config reset    --json --setting <ref> [--scope <compact>]
///           [--changeset <id>]
/// ```
fn parse_config_family(args: &[String]) -> Result<ParsedCommand, (bool, String)> {
    let mut structured = false;
    let mut explicit_root = None;
    let mut positional: Vec<String> = Vec::new();
    let mut setting: Option<String> = None;
    let mut scope: Option<String> = None;
    let mut value: Option<String> = None;
    let mut value_file: Option<String> = None;
    let mut plan_file: Option<String> = None;
    let mut changeset: Option<String> = None;

    let mut index = 0;
    while index < args.len() {
        let argument = args[index].clone();
        let takes_value = |flag: &str, name: &str| -> Result<String, (bool, String)> {
            let next = args
                .get(index + 1)
                .filter(|next| !next.starts_with("--") || next.as_str() == "-");
            match next {
                Some(next) => Ok(next.clone()),
                None => Err((structured, format!("{flag} requires a {name}."))),
            }
        };
        match argument.as_str() {
            "--json" => structured = true,
            "--root" => {
                let path = takes_value("--root", "path")?;
                explicit_root = Some(PathBuf::from(path));
                index += 1;
            }
            "--setting" => {
                setting = Some(takes_value("--setting", "setting ref")?);
                index += 1;
            }
            "--scope" => {
                scope = Some(takes_value("--scope", "scope")?);
                index += 1;
            }
            "--value" => {
                value = Some(takes_value("--value", "JSON value")?);
                index += 1;
            }
            "--value-file" => {
                value_file = Some(takes_value("--value-file", "path or -")?);
                index += 1;
            }
            "--plan-file" => {
                plan_file = Some(takes_value("--plan-file", "path or -")?);
                index += 1;
            }
            "--changeset" => {
                changeset = Some(takes_value("--changeset", "changeset id")?);
                index += 1;
            }
            other if other.starts_with("--root=") => {
                let path = other.strip_prefix("--root=").unwrap_or_default();
                if path.is_empty() {
                    return Err((structured, "--root requires a path.".to_owned()));
                }
                explicit_root = Some(PathBuf::from(path));
            }
            other if other.starts_with("--") => {
                return Err((structured, format!("Unknown option: {other}")));
            }
            other => positional.push(other.to_owned()),
        }
        index += 1;
    }

    if positional.first().map(String::as_str) != Some("config") {
        return Err((structured, "Unknown command.".to_owned()));
    }
    let Some(verb) = positional.get(1).map(String::as_str) else {
        return Err((
            structured,
            "config requires a verb: contribution | validate | plan | apply | reset.".to_owned(),
        ));
    };
    if positional.len() > 2 {
        return Err((
            structured,
            format!("config {verb} takes no positional arguments."),
        ));
    }

    let (action_id, input): (&str, Value) = match verb {
        "contribution" => (crate::configuration::CONFIG_CONTRIBUTION_ACTION, json!({})),
        "validate" | "plan" => {
            let Some(setting_ref) = setting.as_ref() else {
                return Err((
                    structured,
                    format!("config {verb} requires --setting <setting_ref>."),
                ));
            };
            let scope_value = scope.clone().map(Value::String).unwrap_or(Value::Null);
            match (&value, &value_file) {
                (Some(raw), None) => {
                    let parsed: Value = serde_json::from_str(raw).map_err(|error| {
                        (
                            structured,
                            format!("--value must be a JSON document: {error}"),
                        )
                    })?;
                    (
                        if verb == "validate" {
                            crate::configuration::CONFIG_VALIDATE_ACTION
                        } else {
                            crate::configuration::CONFIG_PLAN_ACTION
                        },
                        json!({
                            "setting_ref": setting_ref,
                            "scope": scope_value,
                            "value": parsed,
                            "value_file": Value::Null,
                        }),
                    )
                }
                (None, Some(file)) => (
                    if verb == "validate" {
                        crate::configuration::CONFIG_VALIDATE_ACTION
                    } else {
                        crate::configuration::CONFIG_PLAN_ACTION
                    },
                    json!({
                        "setting_ref": setting_ref,
                        "scope": scope_value,
                        "value": Value::Null,
                        "value_file": file,
                    }),
                ),
                (Some(_), Some(_)) => {
                    return Err((
                        structured,
                        "pass either --value or --value-file, not both.".to_owned(),
                    ))
                }
                (None, None) => {
                    return Err((
                        structured,
                        format!(
                        "config {verb} requires a value: --value <json> or --value-file <path|->."
                    ),
                    ))
                }
            }
        }
        "apply" => {
            let Some(source) = plan_file.as_ref() else {
                return Err((
                    structured,
                    "config apply requires --plan-file <path|->.".to_owned(),
                ));
            };
            (
                crate::configuration::CONFIG_APPLY_ACTION,
                json!({
                    "plan_file": source,
                    "changeset": changeset.clone().map(Value::String).unwrap_or(Value::Null),
                }),
            )
        }
        "reset" => {
            let Some(setting_ref) = setting.as_ref() else {
                return Err((
                    structured,
                    "config reset requires --setting <setting_ref>.".to_owned(),
                ));
            };
            (
                crate::configuration::CONFIG_RESET_ACTION,
                json!({
                    "setting_ref": setting_ref,
                    "scope": scope.clone().map(Value::String).unwrap_or(Value::Null),
                    "changeset": changeset.clone().map(Value::String).unwrap_or(Value::Null),
                }),
            )
        }
        other => {
            return Err((
                structured,
                format!(
                "Unknown config verb: {other} (contribution | validate | plan | apply | reset)."
            ),
            ))
        }
    };

    Ok(ParsedCommand {
        structured,
        explicit_root,
        target: CommandTarget::Direct {
            action_id: action_id.to_owned(),
            input,
        },
    })
}

fn parse_args(args: &[String]) -> Result<ParsedCommand, (bool, String)> {
    // The Configuration Plane family (`ctrl config <verb> ...`) carries its
    // own flag grammar (`--setting`, `--scope`, `--value`, ...); it is parsed
    // by its own parser before the generic Action scan rejects those flags.
    if first_positional(args).map(|index| args[index].as_str()) == Some("config") {
        return parse_config_family(args);
    }

    let mut structured = false;
    let mut explicit_root = None;
    let mut projection_reading_path: Option<PathBuf> = None;
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
        } else if argument == "--projection-reading" {
            // An optional AIKit worktree-projection reading file (JSON), read by
            // ctrl and passed through to `machine plan` / `machine verify` as the
            // `projection_reading` input. ctrl reads this caller-supplied file; it
            // never runs git or derives the projection itself.
            index += 1;
            let Some(value) = args.get(index) else {
                return Err((
                    structured,
                    "--projection-reading requires a path.".to_owned(),
                ));
            };
            if value.starts_with("--") {
                return Err((
                    structured,
                    "--projection-reading requires a path.".to_owned(),
                ));
            }
            projection_reading_path = Some(PathBuf::from(value));
        } else if let Some(value) = argument.strip_prefix("--projection-reading=") {
            if value.is_empty() {
                return Err((
                    structured,
                    "--projection-reading requires a path.".to_owned(),
                ));
            }
            projection_reading_path = Some(PathBuf::from(value));
        } else if argument.starts_with("--") {
            return Err((
                structured,
                format!("Unknown option: {argument}; run `ctrl --help`"),
            ));
        } else {
            positional.push(argument.clone());
        }
        index += 1;
    }

    if positional.is_empty() {
        return Err((structured, "An Action or command is required.".to_owned()));
    }
    if positional.as_slice() == ["pick"] {
        return Ok(ParsedCommand {
            structured,
            explicit_root,
            target: CommandTarget::Guided,
        });
    }
    if positional.first().map(String::as_str) == Some("pick") {
        return Err((structured, "pick takes no positional input.".to_owned()));
    }

    let (action_id, mut input): (&str, Value) = match positional.as_slice() {
        [command] if command == "root" => ("central.root", json!({})),
        [command] if command == "init" => ("central.init", json!({})),
        [command] if command == "doctor" => ("central.doctor", json!({})),
        [command] if command == "world" => ("central.world", json!({})),
        [command, rest @ ..] if command == "world" && rest.len() == 1 && rest[0] == "plan" => {
            return Err((structured, "world plan requires a Project name.".to_owned()));
        }
        [command, rest @ ..] if command == "world" && rest.len() == 1 && rest[0] == "apply" => {
            return Err((
                structured,
                "world apply requires a Project name.".to_owned(),
            ));
        }
        [command, rest @ ..]
            if command == "world"
                && rest.first().map(String::as_str) == Some("plan")
                && rest.len() == 2 =>
        {
            (
                "central.world.reproject.plan",
                json!({ "project": rest[1] }),
            )
        }
        [command, rest @ ..]
            if command == "world"
                && rest.first().map(String::as_str) == Some("apply")
                && rest.len() == 2 =>
        {
            (
                "central.world.reproject.apply",
                json!({ "project": rest[1] }),
            )
        }
        [command, rest @ ..] if command == "world" && rest.len() == 1 => {
            ("central.world.project", json!({ "project": rest[0] }))
        }
        [command] if command == "actions" => ("action.list", json!({})),
        [command] if command == "system" => ("central.system", json!({})),
        [command] if command == "config-contribution" => {
            (crate::configuration::CONFIG_CONTRIBUTION_ACTION, json!({}))
        }
        [domain, verb] if domain == "action" && verb == "list" => ("action.list", json!({})),
        [domain, verb, action] if domain == "action" && verb == "run" => {
            (action.as_str(), json!({}))
        }
        [domain, verb, action, raw] if domain == "action" && verb == "run" => (
            action.as_str(),
            parse_action_input(structured, action, raw)?,
        ),
        [domain, verb] if domain == "action" && verb == "run" => {
            return Err((structured, "action run requires an Action id.".to_owned()));
        }
        [domain, verb, ..] if domain == "action" && verb == "run" => {
            return Err((
                structured,
                "action run accepts one Action id and at most one JSON object argument.".to_owned(),
            ));
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
        [domain, verb]
            if domain == "work" && matches!(verb.as_str(), "search" | "open" | "reveal") =>
        {
            return Err((structured, format!("work {verb} requires a query.")));
        }
        [domain, verb] if domain == "git" && verb == "census" => {
            ("central.git.census", json!({ "format": "list" }))
        }
        [domain, verb, project] if domain == "git" && verb == "census" => (
            "central.git.census",
            json!({ "format": "list", "project": project }),
        ),
        [domain, verb] if domain == "git" && verb == "tree" => {
            ("central.git.census", json!({ "format": "tree" }))
        }
        [domain, verb, project] if domain == "git" && verb == "tree" => (
            "central.git.census",
            json!({ "format": "tree", "project": project }),
        ),
        [domain, verb] if domain == "git" && verb == "graph" => {
            ("central.git.census", json!({ "format": "graph" }))
        }
        [domain, verb, project] if domain == "git" && verb == "graph" => (
            "central.git.census",
            json!({ "format": "graph", "project": project }),
        ),
        [domain, ..] if domain == "git" => {
            return Err((
                structured,
                "git takes census, tree or graph, with an optional Project name.".to_owned(),
            ));
        }
        [command] if command == "open" => {
            return Err((
                structured,
                "open requires a Work name or search.".to_owned(),
            ));
        }
        [domain, verb, target] if domain == "control" && verb == "open" => {
            ("control.open", json!({ "target": target }))
        }
        [domain, verb, rest @ ..]
            if domain == "control" && verb == "search" && !rest.is_empty() =>
        {
            ("control.search", json!({ "query": rest.join(" ") }))
        }
        [domain, verb] if domain == "control" && verb == "index" => ("control.index", json!({})),
        [domain, verb] if domain == "control" && verb == "open" => {
            return Err((
                structured,
                "control open requires one Control root.".to_owned(),
            ));
        }
        [domain, verb] if domain == "control" && verb == "search" => {
            return Err((structured, "control search requires a query.".to_owned()));
        }
        [domain, verb]
            if domain == "machine"
                && matches!(verb.as_str(), "inspect" | "account" | "adopt-current") =>
        {
            let action = match verb.as_str() {
                "inspect" => "machine.inspect",
                "account" => "machine.account",
                _ => "machine.adopt-current",
            };
            (action, json!({}))
        }
        [domain, verb, role]
            if domain == "machine"
                && matches!(
                    verb.as_str(),
                    "declaration" | "plan" | "apply" | "verify" | "adopt-current"
                ) =>
        {
            let action = match verb.as_str() {
                "declaration" => "machine.declaration",
                "plan" => "machine.plan",
                "apply" => "machine.apply",
                "verify" => "machine.verify",
                _ => "machine.adopt-current",
            };
            (action, json!({ "role": role }))
        }
        [domain, verb]
            if domain == "machine"
                && matches!(verb.as_str(), "declaration" | "plan" | "apply" | "verify") =>
        {
            return Err((structured, format!("machine {verb} requires a role.")));
        }
        [domain, verb, role] if domain == "recovery" && verb == "plan" => {
            ("central.recovery.plan", json!({ "role": role }))
        }
        [domain, verb] if domain == "recovery" && verb == "plan" => {
            return Err((structured, "recovery plan requires a role.".to_owned()));
        }
        [command, role] if command == "recover" => ("central.recover", json!({ "role": role })),
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
        [canonical]
            if matches!(
                canonical.as_str(),
                "work.search" | "work.open" | "work.reveal"
            ) =>
        {
            return Err((structured, format!("{canonical} requires a query.")));
        }
        [canonical, target] if canonical == "control.open" => {
            ("control.open", json!({ "target": target }))
        }
        [canonical, rest @ ..] if canonical == "control.search" && !rest.is_empty() => {
            ("control.search", json!({ "query": rest.join(" ") }))
        }
        [canonical] if canonical == "control.open" => {
            return Err((
                structured,
                "control.open requires one Control root.".to_owned(),
            ));
        }
        [canonical] if canonical == "control.search" => {
            return Err((structured, "control.search requires a query.".to_owned()));
        }
        [canonical, role]
            if matches!(
                canonical.as_str(),
                "machine.declaration"
                    | "machine.plan"
                    | "machine.apply"
                    | "machine.verify"
                    | "central.recovery.plan"
                    | "central.recover"
            ) =>
        {
            (canonical.as_str(), json!({ "role": role }))
        }
        [canonical]
            if matches!(
                canonical.as_str(),
                "machine.declaration"
                    | "machine.plan"
                    | "machine.apply"
                    | "machine.verify"
                    | "central.recovery.plan"
                    | "central.recover"
            ) =>
        {
            return Err((structured, format!("{canonical} requires a role.")));
        }
        [canonical]
            if matches!(
                canonical.as_str(),
                "central.root"
                    | "central.init"
                    | "central.doctor"
                    | "central.world"
                    | "central.system"
                    | "central.world.project"
                    | "central.world.reproject.plan"
                    | "central.world.reproject.apply"
                    | "action.list"
                    | "machine.inspect"
                    | "machine.account"
                    | "machine.adopt-current"
                    | "work.list"
            ) =>
        {
            (canonical.as_str(), json!({}))
        }
        [unknown] => {
            return Err((
                structured,
                format!("Unknown command: {unknown}; run `ctrl --help`"),
            ))
        }
        _ => {
            return Err((
                structured,
                format!("Unexpected arguments: {}", positional[1..].join(" ")),
            ));
        }
    };

    if let Some(path) = projection_reading_path {
        if action_id != "machine.plan" && action_id != "machine.verify" {
            return Err((
                structured,
                "--projection-reading applies only to `machine plan` and `machine verify`."
                    .to_owned(),
            ));
        }
        let text = std::fs::read_to_string(&path).map_err(|error| {
            (
                structured,
                format!(
                    "--projection-reading cannot read {}: {error}",
                    path.display()
                ),
            )
        })?;
        let reading: Value = serde_json::from_str(&text).map_err(|error| {
            (
                structured,
                format!(
                    "--projection-reading {} is not valid JSON: {error}",
                    path.display()
                ),
            )
        })?;
        if let Some(object) = input.as_object_mut() {
            object.insert("projection_reading".to_owned(), reading);
        }
    }

    Ok(ParsedCommand {
        structured,
        explicit_root,
        target: CommandTarget::Direct {
            action_id: action_id.to_owned(),
            input,
        },
    })
}

fn human_output(result: &ActionResult) -> String {
    if !result.ok {
        // Configuration-plane failures carry the bare oi.config-error/v1
        // document in `data` (no ActionError); render that document's code
        // and message instead of expecting an envelope error.
        if result
            .action
            .as_deref()
            .is_some_and(|action| action.starts_with("central.config."))
        {
            let data = result.data.as_ref();
            let code = data
                .and_then(|d| d.get("error_code"))
                .and_then(Value::as_str)
                .unwrap_or(result.status.as_str());
            let message = data
                .and_then(|d| d.get("message"))
                .and_then(Value::as_str)
                .unwrap_or_default();
            return format!("{code}: {message}");
        }
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
            let source = data
                .get("source")
                .and_then(Value::as_str)
                .unwrap_or("unknown");
            format!("{path} ({source})")
        }
        Some("central.init") => {
            let root = data.get("root").and_then(Value::as_str).unwrap_or_default();
            format!("Initialized Central at {root}")
        }
        Some("central.doctor") => {
            let root = data.get("root").and_then(Value::as_str).unwrap_or_default();
            let valid = data.get("valid").and_then(Value::as_bool).unwrap_or(false);
            let mut lines = vec![
                format!("Central root: {root}"),
                format!("Valid: {}", if valid { "yes" } else { "no" }),
            ];
            if let Some(checks) = data.get("checks").and_then(Value::as_array) {
                for check in checks {
                    let path = check
                        .get("path")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    let ok = check.get("valid").and_then(Value::as_bool).unwrap_or(false);
                    lines.push(format!("{}  {path}", if ok { "ok" } else { "missing" }));
                }
            }
            if let Some(mixed) = data.get("mixed_root") {
                if mixed
                    .get("detected")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
                {
                    let message = mixed.get("message").and_then(Value::as_str).unwrap_or(
                        "Central personal root is also the Central product source checkout.",
                    );
                    lines.push(format!("warning: {message}"));
                }
            }
            lines.join("\n")
        }
        Some("central.world") => crate::world_map::explain_world_map(data),
        Some("central.world.project") => crate::world_map::explain_project_world_map(data),
        Some("central.world.reproject.plan") => crate::world_map::explain_reproject_plan(data),
        Some("central.world.reproject.apply") => crate::world_map::explain_reproject_receipt(data),
        Some("central.git.census") => {
            if let Some(render) = data.get("render").and_then(Value::as_str) {
                render.to_owned()
            } else {
                let summary = data.get("summary");
                let field = |name: &str| {
                    summary
                        .and_then(|s| s.get(name))
                        .and_then(Value::as_u64)
                        .unwrap_or(0)
                };
                format!(
                    "census: repos={} worktrees={} branches={} local_only={} unattributed={} attention={}",
                    field("repos_censused"),
                    field("worktrees"),
                    field("branches"),
                    field("local_only_branches"),
                    field("unattributed_worktrees"),
                    field("attention_items"),
                )
            }
        }
        Some("central.recovery.plan") => crate::recovery::explain_recovery_plan(data),
        Some("central.recover") => crate::recovery::explain_recovery(data),
        Some("action.list") => data
            .get("actions")
            .and_then(Value::as_array)
            .map(|actions| {
                actions
                    .iter()
                    .map(|action| {
                        let id = action.get("id").and_then(Value::as_str).unwrap_or_default();
                        let title = action
                            .get("title")
                            .and_then(Value::as_str)
                            .unwrap_or_default();
                        format!("{id}\t{title}")
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_default(),
        Some(crate::system_disclosure::SYSTEM_ACTION_ID) => {
            let mut lines = Vec::new();
            let about = data
                .get("about")
                .and_then(Value::as_str)
                .unwrap_or_default();
            lines.push(about.to_owned());
            if let Some(availability) = data.get("availability") {
                if let Some(state) = availability.get("state").and_then(Value::as_str) {
                    lines.push(format!("availability: {state}"));
                }
            }
            if let Some(sections) = data.get("sections").and_then(Value::as_array) {
                for section in sections {
                    let id = section
                        .get("id")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    let title = section
                        .get("title")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    let settings = section
                        .get("settings")
                        .and_then(Value::as_array)
                        .map(|s| s.len())
                        .unwrap_or_default();
                    lines.push(format!("section {id} ({title}): {settings} setting(s)"));
                }
            }
            if let Some(actions) = data.get("actions").and_then(Value::as_array) {
                lines.push(format!("disclosed actions: {}", actions.len()));
            }
            lines.join("\n")
        }
        Some("control.open") => {
            let target = data
                .get("target")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let path = data.get("path").and_then(Value::as_str).unwrap_or_default();
            format!("{target}\t{path}")
        }
        Some("control.search") => data
            .get("matches")
            .and_then(Value::as_array)
            .map(|matches| {
                matches
                    .iter()
                    .map(|item| {
                        let path = item
                            .get("source_path")
                            .and_then(Value::as_str)
                            .unwrap_or_default();
                        let line = item.get("line").and_then(Value::as_u64).unwrap_or_default();
                        let text = item.get("text").and_then(Value::as_str).unwrap_or_default();
                        format!("{path}:{line}\t{text}")
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_default(),
        Some("machine.declaration") => crate::machine::explain_machine_declaration(data),
        Some("machine.inspect") => crate::machine::explain_machine_inspection(data),
        Some("machine.account") => crate::machine_account::explain_account(data),
        Some("machine.adopt-current") => crate::machine::explain_machine_adoption(data),
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
            let mut lines = vec![format!("Work provider: {selected}")];
            if let Some(items) = data.get("items").and_then(Value::as_array) {
                lines.extend(items.iter().map(|item| {
                    let name = item.get("name").and_then(Value::as_str).unwrap_or_default();
                    let path = item.get("path").and_then(Value::as_str).unwrap_or_default();
                    format!("{name}\t{path}")
                }));
            }
            lines.join("\n")
        }
        Some("work.search") => data
            .get("matches")
            .and_then(Value::as_array)
            .map(|matches| {
                matches
                    .iter()
                    .map(|item| {
                        let name = item.get("name").and_then(Value::as_str).unwrap_or_default();
                        let path = item.get("path").and_then(Value::as_str).unwrap_or_default();
                        format!("{name}\t{path}")
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_default(),
        Some("work.open") | Some("work.reveal") => {
            let name = data
                .get("item")
                .and_then(|item| item.get("name"))
                .and_then(Value::as_str)
                .unwrap_or_default();
            let path = data
                .get("item")
                .and_then(|item| item.get("path"))
                .and_then(Value::as_str)
                .unwrap_or_default();
            format!("{name}\t{path}")
        }
        Some(crate::configuration::CONFIG_CONTRIBUTION_ACTION) => {
            let sections = data
                .get("sections")
                .and_then(Value::as_array)
                .map(|sections| sections.len())
                .unwrap_or_default();
            let settings = data
                .get("sections")
                .and_then(Value::as_array)
                .map(|sections| {
                    sections
                        .iter()
                        .filter_map(|s| s.get("settings").and_then(Value::as_array))
                        .map(Vec::len)
                        .sum::<usize>()
                })
                .unwrap_or_default();
            let availability = data
                .pointer("/availability/state")
                .and_then(Value::as_str)
                .unwrap_or("unknown");
            format!(
                "configuration contribution: {sections} section(s), {settings} setting(s), availability {availability}"
            )
        }
        Some(crate::configuration::CONFIG_VALIDATE_ACTION) => {
            let valid = data.get("valid").and_then(Value::as_bool).unwrap_or(false);
            let violations = data
                .get("violations")
                .and_then(Value::as_array)
                .map(Vec::len)
                .unwrap_or_default();
            if valid {
                "valid: yes".to_owned()
            } else {
                format!("valid: no ({violations} violation(s))")
            }
        }
        Some(crate::configuration::CONFIG_PLAN_ACTION) => {
            let plan_id = data
                .get("plan_id")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let digest = data
                .get("plan_digest")
                .and_then(Value::as_str)
                .map(|digest| digest.chars().take(12).collect::<String>())
                .unwrap_or_default();
            let changes = data
                .get("changes")
                .and_then(Value::as_array)
                .map(Vec::len)
                .unwrap_or_default();
            format!("plan {plan_id} (digest {digest}): {changes} change(s)")
        }
        Some(action)
            if action == crate::configuration::CONFIG_APPLY_ACTION
                || action == crate::configuration::CONFIG_RESET_ACTION =>
        {
            let receipt_id = data
                .get("receipt_id")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let outcome = data
                .get("outcome")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let original = data
                .get("original_receipt_id")
                .and_then(Value::as_str)
                .map(|original| format!(" (original receipt {original})"))
                .unwrap_or_default();
            format!("receipt {receipt_id}: {outcome}{original}")
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

pub fn run_cli_with_runtime(
    args: &[String],
    environment: &CliEnvironment,
    surface: &mut dyn TerminalSurface,
    connectors: &ConnectorRegistry,
    connector_context: &ConnectorContext,
) -> CliExecution {
    let config_family = config_family_involved(args);
    let parsed = match parse_args(args) {
        Ok(parsed) => parsed,
        Err((structured, message)) => {
            let result =
                ActionResult::failure(None, ResultStatus::InvalidInput, message.clone(), None);
            // Configuration-plane usage failures answer with the contract
            // error document, not an Action envelope (09 §6).
            let output = if config_family {
                let document = serde_json::json!({
                    "schema": crate::configuration::CONFIG_ERROR_SCHEMA,
                    "error_code": "validation_failed",
                    "message": message,
                    "setting_ref": Value::Null,
                    "scope_kind": Value::Null,
                    "retryable": false,
                    "detail_ref": Value::Null,
                });
                if structured {
                    document.to_string()
                } else {
                    format!("validation_failed: {message}")
                }
            } else if structured {
                serde_json::to_string(&result).expect("ActionResult serializes")
            } else {
                human_output(&result)
            };
            return CliExecution {
                exit_code: exit_code(&result),
                result,
                output,
            };
        }
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
    let registry = create_runtime_action_registry();
    let result = match parsed.target {
        CommandTarget::Direct { action_id, input } => {
            registry.execute(&action_id, &input, &context)
        }
        CommandTarget::Guided => run_guided_action_picker(&registry, &context, surface),
    };
    let config_action = result
        .action
        .as_deref()
        .map(|action| action.starts_with("central.config."))
        .unwrap_or(false);
    let output = if parsed.structured {
        // Configuration-plane verbs always answer with the bare contract
        // document (contribution, validation, plan, receipt or error) on
        // stdout — never the ActionResult envelope (09 §6).
        if config_action {
            match result.data.as_ref() {
                Some(data) => serde_json::to_string(data).expect("config document serializes"),
                None => serde_json::to_string(&result).expect("ActionResult serializes"),
            }
        // The Wave 5 System disclosure is itself the output document: `ctrl system
        // --json` returns the bare descriptor on stdout (the O:I composition kernel
        // mount seam), not the ActionResult envelope.
        } else if result.action.as_deref() == Some(crate::system_disclosure::SYSTEM_ACTION_ID)
            && result.ok
        {
            match result.data.as_ref() {
                Some(data) => serde_json::to_string(data).expect("disclosure serializes"),
                None => serde_json::to_string(&result).expect("ActionResult serializes"),
            }
        } else {
            serde_json::to_string(&result).expect("ActionResult serializes")
        }
    } else {
        human_output(&result)
    };
    CliExecution {
        exit_code: exit_code(&result),
        result,
        output,
    }
}

pub fn run_cli_with_surface(
    args: &[String],
    environment: &CliEnvironment,
    surface: &mut dyn TerminalSurface,
) -> CliExecution {
    let connectors = create_default_connector_registry();
    let connector_context = ConnectorContext::current();
    run_cli_with_runtime(args, environment, surface, &connectors, &connector_context)
}

pub fn run_cli(args: &[String], environment: &CliEnvironment) -> CliExecution {
    let mut surface = NullTerminalSurface;
    run_cli_with_surface(args, environment, &mut surface)
}

/// One runtime Action catalogue shared by ordinary and native host surfaces.
pub fn create_runtime_action_registry() -> crate::action::ActionRegistry {
    let mut registry = create_core_action_registry();
    register_projectcentral_actions(&mut registry);
    crate::template_stamp::register_template_stamp_actions(&mut registry);
    crate::engineering_ground::register_engineering_ground_actions(&mut registry);
    register_agent_profile_actions(&mut registry);
    crate::agent_set_actions::register_agent_set_actions(&mut registry);
    crate::remember_actions::register_remember_actions(&mut registry);
    crate::system_disclosure::register_system_disclosure_action(&mut registry);
    crate::git_census::register_git_actions(&mut registry);
    crate::configuration::register_configuration_actions(&mut registry);
    registry
}
