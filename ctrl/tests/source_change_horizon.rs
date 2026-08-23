use central_ctrl::{
    acknowledge_project_cursor, compact_project_changes, control_source_bindings,
    create_core_action_registry, create_default_connector_registry, initialize_projectcentral,
    projectcentral_ops::register_projectcentral_actions, read_project_change_horizon,
    reconcile_control_sources, reconcile_project_sources, ActionExecutionContext, ConnectorContext,
    RootOptions, SourceChangeKind, GROUND_RELATIONS_SOURCE, PROJECT_HORIZON_STATE,
};
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

struct TempRoot(PathBuf);

impl TempRoot {
    fn new() -> Self {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let path = std::env::temp_dir().join(format!("central-source-horizon-{}-{nonce}", std::process::id()));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn project_fixture(name: &str) -> (TempRoot, PathBuf, PathBuf) {
    let temp = TempRoot::new();
    let central = temp.path().join("Central");
    let project = central.join("Work").join(name);
    fs::create_dir_all(&project).unwrap();
    initialize_projectcentral(&central, &project, &format!("example/{name}")).unwrap();
    (temp, central, project)
}

#[test]
fn baseline_then_direct_edit_yields_one_logical_change_without_agent_invocation() {
    let (_temp, _central, project) = project_fixture("direct-edit");
    let source = project.join("ProjectCentral/user/intent.md");
    fs::write(&source, "alpha\n").unwrap();

    let baseline = reconcile_project_sources(&project).unwrap();
    assert!(baseline.initialized);
    assert!(baseline.new_changes.is_empty());
    assert_eq!(baseline.horizon.cursor, 0);
    assert!(!baseline.horizon.automatic_agent_or_model_invocation);
    assert!(!baseline.horizon.source_payloads_exposed);

    fs::write(&source, "beta\n").unwrap();
    let changed = reconcile_project_sources(&project).unwrap();
    assert_eq!(changed.new_changes.len(), 1);
    assert_eq!(changed.new_changes[0].kind, SourceChangeKind::Modified);
    assert_eq!(changed.new_changes[0].cursor, 1);
    assert!(changed.new_changes[0].actor.is_none());

    // A save/touch whose bytes are unchanged is not a semantic source revision.
    fs::write(&source, "beta\n").unwrap();
    let unchanged = reconcile_project_sources(&project).unwrap();
    assert!(unchanged.new_changes.is_empty());
    assert_eq!(unchanged.horizon.cursor, 1);
}

#[test]
fn atomic_save_burst_collapses_to_final_revision() {
    let (_temp, _central, project) = project_fixture("atomic-save");
    let source = project.join("ProjectCentral/user/note.md");
    fs::write(&source, "v1\n").unwrap();
    reconcile_project_sources(&project).unwrap();

    let tmp = project.join("ProjectCentral/user/.note.md.swap");
    fs::write(&tmp, "intermediate\n").unwrap();
    fs::write(&tmp, "v2\n").unwrap();
    fs::rename(&tmp, &source).unwrap();

    let report = reconcile_project_sources(&project).unwrap();
    assert_eq!(report.new_changes.len(), 1);
    assert_eq!(report.new_changes[0].kind, SourceChangeKind::Modified);
    assert_eq!(report.new_changes[0].after_revision.as_deref(), report.horizon.sources.iter().find(|item| item.binding.path == "ProjectCentral/user/note.md").map(|item| item.revision.revision.as_str()));
}

#[test]
fn restart_and_offline_edit_are_recovered_by_horizon_read() {
    let (_temp, _central, project) = project_fixture("restart");
    let source = project.join("ProjectCentral/agents/wiki/wiki.json");
    reconcile_project_sources(&project).unwrap();
    let original = fs::read_to_string(&source).unwrap();
    fs::write(&source, original.replace("\n}", ",\n  \"offline_marker\": true\n}" )).unwrap();

    // No watcher process is required: the read seam itself reconciles current source truth.
    let horizon = read_project_change_horizon(&project, Some(0)).unwrap();
    assert_eq!(horizon.cursor, 1);
    assert_eq!(horizon.changes.len(), 1);
    assert_eq!(horizon.changes[0].kind, SourceChangeKind::Modified);
    assert_eq!(horizon.provider, "central.filesystem-reconcile/v1");
}

#[test]
fn retained_native_ground_keeps_exact_authority_standing() {
    let (_temp, _central, project) = project_fixture("retained-ground");
    fs::write(project.join("README.md"), "# Purpose\n").unwrap();
    let relation_path = project.join(GROUND_RELATIONS_SOURCE);
    fs::create_dir_all(relation_path.parent().unwrap()).unwrap();
    fs::write(
        &relation_path,
        serde_json::to_vec_pretty(&json!({
            "schema":"central.project.ground-relations/v1",
            "project_id":"example/retained-ground",
            "relations":[{
                "ref":"central:ground:purpose",
                "path":"README.md",
                "provenance":"human-adopted",
                "standing":"authored-human-position",
                "roles":["purpose","overview"],
                "treatment":"retain-native-in-place",
                "recognition":"human-accepted source relation",
                "recorded_at_unix_seconds":1
            }]
        })).unwrap(),
    ).unwrap();

    let baseline = reconcile_project_sources(&project).unwrap();
    let retained = baseline.horizon.sources.iter().find(|source| source.binding.source_ref == "central:ground:purpose").unwrap();
    assert_eq!(retained.binding.path, "README.md");
    assert_eq!(retained.binding.provenance, "human-adopted");
    assert_eq!(retained.binding.standing, "authored-human-position");
    assert_eq!(retained.binding.roles, vec!["purpose", "overview"]);

    fs::write(project.join("README.md"), "# Purpose\n\nA changed authored source.\n").unwrap();
    let changed = reconcile_project_sources(&project).unwrap();
    let event = changed.new_changes.iter().find(|change| change.source_ref == "central:ground:purpose").unwrap();
    assert_eq!(event.provenance, "human-adopted");
    assert_eq!(event.standing, "authored-human-position");
    assert_eq!(event.source_roles, vec!["purpose", "overview"]);
}

#[test]
fn privacy_marker_changes_retrieval_eligibility_without_exposing_payloads() {
    let (_temp, _central, project) = project_fixture("privacy");
    let private = project.join("ProjectCentral/user/private");
    fs::create_dir_all(&private).unwrap();
    fs::write(private.join(".no-agent-retrieval"), "").unwrap();
    fs::write(private.join("secret.md"), "not for retrieval\n").unwrap();

    let report = reconcile_project_sources(&project).unwrap();
    let source = report.horizon.sources.iter().find(|source| source.binding.path.ends_with("secret.md")).unwrap();
    assert!(!source.binding.agent_retrieval_allowed);
    let serialized = serde_json::to_string(&report.horizon).unwrap();
    assert!(!serialized.contains("not for retrieval"));
    assert!(!report.horizon.source_payloads_exposed);
}

#[test]
fn additions_removals_and_path_identity_are_explicit() {
    let (_temp, _central, project) = project_fixture("identity");
    let source = project.join("ProjectCentral/user/a.md");
    fs::write(&source, "one\n").unwrap();
    reconcile_project_sources(&project).unwrap();

    let renamed = project.join("ProjectCentral/user/b.md");
    fs::rename(&source, &renamed).unwrap();
    let report = reconcile_project_sources(&project).unwrap();
    assert_eq!(report.new_changes.len(), 2);
    assert!(report.new_changes.iter().any(|change| change.kind == SourceChangeKind::Removed && change.source_path.ends_with("a.md")));
    assert!(report.new_changes.iter().any(|change| change.kind == SourceChangeKind::Added && change.source_path.ends_with("b.md")));
}

#[test]
fn active_consumer_cursor_controls_safe_compaction() {
    let (_temp, _central, project) = project_fixture("cursor");
    let source = project.join("ProjectCentral/user/a.md");
    fs::write(&source, "one\n").unwrap();
    reconcile_project_sources(&project).unwrap();
    fs::write(&source, "two\n").unwrap();
    reconcile_project_sources(&project).unwrap();
    fs::write(&source, "three\n").unwrap();
    reconcile_project_sources(&project).unwrap();

    acknowledge_project_cursor(&project, "aikit", 1).unwrap();
    acknowledge_project_cursor(&project, "desktop", 2).unwrap();
    let compacted = compact_project_changes(&project).unwrap();
    assert_eq!(compacted.minimum_active_cursor, Some(1));
    assert_eq!(compacted.before_changes, 2);
    assert_eq!(compacted.after_changes, 1);
    let horizon = read_project_change_horizon(&project, Some(1)).unwrap();
    assert_eq!(horizon.changes.len(), 1);
    assert_eq!(horizon.changes[0].cursor, 2);
}

#[test]
fn malformed_authority_relation_fails_closed_without_advancing_state() {
    let (_temp, _central, project) = project_fixture("bad-relation");
    fs::write(project.join("ProjectCentral/user/a.md"), "one\n").unwrap();
    let baseline = reconcile_project_sources(&project).unwrap();
    assert_eq!(baseline.horizon.cursor, 0);

    let relation_path = project.join(GROUND_RELATIONS_SOURCE);
    fs::create_dir_all(relation_path.parent().unwrap()).unwrap();
    fs::write(&relation_path, b"{not json").unwrap();
    assert!(reconcile_project_sources(&project).is_err());

    fs::remove_file(relation_path).unwrap();
    let recovered = reconcile_project_sources(&project).unwrap();
    assert_eq!(recovered.horizon.cursor, 0);
    assert!(recovered.new_changes.is_empty());
    assert!(project.join(PROJECT_HORIZON_STATE).is_file());
}

#[test]
fn control_and_project_use_the_same_fractal_change_contract() {
    let temp = TempRoot::new();
    let central = temp.path().join("Central");
    fs::create_dir_all(central.join("Control/user")).unwrap();
    fs::create_dir_all(central.join("Control/agents/governance")).unwrap();
    fs::create_dir_all(central.join("Control/agents/wiki")).unwrap();
    fs::write(central.join("Control/user/intent.md"), "personal ground\n").unwrap();
    fs::write(central.join("Control/agents/wiki/wiki.json"), "{}\n").unwrap();

    let bindings = control_source_bindings(&central).unwrap();
    assert!(bindings.iter().any(|binding| binding.path == "Control/user/intent.md"));
    let baseline = reconcile_control_sources(&central).unwrap();
    assert!(baseline.initialized);
    fs::write(central.join("Control/user/intent.md"), "personal ground changed\n").unwrap();
    let changed = reconcile_control_sources(&central).unwrap();
    assert_eq!(changed.new_changes.len(), 1);
    assert_eq!(changed.new_changes[0].world_ref, "control:root");
}

#[test]
fn action_surface_reconciles_implicitly_and_contains_no_model_operation() {
    let (_temp, central, project) = project_fixture("action-surface");
    fs::write(project.join("ProjectCentral/user/note.md"), "v1\n").unwrap();

    let root_options = RootOptions {
        explicit_root: Some(central),
        configured_root: None,
        home: None,
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
    let ids = registry.list().into_iter().map(|item| item.id).collect::<Vec<_>>();
    assert!(ids.contains(&"projectcentral.change.horizon".to_owned()));
    assert!(ids.contains(&"projectcentral.change.reconcile".to_owned()));
    assert!(ids.contains(&"projectcentral.change.ack".to_owned()));
    assert!(!ids.iter().any(|id| id.contains("agent.run") || id.contains("model") || id.contains("contemplate")));

    let first = registry.execute("projectcentral.change.horizon", &json!({"project":"action-surface"}), &context);
    assert!(first.ok, "{first:?}");
    fs::write(project.join("ProjectCentral/user/note.md"), "v2\n").unwrap();
    let second = registry.execute("projectcentral.change.horizon", &json!({"project":"action-surface","cursor":0}), &context);
    assert!(second.ok, "{second:?}");
    let data: Value = second.data.unwrap();
    assert_eq!(data["cursor"], 1);
    assert_eq!(data["changes"].as_array().unwrap().len(), 1);
    assert_eq!(data["automatic_agent_or_model_invocation"], false);
}
