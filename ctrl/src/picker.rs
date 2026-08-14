use crate::action::{
    ActionDescriptor, ActionExecutionContext, ActionInputDefinition, ActionRegistry, MutationClass,
};
use crate::result::ActionResult;
use serde_json::{Map, Value};
use std::cmp::Ordering;
use std::io::{self, Write};

pub trait TerminalSurface {
    fn write_line(&mut self, line: &str);
    fn prompt(&mut self, message: &str) -> Option<String>;
}

#[derive(Default)]
pub struct StdioTerminalSurface;

impl TerminalSurface for StdioTerminalSurface {
    fn write_line(&mut self, line: &str) {
        eprintln!("{line}");
    }

    fn prompt(&mut self, message: &str) -> Option<String> {
        eprint!("{message}");
        let _ = io::stderr().flush();
        let mut input = String::new();
        match io::stdin().read_line(&mut input) {
            Ok(0) | Err(_) => None,
            Ok(_) => Some(input.trim_end().to_owned()),
        }
    }
}

#[derive(Default)]
pub struct NullTerminalSurface;

impl TerminalSurface for NullTerminalSurface {
    fn write_line(&mut self, _line: &str) {}

    fn prompt(&mut self, _message: &str) -> Option<String> {
        None
    }
}

enum Flow<T> {
    Value(T),
    Back,
    Cancelled(ActionResult),
}

fn cancel(action: Option<&str>) -> ActionResult {
    ActionResult::cancelled(action, "Guided Action selection cancelled.")
}

fn action_score(action: &ActionDescriptor, query: &str) -> Option<u8> {
    if !action.availability.available {
        return None;
    }
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return Some(5);
    }
    let id = action.id.to_lowercase();
    let title = action.title.to_lowercase();
    let description = action.description.to_lowercase();
    if id == query {
        Some(0)
    } else if id.starts_with(&query) {
        Some(1)
    } else if id.contains(&query) {
        Some(2)
    } else if title.contains(&query) {
        Some(3)
    } else if description.contains(&query) {
        Some(4)
    } else {
        None
    }
}

pub fn search_action_descriptors(registry: &ActionRegistry, query: &str) -> Vec<ActionDescriptor> {
    let mut actions = registry
        .list()
        .into_iter()
        .filter_map(|action| action_score(&action, query).map(|score| (score, action)))
        .collect::<Vec<_>>();
    actions.sort_by(|(left_score, left), (right_score, right)| {
        left_score
            .cmp(right_score)
            .then_with(|| left.id.cmp(&right.id))
            .then(Ordering::Equal)
    });
    actions.into_iter().map(|(_, action)| action).collect()
}

fn select_action(registry: &ActionRegistry, surface: &mut dyn TerminalSurface) -> Flow<ActionDescriptor> {
    loop {
        let Some(raw_query) = surface.prompt("Search Actions (/cancel): ") else {
            return Flow::Cancelled(cancel(None));
        };
        if raw_query.trim() == "/cancel" {
            return Flow::Cancelled(cancel(None));
        }
        let actions = search_action_descriptors(registry, &raw_query);
        if actions.is_empty() {
            surface.write_line("No Actions match that search.");
            continue;
        }
        surface.write_line("Matching Actions:");
        for (index, action) in actions.iter().enumerate() {
            surface.write_line(&format!(
                "{}. {} — {} [{}]",
                index + 1,
                action.id,
                action.title,
                action.mutation_class.as_str(),
            ));
        }

        loop {
            let Some(raw) = surface.prompt("Select Action by number or id (/back, /cancel): ") else {
                return Flow::Cancelled(cancel(None));
            };
            let choice = raw.trim();
            if choice == "/cancel" {
                return Flow::Cancelled(cancel(None));
            }
            if choice == "/back" {
                break;
            }
            if let Ok(number) = choice.parse::<usize>() {
                if let Some(action) = number.checked_sub(1).and_then(|index| actions.get(index)) {
                    return Flow::Value(action.clone());
                }
            }
            if let Some(action) = actions.iter().find(|action| action.id == choice) {
                return Flow::Value(action.clone());
            }
            surface.write_line("Unknown Action selection.");
        }
    }
}

fn dynamic_choices(
    input: &ActionInputDefinition,
    registry: &ActionRegistry,
    context: &ActionExecutionContext<'_>,
    surface: &mut dyn TerminalSurface,
) -> Vec<String> {
    let Some(selection) = &input.selection else {
        return Vec::new();
    };
    let Some(source) = registry.get(&selection.action) else {
        surface.write_line(&format!("Selection source Action is unavailable: {}", selection.action));
        return Vec::new();
    };
    if source.mutation_class != MutationClass::ReadOnly || source.inputs.iter().any(|candidate| candidate.required) {
        surface.write_line(&format!("Selection source Action is not safe for guided discovery: {}", selection.action));
        return Vec::new();
    }
    let result = registry.execute(&selection.action, &Value::Object(Map::new()), context);
    if !result.ok {
        let message = result.error.as_ref().map(|error| error.message.as_str()).unwrap_or("selection source failed");
        surface.write_line(&format!("Selection source unavailable: {message}"));
        return Vec::new();
    }
    let Some(items) = result
        .data
        .as_ref()
        .and_then(|data| data.get(&selection.collection))
        .and_then(Value::as_array)
    else {
        surface.write_line("Selection source did not return the declared collection.");
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| item.get(&selection.value_field).and_then(Value::as_str).map(str::to_owned))
        .collect()
}

