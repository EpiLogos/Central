from pathlib import Path


def replace(path: str, old: str, new: str, count: int = 1):
    p = Path(path)
    text = p.read_text()
    if text.count(old) < count:
        raise SystemExit(f"anchor missing in {path}: {old[:120]!r}")
    p.write_text(text.replace(old, new, count))

flow = "ctrl/src/projectcentral_flow.rs"

# Preserve Central's already-distinct Ground/Wiki/governance/NOW authority containers.
replace(
    flow,
    "fn default_path(local_stamp: Option<&str>) -> io::Result<String> {",
    '''fn validate_flow_placement(project_root: &Path, path: &str) -> io::Result<()> {
    let manifest = read_project_manifest(project_root)?;
    let candidate = Path::new(path);
    let reserved = [
        manifest.human_source.as_str(),
        "ProjectCentral/agents/governance",
        "ProjectCentral/agents/wiki",
        "ProjectCentral/now/user",
        "ProjectCentral/now/agents",
    ];
    if reserved.iter().any(|root| candidate.starts_with(Path::new(root))) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Flow source placement would overlap an existing Central authority container; retain Flow in its own or a neutral provider/domain-local source container",
        ));
    }
    Ok(())
}

fn default_path(local_stamp: Option<&str>) -> io::Result<String> {''',
)
replace(
    flow,
    "    ensure_unique_path(&registry, &path, None)?;\n    let source = safe_flow_path(project_root, &path, false)?;",
    "    ensure_unique_path(&registry, &path, None)?;\n    validate_flow_placement(project_root, &path)?;\n    let source = safe_flow_path(project_root, &path, false)?;",
    1,
)
replace(
    flow,
    "    ensure_unique_path(&registry, &path, None)?;\n    let source = safe_flow_path(project_root, &path, true)?;",
    "    ensure_unique_path(&registry, &path, None)?;\n    validate_flow_placement(project_root, &path)?;\n    let source = safe_flow_path(project_root, &path, true)?;",
    1,
)
replace(
    flow,
    "    ensure_unique_path(&registry, &normalized, Some(flow_ref))?;\n    let destination = safe_flow_path(project_root, &normalized, false)?;",
    "    ensure_unique_path(&registry, &normalized, Some(flow_ref))?;\n    validate_flow_placement(project_root, &normalized)?;\n    let destination = safe_flow_path(project_root, &normalized, false)?;",
)

# If external edits are discovered before metadata-only operations, persist their truthful revision receipt even when the caller is stale.
replace(
    flow,
    "    reconcile_record(project_root, &mut registry.flows[index])?;\n    if registry.flows[index].current_revision != expected_revision {\n        return Err(io::Error::new(io::ErrorKind::AlreadyExists, \"Flow revision conflict before rename\"));",
    "    if reconcile_record(project_root, &mut registry.flows[index])? {\n        write_registry(project_root, &registry)?;\n    }\n    if registry.flows[index].current_revision != expected_revision {\n        return Err(io::Error::new(io::ErrorKind::AlreadyExists, \"Flow revision conflict before rename\"));",
)
replace(
    flow,
    "    reconcile_record(project_root, &mut registry.flows[index])?;\n    if registry.flows[index].current_revision != expected_revision {\n        return Err(io::Error::new(io::ErrorKind::AlreadyExists, \"Flow revision conflict before lifecycle change\"));",
    "    if reconcile_record(project_root, &mut registry.flows[index])? {\n        write_registry(project_root, &registry)?;\n    }\n    if registry.flows[index].current_revision != expected_revision {\n        return Err(io::Error::new(io::ErrorKind::AlreadyExists, \"Flow revision conflict before lifecycle change\"));",
)

