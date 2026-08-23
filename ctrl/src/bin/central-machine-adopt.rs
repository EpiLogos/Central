use central_ctrl::{
    create_core_action_registry, create_default_connector_registry, read_machine_declaration,
    ActionExecutionContext, ConnectorContext, RootOptions, MACHINE_DECLARATION_SCHEMA,
    MACHINE_DECLARATION_VERSION,
};
use serde_json::{json, Value};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const ADOPTION_SCHEMA: &str = "central.machine-adoption/v1";
const DEFAULT_ROLE: &str = "current";
const DEFAULT_WORKCELL_REF: &str = "workcell:local";

#[derive(Debug)]
struct Args {
    root: PathBuf,
    role: String,
    workcell_ref: String,
    json: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BindingChange {
    Added,
    Unchanged,
}

fn main() -> ExitCode {
    match parse_args().and_then(run) {
        Ok(output) => {
            println!("{output}");
            ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("central-machine-adopt: {message}");
            ExitCode::from(2)
        }
    }
}

fn parse_args() -> Result<Args, String> {
    let mut root = None;
    let mut role = DEFAULT_ROLE.to_owned();
    let mut workcell_ref = DEFAULT_WORKCELL_REF.to_owned();
    let mut json = false;
    let args = env::args().skip(1).collect::<Vec<_>>();
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--root" => {
                index += 1;
                root = Some(PathBuf::from(
                    args.get(index)
                        .ok_or_else(|| "--root requires a path".to_owned())?,
                ));
            }
            "--role" => {
                index += 1;
                role = args
                    .get(index)
                    .ok_or_else(|| "--role requires a value".to_owned())?
                    .to_owned();
            }
            "--workcell-ref" => {
                index += 1;
                workcell_ref = args
                    .get(index)
                    .ok_or_else(|| "--workcell-ref requires a value".to_owned())?
                    .to_owned();
            }
            "--json" => json = true,
            "--help" | "-h" => {
                return Err(
                    "usage: central-machine-adopt --root PATH [--role current] [--workcell-ref workcell:local] [--json]"
                        .to_owned(),
                );
            }
            other => return Err(format!("unknown option: {other}")),
        }
        index += 1;
    }

    let root = root.ok_or_else(|| "--root is required".to_owned())?;
    validate_role(&role)?;
    if workcell_ref.trim().is_empty() {
        return Err("--workcell-ref must be non-empty".to_owned());
    }

    Ok(Args {
        root,
        role,
        workcell_ref,
        json,
    })
}

fn run(args: Args) -> Result<String, String> {
    let machines = args.root.join("Control/machines");
    if !machines.is_dir() {
        return Err(format!(
            "Central machine source root is missing: {}; initialise Central first",
            machines.display()
        ));
    }

    let observed = inspect_current_machine(&args.root)?;
    let path = machines.join(format!("{}.json", args.role));
    let relative_path = PathBuf::from("Control/machines").join(format!("{}.json", args.role));

    let (outcome, mut declaration) = if path.exists() {
        read_machine_declaration(&args.root, &args.role)
            .map_err(|error| format!("existing machine declaration is invalid: {}", error.message))?;
        let text = fs::read_to_string(&path)
            .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
        let mut value: Value = serde_json::from_str(&text)
            .map_err(|error| format!("cannot decode {}: {error}", path.display()))?;
        match apply_workcell_binding(&mut value, &args.workcell_ref)? {
            BindingChange::Unchanged => ("unchanged", value),
            BindingChange::Added => {
                write_declaration(&path, &value)?;
                read_machine_declaration(&args.root, &args.role).map_err(|error| {
                    format!("machine declaration failed validation after binding: {}", error.message)
                })?;
                ("bound", value)
            }
        }
    } else {
        let capabilities = observed
            .get("observation")
            .and_then(|value| value.get("capabilities"))
            .cloned()
            .unwrap_or_else(|| json!([]));
        let mut value = json!({
            "schema": MACHINE_DECLARATION_SCHEMA,
            "version": MACHINE_DECLARATION_VERSION,
            "role": args.role.clone(),
            "capabilities": capabilities,
            "requirements": {
                "packages": [],
                "configurations": [],
                "services": []
            },
            "bindings": [
                {
                    "kind": "workcell",
                    "reference": args.workcell_ref.clone()
                }
            ]
        });
        normalize_binding_order(&mut value);
        write_declaration(&path, &value)?;
        read_machine_declaration(&args.root, &args.role).map_err(|error| {
            format!("new machine declaration failed validation: {}", error.message)
        })?;
        ("created", value)
    };

    normalize_binding_order(&mut declaration);
    let payload = json!({
        "ok": true,
        "schema": ADOPTION_SCHEMA,
        "outcome": outcome,
        "role": args.role,
        "workcell_ref": args.workcell_ref,
        "source": {
            "path": relative_path.clone(),
            "source_class": "authored"
        },
        "declaration": declaration,
        "observed": observed
    });

    if args.json {
        serde_json::to_string_pretty(&payload).map_err(|error| error.to_string())
    } else {
        Ok(format!(
            "{}: {} -> {} ({})",
            outcome,
            payload["role"].as_str().unwrap_or(DEFAULT_ROLE),
            payload["workcell_ref"]
                .as_str()
                .unwrap_or(DEFAULT_WORKCELL_REF),
            relative_path.display()
        ))
    }
}

