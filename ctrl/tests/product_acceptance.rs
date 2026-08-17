use central_ctrl::{initialize_central, search_control};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn temporary_directory(label: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "central-product-acceptance-{label}-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir_all(&path).unwrap();
    path
}

#[test]
fn not_agent_readable_control_subtree_is_excluded_from_stock_retrieval() {
    let root = temporary_directory("retrieval").join("Central");
    initialize_central(&root).unwrap();

    fs::write(root.join("Control/user/ordinary.md"), "shared needle\n").unwrap();
    let restricted = root.join("Control/user/private-context");
    fs::create_dir_all(&restricted).unwrap();
    fs::write(restricted.join(".no-agent-retrieval"), "").unwrap();
    fs::write(restricted.join("notes.md"), "private needle\n").unwrap();
    let nested = restricted.join("nested");
    fs::create_dir_all(&nested).unwrap();
    fs::write(nested.join("more.md"), "nested private needle\n").unwrap();

    let result = search_control(&root, "needle").unwrap();
    assert_eq!(result.matches.len(), 1);
    assert_eq!(result.matches[0].source_path, PathBuf::from("Control/user/ordinary.md"));
    assert_eq!(result.files_scanned, 1);
    assert!(result.skipped_sources.iter().any(|source| {
        source.source_path == PathBuf::from("Control/user/private-context")
            && source.reason == "not_agent_readable"
    }));

    assert_eq!(
        fs::read_to_string(restricted.join("notes.md")).unwrap(),
        "private needle\n"
    );
}

#[test]
fn deleting_rebuildable_local_state_preserves_authored_control_and_work() {
    let root = temporary_directory("derived-state").join("Central");
    initialize_central(&root).unwrap();

    fs::write(root.join("Control/user/preference.md"), "durable authored source\n").unwrap();
    fs::create_dir_all(root.join("Work/project-a")).unwrap();
    fs::write(root.join("Work/project-a/README.md"), "ordinary work\n").unwrap();
    fs::write(root.join(".central/cache.json"), "{\"derived\":true}\n").unwrap();

    fs::remove_dir_all(root.join(".central")).unwrap();
    assert_eq!(
        fs::read_to_string(root.join("Control/user/preference.md")).unwrap(),
        "durable authored source\n"
    );
    assert_eq!(
        fs::read_to_string(root.join("Work/project-a/README.md")).unwrap(),
        "ordinary work\n"
    );

    initialize_central(&root).unwrap();
    assert!(root.join(".central").is_dir());
    assert_eq!(
        fs::read_to_string(root.join("Control/user/preference.md")).unwrap(),
        "durable authored source\n"
    );
}
