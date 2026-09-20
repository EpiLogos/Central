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

/// Root counterpart of read_project_change_horizon. The same Control horizon
/// records external edits and native write attribution; reading does not adopt.
pub fn read_control_change_horizon(
    central_root: &std::path::Path,
    since: Option<u64>,
) -> std::io::Result<SourceHorizon> {
    reconcile_control_sources(central_root)?;
    let state = load_state(&central_root.join(CONTROL_HORIZON_STATE))?.ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "Control source horizon state was not created",
        )
    })?;
    Ok(public_horizon(&state, since))
}
