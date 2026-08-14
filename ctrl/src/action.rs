use crate::control::{locate_control_root, search_control};
use crate::result::{ActionResult, ResultStatus};
use crate::root::{inspect_central, initialize_central, resolve_central_root, RootOptions};
use central_connector_sdk::{
    ConnectorContext, ConnectorDiagnostics, ConnectorRegistry, WorkDiscoveryInput, WorkItem,
    WORK_DISCOVERY_PORT,
};
use serde::Serialize;
use serde_json::{json, to_value, Value};
use std::collections::BTreeMap;
use std::io;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum MutationClass {
    #[serde(rename = "read-only")]
    ReadOnly,
    #[serde(rename = "locally-mutating")]
    LocallyMutating,
    #[serde(rename = "externally-mutating")]
    ExternallyMutating,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ActionInputDefinition {
    pub name: String,
    #[serde(rename = "type")]
    pub input_type: String,
    pub required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ActionOutputDefinition {
    #[serde(rename = "type")]
    pub output_type: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ActionAvailability {
    pub available: bool,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ActionDescriptor {
    pub id: String,
    pub title: String,
    pub description: String,
    pub inputs: Vec<ActionInputDefinition>,
    pub output: ActionOutputDefinition,
    pub mutation_class: MutationClass,
    pub preview_supported: bool,
    pub required_ports: Vec<String>,
    pub availability: ActionAvailability,
}

pub struct ActionExecutionContext<'a> {
    pub root_options: &'a RootOptions,
    pub connectors: &'a ConnectorRegistry,
    pub connector_context: &'a ConnectorContext,
}

pub type ActionHandler = for<'a> fn(&ActionRegistry, &Value, &ActionExecutionContext<'a>) -> ActionResult;

struct RegisteredAction {
    descriptor: ActionDescriptor,
    handler: ActionHandler,
}

#[derive(Default)]
pub struct ActionRegistry {
    actions: BTreeMap<String, RegisteredAction>,
}

impl ActionRegistry {
    pub fn register(&mut self, descriptor: ActionDescriptor, handler: ActionHandler) -> Result<(), String> {
        if descriptor.id.trim().is_empty() || !descriptor.id.contains('.') {
            return Err(format!("Invalid Action id: {}", descriptor.id));
        }
        if self.actions.contains_key(&descriptor.id) {
            return Err(format!("Action already registered: {}", descriptor.id));
        }
        self.actions.insert(descriptor.id.clone(), RegisteredAction { descriptor, handler });
        Ok(())
    }

    pub fn get(&self, id: &str) -> Option<&ActionDescriptor> {
        self.actions.get(id).map(|entry| &entry.descriptor)
    }

    pub fn list(&self) -> Vec<ActionDescriptor> {
        self.actions.values().map(|entry| entry.descriptor.clone()).collect()
    }

    pub fn execute(&self, id: &str, input: &Value, context: &ActionExecutionContext<'_>) -> ActionResult {
        let Some(action) = self.actions.get(id) else {
            return ActionResult::failure(Some(id), ResultStatus::InvalidInput, format!("Unknown Action: {id}"), None);
        };
        match catch_unwind(AssertUnwindSafe(|| (action.handler)(self, input, context))) {
            Ok(result) => result,
            Err(_) => ActionResult::failure(
                Some(id),
                ResultStatus::InternalFailure,
                "Action execution failed unexpectedly.",
                None,
            ),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
struct DiscoveredWork {
    items: Vec<WorkItem>,
    root: PathBuf,
    diagnostics: ConnectorDiagnostics,
}

fn descriptor(id: &str, title: &str, description: &str, mutation_class: MutationClass, output_type: &str) -> ActionDescriptor {
    ActionDescriptor {
        id: id.to_owned(),
        title: title.to_owned(),
        description: description.to_owned(),
        inputs: Vec::new(),
        output: ActionOutputDefinition { output_type: output_type.to_owned() },
        mutation_class,
        preview_supported: false,
        required_ports: Vec::new(),
        availability: ActionAvailability { available: true, reason: None },
    }
}

fn string_input(name: &str) -> ActionInputDefinition {
    ActionInputDefinition { name: name.to_owned(), input_type: "string".to_owned(), required: true }
}

fn required_text(input: &Value, field: &str, action: &str) -> Result<String, ActionResult> {
    let Some(value) = input.get(field).and_then(Value::as_str).map(str::trim).filter(|value| !value.is_empty()) else {
        return Err(ActionResult::failure(
            Some(action),
            ResultStatus::InvalidInput,
            format!("{action} requires {field}."),
            None,
        ));
    };
    Ok(value.to_owned())
}

fn root_action(_registry: &ActionRegistry, _input: &Value, context: &ActionExecutionContext<'_>) -> ActionResult {
    match resolve_central_root(context.root_options) {
        Ok(resolved) => ActionResult::success("central.root", to_value(resolved).expect("resolved root serializes")),
        Err(message) => ActionResult::failure(Some("central.root"), ResultStatus::InvalidInput, message, None),
    }
}

fn init_action(_registry: &ActionRegistry, _input: &Value, context: &ActionExecutionContext<'_>) -> ActionResult {
    let resolved = match resolve_central_root(context.root_options) {
        Ok(resolved) => resolved,
        Err(message) => return ActionResult::failure(Some("central.init"), ResultStatus::InvalidInput, message, None),
    };

    match inspect_central(&resolved.path) {
        Ok(report) if report.root_state == "not_directory" => {
            return ActionResult::failure(
                Some("central.init"),
                ResultStatus::InvalidCentralStructure,
                "Central root exists but is not a directory.",
                Some(to_value(report).expect("health report serializes")),
            );
        }
        Ok(_) => {}
        Err(error) => {
            return ActionResult::failure(Some("central.init"), ResultStatus::InternalFailure, error.to_string(), None);
        }
    }

    match initialize_central(&resolved.path) {
        Ok(initialized) => ActionResult::success("central.init", to_value(initialized).expect("initialization serializes")),
        Err(error) => ActionResult::failure(Some("central.init"), ResultStatus::InternalFailure, error.to_string(), None),
    }
}

fn doctor_action(_registry: &ActionRegistry, _input: &Value, context: &ActionExecutionContext<'_>) -> ActionResult {
    let resolved = match resolve_central_root(context.root_options) {
        Ok(resolved) => resolved,
        Err(message) => return ActionResult::failure(Some("central.doctor"), ResultStatus::InvalidInput, message, None),
    };
    match inspect_central(&resolved.path) {
        Ok(report) if report.valid => ActionResult::success("central.doctor", to_value(report).expect("health report serializes")),
        Ok(report) => ActionResult::failure(
            Some("central.doctor"),
            ResultStatus::InvalidCentralStructure,
            "Central structure is incomplete or invalid.",
            Some(to_value(report).expect("health report serializes")),
        ),
        Err(error) => ActionResult::failure(Some("central.doctor"), ResultStatus::InternalFailure, error.to_string(), None),
    }
}

fn list_actions(registry: &ActionRegistry, _input: &Value, _context: &ActionExecutionContext<'_>) -> ActionResult {
    ActionResult::success("action.list", json!({ "actions": registry.list() }))
}

fn control_open_action(_registry: &ActionRegistry, input: &Value, context: &ActionExecutionContext<'_>) -> ActionResult {
    let target = match required_text(input, "target", "control.open") {
        Ok(target) => target,
        Err(result) => return result,
    };
    let root = match resolve_central_root(context.root_options) {
        Ok(root) => root,
        Err(message) => return ActionResult::failure(Some("control.open"), ResultStatus::InvalidInput, message, None),
    };
    let source = match locate_control_root(&root.path, &target) {
        Ok(source) => source,
        Err(message) => return ActionResult::failure(Some("control.open"), ResultStatus::InvalidInput, message, None),
    };
    if !source.exists {
        return ActionResult::failure(
            Some("control.open"),
            ResultStatus::InvalidCentralStructure,
            format!("Control/{target} is missing."),
            Some(to_value(source).expect("Control source root serializes")),
        );
    }
    ActionResult::success("control.open", to_value(source).expect("Control source root serializes"))
}

fn control_search_action(_registry: &ActionRegistry, input: &Value, context: &ActionExecutionContext<'_>) -> ActionResult {
    let query = match required_text(input, "query", "control.search") {
        Ok(query) => query,
        Err(result) => return result,
    };
    let root = match resolve_central_root(context.root_options) {
        Ok(root) => root,
        Err(message) => return ActionResult::failure(Some("control.search"), ResultStatus::InvalidInput, message, None),
    };
    match search_control(&root.path, &query) {
        Ok(result) => ActionResult::success("control.search", to_value(result).expect("Control search serializes")),
        Err(error) if error.kind() == io::ErrorKind::NotFound => ActionResult::failure(
            Some("control.search"),
            ResultStatus::InvalidCentralStructure,
            error.to_string(),
            None,
        ),
        Err(error) if error.kind() == io::ErrorKind::InvalidInput => ActionResult::failure(
            Some("control.search"),
            ResultStatus::InvalidInput,
            error.to_string(),
            None,
        ),
        Err(error) => ActionResult::failure(Some("control.search"), ResultStatus::InternalFailure, error.to_string(), None),
    }
}

fn discover_work(action_id: &str, context: &ActionExecutionContext<'_>) -> Result<DiscoveredWork, ActionResult> {
    let root = resolve_central_root(context.root_options)
        .map_err(|message| ActionResult::failure(Some(action_id), ResultStatus::InvalidInput, message, None))?;
    let resolution = context.connectors.resolve(&WORK_DISCOVERY_PORT, context.connector_context);
    let diagnostics = resolution.diagnostics.clone();
    let Some(connector) = resolution.connector else {
        return Err(ActionResult::failure(
            Some(action_id),
            ResultStatus::UnavailableCapability,
            format!("No eligible Connector implements {}.", WORK_DISCOVERY_PORT.id),
            Some(json!({ "port": WORK_DISCOVERY_PORT.id, "diagnostics": diagnostics })),
        ));
    };
    let Some(implementation) = connector.work_discovery() else {
        return Err(ActionResult::failure(
            Some(action_id),
            ResultStatus::ConnectorFailure,
            format!("Selected Connector does not expose {} implementation.", WORK_DISCOVERY_PORT.id),
            Some(json!({ "port": WORK_DISCOVERY_PORT.id, "connector": connector.manifest().id, "diagnostics": diagnostics })),
        ));
    };
    let input = WorkDiscoveryInput { work_root: root.path.join("Work") };
    match implementation.list(&input) {
        Ok(output) => Ok(DiscoveredWork { items: output.items, root: root.path, diagnostics }),
        Err(error) => Err(ActionResult::failure(
            Some(action_id),
            ResultStatus::ConnectorFailure,
            format!("Connector failed while executing {}.", WORK_DISCOVERY_PORT.id),
            Some(json!({
                "port": WORK_DISCOVERY_PORT.id,
                "connector": connector.manifest().id,
                "provider_error": error,
                "diagnostics": diagnostics,
            })),
        )),
    }
}

fn work_matches<'a>(items: &'a [WorkItem], query: &str) -> Vec<&'a WorkItem> {
    let needle = query.to_lowercase();
    items.iter().filter(|item| item.name.to_lowercase().contains(&needle)).collect()
}

fn work_list_action(_registry: &ActionRegistry, _input: &Value, context: &ActionExecutionContext<'_>) -> ActionResult {
    match discover_work("work.list", context) {
        Ok(discovered) => ActionResult::success("work.list", to_value(discovered).expect("Work discovery serializes")),
        Err(result) => result,
    }
}

fn work_search_action(_registry: &ActionRegistry, input: &Value, context: &ActionExecutionContext<'_>) -> ActionResult {
    let query = match required_text(input, "query", "work.search") {
        Ok(query) => query,
        Err(result) => return result,
    };
    let discovered = match discover_work("work.search", context) {
        Ok(discovered) => discovered,
        Err(result) => return result,
    };
    let matches = work_matches(&discovered.items, &query);
    ActionResult::success(
        "work.search",
        json!({
            "query": query,
            "matches": matches,
            "root": discovered.root,
            "diagnostics": discovered.diagnostics,
        }),
    )
}

fn work_open_action(_registry: &ActionRegistry, input: &Value, context: &ActionExecutionContext<'_>) -> ActionResult {
    let query = match required_text(input, "query", "work.open") {
        Ok(query) => query,
        Err(result) => return result,
    };
    let discovered = match discover_work("work.open", context) {
        Ok(discovered) => discovered,
        Err(result) => return result,
    };
    let normalized = query.to_lowercase();
    let exact = discovered.items.iter().find(|item| item.name.to_lowercase() == normalized);
    let matches = if let Some(item) = exact { vec![item] } else { work_matches(&discovered.items, &query) };
    if matches.is_empty() {
        return ActionResult::failure(
            Some("work.open"),
            ResultStatus::InvalidInput,
            format!("No Work item matches: {query}"),
            Some(json!({ "query": query, "matches": [] })),
        );
    }
    if matches.len() > 1 {
        return ActionResult::failure(
            Some("work.open"),
            ResultStatus::InvalidInput,
            format!("Work search is ambiguous: {query}"),
            Some(json!({ "query": query, "matches": matches })),
        );
    }
    ActionResult::success(
        "work.open",
        json!({
            "query": query,
            "match": if exact.is_some() { "exact" } else { "search" },
            "item": matches[0],
            "root": discovered.root,
            "diagnostics": discovered.diagnostics,
        }),
    )
}

pub fn create_core_action_registry() -> ActionRegistry {
    let mut registry = ActionRegistry::default();
    registry.register(
        descriptor("central.root", "Show Central root", "Resolve the active Central root.", MutationClass::ReadOnly, "central-root"),
        root_action,
    ).expect("core Action ids are valid");
    registry.register(
        descriptor("central.init", "Initialize Central", "Create the required Central root structure without imposing a schema below Control roots.", MutationClass::LocallyMutating, "central-initialization"),
        init_action,
    ).expect("core Action ids are valid");
    registry.register(
        descriptor("central.doctor", "Diagnose Central", "Check the validity of the basic Central filesystem structure.", MutationClass::ReadOnly, "central-health"),
        doctor_action,
    ).expect("core Action ids are valid");
    registry.register(
        descriptor("action.list", "List Actions", "List canonical Action descriptors.", MutationClass::ReadOnly, "action-descriptor-list"),
        list_actions,
    ).expect("core Action ids are valid");

    let mut control_open = descriptor(
        "control.open",
        "Locate Control source root",
        "Resolve one stable authored Control source root without imposing a schema below it.",
        MutationClass::ReadOnly,
        "control-source-root",
    );
    control_open.inputs = vec![string_input("target")];
    registry.register(control_open, control_open_action).expect("core Action ids are valid");

    let mut control_search = descriptor(
        "control.search",
        "Search Control source",
        "Search readable authored content below the three stable Control roots without creating an index.",
        MutationClass::ReadOnly,
        "control-source-search",
    );
    control_search.inputs = vec![string_input("query")];
    registry.register(control_search, control_search_action).expect("core Action ids are valid");

    let mut work_list = descriptor(
        "work.list",
        "List Work items",
        "Discover ordinary directories in the active Central Work root through WorkDiscovery.",
        MutationClass::ReadOnly,
        "work-item-list",
    );
    work_list.required_ports = vec![WORK_DISCOVERY_PORT.id.to_owned()];
    registry.register(work_list, work_list_action).expect("core Action ids are valid");

    let mut work_search = descriptor(
        "work.search",
        "Search Work items",
        "Search names returned by WorkDiscovery without imposing project metadata.",
        MutationClass::ReadOnly,
        "work-item-search",
    );
    work_search.inputs = vec![string_input("query")];
    work_search.required_ports = vec![WORK_DISCOVERY_PORT.id.to_owned()];
    registry.register(work_search, work_search_action).expect("core Action ids are valid");

    let mut work_open = descriptor(
        "work.open",
        "Enter Work item",
        "Resolve one ordinary Work directory by exact name or unambiguous search.",
        MutationClass::ReadOnly,
        "work-item-selection",
    );
    work_open.inputs = vec![string_input("query")];
    work_open.required_ports = vec![WORK_DISCOVERY_PORT.id.to_owned()];
    registry.register(work_open, work_open_action).expect("core Action ids are valid");
    registry
}
