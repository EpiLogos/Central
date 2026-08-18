use central_ctrl::{initialize_now, initialize_projectcentral, inspect_now, rollover_now, NOW_USER_DIR};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn temporary_root() -> PathBuf {
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    std::env::temp_dir().join(format!("central-now-real-copy-{}-{nonce}", std::process::id()))
}

#[test]
fn portable_exact_current_central_project_can_use_now_without_rewriting_native_source() {
    let ctrl = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let real_project = ctrl.parent().expect("ctrl lives inside Central repository");
    let central = temporary_root();
    let project = central.join("Work/Central-current");
    fs::create_dir_all(project.join("docs")).unwrap();
    fs::create_dir_all(project.join("ctrl/src")).unwrap();

    fs::copy(real_project.join("README.md"), project.join("README.md")).unwrap();
    fs::copy(
        real_project.join("docs/CENTRAL-VISION.md"),
        project.join("docs/CENTRAL-VISION.md"),
    )
    .unwrap();
    fs::copy(
        real_project.join("ctrl/src/projectcentral_now.rs"),
        project.join("ctrl/src/projectcentral_now.rs"),
    )
    .unwrap();

    let readme_before = fs::read(project.join("README.md")).unwrap();
    let vision_before = fs::read(project.join("docs/CENTRAL-VISION.md")).unwrap();
    let implementation_before = fs::read(project.join("ctrl/src/projectcentral_now.rs")).unwrap();

    initialize_projectcentral(&central, &project, "epilogos/central-current").unwrap();
    assert!(!inspect_now(&project).unwrap().exists);
    initialize_now(&project).unwrap();

    // NOW is additive and human scratch remains ordinary editable source.
    fs::write(
        project.join(NOW_USER_DIR).join("current.md"),
        "Current acceptance note over the exact checked-out Central source.\n",
    )
    .unwrap();
    let inspection = inspect_now(&project).unwrap();
    assert!(inspection.exists);
    assert_eq!(inspection.human_scratch, vec!["ProjectCentral/now/user/current.md"]);

    let report = rollover_now(&project, "2026-08-19", "2026-08-20").unwrap();
    assert_eq!(report.human_scratch, vec!["ProjectCentral/now/user/current.md"]);
    assert!(project.join("ProjectCentral/now/day/2026-08-19.md").is_file());

    // Temporal collaboration does not rewrite native Project meaning or implementation.
    assert_eq!(fs::read(project.join("README.md")).unwrap(), readme_before);
    assert_eq!(fs::read(project.join("docs/CENTRAL-VISION.md")).unwrap(), vision_before);
    assert_eq!(
        fs::read(project.join("ctrl/src/projectcentral_now.rs")).unwrap(),
        implementation_before
    );
    assert!(project.join("ProjectCentral/agents/wiki/wiki.json").is_file());

    let _ = fs::remove_dir_all(central);
}
