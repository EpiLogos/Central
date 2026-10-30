//! A world's Collections (`central.collections.read`, `.add`, `.remove`; O-I #749; DLW manual §10.3, "Down the browser's left side
//! stand Live's three groups: Collections, the coloured favourites you make"). Run through the real `ctrl`, as the O-I kernel invokes
//! it: your own world keeps its collections in `Control/relations/collections.json`, a project's world in its
//! `ProjectCentral/relations/collections.json`; Live's seven coloured collections, each holding the things you put in it by their
//! refs, added and removed one at a time. Disposable grounds.
use serde_json::{json, Value};
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

const PROJECT: &str = "field-notes";
static NEXT: AtomicU64 = AtomicU64::new(0);

struct Home(PathBuf);
impl Drop for Home {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
impl Home {
    fn ground(&self) -> PathBuf {
        self.0.join("Central")
    }
}

/// A ground with one project.
fn made() -> Home {
    let path = std::env::temp_dir().join(format!(
        "central-collections-{}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&path).unwrap();
    let home = Home(path.canonicalize().unwrap());
    let root = home.ground();
    central_ctrl::initialize_central(&root).unwrap();
    fs::create_dir_all(root.join("Work").join(PROJECT)).unwrap();
    fs::write(
        root.join("Work").join(PROJECT).join("README.md"),
        "# notes\n",
    )
    .unwrap();
    central_ctrl::projectcentral_ops::initialize_projectcentral(
        &root,
        &root.join("Work").join(PROJECT),
        PROJECT,
    )
    .unwrap();
    home
}

fn cli(home: &Home, action: &str, input: Value) -> Value {
    let output = Command::new(env!("CARGO_BIN_EXE_ctrl"))
        .args(["--json", "--root"])
        .arg(home.ground())
        .args(["action", "run", action, &input.to_string()])
        .env("HOME", &home.0)
        .env_remove("CENTRAL_ROOT")
        .env_remove("CENTRAL_NATIVE_TOKEN")
        .current_dir(&home.0)
        .output()
        .unwrap();
    serde_json::from_slice(&output.stdout).unwrap_or_else(|_| {
        panic!(
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

fn items(reading: &Value, collection: &str) -> Vec<String> {
    reading["collections"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["id"] == collection)
        .unwrap_or_else(|| panic!("no {collection} in {reading}"))["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["ref"].as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn your_world_keeps_its_coloured_collections_and_each_holds_what_you_put_in_it() {
    let home = made();
    // Before anything is kept: Live's seven collections, Favourites first, each empty, and no file yet.
    let read = cli(
        &home,
        "central.collections.read",
        json!({"world": "control:root"}),
    );
    assert_eq!(read["ok"], true, "{read}");
    let data = &read["data"];
    assert_eq!(data["schema"], "central.collections/v1");
    assert_eq!(data["world"], "control:root");
    assert_eq!(data["kept"], false);
    let ids: Vec<&str> = data["collections"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["id"].as_str().unwrap())
        .collect();
    assert_eq!(
        ids,
        [
            "favourites",
            "orange",
            "yellow",
            "green",
            "blue",
            "purple",
            "grey"
        ]
    );
    assert_eq!(data["collections"][0]["name"], "Favourites");
    assert!(items(data, "favourites").is_empty());
    assert!(!home
        .ground()
        .join("Control/relations/collections.json")
        .exists());

    let agent = json!({"world": "control:root", "collection": "favourites",
        "item": {"ref": "agent/central-guardian", "label": "Central guardian", "kind": "agent"}});
    let added = cli(&home, "central.collections.add", agent.clone());
    assert_eq!(added["ok"], true, "{added}");
    assert_eq!(added["data"]["outcome"], "added");
    assert_eq!(
        added["data"]["source_path"],
        "Control/relations/collections.json"
    );
    assert_eq!(
        items(&added["data"]["reading"], "favourites"),
        ["agent/central-guardian"]
    );
    // The same thing put in the same collection again: unchanged, never twice.
    let again = cli(&home, "central.collections.add", agent);
    assert_eq!(again["data"]["outcome"], "unchanged", "{again}");
    let project = cli(
        &home,
        "central.collections.add",
        json!({"world": "control:root", "collection": "blue",
            "item": {"ref": format!("project:{PROJECT}"), "label": PROJECT, "kind": "project"}}),
    );
    assert_eq!(project["data"]["outcome"], "added", "{project}");

    let kept: Value = serde_json::from_slice(
        &fs::read(home.ground().join("Control/relations/collections.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(kept["schema"], "central.collections/v1");
    let read = cli(
        &home,
        "central.collections.read",
        json!({"world": "control:root"}),
    );
    assert_eq!(read["data"]["kept"], true);
    assert_eq!(
        items(&read["data"], "favourites"),
        ["agent/central-guardian"]
    );
    assert_eq!(items(&read["data"], "blue"), [format!("project:{PROJECT}")]);
    // The reading's revision is the kept record's: the last act's receipt names the same one.
    assert!(read["data"]["revision"].is_string());
    assert_eq!(
        read["data"]["revision"],
        project["data"]["reading"]["revision"]
    );

    let removed = cli(
        &home,
        "central.collections.remove",
        json!({"world": "control:root", "collection": "favourites", "ref": "agent/central-guardian"}),
    );
    assert_eq!(removed["ok"], true, "{removed}");
    assert_eq!(removed["data"]["outcome"], "removed");
    assert!(items(&removed["data"]["reading"], "favourites").is_empty());
    let gone = cli(
        &home,
        "central.collections.remove",
        json!({"world": "control:root", "collection": "favourites", "ref": "agent/central-guardian"}),
    );
    assert_eq!(gone["data"]["outcome"], "unchanged", "{gone}");
}

#[test]
fn a_project_s_world_keeps_its_own_and_what_is_no_collection_or_no_world_is_refused() {
    let home = made();
    let world = format!("project:{PROJECT}");
    let added = cli(
        &home,
        "central.collections.add",
        json!({"world": world, "collection": "green", "item": {"ref": "Control/user/day/2026-10-30/day.md", "label": "Fri 30 Oct"}}),
    );
    assert_eq!(added["ok"], true, "{added}");
    assert_eq!(
        added["data"]["source_path"],
        format!("Work/{PROJECT}/ProjectCentral/relations/collections.json")
    );
    assert!(home
        .ground()
        .join("Work")
        .join(PROJECT)
        .join("ProjectCentral/relations/collections.json")
        .is_file());
    // Your own world's collections are not the project's.
    let yours = cli(
        &home,
        "central.collections.read",
        json!({"world": "control:root"}),
    );
    assert!(items(&yours["data"], "green").is_empty());

    for (action, input) in [
        (
            "central.collections.add",
            json!({"world": "project:absent", "collection": "green", "item": {"ref": "x", "label": "x"}}),
        ),
        (
            "central.collections.add",
            json!({"world": "control:root", "collection": "crimson", "item": {"ref": "x", "label": "x"}}),
        ),
        (
            "central.collections.add",
            json!({"world": "control:root", "collection": "green", "item": {"ref": "", "label": "x"}}),
        ),
        (
            "central.collections.add",
            json!({"world": "control:root", "collection": "green", "item": {"ref": "x", "label": "two\nlines"}}),
        ),
        (
            "central.collections.read",
            json!({"world": "world:elsewhere"}),
        ),
        (
            "central.collections.remove",
            json!({"world": "control:root", "collection": "green"}),
        ),
    ] {
        let refused = cli(&home, action, input.clone());
        assert_eq!(refused["ok"], false, "{action} {input}: {refused}");
    }
    assert!(!home
        .ground()
        .join("Control/relations/collections.json")
        .exists());
}

/// The name a reading gives one collection, and whether the person chose to show it.
fn named(reading: &Value, collection: &str) -> (String, Value) {
    let entry = reading["collections"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["id"] == collection)
        .unwrap_or_else(|| panic!("no {collection} in {reading}"));
    (
        entry["name"].as_str().unwrap().to_owned(),
        entry["shown"].clone(),
    )
}

/// Live's Rename and Edit (O-I #749 follow-up): a collection takes the name you give it, and you choose whether the browser's column
/// shows it. Both are kept beside what the collections hold, and neither touches it.
#[test]
fn a_collection_is_renamed_and_shown_or_hidden_as_you_choose_and_keeps_what_it_holds() {
    let home = made();
    let world = json!("control:root");
    let added = cli(
        &home,
        "central.collections.add",
        json!({"world": world, "collection": "green", "item": {"ref": "workcell:mac", "label": "This machine", "kind": "workcell"}}),
    );
    assert_eq!(added["ok"], true, "{added}");
    // Nothing chosen yet: Live's names, and no choice about showing.
    let read = cli(&home, "central.collections.read", json!({"world": world}));
    assert_eq!(
        named(&read["data"], "green"),
        ("Green".to_owned(), Value::Null)
    );
    // A record that holds only things is the record it was: no names, no choices.
    let kept: Value = serde_json::from_slice(
        &fs::read(home.ground().join("Control/relations/collections.json")).unwrap(),
    )
    .unwrap();
    let mut keys: Vec<&str> = kept
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(keys, ["collections", "schema", "world"]);

    let renamed = cli(
        &home,
        "central.collections.rename",
        json!({"world": world, "collection": "green", "name": "  Machines I use  "}),
    );
    assert_eq!(renamed["ok"], true, "{renamed}");
    assert_eq!(renamed["data"]["outcome"], "renamed");
    assert_eq!(renamed["data"]["name"], "Machines I use");
    assert_eq!(
        named(&renamed["data"]["reading"], "green").0,
        "Machines I use"
    );
    assert_eq!(
        items(&renamed["data"]["reading"], "green"),
        ["workcell:mac"],
        "a rename keeps what it holds"
    );
    let again = cli(
        &home,
        "central.collections.rename",
        json!({"world": world, "collection": "green", "name": "Machines I use"}),
    );
    assert_eq!(again["data"]["outcome"], "unchanged", "{again}");
    assert_eq!(
        again["data"]["reading"]["revision"],
        renamed["data"]["reading"]["revision"]
    );

    let hidden = cli(
        &home,
        "central.collections.show",
        json!({"world": world, "collection": "favourites", "shown": false}),
    );
    assert_eq!(hidden["ok"], true, "{hidden}");
    assert_eq!(hidden["data"]["outcome"], "hidden");
    assert_eq!(
        named(&hidden["data"]["reading"], "favourites"),
        ("Favourites".to_owned(), json!(false))
    );
    let shown = cli(
        &home,
        "central.collections.show",
        json!({"world": world, "collection": "blue", "shown": true}),
    );
    assert_eq!(shown["data"]["outcome"], "shown", "{shown}");
    assert_eq!(named(&shown["data"]["reading"], "blue").1, json!(true));
    let same = cli(
        &home,
        "central.collections.show",
        json!({"world": world, "collection": "blue", "shown": true}),
    );
    assert_eq!(same["data"]["outcome"], "unchanged", "{same}");

    // What was chosen is kept, read again from the record.
    let read = cli(&home, "central.collections.read", json!({"world": world}));
    assert_eq!(
        named(&read["data"], "green"),
        ("Machines I use".to_owned(), Value::Null)
    );
    assert_eq!(named(&read["data"], "favourites").1, json!(false));
    assert_eq!(items(&read["data"], "green"), ["workcell:mac"]);
    // The name Live gives it, or no name at all, gives it back Live's name; the record keeps no name for it.
    for back in ["Green", ""] {
        cli(
            &home,
            "central.collections.rename",
            json!({"world": world, "collection": "green", "name": "Mine"}),
        );
        let restored = cli(
            &home,
            "central.collections.rename",
            json!({"world": world, "collection": "green", "name": back}),
        );
        assert_eq!(restored["data"]["outcome"], "renamed", "{restored}");
        assert_eq!(named(&restored["data"]["reading"], "green").0, "Green");
    }
    let kept: Value = serde_json::from_slice(
        &fs::read(home.ground().join("Control/relations/collections.json")).unwrap(),
    )
    .unwrap();
    assert!(kept.get("names").is_none(), "{kept}");
    assert_eq!(kept["shown"], json!({"blue": true, "favourites": false}));

    for input in [
        json!({"world": world, "collection": "crimson", "name": "x"}),
        json!({"world": world, "collection": "green", "name": "two\nlines"}),
        json!({"world": world, "collection": "green", "name": "x".repeat(65)}),
        json!({"world": world, "collection": "green"}),
    ] {
        let refused = cli(&home, "central.collections.rename", input.clone());
        assert_eq!(refused["ok"], false, "{input}: {refused}");
    }
    for input in [
        json!({"world": world, "collection": "green", "shown": "no"}),
        json!({"world": world, "collection": "green"}),
        json!({"world": "project:absent", "collection": "green", "shown": false}),
    ] {
        let refused = cli(&home, "central.collections.show", input.clone());
        assert_eq!(refused["ok"], false, "{input}: {refused}");
    }
}
