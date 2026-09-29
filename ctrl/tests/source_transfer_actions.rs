use central_ctrl::projectcentral_ops::register_projectcentral_actions;
use central_ctrl::source_transfer::{SOURCE_TRANSFER_APPLY_RECEIPT_SCHEMA, SOURCE_TRANSFER_SCHEMA};
use central_ctrl::{
    create_core_action_registry, create_default_connector_registry, read_project_change_horizon,
    run_cli, ActionExecutionContext, CliEnvironment, ConnectorContext, ConnectorRegistry,
    ResultStatus, RootOptions,
};
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static NEXT_TEMP_ROOT: AtomicU64 = AtomicU64::new(0);

struct TempRoot(PathBuf);

impl TempRoot {
    fn new(label: &str) -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let sequence = NEXT_TEMP_ROOT.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "central-source-transfer-{label}-{}-{nonce}-{sequence}",
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

/// One ground of the shared world: the same project id names the same world
/// identity on both grounds, which is exactly what makes their grounds
/// related and their source refs mutually grammatical.
fn ground(label: &str, name: &str) -> (TempRoot, PathBuf, PathBuf) {
    let temp = TempRoot::new(label);
    let central = temp.path().join("Central");
    let project = central.join("Work").join(name);
    fs::create_dir_all(&project).unwrap();
    central_ctrl::projectcentral_ops::initialize_projectcentral(
        &central,
        &project,
        &format!("example/{name}"),
    )
    .unwrap();
    (temp, central, project)
}

fn run_action(action: &str, input: Value, central: &Path) -> central_ctrl::ActionResult {
    let options: &'static RootOptions = Box::leak(Box::new(RootOptions {
        explicit_root: Some(central.to_path_buf()),
        configured_root: None,
        home: None,
    }));
    let connectors: &'static ConnectorRegistry =
        Box::leak(Box::new(create_default_connector_registry()));
    let connector_context: &'static ConnectorContext =
        Box::leak(Box::new(ConnectorContext::current()));
    let context = ActionExecutionContext {
        root_options: options,
        connectors,
        connector_context,
    };
    let mut registry = create_core_action_registry();
    register_projectcentral_actions(&mut registry);
    central_ctrl::register_agent_profile_actions(&mut registry);
    central_ctrl::agent_set_actions::register_agent_set_actions(&mut registry);
    registry.execute(action, &input, &context)
}

