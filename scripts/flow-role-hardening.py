from pathlib import Path

source = Path("ctrl/src/source_horizon.rs")
text = source.read_text()
old = '''        // FlowRef is the continuity identity. SourceRef remains the current ordinary-file
        // relation and may therefore change when the retained source is renamed.
        bindings.retain(|_, binding| binding.path != flow.path);
        bindings.insert(
            flow.source_ref.clone(),
            SourceBinding {
                source_ref: flow.source_ref,
                path: flow.path,
                roles: vec!["flow-source".to_owned()],
                provenance: "collaborative-revision-provenance".to_owned(),
                standing: "working-source".to_owned(),
                treatment: "projectcentral-flow-retained-in-place".to_owned(),
                agent_retrieval_allowed: retrieval_allowed(project_root, &path),
            },
        );
'''
new = '''        // FlowRef is the continuity identity. SourceRef remains the current ordinary-file
        // relation and may therefore change when the retained source is renamed. If the same
        // retained file already participates through another explicit source relation (for
        // example an adopted Wiki source), Flow is an additional role on that source rather
        // than a replacement for the existing authority relation.
        if let Some(existing) = bindings.values_mut().find(|binding| binding.path == flow.path) {
            if !existing.roles.iter().any(|role| role == "flow-source") {
                existing.roles.push("flow-source".to_owned());
                existing.roles.sort();
                existing.roles.dedup();
            }
        } else {
            bindings.insert(
                flow.source_ref.clone(),
                SourceBinding {
                    source_ref: flow.source_ref,
                    path: flow.path,
                    roles: vec!["flow-source".to_owned()],
                    provenance: "collaborative-revision-provenance".to_owned(),
                    standing: "working-source".to_owned(),
                    treatment: "projectcentral-flow-retained-in-place".to_owned(),
                    agent_retrieval_allowed: retrieval_allowed(project_root, &path),
                },
            );
        }
'''
if old not in text:
    raise SystemExit("source_horizon Flow binding anchor not found")
source.write_text(text.replace(old, new, 1))

test = Path("ctrl/tests/projectcentral_flow.rs")
text = test.read_text()
text += r'''

#[test]
fn flow_role_composes_with_retained_wiki_role_without_reclassifying_the_source() {
    let (_central, project) = temporary_project("dual-role");
    fs::create_dir_all(project.join("notes")).unwrap();
    fs::write(project.join("notes/shared.md"), "shared source\n").unwrap();

    let manifest_path = project.join("ProjectCentral/project.json");
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
    manifest["wiki"]["adopted_sources"] = serde_json::json!(["notes/shared.md"]);
    fs::write(
        &manifest_path,
        serde_json::to_vec_pretty(&manifest).unwrap(),
    )
    .unwrap();

    let flow = adopt_flow(
        &project,
        "notes/shared.md",
        None,
        "human:test",
        "human",
        None,
    )
    .unwrap();
    let horizon = read_project_change_horizon(&project, None).unwrap();
    let observed = horizon
        .sources
        .iter()
        .find(|source| source.binding.path == "notes/shared.md")
        .unwrap();
    assert!(observed.binding.roles.iter().any(|role| role == "adopted-agent-wiki-source"));
    assert!(observed.binding.roles.iter().any(|role| role == "flow-source"));
    assert_eq!(observed.binding.source_ref, "central:source:project:example/project:notes/shared.md");
    assert_eq!(flow.flow_ref.starts_with("central:flow:project:example/project:"), true);
}
'''
test.write_text(text)

Path("scripts/flow-role-hardening.py").unlink()
Path(".github/workflows/flow-role-hardening.yml").unlink()
