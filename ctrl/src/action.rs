use crate::result::{ActionResult, ResultStatus};
use crate::root::{inspect_central, initialize_central, resolve_central_root, RootOptions};
use serde::Serialize;
use serde_json::{json, to_value, Value};
use std::collections::BTreeMap;
use std::panic::{catch_unwind, AssertUnwindSafe};

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

pub type ActionHandler = fn(&ActionRegistry, &Value, &RootOptions) -> ActionResult;

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

    pub fn execute(&self, id: &str, input: &Value, root_options: &RootOptions) -> ActionResult {
        let Some(action) = self.actions.get(id) else {
            return ActionResult::failure(Some(id), ResultStatus::InvalidInput, format!("Unknown Action: {id}"), None);
        };
        match catch_unwind(AssertUnwindSafe(|| (action.handler)(self, input, root_options))) {
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

fn root_action(_registry: &ActionRegistry, _input: &Value, options: &RootOptions) -> ActionResult {
    match resolve_central_root(options) {
        Ok(resolved) => ActionResult::success("central.root", to_value(resolved).expect("resolved root serializes")),
        Err(message) => ActionResult::failure(Some("central.root"), ResultStatus::InvalidInput, message, None),
    }
}

fn init_action(_registry: &ActionRegistry, _input: &Value, options: &RootOptions) -> ActionResult {
    let resolved = match resolve_central_root(options) {
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

fn doctor_action(_registry: &ActionRegistry, _input: &Value, options: &RootOptions) -> ActionResult {
    let resolved = match resolve_central_root(options) {
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

fn list_actions(registry: &ActionRegistry, _input: &Value, _options: &RootOptions) -> ActionResult {
    ActionResult::success("action.list", json!({ "actions": registry.list() }))
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
    registry
}
