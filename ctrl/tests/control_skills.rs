use central_ctrl::{
    control_source_bindings, create_core_action_registry, create_default_connector_registry,
    initialize_projectcentral, project_source_bindings, reconcile_control_sources, run_cli,
    ActionExecutionContext, CliEnvironment, ConnectorContext, ResultStatus, RootOptions,
    CONTROL_GROUND_RELATIONS_SCHEMA, CONTROL_GROUND_RELATIONS_SOURCE, CONTROL_SKILL_TREATMENT,
    SKILL_BODY, SKILL_MANIFEST, SKILL_MANIFEST_SCHEMA,
};
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static NEXT_TEMP_ROOT: AtomicU64 = AtomicU64::new(0);

struct TempRoot(PathBuf);

impl TempRoot {
    fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let sequence = NEXT_TEMP_ROOT.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "central-skills-integration-{}-{nonce}-{sequence}",
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

fn execute(root: &Path, action: &str, input: Value) -> central_ctrl::ActionResult {
    let registry = create_core_action_registry();
    let connectors = create_default_connector_registry();
    let connector_context = ConnectorContext::current();
    let root_options = RootOptions {
        explicit_root: Some(root.to_path_buf()),
        ..RootOptions::default()
    };
    let context = ActionExecutionContext {
        root_options: &root_options,
        connectors: &connectors,
        connector_context: &connector_context,
    };
    registry.execute(action, &input, &context)
}

fn seed_skill(root: &Path, skill_dir: &str, scope: &str, name: &str) -> PathBuf {
    let dir = root.join(skill_dir);
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join(SKILL_BODY),
        format!("---\nname: {name}\n---\nBody of {name}.\n"),
    )
    .unwrap();
    fs::write(
        dir.join(SKILL_MANIFEST),
        serde_json::to_vec_pretty(&json!({
            "schema": SKILL_MANIFEST_SCHEMA,
            "name": name,
            "scope": scope,
            "standing": "active",
            "provenance": "human-authored"
        }))
        .unwrap(),
    )
    .unwrap();
    dir
}

