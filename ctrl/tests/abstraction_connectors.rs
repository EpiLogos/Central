use central_ctrl::{
    action::ActionRegistry,
    connector::ConnectorRegistry,
    port::{WorkDiscovery, WorkItem},
    reference_connectors::{
        FILESYSTEM_WORK_DISCOVERY_ID, FilesystemWorkDiscovery, StaticWorkDiscovery,
        filesystem_work_discovery_metadata, static_work_discovery_metadata,
    },
    root,
};
use tempfile::tempdir;

fn item(name: &str, path: &str) -> WorkItem {
    WorkItem {
        name: name.into(),
        path: path.into(),
    }
}

#[test]
fn work_list_descriptor_requires_work_discovery() {
    let registry = ActionRegistry::core();
    let descriptor = registry.get("work.list").expect("work.list Action");
    assert_eq!(descriptor.required_ports, vec!["WorkDiscovery"]);
}

#[test]
fn filesystem_reference_connector_lists_ordinary_work_directories() {
    let temp = tempdir().unwrap();
    let central = temp.path().join("Central");
    root::initialize(&central).unwrap();
    std::fs::create_dir(central.join("Work/alpha")).unwrap();
    std::fs::create_dir(central.join("Work/beta")).unwrap();
    std::fs::write(central.join("Work/not-a-project.txt"), "ignored").unwrap();

    let items = FilesystemWorkDiscovery.list(&central).unwrap();
    let names = items
        .iter()
        .map(|work| work.name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(names, vec!["alpha", "beta"]);
}

#[test]
fn two_valid_connectors_have_stable_selection_independent_of_registration_order() {
    fn registry(reverse: bool) -> ConnectorRegistry {
        let mut registry = ConnectorRegistry::new();
        let filesystem = (
            filesystem_work_discovery_metadata(),
            FilesystemWorkDiscovery,
        );
        let static_connector = (
            static_work_discovery_metadata(),
            StaticWorkDiscovery::new(vec![item("static", "/tmp/static")]),
        );

        if reverse {
            registry.register_work_discovery(static_connector.0, static_connector.1);
            registry.register_work_discovery(filesystem.0, filesystem.1);
        } else {
            registry.register_work_discovery(filesystem.0, filesystem.1);
            registry.register_work_discovery(static_connector.0, static_connector.1);
        }
        registry
    }

    for reverse in [false, true] {
        let registry = registry(reverse);
        let resolution = registry.resolve_work_discovery("linux");
        assert_eq!(
            resolution.diagnostics.eligible_connectors,
            vec![
                "reference.filesystem-work-discovery",
                "reference.static-work-discovery"
            ]
        );
        assert_eq!(
            resolution.diagnostics.selected_connector.as_deref(),
            Some(FILESYSTEM_WORK_DISCOVERY_ID)
        );
    }
}
