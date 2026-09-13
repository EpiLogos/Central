use central_ctrl::{
    create_core_action_registry, create_default_connector_registry, initialize_projectcentral,
    projectcentral_ops::register_projectcentral_actions, ActionExecutionContext, ActionResult,
    ConnectorContext, RootOptions, NOW_AGENT_DIR, NOW_DAY_DIR, NOW_DIR, NOW_USER_DIR,
    WIKI_RETURN_DIR,
};
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
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
            "central-now-{}-{nonce}-{sequence}",
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

fn execute(
    registry: &central_ctrl::ActionRegistry,
    context: &ActionExecutionContext<'_>,
    action: &str,
    input: Value,
) -> Value {
    let result = registry.execute(action, &input, context);
    assert!(result.ok, "{action} failed: {result:?}");
    result.data.expect("successful Action has data")
}

#[test]
fn project_without_now_remains_valid_and_now_inspection_is_non_mutating() {
    let temp = TempRoot::new();
    let central = temp.path().join("Central");
    let project = central.join("Work/no-now");
    fs::create_dir_all(&project).unwrap();
    initialize_projectcentral(&central, &project, "example/no-now").unwrap();

    let root_options = RootOptions {
        explicit_root: Some(central.clone()),
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

    let data = execute(
        &registry,
        &context,
        "projectcentral.now.inspect",
        json!({"project":"no-now"}),
    );
    assert_eq!(data["exists"], false);
    assert!(!project.join(NOW_DIR).exists());
    assert!(project.join("ProjectCentral/project.json").is_file());
    assert!(project
        .join("ProjectCentral/agents/wiki/wiki.json")
        .is_file());
}

#[test]
fn real_work_project_flow_survives_sessions_rolls_day_and_returns_meaning() {
    let temp = TempRoot::new();
    let central = temp.path().join("Central");
    let project = central.join("Work/lived-project");
    fs::create_dir_all(&project).unwrap();
    fs::write(project.join("README.md"), "# Lived project\n").unwrap();
    initialize_projectcentral(&central, &project, "example/lived-project").unwrap();

    let root_options = RootOptions {
        explicit_root: Some(central.clone()),
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

    execute(
        &registry,
        &context,
        "projectcentral.now.init",
        json!({"project":"lived-project"}),
    );

    // Human writes directly: no schema or Action ceremony is required for their scratch.
    let scratch = project.join(NOW_USER_DIR).join("current.md");
    fs::write(
        &scratch,
        "The handoff should stay visible after I close this chat.\n",
    )
    .unwrap();

    // A later Agent/session can recover that current human state from NOW alone.
    let after_human = execute(
        &registry,
        &context,
        "projectcentral.now.inspect",
        json!({"project":"lived-project"}),
    );
    assert!(after_human["human_scratch"]
        .as_array()
        .unwrap()
        .iter()
        .any(|value| value == "ProjectCentral/now/user/current.md"));

    let question = execute(
        &registry,
        &context,
        "projectcentral.now.return",
        json!({
            "project":"lived-project",
            "actor":"agent:nara",
            "kind":"question",
            "subject":"Open design question",
            "result":"Should the returned surface privilege prose or source refs?",
            "status":"waiting",
            "run_ref":"factory:run:74",
            "session_ref":"aikit:session:later"
        }),
    );
    let question_id = question["handoff"]["id"].as_str().unwrap().to_owned();

    let resolved = execute(
        &registry,
        &context,
        "projectcentral.now.return",
        json!({
            "project":"lived-project",
            "actor":"agent:builder",
            "kind":"handoff",
            "subject":"Resolved transient check",
            "result":"The bounded check is complete and has no durable caller.",
            "status":"resolved"
        }),
    );
    let resolved_id = resolved["handoff"]["id"].as_str().unwrap().to_owned();

    let learning = execute(
        &registry,
        &context,
        "projectcentral.now.return",
        json!({
            "project":"lived-project",
            "actor":"agent:builder",
            "kind":"learning",
            "subject":"Meaningful returned learning",
            "result":"Keep day closure derived; do not make it authored Project canon.",
            "status":"active",
            "evidence_refs":["central:test:now-flow"]
        }),
    );
    let learning_id = learning["handoff"]["id"].as_str().unwrap().to_owned();
    let learning_source = learning["source"].as_str().unwrap().to_owned();

    // Agent learning returns to the existing Wiki owner path without silently editing wiki.json.
    let agent_promotion = execute(
        &registry,
        &context,
        "projectcentral.now.promote",
        json!({
            "project":"lived-project",
            "source":learning_source,
            "target":"agent-wiki",
            "destination":"now-day/returned-learning.json",
            "acceptance":"agent-return"
        }),
    );
    assert!(agent_promotion["destination"]
        .as_str()
        .unwrap()
        .starts_with(WIKI_RETURN_DIR));
    assert!(project
        .join(WIKI_RETURN_DIR)
        .join("now-day/returned-learning.json")
        .is_file());

    // Human scratch only becomes durable Project ground through explicit human acceptance.
    let human_promotion = execute(
        &registry,
        &context,
        "projectcentral.now.promote",
        json!({
            "project":"lived-project",
            "source":"ProjectCentral/now/user/current.md",
            "target":"human-ground",
            "destination":"returned/current.md",
            "acceptance":"human-accepted"
        }),
    );
    assert_eq!(human_promotion["target"], "human-ground");
    assert_eq!(
        fs::read_to_string(project.join("ProjectCentral/user/returned/current.md")).unwrap(),
        "The handoff should stay visible after I close this chat.\n"
    );

    let rollover = execute(
        &registry,
        &context,
        "projectcentral.now.rollover",
        json!({"project":"lived-project","day":"2026-08-19","next_day":"2026-08-20"}),
    );
    assert!(rollover["carried"]
        .as_array()
        .unwrap()
        .iter()
        .any(|value| value
            .as_str()
            .unwrap()
            .ends_with(&format!("{question_id}.json"))));
    assert!(rollover["removed"]
        .as_array()
        .unwrap()
        .iter()
        .any(|value| value
            .as_str()
            .unwrap()
            .ends_with(&format!("{resolved_id}.json"))));
    assert!(rollover["removed"]
        .as_array()
        .unwrap()
        .iter()
        .any(|value| value
            .as_str()
            .unwrap()
            .ends_with(&format!("{learning_id}.json"))));

    assert!(project
        .join(NOW_AGENT_DIR)
        .join(format!("{question_id}.json"))
        .is_file());
    assert!(!project
        .join(NOW_AGENT_DIR)
        .join(format!("{resolved_id}.json"))
        .exists());
    assert!(!project
        .join(NOW_AGENT_DIR)
        .join(format!("{learning_id}.json"))
        .exists());

    let next_now = execute(
        &registry,
        &context,
        "projectcentral.now.inspect",
        json!({"project":"lived-project"}),
    );
    assert_eq!(next_now["open_questions"].as_array().unwrap().len(), 1);
    assert_eq!(next_now["open_questions"][0]["status"], "carried");
    assert_eq!(next_now["open_questions"][0]["run_ref"], "factory:run:74");
    assert_eq!(
        next_now["open_questions"][0]["session_ref"],
        "aikit:session:later"
    );

    let day = fs::read_to_string(project.join(NOW_DAY_DIR).join("2026-08-19.md")).unwrap();
    assert!(day.contains("Open design question"));
    assert!(day.contains("agent:nara"));
    assert!(day.contains("Resolved transient check"));
    assert!(day.contains("returned-learning.json"));
    assert!(day.contains("ProjectCentral/user/returned/current.md"));

    // NOW references native owners; it never creates replacement Session/Run/Focus/Wiki systems.
    assert!(!project.join("ProjectCentral/now/sessions").exists());
    assert!(!project.join("ProjectCentral/now/runs").exists());
    assert!(!project.join("ProjectCentral/now/focus").exists());
    assert!(project
        .join("ProjectCentral/agents/wiki/wiki.json")
        .is_file());
}

fn attempt(
    registry: &central_ctrl::ActionRegistry,
    context: &ActionExecutionContext<'_>,
    action: &str,
    input: Value,
) -> ActionResult {
    registry.execute(action, &input, context)
}

/// The field a NOW Action runs in, owned so a context can borrow it locally.
struct Field {
    root_options: RootOptions,
    connectors: central_ctrl::ConnectorRegistry,
    connector_context: ConnectorContext,
}

impl Field {
    fn new(central: &Path) -> Self {
        Self {
            root_options: RootOptions {
                explicit_root: Some(central.to_path_buf()),
                configured_root: None,
                home: None,
            },
            connectors: create_default_connector_registry(),
            connector_context: ConnectorContext::current(),
        }
    }

    fn context(&self) -> ActionExecutionContext<'_> {
        ActionExecutionContext {
            root_options: &self.root_options,
            connectors: &self.connectors,
            connector_context: &self.connector_context,
        }
    }
}

fn now_registry() -> central_ctrl::ActionRegistry {
    let mut registry = create_core_action_registry();
    register_projectcentral_actions(&mut registry);
    registry
}

#[test]
fn absent_project_central_is_classified_and_pointed_at_repair() {
    let temp = TempRoot::new();
    let central = temp.path().join("Central");
    let project = central.join("Work/unstrapped");
    fs::create_dir_all(&project).unwrap();
    let project_central = project.join("ProjectCentral");

    let field = Field::new(&central);
    let context = field.context();
    let registry = now_registry();

    let result = attempt(
        &registry,
        &context,
        "projectcentral.now.inspect",
        json!({"project":"unstrapped"}),
    );
    assert!(!result.ok);
    // The status (and therefore the CLI exit code) is unchanged; the code
    // carries the new classification.
    assert_eq!(
        result.status,
        central_ctrl::ResultStatus::InvalidCentralStructure
    );
    let error = result.error.expect("failure names its error");
    assert_eq!(error.code, "project_central_not_found");
    assert!(
        error.message.contains("unstrapped"),
        "message names the project: {}",
        error.message
    );
    assert!(
        error
            .message
            .contains(project_central.display().to_string().as_str()),
        "message names the expected ProjectCentral path: {}",
        error.message
    );
    let hint = error.repair_hint.expect("absent scaffolding is repairable");
    assert!(hint.contains("central.world.reproject.plan"));
    assert!(hint.contains("central.world.reproject.apply"));
    assert!(hint.contains("projectcentral.now.init"));
    let details = error.details.expect("details carry the project facts");
    assert_eq!(details["project"], "unstrapped");
    assert_eq!(
        details["project_central"],
        project_central.display().to_string()
    );
}

#[test]
fn incomplete_project_central_names_the_pieces_it_is_missing() {
    let temp = TempRoot::new();
    let central = temp.path().join("Central");
    let project = central.join("Work/half-built");
    fs::create_dir_all(project.join("ProjectCentral/user")).unwrap();

    let field = Field::new(&central);
    let context = field.context();
    let registry = now_registry();

    let result = attempt(
        &registry,
        &context,
        "projectcentral.now.inspect",
        json!({"project":"half-built"}),
    );
    assert!(!result.ok);
    let error = result.error.expect("failure names its error");
    assert_eq!(error.code, "project_central_incomplete");
    let missing = error.details.expect("details")["missing"]
        .as_array()
        .expect("missing pieces are listed")
        .iter()
        .map(|value| value.as_str().unwrap().to_owned())
        .collect::<Vec<_>>();
    for piece in [
        "ProjectCentral/project.json",
        "ProjectCentral/agents/governance",
        "ProjectCentral/agents/wiki/wiki.json",
    ] {
        assert!(
            missing.iter().any(|named| named == piece),
            "{piece} is named as missing: {missing:?}"
        );
        assert!(
            error.message.contains(piece),
            "message names {piece}: {}",
            error.message
        );
    }
    assert!(error.repair_hint.is_some());
}

#[test]
fn project_central_missing_only_secondary_pieces_is_still_incomplete() {
    let temp = TempRoot::new();
    let central = temp.path().join("Central");
    let project = central.join("Work/barely-there");
    fs::create_dir_all(&project).unwrap();
    initialize_projectcentral(&central, &project, "example/barely-there").unwrap();
    fs::remove_dir_all(project.join("ProjectCentral/agents/wiki")).unwrap();

    let field = Field::new(&central);
    let context = field.context();
    let registry = now_registry();

    let result = attempt(
        &registry,
        &context,
        "projectcentral.now.inspect",
        json!({"project":"barely-there"}),
    );
    assert!(!result.ok);
    let error = result.error.expect("failure names its error");
    assert_eq!(error.code, "project_central_incomplete");
    let details = error.details.expect("details");
    let missing = details["missing"].as_array().unwrap();
    assert!(missing
        .iter()
        .any(|value| value == "ProjectCentral/agents/wiki"));
    assert!(missing
        .iter()
        .any(|value| value == "ProjectCentral/agents/wiki/wiki.json"));
}

#[test]
fn malformed_manifest_keeps_invalid_central_structure_without_a_mechanical_hint() {
    let temp = TempRoot::new();
    let central = temp.path().join("Central");
    let project = central.join("Work/hand-authored");
    fs::create_dir_all(&project).unwrap();
    initialize_projectcentral(&central, &project, "example/hand-authored").unwrap();
    fs::write(project.join("ProjectCentral/project.json"), "{ not json").unwrap();

    let field = Field::new(&central);
    let context = field.context();
    let registry = now_registry();

    let result = attempt(
        &registry,
        &context,
        "projectcentral.now.inspect",
        json!({"project":"hand-authored"}),
    );
    assert!(!result.ok);
    // Genuinely malformed authored content: the original classification
    // stands, and no stamper is offered as a fix.
    let error = result.error.expect("failure names its error");
    assert_eq!(error.code, "invalid_central_structure");
    assert!(error.repair_hint.is_none());
}

#[test]
fn now_init_straps_only_missing_scaffolding_and_discloses_what_it_stamped() {
    let temp = TempRoot::new();
    let central = temp.path().join("Central");
    let project = central.join("Work/strapped");
    fs::create_dir_all(&project).unwrap();
    fs::write(project.join("notes.txt"), "ordinary project material\n").unwrap();
    fs::create_dir_all(project.join("ProjectCentral")).unwrap();
    fs::write(
        project.join("ProjectCentral/existing.txt"),
        "left exactly where it is\n",
    )
    .unwrap();

    let field = Field::new(&central);
    let context = field.context();
    let registry = now_registry();

    let data = execute(
        &registry,
        &context,
        "projectcentral.now.init",
        json!({"project":"strapped"}),
    );
    let strapped = data["strapped"]
        .as_array()
        .expect("strap is disclosed")
        .iter()
        .map(|value| value.as_str().unwrap().to_owned())
        .collect::<Vec<_>>();
    assert!(
        strapped.contains(&"Work/strapped/ProjectCentral/project.json".to_owned()),
        "manifest is disclosed as stamped: {strapped:?}"
    );
    assert!(project.join("ProjectCentral/project.json").is_file());
    assert!(project
        .join("ProjectCentral/agents/wiki/wiki.json")
        .is_file());
    assert!(project.join(NOW_DIR).is_dir());
    assert_eq!(
        fs::read_to_string(project.join("notes.txt")).unwrap(),
        "ordinary project material\n"
    );
    assert_eq!(
        fs::read_to_string(project.join("ProjectCentral/existing.txt")).unwrap(),
        "left exactly where it is\n"
    );

    // A second init finds nothing to strap: no disclosure, no writes.
    let again = attempt(
        &registry,
        &context,
        "projectcentral.now.init",
        json!({"project":"strapped"}),
    );
    assert!(again.ok, "second init succeeds: {:?}", again.error);
    assert!(again.data.unwrap().get("strapped").is_none());
}

#[test]
fn now_init_still_refuses_a_malformed_manifest() {
    let temp = TempRoot::new();
    let central = temp.path().join("Central");
    let project = central.join("Work/corrupt");
    fs::create_dir_all(&project).unwrap();
    initialize_projectcentral(&central, &project, "example/corrupt").unwrap();
    fs::write(project.join("ProjectCentral/project.json"), "{ not json").unwrap();

    let field = Field::new(&central);
    let context = field.context();
    let registry = now_registry();

    let result = attempt(
        &registry,
        &context,
        "projectcentral.now.init",
        json!({"project":"corrupt"}),
    );
    assert!(!result.ok);
    assert_eq!(
        result.error.expect("failure names its error").code,
        "invalid_central_structure"
    );
    assert_eq!(
        fs::read_to_string(project.join("ProjectCentral/project.json")).unwrap(),
        "{ not json",
        "the malformed manifest was never touched"
    );
}

#[test]
fn cli_inspect_reports_an_absent_project_central_with_exit_three_and_valid_json() {
    let temp = TempRoot::new();
    let central = temp.path().join("Central");
    let project = central.join("Work/cli-absent");
    fs::create_dir_all(&project).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_ctrl"))
        .args([
            "--root",
            central.to_str().unwrap(),
            "--json",
            "action",
            "run",
            "projectcentral.now.inspect",
            r#"{"project":"cli-absent"}"#,
        ])
        .output()
        .expect("ctrl binary should run");
    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(3));
    let parsed: Value = serde_json::from_slice(&output.stdout).expect("valid JSON on stdout");
    assert_eq!(parsed["ok"], false);
    assert_eq!(parsed["status"], "invalid_central_structure");
    assert_eq!(parsed["action"], "projectcentral.now.inspect");
    assert_eq!(parsed["error"]["code"], "project_central_not_found");
    let message = parsed["error"]["message"].as_str().unwrap();
    assert!(message.contains("cli-absent"), "project named: {message}");
    assert!(
        message.contains("ProjectCentral"),
        "expected path named: {message}"
    );
    assert_eq!(parsed["error"]["details"]["project"], json!("cli-absent"));
    let hint = parsed["error"]["repair_hint"].as_str().unwrap();
    assert!(hint.contains("central.world.reproject.plan"));
    assert!(hint.contains("central.world.reproject.apply"));
    assert!(hint.contains("projectcentral.now.init"));
}
