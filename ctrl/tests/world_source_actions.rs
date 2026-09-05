use central_ctrl::{
    create_core_action_registry, create_default_connector_registry, create_flow, read_flow,
    read_project_change_horizon, read_world_source, run_cli, write_world_source,
    ActionExecutionContext, CliEnvironment, ConnectorContext, ConnectorRegistry, ResultStatus,
    RootOptions, WORLD_SOURCE_READING_SCHEMA, WORLD_SOURCE_WRITE_RECEIPT_SCHEMA,
};
use central_ctrl::projectcentral_ops::register_projectcentral_actions;
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static NEXT_TEMP_ROOT: AtomicU64 = AtomicU64::new(0);

struct TempRoot(PathBuf);

impl TempRoot {
    fn new(label: &str) -> Self {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let sequence = NEXT_TEMP_ROOT.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "central-world-source-{label}-{}-{nonce}-{sequence}",
            std::process::id()
        ));
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

fn project_fixture(label: &str, name: &str) -> (TempRoot, PathBuf, PathBuf) {
    let temp = TempRoot::new(label);
    let central = temp.path().join("Central");
    let project = central.join("Work").join(name);
    fs::create_dir_all(&project).unwrap();
    central_ctrl::projectcentral_ops::initialize_projectcentral(&central, &project, &format!("example/{name}")).unwrap();
    (temp, central, project)
}

fn run_action(action: &str, input: Value, central: &Path) -> central_ctrl::ActionResult {
    let options: &'static RootOptions = Box::leak(Box::new(RootOptions {
        explicit_root: Some(central.to_path_buf()),
        configured_root: None,
        home: None,
    }));
    let connectors: &'static ConnectorRegistry = Box::leak(Box::new(create_default_connector_registry()));
    let connector_context: &'static ConnectorContext = Box::leak(Box::new(ConnectorContext::current()));
    let context = ActionExecutionContext { root_options: options, connectors, connector_context };
    let mut registry = create_core_action_registry();
    register_projectcentral_actions(&mut registry);
    registry.execute(action, &input, &context)
}

fn source_ref_of(horizon: &central_ctrl::SourceHorizon, path_suffix: &str) -> String {
    horizon
        .sources
        .iter()
        .find(|source| source.binding.path.ends_with(path_suffix))
        .unwrap_or_else(|| panic!("no participating source at {path_suffix}"))
        .binding
        .source_ref
        .clone()
}

fn current_revision(project: &Path, source_ref: &str) -> String {
    let horizon = read_project_change_horizon(project, None).unwrap();
    horizon
        .sources
        .iter()
        .find(|source| source.binding.source_ref == source_ref)
        .unwrap()
        .revision
        .revision
        .clone()
}

#[test]
fn world_source_read_discloses_one_source_with_its_exact_revision_and_provenance() {
    let (_temp, central, project) = project_fixture("read", "read-project");
    let source = project.join("ProjectCentral/user/intent.md");
    fs::write(&source, "keep ordinary files ordinary\n").unwrap();

    let horizon = read_project_change_horizon(&project, None).unwrap();
    let source_ref = source_ref_of(&horizon, "intent.md");

    let reading = run_action(
        "projectcentral.source.read",
        json!({ "project": "read-project", "source_ref": source_ref }),
        &central,
    );
    assert_eq!(reading.status, ResultStatus::Success, "{:?}", reading.error);
    let data = reading.data.unwrap();
    assert_eq!(data["schema"], WORLD_SOURCE_READING_SCHEMA);
    assert_eq!(data["content"], "keep ordinary files ordinary\n");
    assert_eq!(data["content_encoding"], "utf-8");
    assert_eq!(data["revision"]["revision"], current_revision(&project, &source_ref));
    assert_eq!(data["source"]["ref"], source_ref);
    assert_eq!(data["source"]["path"], "ProjectCentral/user/intent.md");
    assert_eq!(data["world_ref"], horizon.world_ref);
    assert_eq!(data["automatic_agent_or_model_invocation"], false);

    // The horizon itself still carries no payloads; disclosure is this Action,
    // one named source at a time.
    assert!(!horizon.source_payloads_exposed);
    let serialized = serde_json::to_string(&horizon).unwrap();
    assert!(!serialized.contains("keep ordinary files ordinary"));
}

