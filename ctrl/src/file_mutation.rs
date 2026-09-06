//! Ordinary-file owner CAS and recovery. Participating sources never enter this
//! route. History is owner-kept recovery material, not authored ground or git.
use crate::{
    action::*,
    files::{participating_source, read_file, CentralPathRef},
    projectcentral_flow::{content_revision_bytes, reject_symlink_components},
    result::{ActionResult, ResultStatus},
    root::resolve_central_root,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::os::unix::{
    fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    io::{AsRawFd, FromRawFd},
};
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
};
const MAX: usize = 4 * 1024 * 1024;
fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}
fn denied(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::PermissionDenied, message)
}
fn conflict(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::AlreadyExists, message)
}
fn open(path: &Path, write: bool) -> io::Result<File> {
    OpenOptions::new()
        .read(true)
        .write(write)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
}
fn bytes(file: &mut File) -> io::Result<Vec<u8>> {
    let mut b = Vec::new();
    file.take((MAX + 1) as u64).read_to_end(&mut b)?;
    if b.len() > MAX {
        return Err(invalid("File exceeds bounded text size"));
    }
    Ok(b)
}
pub(crate) fn directory(root: &Path, relative: &Path) -> io::Result<File> {
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_DIRECTORY)
        .open(root)?;
    for part in relative.components() {
        if !matches!(part, std::path::Component::Normal(_)) {
            return Err(invalid("Invalid parent component"));
        }
        let name = std::ffi::CString::new(part.as_os_str().as_encoded_bytes())
            .map_err(io::Error::other)?;
        let fd = unsafe {
            libc::openat(
                file.as_raw_fd(),
                name.as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW,
            )
        };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        file = unsafe { File::from_raw_fd(fd) };
    }
    Ok(file)
}
pub(crate) fn open_native_file(root: &Path, relative: &str) -> io::Result<File> {
    let path = Path::new(relative);
    let parent = directory(
        root,
        path.parent().ok_or_else(|| invalid("File has no parent"))?,
    )?;
    let name = std::ffi::CString::new(
        path.file_name()
            .ok_or_else(|| invalid("File has no name"))?
            .as_encoded_bytes(),
    )
    .map_err(io::Error::other)?;
    let fd = unsafe {
        libc::openat(
            parent.as_raw_fd(),
            name.as_ptr(),
            libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_NONBLOCK,
        )
    };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(unsafe { File::from_raw_fd(fd) })
}
pub(crate) fn rename_in(parent: &File, from: &str, to: &str) -> io::Result<()> {
    let from = std::ffi::CString::new(from).map_err(io::Error::other)?;
    let to = std::ffi::CString::new(to).map_err(io::Error::other)?;
    if unsafe {
        libc::renameat(
            parent.as_raw_fd(),
            from.as_ptr(),
            parent.as_raw_fd(),
            to.as_ptr(),
        )
    } != 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}
