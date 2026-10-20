//! `central.files.read-batch` answers every location exactly as `central.files.read` does.
//!
//! A batch derives the Control source table once and looks each file's binding up, where a single read derives the whole table for
//! every file under `Control/`. The proof is equality with the single read — the whole result envelope, success or refusal — over
//! files the table treats differently: a governance source bound by a relation, an ordinary flow carrier (no source), a plain
//! Control file, a retrieval-excluded one, a symlink, a binary file, an absent one, a Project file, and a location that is not one.
use central_ctrl::{
    create_core_action_registry, initialize_central, ActionExecutionContext, ConnectorContext,
    ConnectorRegistry, ResultStatus, RootOptions, CONTROL_GROUND_RELATIONS_SCHEMA,
    CONTROL_GROUND_RELATIONS_SOURCE,
};
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static NEXT_TEMP_ROOT: AtomicU64 = AtomicU64::new(0);

/// A scratch directory under the repository's own scratch area, removed when the test ends.
struct TempRoot(PathBuf);

impl TempRoot {
    fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let sequence = NEXT_TEMP_ROOT.fetch_add(1, Ordering::Relaxed);
        let scratch = Path::new(env!("CARGO_MANIFEST_DIR")).join("../ProjectCentral/now/tmp");
        fs::create_dir_all(&scratch).unwrap();
        let path = scratch.join(format!(
            "central-files-batch-{}-{nonce}-{sequence}",
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

fn execute(root: &Path, action: &str, input: Value) -> central_ctrl::ActionResult {
    let registry = create_core_action_registry();
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

/// The location the listing of `directory` names for `name`.
fn location(root: &Path, directory: &str, name: &str) -> Value {
    let listing = central_ctrl::files::list_files(root, directory).unwrap();
    let entry = listing
        .entries
        .into_iter()
        .find(|entry| entry.name == name)
        .unwrap_or_else(|| panic!("{directory}/{name} is listed"));
    serde_json::to_value(entry.location).unwrap()
}

fn as_value(result: central_ctrl::ActionResult) -> Value {
    serde_json::to_value(result).unwrap()
}

/// A root with files the source table treats differently, and the locations to read, in a fixed order.
fn world(root: &Path) -> Vec<Value> {
    initialize_central(root).unwrap();
    let governed = "Control/agents/governance/governed.md";
    fs::create_dir_all(root.join("Control/agents/governance")).unwrap();
    fs::write(root.join(governed), "# governed\n").unwrap();
    let relations = root.join(CONTROL_GROUND_RELATIONS_SOURCE);
    fs::create_dir_all(relations.parent().unwrap()).unwrap();
    fs::write(
        &relations,
        serde_json::to_vec(&json!({
            "schema": CONTROL_GROUND_RELATIONS_SCHEMA, "project_id": "control:root",
            "relations": [{
                "ref": "central:source:control:root:governed", "path": governed,
                "provenance": "human-authored", "standing": "durable-source",
                "roles": ["agent-governance-source"], "treatment": "control-governance-retained-in-place"
            }]
        }))
        .unwrap(),
    )
    .unwrap();
    fs::create_dir_all(root.join("Control/user/flows")).unwrap();
    fs::write(
        root.join("Control/user/flows/carrier.json"),
        "{\"rows\":[]}\n",
    )
    .unwrap();
    fs::write(root.join("Control/user/plain.md"), "plain\n").unwrap();
    fs::write(root.join("Control/user/blob.bin"), [0_u8, 1, 2, 0, 255]).unwrap();
    fs::create_dir_all(root.join("Control/user/private")).unwrap();
    fs::write(root.join("Control/user/private/secret.md"), "secret\n").unwrap();
    // Named before the marker is written: a listing of an excluded directory is itself refused.
    let secret = location(root, "Control/user/private", "secret.md");
    fs::write(root.join("Control/user/private/.no-agent-retrieval"), "").unwrap();
    std::os::unix::fs::symlink(
        root.join("Control/user/plain.md"),
        root.join("Control/user/link.md"),
    )
    .unwrap();
    fs::create_dir_all(root.join("Work/Bare/src")).unwrap();
    fs::write(root.join("Work/Bare/src/main.rs"), "fn main() {}\n").unwrap();
    fs::write(root.join("Control/user/gone.md"), "soon absent\n").unwrap();
    let gone = location(root, "Control/user", "gone.md");
    fs::remove_file(root.join("Control/user/gone.md")).unwrap();
    vec![
        location(root, "Control/agents/governance", "governed.md"),
        location(root, "Control/user/flows", "carrier.json"),
        location(root, "Control/user", "plain.md"),
        location(root, "Control/user", "plain.md"), // a duplicate is answered each time
        secret,
        location(root, "Control/user", "link.md"),
        location(root, "Control/user", "blob.bin"),
        gone,
        location(root, "Work/Bare/src", "main.rs"),
        json!({"not": "a location"}),
        Value::Null,
    ]
}

#[test]
fn every_item_is_the_single_reads_whole_envelope() {
    let temporary = TempRoot::new();
    let root = temporary.path().join("Central");
    let locations = world(&root);
    for encoding in [None, Some("utf-8"), Some("base64")] {
        let mut input = json!({ "locations": locations });
        if let Some(encoding) = encoding {
            input["encoding"] = json!(encoding);
        }
        let batch = as_value(execute(&root, "central.files.read-batch", input));
        assert_eq!(batch["ok"], true, "{batch}");
        assert_eq!(batch["action"], "central.files.read-batch");
        assert_eq!(batch["data"]["schema"], "central.file-batch-reading/v1");
        assert_eq!(batch["data"]["requested"], locations.len());
        let items = batch["data"]["items"].as_array().unwrap();
        assert_eq!(items.len(), locations.len());
        let mut answered = 0;
        for (index, requested) in locations.iter().enumerate() {
            let mut single_input = json!({ "location": requested });
            if let Some(encoding) = encoding {
                single_input["encoding"] = json!(encoding);
            }
            let single = as_value(execute(&root, "central.files.read", single_input));
            assert_eq!(
                items[index]["location"], *requested,
                "item {index} names its location"
            );
            assert_eq!(
                items[index]["result"], single,
                "item {index} ({requested}) answers as the single Action, encoding {encoding:?}"
            );
            answered += usize::from(single["ok"] == true);
        }
        assert_eq!(batch["data"]["answered"], answered);
        assert_eq!(batch["data"]["refused"], locations.len() - answered);
        assert_eq!(batch["data"]["automatic_agent_or_model_invocation"], false);
    }
}

#[test]
fn the_files_that_differ_in_the_table_keep_their_difference() {
    let temporary = TempRoot::new();
    let root = temporary.path().join("Central");
    let locations = world(&root);
    let batch = as_value(execute(
        &root,
        "central.files.read-batch",
        json!({ "locations": locations }),
    ));
    let items = batch["data"]["items"].as_array().unwrap();
    // The governance source is bound by its relation; the ordinary flow carrier and a plain file are not.
    assert_eq!(
        items[0]["result"]["data"]["source"]["roles"],
        json!(["agent-governance-source"]),
        "{}",
        items[0]
    );
    assert!(
        items[1]["result"]["data"]["source"].is_null(),
        "{}",
        items[1]
    );
    // Refusals are items, not the batch's failure.
    assert_eq!(
        items[4]["result"]["ok"], false,
        "retrieval-excluded: {}",
        items[4]
    );
    assert_eq!(items[5]["result"]["ok"], false, "symlink: {}", items[5]);
    assert_eq!(
        items[6]["result"]["ok"], false,
        "binary as text: {}",
        items[6]
    );
    assert_eq!(items[7]["result"]["ok"], false, "absent: {}", items[7]);
    assert_eq!(items[8]["result"]["ok"], true, "{}", items[8]);
    assert_eq!(items[8]["result"]["data"]["project"]["name"], "Bare");
    assert_eq!(
        items[9]["result"]["status"], "invalid_input",
        "{}",
        items[9]
    );
    assert_eq!(
        items[10]["result"]["status"], "invalid_input",
        "{}",
        items[10]
    );
}

#[test]
fn a_fault_in_the_request_refuses_the_call() {
    let temporary = TempRoot::new();
    let root = temporary.path().join("Central");
    let locations = world(&root);
    let refused = |input: Value| execute(&root, "central.files.read-batch", input);
    for input in [
        json!({}),
        json!({ "locations": [] }),
        json!({ "locations": "plain.md" }),
        json!({ "locations": vec![locations[2].clone(); 257] }),
        json!({ "locations": [locations[2].clone()], "encoding": "latin-1" }),
    ] {
        let result = refused(input.clone());
        assert_eq!(
            result.status,
            ResultStatus::InvalidInput,
            "{input}: {result:?}"
        );
    }
    let most = refused(json!({ "locations": vec![locations[2].clone(); 256] }));
    assert_eq!(most.status, ResultStatus::Success, "{most:?}");
    assert_eq!(most.data.unwrap()["answered"], 256);
}

#[test]
fn the_batch_is_read_only_and_listed() {
    let registry = create_core_action_registry();
    let descriptor = registry.get("central.files.read-batch").unwrap();
    assert_eq!(
        descriptor.mutation_class,
        central_ctrl::MutationClass::ReadOnly
    );
    assert!(descriptor
        .inputs
        .iter()
        .any(|input| input.name == "locations" && input.required));
}
