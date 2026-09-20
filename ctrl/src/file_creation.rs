// Included by file_mutation.rs: shares its protected-ground policy, owner
// lock, history and directory-relative no-symlink operations. This is the
// explicit first-save operation; existing central.files.write is unchanged.

fn create_ordinary(root: &Path, input: &Value) -> io::Result<Value> {
    let root = root.canonicalize()?;
    let allowed = ["parent", "name", "content", "expected_absent", "operation_ref", "actor", "actor_kind", "agent_session_ref"];
    let object = input.as_object().ok_or_else(|| invalid("Creation input must be an object"))?;
    if object.keys().any(|key| !allowed.contains(&key.as_str())) {
        return Err(invalid("Unknown ordinary-file creation field"));
    }
    if input.get("expected_absent") != Some(&Value::Bool(true)) {
        return Err(invalid("Creation requires explicit expected_absent: true"));
    }
    let parent_loc: CentralPathRef = serde_json::from_value(input.get("parent").cloned().unwrap_or(Value::Null))?;
    let parent_path = parent_loc.resolve(&root)?;
    if !parent_path.is_dir() { return Err(invalid("The selected parent must already be a native directory")); }
    let name = text(input, "name")?;
    if name.is_empty() || name.len() > 255 || Path::new(name).components().count() != 1
        || !matches!(Path::new(name).components().next(), Some(std::path::Component::Normal(_)))
        || name.contains(['\0', '/', '\\']) {
        return Err(invalid("A single bounded native filename is required"));
    }
    let relative = if parent_loc.path.is_empty() { name.into() } else { format!("{}/{name}", parent_loc.path) };
    let loc = CentralPathRef::new(&root, relative)?;
    ordinary_policy(&root, &loc)?;
    let operation = text(input, "operation_ref")?;
    if operation.trim().is_empty() || operation.len() > 4096 { return Err(invalid("Creation requires a bounded operation identity")); }
    let content = text(input, "content")?;
    if content.len() > MAX || content.contains('\0') { return Err(invalid("Content must be bounded UTF-8 without NUL")); }
    let (actor, actor_kind, agent_session_ref) = attribution(input)?;
    let revision = content_revision_bytes(content.as_bytes());
    let request = json!({"operation_ref":operation,"location":loc,"revision":revision,"actor":actor,"actor_kind":actor_kind,"agent_session_ref":agent_session_ref});
    let (dir, _lock) = state(&root)?;
    let area = area(&dir, &loc)?;
    let record = area.join("creation.json");
    let pending = area.join("pending.json");
    // Revalidate the selected parent and native authority after acquiring the
    // owner lock. Neither a stale directory nor a newly participating source
    // is converted into an ordinary file by a check made before the lock.
    parent_loc.resolve(&root)?;
    ordinary_policy(&root, &loc)?;
    let parent = directory(&root, Path::new(&parent_loc.path))?;
    if parent.metadata()?.permissions().readonly() { return Err(denied("The native directory is read-only")); }
    if record.exists() {
        let previous: Value = serde_json::from_reader(open(&record, false)?.take(65536))?;
        if previous != request { return Err(conflict("This filename has another creation history; open it or choose a new name")); }
        if let Ok(reading) = read_file(&root, &loc, crate::files::FileEncoding::Utf8) {
            if reading.revision != revision || reading.content != content {
                return Err(conflict("The created file has since changed; do not replay first save over it"));
            }
            if pending.exists() {
                let event: Change = serde_json::from_reader(open(&pending, false)?.take(65536))?;
                if event.revision != revision || !event.previous_revision.is_empty() { return Err(invalid("Interrupted creation has another history basis")); }
                fs::rename(&pending, area.join(format!("event-{}.json", event.cursor)))?;
                File::open(&area)?.sync_all()?;
            }
            return Ok(json!({"schema":"central.file-mutation/v1","outcome":"unchanged","location":loc,"revision":revision,"changed":false,"operation_ref":operation,"current":reading,"automatic_agent_or_model_invocation":false}));
        }
        // A recorded but interrupted pre-effect creation can resume only on
        // the same exact request. linkat below still refuses any destination.
    } else if pending.exists() {
        return Err(io::Error::other("An interrupted file mutation is unresolved; do not create over its recovery state"));
    }
    match fs::symlink_metadata(root.join(&loc.path)) {
        Ok(_) => return Err(conflict("The native filename already exists; creation never overwrites")),
        Err(e) if e.kind() == io::ErrorKind::NotFound => {},
        Err(e) => return Err(e),
    }
    let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_err(io::Error::other)?.as_nanos();
    let staging = format!(".central-create-{}-{nonce}", std::process::id());
    let mut file = create_in(&parent, &staging, 0o600)?;
    let c_staging = std::ffi::CString::new(staging).map_err(io::Error::other)?;
    let c_name = std::ffi::CString::new(name).map_err(io::Error::other)?;
    let mut committed = false;
    let result = (|| {
        file.write_all(content.as_bytes())?;
        file.sync_all()?;
        let cursor = events(&area, 1, None)?.first().map(|event| event.cursor).unwrap_or(0)
            .checked_add(1).ok_or_else(|| invalid("History cursor exhausted"))?;
        let event = Change { cursor, previous_revision: String::new(), revision: revision.clone(), actor, actor_kind, agent_session_ref, restored_from: None };
        snapshot(&area, content)?;
        atomic_record(&record, &serde_json::to_vec(&request)?)?;
        atomic_record(&pending, &serde_json::to_vec(&event)?)?;
        File::open(&area)?.sync_all()?;
        let current_parent = directory(&root, Path::new(&parent_loc.path))?.metadata()?;
        let held_parent = parent.metadata()?;
        if current_parent.dev() != held_parent.dev() || current_parent.ino() != held_parent.ino() {
            return Err(conflict("The selected directory moved while preparing the native file"));
        }
        // Unlike rename, linkat atomically REFUSES an existing destination on
        // Linux and macOS. The staged inode already contains fsynced bytes.
        if unsafe { libc::linkat(parent.as_raw_fd(), c_staging.as_ptr(), parent.as_raw_fd(), c_name.as_ptr(), 0) } != 0 {
            return Err(io::Error::last_os_error());
        }
        committed = true;
        if unsafe { libc::unlinkat(parent.as_raw_fd(), c_staging.as_ptr(), 0) } != 0 { return Err(io::Error::last_os_error()); }
        parent.sync_all()?;
        fs::rename(&pending, area.join(format!("event-{}.json", event.cursor)))?;
        File::open(&area)?.sync_all()?;
        let reading = read_file(&root, &loc, crate::files::FileEncoding::Utf8)?;
        if reading.revision != revision || reading.content != content { return Err(conflict("The newly created file changed before independent readback")); }
        Ok(json!({"schema":"central.file-mutation/v1","outcome":"created","location":loc,"revision":revision,"changed":true,"operation_ref":operation,"change":event,"current":reading,"automatic_agent_or_model_invocation":false}))
    })();
    unsafe { libc::unlinkat(parent.as_raw_fd(), c_staging.as_ptr(), 0); }
    if committed {
        result.map_err(|e: io::Error| io::Error::other(format!("File creation committed but readback or recovery finalization failed: {e}; retry the same operation identity, never overwrite or mint another identity automatically")))
    } else { result }
}

