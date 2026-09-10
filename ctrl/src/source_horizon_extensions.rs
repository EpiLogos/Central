// Keep the existing horizon implementation and its public API intact. This
// same-module extension exposes the root counterpart of project writeback.
include!("source_horizon.rs");

pub fn reconcile_control_source_writes(
    central_root: &std::path::Path,
    attributions: &std::collections::BTreeMap<String, SourceWriteAttribution>,
) -> std::io::Result<ReconcileReport> {
    reconcile(
        central_root,
        &central_root.join(CONTROL_HORIZON_STATE),
        CONTROL_WORLD_REF,
        control_source_bindings(central_root)?,
        attributions,
    )
}