fn write_external(project: &Path, relative: &str, content: &str) {
    let path = project.join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
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

fn write_via_action(
    central: &Path,
    project: &Path,
    source_ref: &str,
    expected: &str,
    content: &str,
) -> String {
    let result = run_action(
        "projectcentral.source.write",
        json!({
            "project": project.file_name().unwrap().to_string_lossy(),
            "source_ref": source_ref,
            "expected_revision": expected,
            "content": content,
            "actor": "transfer-test",
            "actor_kind": "agent",
            "agent_session_ref": "session/transfer-test",
        }),
        central,
    );
    assert_eq!(result.status, ResultStatus::Success, "{:?}", result.error);
    let data = result.data.unwrap();
    data["receipt"]["revision"]["revision"]
        .as_str()
        .unwrap()
        .to_owned()
}

/// Origin ground A and receiving ground B of one world. The fork content is
/// written on A after A's horizon exists, then identical bytes are placed on
/// B before B's horizon ever reconciles: both grounds then hold the same
/// content revision as the beginning of their shared lineage, without any
/// horizon state copied between them.
fn forked_grounds(
    label: &str,
) -> (
    (TempRoot, PathBuf, PathBuf),
    (TempRoot, PathBuf, PathBuf),
    String,
) {
    let a = ground(&format!("{label}-a"), "case6");
    let b = ground(&format!("{label}-b"), "case6");
    let _ = read_project_change_horizon(&a.2, None).unwrap();
    write_external(
        &a.2,
        "ProjectCentral/agents/wiki/case6-note.md",
        "shared fork\n",
    );
    let horizon = read_project_change_horizon(&a.2, None).unwrap();
    let source_ref = source_ref_of(&horizon, "case6-note.md");
    write_external(
        &b.2,
        "ProjectCentral/agents/wiki/case6-note.md",
        "shared fork\n",
    );
    let _ = read_project_change_horizon(&b.2, None).unwrap();
    assert_eq!(
        current_revision(&a.2, &source_ref),
        current_revision(&b.2, &source_ref)
    );
    (a, b, source_ref)
}

fn export(
    central: &Path,
    project: &Path,
    to_world_ref: &str,
    source_ref: &str,
    since_cursor: Option<u64>,
) -> central_ctrl::ActionResult {
    let mut input = json!({
        "project": project.file_name().unwrap().to_string_lossy(),
        "source_refs": [source_ref],
        "to_world_ref": to_world_ref,
        "from_ground": "origin-ground",
        "to_ground": "receiving-ground",
        "actor": "origin-operator",
        "actor_kind": "agent",
        "agent_session_ref": "session/origin",
    });
    if let Some(cursor) = since_cursor {
        input["since_cursor"] = json!(cursor);
    }
    run_action("projectcentral.source.transfer.export", input, central)
}

fn apply_bundle(
    central: &Path,
    project: &Path,
    bundle_file: &Path,
    extra: Value,
) -> central_ctrl::ActionResult {
    let mut input = json!({
        "project": project.file_name().unwrap().to_string_lossy(),
        "bundle_file": bundle_file.to_string_lossy(),
        "actor": "receiving-operator",
        "actor_kind": "agent",
        "agent_session_ref": "session/receiving",
    });
    if let Some(extra) = extra.as_object() {
        for (key, value) in extra {
            input[key.as_str()] = value.clone();
        }
    }
    run_action("projectcentral.source.transfer.apply", input, central)
}

fn world_ref_of(project: &Path) -> String {
    read_project_change_horizon(project, None)
        .unwrap()
        .world_ref
}

fn write_bundle_file(dir: &Path, payload: &Value) -> PathBuf {
    let path = dir.join("bundle.json");
    fs::write(&path, serde_json::to_vec_pretty(payload).unwrap()).unwrap();
    path
}

#[test]
fn export_carries_scope_direction_and_payloads_without_absolute_paths() {
    let (_a, central_a, project_a) = ground("export", "case6");
    let _ = read_project_change_horizon(&project_a, None).unwrap();
    write_external(
        &project_a,
        "ProjectCentral/agents/wiki/case6-note.md",
        "shared fork\n",
    );
    let horizon = read_project_change_horizon(&project_a, None).unwrap();
    let source_ref = source_ref_of(&horizon, "case6-note.md");
    write_via_action(
        &central_a,
        &project_a,
        &source_ref,
        &current_revision(&project_a, &source_ref),
        "changed on origin\n",
    );

    let world_ref = world_ref_of(&project_a);
    let result = export(
        &central_a,
        &project_a,
        "project:example/elsewhere",
        &source_ref,
        Some(1),
    );
    assert_eq!(result.status, ResultStatus::Success, "{:?}", result.error);
    let bundle = result.data.unwrap();
    assert_eq!(bundle["schema"], SOURCE_TRANSFER_SCHEMA);
    assert_eq!(bundle["direction"]["from_world_ref"], world_ref.as_str());
    assert_eq!(
        bundle["direction"]["to_world_ref"],
        "project:example/elsewhere"
    );
    assert_eq!(bundle["direction"]["from_ground"], "origin-ground");
    assert_eq!(bundle["direction"]["to_ground"], "receiving-ground");
    assert_eq!(bundle["scope"]["source_refs"][0], source_ref.as_str());
    assert_eq!(bundle["scope"]["since_cursor"], 1);
    assert_eq!(bundle["payloads_included"], true);
    assert_eq!(bundle["sources"].as_array().unwrap().len(), 1);
    let entry = &bundle["sources"][0];
    assert_eq!(entry["kind"], "modified");
    assert_eq!(entry["content"], "changed on origin\n");
    assert!(entry["base_revision"].is_string());
    assert_eq!(
        entry["after_revision"],
        current_revision(&project_a, &source_ref).as_str()
    );
    // The bundle names worlds, relative paths and revisions - never the
    // machine's own locations.
    let raw = serde_json::to_string(&bundle).unwrap();
    assert!(!raw.contains(central_a.to_string_lossy().as_ref()));
    assert!(!raw.contains(&temp_dir_string()));
}

#[test]
fn export_refuses_own_ground_destination_and_foreign_and_masked_sources() {
    let (_a, central_a, project_a) = ground("export-refuse", "case6");
    let _ = read_project_change_horizon(&project_a, None).unwrap();
    write_external(
        &project_a,
        "ProjectCentral/agents/wiki/case6-note.md",
        "shared fork\n",
    );
    let horizon = read_project_change_horizon(&project_a, None).unwrap();
    let _source_ref = source_ref_of(&horizon, "case6-note.md");

    let foreign = export(
        &central_a,
        &project_a,
        "project:example/elsewhere",
        "central:source:project:example/other:x.md",
        None,
    );
    assert_eq!(foreign.status, ResultStatus::InvalidInput);

    // A masked source participates but is never disclosed or exported:
    // masking is enforced before any payload is read.
    write_external(
        &project_a,
        "ProjectCentral/agents/wiki/masked/.no-agent-retrieval",
        "",
    );
    write_external(
        &project_a,
        "ProjectCentral/agents/wiki/masked/secret.md",
        "masked\n",
    );
    let horizon = read_project_change_horizon(&project_a, None).unwrap();
    let masked = horizon
        .sources
        .iter()
        .find(|source| source.binding.path.ends_with("masked/secret.md"))
        .expect("masked source participates with disclosure refused");
    assert!(!masked.binding.agent_retrieval_allowed);
    let refused = export(
        &central_a,
        &project_a,
        "project:example/elsewhere",
        &masked.binding.source_ref,
        None,
    );
    assert_eq!(
        refused.status,
        ResultStatus::UnavailableCapability,
        "{:?}",
        refused.error
    );
    assert!(refused
        .error
        .unwrap()
        .message
        .contains("no-agent-retrieval"));
}

#[test]
fn apply_fast_forwards_from_the_recorded_base_and_emits_an_attributed_change() {
    let ((_temp_a, central_a, project_a), (temp_b, central_b, project_b), source_ref) =
        forked_grounds("fast-forward");
    let revision_a1 = write_via_action(
        &central_a,
        &project_a,
        &source_ref,
        &current_revision(&project_a, &source_ref),
        "changed on origin\n",
    );

    // Scope the transfer to the changes after the shared fork.
    let world_ref = world_ref_of(&project_b);
    let exported = export(&central_a, &project_a, &world_ref, &source_ref, Some(1));
    assert_eq!(
        exported.status,
        ResultStatus::Success,
        "{:?}",
        exported.error
    );
    let bundle = exported.data.unwrap();
    assert_eq!(bundle["sources"][0]["kind"], "modified");
    assert_eq!(
        bundle["sources"][0]["base_revision"],
        current_revision(&project_b, &source_ref).as_str()
    );
    let bundle_file = write_bundle_file(temp_b.path(), &bundle);

    let applied = apply_bundle(&central_b, &project_b, &bundle_file, json!({}));
    assert_eq!(applied.status, ResultStatus::Success, "{:?}", applied.error);
    let receipt = applied.data.unwrap();
    assert_eq!(receipt["schema"], SOURCE_TRANSFER_APPLY_RECEIPT_SCHEMA);
    assert_eq!(receipt["status"], "applied");
    assert_eq!(receipt["conflicted_count"], 0);
    assert_eq!(receipt["outcomes"][0]["outcome"], "applied");
    assert_eq!(receipt["outcomes"][0]["revision"], revision_a1.as_str());
    assert_eq!(current_revision(&project_b, &source_ref), revision_a1);
    assert_eq!(
        fs::read_to_string(project_b.join("ProjectCentral/agents/wiki/case6-note.md")).unwrap(),
        "changed on origin\n"
    );

    // The receiving ground's horizon carries the transfer's declared actor.
    let horizon = read_project_change_horizon(&project_b, None).unwrap();
    let change = horizon
        .changes
        .iter()
        .find(|change| change.source_ref == source_ref)
        .unwrap();
    assert_eq!(change.actor.as_deref(), Some("receiving-operator"));
    assert_eq!(change.actor_kind.as_deref(), Some("agent"));

    // Replaying the same bundle is idempotent: nothing moves, one receipt.
    let replay = apply_bundle(&central_b, &project_b, &bundle_file, json!({}));
    assert_eq!(replay.status, ResultStatus::Success, "{:?}", replay.error);
    let receipt = replay.data.unwrap();
    assert_eq!(receipt["status"], "applied");
    assert_eq!(receipt["outcomes"][0]["outcome"], "already-applied");
    let records = fs::read_dir(project_b.join(".central/source-transfer/records"))
        .unwrap()
        .count();
    assert_eq!(records, 1, "one receipt per distinct bundle");
}

#[test]
fn divergent_change_conflicts_is_recorded_and_overwrites_nothing() {
    let ((_temp_a, central_a, project_a), (temp_b, central_b, project_b), source_ref) =
        forked_grounds("conflict");
    let revision_shared = current_revision(&project_a, &source_ref);

    // Both grounds move, in different directions, from the shared revision.
    let origin_revision = write_via_action(
        &central_a,
        &project_a,
        &source_ref,
        &revision_shared,
        "origin's divergent line\n",
    );
    let local_revision = write_via_action(
        &central_b,
        &project_b,
        &source_ref,
        &revision_shared,
        "receiver's own divergent line\n",
    );

    let world_ref = world_ref_of(&project_b);
    let exported = export(&central_a, &project_a, &world_ref, &source_ref, Some(1));
    assert_eq!(
        exported.status,
        ResultStatus::Success,
        "{:?}",
        exported.error
    );
    let bundle = exported.data.unwrap();
    assert_eq!(
        bundle["sources"][0]["base_revision"],
        revision_shared.as_str()
    );
    assert_eq!(
        bundle["sources"][0]["after_revision"],
        origin_revision.as_str()
    );
    let bundle_file = write_bundle_file(temp_b.path(), &bundle);

    let applied = apply_bundle(&central_b, &project_b, &bundle_file, json!({}));
    assert_eq!(applied.status, ResultStatus::Success, "{:?}", applied.error);
    let receipt = applied.data.unwrap();
    assert_eq!(receipt["status"], "conflicted");
    assert_eq!(receipt["conflicted_count"], 1);
    assert_eq!(receipt["outcomes"][0]["outcome"], "conflicted");
    let conflict_ref = receipt["outcomes"][0]["conflict_ref"]
        .as_str()
        .unwrap()
        .to_owned();

    // The receiving source is byte-identical: the conflict overwrote nothing.
    assert_eq!(
        fs::read_to_string(project_b.join("ProjectCentral/agents/wiki/case6-note.md")).unwrap(),
        "receiver's own divergent line\n"
    );
    assert_eq!(current_revision(&project_b, &source_ref), local_revision);

    // The conflict is surfaced with both revisions named and the resolution
    // path stated.
    let listed = run_action(
        "projectcentral.source.transfer.conflicts",
        json!({"project": project_b.file_name().unwrap().to_string_lossy(), "status": "open"}),
        &central_b,
    );
    assert_eq!(listed.status, ResultStatus::Success, "{:?}", listed.error);
    let page = listed.data.unwrap();
    assert_eq!(page["open_conflicts"], 1);
    let record = &page["conflicts"][0];
    assert_eq!(record["conflict_ref"], conflict_ref.as_str());
    assert_eq!(record["source_ref"], source_ref.as_str());
    assert_eq!(record["base_revision"], revision_shared.as_str());
    assert_eq!(record["incoming_revision"], origin_revision.as_str());
    assert_eq!(record["local_revision"], local_revision.as_str());
    assert!(record["resolution_path"]
        .as_str()
        .unwrap()
        .contains("accept-incoming"));
    // Both sides of the divergence are snapshotted beside the record.
    let stem = conflict_ref.rsplit(':').next().unwrap();
    let local_snapshot = fs::read_to_string(project_b.join(format!(
        ".central/source-transfer/conflicts/{stem}/local-source.txt"
    )))
    .unwrap();
    assert!(local_snapshot.contains("receiver's own divergent line"));
}

#[test]
fn resolve_accept_incoming_requires_the_exact_recorded_local_revision() {
    let ((_temp_a, central_a, project_a), (temp_b, central_b, project_b), source_ref) =
        forked_grounds("resolve");
    let revision_shared = current_revision(&project_a, &source_ref);
    let _origin_revision = write_via_action(
        &central_a,
        &project_a,
        &source_ref,
        &revision_shared,
        "origin's divergent line\n",
    );
    let local_revision = write_via_action(
        &central_b,
        &project_b,
        &source_ref,
        &revision_shared,
        "receiver's own divergent line\n",
    );
    let world_ref = world_ref_of(&project_b);
    let exported = export(&central_a, &project_a, &world_ref, &source_ref, Some(1));
    let bundle_file = write_bundle_file(temp_b.path(), &exported.data.unwrap());
    let applied = apply_bundle(&central_b, &project_b, &bundle_file, json!({}));
    assert_eq!(applied.data.unwrap()["status"], "conflicted");

    let resolve_input = |extra: Value| {
        let mut input = json!({
            "project": project_b.file_name().unwrap().to_string_lossy(),
            "source_ref": source_ref,
            "disposition": "accept-incoming",
            "actor": "resolver",
            "actor_kind": "agent",
            "agent_session_ref": "session/resolver",
        });
        if let Some(extra) = extra.as_object() {
            for (key, value) in extra {
                input[key.as_str()] = value.clone();
            }
        }
        input
    };

    // A wrong basis is refused: the recorded divergence is the only basis.
    let wrong = run_action(
        "projectcentral.source.transfer.resolve",
        resolve_input(
            json!({"expected_local_revision": "central.content-fnv1a64/v1:0:cbf29ce484222325"}),
        ),
        &central_b,
    );
    assert_eq!(
        wrong.status,
        ResultStatus::InvalidInput,
        "{:?}",
        wrong.error
    );

    let result = run_action(
        "projectcentral.source.transfer.resolve",
        resolve_input(json!({"expected_local_revision": local_revision})),
        &central_b,
    );
    assert_eq!(result.status, ResultStatus::Success, "{:?}", result.error);
    let receipt = result.data.unwrap();
    assert_eq!(receipt["disposition"], "accept-incoming");
    assert_eq!(
        fs::read_to_string(project_b.join("ProjectCentral/agents/wiki/case6-note.md")).unwrap(),
        "origin's divergent line\n"
    );
    let horizon = read_project_change_horizon(&project_b, None).unwrap();
    let change = horizon
        .changes
        .iter()
        .rfind(|change| change.source_ref == source_ref)
        .unwrap();
    assert_eq!(change.actor.as_deref(), Some("resolver"));

    // The resolved record is retained evidence, not deleted.
    let listed = run_action(
        "projectcentral.source.transfer.conflicts",
        json!({"project": project_b.file_name().unwrap().to_string_lossy(), "status": "resolved"}),
        &central_b,
    );
    assert_eq!(
        listed.data.unwrap()["conflicts"].as_array().unwrap().len(),
        1
    );
    // And with no open conflict left, a repeat resolution refuses honestly.
    let repeat = run_action(
        "projectcentral.source.transfer.resolve",
        resolve_input(json!({"expected_local_revision": local_revision})),
        &central_b,
    );
    assert_eq!(repeat.status, ResultStatus::InvalidInput);
}

#[test]
fn resolve_keep_local_records_the_decision_without_touching_the_source() {
    let ((_temp_a, central_a, project_a), (temp_b, central_b, project_b), source_ref) =
        forked_grounds("keep-local");
    let revision_shared = current_revision(&project_a, &source_ref);
    let _ = write_via_action(
        &central_a,
        &project_a,
        &source_ref,
        &revision_shared,
        "origin's divergent line\n",
    );
    let local_revision = write_via_action(
        &central_b,
        &project_b,
        &source_ref,
        &revision_shared,
        "receiver's own divergent line\n",
    );
    let world_ref = world_ref_of(&project_b);
    let exported = export(&central_a, &project_a, &world_ref, &source_ref, Some(1));
    let bundle_file = write_bundle_file(temp_b.path(), &exported.data.unwrap());
    let applied = apply_bundle(&central_b, &project_b, &bundle_file, json!({}));
    assert_eq!(applied.data.unwrap()["status"], "conflicted");

    let result = run_action(
        "projectcentral.source.transfer.resolve",
        json!({
            "project": project_b.file_name().unwrap().to_string_lossy(),
            "source_ref": source_ref,
            "disposition": "keep-local",
            "actor": "resolver",
            "actor_kind": "human",
        }),
        &central_b,
    );
    assert_eq!(result.status, ResultStatus::Success, "{:?}", result.error);
    let receipt = result.data.unwrap();
    assert_eq!(receipt["disposition"], "keep-local");
    assert_eq!(receipt["retained_revision"], local_revision.as_str());
    assert_eq!(
        fs::read_to_string(project_b.join("ProjectCentral/agents/wiki/case6-note.md")).unwrap(),
        "receiver's own divergent line\n"
    );
}

#[test]
fn apply_refuses_direction_and_payload_violations_without_mutating() {
    let ((_temp_a, central_a, project_a), (temp_b, central_b, project_b), source_ref) =
        forked_grounds("refuse");
    let _ = write_via_action(
        &central_a,
        &project_a,
        &source_ref,
        &current_revision(&project_a, &source_ref),
        "changed on origin\n",
    );
    let before =
        fs::read_to_string(project_b.join("ProjectCentral/agents/wiki/case6-note.md")).unwrap();

    // Directed at another ground.
    let exported = export(
        &central_a,
        &project_a,
        "project:example/elsewhere",
        &source_ref,
        None,
    );
    let bundle = exported.data.unwrap();
    let bundle_file = write_bundle_file(temp_b.path(), &bundle);
    let refused = apply_bundle(&central_b, &project_b, &bundle_file, json!({}));
    assert_eq!(
        refused.status,
        ResultStatus::InvalidInput,
        "{:?}",
        refused.error
    );

    let world_ref = world_ref_of(&project_b);

    // Tampered payload: the content no longer hashes to its revision.
    let exported = export(&central_a, &project_a, &world_ref, &source_ref, Some(1));
    let mut bundle = exported.data.unwrap();
    bundle["sources"][0]["content"] = json!("tampered\n");
    let bundle_file = write_bundle_file(temp_b.path(), &bundle);
    let refused = apply_bundle(&central_b, &project_b, &bundle_file, json!({}));
    assert_eq!(
        refused.status,
        ResultStatus::VerificationFailure,
        "{:?}",
        refused.error
    );

    // A foreign entry ref does not name this ground.
    let exported = export(&central_a, &project_a, &world_ref, &source_ref, Some(1));
    let mut bundle = exported.data.unwrap();
    bundle["sources"][0]["source_ref"] =
        json!("central:source:project:example/other:ProjectCentral/x.md");
    let bundle_file = write_bundle_file(temp_b.path(), &bundle);
    let refused = apply_bundle(&central_b, &project_b, &bundle_file, json!({}));
    assert_eq!(
        refused.status,
        ResultStatus::InvalidInput,
        "{:?}",
        refused.error
    );

    // A path outside the participating trees has no home on this ground.
    let exported = export(&central_a, &project_a, &world_ref, &source_ref, Some(1));
    let mut bundle = exported.data.unwrap();
    bundle["sources"][0]["path"] = json!("docs/elbow-room.md");
    bundle["sources"][0]["source_ref"] =
        json!(format!("central:source:{world_ref}:docs%2Felbow-room.md"));
    let bundle_file = write_bundle_file(temp_b.path(), &bundle);
    let refused = apply_bundle(&central_b, &project_b, &bundle_file, json!({}));
    assert_eq!(
        refused.status,
        ResultStatus::InvalidInput,
        "{:?}",
        refused.error
    );

    assert_eq!(
        fs::read_to_string(project_b.join("ProjectCentral/agents/wiki/case6-note.md")).unwrap(),
        before
    );
}

#[test]
fn apply_refuses_declared_non_human_write_to_human_ground() {
    let ((_temp_a, central_a, project_a), (temp_b, central_b, project_b), _source_ref) =
        forked_grounds("authority");
    // A human-ground aperture source, changed on the origin ground.
    write_external(
        &project_a,
        "ProjectCentral/user/intent.md",
        "origin intent\n",
    );
    let horizon = read_project_change_horizon(&project_a, None).unwrap();
    let intent_ref = source_ref_of(&horizon, "intent.md");
    let world_ref = world_ref_of(&project_b);
    let exported = export(&central_a, &project_a, &world_ref, &intent_ref, None);
    assert_eq!(
        exported.status,
        ResultStatus::Success,
        "{:?}",
        exported.error
    );
    let bundle_file = write_bundle_file(temp_b.path(), &exported.data.unwrap());

    // B never held this source: absent local, unacknowledged lineage, and a
    // declared agent caller - the receiving ground's write authority refuses
    // before anything is created.
    let applied = apply_bundle(&central_b, &project_b, &bundle_file, json!({}));
    assert_eq!(
        applied.status,
        ResultStatus::UnavailableCapability,
        "{:?}",
        applied.error
    );
    assert!(applied.error.unwrap().message.contains("write authority"));
    assert!(!project_b.join("ProjectCentral/user/intent.md").exists());
}

#[test]
fn unestablished_lineage_conflicts_until_explicitly_acknowledged() {
    let ((_temp_a, central_a, project_a), (temp_b, central_b, project_b), source_ref) =
        forked_grounds("lineage");
    let _ = write_via_action(
        &central_a,
        &project_a,
        &source_ref,
        &current_revision(&project_a, &source_ref),
        "changed on origin\n",
    );

    // A source the origin created within the transferred range.
    write_external(
        &project_a,
        "ProjectCentral/agents/wiki/fresh-note.md",
        "born on origin\n",
    );
    let horizon = read_project_change_horizon(&project_a, None).unwrap();
    let fresh_ref = source_ref_of(&horizon, "fresh-note.md");

    let world_ref = world_ref_of(&project_b);
    let exported = export(&central_a, &project_a, &world_ref, &fresh_ref, None);
    assert_eq!(
        exported.status,
        ResultStatus::Success,
        "{:?}",
        exported.error
    );
    let bundle = exported.data.unwrap();
    assert_eq!(
        bundle["sources"][0]["kind"], "added",
        "the origin's creation is in the transferred range"
    );
    let bundle_file = write_bundle_file(temp_b.path(), &bundle);
    let applied = apply_bundle(&central_b, &project_b, &bundle_file, json!({}));
    assert_eq!(applied.status, ResultStatus::Success, "{:?}", applied.error);
    let receipt = applied.data.unwrap();
    assert_eq!(receipt["outcomes"][0]["outcome"], "applied-added");
    assert_eq!(
        fs::read_to_string(project_b.join("ProjectCentral/agents/wiki/fresh-note.md")).unwrap(),
        "born on origin\n"
    );

    // A source with no retained origin history exports as unestablished
    // state: applying it without acknowledgement conflicts, with the caller's
    // explicit lineage acknowledgement it is established.
    let (_temp_c, central_c, project_c) = ground("lineage-c", "case6");
    write_external(
        &project_c,
        "ProjectCentral/agents/wiki/prehistoric.md",
        "before any horizon\n",
    );
    let horizon_c = read_project_change_horizon(&project_c, None).unwrap();
    let prehistoric_ref = source_ref_of(&horizon_c, "prehistoric.md");
    let exported = export(&central_c, &project_c, &world_ref, &prehistoric_ref, None);
    let bundle = exported.data.unwrap();
    assert_eq!(bundle["sources"][0]["kind"], "state");
    assert!(bundle["sources"][0]["base_revision"].is_null());
    let bundle_file = write_bundle_file(temp_b.path(), &bundle);
    let refused = apply_bundle(&central_b, &project_b, &bundle_file, json!({}));
    assert_eq!(refused.data.unwrap()["status"], "conflicted");
    assert!(!project_b
        .join("ProjectCentral/agents/wiki/prehistoric.md")
        .exists());

    let bundle_file = write_bundle_file(temp_b.path(), &bundle);
    let established = apply_bundle(
        &central_b,
        &project_b,
        &bundle_file,
        json!({
            "accept_unestablished_lineage": [prehistoric_ref],
            "actor": "establishing-operator",
            "agent_session_ref": "session/establish",
        }),
    );
    assert_eq!(
        established.status,
        ResultStatus::Success,
        "{:?}",
        established.error
    );
    let receipt = established.data.unwrap();
    assert_eq!(receipt["outcomes"][0]["outcome"], "established");
    assert_eq!(
        fs::read_to_string(project_b.join("ProjectCentral/agents/wiki/prehistoric.md")).unwrap(),
        "before any horizon\n"
    );
}

#[test]
fn transfer_records_carry_no_absolute_paths() {
    let ((temp_a, central_a, project_a), (temp_b, central_b, project_b), source_ref) =
        forked_grounds("hygiene");
    let revision_shared = current_revision(&project_a, &source_ref);
    let _ = write_via_action(
        &central_a,
        &project_a,
        &source_ref,
        &revision_shared,
        "origin's divergent line\n",
    );
    let _ = write_via_action(
        &central_b,
        &project_b,
        &source_ref,
        &revision_shared,
        "receiver's own divergent line\n",
    );
    let world_ref = world_ref_of(&project_b);
    let exported = export(&central_a, &project_a, &world_ref, &source_ref, Some(1));
    let bundle = exported.data.unwrap();
    let raw_bundle = serde_json::to_string(&bundle).unwrap();
    assert!(!raw_bundle.contains(central_a.to_string_lossy().as_ref()));
    assert!(!raw_bundle.contains(temp_a.path().to_string_lossy().as_ref()));

    let bundle_file = write_bundle_file(temp_b.path(), &bundle);
    let applied = apply_bundle(&central_b, &project_b, &bundle_file, json!({}));
    assert_eq!(applied.data.unwrap()["status"], "conflicted");
    let transfer_area = project_b.join(".central/source-transfer");
    assert!(transfer_area.is_dir());
    let forbidden = [
        central_b.to_string_lossy().into_owned(),
        project_b.to_string_lossy().into_owned(),
        temp_b.path().to_string_lossy().into_owned(),
    ];
    let mut inspected = 0;
    for entry in walk(transfer_area) {
        let raw = fs::read_to_string(&entry).unwrap();
        for path in &forbidden {
            assert!(
                !raw.contains(path.as_str()),
                "{} leaks {}",
                entry.display(),
                path
            );
        }
        inspected += 1;
    }
    assert!(
        inspected >= 3,
        "receipt, conflict record and snapshots expected"
    );
}

fn walk(root: PathBuf) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![root];
    while let Some(current) = stack.pop() {
        for entry in fs::read_dir(&current).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                found.push(path);
            }
        }
    }
    found
}

