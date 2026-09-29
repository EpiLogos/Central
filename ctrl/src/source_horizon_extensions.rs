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

/// The binding the Control binding law gives one root path, whether or not
/// the file exists yet: a declared Control relation for the exact path wins,
/// otherwise the tree stamp of the participating tree that contains it. None
/// means the path has no home in this root ground. Used where a source must
/// be judged before it is created (a root source transfer establishing a
/// file on a bootstrap ground), so the receiving ground's own law — never the
/// origin's binding — decides authority.
pub(crate) fn control_binding_for_path(
    central_root: &std::path::Path,
    path: &str,
) -> std::io::Result<Option<SourceBinding>> {
    validate_project_member(path)?;
    let absolute = central_root.join(path);
    if let Some(relation) = read_control_ground_relations(central_root)?
        .into_iter()
        .find(|relation| relation.path == path)
    {
        return Ok(Some(SourceBinding {
            source_ref: relation.source_ref,
            path: relation.path,
            roles: relation.roles,
            provenance: relation.provenance,
            standing: relation.standing,
            treatment: relation.treatment,
            agent_retrieval_allowed: retrieval_allowed(central_root, &absolute),
        }));
    }
    for (dir, role, provenance, treatment) in CONTROL_TREE_BINDINGS {
        if path.starts_with(&format!("{dir}/")) {
            return Ok(Some(SourceBinding {
                source_ref: source_ref(CONTROL_WORLD_REF, path),
                path: path.to_owned(),
                roles: vec![role.to_owned()],
                provenance: provenance.to_owned(),
                standing: "unspecified".to_owned(),
                treatment: treatment.to_owned(),
                agent_retrieval_allowed: retrieval_allowed(central_root, &absolute),
            }));
        }
    }
    Ok(None)
}
