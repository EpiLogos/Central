// Same native Flow module, register and revision store; no second Flow owner.
include!("projectcentral_flow.rs");

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
    let register = register_of(&scope.root)?;
    validate_flow_placement(&scope.root,path)?;
    if content.len() > MAX_FLOW_TEXT_BYTES || content.contains('\0') {
        return Err(io::Error::new(io::ErrorKind::InvalidInput,"native Flow document exceeds the current bounded source size"));
    }
    let reference = format!("central:flow:{}:{}",register.id,crate::continuous_work::source::key(document_id));
    let mut registry = load_registry(&scope.root)?;
    if let Some(record) = registry.flows.iter().find(|record|record.flow_ref==reference) {
        if record.path != path || flow_bytes(&scope.root,path)? != content.as_bytes() {
            return Err(io::Error::new(io::ErrorKind::AlreadyExists,"document identity already has different Flow source bytes or placement"));
        }
        return Ok(record.clone());
    }
    ensure_unique_path(&registry,path,None)?;
    crate::continuous_work::source::put_new(&scope.root,path,content)?;
    let kind = if actor.actor_kind == "native-service" {"system"} else {&actor.actor_kind};
    let mut record = FlowRecord {
        flow_ref:reference,source_ref:scope.source_ref(path),path:path.into(),
        created_at_unix_seconds:now,current_revision:String::new(),lifecycle:"active".into(),
        title:title.map(str::to_owned),scope_ref:register.id,privacy:"inherit".into(),revisions:vec![],
    };
    seed_revision(&scope.root,&mut record,content.as_bytes(),&actor.principal_ref,kind,None)?;
    if let Some(receipt)=record.revisions.last_mut() {receipt.recorded_at_unix_seconds=now;}
    registry.flows.push(record.clone());
    write_registry(&scope.root,&registry)?;
    scope.reconcile(Some((&actor.principal_ref,&actor.actor_kind,None)),std::slice::from_ref(&record.source_ref))?;
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
    if content.len() > MAX_FLOW_TEXT_BYTES || content.contains('\0') {
        return Err(io::Error::new(io::ErrorKind::InvalidInput,"native Flow document exceeds the current bounded source size"));
    }
    let mut registry=load_registry(&scope.root)?;
    let index=registry.flows.iter().position(|record|record.source_ref==current.source.source_ref)
        .ok_or_else(||io::Error::new(io::ErrorKind::NotFound,"document SourceRef is not in its existing Flow register"))?;
    if registry.flows[index].path != current.source.path {
        return Err(io::Error::new(io::ErrorKind::AlreadyExists,"Flow registry and document source path disagree"));
    }
    let current_bytes=flow_bytes(&scope.root,&current.source.path)?;
    if content_revision_bytes(&current_bytes)!=current.revision.revision {
        return Err(io::Error::new(io::ErrorKind::AlreadyExists,"Flow source changed before native document commit"));
    }
    if reconcile_record(&scope.root,&mut registry.flows[index])? {write_registry(&scope.root,&registry)?;}
    crate::source_safety::replace(&scope.root,&current.source.path,&current.revision.revision,content)?;
    let kind=if actor_kind=="native-service" {"system"} else {actor_kind};
    if store_revision(&scope.root,&mut registry.flows[index],content.as_bytes(),actor_ref,kind,None)? {
        if let Some(receipt)=registry.flows[index].revisions.last_mut() {receipt.recorded_at_unix_seconds=now;}
        write_registry(&scope.root,&registry)?;
    }
    let record=registry.flows[index].clone();
    scope.reconcile(Some((actor_ref,actor_kind,None)),std::slice::from_ref(&record.source_ref))?;
    Ok(record)
}