#[test]
fn world_source_read_refuses_non_participating_and_masked_source() {
    let (_temp, central, project) = project_fixture("refuse-read", "refuse-read-project");
    fs::write(project.join("ProjectCentral/user/intent.md"), "open\n").unwrap();
    let private = project.join("ProjectCentral/user/private");
    fs::create_dir_all(&private).unwrap();
    fs::write(private.join(".no-agent-retrieval"), "").unwrap();
    fs::write(private.join("secret.md"), "not for retrieval\n").unwrap();

    let horizon = read_project_change_horizon(&project, None).unwrap();
    let masked_ref = source_ref_of(&horizon, "secret.md");

    let unknown = run_action(
        "projectcentral.source.read",
        json!({ "project": "refuse-read-project", "source_ref": "central:source:project:x:absent.md" }),
        &central,
    );
    assert_eq!(unknown.status, ResultStatus::InvalidInput);

    let masked = run_action(
        "projectcentral.source.read",
        json!({ "project": "refuse-read-project", "source_ref": masked_ref }),
        &central,
    );
    assert_eq!(masked.status, ResultStatus::UnavailableCapability);
    assert!(masked.error.unwrap().message.contains(".no-agent-retrieval"));

    let masked_library = read_world_source(&project, &masked_ref);
    assert!(masked_library.is_err());
}

#[test]
fn world_source_write_is_cas_and_attributes_the_emitted_change() {
    let (_temp, central, project) = project_fixture("write", "write-project");
    let source = project.join("ProjectCentral/agents/wiki/notes.md");
    fs::create_dir_all(source.parent().unwrap()).unwrap();
    fs::write(&source, "v1\n").unwrap();
    let horizon = read_project_change_horizon(&project, None).unwrap();
    let source_ref = source_ref_of(&horizon, "notes.md");
    let basis = current_revision(&project, &source_ref);
    let cursor_before = horizon.cursor;

    let written = run_action(
        "projectcentral.source.write",
        json!({
            "project": "write-project",
            "source_ref": source_ref,
            "expected_revision": basis,
            "content": "v2\n",
            "actor": "human:cradle",
            "actor_kind": "human"
        }),
        &central,
    );
    assert_eq!(written.status, ResultStatus::Success, "{:?}", written.error);
    let receipt = written.data.unwrap()["receipt"].clone();
    assert_eq!(receipt["schema"], WORLD_SOURCE_WRITE_RECEIPT_SCHEMA);
    assert_eq!(receipt["changed"], true);
    assert_eq!(receipt["previous_revision"], basis);
    assert_eq!(receipt["actor"], "human:cradle");
    assert_eq!(receipt["actor_kind"], "human");
    assert_eq!(receipt["automatic_agent_or_model_invocation"], false);
    let new_revision = receipt["revision"]["revision"].as_str().unwrap().to_owned();
    assert_ne!(new_revision, basis);
    assert_eq!(fs::read_to_string(&source).unwrap(), "v2\n");

    let after = read_project_change_horizon(&project, None).unwrap();
    assert_eq!(after.cursor, cursor_before + 1);
    let change = after.changes.iter().find(|change| change.source_ref == source_ref).unwrap();
    assert_eq!(change.change_ref, receipt["change_ref"]);
    assert_eq!(change.actor.as_deref(), Some("human:cradle"));
    assert_eq!(change.actor_kind.as_deref(), Some("human"));
    assert_eq!(change.before_revision.as_deref(), Some(basis.as_str()));
    assert_eq!(change.after_revision.as_deref(), Some(new_revision.as_str()));

    // An agent session write on the same working source keeps its session lineage.
    let session_basis = current_revision(&project, &source_ref);
    let agent = run_action(
        "projectcentral.source.write",
        json!({
            "project": "write-project",
            "source_ref": source_ref,
            "expected_revision": session_basis,
            "content": "v2\nagent note\n",
            "actor": "agent:epii",
            "actor_kind": "agent",
            "agent_session_ref": "aikit:agent-session:1"
        }),
        &central,
    );
    assert_eq!(agent.status, ResultStatus::Success, "{:?}", agent.error);
    let agent_receipt = agent.data.unwrap()["receipt"].clone();
    assert_eq!(agent_receipt["agent_session_ref"], "aikit:agent-session:1");
    let after_agent = read_project_change_horizon(&project, None).unwrap();
    let agent_change = after_agent
        .changes
        .iter()
        .find(|change| Some(change.change_ref.as_str()) == agent_receipt["change_ref"].as_str())
        .unwrap();
    assert_eq!(agent_change.actor.as_deref(), Some("agent:epii"));
    assert_eq!(agent_change.actor_kind.as_deref(), Some("agent"));
    assert_eq!(agent_change.agent_session_ref.as_deref(), Some("aikit:agent-session:1"));
    assert!(!after_agent.automatic_agent_or_model_invocation);
}

