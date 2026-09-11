use central_ctrl::projectcentral_ops::initialize_projectcentral;
use central_ctrl::{
    adopt_flow, create_flow, initialize_now, read_flow, read_project_change_horizon,
    reconcile_project_sources, rename_flow, rollover_now, set_flow_lifecycle, write_flow,
    DEFAULT_FLOW_DIR,
};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn temporary_project(label: &str) -> (PathBuf, PathBuf) {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let central = std::env::temp_dir().join(format!(
        "central-flow-{label}-{}-{nonce}",
        std::process::id()
    ));
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
            && source
                .binding
                .roles
                .iter()
                .any(|role| role == "flow-source")
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
    assert_eq!(
        fs::read_to_string(project.join(&flow.path)).unwrap(),
        "human revision\n"
    );

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
    assert_eq!(
        reading.flow.revisions.last().unwrap().actor_kind,
        "unknown-external"
    );
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
    let snapshot = day
        .flows
        .iter()
        .find(|entry| entry.flow_ref == adopted.flow_ref)
        .unwrap();
    assert_eq!(snapshot.revision, renamed.current_revision);
    assert_eq!(snapshot.source_path, "notes/renamed.md");
    assert_eq!(
        fs::read_to_string(project.join(&snapshot.snapshot_source)).unwrap(),
        "retained source\n"
    );

    let after = read_flow(&project, &adopted.flow_ref).unwrap();
    assert_eq!(after.flow.flow_ref, adopted.flow_ref);
    assert_eq!(after.flow.current_revision, snapshot.revision);
}

#[test]
fn multiple_flows_have_independent_lifecycle_and_revision_history() {
    let (_central, project) = temporary_project("multiple");
    let first = create_flow(
        &project,
        Some("2026-08-23-2313"),
        None,
        None,
        "human:test",
        "human",
        None,
    )
    .unwrap();
    let second = create_flow(
        &project,
        Some("2026-08-23-2314"),
        None,
        None,
        "human:test",
        "human",
        None,
    )
    .unwrap();
    let first = set_flow_lifecycle(
        &project,
        &first.flow_ref,
        &first.current_revision,
        "dormant",
    )
    .unwrap();
    assert_eq!(first.lifecycle, "dormant");
    assert_eq!(
        read_flow(&project, &second.flow_ref)
            .unwrap()
            .flow
            .lifecycle,
        "active"
    );
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
    assert!(observed
        .binding
        .roles
        .iter()
        .any(|role| role == "adopted-agent-wiki-source"));
    assert!(observed
        .binding
        .roles
        .iter()
        .any(|role| role == "flow-source"));
    assert_eq!(
        observed.binding.source_ref,
        "central:source:project:example/project:notes/shared.md"
    );
    assert_eq!(
        flow.flow_ref
            .starts_with("central:flow:project:example/project:"),
        true
    );
}

// ---- Wave-4 cell W4-B: U4.1 owner-side acceptance proofs + W1.3 read models ----

use central_ctrl::{
    flow_now_view, initialize_central, inspect_flow, list_flows, run_cli, CliEnvironment,
};

fn cli(root: &PathBuf) -> CliEnvironment {
    CliEnvironment {
        configured_root: Some(root.clone()),
        home: None,
    }
}

fn run_action(root: &PathBuf, action: &str, input: serde_json::Value) -> serde_json::Value {
    let execution = run_cli(
        &[
            "--json".to_owned(),
            "action".to_owned(),
            "run".to_owned(),
            action.to_owned(),
            serde_json::to_string(&input).unwrap(),
        ],
        &cli(root),
    );
    assert_eq!(
        execution.exit_code, 0,
        "{action} failed: {}",
        execution.output
    );
    let value: serde_json::Value = serde_json::from_str(&execution.output).unwrap();
    assert_eq!(value["status"], "success", "{action}: {}", execution.output);
    value["data"].clone()
}