fn prompt_input(
    input: &ActionInputDefinition,
    registry: &ActionRegistry,
    context: &ActionExecutionContext<'_>,
    surface: &mut dyn TerminalSurface,
) -> Flow<Value> {
    let static_choices = input.choices.clone().unwrap_or_default();
    let dynamic = dynamic_choices(input, registry, context, surface);
    let displayed = if !static_choices.is_empty() { static_choices } else { dynamic };

    if !displayed.is_empty() {
        surface.write_line(&format!("Choices for {}:", input.name));
        for (index, value) in displayed.iter().enumerate() {
            surface.write_line(&format!("{}. {value}", index + 1));
        }
    }

    loop {
        let Some(raw) = surface.prompt(&format!("{} (/back, /cancel): ", input.name)) else {
            return Flow::Cancelled(cancel(None));
        };
        let value = raw.trim();
        if value == "/back" {
            return Flow::Back;
        }
        if value == "/cancel" {
            return Flow::Cancelled(cancel(None));
        }
        if value.is_empty() && input.required {
            surface.write_line("A value is required.");
            continue;
        }

        let selected = if let Ok(number) = value.parse::<usize>() {
            number.checked_sub(1).and_then(|index| displayed.get(index)).cloned()
        } else {
            None
        };
        let text = selected.unwrap_or_else(|| value.to_owned());
        if !displayed.is_empty() && !displayed.iter().any(|choice| choice == &text) {
            surface.write_line("Choose one of the available values.");
            continue;
        }

        return match input.input_type.as_str() {
            "string" => Flow::Value(Value::String(text)),
            "boolean" => match text.to_lowercase().as_str() {
                "true" | "yes" | "y" => Flow::Value(Value::Bool(true)),
                "false" | "no" | "n" => Flow::Value(Value::Bool(false)),
                _ => {
                    surface.write_line("Enter true/false or yes/no.");
                    continue;
                }
            },
            other => {
                surface.write_line(&format!("Unsupported guided input type: {other}"));
                return Flow::Cancelled(cancel(None));
            }
        };
    }
}

fn collect_input(
    action: &ActionDescriptor,
    registry: &ActionRegistry,
    context: &ActionExecutionContext<'_>,
    surface: &mut dyn TerminalSurface,
) -> Flow<Value> {
    let mut input = Map::new();
    for definition in &action.inputs {
        match prompt_input(definition, registry, context, surface) {
            Flow::Value(value) => {
                input.insert(definition.name.clone(), value);
            }
            Flow::Back => return Flow::Back,
            Flow::Cancelled(result) => return Flow::Cancelled(result),
        }
    }
    Flow::Value(Value::Object(input))
}

fn confirm_mutation(action: &ActionDescriptor, surface: &mut dyn TerminalSurface) -> Flow<()> {
    if action.mutation_class == MutationClass::ReadOnly {
        return Flow::Value(());
    }
    loop {
        let Some(raw) = surface.prompt(&format!(
            "Execute {} Action {}? [y/N] (/back, /cancel): ",
            action.mutation_class.as_str(),
            action.id,
        )) else {
            return Flow::Cancelled(cancel(Some(&action.id)));
        };
        match raw.trim().to_lowercase().as_str() {
            "y" | "yes" => return Flow::Value(()),
            "/back" => return Flow::Back,
            "/cancel" | "" | "n" | "no" => return Flow::Cancelled(cancel(Some(&action.id))),
            _ => surface.write_line("Enter yes, no, /back, or /cancel."),
        }
    }
}

pub fn run_guided_action_picker(
    registry: &ActionRegistry,
    context: &ActionExecutionContext<'_>,
    surface: &mut dyn TerminalSurface,
) -> ActionResult {
    loop {
        let action = match select_action(registry, surface) {
            Flow::Value(action) => action,
            Flow::Back => continue,
            Flow::Cancelled(result) => return result,
        };
        let input = match collect_input(&action, registry, context, surface) {
            Flow::Value(input) => input,
            Flow::Back => continue,
            Flow::Cancelled(result) => return result,
        };
        match confirm_mutation(&action, surface) {
            Flow::Value(()) => return registry.execute(&action.id, &input, context),
            Flow::Back => continue,
            Flow::Cancelled(result) => return result,
        }
    }
}
