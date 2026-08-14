use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MutationClass {
    ReadOnly,
    LocallyMutating,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AvailabilityStatus {
    Available,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct InputDefinition {
    pub name: String,
    pub description: String,
    pub required: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selectable_source: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct OutputDefinition {
    pub description: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ActionDescriptor {
    pub id: String,
    pub title: String,
    pub description: String,
    pub input_definitions: Vec<InputDefinition>,
    pub output_definition: OutputDefinition,
    pub mutation_class: MutationClass,
    pub preview_support: bool,
    pub required_ports: Vec<String>,
    pub availability_status: AvailabilityStatus,
}

#[derive(Debug, Clone, Default)]
pub struct ActionRegistry {
    actions: BTreeMap<String, ActionDescriptor>,
}

impl ActionRegistry {
    pub fn core() -> Self {
        let mut registry = Self::default();
        for descriptor in [
            descriptor(
                "central.root",
                "Show Central root",
                "Resolve the active Central root.",
                "The resolved Central root path.",
                MutationClass::ReadOnly,
            ),
            descriptor(
                "central.init",
                "Initialize Central",
                "Ensure the required Central root structure exists.",
                "The root and protocol directories that were ensured.",
                MutationClass::LocallyMutating,
            ),
            descriptor(
                "central.doctor",
                "Diagnose Central",
                "Check whether the basic Central structure is valid.",
                "A structured Central root health report.",
                MutationClass::ReadOnly,
            ),
            descriptor(
                "action.list",
                "List Actions",
                "List canonical Actions and their descriptors.",
                "Canonical Action descriptors in stable identifier order.",
                MutationClass::ReadOnly,
            ),
        ] {
            registry.register(descriptor);
        }
        registry
    }

    pub fn register(&mut self, descriptor: ActionDescriptor) {
        self.actions.insert(descriptor.id.clone(), descriptor);
    }

    pub fn get(&self, id: &str) -> Option<&ActionDescriptor> {
        self.actions.get(id)
    }

    pub fn descriptors(&self) -> Vec<ActionDescriptor> {
        self.actions.values().cloned().collect()
    }
}

fn descriptor(
    id: &str,
    title: &str,
    description: &str,
    output: &str,
    mutation_class: MutationClass,
) -> ActionDescriptor {
    ActionDescriptor {
        id: id.into(),
        title: title.into(),
        description: description.into(),
        input_definitions: vec![],
        output_definition: OutputDefinition {
            description: output.into(),
        },
        mutation_class,
        preview_support: false,
        required_ports: vec![],
        availability_status: AvailabilityStatus::Available,
    }
}