// U4.1 acceptance 2: the Flow remains the same stable ref after several human saves.
#[test]
fn u41_stable_ref_across_repeated_human_saves() {
    let (_central, project) = temporary_project("u41-saves");
    let flow = create_flow(
        &project,
        Some("2026-08-23-2315"),
        None,
        None,
        "human:test",
        "human",
        None,
    )
    .unwrap();
    let mut revision = flow.current_revision.clone();
    for (index, text) in ["one\n", "one two\n", "one two three\n"].iter().enumerate() {
        let saved = write_flow(
            &project,
            &flow.flow_ref,
            &revision,
            text,
            "human:test",
            "human",
            None,
        )
        .unwrap();
        assert_eq!(
            saved.flow_ref,
            flow.flow_ref,
            "save {} changed FlowRef",
            index + 1
        );
        assert_eq!(saved.revisions.len(), index + 2);
        revision = saved.current_revision;
    }
    let reading = read_flow(&project, &flow.flow_ref).unwrap();
    assert_eq!(reading.flow.flow_ref, flow.flow_ref);
    assert_eq!(reading.content, "one two three\n");
    assert_eq!(
        fs::read_to_string(project.join(&flow.path)).unwrap(),
        "one two three\n"
    );
}

// U4.1 acceptance 3 + 4: a canonical AgentSession binds by attribution without owning
// identity; ending one session and starting another continues the same Flow/current revision.
#[test]
fn u41_agent_session_binds_without_owning_identity_and_handoff_continues_thread() {
    let (_central, project) = temporary_project("u41-handoff");
    let flow = create_flow(
        &project,
        Some("2026-08-23-2316"),
        None,
        None,
        "human:test",
        "human",
        None,
    )
    .unwrap();

    let first_session = write_flow(
        &project,
        &flow.flow_ref,
        &flow.current_revision,
        "session one articulation\n",
        "agent:epii",
        "agent",
        Some("aikit:agent-session:100".into()),
    )
    .unwrap();
    assert_eq!(first_session.flow_ref, flow.flow_ref);
    assert_ne!(
        first_session.current_revision, flow.current_revision,
        "session write must advance the shared revision"
    );

    // Session one ends; session two attaches and continues from the current revision.
    let second_session = write_flow(
        &project,
        &flow.flow_ref,
        &first_session.current_revision,
        "session one articulation\nsession two continuation\n",
        "agent:epii",
        "agent",
        Some("aikit:agent-session:200".into()),
    )
    .unwrap();
    assert_eq!(second_session.flow_ref, flow.flow_ref);
    let sessions: Vec<Option<&str>> = second_session
        .revisions
        .iter()
        .map(|receipt| receipt.agent_session_ref.as_deref())
        .collect();
    assert_eq!(
        sessions,
        vec![
            None,
            Some("aikit:agent-session:100"),
            Some("aikit:agent-session:200")
        ],
        "both sessions attribute on one stable identity"
    );
    let reading = read_flow(&project, &flow.flow_ref).unwrap();
    assert_eq!(
        reading.flow.current_revision,
        second_session.current_revision
    );
}

// U4.1 acceptance 5: a dirty human buffer (external edit) plus a stale Agent write
// produces an explicit conflict without data loss on either side.
#[test]
fn u41_external_revision_conflict_preserves_both_sides() {
    let (_central, project) = temporary_project("u41-conflict");
    let flow = create_flow(
        &project,
        Some("2026-08-23-2317"),
        None,
        None,
        "human:test",
        "human",
        None,
    )
    .unwrap();
    // The human's unsaved buffer is landed by an external editor.
    fs::write(project.join(&flow.path), "human buffer landed externally\n").unwrap();
    // The Agent acts from the stale revision: explicit conflict, no overwrite.
    let conflict = write_flow(
        &project,
        &flow.flow_ref,
        &flow.current_revision,
        "agent wording\n",
        "agent:epii",
        "agent",
        Some("aikit:agent-session:300".into()),
    );
    assert!(conflict.is_err(), "stale expected_revision must conflict");
    assert!(
        conflict.unwrap_err().to_string().contains("conflict"),
        "conflict must be named explicitly"
    );
    assert_eq!(
        fs::read_to_string(project.join(&flow.path)).unwrap(),
        "human buffer landed externally\n",
        "human side preserved"
    );
    // The failed write already reconciled the external edit into provenance as an
    // unknown-actor revision; the Agent can continue from that reconciled revision.
    let reading = read_flow(&project, &flow.flow_ref).unwrap();
    assert_eq!(reading.content, "human buffer landed externally\n");
    assert_eq!(
        reading.flow.revisions.last().unwrap().actor_kind,
        "unknown-external"
    );
    let continued = write_flow(
        &project,
        &flow.flow_ref,
        &reading.flow.current_revision,
        "human buffer landed externally\nagent continuation\n",
        "agent:epii",
        "agent",
        Some("aikit:agent-session:300".into()),
    )
    .unwrap();
    assert_eq!(continued.flow_ref, flow.flow_ref);
}

