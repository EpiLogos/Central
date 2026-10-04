// Keep the existing horizon implementation and its public API intact. This
// same-module extension exposes the root counterpart of project writeback.

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
    let bytes = match std::fs::read(central_root.join(CONTROL_GROUND_RELATIONS_SOURCE)) {
        Ok(bytes) => Some(bytes),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(error),
    };
    let relations = bytes.as_deref().map(serde_json::from_slice::<Value>).transpose()
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
    if let Some(value) = &relations {
        validate_relations_value(value, CONTROL_GROUND_RELATIONS_SCHEMA, CONTROL_WORLD_REF)?;
    }
    let member_key = crate::source_safety::normal_member_key(path)?;
    let mut explicitly_bound = false;
    if let Some(value) = &relations {
        for entry in value["relations"].as_array().ok_or_else(|| std::io::Error::new(
            std::io::ErrorKind::InvalidData, "relations must be an array"))? {
            let declared_path = entry["path"].as_str().ok_or_else(|| std::io::Error::new(
                std::io::ErrorKind::InvalidData, "source relation path must be text"))?;
            if crate::source_safety::normal_member_key(declared_path)? == member_key {
                explicitly_bound = true;
                break;
            }
        }
    }
    let manifest_path = if explicitly_bound { None } else {
        crate::control_skills::control_skill_manifest_path(path)?
    };
    let manifest = manifest_path.as_deref().map(|relative|
        crate::control_skills::read_skill_manifest(
            central_root.join(relative).parent().expect("native Skill manifest parent")))
        .transpose()?.flatten();
    control_binding_for_observed_path(path, relations.as_ref(), manifest.as_ref(),
        retrieval_allowed(central_root, &absolute))
}

/// Resolve already-observed native metadata without scanning the World or
/// reading another source. The caller qualifies the metadata's current basis.
pub(crate) fn control_binding_for_observed_path(
    path: &str,
    relations: Option<&Value>,
    skill_manifest: Option<&crate::control_skills::SkillManifest>,
    agent_retrieval_allowed: bool,
) -> std::io::Result<Option<SourceBinding>> {
    validate_project_member(path)?;
    let declared = relations.map(|value| parse_relations_value(value,
        CONTROL_GROUND_RELATIONS_SCHEMA, CONTROL_WORLD_REF)).transpose()?;
    let member_key = crate::source_safety::normal_member_key(path)?;
    for relation in declared.into_iter().flat_map(|file| file.relations) {
        if crate::source_safety::normal_member_key(&relation.path)? != member_key { continue; }
        return Ok(Some(SourceBinding {
            source_ref: relation.source_ref,
            path: relation.path,
            roles: relation.roles,
            provenance: relation.provenance,
            standing: relation.standing,
            treatment: relation.treatment,
            agent_retrieval_allowed,
        }));
    }
    if let Some(binding) = crate::control_skills::control_skill_binding(
        path, skill_manifest, agent_retrieval_allowed)? {
        return Ok(Some(binding));
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
                agent_retrieval_allowed,
            }));
        }
    }
    Ok(None)
}

include!("source_horizon.rs");