fn inspect_current_machine(root: &Path) -> Result<Value, String> {
    let registry = create_core_action_registry();
    let connectors = create_default_connector_registry();
    let connector_context = ConnectorContext::current();
    let root_options = RootOptions {
        explicit_root: Some(root.to_path_buf()),
        configured_root: None,
        home: None,
    };
    let context = ActionExecutionContext {
        root_options: &root_options,
        connectors: &connectors,
        connector_context: &connector_context,
    };
    let result = registry.execute("machine.inspect", &json!({}), &context);
    if !result.ok {
        return Err(result
            .error
            .map(|error| error.message)
            .unwrap_or_else(|| "machine.inspect failed".to_owned()));
    }
    result
        .data
        .ok_or_else(|| "machine.inspect returned no observation".to_owned())
}

fn validate_role(role: &str) -> Result<(), String> {
    let role = role.trim();
    if role.is_empty()
        || !role
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
        || matches!(role, "." | "..")
    {
        return Err(
            "machine role may contain only letters, digits, '.', '-', and '_' and may not be '.' or '..'"
                .to_owned(),
        );
    }
    Ok(())
}

fn apply_workcell_binding(value: &mut Value, workcell_ref: &str) -> Result<BindingChange, String> {
    let object = value
        .as_object_mut()
        .ok_or_else(|| "machine declaration must be a JSON object".to_owned())?;
    let bindings = object.entry("bindings").or_insert_with(|| json!([]));
    let bindings = bindings
        .as_array_mut()
        .ok_or_else(|| "machine declaration bindings must be an array".to_owned())?;

    let mut exact = false;
    let mut conflicts = Vec::new();
    for binding in bindings.iter() {
        if binding.get("kind").and_then(Value::as_str) != Some("workcell") {
            continue;
        }
        match binding.get("reference").and_then(Value::as_str) {
            Some(reference) if reference == workcell_ref => exact = true,
            Some(reference) => conflicts.push(reference.to_owned()),
            None => conflicts.push("<missing reference>".to_owned()),
        }
    }

    if !conflicts.is_empty() {
        return Err(format!(
            "machine declaration already has a different Workcell binding: {}",
            conflicts.join(", ")
        ));
    }
    if exact {
        return Ok(BindingChange::Unchanged);
    }

    bindings.push(json!({
        "kind": "workcell",
        "reference": workcell_ref
    }));
    Ok(BindingChange::Added)
}

fn normalize_binding_order(value: &mut Value) {
    let Some(bindings) = value.get_mut("bindings").and_then(Value::as_array_mut) else {
        return;
    };
    bindings.sort_by(|left, right| {
        let left = (
            left.get("kind").and_then(Value::as_str).unwrap_or_default(),
            left.get("reference")
                .and_then(Value::as_str)
                .unwrap_or_default(),
        );
        let right = (
            right.get("kind")
                .and_then(Value::as_str)
                .unwrap_or_default(),
            right
                .get("reference")
                .and_then(Value::as_str)
                .unwrap_or_default(),
        );
        left.cmp(&right)
    });
}

fn write_declaration(path: &Path, value: &Value) -> Result<(), String> {
    let mut text = serde_json::to_string_pretty(value).map_err(|error| error.to_string())?;
    text.push('\n');
    fs::write(path, text).map_err(|error| format!("cannot write {}: {error}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn declaration() -> Value {
        json!({
            "schema": "central.machine",
            "version": 1,
            "role": "current",
            "capabilities": [],
            "requirements": {
                "packages": [],
                "configurations": [],
                "services": []
            }
        })
    }

    #[test]
    fn first_binding_is_added() {
        let mut value = declaration();
        assert_eq!(
            apply_workcell_binding(&mut value, "workcell:local").unwrap(),
            BindingChange::Added
        );
        assert_eq!(value["bindings"][0]["kind"], "workcell");
        assert_eq!(value["bindings"][0]["reference"], "workcell:local");
    }

    #[test]
    fn repeated_binding_is_idempotent() {
        let mut value = declaration();
        apply_workcell_binding(&mut value, "workcell:local").unwrap();
        assert_eq!(
            apply_workcell_binding(&mut value, "workcell:local").unwrap(),
            BindingChange::Unchanged
        );
        assert_eq!(value["bindings"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn different_workcell_binding_is_a_conflict() {
        let mut value = declaration();
        apply_workcell_binding(&mut value, "workcell:remote").unwrap();
        let error = apply_workcell_binding(&mut value, "workcell:local").unwrap_err();
        assert!(error.contains("workcell:remote"));
    }
}