fn temp_dir_string() -> String {
    std::env::temp_dir().to_string_lossy().into_owned()
}

#[test]
fn cli_seam_lists_the_transfer_actions() {
    let root = std::env::temp_dir().join(format!(
        "central-source-transfer-cli-{}",
        std::process::id()
    ));
    let execution = run_cli(
        &["--json".to_owned(), "action".to_owned(), "list".to_owned()],
        &CliEnvironment {
            configured_root: Some(root),
            home: None,
        },
    );
    assert_eq!(execution.result.status, ResultStatus::Success);
    let output = execution.result.data.unwrap().to_string();
    assert!(output.contains("projectcentral.source.transfer.export"));
    assert!(output.contains("projectcentral.source.transfer.apply"));
    assert!(output.contains("projectcentral.source.transfer.conflicts"));
    assert!(output.contains("projectcentral.source.transfer.resolve"));
}

// --- Root register: `project` absent is the Central root (control:root) ---

const ROOT_WORLD: &str = "control:root";
const OWNER_SUBJECT: &str = "central:pasu:nara:local";

fn root_ref(path: &str) -> String {
    format!("central:source:{ROOT_WORLD}:{path}")
}

/// A root ground shaped like a second machine's bootstrap: the Agent
/// governance, NOW and Wiki trees and a ground relations file, a Work root,
/// and no profiles, expressions, agent sets or identity manifest.
fn bootstrap_root(label: &str) -> (TempRoot, PathBuf) {
    let temp = TempRoot::new(label);
    let central = temp.path().join("Central");
    for dir in [
        "Control/agents/governance",
        "Control/agents/now",
        "Control/agents/wiki",
        "Control/relations",
        "Control/user",
        "Work",
    ] {
        fs::create_dir_all(central.join(dir)).unwrap();
    }
    fs::write(
        central.join("Control/relations/source-relations.json"),
        serde_json::to_vec_pretty(&json!({
            "schema": "central.control.ground-relations/v1",
            "project_id": ROOT_WORLD,
            "relations": [],
        }))
        .unwrap(),
    )
    .unwrap();
    (temp, central)
}

