from pathlib import Path


def replace(path: str, old: str, new: str, count: int = 1):
    p = Path(path)
    text = p.read_text()
    actual = text.count(old)
    if actual < count:
        raise SystemExit(f"anchor missing in {path}: expected at least {count}, got {actual}: {old[:120]!r}")
    text = text.replace(old, new, count)
    p.write_text(text)

# Flow owner action failures use the existing structured invalid-input status for optimistic conflicts.
replace(
    "ctrl/src/projectcentral_flow.rs",
    "io::ErrorKind::AlreadyExists => ResultStatus::Conflict,",
    "io::ErrorKind::AlreadyExists => ResultStatus::InvalidInput,",
)

# Compose Flow into the native ProjectCentral action/export surface.
replace(
    "ctrl/src/lib.rs",
    "pub mod projectcentral_now;\npub mod source_horizon;",
    "pub mod projectcentral_now;\npub mod projectcentral_flow;\npub mod source_horizon;",
)
replace(
    "ctrl/src/lib.rs",
    "        super::projectcentral_ground::register_projectcentral_ground_actions(registry);\n        super::projectcentral_now::register_projectcentral_now_actions(registry);",
    "        super::projectcentral_ground::register_projectcentral_ground_actions(registry);\n        super::projectcentral_now::register_projectcentral_now_actions(registry);\n        super::projectcentral_flow::register_projectcentral_flow_actions(registry);",
)
replace(
    "ctrl/src/lib.rs",
    "pub use projectcentral_now::{\n",
    "pub use projectcentral_flow::{\n    adopt_flow, create_flow, list_flows, read_flow, registered_flow_records, rename_flow,\n    set_flow_lifecycle, snapshot_flows_for_day, write_flow, FlowDaySnapshot, FlowList,\n    FlowReading, FlowRecord, FlowRevisionReceipt, DEFAULT_FLOW_DIR, FLOW_DAY_SCHEMA,\n    FLOW_HISTORY_DIR, FLOW_REGISTRY, FLOW_REGISTRY_SCHEMA,\n};\npub use projectcentral_now::{\n",
)

# Registered Flows become first-class Source Change Horizon participants wherever their ordinary file lives.
replace(
    "ctrl/src/source_horizon.rs",
    "use crate::control::AGENT_RETRIEVAL_DENY_MARKER;",
    "use crate::control::AGENT_RETRIEVAL_DENY_MARKER;\nuse crate::projectcentral_flow::registered_flow_records;",
)
replace(
    "ctrl/src/source_horizon.rs",
    "    Ok(bindings.into_values().collect())\n}\n\npub fn control_source_bindings",
    "    for flow in registered_flow_records(project_root)? {\n        validate_project_member(&flow.path)?;\n        let path = project_root.join(&flow.path);\n        if !safe_regular_file(project_root, &path)? {\n            continue;\n        }\n        // FlowRef is the continuity identity. SourceRef remains the current ordinary-file\n        // relation and may therefore change when the retained source is renamed.\n        bindings.retain(|_, binding| binding.path != flow.path);\n        bindings.insert(\n            flow.source_ref.clone(),\n            SourceBinding {\n                source_ref: flow.source_ref,\n                path: flow.path,\n                roles: vec![\"flow-source\".to_owned()],\n                provenance: \"collaborative-revision-provenance\".to_owned(),\n                standing: \"working-source\".to_owned(),\n                treatment: \"projectcentral-flow-retained-in-place\".to_owned(),\n                agent_retrieval_allowed: retrieval_allowed(project_root, &path),\n            },\n        );\n    }\n\n    Ok(bindings.into_values().collect())\n}\n\npub fn control_source_bindings",
)

