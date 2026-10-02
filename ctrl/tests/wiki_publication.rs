use central_ctrl::projectcentral_ops::ensure_root_federation;
use serde_json::{json, Value};
use std::fs;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Barrier};

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../ProjectCentral/now/tmp")
            .join(format!("wiki-publication-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
    fn root(&self) -> &Path {
        &self.0
    }
    fn wiki(&self) -> PathBuf {
        self.0.join("Control/agents/wiki/wiki.json")
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn unchanged_initialization_preserves_exact_bytes_inode_and_metadata() {
    let fixture = Fixture::new();
    ensure_root_federation(fixture.root(), None).unwrap();
    let wiki = fixture.wiki();
    let mut value: Value = serde_json::from_slice(&fs::read(&wiki).unwrap()).unwrap();
    value["owner_extension"] = json!({"source": "retained"});
    value["objects"][0]["object_extension"] = json!({"private": true});
    let bytes = serde_json::to_vec(&value).unwrap();
    fs::write(&wiki, &bytes).unwrap();
    fs::set_permissions(&wiki, fs::Permissions::from_mode(0o640)).unwrap();
    let before = fs::metadata(&wiki).unwrap();
    ensure_root_federation(fixture.root(), None).unwrap();
    let after = fs::metadata(&wiki).unwrap();
    assert_eq!(fs::read(&wiki).unwrap(), bytes);
    assert_eq!(
        (after.ino(), after.mtime(), after.mtime_nsec()),
        (before.ino(), before.mtime(), before.mtime_nsec())
    );
    assert_eq!(
        (after.mode(), after.uid(), after.gid()),
        (before.mode(), before.uid(), before.gid())
    );
}

#[test]
fn actual_cli_later_federation_failure_retains_every_completed_project_source() {
    use std::process::Command;
    for action in [
        "projectcentral.init",
        "projectcentral.adopt",
        "projectcentral.migrate",
    ] {
        let fixture = Fixture::new();
        central_ctrl::initialize_central(fixture.root()).unwrap();
        let project = fixture.root().join("Work/example");
        fs::create_dir_all(project.join("legacy")).unwrap();
        let selected = project.join("legacy/wiki.json");
        let selected_bytes = serde_json::to_vec(&json!({"owner_extension":{"retain":true},"objects":[{
            "profile":"okf-wiki/v1","object":"space","ref":"wiki:space:selected",
            "revision":1,"provenance":[],"parent_space_refs":[],"child_space_refs":[],"node_refs":[],
            "owner_extension":{"retain":true}
        }]})).unwrap();
        if action != "projectcentral.init" {
            fs::write(&selected, &selected_bytes).unwrap();
        }
        let malformed = b"{genuine malformed root source";
        fs::write(fixture.wiki(), malformed).unwrap();
        let request =
            json!({"project":"example","project_id":"example/project","source":"legacy/wiki.json"});
        let output = Command::new(env!("CARGO_BIN_EXE_ctrl"))
            .args([
                "--root",
                fixture.root().to_str().unwrap(),
                "--json",
                "action",
                "run",
                action,
                &request.to_string(),
            ])
            .output()
            .unwrap();
        assert_eq!(
            output.status.code(),
            Some(6),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let result: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(result["ok"], false);
        assert_eq!(result["action"], action);
        assert_eq!(result["error"]["code"], "central.mutation_incomplete");
        let details = &result["error"]["details"];
        assert_eq!(details["published"], true);
        assert_eq!(details["outcome"], "partial");
        assert_eq!(details["failed_source"], json!(fixture.wiki()));
        assert_eq!(details["cause"]["kind"], "InvalidData");
        assert!(details.get("operation_ref").is_none());
        let completed = details["completed_sources"].as_array().unwrap();
        assert_eq!(completed.len(), 2);
        for relative in [
            "ProjectCentral/project.json",
            "ProjectCentral/agents/wiki/wiki.json",
        ] {
            let path = project.join(relative);
            assert!(completed.contains(&json!(path)));
            let _: Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
        }
        assert_eq!(fs::read(fixture.wiki()).unwrap(), malformed);
        if action != "projectcentral.init" {
            assert_eq!(fs::read(&selected).unwrap(), selected_bytes);
        }
        let inspection = Command::new(env!("CARGO_BIN_EXE_ctrl"))
            .args([
                "--root",
                fixture.root().to_str().unwrap(),
                "--json",
                "action",
                "run",
                "projectcentral.inspect",
                r#"{"project":"example"}"#,
            ])
            .output()
            .unwrap();
        assert!(inspection.status.success());
    }
}

#[test]
fn parallel_federation_retains_every_relation_and_unknown_field() {
    let fixture = Fixture::new();
    ensure_root_federation(fixture.root(), None).unwrap();
    let mut value: Value = serde_json::from_slice(&fs::read(fixture.wiki()).unwrap()).unwrap();
    value["owner_extension"] = json!({"retained": "x".repeat(1024 * 1024)});
    value["objects"][0]["object_extension"] = json!({"retained": true});
    fs::write(fixture.wiki(), serde_json::to_vec(&value).unwrap()).unwrap();
    let barrier = Arc::new(Barrier::new(12));
    let workers: Vec<_> = (0..12)
        .map(|index| {
            let barrier = barrier.clone();
            let root = fixture.0.clone();
            std::thread::spawn(move || {
                barrier.wait();
                ensure_root_federation(&root, Some(&format!("central:wiki:project:lane-{index}")))
            })
        })
        .collect();
    for worker in workers {
        worker.join().unwrap().unwrap();
    }
    let final_value: Value = serde_json::from_slice(&fs::read(fixture.wiki()).unwrap()).unwrap();
    assert_eq!(final_value["owner_extension"], value["owner_extension"]);
    assert_eq!(
        final_value["objects"][0]["object_extension"],
        value["objects"][0]["object_extension"]
    );
    assert_eq!(final_value["objects"][0]["revision"], 13);
    let children = final_value["objects"][0]["child_space_refs"]
        .as_array()
        .unwrap();
    for index in 0..12 {
        assert!(children.contains(&json!(format!("central:wiki:project:lane-{index}"))));
    }
    let before = fs::read(fixture.wiki()).unwrap();
    let metadata = fs::metadata(fixture.wiki()).unwrap();
    ensure_root_federation(fixture.root(), Some("central:wiki:project:lane-0")).unwrap();
    assert_eq!(before, fs::read(fixture.wiki()).unwrap());
    assert_eq!(metadata.ino(), fs::metadata(fixture.wiki()).unwrap().ino());
}

#[test]
fn source_links_and_readonly_mutation_are_refused_without_changing_source() {
    let fixture = Fixture::new();
    ensure_root_federation(fixture.root(), None).unwrap();
    let wiki = fixture.wiki();
    let bytes = fs::read(&wiki).unwrap();
    fs::set_permissions(&wiki, fs::Permissions::from_mode(0o400)).unwrap();
    assert!(ensure_root_federation(fixture.root(), Some("central:wiki:project:new")).is_err());
    assert_eq!(fs::read(&wiki).unwrap(), bytes);
    fs::set_permissions(&wiki, fs::Permissions::from_mode(0o600)).unwrap();
    let retained = wiki.with_file_name("retained.json");
    fs::rename(&wiki, &retained).unwrap();
    std::os::unix::fs::symlink(&retained, &wiki).unwrap();
    assert!(ensure_root_federation(fixture.root(), Some("central:wiki:project:new")).is_err());
    assert!(fs::symlink_metadata(&wiki)
        .unwrap()
        .file_type()
        .is_symlink());
    assert_eq!(fs::read(&retained).unwrap(), bytes);
    fs::remove_file(&wiki).unwrap();
    fs::hard_link(&retained, &wiki).unwrap();
    assert!(ensure_root_federation(fixture.root(), Some("central:wiki:project:new")).is_err());
    assert_eq!(fs::read(&retained).unwrap(), bytes);
}

#[test]
fn held_publication_lock_refuses_within_the_native_bound_then_recovers() {
    use std::os::unix::io::AsRawFd;
    let fixture = Fixture::new();
    ensure_root_federation(fixture.root(), None).unwrap();
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(fixture.wiki().with_file_name(".wiki.json.publication.lock"))
        .unwrap();
    assert_eq!(unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX) }, 0);
    let before = fs::read(fixture.wiki()).unwrap();
    let started = std::time::Instant::now();
    let error =
        ensure_root_federation(fixture.root(), Some("central:wiki:project:new")).unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::TimedOut);
    assert!(started.elapsed() < std::time::Duration::from_secs(7));
    assert_eq!(fs::read(fixture.wiki()).unwrap(), before);
    drop(lock);
    ensure_root_federation(fixture.root(), Some("central:wiki:project:new")).unwrap();
}

#[test]
fn canonical_parent_aliases_share_publication_and_legacy_staging_is_retained() {
    let fixture = Fixture::new();
    ensure_root_federation(fixture.root(), None).unwrap();
    let wiki = fixture.wiki();
    let legacy = wiki.with_file_name("wiki.json.aikit-tmp");
    let legacy_lock = wiki.with_file_name(".wiki.json.construction.lock");
    fs::write(&legacy, b"legacy evidence, never a source").unwrap();
    fs::write(&legacy_lock, b"retained old lock").unwrap();
    let alias = fixture.root().join("alias");
    std::os::unix::fs::symlink(fixture.root().join("Control"), &alias).unwrap();
    // The whole root alias also exercises canonical parent identity without
    // changing the Central structural operation's source address.
    let root_alias = fixture.root().join("root-alias");
    std::os::unix::fs::symlink(fixture.root(), &root_alias).unwrap();
    ensure_root_federation(&root_alias, Some("central:wiki:project:alias")).unwrap();
    ensure_root_federation(fixture.root(), Some("central:wiki:project:direct")).unwrap();
    let value: Value = serde_json::from_slice(&fs::read(&wiki).unwrap()).unwrap();
    assert_eq!(
        value["objects"][0]["child_space_refs"],
        json!(["central:wiki:project:alias", "central:wiki:project:direct"])
    );
    assert_eq!(
        fs::read(&legacy).unwrap(),
        b"legacy evidence, never a source"
    );
    assert_eq!(fs::read(&legacy_lock).unwrap(), b"retained old lock");
}

#[test]
fn migration_copies_exact_source_and_retains_mode_and_extension_fields() {
    let fixture = Fixture::new();
    ensure_root_federation(fixture.root(), None).unwrap();
    let project = fixture.root().join("Work/Legacy");
    fs::create_dir_all(&project).unwrap();
    let source = project.join("selected.json");
    let bytes = serde_json::to_vec(&json!({
        "owner_extension": {"private": true},
        "objects": [{"profile":"okf-wiki/v1","object":"space","ref":"wiki:space:legacy",
            "revision":1,"provenance":[],"parent_space_refs":[],"child_space_refs":[],
            "node_refs":[],"object_extension":{"retained":true}}]
    }))
    .unwrap();
    fs::write(&source, &bytes).unwrap();
    fs::set_permissions(&source, fs::Permissions::from_mode(0o640)).unwrap();
    central_ctrl::projectcentral_ops::migrate_selected(
        fixture.root(),
        &project,
        "legacy",
        "selected.json",
    )
    .unwrap();
    let target = project.join("ProjectCentral/agents/wiki/wiki.json");
    assert_eq!(fs::read(&source).unwrap(), bytes);
    assert_eq!(fs::read(&target).unwrap(), bytes);
    assert_eq!(fs::metadata(&target).unwrap().mode() & 0o7777, 0o640);
    let retained = fs::read(&target).unwrap();
    assert!(central_ctrl::projectcentral_ops::migrate_selected(
        fixture.root(),
        &project,
        "legacy",
        "selected.json"
    )
    .is_err());
    assert_eq!(fs::read(&target).unwrap(), retained);
}

#[cfg(target_os = "macos")]
#[test]
fn native_publication_preserves_actual_macos_acl_xattrs_and_mode() {
    use std::process::Command;
    let fixture = Fixture::new();
    ensure_root_federation(fixture.root(), None).unwrap();
    let wiki = fixture.wiki();
    fs::set_permissions(&wiki, fs::Permissions::from_mode(0o640)).unwrap();
    assert!(Command::new("/usr/bin/xattr")
        .args([
            "-w",
            "com.central.publication-test",
            "retained-private-metadata"
        ])
        .arg(&wiki)
        .status()
        .unwrap()
        .success());
    assert!(Command::new("/bin/chmod")
        .args(["+a", "everyone allow read"])
        .arg(&wiki)
        .status()
        .unwrap()
        .success());
    fn acl(path: &Path) -> Vec<String> {
        let output = Command::new("/bin/ls")
            .arg("-lde")
            .arg(path)
            .output()
            .unwrap();
        assert!(output.status.success());
        String::from_utf8(output.stdout)
            .unwrap()
            .lines()
            .skip(1)
            .map(str::to_owned)
            .collect()
    }
    let before = fs::metadata(&wiki).unwrap();
    let before_acl = acl(&wiki);
    assert!(!before_acl.is_empty());
    ensure_root_federation(fixture.root(), Some("central:wiki:project:metadata")).unwrap();
    let after = fs::metadata(&wiki).unwrap();
    assert_eq!(
        (after.uid(), after.gid(), after.mode()),
        (before.uid(), before.gid(), before.mode())
    );
    assert_eq!(acl(&wiki), before_acl);
    let attr = Command::new("/usr/bin/xattr")
        .args(["-p", "com.central.publication-test"])
        .arg(&wiki)
        .output()
        .unwrap();
    assert!(attr.status.success());
    assert_eq!(
        String::from_utf8(attr.stdout).unwrap().trim(),
        "retained-private-metadata"
    );
}

#[cfg(target_os = "linux")]
#[test]
fn federation_retains_real_linux_descriptor_xattr_bytes() {
    use std::{ffi::CString, fs::File, os::unix::io::AsRawFd};
    let fixture = Fixture::new();
    ensure_root_federation(fixture.root(), None).unwrap();
    let path = fixture.wiki();
    let attribute = CString::new("user.native-publication-test").unwrap();
    let value = b"retained\0source";
    let file = File::open(&path).unwrap();
    assert_eq!(
        unsafe {
            libc::fsetxattr(
                file.as_raw_fd(),
                attribute.as_ptr(),
                value.as_ptr().cast(),
                value.len(),
                0,
            )
        },
        0
    );
    drop(file);
    ensure_root_federation(fixture.root(), Some("central:wiki:project:metadata")).unwrap();
    let file = File::open(&path).unwrap();
    let mut retained = [0u8; 64];
    let read = unsafe {
        libc::fgetxattr(
            file.as_raw_fd(),
            attribute.as_ptr(),
            retained.as_mut_ptr().cast(),
            retained.len(),
        )
    };
    assert_eq!(read, value.len() as libc::ssize_t);
    assert_eq!(&retained[..read as usize], value);
}