fn write_identity_manifest(central: &Path, subject: &str) {
    write_external(
        central,
        "Control/user/identity/manifest.json",
        &serde_json::to_string_pretty(&json!({
            "schema": "central.pasu.identity-manifest/v1",
            "revision": "1",
            "subject": {"ref": subject},
            "identity_source": {
                "path": "Control/user/identity",
                "provenance_law": "authored",
                "sources": [],
            },
        }))
        .unwrap(),
    );
}

/// The origin root ground: the owner's identity manifest plus the root agent
/// ground an agent host needs - one AgentProfile, one AgentSet and one agent
/// expression. Returns the three world-relative paths.
fn agent_ground_origin(label: &str) -> (TempRoot, PathBuf, [String; 3]) {
    let (temp, central) = bootstrap_root(label);
    write_identity_manifest(&central, OWNER_SUBJECT);
    let store = central_ctrl::AgentProfileStore::personal(&central);
    let profile = central_ctrl::AgentProfile::new(
        "profile/x-guardian",
        "r1",
        "agent/x-guardian",
        central_ctrl::AgentProfileScope::Personal,
        central_ctrl::WorldRef::new(ROOT_WORLD).unwrap(),
    )
    .unwrap();
    store.save(&profile, None).unwrap();
    let profile_path = relative_to(&central, &store.source_path("profile/x-guardian").unwrap());
    let sets = central_ctrl::agent_set_store::RelationRecordStore::agent_sets_at_root(&central);
    let set = central_ctrl::AgentSetRecord::new(
        central_ctrl::AgentSetRef::new("agent-set:guardians").unwrap(),
        "r1",
    );
    sets.save(&serde_json::to_value(&set).unwrap(), None)
        .unwrap();
    let set_path = relative_to(&central, &sets.source_path("agent-set:guardians").unwrap());
    let expression_path = "Control/agents/expressions/x-guardian/EXPRESSION.md".to_owned();
    write_external(&central, &expression_path, "# x-guardian\n\nGuards x.\n");
    (temp, central, [profile_path, set_path, expression_path])
}