fn register_create(registry: &mut ActionRegistry) {
    let fields = [("parent","object",true),("name","string",true),("content","string",true),
        ("expected_absent","boolean",true),("operation_ref","string",true),("actor","string",true),
        ("actor_kind","string",true),("agent_session_ref","string",false)];
    registry.register(ActionDescriptor {
        id:"central.files.create".into(), title:"Create an ordinary file".into(),
        description:"Explicit first save into an existing Central directory, with atomic no-overwrite admission, native source/protected-ground guards, declared attribution, idempotent operation identity and owner history. Reading and creation do not publish or adopt source ownership.".into(),
        inputs:fields.into_iter().map(|(name,kind,required)|ActionInputDefinition{name:name.into(),input_type:kind.into(),required,choices:None,selection:None}).collect(),
        output:ActionOutputDefinition{output_type:"central-file-mutation".into()},mutation_class:MutationClass::LocallyMutating,
        preview_supported:false,required_ports:vec![],availability:ActionAvailability{available:true,reason:None},
    }, |_,input,context| {
        let result=resolve_central_root(context.root_options).map_err(io::Error::other).and_then(|root|create_ordinary(&root.path,input));
        match result {
            Ok(data)=>ActionResult::success("central.files.create",data),
            Err(e)=>ActionResult::failure(Some("central.files.create"),match e.kind(){
                io::ErrorKind::PermissionDenied=>ResultStatus::UnavailableCapability,
                io::ErrorKind::InvalidInput|io::ErrorKind::NotFound=>ResultStatus::InvalidInput,
                io::ErrorKind::AlreadyExists=>ResultStatus::VerificationFailure,
                _=>ResultStatus::InternalFailure,
            },e.to_string(),Some(json!({"outcome":if e.kind()==io::ErrorKind::AlreadyExists{"conflict"}else if e.kind()==io::ErrorKind::PermissionDenied{"refused"}else{"error"}}))),
        }
    }).expect("unique ordinary file creation Action");
}
