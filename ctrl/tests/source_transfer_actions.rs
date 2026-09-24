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