fn relative_to(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap()
        .to_string_lossy()
        .into_owned()
}

fn root_revision(central: &Path, source_ref: &str) -> Option<String> {
    let scope = central_ctrl::continuous_work::source::Scope::resolve(central, None).unwrap();
    central_ctrl::source_horizon::read_control_change_horizon(&scope.root, None)
        .unwrap()
        .sources
        .iter()
        .find(|source| source.binding.source_ref == source_ref)
        .map(|source| source.revision.revision.clone())
}

fn root_export(
    central: &Path,
    refs: &[String],
    actor_kind: &str,
    since_cursor: Option<u64>,
) -> central_ctrl::ActionResult {
    let mut input = json!({
        "source_refs": refs,
        "to_world_ref": ROOT_WORLD,
        "from_ground": "origin-root",
        "to_ground": "agent-host-root",
        "actor": "root-operator",
        "actor_kind": actor_kind,
    });
    if actor_kind == "agent" {
        input["agent_session_ref"] = json!("session/root-origin");
    }
    if let Some(cursor) = since_cursor {
        input["since_cursor"] = json!(cursor);
    }
    run_action("projectcentral.source.transfer.export", input, central)
}

fn root_apply(
    central: &Path,
    bundle_file: &Path,
    actor_kind: &str,
    extra: Value,
) -> central_ctrl::ActionResult {
    let mut input = json!({
        "bundle_file": bundle_file.to_string_lossy(),
        "actor": "agent-host-operator",
        "actor_kind": actor_kind,
    });
    if actor_kind == "agent" {
        input["agent_session_ref"] = json!("session/agent-host");
    }
    for (key, value) in extra.as_object().unwrap() {
        input[key.as_str()] = value.clone();
    }
    run_action("projectcentral.source.transfer.apply", input, central)
}

fn root_write(central: &Path, source_ref: &str, content: &str) -> String {
    let result = run_action(
        "projectcentral.source.write",
        json!({
            "source_ref": source_ref,
            "expected_revision": root_revision(central, source_ref).unwrap(),
            "content": content,
            "actor": "root-writer",
            "actor_kind": "agent",
            "agent_session_ref": "session/root-writer",
        }),
        central,
    );
    assert_eq!(result.status, ResultStatus::Success, "{:?}", result.error);
    result.data.unwrap()["receipt"]["revision"]["revision"]
        .as_str()
        .unwrap()
        .to_owned()
}