// U4.1 acceptance 14: ordinary source/code files still open/edit normally and are not
// reclassified as Flow by listing or NOW presentation.
#[test]
fn u41_ordinary_files_are_not_reclassified_as_flow() {
    let (_central, project) = temporary_project("u41-ordinary");
    fs::create_dir_all(project.join("src")).unwrap();
    let ordinary = project.join("src/2026-08-23-2318-main.rs");
    fs::write(&ordinary, "fn main() {}\n").unwrap();

    let list = list_flows(&project).unwrap();
    assert!(
        list.flows.is_empty(),
        "ordinary files must never enter the Flow registry by observation"
    );
    let view = flow_now_view(&project, Some("2026-08-23")).unwrap();
    assert!(view.live_flows.is_empty());
    assert!(view.day_groups.is_empty());
    assert!(view.undated_flows.is_empty());
    // The ordinary file is untouched and still readable as plain source.
    assert_eq!(fs::read_to_string(&ordinary).unwrap(), "fn main() {}\n");
    // Only an explicit owner adopt confers Flow identity.
    let adopted = adopt_flow(
        &project,
        "src/2026-08-23-2318-main.rs",
        None,
        "human:test",
        "human",
        None,
    )
    .unwrap();
    let list = list_flows(&project).unwrap();
    assert_eq!(list.flows.len(), 1);
    assert_eq!(list.flows[0].flow_ref, adopted.flow_ref);
}

// U4.1 acceptance 15 + 16: structured owner Actions co-refer to the same
// FlowRef/revision across every read surface without screen scraping, and no owner
// surface reports automatic Agent/model invocation.
#[test]
fn u41_structured_refs_corefer_across_actions_and_never_invoke_a_model() {
    let central = temporary_project("u41-corefer").0;
    initialize_central(&central).unwrap();
    let created = run_action(
        &central,
        "projectcentral.flow.create",
        serde_json::json!({
            "project": "example",
            "actor": "human:test",
            "actor_kind": "human",
            "local_stamp": "2026-08-23-2319",
        }),
    );
    assert_eq!(created["automatic_agent_or_model_invocation"], false);
    let flow_ref = created["flow"]["flow_ref"].as_str().unwrap().to_owned();
    let revision = created["flow"]["current_revision"]
        .as_str()
        .unwrap()
        .to_owned();

    let listed = run_action(
        &central,
        "projectcentral.flow.list",
        serde_json::json!({"project": "example"}),
    );
    assert_eq!(listed["automatic_agent_or_model_invocation"], false);
    let inspected = run_action(
        &central,
        "projectcentral.flow.inspect",
        serde_json::json!({"project": "example", "flow_ref": flow_ref}),
    );
    let read = run_action(
        &central,
        "projectcentral.flow.read",
        serde_json::json!({"project": "example", "flow_ref": flow_ref}),
    );
    let history = run_action(
        &central,
        "projectcentral.flow.history",
        serde_json::json!({"project": "example", "flow_ref": flow_ref}),
    );
    let lifecycle = run_action(
        &central,
        "projectcentral.flow.lifecycle",
        serde_json::json!({
            "project": "example",
            "flow_ref": flow_ref,
            "expected_revision": revision,
            "lifecycle": "dormant",
        }),
    );
    let now = run_action(
        &central,
        "projectcentral.flow.now",
        serde_json::json!({"project": "example", "current_day": "2026-08-23"}),
    );

    // One stable FlowRef and revision co-refer across every structured surface.
    assert_eq!(
        listed["flows"][0]["flow_ref"].as_str().unwrap(),
        flow_ref.as_str()
    );
    assert_eq!(
        inspected["flow"]["flow_ref"].as_str().unwrap(),
        flow_ref.as_str()
    );
    assert_eq!(
        read["flow"]["flow_ref"].as_str().unwrap(),
        flow_ref.as_str()
    );
    assert_eq!(read["flow"]["current_revision"].as_str().unwrap(), revision);
    assert_eq!(history["flow_ref"].as_str().unwrap(), flow_ref.as_str());
    assert_eq!(
        lifecycle["flow"]["flow_ref"].as_str().unwrap(),
        flow_ref.as_str()
    );

    // U4.1 acceptance 16: no owner surface performs automatic model invocation.
    for (surface, value) in [
        ("create", &created),
        ("list", &listed),
        ("inspect", &inspected),
        ("read", &read),
        ("history", &history),
        ("lifecycle", &lifecycle),
        ("now", &now),
    ] {
        assert_eq!(
            value["automatic_agent_or_model_invocation"], false,
            "{surface} must never invoke an Agent/model"
        );
    }
}

