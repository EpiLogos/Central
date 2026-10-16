//! The Source Change Horizon's state file is rewritten only when what it
//! records changed. A reconciliation that finds the ground as recorded leaves
//! the file untouched, and every caller still receives the horizon of its own
//! reconciliation.

use central_ctrl::{
    initialize_central, initialize_projectcentral, read_project_change_horizon,
    reconcile_control_sources, reconcile_project_sources, CONTROL_HORIZON_STATE,
    PROJECT_HORIZON_STATE,
};
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Ground(PathBuf);
impl Drop for Ground {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn ground() -> Ground {
    let path = std::env::temp_dir().join(format!(
        "central-horizon-writes-{}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&path).unwrap();
    Ground(path)
}

/// The file's bytes and identity, its modification time included.
fn file(path: &Path) -> (Vec<u8>, u64, i64, i64) {
    let metadata = fs::metadata(path).unwrap();
    (
        fs::read(path).unwrap(),
        metadata.ino(),
        metadata.mtime(),
        metadata.mtime_nsec(),
    )
}

fn stored_reconciled_at(path: &Path) -> u64 {
    let state: serde_json::Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    state["reconciled_at_unix_seconds"].as_u64().unwrap()
}

/// Past one whole second, so a rewrite would carry a different time.
fn next_second() {
    std::thread::sleep(Duration::from_millis(1_100));
}

#[test]
fn a_change_free_project_reconciliation_leaves_the_state_file_untouched() {
    let ground = ground();
    let project = ground.0.join("Work/horizon");
    fs::create_dir_all(&project).unwrap();
    initialize_projectcentral(&ground.0, &project, "example/horizon").unwrap();
    let authored = project.join("ProjectCentral/user/position.md");
    fs::write(&authored, "alpha\n").unwrap();
    let state = project.join(PROJECT_HORIZON_STATE);

    let founded = reconcile_project_sources(&project).unwrap();
    assert!(founded.initialized, "a new horizon is written");
    let written = file(&state);
    let written_at = stored_reconciled_at(&state);

    next_second();
    let quiet = reconcile_project_sources(&project).unwrap();
    assert!(!quiet.initialized && quiet.new_changes.is_empty());
    assert_eq!(file(&state), written, "nothing changed, nothing written");
    assert!(
        quiet.horizon.reconciled_at_unix_seconds > written_at,
        "the report carries its own reconciliation's time"
    );
    assert_eq!(quiet.horizon.sources, founded.horizon.sources);
    assert_eq!(quiet.horizon.cursor, founded.horizon.cursor);

    next_second();
    let read = read_project_change_horizon(&project, None).unwrap();
    assert_eq!(
        file(&state),
        written,
        "a horizon read writes nothing either"
    );
    assert!(read.reconciled_at_unix_seconds > quiet.horizon.reconciled_at_unix_seconds);
    assert_eq!(read.sources, quiet.horizon.sources);
    assert_eq!(read.changes, quiet.horizon.changes);

    fs::write(&authored, "beta\n").unwrap();
    let changed = reconcile_project_sources(&project).unwrap();
    assert_eq!(changed.new_changes.len(), 1);
    assert_ne!(file(&state), written, "a recorded change is written");
    assert_eq!(
        stored_reconciled_at(&state),
        changed.horizon.reconciled_at_unix_seconds
    );
    let since = read_project_change_horizon(&project, Some(founded.horizon.cursor)).unwrap();
    assert_eq!(since.changes, changed.new_changes);
}

#[test]
fn a_change_free_control_reconciliation_leaves_the_state_file_untouched() {
    let ground = ground();
    initialize_central(&ground.0).unwrap();
    let state = ground.0.join(CONTROL_HORIZON_STATE);
    reconcile_control_sources(&ground.0).unwrap();
    let written = file(&state);
    next_second();
    for _ in 0..3 {
        let report = reconcile_control_sources(&ground.0).unwrap();
        assert!(report.new_changes.is_empty());
    }
    assert_eq!(file(&state), written);
}