#[test]
fn stale_revision_fails_without_mutating_source_or_horizon() {
    let (_temp, central, project) = project_fixture("conflict", "conflict-project");
    let source = project.join("ProjectCentral/agents/wiki/notes.md");
    fs::create_dir_all(source.parent().unwrap()).unwrap();
    fs::write(&source, "current\n").unwrap();
    let horizon = read_project_change_horizon(&project, None).unwrap();
    let source_ref = source_ref_of(&horizon, "notes.md");

    let stale = run_action(
        "projectcentral.source.write",
        json!({
            "project": "conflict-project",
            "source_ref": source_ref,
            "expected_revision": "central.content-fnv1a64/v1:0:0000000000000000",
            "content": "overwritten\n",
            "actor": "agent:epii",
            "actor_kind": "agent"
        }),
        &central,
    );
    assert_eq!(stale.status, ResultStatus::InvalidInput);
    let message = stale.error.unwrap().message;
    assert!(message.contains("revision conflict"), "{message}");
    assert_eq!(fs::read_to_string(&source).unwrap(), "current\n");
    let after = read_project_change_horizon(&project, None).unwrap();
    assert_eq!(after.cursor, horizon.cursor);
    assert!(after.changes.iter().all(|change| change.source_ref != source_ref));
}

#[test]
fn non_human_callers_propose_rather_than_write_authored_human_ground() {
    let (_temp, central, project) = project_fixture("authority", "authority-project");
    fs::write(project.join("ProjectCentral/user/intent.md"), "mine\n").unwrap();
    let governance = project.join("ProjectCentral/agents/governance");
    fs::create_dir_all(&governance).unwrap();
    fs::write(governance.join("repo-structure.md"), "# Where things go\n").unwrap();
    fs::write(project.join("README.md"), "# Purpose\n").unwrap();
    let relation_path = project.join(central_ctrl::GROUND_RELATIONS_SOURCE);
    fs::create_dir_all(relation_path.parent().unwrap()).unwrap();
    fs::write(
        &relation_path,
        serde_json::to_vec_pretty(&json!({
            "schema": "central.project.ground-relations/v1",
            "project_id": "example/authority-project",
            "relations": [{
                "ref": "central:ground:purpose",
                "path": "README.md",
                "provenance": "human-adopted",
                "standing": "authored-human-position",
                "roles": ["purpose"],
                "treatment": "retain-native-in-place",
                "recognition": "human-accepted source relation",
                "recorded_at_unix_seconds": 1
            }]
        }))
        .unwrap(),
    )
    .unwrap();

    let horizon = read_project_change_horizon(&project, None).unwrap();
    let aperture_ref = source_ref_of(&horizon, "intent.md");
    let governance_ref = source_ref_of(&horizon, "repo-structure.md");
    let adopted_ref = source_ref_of(&horizon, "README.md");

    for (source_ref, label) in [
        (aperture_ref.clone(), "human source aperture"),
        (governance_ref, "agent governance"),
        (adopted_ref, "adopted human ground"),
    ] {
        let refused = run_action(
            "projectcentral.source.write",
            json!({
                "project": "authority-project",
                "source_ref": source_ref,
                "expected_revision": current_revision(&project, &source_ref),
                "content": "agent revision\n",
                "actor": "agent:epii",
                "actor_kind": "agent"
            }),
            &central,
        );
        assert_eq!(
            refused.status,
            ResultStatus::UnavailableCapability,
            "{label} should refuse a non-human writer: {:?}",
            refused.error
        );
    }

    // The human writes their own ground through the same Action and the same gate.
    let human = run_action(
        "projectcentral.source.write",
        json!({
            "project": "authority-project",
            "source_ref": aperture_ref,
            "expected_revision": current_revision(&project, &aperture_ref),
            "content": "mine, revised\n",
            "actor": "human:cradle",
            "actor_kind": "human"
        }),
        &central,
    );
    assert_eq!(human.status, ResultStatus::Success, "{:?}", human.error);
    assert_eq!(
        fs::read_to_string(project.join("ProjectCentral/user/intent.md")).unwrap(),
        "mine, revised\n"
    );
}

