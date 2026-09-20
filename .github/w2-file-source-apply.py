from pathlib import Path
p=Path('ctrl/src/files.rs')
s=p.read_text()
old='''    let project = file_project(root, relative)?;
    project.project_ref.as_ref()?;'''
new='''    // Control is the root meta-Project's native authored ground, not an
    // ordinary unowned file merely because no child Project holds it.
    // Identity comes from the existing binding, never from its filename.
    if Path::new(relative).starts_with("Control") {
        return crate::source_horizon::control_source_bindings(root)
            .ok()?.into_iter().find(|binding| binding.path == relative);
    }
    let project = file_project(root, relative)?;
    project.project_ref.as_ref()?;'''
assert s.count(old)==1
p.write_text(s.replace(old,new))
p=Path('ctrl/tests/root_source_day.rs')
s=p.read_text()+r'''

#[test]
fn control_files_return_the_existing_root_source_without_child_adoption() {
    let w = world();
    fs::write(w.0.join("Control/user/intent.md"), "root authored source").unwrap();
    let observed = source(&w.0, "Control/user/intent.md");
    let directory = cli(&w.0, "central.files.list", json!({"path":"Control/user"}));
    assert_eq!(directory["ok"], true, "{directory}");
    let location = directory["data"]["entries"].as_array().unwrap().iter()
        .find(|entry| entry["name"] == "intent.md").unwrap()["location"].clone();
    let file = cli(&w.0, "central.files.read", json!({"location":location}));
    assert_eq!(file["ok"], true, "{file}");
    assert_eq!(file["data"]["source"], observed["binding"]);
    assert!(file["data"]["project"].is_null());
    assert!(!w.0.join("ProjectCentral").exists());
    // No binding is invented for unadopted ordinary work.
    fs::write(w.0.join("Work/sources/plain.md"), "ordinary file").unwrap();
    let directory = cli(&w.0, "central.files.list", json!({"path":"Work/sources"}));
    let location = directory["data"]["entries"].as_array().unwrap().iter()
        .find(|entry| entry["name"] == "plain.md").unwrap()["location"].clone();
    let file = cli(&w.0, "central.files.read", json!({"location":location}));
    assert_eq!(file["ok"], true, "{file}");
    assert!(file["data"]["source"].is_null());
    assert!(!w.0.join("Work/sources/ProjectCentral").exists());
}
'''
p.write_text(s)
