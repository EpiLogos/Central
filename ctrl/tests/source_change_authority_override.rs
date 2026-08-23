use central_ctrl::{initialize_projectcentral, project_source_bindings, reconcile_project_sources, GROUND_RELATIONS_SOURCE};
use serde_json::json;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn fixture() -> (PathBuf, PathBuf) {
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let central = std::env::temp_dir().join(format!("central-authority-{}-{nonce}", std::process::id()));
    let project = central.join("Work/authority");
    fs::create_dir_all(&project).unwrap();
    initialize_projectcentral(&central, &project, "example/authority").unwrap();
    (central, project)
}

#[test]
fn explicit_relation_replaces_unresolved_aperture_identity() {
    let (central, project) = fixture();
    let authored = project.join("ProjectCentral/user/position.md");
    fs::write(&authored, "alpha\n").unwrap();
    let ledger = project.join(GROUND_RELATIONS_SOURCE);
    fs::create_dir_all(ledger.parent().unwrap()).unwrap();
    fs::write(
        &ledger,
        serde_json::to_vec_pretty(&json!({
            "schema":"central.project.ground-relations/v1",
            "project_id":"example/authority",
            "relations":[{
                "ref":"central:ground:position",
                "path":"ProjectCentral/user/position.md",
                "provenance":"human-authored",
                "standing":"authored-human-position",
                "roles":["purpose"],
                "treatment":"projectcentral-user",
                "recognition":"direct-authorship",
                "recorded_at_unix_seconds":1
            }]
        })).unwrap(),
    ).unwrap();

    let bindings = project_source_bindings(&project).unwrap();
    let same_path = bindings.iter().filter(|binding| binding.path == "ProjectCentral/user/position.md").collect::<Vec<_>>();
    assert_eq!(same_path.len(), 1);
    assert_eq!(same_path[0].source_ref, "central:ground:position");
    assert_eq!(same_path[0].provenance, "human-authored");

    reconcile_project_sources(&project).unwrap();
    fs::write(&authored, "beta\n").unwrap();
    let changed = reconcile_project_sources(&project).unwrap();
    let position_changes = changed.new_changes.iter().filter(|change| change.source_path == "ProjectCentral/user/position.md").collect::<Vec<_>>();
    assert_eq!(position_changes.len(), 1);
    assert_eq!(position_changes[0].source_ref, "central:ground:position");
    assert_eq!(position_changes[0].provenance, "human-authored");

    let _ = fs::remove_dir_all(central);
}
