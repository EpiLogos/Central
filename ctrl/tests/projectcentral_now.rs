use central_ctrl::{
    create_core_action_registry, create_default_connector_registry, initialize_projectcentral,
    projectcentral_ops::register_projectcentral_actions, ActionExecutionContext, ConnectorContext,
    RootOptions, NOW_AGENT_DIR, NOW_DAY_DIR, NOW_DIR, NOW_USER_DIR, WIKI_RETURN_DIR,
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
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
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

    let result = execute(
        &registry,
        &context,
        "projectcentral.now.return",
        json!({
            "project":"lived-project",
            "actor":"agent:epii",
            "kind":"result",
            "subject":"Implemented source relation",
            "result":"The source relation now carries explicit provenance.",
            "status":"resolved",
            "run_ref":"factory:run:75",
            "evidence_refs":["git:commit:abc123"]
        }),
    );
    let result_id = result["handoff"]["id"].as_str().unwrap().to_owned();

    let updated = execute(
        &registry,
        &context,
        "projectcentral.now.update",
        json!({
            "project":"lived-project",
            "id":question_id,
            "status":"active",
            "preserve_refs":["central:project:design-question"]
        }),
    );
    assert_eq!(updated["handoff"]["status"], "active");

    let promotion = execute(
        &registry,
        &context,
        "projectcentral.now.promote",
        json!({
            "project":"lived-project",
            "source": format!("{NOW_AGENT_DIR}/{result_id}.json"),
            "target":"ProjectCentral/agents/wiki/returns/implemented-source-relation.json",
            "actor":"agent:epii",
            "reason":"Preserve the implementation result in the durable Agent Wiki return field."
        }),
    );
    assert_eq!(promotion["receipt"]["target"], "ProjectCentral/agents/wiki/returns/implemented-source-relation.json");
    assert!(project.join(WIKI_RETURN_DIR).join("implemented-source-relation.json").is_file());

    let report = execute(
        &registry,
        &context,
        "projectcentral.now.rollover",
        json!({
            "project":"lived-project",
            "from_day":"2026-08-19",
            "to_day":"2026-08-20"
        }),
    );
    assert_eq!(report["to_day"], "2026-08-20");
    assert!(project.join(NOW_DAY_DIR).join("2026-08-19.md").is_file());
    assert!(project.join(NOW_DIR).is_dir());
}
