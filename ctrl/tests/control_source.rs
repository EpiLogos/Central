use central_ctrl::{
    control_source_bindings, CONTROL_GROUND_RELATIONS_SCHEMA, CONTROL_GROUND_RELATIONS_SOURCE,
};
use central_ctrl::{
    create_core_action_registry, initialize_central, run_cli, ActionExecutionContext,
    CliEnvironment, ConnectorContext, ConnectorRegistry, ResultStatus, RootOptions, CONTROL_ROOTS,
};
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static NEXT_TEMP_ROOT: AtomicU64 = AtomicU64::new(0);

struct TempRoot(PathBuf);

impl TempRoot {
    fn new() -> Self {
        Self::with_label("source")
    }

    fn with_label(label: &str) -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let sequence = NEXT_TEMP_ROOT.fetch_add(1, Ordering::Relaxed);
        let scratch = Path::new(env!("CARGO_MANIFEST_DIR")).join("../ProjectCentral/now/tmp");
        fs::create_dir_all(&scratch).unwrap();
        let path = scratch.join(format!(
            "central-control-{label}-{}-{nonce}-{sequence}",
            std::process::id()
        ));
        fs::create_dir(&path).unwrap();
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

fn execute(root: &Path, action: &str, input: serde_json::Value) -> central_ctrl::ActionResult {
    let mut registry = create_core_action_registry();
    central_ctrl::projectcentral_ops::register_projectcentral_actions(&mut registry);
    let connectors = ConnectorRegistry::default();
    let connector_context = ConnectorContext {
        platform: "test".to_owned(),
    };
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

fn declare_human_fixture(root: &Path, paths: &[&str]) {
    let relation_path = root.join(CONTROL_GROUND_RELATIONS_SOURCE);
    fs::create_dir_all(relation_path.parent().unwrap()).unwrap();
    let relations: Vec<_> = paths.iter().map(|path| json!({
        "ref": format!("central:source:control:root:{path}"), "path":path,
        "provenance":"human-authored", "standing":"durable-source",
        "roles":["agent-governance-source"], "treatment":"control-governance-retained-in-place"
    })).collect();
    fs::write(relation_path, serde_json::to_vec(&json!({
        "schema":CONTROL_GROUND_RELATIONS_SCHEMA, "project_id":"control:root",
        "relations":relations
    })).unwrap()).unwrap();
}

#[test]
fn control_open_keeps_agents_explicitly_mixed_after_the_governance_wiki_split() {
    let temporary = TempRoot::with_label("roots");
    let root = temporary.path().join("Central");
    initialize_central(&root).unwrap();
    for target in CONTROL_ROOTS {
        let result = execute(&root, "control.open", json!({ "target": target }));
        assert_eq!(result.status, ResultStatus::Success);
        let data = result.data.unwrap();
        assert_eq!(data["target"], target);
        assert_eq!(
            data["source_class"],
            if target == "agents" {
                "mixed"
            } else {
                "authored"
            }
        );
        assert_eq!(data["exists"], true);
        assert!(data["path"]
            .as_str()
            .unwrap()
            .ends_with(&format!("Control/{target}")));
    }
}

#[test]
fn control_search_reads_human_source_but_not_agent_wiki_as_authored_source() {
    let temporary = TempRoot::with_label("formats");
    let root = temporary.path().join("Central");
    initialize_central(&root).unwrap();
    fs::write(
        root.join("Control/user/about.md"),
        "# About\nI prefer quiet launchers.\n",
    )
    .unwrap();
    fs::write(
        root.join("Control/machines/tools.json"),
        "{\"launcher\":\"Raycast launcher\"}\n",
    )
    .unwrap();
    fs::create_dir_all(root.join("Control/agents/governance/nested")).unwrap();
    fs::write(
        root.join("Control/agents/governance/nested/voice.notes"),
        "launcher guidance can remain plain prose\n",
    )
    .unwrap();
    fs::write(
        root.join("Control/agents/wiki/should-not-surface.txt"),
        "launcher is Agent-maintained Wiki knowledge, not human-authored governance\n",
    )
    .unwrap();

    declare_human_fixture(&root, &["Control/user/about.md", "Control/machines/tools.json", "Control/agents/governance/nested/voice.notes"]);
    let result = execute(&root, "control.search", json!({ "query": "launcher" }));
    assert_eq!(result.status, ResultStatus::Success);
    let data = result.data.unwrap();
    let matches = data["matches"].as_array().unwrap();
    assert_eq!(matches.len(), 3);
    assert_eq!(data["files_scanned"], 3);
    assert!(data["skipped_sources"].as_array().unwrap().is_empty());
    assert!(matches
        .iter()
        .all(|item| item["source_class"] == "authored"));
    assert!(matches
        .iter()
        .any(|item| item["source_path"] == "Control/user/about.md"));
    assert!(matches
        .iter()
        .any(|item| item["source_path"] == "Control/machines/tools.json"));
    assert!(matches
        .iter()
        .any(|item| item["source_path"] == "Control/agents/governance/nested/voice.notes"));
    assert!(!matches.iter().any(|item| item["source_path"]
        .as_str()
        .unwrap()
        .contains("agents/wiki")));
}

#[test]
fn pre_split_direct_agent_governance_files_remain_human_authored_by_provenance() {
    let temporary = TempRoot::with_label("legacy-agents");
    let root = temporary.path().join("Central");
    initialize_central(&root).unwrap();
    fs::write(
        root.join("Control/agents/legacy-style.txt"),
        "Legacy governance prefers compact evidence.\n",
    )
    .unwrap();
    declare_human_fixture(&root, &["Control/agents/legacy-style.txt"]);
    let actual = central_ctrl::world_source::read_control_world_source(&root,
        "central:source:control:root:Control/agents/legacy-style.txt").unwrap();
    assert_eq!(actual.source.provenance, "human-authored");
    let result = execute(&root, "control.search", json!({ "query": "compact" }));
    let data = result.data.unwrap();
    assert_eq!(data["matches"].as_array().unwrap().len(), 1);
    assert_eq!(
        data["matches"][0]["source_path"],
        "Control/agents/legacy-style.txt"
    );
    assert_eq!(data["matches"][0]["source_class"], "authored");
}

#[test]
fn product_ground_is_ordinary_nested_user_source_not_a_fourth_control_root() {
    let temporary = TempRoot::with_label("product-ground");
    let root = temporary.path().join("Central");
    initialize_central(&root).unwrap();
    let product = root.join("Control/user/products/example");
    fs::create_dir_all(product.join("expressions")).unwrap();
    fs::create_dir_all(product.join("positions")).unwrap();
    fs::write(
        product.join("expressions/encounter.md"),
        "I want the encounter to remain directly manipulable by the person.\n",
    )
    .unwrap();
    fs::write(
        product.join("positions/INTERACTION.md"),
        "The primary interaction remains human-addressable.\n",
    )
    .unwrap();
    fs::write(
        product.join("VISION.md"),
        "The product should preserve human authorship while increasing agency.\n",
    )
    .unwrap();

    let opened = execute(&root, "control.open", json!({ "target": "user" }));
    assert_eq!(opened.status, ResultStatus::Success);
    assert_eq!(opened.data.unwrap()["source_class"], "authored");

    declare_human_fixture(&root, &["Control/user/products/example/positions/INTERACTION.md", "Control/user/products/example/VISION.md"]);
    let result = execute(&root, "control.search", json!({ "query": "human" }));
    assert_eq!(result.status, ResultStatus::Success);
    let data = result.data.unwrap();
    let matches = data["matches"].as_array().unwrap();
    assert_eq!(matches.len(), 2);
    assert!(matches.iter().all(|item| item["target"] == "user"));
    assert!(matches
        .iter()
        .all(|item| item["source_class"] == "authored"));
    assert!(matches.iter().any(
        |item| item["source_path"] == "Control/user/products/example/positions/INTERACTION.md"
    ));
    assert!(matches
        .iter()
        .any(|item| item["source_path"] == "Control/user/products/example/VISION.md"));
    assert_eq!(fs::read_dir(root.join(".central")).unwrap().count(), 0);
    assert_eq!(CONTROL_ROOTS, ["user", "agents", "machines"]);
}

#[test]
fn control_search_reports_unsupported_human_source_explicitly() {
    let temporary = TempRoot::with_label("unsupported");
    let root = temporary.path().join("Central");
    initialize_central(&root).unwrap();
    fs::write(
        root.join("Control/user/about.md"),
        "A searchable durable preference.\n",
    )
    .unwrap();
    fs::write(
        root.join("Control/agents/governance/archive.bin"),
        [0xff, 0xfe, 0x00, 0x80],
    )
    .unwrap();

    declare_human_fixture(&root, &["Control/user/about.md", "Control/agents/governance/archive.bin"]);
    let result = execute(&root, "control.search", json!({ "query": "durable" }));
    assert_eq!(result.status, ResultStatus::Success);
    let data = result.data.unwrap();
    assert_eq!(data["files_scanned"], 1);
    assert_eq!(data["matches"].as_array().unwrap().len(), 1);

    let skipped = data["skipped_sources"].as_array().unwrap();
    assert_eq!(skipped.len(), 1);
    assert_eq!(skipped[0]["target"], "agents");
    assert_eq!(
        skipped[0]["source_path"],
        "Control/agents/governance/archive.bin"
    );
    assert_eq!(skipped[0]["source_class"], "authored");
    assert_eq!(skipped[0]["reason"], "unsupported_non_text_source");
}

#[test]
fn direct_governance_filesystem_edits_are_visible_without_import_or_generated_index() {
    let temporary = TempRoot::with_label("direct-edit");
    let root = temporary.path().join("Central");
    initialize_central(&root).unwrap();
    let source = root.join("Control/agents/governance/style");
    fs::write(&source, "Prefer concise technical prose.\n").unwrap();

    let first = execute(&root, "control.search", json!({ "query": "concise" }));
    assert_eq!(first.data.unwrap()["matches"].as_array().unwrap().len(), 1);

    fs::write(&source, "Prefer spacious explanatory prose.\n").unwrap();
    let old = execute(&root, "control.search", json!({ "query": "concise" }));
    assert!(old.data.unwrap()["matches"].as_array().unwrap().is_empty());
    let changed = execute(&root, "control.search", json!({ "query": "spacious" }));
    assert_eq!(
        changed.data.unwrap()["matches"].as_array().unwrap().len(),
        1
    );
    assert_eq!(fs::read_dir(root.join(".central")).unwrap().count(), 0);
}

#[test]
fn retrieval_deny_marker_excludes_an_arbitrary_human_subtree() {
    let temporary = TempRoot::with_label("private");
    let root = temporary.path().join("Central");
    initialize_central(&root).unwrap();
    let private = root.join("Control/user/my-own-name-for-private-material");
    fs::create_dir_all(&private).unwrap();
    fs::write(private.join(".no-agent-retrieval"), "").unwrap();
    fs::write(
        private.join("note.md"),
        "This concealed-marker-text must not be retrieved.\n",
    )
    .unwrap();

    let result = execute(
        &root,
        "control.search",
        json!({ "query": "concealed-marker-text" }),
    );
    let data = result.data.unwrap();
    assert!(data["matches"].as_array().unwrap().is_empty());
    assert_eq!(data["skipped_sources"].as_array().unwrap().len(), 1);
    assert_eq!(data["skipped_sources"][0]["reason"], "not_agent_readable");
}

#[test]
fn control_actions_diagnose_invalid_target_and_missing_source_root() {
    let temporary = TempRoot::with_label("invalid");
    let root = temporary.path().join("Central");
    initialize_central(&root).unwrap();
    let invalid = execute(&root, "control.open", json!({ "target": "projects" }));
    assert_eq!(invalid.status, ResultStatus::InvalidInput);

    fs::remove_dir(root.join("Control/machines")).unwrap();
    let missing = execute(&root, "control.open", json!({ "target": "machines" }));
    assert_eq!(missing.status, ResultStatus::InvalidCentralStructure);
    let search = execute(&root, "control.search", json!({ "query": "anything" }));
    assert_eq!(search.status, ResultStatus::InvalidCentralStructure);
}

#[test]
fn cli_projects_control_open_and_search_over_the_same_actions() {
    let temporary = TempRoot::with_label("cli");
    let root = temporary.path().join("Central");
    initialize_central(&root).unwrap();
    fs::write(
        root.join("Control/user/note.txt"),
        "A durable preference for terminal clarity.\n",
    )
    .unwrap();
    let environment = CliEnvironment {
        configured_root: None,
        home: None,
    };

    let open = run_cli(
        &[
            "--root".to_owned(),
            root.display().to_string(),
            "control".to_owned(),
            "open".to_owned(),
            "user".to_owned(),
        ],
        &environment,
    );
    assert_eq!(open.exit_code, 0);
    assert!(open.output.contains("user"));
    assert!(open.output.contains("Control/user"));

    let search = run_cli(
        &[
            "--json".to_owned(),
            "--root".to_owned(),
            root.display().to_string(),
            "control".to_owned(),
            "search".to_owned(),
            "terminal".to_owned(),
            "clarity".to_owned(),
        ],
        &environment,
    );
    assert_eq!(search.exit_code, 0);
    let payload: serde_json::Value = serde_json::from_str(&search.output).unwrap();
    assert_eq!(payload["action"], "control.search");
    assert_eq!(payload["data"]["matches"][0]["source_class"], "unresolved");
    assert_eq!(payload["data"]["matches"][0]["source_binding"]["provenance"], "unresolved");
    assert!(payload["data"]["matches"][0]["source_revision"]["revision"].as_str().unwrap().starts_with("central.content-fnv1a64/v1:"));
    assert_eq!(
        payload["data"]["matches"][0]["source_path"],
        "Control/user/note.txt"
    );
}

#[test]
fn control_ground_relations_override_tree_provenance() {
    let temp = TempRoot::new();
    let central = temp.path();
    let statement = central.join("Control/agents/governance/engineering/agent-operations.md");
    fs::create_dir_all(statement.parent().unwrap()).unwrap();
    fs::write(&statement, "You branch small and commit honestly.\n").unwrap();

    // Tree default before relations exist.
    let bindings = control_source_bindings(central).unwrap();
    let tree_binding = bindings
        .iter()
        .find(|binding| binding.path == "Control/agents/governance/engineering/agent-operations.md")
        .expect("statement appears in control bindings");
    assert_eq!(tree_binding.provenance, "unresolved");
    assert!(tree_binding
        .roles
        .iter()
        .any(|role| role == "agent-governance-source"));

    // Relations file promotes it to recognised human ground.
    let relations = central.join(CONTROL_GROUND_RELATIONS_SOURCE);
    fs::create_dir_all(relations.parent().unwrap()).unwrap();
    fs::write(
        &relations,
        serde_json::to_string_pretty(&json!({
            "schema": CONTROL_GROUND_RELATIONS_SCHEMA,
            "project_id": "control:root",
            "relations": [{
                "ref": "central:source:control:root:Control/agents/governance/engineering/agent-operations.md",
                "path": "Control/agents/governance/engineering/agent-operations.md",
                "provenance": "human-authored",
                "standing": "durable-source",
                "roles": ["agent-governance-source"],
                "treatment": "control-governance-retained-in-place"
            }]
        }))
        .unwrap(),
    )
    .unwrap();

    let bindings = control_source_bindings(central).unwrap();
    let relation_binding = bindings
        .iter()
        .find(|binding| binding.path == "Control/agents/governance/engineering/agent-operations.md")
        .expect("statement still appears");
    assert_eq!(relation_binding.provenance, "human-authored");
    assert_eq!(
        relation_binding.source_ref,
        "central:source:control:root:Control/agents/governance/engineering/agent-operations.md"
    );
    // One physical source, one logical binding: the tree fallback is replaced, not duplicated.
    assert_eq!(
        bindings
            .iter()
            .filter(|binding| binding.path
                == "Control/agents/governance/engineering/agent-operations.md")
            .count(),
        1
    );
}

#[test]
fn control_ground_relations_missing_file_keeps_tree_defaults() {
    let temp = TempRoot::new();
    let central = temp.path();
    let statement = central.join("Control/agents/governance/engineering/coding-approach.md");
    fs::create_dir_all(statement.parent().unwrap()).unwrap();
    fs::write(&statement, "You change the smallest thing that can work.\n").unwrap();

    let bindings = control_source_bindings(central).unwrap();
    let binding = bindings
        .iter()
        .find(|binding| binding.path.ends_with("coding-approach.md"))
        .expect("statement appears");
    assert_eq!(binding.provenance, "unresolved");
}

#[test]
fn selected_root_and_governance_self_markers_refuse_truthfully_then_reopen_same_source() {
    use std::os::unix::fs::MetadataExt;
    let temporary = TempRoot::with_label("current-aperture");
    let root = temporary.path().join("Central");
    initialize_central(&root).unwrap();
    let source = root.join("Control/agents/governance/statement.md");
    fs::write(&source, b"# Current statement\ncurrent-control-needle\n").unwrap();
    fs::write(root.join("Control/user/sibling.txt"), b"sibling-control-needle\n").unwrap();
    let original = fs::metadata(&source).unwrap();
    for relative in ["", "Control", "Control/agents", "Control/agents/governance"] {
        let marker = root.join(relative).join(".no-agent-retrieval");
        fs::write(&marker, b"").unwrap();
        let index = execute(&root, "control.index", json!({}));
        assert!(!index.ok);
        assert!(index.data.is_none());
        assert_eq!(index.error.unwrap().details.unwrap()["io_error"]["kind"], "PermissionDenied");
        let search = execute(&root, "control.search", json!({"query":"control-needle"}));
        if relative == "Control/agents/governance" {
            let data = search.data.unwrap();
            assert_eq!(data["matches"].as_array().unwrap().len(), 1);
            assert_eq!(data["matches"][0]["text"], "sibling-control-needle");
        } else { assert!(!search.ok); assert!(search.data.is_none()); }
        fs::remove_file(&marker).unwrap();
        assert_eq!(execute(&root, "control.index", json!({})).data.unwrap()["statements"].as_array().unwrap().len(), 1);
        assert_eq!(execute(&root, "control.search", json!({"query":"control-needle"})).data.unwrap()["matches"].as_array().unwrap().len(), 2);
        let current = fs::metadata(&source).unwrap();
        assert_eq!((current.dev(), current.ino(), current.mtime(), current.mtime_nsec()), (original.dev(), original.ino(), original.mtime(), original.mtime_nsec()));
        assert_eq!(fs::read(&source).unwrap(), b"# Current statement\ncurrent-control-needle\n");
    }
}

#[test]
fn marked_governance_child_names_titles_and_body_are_absent_while_open_sibling_survives() {
    let temporary = TempRoot::with_label("private-index");
    let root = temporary.path().join("Central");
    initialize_central(&root).unwrap();
    let private = root.join("Control/agents/governance/private");
    fs::create_dir(&private).unwrap();
    fs::write(private.join(".no-agent-retrieval"), b"").unwrap();
    fs::write(private.join("hidden-file-name.md"), b"# Hidden-title-text\nhidden-body-text\n").unwrap();
    fs::write(root.join("Control/agents/governance/open.md"), b"# Open statement\nopen-body-text\n").unwrap();
    let index = execute(&root, "control.index", json!({}));
    let serialized = serde_json::to_string(&index).unwrap();
    assert!(index.ok);
    for private_text in ["hidden-file-name", "Hidden-title", "hidden-body"] { assert!(!serialized.contains(private_text)); }
    let data = index.data.unwrap();
    assert_eq!(data["statements"][0]["file"], "open.md");
    assert_eq!(data["statements"][0]["topic"], "Open statement");
    assert_eq!(data["statements"][0]["standing"], "undeclared");
    let search = execute(&root, "control.search", json!({"query":"body-text"}));
    let serialized = serde_json::to_string(&search).unwrap();
    assert!(!serialized.contains("hidden-file-name"));
    assert_eq!(search.data.unwrap()["matches"].as_array().unwrap().len(), 1);
}

#[test]
fn actual_binary_skip_and_four_mebibyte_budget_never_become_truncated_success() {
    let temporary = TempRoot::with_label("bounded-control");
    let root = temporary.path().join("Central");
    initialize_central(&root).unwrap();
    fs::write(root.join("Control/user/binary.bin"), [0xff, 0x00]).unwrap();
    fs::write(root.join("Control/user/nul.bin"), b"needle\0retained-binary").unwrap();
    fs::write(root.join("Control/user/ordinary.txt"), b"needle\n").unwrap();
    let data = execute(&root, "control.search", json!({"query":"needle"})).data.unwrap();
    assert_eq!(data["matches"].as_array().unwrap().len(), 1);
    assert_eq!(data["skipped_sources"].as_array().unwrap().len(), 2);
    assert!(data["skipped_sources"].as_array().unwrap().iter().all(|entry| entry["reason"] == "unsupported_non_text_source"));
    let oversized = root.join("Control/agents/governance/large.md");
    fs::write(&oversized, vec![b'x'; 4 * 1024 * 1024 + 1]).unwrap();
    for action in ["control.index", "control.search"] {
        let result = execute(&root, action, json!({"query":"needle"}));
        assert!(!result.ok);
        assert!(result.data.is_none());
        let details = result.error.unwrap().details.unwrap();
        assert_eq!(details["io_error"]["kind"], "InvalidData");
        assert_eq!(details["effects"], "none");
    }
    assert_eq!(fs::metadata(&oversized).unwrap().len(), 4 * 1024 * 1024 + 1);
    assert_eq!(fs::read(root.join("Control/user/binary.bin")).unwrap(), [0xff, 0x00]);
}

#[test]
fn actual_governance_standing_invalid_json_and_withholding_never_report_undeclared() {
    let temporary = TempRoot::with_label("standing-source");
    let root = temporary.path().join("Central");
    initialize_central(&root).unwrap();
    fs::write(root.join("Control/agents/governance/statement.md"), b"# Actual statement\n").unwrap();
    let relations = root.join("Control/relations");
    fs::create_dir(&relations).unwrap();
    let source = relations.join("source-relations.json");
    fs::write(&source, b"{malformed").unwrap();
    let result = execute(&root, "control.index", json!({}));
    assert_eq!(result.error.unwrap().details.unwrap()["io_error"]["kind"], "InvalidData");
    let actual = serde_json::to_vec(&json!({"schema":CONTROL_GROUND_RELATIONS_SCHEMA,"project_id":"control:root","relations":[{
        "ref":"central:source:control:root:Control/agents/governance/statement.md", "path":"Control/agents/governance/statement.md",
        "standing":"draft-source", "provenance":"unresolved", "roles":["agent-governance-source"], "treatment":"control-agent-governance"
    }]})).unwrap();
    fs::write(&source, &actual).unwrap();
    let data = execute(&root, "control.index", json!({})).data.unwrap();
    assert_eq!(data["drafts"], 1);
    assert_eq!(data["statements"][0]["standing"], "draft");
    fs::write(relations.join(".no-agent-retrieval"), b"").unwrap();
    let result = execute(&root, "control.index", json!({}));
    assert_eq!(result.error.unwrap().details.unwrap()["io_error"]["kind"], "PermissionDenied");
    assert_eq!(fs::read(&source).unwrap(), actual);
}

#[test]
fn actual_native_source_reference_with_spaces_and_colon_keeps_its_standing() {
    let temporary = TempRoot::with_label("source-reference-standing");
    let root = temporary.path().join("Central");
    initialize_central(&root).unwrap();
    let relative = "Control/agents/governance/spaced name:statement.md";
    fs::write(root.join(relative), b"# Native source address\n").unwrap();
    let actual_binding = control_source_bindings(&root).unwrap().into_iter()
        .find(|binding| binding.path == relative).unwrap();
    assert!(actual_binding.source_ref.contains("%20"));
    assert!(actual_binding.source_ref.contains("%3A"));
    let relations = root.join("Control/relations");
    fs::create_dir(&relations).unwrap();
    fs::write(relations.join("source-relations.json"), serde_json::to_vec(&json!({
        "schema":CONTROL_GROUND_RELATIONS_SCHEMA, "project_id":"control:root",
        "relations":[{"ref":actual_binding.source_ref,"path":relative,"standing":"durable-source",
            "provenance":"unresolved","roles":["agent-governance-source"],"treatment":"control-agent-governance"}]
    })).unwrap()).unwrap();
    let data = execute(&root, "control.index", json!({})).data.unwrap();
    assert_eq!(data["statements"][0]["file"], "spaced name:statement.md");
    assert_eq!(data["statements"][0]["standing"], "durable");
    assert_eq!(fs::read(root.join(relative)).unwrap(), b"# Native source address\n");
}

#[test]
fn actual_action_body_eacces_preserves_original_errno_without_fake_topic() {
    use std::os::unix::fs::PermissionsExt;
    if unsafe { libc::geteuid() } == 0 {
        eprintln!("qualification unavailable: actual native Action EACCES requires a nonroot OS user");
        return;
    }
    let temporary = TempRoot::with_label("native-action-eacces");
    let root = temporary.path().join("Central");
    initialize_central(&root).unwrap();
    let source = root.join("Control/agents/governance/retained.md");
    fs::write(&source, b"# Retained private title\n").unwrap();
    struct Restore(PathBuf);
    impl Drop for Restore {
        fn drop(&mut self) { let _ = fs::set_permissions(&self.0, fs::Permissions::from_mode(0o600)); }
    }
    let restore = Restore(source.clone());
    fs::set_permissions(&source, fs::Permissions::from_mode(0o000)).unwrap();
    let actual = fs::read(&source).unwrap_err();
    for action in ["control.index", "control.search"] {
        let result = execute(&root, action, json!({"query":"Retained"}));
        assert!(!result.ok);
        assert!(result.data.is_none());
        let details = result.error.unwrap().details.unwrap();
        assert_eq!(details["io_error"]["kind"], format!("{:?}", actual.kind()));
        assert_eq!(details["io_error"]["raw_os_error"], json!(actual.raw_os_error()));
    }
    drop(restore);
    assert_eq!(fs::read(source).unwrap(), b"# Retained private title\n");
}

#[test]
fn actual_control_cli_dispatch_and_native_action_share_current_admission() {
    let temporary = TempRoot::with_label("native-cli-current");
    let root = temporary.path().join("Central");
    initialize_central(&root).unwrap();
    let private = root.join("Control/agents/governance/private");
    fs::create_dir(&private).unwrap();
    fs::write(private.join(".no-agent-retrieval"), b"").unwrap();
    fs::write(private.join("hidden.md"), b"# Hidden native title\n").unwrap();
    fs::write(root.join("Control/agents/governance/open.md"), b"# Open native title\n").unwrap();
    let native = serde_json::to_value(execute(&root, "control.index", json!({}))).unwrap();
    let cli = run_cli(&["--json".into(), "--root".into(), root.display().to_string(), "control".into(), "index".into()], &CliEnvironment {configured_root:None,home:None});
    assert_eq!(cli.exit_code, 0, "actual CLI result={}", cli.output);
    assert_eq!(serde_json::from_str::<serde_json::Value>(&cli.output).unwrap(), native);
    assert!(!cli.output.contains("hidden.md"));
    assert_eq!(fs::read(private.join("hidden.md")).unwrap(), b"# Hidden native title\n");
    assert_eq!(fs::read(root.join("Control/agents/governance/open.md")).unwrap(), b"# Open native title\n");
}

#[test]
fn actual_native_return_remains_useful_without_human_authorship_or_reminted_source() {
    let temporary = TempRoot::with_label("return-disclosure");
    let root = temporary.path().join("Central");
    initialize_central(&root).unwrap();
    let returned = execute(&root, "projectcentral.now.return", json!({
        "actor":"agent:fixture:guardian", "kind":"note", "subject":"Native return disclosure",
        "result":"unique-return-disclosure-needle", "status":"active"
    }));
    assert_eq!(returned.status, ResultStatus::Success, "{returned:?}");
    let data = returned.data.unwrap();
    let serialized = serde_json::to_string(&data).unwrap();
    assert!(serialized.contains("agent-authored-bounded-return"));
    let carrier = root.join(data["source"].as_str().unwrap());
    let stored: serde_json::Value = serde_json::from_slice(&fs::read(&carrier).unwrap()).unwrap();
    assert_eq!(stored["schema"], "central.project-now.handoff/v1");
    assert_eq!(stored["provenance"], "agent-authored-bounded-return");
    assert_eq!(stored["actor"], "agent:fixture:guardian");
    let reading = execute(&root, "control.search", json!({"query":"unique-return-disclosure-needle"}));
    assert_eq!(reading.status, ResultStatus::Success, "{reading:?}");
    let data = reading.data.unwrap();
    let hits = data["matches"].as_array().unwrap();
    assert!(!hits.is_empty());
    for hit in hits {
        assert_eq!(hit["source_class"], "unresolved");
        assert!(hit["source_binding"].is_null());
        let path = root.join(hit["source_path"].as_str().unwrap());
        let bytes = fs::read(&path).unwrap();
        let relative = Path::new(hit["source_path"].as_str().unwrap());
        let listing = central_ctrl::files::list_files(&root, relative.parent().unwrap().to_str().unwrap()).unwrap();
        let entry = listing.entries.into_iter().find(|entry| entry.name == relative.file_name().unwrap().to_str().unwrap()).unwrap();
        let reread = execute(&root, "central.files.read", json!({"location":entry.location}));
        assert_eq!(reread.status, ResultStatus::Success, "{reread:?}");
        assert_eq!(hit["source_revision"]["revision"], reread.data.unwrap()["revision"]);
        assert_eq!(fs::read(path).unwrap(), bytes);
    }
}

#[test]
fn actual_lookalike_and_declared_generated_or_human_sources_keep_native_disclosure() {
    let temporary = TempRoot::with_label("actual-disclosure");
    let root = temporary.path().join("Central");
    initialize_central(&root).unwrap();
    let unbound = "Control/agents/lookalike.md";
    let human = "Control/agents/legacy-human.md";
    let generated = "Control/agents/governance/generated.md";
    for path in [unbound, human, generated] {
        fs::write(root.join(path), b"disclosure-needle").unwrap();
    }
    let source = root.join(CONTROL_GROUND_RELATIONS_SOURCE);
    fs::create_dir_all(source.parent().unwrap()).unwrap();
    fs::write(&source, serde_json::to_vec(&json!({
        "schema":CONTROL_GROUND_RELATIONS_SCHEMA, "project_id":"control:root",
        "retained_extension":{"owner":"fixture"},
        "relations":[
            {"ref":"central:source:control:root:legacy-human", "path":human,
             "provenance":"human-adopted","standing":"durable-source","roles":["agent-governance-source"],"treatment":"retain-native-in-place"},
            {"ref":"central:source:control:root:generated", "path":generated,
             "provenance":"generated-derived","standing":"durable-source","roles":["projection"],"treatment":"generated-derived"}
        ]
    })).unwrap()).unwrap();
    let before = fs::read(&source).unwrap();
    let data = execute(&root, "control.search", json!({"query":"disclosure-needle"})).data.unwrap();
    let hits = data["matches"].as_array().unwrap();
    assert_eq!(hits.len(), 3);
    let selected = |path: &str| hits.iter().find(|hit| hit["source_path"] == path).unwrap();
    assert_eq!(selected(unbound)["source_class"], "unresolved");
    assert!(selected(unbound)["source_binding"].is_null());
    assert_eq!(selected(human)["source_class"], "authored");
    assert_eq!(selected(human)["source_binding"]["ref"], "central:source:control:root:legacy-human");
    assert_eq!(selected(generated)["source_class"], "unresolved");
    assert_eq!(selected(generated)["source_binding"]["standing"], "durable-source");
    assert_eq!(selected(generated)["source_binding"]["provenance"], "generated-derived");
    for path in [human, generated] {
        let hit = selected(path);
        let native = central_ctrl::world_source::read_control_world_source(&root,
            hit["source_binding"]["ref"].as_str().unwrap()).unwrap();
        assert_eq!(serde_json::to_value(native.source).unwrap(), hit["source_binding"]);
        assert_eq!(native.revision.revision, hit["source_revision"]["revision"]);
    }
    assert_eq!(fs::read(source).unwrap(), before);
}

#[test]
fn selected_native_skill_manifest_matches_bulk_binding_without_scanning_other_skills() {
    let temporary = TempRoot::with_label("skill-disclosure");
    let root = temporary.path().join("Central");
    initialize_central(&root).unwrap();
    for (prefix,scope) in [("Control/user/skills", "control-user"),
                          ("Control/machines/fixture/skills", "control-machine")] {
        let directory = root.join(prefix).join("selected");
        fs::create_dir_all(&directory).unwrap();
        fs::write(directory.join("SKILL.md"), b"selected-skill-needle").unwrap();
        let manifest = serde_json::to_vec(&json!({
            "schema":"central.skill/v1","name":"selected","scope":scope,
            "standing":"retired","provenance":"adopted",
            "retirement":{"retired_by":"human:fixture","retired_at_unix_seconds":1,"retirement_reason":"fixture"},
            "unknown_extension":{"retained":true}
        })).unwrap();
        fs::write(directory.join("skill.json"), &manifest).unwrap();
        let data = execute(&root, "control.search", json!({"query":"selected-skill-needle"})).data.unwrap();
        let path = format!("{prefix}/selected/SKILL.md");
        let hit = data["matches"].as_array().unwrap().iter().find(|hit| hit["source_path"] == path).unwrap();
        let bulk = control_source_bindings(&root).unwrap().into_iter().find(|binding| binding.path == path).unwrap();
        assert_eq!(serde_json::to_value(bulk).unwrap(), hit["source_binding"]);
        assert_eq!(hit["source_binding"]["provenance"], "human-adopted");
        assert_eq!(hit["source_binding"]["standing"], "retired");
        assert_eq!(hit["source_class"], "authored");
        assert_eq!(fs::read(directory.join("skill.json")).unwrap(), manifest);
    }
}

#[test]
fn same_native_validator_rejects_duplicate_refs_or_paths_in_search_and_bulk_bindings() {
    let temporary = TempRoot::with_label("ambiguous-relations");
    let root = temporary.path().join("Central");
    initialize_central(&root).unwrap();
    let path = "Control/agents/governance/a.md";
    fs::write(root.join(path), b"ambiguous-needle").unwrap();
    fs::write(root.join("Control/agents/governance/b.md"), b"ambiguous-needle").unwrap();
    let source = root.join(CONTROL_GROUND_RELATIONS_SOURCE);
    fs::create_dir_all(source.parent().unwrap()).unwrap();
    for duplicate_path in [false, true] {
        let second_path = if duplicate_path { path } else { "Control/agents/governance/b.md" };
        let second_ref = if duplicate_path { "central:source:control:root:second" } else { "central:source:control:root:first" };
        let relation = |reference: &str, path: &str| json!({
            "ref":reference,"path":path,"provenance":"unresolved","standing":"durable-source",
            "roles":["agent-governance-source"],"treatment":"control-agent-governance"
        });
        let bytes = serde_json::to_vec(&json!({
            "schema":CONTROL_GROUND_RELATIONS_SCHEMA,"project_id":"control:root",
            "relations":[relation("central:source:control:root:first",path),relation(second_ref,second_path)]
        })).unwrap();
        fs::write(&source, &bytes).unwrap();
        let bulk = control_source_bindings(&root).unwrap_err();
        assert_eq!(bulk.kind(), std::io::ErrorKind::InvalidData);
        let result = execute(&root, "control.search", json!({"query":"ambiguous-needle"}));
        let details = result.error.unwrap().details.unwrap();
        assert_eq!(details["io_error"]["kind"], "InvalidData");
        assert!(bulk.to_string().contains("ambiguous duplicate source relation"));
        assert_eq!(fs::read(&source).unwrap(), bytes);
    }
}

#[test]
fn actual_control_open_distinguishes_final_absence_form_marker_and_unchanged_alias() {
    use std::os::unix::fs::symlink;
    let temporary = TempRoot::with_label("open-aperture");
    let root = temporary.path().join("Central");
    initialize_central(&root).unwrap();
    let alias = temporary.path().join("root-alias");
    symlink(&root, &alias).unwrap();
    assert_eq!(execute(&alias, "control.open", json!({"target":"user"})).status, ResultStatus::Success);
    fs::remove_dir(root.join("Control/user")).unwrap();
    let absent = execute(&root, "control.open", json!({"target":"user"}));
    assert_eq!(absent.status, ResultStatus::InvalidCentralStructure);
    assert_eq!(absent.error.unwrap().details.unwrap()["exists"], false);
    fs::write(root.join("Control/user"), b"wrong-form").unwrap();
    let wrong = execute(&root, "control.open", json!({"target":"user"}));
    assert_eq!(wrong.error.unwrap().details.unwrap()["io_error"]["kind"], "InvalidInput");
    fs::remove_file(root.join("Control/user")).unwrap();
    let outside = temporary.path().join("outside");
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("retained.txt"), b"retained").unwrap();
    symlink(&outside, root.join("Control/user")).unwrap();
    assert!(!execute(&root, "control.open", json!({"target":"user"})).ok);
    assert_eq!(fs::read(outside.join("retained.txt")).unwrap(), b"retained");
    fs::remove_file(root.join("Control/user")).unwrap();
    fs::create_dir(root.join("Control/user")).unwrap();
    fs::write(root.join("Control/user/.no-agent-retrieval"), b"").unwrap();
    let denied = execute(&root, "control.open", json!({"target":"user"}));
    assert_eq!(denied.error.unwrap().details.unwrap()["io_error"]["kind"], "PermissionDenied");
    fs::remove_file(root.join("Control/user/.no-agent-retrieval")).unwrap();
    assert_eq!(execute(&root, "control.open", json!({"target":"user"})).status, ResultStatus::Success);
}