# Public CLI contract must name every newly registered Action.
replace("docs/CLI-REFERENCE.md", "The current composed registry exposes 37 Actions:", "The current composed registry exposes 45 Actions:")
replace(
    "docs/CLI-REFERENCE.md",
    "| `projectcentral.now.rollover` | close a DAY snapshot and roll NOW forward | `action run projectcentral.now.rollover` |",
    '''| `projectcentral.now.rollover` | close a DAY snapshot and roll NOW forward | `action run projectcentral.now.rollover` |
| `projectcentral.flow.list` | list stable Flow identities and current source/revision state | `action run projectcentral.flow.list` |
| `projectcentral.flow.read` | read current Flow source by FlowRef and reconcile external edits | `action run projectcentral.flow.read` |
| `projectcentral.flow.create` | create a blank ordinary-file Flow with stable identity | `action run projectcentral.flow.create` |
| `projectcentral.flow.adopt` | adopt an existing retained ordinary file as a Flow without moving it | `action run projectcentral.flow.adopt` |
| `projectcentral.flow.write` | perform a revision-safe human/Agent Flow write | `action run projectcentral.flow.write` |
| `projectcentral.flow.rename` | rename the retained source while preserving FlowRef | `action run projectcentral.flow.rename` |
| `projectcentral.flow.lifecycle` | set active/dormant/closed lifecycle without changing source revision | `action run projectcentral.flow.lifecycle` |
| `projectcentral.flow.history` | read exact Flow revision provenance/history | `action run projectcentral.flow.history` |''',
)
replace(
    "docs/CLI-REFERENCE.md",
    "ctrl --json action run projectcentral.now.inspect '{\"project\":\"Central\"}'",
    "ctrl --json action run projectcentral.now.inspect '{\"project\":\"Central\"}'\nctrl --json action run projectcentral.flow.create '{\"project\":\"Central\",\"actor\":\"human:local\",\"actor_kind\":\"human\",\"local_stamp\":\"2026-08-23-2310\"}'",
)

# Index the new normative implementation contract from the docs front door.
replace(
    "docs/README.md",
    "- [PROJECTCENTRAL-NOW.md](PROJECTCENTRAL-NOW.md) — the opt-in NOW temporal field,\n  DAY source snapshots/closure, bounded Agent returns, promotion lineage and\n  rollover semantics over an already-valid ProjectCentral.",
    "- [PROJECTCENTRAL-NOW.md](PROJECTCENTRAL-NOW.md) — the opt-in NOW temporal field,\n  DAY source snapshots/closure, bounded Agent returns, promotion lineage and\n  rollover semantics over an already-valid ProjectCentral.\n- [PROJECTCENTRAL-FLOW.md](PROJECTCENTRAL-FLOW.md) — stable Flow source identity,\n  revision-safe collaborative writes, retained-in-place source placement,\n  Source Change Horizon participation and exact DAY revision snapshots.",
)
replace(
    "docs/README.md",
    "| Working with Project NOW / DAY | `PROJECTCENTRAL-CONTRACT.md` → `PROJECTCENTRAL-NOW.md` → `CLI-REFERENCE.md` |",
    "| Working with Project NOW / DAY | `PROJECTCENTRAL-CONTRACT.md` → `PROJECTCENTRAL-NOW.md` → `CLI-REFERENCE.md` |\n| Working with a live Flow | `PROJECTCENTRAL-FLOW.md` → `PROJECTCENTRAL-NOW.md` → `CLI-REFERENCE.md` |",
)

# Conformance: a Flow cannot silently reclassify an already-owned human/Wiki source container.
test = Path("ctrl/tests/projectcentral_flow.rs")
text = test.read_text()
text += r'''

#[test]
fn flow_adoption_preserves_existing_central_authority_boundaries() {
    let (_central, project) = temporary_project("authority-boundary");
    let human_source = project.join("ProjectCentral/user/meaning.md");
    fs::write(&human_source, "authored ground candidate\n").unwrap();
    let attempt = adopt_flow(
        &project,
        "ProjectCentral/user/meaning.md",
        None,
        "human:test",
        "human",
        None,
    );
    assert!(attempt.is_err());
}
'''
test.write_text(text)

Path("scripts/apply-flow-93-hardening.py").unlink()
Path(".github/workflows/flow-93-hardening.yml").unlink()
