use central_ctrl::{
    create_core_action_registry, create_default_connector_registry, initialize_projectcentral,
    projectcentral_ops::register_projectcentral_actions, ActionExecutionContext, ConnectorContext,
    RootOptions, NOW_AGENT_DIR, NOW_DAY_DIR, NOW_DIR, NOW_USER_DIR, WIKI_RETURN_DIR,
};
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

struct TempRoot(PathBuf);

impl TempRoot {
    fn new() -> Self {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let path = std::env::temp_dir().join(format!("central-now-{}-{nonce}", std::process::id()));
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
    assert!(project.join("ProjectCentral/agents/wiki/wiki.json").is_file());
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

    execute(&registry, &context, "projectcentral.now.init", json!({"project":"lived-project"}));

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
    assert!(project.join(WIKI_RETURN_DIR).join("now-day/returned-learning.json").is_file());

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
        .any(|value| value.as_str().unwrap().ends_with(&format!("{question_id}.json"))));
    assert!(rollover["removed"]
        .as_array()
        .unwrap()
        .iter()
        .any(|value| value.as_str().unwrap().ends_with(&format!("{resolved_id}.json"))));
    assert!(rollover["removed"]
        .as_array()
        .unwrap()
        .iter()
        .any(|value| value.as_str().unwrap().ends_with(&format!("{learning_id}.json"))));

    assert!(project.join(NOW_AGENT_DIR).join(format!("{question_id}.json")).is_file());
    assert!(!project.join(NOW_AGENT_DIR).join(format!("{resolved_id}.json")).exists());
    assert!(!project.join(NOW_AGENT_DIR).join(format!("{learning_id}.json")).exists());

    let next_now = execute(
        &registry,
        &context,
        "projectcentral.now.inspect",
        json!({"project":"lived-project"}),
    );
    assert_eq!(next_now["open_questions"].as_array().unwrap().len(), 1);
    assert_eq!(next_now["open_questions"][0]["status"], "carried");
    assert_eq!(next_now["open_questions"][0]["run_ref"], "factory:run:74");
    assert_eq!(next_now["open_questions"][0]["session_ref"], "aikit:session:later");

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
    assert!(project.join("ProjectCentral/agents/wiki/wiki.json").is_file());
}
