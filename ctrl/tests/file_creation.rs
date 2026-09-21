//! Real registered owner Actions and native files, not a renderer fixture.
use central_ctrl::{
    create_core_action_registry, ActionExecutionContext, ActionResult, ConnectorContext,
    ConnectorRegistry, ResultStatus, RootOptions,
};
use serde_json::{json, Value};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

// A per-process monotonic sequence guarantees a distinct ground per Ground even
// when two parallel test threads read the clock within the same tick (a coarse
// clock, e.g. on macOS, otherwise lets nonces collide and grounds overlap).
static GROUND_SEQ: AtomicU64 = AtomicU64::new(0);

struct Ground(PathBuf);
impl Ground {
    fn new() -> Self {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let seq = GROUND_SEQ.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "central-first-save-{}-{nonce}-{seq}",
            std::process::id()
        ));
        fs::create_dir_all(&root).unwrap();
        let ground = Self(root.canonicalize().unwrap());
        assert_eq!(
            call(&ground.0, "central.init", json!({})).status,
            ResultStatus::Success
        );
        fs::create_dir_all(ground.0.join("notes")).unwrap();
        ground
    }
}
impl Drop for Ground {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn call(root: &Path, action: &str, input: Value) -> ActionResult {
    let registry = create_core_action_registry();
    let options = RootOptions {
        explicit_root: Some(root.into()),
        ..RootOptions::default()
    };
    let connectors = ConnectorRegistry::default();
    let connector_context = ConnectorContext {
        platform: "test".into(),
    };
    registry.execute(
        action,
        &input,
        &ActionExecutionContext {
            root_options: &options,
            connectors: &connectors,
            connector_context: &connector_context,
        },
    )
}
fn parent(root: &Path, path: &str) -> Value {
    // Use the native directory's actual returned location, not a minted path.
    let reply = call(root, "central.files.list", json!({"path":path}));
    assert_eq!(reply.status, ResultStatus::Success, "{reply:?}");
    reply.data.unwrap()["location"].clone()
}
fn create(root: &Path) -> Value {
    json!({"parent":parent(root,"notes"),"name":"inquiry.expression.json",
    "content":"{\"schema\":\"oi.expression/v1\",\"expression_ref\":\"expression:inquiry\"}",
    "expected_absent":true,"operation_ref":"return:inquiry:first-save","actor":"human:author","actor_kind":"human"})
}

#[test]
fn first_save_reopens_and_shares_the_existing_native_history() {
    let ground = Ground::new();
    let input = create(&ground.0);
    let saved = call(&ground.0, "central.files.create", input.clone());
    assert_eq!(saved.status, ResultStatus::Success, "{saved:?}");
    let data = saved.data.unwrap();
    assert_eq!(data["outcome"], "created");
    assert_eq!(data["current"]["content"], input["content"]);
    let reopened = call(
        &ground.0,
        "central.files.read",
        json!({"location":data["location"]}),
    );
    assert_eq!(reopened.status, ResultStatus::Success, "{reopened:?}");
    assert_eq!(reopened.data.unwrap()["revision"], data["revision"]);
    let repeated = call(&ground.0, "central.files.create", input);
    assert_eq!(repeated.status, ResultStatus::Success, "{repeated:?}");
    assert_eq!(repeated.data.unwrap()["outcome"], "unchanged");
    let history = call(
        &ground.0,
        "central.files.history",
        json!({"location":data["location"]}),
    )
    .data
    .unwrap();
    assert_eq!(history["entries"].as_array().unwrap().len(), 1);
    assert_eq!(history["entries"][0]["previous_revision"], "");
    let edited = call(
        &ground.0,
        "central.files.write",
        json!({"location":data["location"],"expected_revision":data["revision"],"content":"revised composition","actor":"human:author","actor_kind":"human"}),
    );
    assert_eq!(edited.status, ResultStatus::Success, "{edited:?}");
    assert_eq!(
        call(
            &ground.0,
            "central.files.history",
            json!({"location":data["location"]})
        )
        .data
        .unwrap()["entries"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}

#[test]
fn another_operation_or_late_first_save_cannot_overwrite_existing_work() {
    let ground = Ground::new();
    let input = create(&ground.0);
    assert_eq!(
        call(&ground.0, "central.files.create", input.clone()).status,
        ResultStatus::Success
    );
    let path = ground.0.join("notes/inquiry.expression.json");
    let before = fs::read(&path).unwrap();
    let mut different = input.clone();
    different["content"] = json!("overwrite");
    different["operation_ref"] = json!("return:other");
    assert_ne!(
        call(&ground.0, "central.files.create", different).status,
        ResultStatus::Success
    );
    assert_eq!(fs::read(&path).unwrap(), before);
    fs::write(&path, "newer external work").unwrap();
    assert_ne!(
        call(&ground.0, "central.files.create", input).status,
        ResultStatus::Success
    );
    assert_eq!(fs::read_to_string(&path).unwrap(), "newer external work");
}

#[test]
fn creation_requires_explicit_absence_and_cannot_escape_or_relabel_protected_ground() {
    let ground = Ground::new();
    let base = create(&ground.0);
    for name in [
        "../secret",
        "/absolute",
        ".git",
        "ProjectCentral",
        ".central",
        "two/names",
        "",
        "..",
    ] {
        let mut input = base.clone();
        input["name"] = json!(name);
        assert_ne!(
            call(&ground.0, "central.files.create", input).status,
            ResultStatus::Success,
            "{name}"
        );
    }
    for path in ["Control", "Control/user", "Control/agents"] {
        let mut input = base.clone();
        input["parent"] = parent(&ground.0, path);
        assert_ne!(
            call(&ground.0, "central.files.create", input).status,
            ResultStatus::Success,
            "{path}"
        );
    }
    let mut input = base.clone();
    input["expected_absent"] = json!(false);
    assert_ne!(
        call(&ground.0, "central.files.create", input).status,
        ResultStatus::Success
    );
    let mut input = base;
    input["parent"]["root"] = json!("/different-root");
    assert_ne!(
        call(&ground.0, "central.files.create", input).status,
        ResultStatus::Success
    );
    assert!(!ground.0.join("notes/inquiry.expression.json").exists());
}

#[cfg(unix)]
#[test]
fn symlink_destination_is_not_followed_and_new_material_is_private_by_default() {
    use std::os::unix::fs::{symlink, PermissionsExt};
    let ground = Ground::new();
    let input = create(&ground.0);
    let target = ground.0.join("target");
    fs::write(&target, "private existing bytes").unwrap();
    let path = ground.0.join("notes/inquiry.expression.json");
    symlink(&target, &path).unwrap();
    assert_ne!(
        call(&ground.0, "central.files.create", input.clone()).status,
        ResultStatus::Success
    );
    assert_eq!(
        fs::read_to_string(&target).unwrap(),
        "private existing bytes"
    );
    fs::remove_file(&path).unwrap();
    assert_eq!(
        call(&ground.0, "central.files.create", input).status,
        ResultStatus::Success
    );
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
}

#[test]
fn concurrent_first_saves_admit_one_without_last_writer_wins() {
    let ground = Ground::new();
    let input = create(&ground.0);
    let other = input.clone();
    let root = ground.0.clone();
    let thread = std::thread::spawn(move || call(&root, "central.files.create", other));
    let mut competing = input;
    competing["content"] = json!("another composition");
    competing["operation_ref"] = json!("return:another");
    let two = call(&ground.0, "central.files.create", competing);
    let one = thread.join().unwrap();
    assert_ne!(
        one.status == ResultStatus::Success,
        two.status == ResultStatus::Success
    );
    let content = fs::read_to_string(ground.0.join("notes/inquiry.expression.json")).unwrap();
    assert!(content == "another composition" || content.contains("expression:inquiry"));
}