#[test]
fn an_agent_session_cannot_claim_human_authorship_to_write_human_ground() {
    let (_temp, central, project) = project_fixture("self-declared", "self-declared-project");
    let source = project.join("ProjectCentral/user/intent.md");
    fs::write(&source, "held by the human\n").unwrap();
    let horizon = read_project_change_horizon(&project, None).unwrap();
    let source_ref = source_ref_of(&horizon, "intent.md");

    // Declaring actor_kind human while carrying an agent session is an
    // incoherent declaration, refused before anything is written — the ledger
    // must never record an agent session's write as human authorship.
    let claimed = run_action(
        "projectcentral.source.write",
        json!({
            "project": "self-declared-project",
            "source_ref": source_ref,
            "expected_revision": current_revision(&project, &source_ref),
            "content": "written by the session, declared human\n",
            "actor": "agent:epii",
            "actor_kind": "human",
            "agent_session_ref": "aikit:agent-session:9"
        }),
        &central,
    );
    assert_eq!(claimed.status, ResultStatus::InvalidInput, "{:?}", claimed.error);
    assert!(
        claimed
            .error
            .unwrap()
            .message
            .contains("does not also carry an agent_session_ref")
    );
    assert_eq!(fs::read_to_string(&source).unwrap(), "held by the human\n");
    let after = read_project_change_horizon(&project, None).unwrap();
    assert_eq!(after.cursor, horizon.cursor);
    assert!(after.changes.iter().all(|change| change.source_ref != source_ref));
    assert_eq!(
        after
            .sources
            .iter()
            .find(|source| source.binding.source_ref == source_ref)
            .unwrap()
            .revision
            .revision,
        current_revision(&project, &source_ref)
    );

    // The same incoherent declaration is refused on a working source too: it is
    // the declaration that is invalid, not only the ground it targets.
    let working = project.join("ProjectCentral/agents/wiki/notes.md");
    fs::create_dir_all(working.parent().unwrap()).unwrap();
    fs::write(&working, "session notes\n").unwrap();
    let working_ref = source_ref_of(&read_project_change_horizon(&project, None).unwrap(), "notes.md");
    let claimed_working = run_action(
        "projectcentral.source.write",
        json!({
            "project": "self-declared-project",
            "source_ref": working_ref,
            "expected_revision": current_revision(&project, &working_ref),
            "content": "still incoherent\n",
            "actor": "agent:epii",
            "actor_kind": "human",
            "agent_session_ref": "aikit:agent-session:9"
        }),
        &central,
    );
    assert_eq!(claimed_working.status, ResultStatus::InvalidInput);
    assert_eq!(fs::read_to_string(&working).unwrap(), "session notes\n");

    // A coherent human declaration without a session still writes its own
    // aperture ground through the same Action.
    let human = run_action(
        "projectcentral.source.write",
        json!({
            "project": "self-declared-project",
            "source_ref": source_ref,
            "expected_revision": current_revision(&project, &source_ref),
            "content": "revised by the human\n",
            "actor": "human:cradle",
            "actor_kind": "human"
        }),
        &central,
    );
    assert_eq!(human.status, ResultStatus::Success, "{:?}", human.error);
    assert_eq!(fs::read_to_string(&source).unwrap(), "revised by the human\n");
}

#[test]
fn world_source_seam_and_flow_seam_compose_over_one_source_ref() {
    let (_temp, _central, project) = project_fixture("flow-compose", "flow-compose-project");
    let flow = create_flow(
        &project,
        Some("2026-09-04-1200"),
        None,
        Some("Flow thread".to_owned()),
        "human:cradle",
        "human",
        None,
    )
    .unwrap();

    let horizon = read_project_change_horizon(&project, None).unwrap();
    let flow_source_ref = source_ref_of(&horizon, &flow.path);

    let reading = read_world_source(&project, &flow_source_ref).unwrap();
    assert_eq!(reading.content, "");
    assert_eq!(reading.revision.revision, flow.current_revision);

    let basis = current_revision(&project, &flow_source_ref);
    let receipt = write_world_source(
        &project,
        &flow_source_ref,
        &basis,
        "Flow thread\ncontinued\n",
        "agent:epii",
        "agent",
        Some("aikit:agent-session:2".to_owned()),
    )
    .unwrap();
    assert!(receipt.changed);

    // One source, one content, two owner seams: the Flow reading reconciles the
    // source-level write as an external revision while the Horizon change keeps
    // the true attribution.
    let flow_after = read_flow(&project, &flow.flow_ref).unwrap();
    assert_eq!(flow_after.content, "Flow thread\ncontinued\n");
    assert_eq!(flow_after.flow.current_revision, receipt.revision.revision);
    assert!(flow_after.dirty_external_revision_reconciled);
}

