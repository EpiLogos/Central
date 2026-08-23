use central_ctrl::{
    adopt_flow, create_flow, initialize_now, read_flow, read_project_change_horizon,
    reconcile_project_sources, rename_flow, rollover_now, set_flow_lifecycle, write_flow,
    DEFAULT_FLOW_DIR,
};
use central_ctrl::projectcentral_ops::initialize_projectcentral;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn temporary_project(label: &str) -> (PathBuf, PathBuf) {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let central = std::env::temp_dir().join(format!("central-flow-{label}-{}-{nonce}", std::process::id()));
    let project = central.join("Work/example");
    fs::create_dir_all(&project).unwrap();
    initialize_projectcentral(&central, &project, "example/project").unwrap();
    (central, project)
}

#[test]
fn blank_flow_has_stable_identity_and_enters_change_horizon_without_model_invocation() {
    let (_central, project) = temporary_project("blank-horizon");
    reconcile_project_sources(&project).unwrap();

    let flow = create_flow(
        &project,
        Some("2026-08-23-2310"),
        None,
        None,
        "human:test",
        "human",
        None,
    )
    .unwrap();

    assert!(flow.path.starts_with(DEFAULT_FLOW_DIR));
    assert_eq!(fs::read_to_string(project.join(&flow.path)).unwrap(), "");
    let horizon = read_project_change_horizon(&project, None).unwrap();
    assert!(!horizon.automatic_agent_or_model_invocation);
    assert!(horizon.sources.iter().any(|source| {
        source.binding.source_ref == flow.source_ref
            && source.binding.roles.iter().any(|role| role == "flow-source")
    }));
    assert!(horizon.changes.iter().any(|change| {
        change.source_ref == flow.source_ref
            && change.source_roles.iter().any(|role| role == "flow-source")
    }));
}

#[test]
fn human_and_agent_share_revision_safe_write_semantics_and_stale_agent_cannot_overwrite_human() {
    let (_central, project) = temporary_project("collaborative-write");
    let flow = create_flow(
        &project,
        Some("2026-08-23-2311"),
        None,
        Some("A thought".into()),
        "human:test",
        "human",
        None,
    )
    .unwrap();
    let initial = flow.current_revision.clone();

    let human = write_flow(
        &project,
        &flow.flow_ref,
        &initial,
        "human revision\n",
        "human:test",
        "human",
        None,
    )
    .unwrap();
    let stale = write_flow(
        &project,
        &flow.flow_ref,
        &initial,
        "stale agent overwrite\n",
        "agent:test",
        "agent",
        Some("aikit:agent-session:1".into()),
    );
    assert!(stale.is_err());
    assert_eq!(fs::read_to_string(project.join(&flow.path)).unwrap(), "human revision\n");

    let agent = write_flow(
        &project,
        &flow.flow_ref,
        &human.current_revision,
        "human revision\nagent contribution\n",
        "agent:test",
        "agent",
        Some("aikit:agent-session:2".into()),
    )
    .unwrap();
    assert_eq!(agent.flow_ref, flow.flow_ref);
    assert_eq!(agent.revisions.last().unwrap().actor_kind, "agent");
    assert_eq!(
        agent.revisions.last().unwrap().agent_session_ref.as_deref(),
        Some("aikit:agent-session:2")
    );
}

#[test]
fn external_editor_revision_is_reconciled_with_unknown_actor_and_preserves_flow_ref() {
    let (_central, project) = temporary_project("external");
    let flow = create_flow(
        &project,
        Some("2026-08-23-2312"),
        None,
        None,
        "human:test",
        "human",
        None,
    )
    .unwrap();
    fs::write(project.join(&flow.path), "edited outside ctrl\n").unwrap();

    let reading = read_flow(&project, &flow.flow_ref).unwrap();
    assert!(reading.dirty_external_revision_reconciled);
    assert_eq!(reading.flow.flow_ref, flow.flow_ref);
    assert_eq!(reading.flow.revisions.last().unwrap().actor_kind, "unknown-external");
    assert_eq!(reading.flow.revisions.last().unwrap().actor, "unknown");
}

