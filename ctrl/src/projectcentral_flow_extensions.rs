// Same native Flow module, register and revision store; no second Flow owner.
include!("projectcentral_flow.rs");

fn document_io(error: io::Error, stage: &str, path: &str) -> io::Error {
    io::Error::new(error.kind(), format!("native Flow document {stage} ({path}): {error}"))
}
fn bind_native_document(
    scope: &crate::continuous_work::source::Scope,
    record: &FlowRecord,
    now: u64,
) -> io::Result<()> {
    scope.bind(&crate::source_horizon::SourceBinding {
        source_ref: record.source_ref.clone(), path: record.path.clone(),
        roles: vec!["flow-transcript".into(), crate::continuous_work::documents::ROLE.into()],
        provenance: "agent-maintained".into(), standing: "current-development-state".into(),
        treatment: "generated-derived".into(), agent_retrieval_allowed: true,
    }, now).map_err(|error| document_io(error, "source binding", &record.path))
}

/// Caller holds the existing source-mutation lock and has authenticated the
/// native document operation. Stable document identity makes retry inspectable.
pub(crate) fn publish_native_document(
    scope: &crate::continuous_work::source::Scope,
    document_id: &str,
    path: &str,
    content: &str,
    title: Option<&str>,
    actor: &crate::continuous_work::authority::Principal,
    now: u64,
) -> io::Result<FlowRecord> {
    let register = register_of(&scope.root).map_err(|error| document_io(error, "register resolution", path))?;
    validate_flow_placement(&scope.root,path).map_err(|error| document_io(error, "placement", path))?;
    if content.len() > MAX_FLOW_TEXT_BYTES || content.contains('\0') {
        return Err(io::Error::new(io::ErrorKind::InvalidInput,"native Flow document exceeds the current bounded source size"));
    }
    let reference = format!("central:flow:{}:{}",register.id,crate::continuous_work::source::key(document_id));
    let mut registry = load_registry(&scope.root).map_err(|error| document_io(error, "registry read", path))?;
    if let Some(record) = registry.flows.iter().find(|record|record.flow_ref==reference) {
        let bytes = flow_bytes(&scope.root,path).map_err(|error| document_io(error, "replay source read", path))?;
        if record.path != path || bytes != content.as_bytes() {
            return Err(io::Error::new(io::ErrorKind::AlreadyExists,"document identity already has different Flow source bytes or placement"));
        }
        bind_native_document(scope, record, now)?;
        return Ok(record.clone());
    }
    ensure_unique_path(&registry,path,None)?;
    let relative = relative_member(path)?;
    let parent = relative.parent().ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput,"native Flow requires a source parent"))?;
    crate::continuous_work::source::directories(&scope.root,parent)
        .map_err(|error| document_io(error, "source parent creation", path))?;
    crate::continuous_work::source::put_new(&scope.root,path,content)
        .map_err(|error| document_io(error, "source publication", path))?;
    let kind = if actor.actor_kind == "native-service" {"system"} else {&actor.actor_kind};
    let mut record = FlowRecord {
        flow_ref:reference,source_ref:scope.source_ref(path),path:path.into(),
        created_at_unix_seconds:now,current_revision:String::new(),lifecycle:"active".into(),
        title:title.map(str::to_owned),scope_ref:register.id,privacy:"inherit".into(),revisions:vec![],
    };
    seed_revision(&scope.root,&mut record,content.as_bytes(),&actor.principal_ref,kind,None)
        .map_err(|error| document_io(error, "initial history snapshot", path))?;
    if let Some(receipt)=record.revisions.last_mut() {receipt.recorded_at_unix_seconds=now;}
    registry.flows.push(record.clone());
    write_registry(&scope.root,&registry).map_err(|error| document_io(error, "registry publication", path))?;
    // Publish the explicit native SourceRef/roles before any observer consumes
    // this source. Replay repairs an interrupted registry-to-relation boundary.
    bind_native_document(scope, &record, now)?;
    scope.reconcile(Some((&actor.principal_ref,&actor.actor_kind,None)),std::slice::from_ref(&record.source_ref))
        .map_err(|error| document_io(error, "source horizon reconciliation", path))?;
    Ok(record)
}

/// Authenticated contribution operations call this only after their exact
/// document-level ownership and source-revision checks. Generic Flow writes do
/// not acquire this authority merely by declaring actor_kind=human.
pub(crate) fn write_native_document(
    scope: &crate::continuous_work::source::Scope,
    current: &crate::continuous_work::source::SourceReading,
    content: &str,
    actor_ref: &str,
    actor_kind: &str,
    now: u64,
) -> io::Result<FlowRecord> {
    let path = &current.source.path;
    if content.len() > MAX_FLOW_TEXT_BYTES || content.contains('\0') {
        return Err(io::Error::new(io::ErrorKind::InvalidInput,"native Flow document exceeds the current bounded source size"));
    }
    let mut registry=load_registry(&scope.root).map_err(|error| document_io(error,"registry read",path))?;
    let index=registry.flows.iter().position(|record|record.source_ref==current.source.source_ref)
        .ok_or_else(||io::Error::new(io::ErrorKind::NotFound,"document SourceRef is not in its existing Flow register"))?;
    if registry.flows[index].path != current.source.path {
        return Err(io::Error::new(io::ErrorKind::AlreadyExists,"Flow registry and document source path disagree"));
    }
    let current_bytes=flow_bytes(&scope.root,path).map_err(|error| document_io(error,"current source read",path))?;
    if content_revision_bytes(&current_bytes)!=current.revision.revision {
        return Err(io::Error::new(io::ErrorKind::AlreadyExists,"Flow source changed before native document commit"));
    }
    if reconcile_record(&scope.root,&mut registry.flows[index]).map_err(|error| document_io(error,"history reconciliation",path))? {
        write_registry(&scope.root,&registry).map_err(|error| document_io(error,"reconciled registry publication",path))?;
    }
    crate::source_safety::replace(&scope.root,path,&current.revision.revision,content)
        .map_err(|error| document_io(error,"exact source replacement",path))?;
    let kind=if actor_kind=="native-service" {"system"} else {actor_kind};
    if store_revision(&scope.root,&mut registry.flows[index],content.as_bytes(),actor_ref,kind,None)
        .map_err(|error| document_io(error,"history snapshot",path))? {
        if let Some(receipt)=registry.flows[index].revisions.last_mut() {receipt.recorded_at_unix_seconds=now;}
        write_registry(&scope.root,&registry).map_err(|error| document_io(error,"revision registry publication",path))?;
    }
    let record=registry.flows[index].clone();
    scope.reconcile(Some((actor_ref,actor_kind,None)),std::slice::from_ref(&record.source_ref))
        .map_err(|error| document_io(error,"source horizon reconciliation",path))?;
    Ok(record)
}
