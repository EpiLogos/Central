use crate::action::{
    ActionDescriptor, ActionRegistry, AvailabilityStatus, InputDefinition, MutationClass,
    OutputDefinition,
};

pub fn register(actions: &mut ActionRegistry) {
    actions.register(ActionDescriptor {
        id: "control.open".into(),
        title: "Open Control source root".into(),
        description: "Resolve one stable authored Control source root.".into(),
        input_definitions: vec![InputDefinition {
            name: "root".into(),
            description: "One of user, agents, or machines.".into(),
            required: true,
            selectable_source: None,
        }],
        output_definition: OutputDefinition {
            description: "The live authored Control source root.".into(),
        },
        mutation_class: MutationClass::ReadOnly,
        preview_support: false,
        required_ports: vec![],
        availability_status: AvailabilityStatus::Available,
    });

    actions.register(ActionDescriptor {
        id: "control.search".into(),
        title: "Search Control source".into(),
        description: "Search live authored Control text without imposing a schema.".into(),
        input_definitions: vec![InputDefinition {
            name: "query".into(),
            description: "Text to search for across authored Control source.".into(),
            required: true,
            selectable_source: None,
        }],
        output_definition: OutputDefinition {
            description: "Structured source matches and explicitly skipped source files.".into(),
        },
        mutation_class: MutationClass::ReadOnly,
        preview_support: false,
        required_ports: vec![],
        availability_status: AvailabilityStatus::Available,
    });
}
