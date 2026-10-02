use serde_json::{json, Value};
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../ProjectCentral/now/tmp")
            .join(format!("machine-compat-{}", uuid::Uuid::new_v4()));
        central_ctrl::initialize_central(&root).unwrap();
        Self(root)
    }
    fn adopt(&self, reference: &str) -> Output {
        Command::new(env!("CARGO_BIN_EXE_central-machine-adopt"))
            .args([
                "--root",
                self.0.to_str().unwrap(),
                "--workcell-ref",
                reference,
                "--json",
            ])
            .output()
            .unwrap()
    }
    fn declaration(&self) -> PathBuf {
        self.0.join("Control/machines/current.json")
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn compatibility_command_uses_native_owner_for_creation_noop_and_conflict() {
    let fixture = Fixture::new();
    let first = fixture.adopt("workcell:compatibility");
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let result: Value = serde_json::from_slice(&first.stdout).unwrap();
    assert_eq!(result["schema"], "central.machine-adoption/v1");
    assert_eq!(result["outcome"], "created");
    assert_eq!(result["source"]["source_class"], "authored");
    assert_eq!(
        result["declaration"]["bindings"],
        json!([{"kind":"workcell","reference":"workcell:compatibility"}])
    );
    assert!(result["observed"]["observation"]["platform"].is_string());

    // A genuine authored extension is outside the typed machine schema. Both
    // the retained source and compatibility result must keep it unchanged.
    let mut authored: Value =
        serde_json::from_slice(&fs::read(fixture.declaration()).unwrap()).unwrap();
    authored["owner_extension"] = json!({"private": "retain this field"});
    authored["bindings"] = json!([]);
    fs::write(
        fixture.declaration(),
        serde_json::to_vec(&authored).unwrap(),
    )
    .unwrap();
    let bound = fixture.adopt("workcell:compatibility");
    assert!(bound.status.success());
    let bound: Value = serde_json::from_slice(&bound.stdout).unwrap();
    assert_eq!(bound["outcome"], "bound");
    assert_eq!(
        bound["declaration"]["owner_extension"],
        authored["owner_extension"]
    );
    authored["bindings"] = bound["declaration"]["bindings"].clone();
    let before = fs::read(fixture.declaration()).unwrap();
    let metadata = fs::metadata(fixture.declaration()).unwrap();
    let repeat = fixture.adopt("workcell:compatibility");
    assert!(repeat.status.success());
    let result: Value = serde_json::from_slice(&repeat.stdout).unwrap();
    assert_eq!(result["outcome"], "unchanged");
    assert_eq!(
        result["declaration"]["owner_extension"],
        authored["owner_extension"]
    );
    assert_eq!(fs::read(fixture.declaration()).unwrap(), before);
    assert_eq!(
        fs::metadata(fixture.declaration())
            .unwrap()
            .modified()
            .unwrap(),
        metadata.modified().unwrap()
    );

    let conflict = fixture.adopt("workcell:another");
    assert_eq!(conflict.status.code(), Some(2));
    let conflict: Value = serde_json::from_slice(&conflict.stdout).unwrap();
    assert_eq!(conflict["ok"], false);
    assert_eq!(conflict["action"], "machine.adopt-current");
    assert_eq!(
        conflict["error"]["details"]["code"],
        "workcell_binding_conflict"
    );
    assert_eq!(
        conflict["error"]["details"]["requested_workcell_ref"],
        "workcell:another"
    );
    assert_eq!(fs::read(fixture.declaration()).unwrap(), before);

    let canonical_conflict = Command::new(env!("CARGO_BIN_EXE_ctrl"))
        .args([
            "--root",
            fixture.0.to_str().unwrap(),
            "--json",
            "action",
            "run",
            "machine.adopt-current",
            r#"{"workcell_ref":"workcell:another"}"#,
        ])
        .output()
        .unwrap();
    assert_eq!(canonical_conflict.status.code(), Some(2));
    let canonical_conflict: Value = serde_json::from_slice(&canonical_conflict.stdout).unwrap();
    assert_eq!(
        conflict, canonical_conflict,
        "compatibility must preserve the complete native failure envelope"
    );

    // Inspect through the canonical public operation using the real host's
    // default connectors, rather than a substituted machine inspector.
    let read = Command::new(env!("CARGO_BIN_EXE_ctrl"))
        .args([
            "--root",
            fixture.0.to_str().unwrap(),
            "--json",
            "action",
            "run",
            "machine.declaration",
            "{}",
        ])
        .output()
        .unwrap();
    assert!(read.status.success());
    let read: Value = serde_json::from_slice(&read.stdout).unwrap();
    assert_eq!(
        read["data"]["declaration"]["bindings"],
        authored["bindings"]
    );
}
