//! A passage filed on a place of your world (`central.place.file-passage`, O-I #820; DLW manual §18.4 What a tag sends, "Place —
//! Files the passage on that spot of your world, with a link back to the page"). Run through the real `ctrl`, as the O-I kernel
//! invokes it: your own world keeps the passage under `Control/agents/places/filed`, a project's world under its
//! `ProjectCentral/agents/places/filed`, word for word, with the link back, as a generated proposal that stays unrecognised.
//! Disposable grounds.
use serde_json::{json, Value};
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

const PROJECT: &str = "field-notes";
const ACTION: &str = "central.place.file-passage";
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
        "central-place-passage-{}-{}-{}",
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

fn cli(home: &Home, input: Value) -> Value {
    let output = Command::new(env!("CARGO_BIN_EXE_ctrl"))
        .args(["--json", "--root"])
        .arg(home.ground())
        .args(["action", "run", ACTION, &input.to_string()])
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

fn sent(place: &str) -> Value {
    json!({"place": place, "passage": "Tag a place and the passage is filed on that spot of your world.",
        "source": {"title": "Flow — Walk notes", "path": "Control/user/flows/walk-notes.html"}})
}

#[test]
fn a_passage_is_filed_on_a_project_s_world_and_on_your_own_through_ctrl() {
    let home = made();
    let filed = cli(&home, sent(&format!("project:{PROJECT}")));
    assert_eq!(filed["ok"], true, "{filed}");
    let data = &filed["data"];
    assert_eq!(data["outcome"], "filed", "{data}");
    assert_eq!(data["place"], format!("project:{PROJECT}"));
    let path = data["source_path"].as_str().unwrap();
    assert!(
        path.starts_with(&format!(
            "Work/{PROJECT}/ProjectCentral/agents/places/filed/passage-"
        )),
        "{path}"
    );
    let kept: Value = serde_json::from_slice(&fs::read(home.ground().join(path)).unwrap()).unwrap();
    assert_eq!(kept["schema"], "central.filed-passage/v1");
    assert_eq!(
        kept["passage"],
        "Tag a place and the passage is filed on that spot of your world."
    );
    assert_eq!(kept["source"]["path"], "Control/user/flows/walk-notes.html");
    assert_eq!(kept["provenance"]["recognition"], "unrecognised");
    assert_eq!(data["location"]["path"], path);
    let again = cli(&home, sent(&format!("project:{PROJECT}")));
    assert_eq!(again["data"]["outcome"], "unchanged", "{again}");
    let root = cli(&home, sent("control:root"));
    assert_eq!(root["data"]["outcome"], "filed", "{root}");
    assert!(root["data"]["source_path"]
        .as_str()
        .unwrap()
        .starts_with("Control/agents/places/filed/passage-"));
    let refused = cli(&home, sent("project:absent"));
    assert_eq!(refused["ok"], false, "{refused}");
}