pub(crate) fn create_in(parent: &File, name: &str, mode: u32) -> io::Result<File> {
    let name = std::ffi::CString::new(name).map_err(io::Error::other)?;
    let fd = unsafe {
        libc::openat(
            parent.as_raw_fd(),
            name.as_ptr(),
            libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW,
            mode as libc::c_uint,
        )
    };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(unsafe { File::from_raw_fd(fd) })
}
pub(crate) fn ordinary_address(root: &Path, loc: &CentralPathRef) -> io::Result<PathBuf> {
    let path = loc.resolve(root)?;
    let components: Vec<_> = Path::new(&loc.path)
        .components()
        .map(|p| p.as_os_str().to_string_lossy().into_owned())
        .collect();
    if components.first().is_some_and(|p| p == "Control")
        || components
            .iter()
            .any(|p| matches!(p.as_str(), "ProjectCentral" | ".central" | ".git"))
    {
        return Err(denied("Protected ground or owner state must use its native authored operation; ordinary file writes are unavailable"));
    }
    // Do not turn malformed participating-project ground into an ordinary file
    // simply because the discovery convenience reader returns no binding.
    if components.len() > 2 && components[0] == "Work" {
        let project = root.join("Work").join(&components[1]);
        if project.join("ProjectCentral").exists() {
            let manifest = crate::projectcentral::read_project_manifest(&project)?;
            if !manifest.validate().valid {
                return Err(denied("Project identity is invalid; ordinary write cannot infer absence of source participation"));
            }
            crate::source_horizon::project_source_bindings(&project)?;
        }
    }
    if participating_source(root, &loc.path).is_some() {
        return Err(denied("Participating SourceRef must use projectcentral.source operations; filesystem identity cannot bypass source authority"));
    }
    Ok(path)
}
fn ordinary(root: &Path, loc: &CentralPathRef) -> io::Result<PathBuf> {
    let path = ordinary_address(root, loc)?;
    let reading = read_file(root, loc)?;
    if reading.source.is_some() {
        return Err(denied("Participating source requires source authority"));
    }
    Ok(path)
}
struct Lock {
    file: File,
}
impl Drop for Lock {
    fn drop(&mut self) {
        unsafe {
            libc::flock(self.file.as_raw_fd(), libc::LOCK_UN);
        }
    }
}
fn state(root: &Path) -> io::Result<(PathBuf, Lock)> {
    let dir = root.join(".central/file-history");
    for p in [root.join(".central"), dir.clone()] {
        match fs::create_dir(&p) {
            Ok(()) => fs::set_permissions(&p, fs::Permissions::from_mode(0o700))?,
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e),
        };
        if !fs::symlink_metadata(&p)?.file_type().is_dir() {
            return Err(denied("Owner history directory redirected"));
        }
    }
    reject_symlink_components(root, Path::new(".central/file-history"))?;
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(dir.join("lock"))?;
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) } != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok((dir, Lock { file }))
}
fn key(text: &[u8]) -> String {
    content_revision_bytes(text).replace([':', '/'], "_")
}
// The owner lock serializes this directory. Only a fully fsynced record becomes
// visible under its durable name, so process death never leaves partial JSON.
pub(crate) fn atomic_record(path: &Path, data: &[u8]) -> io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| invalid("Record has no parent"))?;
    let tmp = parent.join("record-staging");
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(&tmp)?;
    file.write_all(data)?;
    file.sync_all()?;
    fs::rename(&tmp, path)?;
    File::open(parent)?.sync_all()
}
fn area(dir: &Path, loc: &CentralPathRef) -> io::Result<PathBuf> {
    let area = dir.join(key(loc.ref_id.as_bytes()));
    match fs::create_dir(&area) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(e),
    }
    if !fs::symlink_metadata(&area)?.is_dir()
        || fs::symlink_metadata(&area)?.file_type().is_symlink()
    {
        return Err(denied("History address redirected"));
    }
    let identity = area.join("identity.json");
    let data = serde_json::to_vec(loc)?;
    if identity.exists() {
        if bytes(&mut open(&identity, false)?)? != data {
            return Err(invalid("History identity collision"));
        }
    } else {
        atomic_record(&identity, &data)?;
    }
    Ok(area)
}
#[derive(Debug, Serialize, Deserialize)]
struct Snapshot {
    revision: String,
    content: String,
}
fn snapshot(area: &Path, content: &str) -> io::Result<String> {
    let revision = content_revision_bytes(content.as_bytes());
    let path = area.join(format!("{}.json", key(revision.as_bytes())));
    let value = Snapshot {
        revision: revision.clone(),
        content: content.into(),
    };
    let data = serde_json::to_vec(&value)?;
    if path.exists() {
        let existing: Snapshot =
            serde_json::from_reader(open(&path, false)?.take((MAX * 6 + 1024) as u64))?;
        if existing.revision != revision || existing.content != content {
            return Err(invalid("History revision collision"));
        }
    } else {
        atomic_record(&path, &data)?;
    }
    Ok(revision)
}
fn historical(area: &Path, revision: &str) -> io::Result<String> {
    let value: Snapshot = serde_json::from_reader(
        open(
            &area.join(format!("{}.json", key(revision.as_bytes()))),
            false,
        )?
        .take((MAX * 6 + 1024) as u64),
    )?;
    if value.revision != revision || content_revision_bytes(value.content.as_bytes()) != revision {
        return Err(invalid("History revision is corrupt"));
    }
    Ok(value.content)
}
#[derive(Debug, Serialize, Deserialize)]
struct Change {
    cursor: u64,
    previous_revision: String,
    revision: String,
    actor: String,
    actor_kind: String,
    agent_session_ref: Option<String>,
    restored_from: Option<String>,
}
fn events(area: &Path, limit: usize, before: Option<u64>) -> io::Result<Vec<Change>> {
    let mut selected = BTreeMap::new();
    for entry in fs::read_dir(area)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if let Some(id) = name
            .strip_prefix("event-")
            .and_then(|s| s.strip_suffix(".json"))
            .and_then(|s| s.parse::<u64>().ok())
        {
            if before.is_some_and(|b| id >= b) {
                continue;
            }
            selected.insert(id, entry.path());
            if selected.len() > limit {
                selected.pop_first();
            }
        }
    }
    selected
        .into_iter()
        .rev()
        .map(|(cursor, path)| {
            let item: Change = serde_json::from_reader(open(&path, false)?.take(65536))?;
            if item.cursor != cursor {
                return Err(invalid("History cursor mismatch"));
            }
            Ok(item)
        })
        .collect()
}
fn text<'a>(input: &'a Value, name: &str) -> io::Result<&'a str> {
    input
        .get(name)
        .and_then(Value::as_str)
        .ok_or_else(|| invalid(&format!("{name} must be a string")))
}
fn attribution(input: &Value) -> io::Result<(String, String, Option<String>)> {
    let actor = text(input, "actor")?;
    if actor.trim().is_empty() || actor.len() > 1024 {
        return Err(invalid("actor must be bounded and nonempty"));
    }
    let kind = text(input, "actor_kind")?;
    crate::projectcentral_flow::validate_actor_kind(kind)?;
    let session = input
        .get("agent_session_ref")
        .filter(|s| !s.is_null())
        .map(|s| {
            s.as_str()
                .ok_or_else(|| invalid("agent_session_ref must be a string"))
        })
        .transpose()?;
    if session.is_some_and(|s| s.is_empty() || s.len() > 4096) {
        return Err(invalid("agent_session_ref must be nonempty and bounded"));
    }
    if kind == "human" && session.is_some() {
        return Err(invalid("Human attribution cannot carry an agent session"));
    }
    Ok((actor.into(), kind.into(), session.map(str::to_owned)))
}
fn execute(root: &Path, op: &str, input: &Value) -> io::Result<Value> {
    let root = root.canonicalize()?;
    let loc: CentralPathRef =
        serde_json::from_value(input.get("location").cloned().unwrap_or(Value::Null))?;
    let path = ordinary(&root, &loc)?;
    if op == "history"
        && !root
            .join(".central/file-history")
            .join(key(loc.ref_id.as_bytes()))
            .exists()
    {
        return Ok(
            json!({"schema":"central.file-history/v1","location":loc,"current_revision":read_file(&root,&loc)?.revision,"entries":[],"next_before":null,"more":false,"automatic_agent_or_model_invocation":false}),
        );
    }
    if op == "recovery_preview"
        && !root
            .join(".central/file-history")
            .join(key(loc.ref_id.as_bytes()))
            .exists()
    {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "No recorded ordinary-file history",
        ));
    }
    let (dir, _lock) = state(&root)?;
    let area = area(&dir, &loc)?;
    // Revalidate after the cross-process owner lock: another writer may have
    // changed the source or introduced authored participation while waiting.
    ordinary(&root, &loc)?;
    let current = read_file(&root, &loc)?;
    let pending = area.join("pending.json");
    if pending.exists() {
        let event: Change = serde_json::from_reader(open(&pending, false)?.take(65536))?;
        if current.revision == event.revision {
            fs::rename(&pending, area.join(format!("event-{}.json", event.cursor)))?;
            File::open(&area)?.sync_all()?;
        } else if current.revision == event.previous_revision {
            fs::remove_file(&pending)?;
        } else {
            return Err(io::Error::other("File commit has an unresolved interrupted receipt; current bytes match neither journal basis nor target. Owner recovery is required; do not resend."));
        }
    }
    if op == "history" {
        let limit = input
            .get("limit")
            .map(|v| {
                v.as_u64()
                    .ok_or_else(|| invalid("limit must be an integer"))
            })
            .transpose()?
            .unwrap_or(20);
        if limit == 0 || limit > 200 {
            return Err(invalid("limit must be 1..200"));
        }
        let before = input
            .get("before")
            .map(|v| {
                v.as_u64()
                    .ok_or_else(|| invalid("before must be an integer"))
            })
            .transpose()?;
        let mut page = events(&area, limit as usize + 1, before)?;
        let more = page.len() > limit as usize;
        page.truncate(limit as usize);
        let next = if more {
            page.last().map(|c| c.cursor)
        } else {
            None
        };
        return Ok(
            json!({"schema":"central.file-history/v1","location":loc,"current_revision":current.revision,"entries":page,"next_before":next,"more":more,"automatic_agent_or_model_invocation":false}),
        );
    }
    let basis = text(input, "expected_revision")?;
    if basis != current.revision {
        return Ok(
            json!({"schema":"central.file-mutation/v1","outcome":"conflict","location":loc,"expected_revision":basis,"current":current,"changed":false}),
        );
    }
    let restored = if op == "restore" || op == "recovery_preview" {
        Some(text(input, "revision")?)
    } else {
        None
    };
    let content = match restored {
        Some(revision) => historical(&area, revision)?,
        None => text(input, "content")?.into(),
    };
    if content.len() > MAX || content.contains('\0') {
        return Err(invalid("Content must be bounded UTF-8 text without NUL"));
    }
    let revision = content_revision_bytes(content.as_bytes());
    if op == "recovery_preview" {
        return Ok(
            json!({"schema":"central.file-recovery-preview/v1","outcome":"preview","location":loc,"expected_revision":basis,"revision":revision,"content":content,"current_content":current.content,"changed":content!=current.content,"automatic_agent_or_model_invocation":false}),
        );
    }
    let (actor, actor_kind, agent_session_ref) = attribution(input)?;
    if content == current.content {
        return Ok(
            json!({"schema":"central.file-mutation/v1","outcome":"unchanged","location":loc,"previous_revision":basis,"revision":revision,"changed":false}),
        );
    }
    let mut original = open_native_file(&root, &loc.path)?;
    let meta = original.metadata()?;
    if meta.permissions().readonly() {
        return Err(denied("File permissions are read-only"));
    }
    if meta.nlink() != 1 {
        return Err(denied(
            "Files with multiple hard links require an explicit native operation",
        ));
    }
    if bytes(&mut original)? != current.content.as_bytes() {
        return Err(conflict("File changed before commit"));
    }
    snapshot(&area, &current.content)?;
    snapshot(&area, &content)?;
    let relative_parent = Path::new(&loc.path)
        .parent()
        .ok_or_else(|| invalid("Missing parent"))?;
    let parent = directory(&root, relative_parent)?;
    let name = format!(
        ".central-write-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(io::Error::other)?
            .as_nanos()
    );
    let mut staged = create_in(&parent, &name, meta.mode() & 0o777)?;
    let cursor = events(&area, 1, None)?
        .first()
        .map(|e| e.cursor)
        .unwrap_or(0)
        .checked_add(1)
        .ok_or_else(|| invalid("History cursor exhausted"))?;
    let event = Change {
        cursor,
        previous_revision: basis.into(),
        revision: revision.clone(),
        actor,
        actor_kind,
        agent_session_ref,
        restored_from: restored.map(str::to_owned),
    };
    let mut committed = false;
    let result = (|| {
        #[cfg(target_os = "macos")]
        {
            unsafe extern "C" {
                fn fcopyfile(
                    from: libc::c_int,
                    to: libc::c_int,
                    state: *mut libc::c_void,
                    flags: libc::c_uint,
                ) -> libc::c_int;
            }
            // COPYFILE_METADATA = ACL | STAT | XATTR. Source data is deliberately
            // excluded: the staged content is the caller's explicit CAS target.
            if unsafe {
                fcopyfile(
                    original.as_raw_fd(),
                    staged.as_raw_fd(),
                    std::ptr::null_mut(),
                    7,
                )
            } != 0
            {
                return Err(io::Error::last_os_error());
            }
        }
        staged.set_permissions(meta.permissions())?;
        staged.write_all(content.as_bytes())?;
        staged.sync_all()?;
        ordinary(&root, &loc)?;
        let now = open_native_file(&root, &loc.path)?.metadata()?;
        if now.dev() != meta.dev()
            || now.ino() != meta.ino()
            || read_file(&root, &loc)?.revision != basis
        {
            return Err(conflict("File changed during commit"));
        }
        let check_parent = directory(&root, relative_parent)?.metadata()?;
        let held_parent = parent.metadata()?;
        if check_parent.dev() != held_parent.dev() || check_parent.ino() != held_parent.ino() {
            return Err(conflict("Parent directory changed during commit"));
        }
        atomic_record(&pending, &serde_json::to_vec(&event)?)?;
        // The journal fsync can yield to an external editor. Re-read after it
        // before publishing the staged file; owner writers are already locked.
        let final_meta = open_native_file(&root, &loc.path)?.metadata()?;
        if final_meta.dev() != meta.dev()
            || final_meta.ino() != meta.ino()
            || read_file(&root, &loc)?.revision != basis
        {
            fs::remove_file(&pending)?;
            return Err(conflict("File changed while preparing durable receipt"));
        }

        rename_in(
            &parent,
            &name,
            path.file_name()
                .and_then(|s| s.to_str())
                .ok_or_else(|| invalid("Invalid filename"))?,
        )?;
        committed = true;
        parent.sync_all()?;
        fs::rename(&pending, area.join(format!("event-{cursor}.json")))?;
        File::open(&area)?.sync_all()?;
        Ok(
            json!({"schema":"central.file-mutation/v1","outcome":"written","location":loc,"previous_revision":basis,"revision":revision,"changed":true,"change":event,"automatic_agent_or_model_invocation":false}),
        )
    })();
    let c_name = std::ffi::CString::new(name).map_err(io::Error::other)?;
    unsafe {
        libc::unlinkat(parent.as_raw_fd(), c_name.as_ptr(), 0);
    }
    if committed {
        result.map_err(|e:io::Error|io::Error::other(format!("File was committed but durable receipt finalization failed: {e}; do not automatically resend. Next owner read reconciles the journal.")))
    } else {
        result
    }
}
fn action(op: &str, id: &str, input: &Value, context: &ActionExecutionContext<'_>) -> ActionResult {
    let result = resolve_central_root(context.root_options)
        .map_err(io::Error::other)
        .and_then(|r| execute(&r.path, op, input));
    match result {
        Ok(data) => ActionResult::success(id, data),
        Err(e) => ActionResult::failure(
            Some(id),
            match e.kind() {
                io::ErrorKind::PermissionDenied => ResultStatus::UnavailableCapability,
                io::ErrorKind::InvalidInput | io::ErrorKind::NotFound => ResultStatus::InvalidInput,
                io::ErrorKind::AlreadyExists | io::ErrorKind::InvalidData => {
                    ResultStatus::VerificationFailure
                }
                _ => ResultStatus::InternalFailure,
            },
            e.to_string(),
            Some(
                json!({"outcome":if e.kind()==io::ErrorKind::PermissionDenied {"refused"}else if e.kind()==io::ErrorKind::AlreadyExists {"conflict"}else{"error"}}),
            ),
        ),
    }
}
pub fn register(registry: &mut ActionRegistry) {
    type Handler = fn(&ActionRegistry, &Value, &ActionExecutionContext<'_>) -> ActionResult;
    for (op, handler) in [
        (
            "write",
            (|_, i, c| action("write", "central.files.write", i, c)) as Handler,
        ),
        (
            "history",
            (|_, i, c| action("history", "central.files.history", i, c)) as Handler,
        ),
        (
            "restore",
            (|_, i, c| action("restore", "central.files.restore", i, c)) as Handler,
        ),
        (
            "recovery_preview",
            (|_, i, c| action("recovery_preview", "central.files.recovery_preview", i, c))
                as Handler,
        ),
    ] {
        let mut inputs = vec![ActionInputDefinition {
            name: "location".into(),
            input_type: "object".into(),
            required: true,
            choices: None,
            selection: None,
        }];
        let fields: Vec<(&str, &str, bool)> = match op {
            "history" => vec![("limit", "integer", false), ("before", "integer", false)],
            "recovery_preview" => vec![
                ("expected_revision", "string", true),
                ("revision", "string", true),
            ],
            _ => vec![
                ("expected_revision", "string", true),
                (
                    if op == "write" { "content" } else { "revision" },
                    "string",
                    true,
                ),
                ("actor", "string", true),
                ("actor_kind", "string", true),
                ("agent_session_ref", "string", false),
            ],
        };
        inputs.extend(
            fields
                .into_iter()
                .map(|(name, kind, required)| ActionInputDefinition {
                    name: name.into(),
                    input_type: kind.into(),
                    required,
                    choices: None,
                    selection: None,
                }),
        );
        registry.register(ActionDescriptor{id:format!("central.files.{op}"),title:format!("Ordinary file {op}"),description:"Native ordinary-file history and CAS. Protected ground and participating SourceRefs must use their authored operations. Attribution is declared, not an authentication credential.".into(),inputs,output:ActionOutputDefinition{output_type:format!("central-file-{op}")},mutation_class:if op=="history" || op=="recovery_preview" {MutationClass::ReadOnly}else{MutationClass::LocallyMutating},preview_supported:false,required_ports:vec![],availability:ActionAvailability{available:true,reason:None}},handler).expect("unique file mutation action");
    }
}