#[test]
fn cli_doorway_discovers_and_serves_world_source_actions() {
    let (_temp, central, project) = project_fixture("cli", "cli-project");
    fs::write(project.join("ProjectCentral/user/intent.md"), "via cli\n").unwrap();
    let environment = CliEnvironment { configured_root: Some(central.clone()), home: None };

    let listed = run_cli(&["--json".to_owned(), "action".to_owned(), "list".to_owned()], &environment);
    assert_eq!(listed.result.status, ResultStatus::Success);
    let actions: Value = listed.result.data.unwrap();
    let ids: Vec<String> = actions["actions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|descriptor| descriptor["id"].as_str().unwrap().to_owned())
        .collect();
    assert!(ids.contains(&"projectcentral.source.read".to_owned()), "{ids:?}");
    assert!(ids.contains(&"projectcentral.source.write".to_owned()), "{ids:?}");

    let horizon = read_project_change_horizon(&project, None).unwrap();
    let source_ref = source_ref_of(&horizon, "intent.md");

    let read = run_cli(
        &[
            "--json".to_owned(),
            "action".to_owned(),
            "run".to_owned(),
            "projectcentral.source.read".to_owned(),
            json!({ "project": "cli-project", "source_ref": source_ref }).to_string(),
        ],
        &environment,
    );
    assert_eq!(read.result.status, ResultStatus::Success);
    assert_eq!(read.result.data.unwrap()["content"], "via cli\n");

    let basis = current_revision(&project, &source_ref);
    let write = run_cli(
        &[
            "--json".to_owned(),
            "action".to_owned(),
            "run".to_owned(),
            "projectcentral.source.write".to_owned(),
            json!({
                "project": "cli-project",
                "source_ref": source_ref,
                "expected_revision": basis,
                "content": "via cli, revised\n",
                "actor": "human:cradle",
                "actor_kind": "human"
            })
            .to_string(),
        ],
        &environment,
    );
    assert_eq!(write.result.status, ResultStatus::Success);
    let receipt = write.result.data.unwrap()["receipt"].clone();
    assert_eq!(receipt["changed"], true);
    assert_eq!(
        fs::read_to_string(project.join("ProjectCentral/user/intent.md")).unwrap(),
        "via cli, revised\n"
    );

    let history = run_cli(
        &[
            "--json".to_owned(),
            "action".to_owned(),
            "run".to_owned(),
            "projectcentral.change.horizon".to_owned(),
            json!({ "project": "cli-project" }).to_string(),
        ],
        &environment,
    );
    assert_eq!(history.result.status, ResultStatus::Success);
    let horizon_data = history.result.data.unwrap();
    let change = horizon_data["changes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|change| change["source_ref"] == json!(source_ref))
        .unwrap()
        .clone();
    assert_eq!(change["actor"], json!("human:cradle"));
    assert_eq!(change["actor_kind"], json!("human"));
    assert_eq!(horizon_data["source_payloads_exposed"], json!(false));
}

#[test]
fn ground_inspection_discloses_the_ref_source_read_accepts() {
    let (temp, central, project) = project_fixture("canonical-ref", "canonical-project");
    let user = project.join("ProjectCentral").join("user").join("notes");
    fs::create_dir_all(&user).unwrap();
    fs::write(user.join("one.md"), "one").unwrap();
    fs::write(user.join("two.md"), "two").unwrap();
    fs::write(user.join("three.md"), "three").unwrap();

    // U0.2 (cradle map D12): the ref ground inspection discloses and the ref
    // source.read/write accept are one canonical grammar, byte-identical.
    let inspection = central_ctrl::inspect_project_ground(&project).unwrap();
    assert!(inspection.projectcentral_ready);
    let expected_prefix = format!(
        "central:source:project:{}:",
        inspection.project_id.clone().unwrap()
    );
    let disclosed: Vec<String> = inspection
        .account_handoff
        .other_source_relations
        .iter()
        .map(|record| record.source_ref.clone())
        .collect();
    assert_eq!(disclosed.len(), 3, "three user-aperture sources disclosed");
    for source_ref in &disclosed {
        assert!(
            source_ref.starts_with(&expected_prefix),
            "canonical horizon grammar, got {source_ref}"
        );
        let reading = central_ctrl::read_world_source(&project, &source_ref).unwrap();
        assert_eq!(reading.source.source_ref, *source_ref);
    }

    // Round-trip: the inspect-disclosed ref drives a CAS read/write cycle.
    let first = disclosed[0].clone();
    let reading = central_ctrl::read_world_source(&project, &first).unwrap();
    let receipt = central_ctrl::write_world_source(
        &project,
        &first,
        &reading.revision.revision,
        "one revised",
        "human:cradle",
        "human",
        None,
    )
    .unwrap();
    assert!(receipt.changed);
    assert_ne!(receipt.previous_revision, receipt.revision.revision);
    let reopened = central_ctrl::read_world_source(&project, &first).unwrap();
    assert_eq!(reopened.content, "one revised");
    assert_eq!(reopened.revision.revision, receipt.revision.revision);
    drop(temp);
}