// W1.3: NOW/DAY grouping, multiple live Flows, and the date-boundary law — a Flow
// created yesterday remains one continuing live thread under the same ref today.
#[test]
fn w13_now_view_groups_multiple_live_flows_and_honours_date_boundary_law() {
    let (_central, project) = temporary_project("w13-grouping");
    let yesterday = create_flow(
        &project,
        Some("2026-08-23-2010"),
        None,
        Some("yesterday thread".into()),
        "human:test",
        "human",
        None,
    )
    .unwrap();
    let today = create_flow(
        &project,
        Some("2026-08-24-0900"),
        None,
        None,
        "human:test",
        "human",
        None,
    )
    .unwrap();
    let second_today = create_flow(
        &project,
        Some("2026-08-24-0915"),
        None,
        None,
        "human:test",
        "human",
        None,
    )
    .unwrap();
    fs::create_dir_all(project.join("notes")).unwrap();
    fs::write(
        project.join("notes/retained.md"),
        "undated retained thread\n",
    )
    .unwrap();
    let undated = adopt_flow(
        &project,
        "notes/retained.md",
        None,
        "human:test",
        "human",
        None,
    )
    .unwrap();
    let held = set_flow_lifecycle(
        &project,
        &second_today.flow_ref,
        &second_today.current_revision,
        "dormant",
    )
    .unwrap();
    let closed = set_flow_lifecycle(
        &project,
        &undated.flow_ref,
        &undated.current_revision,
        "closed",
    )
    .unwrap();

    let view = flow_now_view(&project, Some("2026-08-24")).unwrap();
    assert_eq!(view.schema, "central.project-flow-now/v1");
    assert_eq!(view.current_day_source, "caller-supplied");

    // Multiple live Flows remain navigable in one NOW (acceptance 12 companion):
    // yesterday's continuing thread plus both of today's Flows before the
    // dormant/closed transitions below.
    let mut live_refs: Vec<&str> = view
        .live_flows
        .iter()
        .map(|entry| entry.flow.flow_ref.as_str())
        .collect();
    live_refs.sort_unstable();
    let mut expected_live = vec![yesterday.flow_ref.as_str(), today.flow_ref.as_str()];
    expected_live.sort_unstable();
    assert_eq!(live_refs, expected_live);
    assert_eq!(view.held_flows.len(), 1);
    assert_eq!(view.held_flows[0].flow.flow_ref, held.flow_ref);
    assert_eq!(view.closed_flows.len(), 1);
    assert_eq!(view.closed_flows[0].flow.flow_ref, closed.flow_ref);

    // Day grouping by embedded local civil stamp, most recent first.
    let days: Vec<&str> = view
        .day_groups
        .iter()
        .map(|group| group.day.as_str())
        .collect();
    assert_eq!(days, vec!["2026-08-24", "2026-08-23"]);
    assert_eq!(view.day_groups[0].flows.len(), 2);
    assert_eq!(view.day_groups[1].flows.len(), 1);

    // Date-boundary law: yesterday's Flow is one continuing live thread today —
    // not closed, not re-identified.
    let facts = view.day_facts.as_ref().unwrap();
    assert_eq!(facts.current_day, "2026-08-24");
    let mut begun: Vec<&str> = facts
        .begun_today
        .iter()
        .map(|entry| entry.flow.flow_ref.as_str())
        .collect();
    begun.sort_unstable();
    let mut expected_begun = vec![second_today.flow_ref.as_str(), today.flow_ref.as_str()];
    expected_begun.sort_unstable();
    assert_eq!(
        begun, expected_begun,
        "both Flows begun today are deterministic DAY facts, whatever their later lifecycle"
    );
    let continuing: Vec<&str> = facts
        .continuing
        .iter()
        .map(|entry| entry.flow.flow_ref.as_str())
        .collect();
    assert_eq!(
        continuing,
        vec![yesterday.flow_ref.as_str()],
        "a Flow crossing the date boundary continues under the same ref"
    );
    assert_eq!(
        facts.continuing[0].flow.flow_ref, yesterday.flow_ref,
        "date boundary must not mint a new identity"
    );
    assert_eq!(facts.closed.len(), 1);
    assert_eq!(facts.undated_live.len(), 0);

    // Explicit undated state for the adopted retained source.
    assert_eq!(view.undated_flows.len(), 1);
    assert_eq!(view.undated_flows[0].local_stamp_date, None);
    assert_eq!(view.undated_flows[0].currentness, "closed");
    assert!(view.date_boundary_law.contains("does not close a Flow"));

    // Rest-vs-thinking disclosure: rest is disclosed from owner data; thinking is
    // an explicit unavailable state owned by AIKit.
    assert!(view.engagement.at_rest.available);
    assert_eq!(
        view.engagement.at_rest.disclosed_by,
        "central:projectcentral.flow"
    );
    assert!(!view.engagement.while_thinking.available);
    assert_eq!(view.engagement.while_thinking.owner, "aikit:agent-session");
    assert_eq!(
        view.engagement.disclosure_ladder,
        vec!["available", "retrieved", "loaded", "disclosed"]
    );
    assert!(!view.automatic_agent_or_model_invocation);
}

