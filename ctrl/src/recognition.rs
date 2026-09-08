//! Read-only recognition of a directory explicitly selected by the caller.
//! This does not resolve, change, initialize, adopt or bind the active root.
use crate::action::*;
use crate::result::{ActionResult, ResultStatus};
use serde::Serialize;
use serde_json::Value;
use std::os::unix::{
    fs::{MetadataExt, OpenOptionsExt},
    io::{AsRawFd, FromRawFd},
};
use std::{
    fs::{File, OpenOptions},
    io,
    path::Path,
};
#[derive(Debug, Serialize)]
pub struct DirectoryIdentity {
    pub device: String,
    pub inode: String,
}
#[derive(Debug, Serialize)]
pub struct AccessReading {
    pub readable: bool,
    pub writable: bool,
    pub searchable: bool,
    pub read_only: bool,
    pub basis: String,
}
#[derive(Debug, Serialize)]
pub struct RecognitionCheck {
    pub path: String,
    pub status: String,
    pub detail: Option<String>,
}
#[derive(Debug, Serialize)]
pub struct RootRecognition {
    pub schema: String,
    pub requested_path: String,
    pub canonical_path: Option<String>,
    pub redirected: bool,
    pub outcome: String,
    pub identity: Option<DirectoryIdentity>,
    pub access: Option<AccessReading>,
    pub checks: Vec<RecognitionCheck>,
    pub limitations: Vec<String>,
    pub mutated: bool,
    pub bound: bool,
}
fn standing(e: &io::Error) -> &'static str {
    match e.kind() {
        io::ErrorKind::NotFound => "missing",
        io::ErrorKind::PermissionDenied => "inaccessible",
        _ => match e.raw_os_error() {
            Some(libc::ELOOP) => "redirected",
            Some(libc::ENOTDIR) => "not_directory",
            _ => "error",
        },
    }
}
fn inspect_member(root: &File, relative: &str) -> io::Result<()> {
    let mut parent = root.try_clone()?;
    for part in Path::new(relative).components() {
        let name = std::ffi::CString::new(part.as_os_str().as_encoded_bytes())
            .map_err(io::Error::other)?;
        // fstatat diagnoses a link before openat; openat remains the race-resistant
        // access check if the entry changes between these calls.
        let mut metadata = std::mem::MaybeUninit::<libc::stat>::uninit();
        if unsafe {
            libc::fstatat(
                parent.as_raw_fd(),
                name.as_ptr(),
                metadata.as_mut_ptr(),
                libc::AT_SYMLINK_NOFOLLOW,
            )
        } != 0
        {
            return Err(io::Error::last_os_error());
        }
        let metadata = unsafe { metadata.assume_init() };
        if metadata.st_mode & libc::S_IFMT == libc::S_IFLNK {
            return Err(io::Error::from_raw_os_error(libc::ELOOP));
        }
        let fd = unsafe {
            libc::openat(
                parent.as_raw_fd(),
                name.as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW,
            )
        };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        parent = unsafe { File::from_raw_fd(fd) };
    }
    Ok(())
}
pub fn recognize(path: &Path) -> io::Result<RootRecognition> {
    let raw = path.to_str().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "Chosen directory path must be UTF-8",
        )
    })?;
    if !path.is_absolute() || raw.len() > 16384 || raw.contains('\0') {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Recognition requires one explicit bounded absolute path",
        ));
    }
    let mut result=RootRecognition{schema:"central.root-recognition/v1".into(),requested_path:raw.into(),canonical_path:None,redirected:false,outcome:"unrecognized".into(),identity:None,access:None,checks:Vec::new(),limitations:vec!["Recognition checks the six native structural directories; it does not adopt files, validate every project or read source bodies.".into(),"Access is a read-only OS permission observation, not a mutation-authority grant or a guarantee of a later write. Binding must revalidate the canonical directory identity.".into()],mutated:false,bound:false};
    let canonical = match path.canonicalize() {
        Ok(p) => p,
        Err(e) => {
            result.outcome = standing(&e).into();
            result.limitations.push(e.to_string());
            return Ok(result);
        }
    };
    result.redirected = canonical != path;
    result.canonical_path = Some(
        canonical
            .to_str()
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Canonical directory path is not UTF-8",
                )
            })?
            .into(),
    );
    let metadata = match std::fs::symlink_metadata(&canonical) {
        Ok(m) => m,
        Err(e) => {
            result.outcome = standing(&e).into();
            result.limitations.push(e.to_string());
            return Ok(result);
        }
    };
    if !metadata.is_dir() {
        result.outcome = "not_directory".into();
        return Ok(result);
    }
    result.identity = Some(DirectoryIdentity {
        device: metadata.dev().to_string(),
        inode: metadata.ino().to_string(),
    });
    let name = std::ffi::CString::new(canonical.as_os_str().as_encoded_bytes())
        .map_err(io::Error::other)?;
    let access = |mode| unsafe { libc::access(name.as_ptr(), mode) == 0 };
    let readable = access(libc::R_OK);
    let writable = access(libc::W_OK);
    let searchable = access(libc::X_OK);
    result.access = Some(AccessReading {
        readable,
        writable,
        searchable,
        read_only: readable && searchable && !writable,
        basis: "posix-access-current-process".into(),
    });
    let root = match OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW)
        .open(&canonical)
    {
        Ok(f) => f,
        Err(e) => {
            result.outcome = standing(&e).into();
            result.limitations.push(e.to_string());
            return Ok(result);
        }
    };
    if !readable || !searchable {
        result.outcome = "inaccessible".into();
        return Ok(result);
    }
    for member in crate::root::REQUIRED_DIRECTORIES {
        let check = match inspect_member(&root, member) {
            Ok(()) => RecognitionCheck {
                path: member.into(),
                status: "present".into(),
                detail: None,
            },
            Err(e) => RecognitionCheck {
                path: member.into(),
                status: standing(&e).into(),
                detail: Some(e.to_string()),
            },
        };
        result.checks.push(check);
    }
    result.outcome = if result.checks.iter().all(|c| c.status == "present") {
        "recognized"
    } else if result.checks.iter().any(|c| c.status == "inaccessible") {
        "inaccessible"
    } else {
        "unrecognized"
    }
    .into();
    let held = root.metadata()?;
    let unchanged = std::fs::symlink_metadata(&canonical)
        .map(|m| {
            m.is_dir()
                && m.dev() == held.dev()
                && m.ino() == held.ino()
                && m.dev() == metadata.dev()
                && m.ino() == metadata.ino()
        })
        .unwrap_or(false);
    if !unchanged {
        result.outcome = "changed".into();
        result.limitations.push("Chosen directory changed during recognition; reselect or inspect again before binding.".into());
    }
    Ok(result)
}
fn action(_: &ActionRegistry, input: &Value, _: &ActionExecutionContext<'_>) -> ActionResult {
    let result = input
        .get("path")
        .and_then(Value::as_str)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "path is required"))
        .and_then(|p| recognize(Path::new(p)));
    match result {
        Ok(reading) => ActionResult::success(
            "central.recognize",
            serde_json::to_value(reading).expect("recognition serializes"),
        ),
        Err(e) => ActionResult::failure(
            Some("central.recognize"),
            ResultStatus::InvalidInput,
            e.to_string(),
            None,
        ),
    }
}
pub(crate) fn register(registry: &mut ActionRegistry) {
    registry.register(ActionDescriptor{id:"central.recognize".into(),title:"Recognize chosen Central directory".into(),description:"Inspect one explicitly chosen absolute directory without source reads, initialization, adoption, moves or active-root binding.".into(),inputs:vec![ActionInputDefinition{name:"path".into(),input_type:"string".into(),required:true,choices:None,selection:None}],output:ActionOutputDefinition{output_type:"central-root-recognition".into()},mutation_class:MutationClass::ReadOnly,preview_supported:false,required_ports:vec![],availability:ActionAvailability{available:true,reason:None}},action).expect("Unique recognition Action");
}