#[test]
fn retained_in_place_flow_can_rename_and_cross_day_without_identity_change() {
    let (_central, project) = temporary_project("day-rename");
    initialize_now(&project).unwrap();
    fs::create_dir_all(project.join("notes")).unwrap();
    fs::write(project.join("notes/current.md"), "retained source\n").unwrap();

    let adopted = adopt_flow(
        &project,
        "notes/current.md",
        Some("Retained research thread".into()),
        "human:test",
        "human",
        None,
    )
    .unwrap();
    let renamed = rename_flow(
        &project,
        &adopted.flow_ref,
        &adopted.current_revision,
        "notes/renamed.md",
    )
    .unwrap();
    assert_eq!(renamed.flow_ref, adopted.flow_ref);
    assert_ne!(renamed.source_ref, adopted.source_ref);

    let day = rollover_now(&project, "2026-08-23", "2026-08-24").unwrap();
    let snapshot = day.flows.iter().find(|entry| entry.flow_ref == adopted.flow_ref).unwrap();
    assert_eq!(snapshot.revision, renamed.current_revision);
    assert_eq!(snapshot.source_path, "notes/renamed.md");
    assert_eq!(fs::read_to_string(project.join(&snapshot.snapshot_source)).unwrap(), "retained source\n");

    let after = read_flow(&project, &adopted.flow_ref).unwrap();
    assert_eq!(after.flow.flow_ref, adopted.flow_ref);
    assert_eq!(after.flow.current_revision, snapshot.revision);
}

#[test]
fn multiple_flows_have_independent_lifecycle_and_revision_history() {
    let (_central, project) = temporary_project("multiple");
    let first = create_flow(&project, Some("2026-08-23-2313"), None, None, "human:test", "human", None).unwrap();
    let second = create_flow(&project, Some("2026-08-23-2314"), None, None, "human:test", "human", None).unwrap();
    let first = set_flow_lifecycle(&project, &first.flow_ref, &first.current_revision, "dormant").unwrap();
    assert_eq!(first.lifecycle, "dormant");
    assert_eq!(read_flow(&project, &second.flow_ref).unwrap().flow.lifecycle, "active");
    assert_ne!(first.flow_ref, second.flow_ref);
}


#[test]
fn flow_adoption_preserves_existing_central_authority_boundaries() {
    let (_central, project) = temporary_project("authority-boundary");
    let human_source = project.join("ProjectCentral/user/meaning.md");
    fs::write(&human_source, "authored ground candidate\n").unwrap();
    let attempt = adopt_flow(
        &project,
        "ProjectCentral/user/meaning.md",
        None,
        "human:test",
        "human",
        None,
    );
    assert!(attempt.is_err());
}


#[test]
fn flow_role_composes_with_retained_wiki_role_without_reclassifying_the_source() {
    let (_central, project) = temporary_project("dual-role");
    fs::create_dir_all(project.join("notes")).unwrap();
    fs::write(project.join("notes/shared.md"), "shared source\n").unwrap();

    let manifest_path = project.join("ProjectCentral/project.json");
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
    manifest["wiki"]["adopted_sources"] = serde_json::json!(["notes/shared.md"]);
    fs::write(
        &manifest_path,
        serde_json::to_vec_pretty(&manifest).unwrap(),
    )
    .unwrap();

    let flow = adopt_flow(
        &project,
        "notes/shared.md",
        None,
        "human:test",
        "human",
        None,
    )
    .unwrap();
    let horizon = read_project_change_horizon(&project, None).unwrap();
    let observed = horizon
        .sources
        .iter()
        .find(|source| source.binding.path == "notes/shared.md")
        .unwrap();
    assert!(observed.binding.roles.iter().any(|role| role == "adopted-agent-wiki-source"));
    assert!(observed.binding.roles.iter().any(|role| role == "flow-source"));
    assert_eq!(observed.binding.source_ref, "central:source:project:example/project:notes/shared.md");
    assert_eq!(flow.flow_ref.starts_with("central:flow:project:example/project:"), true);
}