// W1.3: the civil date is caller-supplied or unavailable — Central never guesses
// from a timezone, and a malformed caller day is an explicit invalid state.
#[test]
fn w13_now_view_never_guesses_the_civil_date() {
    let (_central, project) = temporary_project("w13-no-guess");
    create_flow(
        &project,
        Some("2026-08-23-2020"),
        None,
        None,
        "human:test",
        "human",
        None,
    )
    .unwrap();

    let view = flow_now_view(&project, None).unwrap();
    assert_eq!(view.current_day_source, "unavailable");
    assert!(
        view.day_facts.is_none(),
        "DAY facts are unavailable without a caller-supplied civil date"
    );
    assert_eq!(view.day_groups.len(), 1);
    assert_eq!(view.day_groups[0].day, "2026-08-23");
    assert_eq!(view.live_flows.len(), 1);

    let central = _central;
    initialize_central(&central).unwrap();
    let execution = run_cli(
        &[
            "--json".to_owned(),
            "action".to_owned(),
            "run".to_owned(),
            "projectcentral.flow.now".to_owned(),
            serde_json::to_string(&serde_json::json!({
                "project": "example",
                "current_day": "23 August 2026",
            }))
            .unwrap(),
        ],
        &cli(&central),
    );
    assert_ne!(execution.exit_code, 0, "{}", execution.output);
    let value: serde_json::Value = serde_json::from_str(&execution.output).unwrap();
    assert_eq!(value["status"], "invalid_input", "{}", execution.output);
}

/// Central root is the meta-project: a Flow that belongs to no one project
/// lives in the root register's own NOW field, names the root in its refs, and
/// reads, writes and keeps history like any other Flow.
#[test]
fn the_central_root_register_holds_a_flow_of_its_own() {
    use central_ctrl::projectcentral_flow::ROOT_FLOW_DIR;
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let central =
        std::env::temp_dir().join(format!("central-root-flow-{}-{nonce}", std::process::id()));
    fs::create_dir_all(central.join("Control/agents/now/flows")).unwrap();
    fs::create_dir_all(central.join("Control/user")).unwrap();
    fs::create_dir_all(central.join("Work")).unwrap();

    let record = create_flow(
        &central,
        Some("2026-09-09-1400"),
        None,
        Some("A loose thought".into()),
        "frank",
        "human",
        None,
    )
    .expect("the root register accepts a Flow");

    assert!(
        record.path.starts_with(&format!("{ROOT_FLOW_DIR}/")),
        "the Flow lands in the root NOW field: {}",
        record.path
    );
    assert_eq!(record.scope_ref, "control:root");
    assert!(
        record.flow_ref.starts_with("central:flow:control:root:"),
        "the Flow names the root register: {}",
        record.flow_ref
    );
    assert!(
        record
            .source_ref
            .starts_with("central:source:control:root:"),
        "the source names the root register: {}",
        record.source_ref
    );
    assert!(central.join(&record.path).is_file());

    let written = write_flow(
        &central,
        &record.flow_ref,
        &record.current_revision,
        "A thought that belongs to no one project.\n",
        "frank",
        "human",
        None,
    )
    .expect("the root register accepts a revision");
    let reading = read_flow(&central, &record.flow_ref).expect("the root Flow reads back");
    assert_eq!(
        reading.content,
        "A thought that belongs to no one project.\n"
    );
    assert_ne!(written.current_revision, record.current_revision);
    assert!(written.revisions.len() >= 2, "history accrues at the root");

    let _ = fs::remove_dir_all(&central);
}