/// Every regular file under a root, with its bytes: a refusal must leave the
/// receiving ground byte-identical, generated state included.
fn snapshot(root: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut files: Vec<(PathBuf, Vec<u8>)> = walk(root.to_path_buf())
        .into_iter()
        .map(|path| {
            let bytes = fs::read(&path).unwrap();
            (path, bytes)
        })
        .collect();
    files.sort();
    files
}

fn refusal_parts(result: &central_ctrl::ActionResult) -> (String, Value) {
    let error = result.error.as_ref().expect("refusal carries an error");
    let details = error.details.clone().expect("three-part details");
    assert!(details["fact"].is_string(), "{details}");
    assert!(details["consequence"].is_string(), "{details}");
    assert!(details["action"].is_string(), "{details}");
    (error.code.clone(), details)
}

#[test]
fn root_agent_ground_reaches_a_bootstrap_root_and_resolves_its_positions() {
    let (_temp_a, central_a, paths) = agent_ground_origin("root-agent-ground-a");
    let (temp_b, central_b) = bootstrap_root("root-agent-ground-b");
    // The receiving ground's Project names profile/x-guardian in a Position,
    // exactly the record a second machine refuses today.
    let project_b = central_b.join("Work/case7");
    fs::create_dir_all(&project_b).unwrap();
    central_ctrl::projectcentral_ops::initialize_projectcentral(
        &central_b,
        &project_b,
        "example/case7",
    )
    .unwrap();
    write_external(
        &project_b,
        "ProjectCentral/relations/positions/x-guardian.json",
        &serde_json::to_string_pretty(&json!({
            "schema": "central.world-position/v1",
            "ref": "central:position:project:example/case7:x-guardian",
            "revision": "r1",
            "slug": "x-guardian",
            "label": "x guardian",
            "enclosing_world_ref": "project:example/case7",
            "profile_ref": "profile/x-guardian",
        }))
        .unwrap(),
    );
    let before = run_action(
        "central.position.list",
        json!({"project": "case7"}),
        &central_b,
    );
    assert_eq!(before.status, ResultStatus::Success, "{:?}", before.error);
    let before = before.data.unwrap();
    assert_eq!(before["positions"].as_array().unwrap().len(), 0);
    assert!(before["invalid"][0]["error"]
        .as_str()
        .unwrap()
        .contains("does not resolve to an AgentProfile"));

    // The root agent ground participates in the origin's root horizon.
    let refs: Vec<String> = paths.iter().map(|path| root_ref(path)).collect();
    let exported = root_export(&central_a, &refs, "agent", None);
    assert_eq!(
        exported.status,
        ResultStatus::Success,
        "{:?}",
        exported.error
    );
    let bundle = exported.data.unwrap();
    assert_eq!(bundle["direction"]["from_world_ref"], ROOT_WORLD);
    assert_eq!(bundle["origin_identity"]["subject_ref"], OWNER_SUBJECT);
    assert_eq!(bundle["sources"].as_array().unwrap().len(), 3);
    let bundle_file = write_bundle_file(temp_b.path(), &bundle);

    // This receiver has no identity manifest: identity is unestablished and
    // nothing moves without the caller's explicit acknowledgement.
    let untouched = snapshot(&central_b);
    let refused = root_apply(&central_b, &bundle_file, "agent", json!({}));
    assert_eq!(refused.status, ResultStatus::UnavailableCapability);
    let (code, details) = refusal_parts(&refused);
    assert_eq!(code, "central.source_transfer.identity_unestablished");
    assert!(details["action"]
        .as_str()
        .unwrap()
        .contains("accept_unestablished_identity"));
    assert_eq!(snapshot(&central_b), untouched);

    // Identity acknowledged, lineage not: a bootstrap root holds no base for
    // any of these files, so each entry is a recorded conflict and nothing is
    // created - even the entries the origin's history calls `added`.
    let unacknowledged = root_apply(
        &central_b,
        &bundle_file,
        "agent",
        json!({"accept_unestablished_identity": true}),
    );
    assert_eq!(
        unacknowledged.status,
        ResultStatus::Success,
        "{:?}",
        unacknowledged.error
    );
    let receipt = unacknowledged.data.unwrap();
    assert_eq!(receipt["status"], "conflicted");
    assert_eq!(receipt["conflicted_count"], 3);
    for path in &paths {
        assert!(
            !central_b.join(path).exists(),
            "{path} created unacknowledged"
        );
    }

    // Both acknowledgements: every file is established with its incoming
    // revision, and the identity acceptance is on the receipt.
    let established = root_apply(
        &central_b,
        &bundle_file,
        "agent",
        json!({"accept_unestablished_identity": true, "accept_unestablished_lineage": refs}),
    );
    assert_eq!(
        established.status,
        ResultStatus::Success,
        "{:?}",
        established.error
    );
    let receipt = established.data.unwrap();
    assert_eq!(receipt["status"], "applied");
    assert_eq!(receipt["world_ref"], ROOT_WORLD);
    assert_eq!(
        receipt["identity"]["verification"],
        "accepted-unestablished"
    );
    assert_eq!(receipt["identity"]["origin_subject_ref"], OWNER_SUBJECT);
    assert!(receipt["identity"]["receiving_subject_ref"].is_null());
    for (index, path) in paths.iter().enumerate() {
        assert_eq!(receipt["outcomes"][index]["outcome"], "established");
        assert_eq!(
            fs::read(central_b.join(path)).unwrap(),
            fs::read(central_a.join(path)).unwrap()
        );
        assert_eq!(
            root_revision(&central_b, &root_ref(path)),
            root_revision(&central_a, &root_ref(path))
        );
    }
    assert!(central_b.join(".central/source-transfer/records").is_dir());
    // The acknowledgement settled the earlier unestablished conflicts.
    let conflicts = run_action(
        "projectcentral.source.transfer.conflicts",
        json!({}),
        &central_b,
    );
    assert_eq!(
        conflicts.status,
        ResultStatus::Success,
        "{:?}",
        conflicts.error
    );
    let conflicts = conflicts.data.unwrap();
    assert_eq!(conflicts["world_ref"], ROOT_WORLD);
    assert_eq!(conflicts["open_conflicts"], 0);
    assert_eq!(conflicts["conflicts"].as_array().unwrap().len(), 3);
    assert_eq!(
        conflicts["conflicts"][0]["resolution"]["disposition"],
        "established"
    );

    // The receiving Central now sees the records: the Position resolves its
    // profile, and the AgentSet reads back through its own Action.
    let after = run_action(
        "central.position.list",
        json!({"project": "case7"}),
        &central_b,
    );
    let after = after.data.unwrap();
    assert_eq!(after["invalid"].as_array().unwrap().len(), 0, "{after}");
    assert_eq!(
        after["positions"][0]["record"]["profile_ref"],
        "profile/x-guardian"
    );
    let set = run_action(
        "central.agent-set.read",
        json!({"scope": "root", "ref": "agent-set:guardians"}),
        &central_b,
    );
    assert_eq!(set.status, ResultStatus::Success, "{:?}", set.error);

    // The established revision is the base the next transfer fast-forwards
    // from, windowed at the previous bundle's origin cursor.
    let expression_ref = root_ref(&paths[2]);
    let edited = root_write(
        &central_a,
        &expression_ref,
        "# x-guardian\n\nGuards x, now y.\n",
    );
    let next = root_export(
        &central_a,
        std::slice::from_ref(&expression_ref),
        "agent",
        bundle["origin_cursor"].as_u64(),
    );
    assert_eq!(next.status, ResultStatus::Success, "{:?}", next.error);
    let next = next.data.unwrap();
    assert_eq!(next["sources"][0]["kind"], "modified");
    let next_file = write_bundle_file(temp_b.path(), &next);
    let forwarded = root_apply(
        &central_b,
        &next_file,
        "agent",
        json!({"accept_unestablished_identity": true}),
    );
    assert_eq!(
        forwarded.status,
        ResultStatus::Success,
        "{:?}",
        forwarded.error
    );
    let forwarded = forwarded.data.unwrap();
    assert_eq!(forwarded["outcomes"][0]["outcome"], "applied");
    assert_eq!(root_revision(&central_b, &expression_ref), Some(edited));
    let replay = root_apply(
        &central_b,
        &next_file,
        "agent",
        json!({"accept_unestablished_identity": true}),
    );
    assert_eq!(
        replay.data.unwrap()["outcomes"][0]["outcome"],
        "already-applied"
    );
}