#[test]
fn inspect_retire_inspect_restore_walk_through_the_owner_actions() {
    let temp = TempRoot::new();
    let central = temp.path().join("Central");
    fs::create_dir_all(central.join("Control/user")).unwrap();
    fs::create_dir_all(central.join("Control/agents/governance")).unwrap();
    fs::create_dir_all(central.join("Control/agents/wiki")).unwrap();
    seed_skill(
        &central,
        "Control/user/skills/central-ground-keeping",
        "control-user",
        "central-ground-keeping",
    );

    // inspect discloses the seed skill with correct scope and standing.
    let inspect = execute(&central, "control.skills.inspect", json!({}));
    assert_eq!(inspect.status, ResultStatus::Success);
    let data = inspect.data.unwrap();
    let skill = &data["skills"][0];
    assert_eq!(skill["name"], "central-ground-keeping");
    assert_eq!(skill["scope"], "control-user");
    assert_eq!(skill["standing"], "active");
    assert_eq!(skill["provenance"], "human-authored");
    assert_eq!(
        skill["body_source_ref"],
        "central:source:control:root:Control/user/skills/central-ground-keeping/SKILL.md"
    );
    assert_eq!(data["active_skills"], 1);
    assert_eq!(data["retired_skills"], 0);
    assert_eq!(
        data["projection_policy"]["retired_standing_projects"],
        false
    );

    // retire writes the standing change with reason and provenance.
    let retire = execute(
        &central,
        "control.skills.retire",
        json!({
            "scope": "control-user",
            "name": "central-ground-keeping",
            "retired_by": "owner-in-session",
            "retirement_reason": "walk rehearsal: superseded by the ontology skill"
        }),
    );
    assert_eq!(retire.status, ResultStatus::Success);
    let receipt = retire.data.unwrap();
    assert_eq!(receipt["previous_standing"], "active");
    assert_eq!(receipt["standing"], "retired");
    assert_eq!(
        receipt["skill"]["retirement"]["retired_by"],
        "owner-in-session"
    );
    assert_eq!(
        receipt["skill"]["retirement"]["retirement_reason"],
        "walk rehearsal: superseded by the ontology skill"
    );
    assert!(
        receipt["skill"]["retirement"]["retired_at_unix_seconds"]
            .as_u64()
            .unwrap()
            > 0
    );
    assert_eq!(receipt["skill_bytes_mutated"], false);
    assert_eq!(receipt["skill_directory_mutated"], false);

    // the manifest on disk carries the record.
    let disk: Value = serde_json::from_slice(
        &fs::read(central.join("Control/user/skills/central-ground-keeping/skill.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(disk["standing"], "retired");
    assert_eq!(disk["retirement"]["retired_by"], "owner-in-session");

    // inspect discloses retired with reason and provenance, never hidden.
    let retired_inspect = execute(&central, "control.skills.inspect", json!({}));
    assert_eq!(retired_inspect.status, ResultStatus::Success);
    let data = retired_inspect.data.unwrap();
    let skill = &data["skills"][0];
    assert_eq!(skill["standing"], "retired");
    assert_eq!(data["retired_skills"], 1);
    assert_eq!(data["active_skills"], 0);
    assert_eq!(
        skill["retirement"]["retirement_reason"],
        "walk rehearsal: superseded by the ontology skill"
    );

    // retiring again refuses.
    let double = execute(
        &central,
        "control.skills.retire",
        json!({
            "scope": "control-user",
            "name": "central-ground-keeping",
            "retired_by": "owner-in-session",
            "retirement_reason": "again"
        }),
    );
    assert_eq!(double.status, ResultStatus::InvalidInput);
    assert!(double.error.unwrap().message.contains("already retired"));

    // a nonexistent skill refuses honestly.
    let missing = execute(
        &central,
        "control.skills.retire",
        json!({
            "scope": "control-user",
            "name": "never-authored",
            "retired_by": "owner-in-session",
            "retirement_reason": "missing"
        }),
    );
    assert_eq!(missing.status, ResultStatus::InvalidInput);
    assert!(missing
        .error
        .unwrap()
        .message
        .contains("skill ground does not exist"));

    // restore reverses to active.
    let restore = execute(
        &central,
        "control.skills.restore",
        json!({ "scope": "control-user", "name": "central-ground-keeping" }),
    );
    assert_eq!(restore.status, ResultStatus::Success);
    let receipt = restore.data.unwrap();
    assert_eq!(receipt["standing"], "active");
    assert_eq!(
        receipt["removed_retirement"]["retirement_reason"],
        "walk rehearsal: superseded by the ontology skill"
    );
    assert_eq!(receipt["skill"]["standing"], "active");
    let final_inspect = execute(&central, "control.skills.inspect", json!({}));
    assert_eq!(final_inspect.status, ResultStatus::Success);
    assert_eq!(final_inspect.data.unwrap()["active_skills"], 1);
}

#[test]
fn cli_action_run_supports_the_skills_surface() {
    let temp = TempRoot::new();
    let central = temp.path().join("Central");
    fs::create_dir_all(central.join("Control/user")).unwrap();
    seed_skill(
        &central,
        "Control/user/skills/central-ground-keeping",
        "control-user",
        "central-ground-keeping",
    );

    let environment = CliEnvironment {
        configured_root: None,
        home: None,
    };
    let inspect = run_cli(
        &[
            "--json".to_owned(),
            "--root".to_owned(),
            central.display().to_string(),
            "action".to_owned(),
            "run".to_owned(),
            "control.skills.inspect".to_owned(),
        ],
        &environment,
    );
    assert_eq!(inspect.exit_code, 0);
    let payload: Value = serde_json::from_str(&inspect.output).unwrap();
    assert_eq!(payload["action"], "control.skills.inspect");
    assert_eq!(payload["data"]["skills"][0]["scope"], "control-user");
}

#[test]
fn skills_participate_in_ground_relations_with_control_skill_treatment_and_manifest_standing() {
    let temp = TempRoot::new();
    let central = temp.path();
    fs::create_dir_all(central.join("Control/user")).unwrap();
    fs::create_dir_all(central.join("Control/agents/governance")).unwrap();
    fs::create_dir_all(central.join("Control/agents/wiki")).unwrap();
    fs::create_dir_all(central.join("Control/machines/primary-workstation")).unwrap();
    let personal = seed_skill(
        &central,
        "Control/user/skills/central-ground-keeping",
        "control-user",
        "central-ground-keeping",
    );
    let machine = seed_skill(
        &central,
        "Control/machines/primary-workstation/skills/brandkit",
        "control-machine",
        "brandkit",
    );
    // A skill directory without a manifest participates unresolved, never inferred.
    fs::create_dir_all(central.join("Control/user/skills/anonymous")).unwrap();
    fs::write(
        central.join("Control/user/skills/anonymous/SKILL.md"),
        "no manifest\n",
    )
    .unwrap();

    let bindings = control_source_bindings(central).unwrap();
    let body = bindings
        .iter()
        .find(|binding| binding.path == "Control/user/skills/central-ground-keeping/SKILL.md")
        .expect("personal skill body participates");
    assert_eq!(body.treatment, CONTROL_SKILL_TREATMENT);
    assert_eq!(body.provenance, "human-authored");
    assert_eq!(body.standing, "active");
    assert_eq!(
        body.source_ref,
        "central:source:control:root:Control/user/skills/central-ground-keeping/SKILL.md"
    );
    assert!(body.roles.iter().any(|role| role == "skill-source"));
    // one physical source, one logical binding: the Control/user aperture
    // fallback did not duplicate the skill file.
    assert_eq!(
        bindings
            .iter()
            .filter(|binding| binding.path == "Control/user/skills/central-ground-keeping/SKILL.md")
            .count(),
        1
    );

    let machine_body = bindings
        .iter()
        .find(|binding| {
            binding.path == "Control/machines/primary-workstation/skills/brandkit/SKILL.md"
        })
        .expect("machine skill body participates in Control ground");
    assert_eq!(machine_body.treatment, CONTROL_SKILL_TREATMENT);

    let anonymous = bindings
        .iter()
        .find(|binding| binding.path == "Control/user/skills/anonymous/SKILL.md")
        .expect("manifest-less skill still participates");
    assert_eq!(anonymous.provenance, "unresolved");
    assert_eq!(anonymous.standing, "unspecified");

    // retirement changes the standing the horizon carries, and the change is
    // observed as a logical modification of the same source ref.
    let baseline = reconcile_control_sources(central).unwrap();
    assert!(baseline.initialized);
    let manifest: Value =
        serde_json::from_slice(&fs::read(personal.join(SKILL_MANIFEST)).unwrap()).unwrap();
    let mut retired = manifest.clone();
    retired["standing"] = json!("retired");
    retired["retirement"] = json!({
        "retired_by": "owner-in-session",
        "retired_at_unix_seconds": 1757097600u64,
        "retirement_reason": "rehearsal"
    });
    fs::write(
        personal.join(SKILL_MANIFEST),
        serde_json::to_vec_pretty(&retired).unwrap(),
    )
    .unwrap();
    let changed = reconcile_control_sources(central).unwrap();
    assert_eq!(changed.new_changes.len(), 1);
    assert_eq!(
        changed.new_changes[0].source_path,
        "Control/user/skills/central-ground-keeping/skill.json"
    );
    assert_eq!(changed.new_changes[0].standing, "retired");
    let bindings = control_source_bindings(central).unwrap();
    let body = bindings
        .iter()
        .find(|binding| binding.path == "Control/user/skills/central-ground-keeping/SKILL.md")
        .unwrap();
    assert_eq!(body.standing, "retired");

    // an explicit accepted control ground relation remains the authority for a
    // skill path, exactly as for any other source.
    let relations = central.join(CONTROL_GROUND_RELATIONS_SOURCE);
    fs::create_dir_all(relations.parent().unwrap()).unwrap();
    fs::write(
        &relations,
        serde_json::to_vec_pretty(&json!({
            "schema": CONTROL_GROUND_RELATIONS_SCHEMA,
            "project_id": "control:root",
            "relations": [{
                "ref": "central:source:control:root:Control/user/skills/central-ground-keeping/SKILL.md",
                "path": "Control/user/skills/central-ground-keeping/SKILL.md",
                "provenance": "human-authored",
                "standing": "durable-source",
                "roles": ["skill-source"],
                "treatment": "control-skill"
            }]
        }))
        .unwrap(),
    )
    .unwrap();
    let bindings = control_source_bindings(central).unwrap();
    let body = bindings
        .iter()
        .find(|binding| binding.path == "Control/user/skills/central-ground-keeping/SKILL.md")
        .unwrap();
    assert_eq!(body.standing, "durable-source");
    assert_eq!(
        bindings
            .iter()
            .filter(|binding| binding.path == "Control/user/skills/central-ground-keeping/SKILL.md")
            .count(),
        1
    );
    // the machine skill body still participates untouched by the relation.
    assert!(machine.join(SKILL_BODY).is_file());
}

#[test]
fn projectcentral_user_skills_participate_in_the_project_horizon() {
    let temp = TempRoot::new();
    let central = temp.path().join("Central");
    let project = central.join("Work/suite");
    fs::create_dir_all(&project).unwrap();
    initialize_projectcentral(&central, &project, "example/suite").unwrap();
    seed_skill(
        &project,
        "ProjectCentral/user/skills/suite-operator",
        "projectcentral-user",
        "suite-operator",
    );

    let bindings = project_source_bindings(&project).unwrap();
    let body = bindings
        .iter()
        .find(|binding| binding.path == "ProjectCentral/user/skills/suite-operator/SKILL.md")
        .expect("project skill participates in its project ground");
    assert_eq!(body.treatment, CONTROL_SKILL_TREATMENT);
    assert_eq!(body.provenance, "human-authored");
    assert_eq!(
        body.source_ref,
        "central:source:project:example/suite:ProjectCentral/user/skills/suite-operator/SKILL.md"
    );
    assert_eq!(
        bindings
            .iter()
            .filter(|binding| binding.path == "ProjectCentral/user/skills/suite-operator/SKILL.md")
            .count(),
        1
    );

    // inspection discloses the project scope alongside the Control scopes.
    let inspect = execute(&central, "control.skills.inspect", json!({}));
    assert_eq!(inspect.status, ResultStatus::Success);
    let data = inspect.data.unwrap();
    let skill = data["skills"]
        .as_array()
        .unwrap()
        .iter()
        .find(|skill| skill["name"] == "suite-operator")
        .expect("project skill disclosed");
    assert_eq!(skill["scope"], "projectcentral-user");
    assert_eq!(skill["project"], "suite");
    assert_eq!(skill["standing"], "active");
}