# DAY snapshots include exact current Flow revisions while leaving live Flow identity and lifecycle untouched.
replace(
    "ctrl/src/projectcentral_now.rs",
    "use crate::projectcentral::{read_project_manifest, HUMAN_SOURCE_DIR};",
    "use crate::projectcentral::{read_project_manifest, HUMAN_SOURCE_DIR};\nuse crate::projectcentral_flow::{snapshot_flows_for_day, FlowDaySnapshot};",
)
replace(
    "ctrl/src/projectcentral_now.rs",
    "    pub promotions: Vec<PromotionReceipt>,\n    pub cleanup_failures: Vec<String>,",
    "    pub promotions: Vec<PromotionReceipt>,\n    pub flows: Vec<FlowDaySnapshot>,\n    pub cleanup_failures: Vec<String>,",
)
replace(
    "ctrl/src/projectcentral_now.rs",
    ") -> io::Result<PathBuf> {\n    let snapshot_root = day_root.join(format!(\"{day}.sources\"));",
    ") -> io::Result<(PathBuf, Vec<FlowDaySnapshot>)> {\n    let snapshot_root = day_root.join(format!(\"{day}.sources\"));",
)
replace(
    "ctrl/src/projectcentral_now.rs",
    "    Ok(snapshot_root)\n}\n\nfn indented",
    "    let flows = match snapshot_flows_for_day(project_root, &snapshot_root, day) {\n        Ok(flows) => flows,\n        Err(error) => {\n            let _ = fs::remove_dir_all(&snapshot_root);\n            return Err(error);\n        }\n    };\n    Ok((snapshot_root, flows))\n}\n\nfn indented",
)
replace(
    "ctrl/src/projectcentral_now.rs",
    "    protected: &[String],\n    promotions: &[PromotionReceipt],\n) -> io::Result<String> {",
    "    protected: &[String],\n    promotions: &[PromotionReceipt],\n    flows: &[FlowDaySnapshot],\n) -> io::Result<String> {",
)
replace(
    "ctrl/src/projectcentral_now.rs",
    "    output.push_str(\"## Carry forward by stable NOW source ref\\n\\n\");",
    "    output.push_str(\"## Flows present at close\\n\\n\");\n    if flows.is_empty() {\n        output.push_str(\"- none\\n\");\n    } else {\n        for flow in flows {\n            output.push_str(&format!(\n                \"- `{}` @ `{}` — source `{}`; DAY snapshot `{}`; lifecycle `{}`\\n\",\n                flow.flow_ref, flow.revision, flow.source_path, flow.snapshot_source, flow.lifecycle\n            ));\n        }\n    }\n    output.push_str(\"\\nFlowRef remains the continuity identity across this DAY boundary; DAY records the exact revision present at close.\\n\\n\");\n\n    output.push_str(\"## Carry forward by stable NOW source ref\\n\\n\");",
)
replace(
    "ctrl/src/projectcentral_now.rs",
    "    let snapshot_root = snapshot_day_sources(\n        project_root,",
    "    let (snapshot_root, flows) = snapshot_day_sources(\n        project_root,",
)
replace(
    "ctrl/src/projectcentral_now.rs",
    "        &protected,\n        &promotions,\n    ) {",
    "        &protected,\n        &promotions,\n        &flows,\n    ) {",
)
replace(
    "ctrl/src/projectcentral_now.rs",
    "        promotions,\n        cleanup_failures,",
    "        promotions,\n        flows,\n        cleanup_failures,",
)

# Keep the public action-contract test exact after eight native Flow actions are added.
replace("ctrl/tests/foundation.rs", "assert_eq!(actions.len(), 37);", "assert_eq!(actions.len(), 45);")
replace(
    "ctrl/tests/foundation.rs",
    "        \"projectcentral.now.rollover\",\n        \"machine.account\",",
    "        \"projectcentral.now.rollover\",\n        \"projectcentral.flow.list\",\n        \"projectcentral.flow.read\",\n        \"projectcentral.flow.create\",\n        \"projectcentral.flow.adopt\",\n        \"projectcentral.flow.write\",\n        \"projectcentral.flow.rename\",\n        \"projectcentral.flow.lifecycle\",\n        \"projectcentral.flow.history\",\n        \"machine.account\",",
)
replace(
    "ctrl/tests/foundation.rs",
    "    assert!(human.output.contains(\"projectcentral.now.rollover\\tClose DAY and roll NOW\"));",
    "    assert!(human.output.contains(\"projectcentral.now.rollover\\tClose DAY and roll NOW\"));\n    assert!(human.output.contains(\"projectcentral.flow.create\\tCreate Project Flow\"));\n    assert!(human.output.contains(\"projectcentral.flow.write\\tWrite Project Flow revision\"));\n    assert!(human.output.contains(\"projectcentral.flow.history\\tRead Project Flow history\"));",
)

# Remove this temporary branch bootstrap from the resulting product diff.
Path("scripts/apply-flow-93.py").unlink()
Path(".github/workflows/flow-93-bootstrap.yml").unlink()