#[test]
fn root_identity_is_verified_by_subject_and_a_different_owner_is_refused() {
    let (_temp_a, central_a, paths) = agent_ground_origin("root-identity-a");
    let refs = vec![root_ref(&paths[2])];
    let exported = root_export(&central_a, &refs, "agent", None);
    let bundle = exported.data.unwrap();

    let (temp_b, central_b) = bootstrap_root("root-identity-b");
    write_identity_manifest(&central_b, OWNER_SUBJECT);
    let bundle_file = write_bundle_file(temp_b.path(), &bundle);
    let applied = root_apply(
        &central_b,
        &bundle_file,
        "agent",
        json!({"accept_unestablished_lineage": refs}),
    );
    assert_eq!(applied.status, ResultStatus::Success, "{:?}", applied.error);
    let receipt = applied.data.unwrap();
    assert_eq!(receipt["identity"]["verification"], "verified");
    assert_eq!(receipt["identity"]["receiving_subject_ref"], OWNER_SUBJECT);

    // Another owner's root is another world, whatever its world ref says:
    // refused even with every acknowledgement, and left byte-identical.
    let (temp_c, central_c) = bootstrap_root("root-identity-c");
    write_identity_manifest(&central_c, "central:pasu:nara:someone-else");
    let bundle_file = write_bundle_file(temp_c.path(), &bundle);
    let untouched = snapshot(&central_c);
    let refused = root_apply(
        &central_c,
        &bundle_file,
        "agent",
        json!({"accept_unestablished_identity": true, "accept_unestablished_lineage": refs}),
    );
    assert_eq!(refused.status, ResultStatus::UnavailableCapability);
    assert_eq!(
        refusal_parts(&refused).0,
        "central.source_transfer.identity_mismatch"
    );
    assert_eq!(snapshot(&central_c), untouched);
}

#[test]
fn root_divergence_is_a_recorded_conflict_resolved_by_keep_local_or_accept_incoming() {
    let (_temp_a, central_a) = bootstrap_root("root-diverge-a");
    let (temp_b, central_b) = bootstrap_root("root-diverge-b");
    write_identity_manifest(&central_a, OWNER_SUBJECT);
    write_identity_manifest(&central_b, OWNER_SUBJECT);
    // Two expressions both grounds already hold byte-identically.
    let paths = [
        "Control/agents/expressions/one/EXPRESSION.md",
        "Control/agents/expressions/two/EXPRESSION.md",
    ];
    let refs: Vec<String> = paths.iter().map(|path| root_ref(path)).collect();
    let _ = root_revision(&central_a, "none");
    for path in paths {
        write_external(&central_a, path, "shared fork\n");
        write_external(&central_b, path, "shared fork\n");
    }
    let _ = root_revision(&central_b, "none");
    let fork = root_export(&central_a, &refs, "agent", None).data.unwrap();
    let fork_cursor = fork["origin_cursor"].as_u64();
    let fork_file = write_bundle_file(temp_b.path(), &fork);
    let present = root_apply(&central_b, &fork_file, "agent", json!({}));
    assert_eq!(present.status, ResultStatus::Success, "{:?}", present.error);
    let present = present.data.unwrap();
    assert_eq!(present["status"], "applied");
    assert_eq!(present["outcomes"][0]["outcome"], "already-applied");
    assert_eq!(present["outcomes"][1]["outcome"], "already-applied");

    // Both grounds move each expression in different directions.
    for source_ref in &refs {
        root_write(&central_a, source_ref, "origin's divergent line\n");
        root_write(&central_b, source_ref, "receiver's own divergent line\n");
    }
    let local: Vec<String> = refs
        .iter()
        .map(|source_ref| root_revision(&central_b, source_ref).unwrap())
        .collect();
    let diverged = root_export(&central_a, &refs, "agent", fork_cursor)
        .data
        .unwrap();
    let diverged_file = write_bundle_file(temp_b.path(), &diverged);
    let applied = root_apply(&central_b, &diverged_file, "agent", json!({}));
    assert_eq!(applied.status, ResultStatus::Success, "{:?}", applied.error);
    let receipt = applied.data.unwrap();
    assert_eq!(receipt["status"], "conflicted");
    assert_eq!(receipt["conflicted_count"], 2);
    for path in paths {
        assert_eq!(
            fs::read_to_string(central_b.join(path)).unwrap(),
            "receiver's own divergent line\n"
        );
    }
    let listed = run_action(
        "projectcentral.source.transfer.conflicts",
        json!({"status": "open"}),
        &central_b,
    )
    .data
    .unwrap();
    assert_eq!(listed["open_conflicts"], 2);
    let conflict_ref = listed["conflicts"][0]["conflict_ref"].as_str().unwrap();
    assert!(conflict_ref.starts_with("central:transfer-conflict:control:root:"));
    let stem = conflict_ref.rsplit(':').next().unwrap();
    assert!(central_b
        .join(format!(
            ".central/source-transfer/conflicts/{stem}/local-source.txt"
        ))
        .is_file());

    let keep = run_action(
        "projectcentral.source.transfer.resolve",
        json!({
            "source_ref": refs[0],
            "disposition": "keep-local",
            "actor": "resolver",
            "actor_kind": "agent",
            "agent_session_ref": "session/resolver",
        }),
        &central_b,
    );
    assert_eq!(keep.status, ResultStatus::Success, "{:?}", keep.error);
    assert_eq!(keep.data.unwrap()["retained_revision"], local[0].as_str());
    assert_eq!(
        fs::read_to_string(central_b.join(paths[0])).unwrap(),
        "receiver's own divergent line\n"
    );

    let accept = run_action(
        "projectcentral.source.transfer.resolve",
        json!({
            "source_ref": refs[1],
            "disposition": "accept-incoming",
            "expected_local_revision": local[1],
            "actor": "resolver",
            "actor_kind": "agent",
            "agent_session_ref": "session/resolver",
        }),
        &central_b,
    );
    assert_eq!(accept.status, ResultStatus::Success, "{:?}", accept.error);
    assert_eq!(
        fs::read_to_string(central_b.join(paths[1])).unwrap(),
        "origin's divergent line\n"
    );
    let open = run_action(
        "projectcentral.source.transfer.conflicts",
        json!({"status": "open"}),
        &central_b,
    )
    .data
    .unwrap();
    assert_eq!(open["open_conflicts"], 0);
}

