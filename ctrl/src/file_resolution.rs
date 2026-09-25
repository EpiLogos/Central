//! Resolve an existing native path identity within its configured Central root.
//! The owner alone decodes its ref grammar; this confers no write authority.
use super::*;

fn resolve_ref(configured: &Path, reference: &str) -> io::Result<CentralPathRef> {
    let root = configured.canonicalize()?;
    let prefix = format!("central:path:{}:", escape(utf8(&root)?));
    let encoded = reference.strip_prefix(&prefix).filter(|value| !value.is_empty() && value.len() <= 32768)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "File reference belongs to another root or is not a bounded native file identity"))?;
    let mut bytes = Vec::with_capacity(encoded.len());
    let mut input = encoded.as_bytes().iter().copied();
    while let Some(byte) = input.next() {
        if byte == b'%' {
            let high = input.next().and_then(|c| (c as char).to_digit(16));
            let low = input.next().and_then(|c| (c as char).to_digit(16));
            let (Some(high), Some(low)) = (high, low) else { return Err(io::Error::new(io::ErrorKind::InvalidInput, "Malformed native file reference")); };
            bytes.push((high * 16 + low) as u8);
        } else { bytes.push(byte); }
    }
    let relative = String::from_utf8(bytes).map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
    let location = CentralPathRef::new(&root, relative)?;
    if location.ref_id != reference { return Err(io::Error::new(io::ErrorKind::InvalidInput, "Non-canonical native file reference")); }
    let path = location.resolve(&root)?;
    if !path.is_file() { return Err(io::Error::new(io::ErrorKind::InvalidInput, "The native reference does not name an existing file")); }
    require_retrieval(&root, &path, false)?;
    Ok(location)
}

pub(super) fn register(registry: &mut ActionRegistry) {
    let id = "central.files.resolve";
    registry.register(ActionDescriptor {
        id:id.into(), title:"Resolve native Central file".into(),
        description:"Resolve one exact canonical path reference through its owning root and retrieval policy. No recursive search, adoption or mutation.".into(),
        inputs:vec![ActionInputDefinition{name:"ref".into(),input_type:"string".into(),required:true,choices:None,selection:None}],
        output:ActionOutputDefinition{output_type:"central-file-location".into()}, mutation_class:MutationClass::ReadOnly,
        preview_supported:false,required_ports:vec![],availability:ActionAvailability{available:true,reason:None},
    }, |_, input, context| {
        let result = (|| -> io::Result<Value> {
            let value=input.as_object().filter(|value| value.len()==1).and_then(|value| value.get("ref")).and_then(Value::as_str)
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput,"One exact native file ref is required"))?;
            let root=resolve_central_root(context.root_options).map_err(io::Error::other)?.path;
            let location=resolve_ref(&root,value)?;
            Ok(serde_json::json!({"schema":"central.file-location/v1","location":location,"automatic_agent_or_model_invocation":false}))
        })();
        match result {
            Ok(data)=>ActionResult::success("central.files.resolve",data),
            Err(error)=>ActionResult::failure(Some("central.files.resolve"),match error.kind(){io::ErrorKind::PermissionDenied=>ResultStatus::UnavailableCapability,io::ErrorKind::InvalidInput|io::ErrorKind::NotFound=>ResultStatus::InvalidInput,_=>ResultStatus::InternalFailure},error.to_string(),None),
        }
    }).expect("Unique native file resolve action");
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_owner_reference_roundtrip_and_refusals() {
        let temp=tempfile::tempdir().unwrap();let root=temp.path().canonicalize().unwrap();
        crate::root::initialize_central(&root).unwrap();
        fs::create_dir_all(root.join("Work/Bare")).unwrap();
        let relative="Work/Bare/a b:é.txt";
        fs::write(root.join(relative),"actual native file").unwrap();
        let location=CentralPathRef::new(&root,relative.into()).unwrap();
        assert_eq!(resolve_ref(&root,&location.ref_id).unwrap(),location);
        let options=crate::RootOptions{explicit_root:Some(root.clone()),..Default::default()};
        let connectors=crate::create_default_connector_registry();let connector_context=crate::ConnectorContext::current();
        let context=ActionExecutionContext{root_options:&options,connectors:&connectors,connector_context:&connector_context};
        let registry=crate::create_core_action_registry();
        assert_eq!(registry.get("central.files.resolve").unwrap().mutation_class,MutationClass::ReadOnly);
        let resolved=registry.execute("central.files.resolve",&serde_json::json!({"ref":location.ref_id}),&context);
        assert!(resolved.ok,"{resolved:?}");
        assert_eq!(resolved.data.unwrap()["location"],serde_json::to_value(&location).unwrap());

        assert!(resolve_ref(&root,&location.ref_id.replace("%20"," ")).is_err());
        assert!(resolve_ref(&root,&location.ref_id.replace("%C3","%c3")).is_err());
        assert!(resolve_ref(&root,"central:path:/another-root:file.txt").is_err());
        let traversal=CentralPathRef::new(&root,"../outside.txt".into()).unwrap();
        assert!(resolve_ref(&root,&traversal.ref_id).is_err());
        let directory=CentralPathRef::new(&root,"Work/Bare".into()).unwrap();
        assert!(resolve_ref(&root,&directory.ref_id).is_err());
        fs::write(root.join("Work/Bare/.no-agent-retrieval"),"").unwrap();
        assert_eq!(resolve_ref(&root,&location.ref_id).unwrap_err().kind(),io::ErrorKind::PermissionDenied);
    }
    #[cfg(unix)]
    #[test]
    fn redirected_symlink_reference_is_refused() {
        let temp=tempfile::tempdir().unwrap();let root=temp.path().canonicalize().unwrap();
        crate::root::initialize_central(&root).unwrap();
        fs::write(root.join("real.txt"),"native").unwrap();
        std::os::unix::fs::symlink(root.join("real.txt"),root.join("link.txt")).unwrap();
        let location=CentralPathRef::new(&root,"link.txt".into()).unwrap();
        assert!(resolve_ref(&root,&location.ref_id).is_err());
    }
}
