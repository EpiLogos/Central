//! `central.files.reach`: what a reader of Central's ground may not reach, and why — the reason the admission finds, kept instead of
//! reduced to `retrieval_allowed`'s bool. A marker on a folder withholds it and everything under it; a link is refused as redirected;
//! a path outside the ground or one that climbs is no member; the walk under a folder lists the withheld places without opening the
//! tool-state folders. The listing carries the same reason on each entry it withholds.
use central_ctrl::{
    create_core_action_registry, initialize_central, ActionExecutionContext, ConnectorContext,
    ConnectorRegistry, RootOptions,
};
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static NEXT_TEMP_ROOT: AtomicU64 = AtomicU64::new(0);

/// A scratch ground under the system's temporary directory, removed when the test ends.
struct TempRoot(PathBuf);

impl TempRoot {
    fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let sequence = NEXT_TEMP_ROOT.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "central-files-reach-{}-{nonce}-{sequence}",
            std::process::id()
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path.canonicalize().unwrap())
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

/// A ground with a project holding an ordinary file, a withheld folder, a link, and a marker inside a tool-state folder.
fn ground() -> TempRoot {
    let temp = TempRoot::new();
    let root = &temp.0;
    initialize_central(root).unwrap();
    fs::create_dir_all(root.join("Work/P/src")).unwrap();
    fs::write(root.join("Work/P/src/a.rs"), "fn a() {}\n").unwrap();
    fs::create_dir_all(root.join("Work/P/private")).unwrap();
    fs::write(root.join("Work/P/private/secret.md"), "secret\n").unwrap();
    fs::write(root.join("Work/P/private/.no-agent-retrieval"), "").unwrap();
    std::os::unix::fs::symlink(root.join("Work/P/src"), root.join("Work/P/link")).unwrap();
    fs::create_dir_all(root.join("Work/P/node_modules/x")).unwrap();
    fs::write(root.join("Work/P/node_modules/x/.no-agent-retrieval"), "").unwrap();
    temp
}

fn place<'a>(answer: &'a Value, path: &str) -> &'a Value {
    answer["places"]
        .as_array()
        .unwrap()
        .iter()
        .find(|place| place["path"] == path)
        .unwrap_or_else(|| panic!("{path} is answered: {answer}"))
}

#[test]
fn reach_names_each_withheld_place_with_its_reason_and_where_it_comes_from() {
    let temp = ground();
    let root = &temp.0;
    let file = root.join("Work/P/src/a.rs").to_string_lossy().into_owned();
    let result = execute(
        root,
        "central.files.reach",
        json!({
            "paths": [file, "Work/P/private/secret.md", "Work/P/private", "Work/P/link/a.rs", "/etc/hosts", "Work/P/../P/src/a.rs"],
            "agent_session_ref": "agent-session/reader",
        }),
    );
    assert!(result.ok, "{result:?}");
    let answer = result.data.unwrap();
    assert_eq!(answer["schema"], "central.files-reach/v1");
    assert_eq!(
        answer["reader"]["agent_session_ref"],
        "agent-session/reader"
    );
    assert_eq!(
        answer["same_for_every_agent"], true,
        "Central's treatment is the world's"
    );

    let open = place(&answer, &file);
    assert_eq!(open["reachable"], true);
    assert_eq!(open["relative"], "Work/P/src/a.rs");
    assert!(open.get("withheld").is_none(), "{open}");

    let below = place(&answer, "Work/P/private/secret.md");
    assert_eq!(below["reachable"], false);
    assert_eq!(below["withheld"]["code"], "deny-marker");
    assert_eq!(
        below["withheld"]["at"], "Work/P/private",
        "the folder whose marker withholds it"
    );
    assert!(
        below["withheld"]["words"]
            .as_str()
            .unwrap()
            .contains("everything under it"),
        "{below}"
    );

    let folder = place(&answer, "Work/P/private");
    assert_eq!(folder["withheld"]["code"], "deny-marker");
    assert_eq!(folder["withheld"]["at"], "Work/P/private");
    assert!(
        folder["withheld"]["words"]
            .as_str()
            .unwrap()
            .contains("withholds it from agent retrieval"),
        "{folder}"
    );

    let linked = place(&answer, "Work/P/link/a.rs");
    assert_eq!(linked["withheld"]["code"], "redirected");
    assert_eq!(
        linked["withheld"]["at"], "Work/P/link",
        "the link on the way"
    );

    let outside = place(&answer, "/etc/hosts");
    assert_eq!(outside["reachable"], false);
    assert_eq!(outside["withheld"]["code"], "outside");
    assert!(
        outside["withheld"].get("at").is_none(),
        "nothing inside the ground withholds it"
    );

    let climbing = place(&answer, "Work/P/../P/src/a.rs");
    assert_eq!(climbing["withheld"]["code"], "not-a-member");
}

#[test]
fn reach_under_a_folder_lists_its_withheld_places_and_leaves_tool_state_unopened() {
    let temp = ground();
    let root = &temp.0;
    let result = execute(root, "central.files.reach", json!({"under": "Work/P"}));
    assert!(result.ok, "{result:?}");
    let under = &result.data.unwrap()["under"];
    assert_eq!(under["folder"], "Work/P");
    assert_eq!(under["complete"], true);
    let found: Vec<(String, String)> = under["withheld"]
        .as_array()
        .unwrap()
        .iter()
        .map(|w| {
            (
                w["at"].as_str().unwrap().to_owned(),
                w["code"].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    assert_eq!(
        found,
        vec![
            ("Work/P/link".to_owned(), "redirected".to_owned()),
            ("Work/P/private".to_owned(), "deny-marker".to_owned()),
        ],
        "the link and the marked folder; node_modules is not opened"
    );

    // A folder that is itself withheld answers with that alone.
    let withheld = execute(
        root,
        "central.files.reach",
        json!({"under": "Work/P/private"}),
    );
    let under = &withheld.data.unwrap()["under"];
    assert_eq!(under["withheld"].as_array().unwrap().len(), 1);
    assert_eq!(under["withheld"][0]["code"], "deny-marker");

    // Nothing to judge is a refusal in Central's words.
    let empty = execute(root, "central.files.reach", json!({}));
    assert!(!empty.ok);
    assert!(format!("{empty:?}").contains("needs paths to judge"));
}

#[test]
fn a_listing_carries_the_reason_beside_retrieval_allowed() {
    let temp = ground();
    let root = &temp.0;
    let listing = central_ctrl::files::list_files(root, "Work/P").unwrap();
    let entry = |name: &str| {
        listing
            .entries
            .iter()
            .find(|entry| entry.name == name)
            .unwrap_or_else(|| panic!("{name} is listed"))
    };
    assert!(entry("src").retrieval_allowed);
    assert!(entry("src").retrieval_withheld.is_none());
    let private = entry("private");
    assert!(!private.retrieval_allowed);
    let reason = private.retrieval_withheld.as_ref().unwrap();
    assert_eq!(reason.code, "deny-marker");
    assert_eq!(reason.at.as_deref(), Some("Work/P/private"));
    let link = entry("link");
    assert!(!link.retrieval_allowed);
    assert_eq!(link.retrieval_withheld.as_ref().unwrap().code, "redirected");
    let value = serde_json::to_value(entry("src")).unwrap();
    assert!(
        value.get("retrieval_withheld").is_none(),
        "an admitted entry carries no reason: {value}"
    );
}