#[test]
fn root_personal_ground_moves_only_for_a_declared_human_and_machine_ground_never() {
    let (_temp_a, central_a) = bootstrap_root("root-personal-a");
    let (temp_b, central_b) = bootstrap_root("root-personal-b");
    write_identity_manifest(&central_a, OWNER_SUBJECT);
    write_identity_manifest(&central_b, OWNER_SUBJECT);
    let personal = "Control/user/identity/present.md";
    write_external(&central_a, personal, "present, as the owner wrote it\n");
    let personal_ref = root_ref(personal);

    let refused = root_export(
        &central_a,
        std::slice::from_ref(&personal_ref),
        "agent",
        None,
    );
    assert_eq!(refused.status, ResultStatus::UnavailableCapability);
    let (code, details) = refusal_parts(&refused);
    assert_eq!(code, "central.source_transfer.personal_ground");
    assert!(details["fact"].as_str().unwrap().contains("Control/user/"));
    assert!(details["consequence"]
        .as_str()
        .unwrap()
        .contains("Nothing was exported"));
    assert!(details["action"]
        .as_str()
        .unwrap()
        .contains("Control/agents/profiles"));

    // The owner carries personal ground as a declared human.
    let exported = root_export(
        &central_a,
        std::slice::from_ref(&personal_ref),
        "human",
        None,
    );
    assert_eq!(
        exported.status,
        ResultStatus::Success,
        "{:?}",
        exported.error
    );
    let bundle_file = write_bundle_file(temp_b.path(), &exported.data.unwrap());

    // An agent cannot apply it, however it was exported.
    let untouched = snapshot(&central_b);
    let refused = root_apply(
        &central_b,
        &bundle_file,
        "agent",
        json!({"accept_unestablished_lineage": [personal_ref]}),
    );
    assert_eq!(refused.status, ResultStatus::UnavailableCapability);
    assert_eq!(
        refusal_parts(&refused).0,
        "central.source_transfer.personal_ground"
    );
    assert_eq!(snapshot(&central_b), untouched);
    let human = root_apply(
        &central_b,
        &bundle_file,
        "human",
        json!({"accept_unestablished_lineage": [personal_ref]}),
    );
    assert_eq!(human.status, ResultStatus::Success, "{:?}", human.error);
    assert_eq!(human.data.unwrap()["outcomes"][0]["outcome"], "established");
    assert_eq!(
        fs::read_to_string(central_b.join(personal)).unwrap(),
        "present, as the owner wrote it\n"
    );

    // An agent cannot resolve personal ground either.
    let resolve = run_action(
        "projectcentral.source.transfer.resolve",
        json!({
            "source_ref": personal_ref,
            "disposition": "keep-local",
            "actor": "resolver",
            "actor_kind": "agent",
            "agent_session_ref": "session/resolver",
        }),
        &central_b,
    );
    assert_eq!(
        refusal_parts(&resolve).0,
        "central.source_transfer.personal_ground"
    );

    // Machine ground names one machine: nobody transfers it.
    let machine = root_export(
        &central_a,
        &[root_ref("Control/machines/alpha/machine.json")],
        "human",
        None,
    );
    assert_eq!(machine.status, ResultStatus::UnavailableCapability);
    assert_eq!(
        refusal_parts(&machine).0,
        "central.source_transfer.machine_ground"
    );
}

#[test]
fn root_bundle_carries_no_paths_credentials_machine_identities_or_generated_state() {
    let (temp_a, central_a, paths) = agent_ground_origin("root-hygiene-a");
    let (temp_b, central_b) = bootstrap_root("root-hygiene-b");
    let token_digest = "4d2c1c0ffee0b5e55e1f00dcafe5ecre7d19e5700000000000000000000000000";
    let authority = serde_json::to_string_pretty(&json!({
        "schema": "central.native-action-authority/v1",
        "scope_ref": ROOT_WORLD,
        "grants": [{"token_sha256": token_digest}],
    }))
    .unwrap();
    write_external(
        &central_a,
        "Control/user/native-action-authority.json",
        &authority,
    );
    write_external(
        &central_a,
        "Control/user/civil-time-policy.json",
        "{\"schema\": \"central.civil-time-policy/v1\"}\n",
    );
    fs::write(
        central_a.join("Control/relations/source-relations.json"),
        serde_json::to_vec_pretty(&json!({
            "schema": "central.control.ground-relations/v1",
            "project_id": ROOT_WORLD,
            "relations": [{
                "ref": root_ref("Control/user/civil-time-policy.json"),
                "path": "Control/user/civil-time-policy.json",
                "provenance": "human-adopted",
                "standing": "architecture-contract",
                "roles": ["civil-time-policy"],
                "treatment": "projectcentral-user",
            }],
        }))
        .unwrap(),
    )
    .unwrap();
    write_external(
        &central_a,
        "Control/machines/alpha/machine.json",
        "{\"workcell_ref\": \"workcell:alpha-machine\"}\n",
    );

    // Credential-bearing and natively owned sources refuse even the owner.
    for path in [
        "Control/user/native-action-authority.json",
        "Control/user/civil-time-policy.json",
    ] {
        let refused = root_export(&central_a, &[root_ref(path)], "human", None);
        assert_eq!(
            refused.status,
            ResultStatus::UnavailableCapability,
            "{path}"
        );
        assert_eq!(
            refusal_parts(&refused).0,
            "central.source_transfer.natively_owned",
            "{path}"
        );
    }

    let refs: Vec<String> = paths.iter().map(|path| root_ref(path)).collect();
    let bundle = root_export(&central_a, &refs, "agent", None).data.unwrap();
    let raw = serde_json::to_string(&bundle).unwrap();
    let canonical_a = fs::canonicalize(temp_a.path()).unwrap();
    let forbidden = [
        temp_a.path().to_string_lossy().into_owned(),
        canonical_a.to_string_lossy().into_owned(),
        temp_dir_string(),
        token_digest.to_owned(),
        "native-action-authority".to_owned(),
        "workcell:alpha-machine".to_owned(),
        "Control/machines".to_owned(),
        ".central".to_owned(),
        "source-change-control".to_owned(),
    ];
    for needle in &forbidden {
        assert!(!raw.contains(needle.as_str()), "bundle leaks {needle}");
    }

    let bundle_file = write_bundle_file(temp_b.path(), &bundle);
    let applied = root_apply(
        &central_b,
        &bundle_file,
        "agent",
        json!({"accept_unestablished_identity": true, "accept_unestablished_lineage": refs}),
    );
    assert_eq!(applied.status, ResultStatus::Success, "{:?}", applied.error);
    let canonical_b = fs::canonicalize(temp_b.path()).unwrap();
    for entry in walk(central_b.join(".central/source-transfer")) {
        let raw = fs::read_to_string(&entry).unwrap();
        for needle in [
            temp_b.path().to_string_lossy().into_owned(),
            canonical_b.to_string_lossy().into_owned(),
            canonical_a.to_string_lossy().into_owned(),
        ] {
            assert!(
                !raw.contains(needle.as_str()),
                "{} leaks {needle}",
                entry.display()
            );
        }
    }

    // A crafted bundle carrying authority grants is refused at apply too.
    let mut crafted = bundle.clone();
    crafted["sources"][0]["content"] = json!(authority);
    crafted["sources"][0]["after_revision"] = json!(
        central_ctrl::content_revision(
            &central_a.join("Control/user/native-action-authority.json")
        )
        .unwrap()
        .revision
    );
    let crafted_file = write_bundle_file(temp_b.path(), &crafted);
    let untouched = snapshot(&central_b);
    let refused = root_apply(
        &central_b,
        &crafted_file,
        "agent",
        json!({"accept_unestablished_identity": true}),
    );
    assert_eq!(
        refusal_parts(&refused).0,
        "central.source_transfer.natively_owned"
    );
    assert_eq!(snapshot(&central_b), untouched);
}
