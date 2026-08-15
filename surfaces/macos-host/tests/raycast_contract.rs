const RAYCAST_SOURCE: &str = include_str!("../../raycast/src/central-actions.tsx");
const RAYCAST_PACKAGE: &str = include_str!("../../raycast/package.json");
const SHORTCUTS_GUIDE: &str = include_str!("../../shortcuts/README.md");

#[test]
fn raycast_surface_is_descriptor_driven_and_does_not_reimplement_work_actions() {
    for required in [
        "action\", \"list",
        "action\", \"run",
        "selection.action",
        "selection.collection",
        "selection.value_field",
        "confirmAlert",
        "mutation_class",
        "actionHotkeys",
        "shortcut=",
    ] {
        assert!(RAYCAST_SOURCE.contains(required), "Raycast Surface is missing {required}");
    }

    assert!(!RAYCAST_SOURCE.contains("\"work.open\""));
    assert!(!RAYCAST_SOURCE.contains("\"work.reveal\""));
    assert!(!RAYCAST_SOURCE.contains("Work/"));
    assert!(RAYCAST_PACKAGE.contains("\"name\": \"central-actions\""));
    assert!(RAYCAST_PACKAGE.contains("\"default\": \"ctrl-macos\""));
}

#[test]
fn shortcuts_document_both_directions_through_the_same_action_protocol() {
    assert!(SHORTCUTS_GUIDE.contains("automation.run Action"));
    assert!(SHORTCUTS_GUIDE.contains("Automation Port"));
    assert!(SHORTCUTS_GUIDE.contains("ShortcutsAutomationConnector"));
    assert!(SHORTCUTS_GUIDE.contains("action run central.root"));
    assert!(SHORTCUTS_GUIDE.contains("action run work.open"));
    assert!(SHORTCUTS_GUIDE.contains("primary macOS workstation"));
}
