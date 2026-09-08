//! Native filesystem reading within the configured Central root. Locations are
//! owner-resolved filesystem addresses, never Project/World or authored Source
//! identities. Reading does not adopt files into ProjectCentral or a horizon.
use crate::action::{
    ActionAvailability, ActionDescriptor, ActionExecutionContext, ActionInputDefinition,
    ActionOutputDefinition, ActionRegistry, MutationClass,
};
use crate::projectcentral_flow::{content_revision_bytes, reject_symlink_components};
use crate::result::{ActionResult, ResultStatus};
use crate::root::resolve_central_root;
use crate::source_horizon::retrieval_allowed;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    fs,
    io::{self, Read},
    path::{Component, Path, PathBuf},
};

const MAX_TEXT_BYTES: u64 = 4 * 1024 * 1024;
const MAX_MATERIAL_BYTES: u64 = 32 * 1024 * 1024;
const MAX_DIRECTORY_ENTRIES: usize = 20_000;

/// The requested reading shape for `central.files.read`. Defaults to
/// `Utf8`, which preserves the original text-only contract exactly.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileEncoding {
    Utf8,
    Base64,
}
impl FileEncoding {
    fn parse(value: Option<&Value>) -> io::Result<Self> {
        match value {
            None | Some(Value::Null) => Ok(FileEncoding::Utf8),
            Some(Value::String(text)) if text == "utf-8" => Ok(FileEncoding::Utf8),
            Some(Value::String(text)) if text == "base64" => Ok(FileEncoding::Base64),
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "encoding must be \"utf-8\" or \"base64\"",
            )),
        }
    }
}
/// Owner sniff of material kind from magic bytes, falling back to a small
/// extension allowlist. Never trusted for security decisions, only for
/// disclosing a rendering hint to consumers.
fn sniff_mime(bytes: &[u8], relative_path: &str) -> Option<String> {
    if bytes.starts_with(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]) {
        return Some("image/png".into());
    }
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Some("image/jpeg".into());
    }
    if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        return Some("image/gif".into());
    }
    if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        return Some("image/webp".into());
    }
    if bytes.starts_with(b"%PDF-") {
        return Some("application/pdf".into());
    }
    let extension = Path::new(relative_path)
        .extension()?
        .to_str()?
        .to_lowercase();
    match extension.as_str() {
        "html" | "htm" => Some("text/html".into()),
        "md" => Some("text/markdown".into()),
        "svg" => Some("image/svg+xml".into()),
        "css" => Some("text/css".into()),
        "js" => Some("text/javascript".into()),
        "json" => Some("application/json".into()),
        "txt" => Some("text/plain".into()),
        _ => None,
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CentralPathRef {
    pub schema: String,
    #[serde(rename = "ref")]
    pub ref_id: String,
    pub root: String,
    pub path: String,
}
impl CentralPathRef {
    fn new(root: &Path, path: String) -> io::Result<Self> {
        Ok(Self {
            schema: "central.path-ref/v1".into(),
            ref_id: format!("central:path:{}:{}", escape(utf8(root)?), escape(&path)),
            root: utf8(root)?.into(),
            path,
        })
    }
    fn resolve(&self, configured: &Path) -> io::Result<PathBuf> {
        let root = configured.canonicalize()?;
        if *self != Self::new(&root, self.path.clone())? {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Central location belongs to another root or an unsupported schema",
            ));
        }
        if self.path.is_empty() {
            return Ok(root);
        }
        let relative = Path::new(&self.path);
        if relative.is_absolute()
            || !relative
                .components()
                .all(|part| matches!(part, Component::Normal(_)))
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Central location must contain only relative path components",
            ));
        }
        reject_symlink_components(&root, &relative)?;
        let path = root.join(relative);
        if path.canonicalize()? != path {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Central location has redirected",
            ));
        }
        Ok(path)
    }
}
#[derive(Clone, Debug, Serialize)]
pub struct FileEntry {
    pub name: String,
    pub location: CentralPathRef,
    pub kind: String,
    pub byte_len: u64,
    pub retrieval_allowed: bool,
}
#[derive(Debug, Serialize)]
pub struct DirectoryReading {
    pub schema: String,
    pub location: CentralPathRef,
    pub entries: Vec<FileEntry>,
    pub automatic_agent_or_model_invocation: bool,
}
#[derive(Debug, Serialize)]
pub struct FileReading {
    pub schema: String,
    pub location: CentralPathRef,
    pub revision: String,
    pub byte_len: u64,
    pub content_encoding: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_hint: Option<String>,
    pub project: Option<FileProject>,
    pub source: Option<crate::source_horizon::SourceBinding>,
    pub automatic_agent_or_model_invocation: bool,
}
#[derive(Debug, Serialize)]
pub struct FileProject {
    pub name: String,
    pub path: String,
    pub project_ref: Option<String>,
}
fn escape(text: &str) -> String {
    text.bytes()
        .map(|byte| {
            if byte.is_ascii_alphanumeric() || b"/-_.".contains(&byte) {
                char::from(byte).to_string()
            } else {
                format!("%{byte:02X}")
            }
        })
        .collect()
}
fn file_project(root: &Path, relative: &str) -> Option<FileProject> {
    let mut parts = Path::new(relative).components();
    if parts.next()?.as_os_str() != "Work" {
        return None;
    }
    let name = parts.next()?.as_os_str().to_str()?.to_owned();
    let path = format!("Work/{name}");
    let project_root = root.join(&path);
    if !project_root.is_dir() {
        return None;
    }
    let project_ref = crate::projectcentral::read_project_manifest(&project_root)
        .ok()
        .filter(|manifest| manifest.validate().valid)
        .map(|manifest| manifest.project_id);
    Some(FileProject {
        name,
        path,
        project_ref,
    })
}
fn participating_source(
    root: &Path,
    relative: &str,
) -> Option<crate::source_horizon::SourceBinding> {
    let project = file_project(root, relative)?;
    project.project_ref.as_ref()?;
    let within_project = Path::new(relative)
        .strip_prefix(&project.path)
        .ok()?
        .to_str()?;
    crate::source_horizon::project_source_bindings(&root.join(&project.path))
        .ok()?
        .into_iter()
        .find(|binding| binding.path == within_project)
}
fn utf8(path: &Path) -> io::Result<&str> {
    path.to_str().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "Filesystem path cannot be represented as UTF-8",
        )
    })
}
fn require_retrieval(root: &Path, path: &Path, directory: bool) -> io::Result<()> {
    if !retrieval_allowed(root, path) || (directory && path.join(".no-agent-retrieval").is_file()) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "This location is excluded by .no-agent-retrieval",
        ));
    }
    Ok(())
}
pub fn list_files(configured: &Path, relative: &str) -> io::Result<DirectoryReading> {
    let root = configured.canonicalize()?;
    let location = CentralPathRef::new(&root, relative.into())?;
    let path = location.resolve(&root)?;
    if !path.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Central listing requires a directory",
        ));
    }
    require_retrieval(&root, &path, true)?;
    let mut entries = Vec::new();
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        if entries.len() == MAX_DIRECTORY_ENTRIES {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "Directory exceeds the bounded listing size; no partial listing is presented as complete"));
        }
        let metadata = fs::symlink_metadata(entry.path())?;
        let name = entry.file_name().into_string().map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "Directory contains a non-UTF-8 filename",
            )
        })?;
        let kind = if metadata.file_type().is_symlink() {
            "symlink"
        } else if metadata.is_dir() {
            "directory"
        } else if metadata.is_file() {
            "file"
        } else {
            "other"
        };
        let relative = entry
            .path()
            .strip_prefix(&root)
            .map_err(io::Error::other)?
            .to_path_buf();
        entries.push(FileEntry {
            name,
            location: CentralPathRef::new(&root, utf8(&relative)?.into())?,
            kind: kind.into(),
            byte_len: metadata.len(),
            retrieval_allowed: kind != "symlink"
                && retrieval_allowed(&root, &entry.path())
                && !(metadata.is_dir() && entry.path().join(".no-agent-retrieval").is_file()),
        });
    }
    entries.sort_by(|a, b| (a.kind != "directory", &a.name).cmp(&(b.kind != "directory", &b.name)));
    Ok(DirectoryReading {
        schema: "central.directory-reading/v1".into(),
        location,
        entries,
        automatic_agent_or_model_invocation: false,
    })
}
pub fn read_file(
    configured: &Path,
    location: &CentralPathRef,
    encoding: FileEncoding,
) -> io::Result<FileReading> {
    let root = configured.canonicalize()?;
    let path = location.resolve(&root)?;
    require_retrieval(&root, &path, false)?;
    if !fs::symlink_metadata(&path)?.file_type().is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Central text reading requires a regular file",
        ));
    }
    let ceiling = match encoding {
        FileEncoding::Utf8 => MAX_TEXT_BYTES,
        FileEncoding::Base64 => MAX_MATERIAL_BYTES,
    };
    let file = fs::File::open(path)?;
    let mut bytes = Vec::new();
    file.take(ceiling + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > ceiling {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            match encoding {
                FileEncoding::Utf8 => "File exceeds the bounded text read size",
                FileEncoding::Base64 => "File exceeds the bounded material read size",
            },
        ));
    }
    let revision = content_revision_bytes(&bytes);
    let byte_len = bytes.len() as u64;
    let (content_encoding, content, mime_hint) = match encoding {
        FileEncoding::Utf8 => {
            let content = String::from_utf8(bytes).map_err(|_| {
                io::Error::new(io::ErrorKind::InvalidData, "File is not UTF-8 text")
            })?;
            if content.contains('\0') {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "File contains binary data",
                ));
            }
            ("utf-8".to_string(), content, None)
        }
        FileEncoding::Base64 => {
            let mime_hint = sniff_mime(&bytes, &location.path);
            use base64::Engine;
            let content = base64::engine::general_purpose::STANDARD.encode(&bytes);
            ("base64".to_string(), content, mime_hint)
        }
    };
    Ok(FileReading {
        schema: "central.file-reading/v1".into(),
        location: location.clone(),
        revision,
        byte_len,
        content_encoding,
        content,
        mime_hint,
        project: file_project(&root, &location.path),
        source: participating_source(&root, &location.path),
        automatic_agent_or_model_invocation: false,
    })
}
fn run(input: &Value, context: &ActionExecutionContext<'_>, read: bool) -> io::Result<Value> {
    let root = resolve_central_root(context.root_options)
        .map_err(io::Error::other)?
        .path;
    if read {
        let location: CentralPathRef =
            serde_json::from_value(input.get("location").cloned().unwrap_or(Value::Null))
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
        let encoding = FileEncoding::parse(input.get("encoding"))?;
        serde_json::to_value(read_file(&root, &location, encoding)?).map_err(io::Error::other)
    } else {
        let path = match input.get("path") {
            None => "",
            Some(Value::String(path)) => path,
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "path must be a string",
                ))
            }
        };
        serde_json::to_value(list_files(&root, path)?).map_err(io::Error::other)
    }
}
fn result(input: &Value, context: &ActionExecutionContext<'_>, read: bool) -> ActionResult {
    let action = if read {
        "central.files.read"
    } else {
        "central.files.list"
    };
    match run(input, context, read) {
        Ok(data) => ActionResult::success(action, data),
        Err(error) => ActionResult::failure(
            Some(action),
            match error.kind() {
                io::ErrorKind::PermissionDenied => ResultStatus::UnavailableCapability,
                io::ErrorKind::InvalidData => ResultStatus::VerificationFailure,
                io::ErrorKind::InvalidInput | io::ErrorKind::NotFound => ResultStatus::InvalidInput,
                _ => ResultStatus::InternalFailure,
            },
            error.to_string(),
            None,
        ),
    }
}
pub fn register_file_actions(registry: &mut ActionRegistry) {
    for (id, title, read) in [
        ("central.files.list", "List native Central directory", false),
        ("central.files.read", "Read native Central file", true),
    ] {
        let descriptor = ActionDescriptor {
            id: id.into(), title: title.into(), description: "Read actual filesystem material under the configured Central root without adoption, semantic identity promotion or source mutation. Symlink traversal and retrieval-excluded content are refused.".into(),
            inputs: if read {
                vec![
                    ActionInputDefinition { name: "location".into(), input_type: "object".into(), required: true, choices: None, selection: None },
                    ActionInputDefinition { name: "encoding".into(), input_type: "string".into(), required: false, choices: Some(vec!["utf-8".into(), "base64".into()]), selection: None },
                ]
            } else {
                vec![ActionInputDefinition { name: "path".into(), input_type: "string".into(), required: false, choices: None, selection: None }]
            },
            output: ActionOutputDefinition {output_type: if read {"central-file-reading"} else {"central-directory-reading"}.into()}, mutation_class: MutationClass::ReadOnly,
            preview_supported: false, required_ports: vec![], availability: ActionAvailability {available:true, reason:None},
        };
        registry
            .register(
                descriptor,
                if read {
                    |_, input, context| result(input, context, true)
                } else {
                    |_, input, context| result(input, context, false)
                },
            )
            .expect("Unique native filesystem actions");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        create_core_action_registry, create_default_connector_registry, ConnectorContext,
        RootOptions,
    };
    use serde_json::json;

    #[test]
    fn native_actions_browse_and_read_unadopted_files_without_projectcentral() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        crate::root::initialize_central(root).unwrap();
        fs::create_dir_all(root.join("Work/Bare/src")).unwrap();
        fs::write(root.join("Work/Bare/src/main.rs"), "fn main() {}\n").unwrap();
        let options = RootOptions {
            explicit_root: Some(root.into()),
            ..Default::default()
        };
        let connectors = create_default_connector_registry();
        let connector_context = ConnectorContext::current();
        let context = ActionExecutionContext {
            root_options: &options,
            connectors: &connectors,
            connector_context: &connector_context,
        };
        let registry = create_core_action_registry();
        assert_eq!(
            registry.get("central.files.read").unwrap().mutation_class,
            MutationClass::ReadOnly
        );
        let listing = registry.execute(
            "central.files.list",
            &json!({"path":"Work/Bare/src"}),
            &context,
        );
        assert!(listing.ok, "{listing:?}");
        let listing = listing.data.unwrap();
        assert_eq!(listing["entries"].as_array().unwrap().len(), 1);
        let location = &listing["entries"][0]["location"];
        let read = registry.execute(
            "central.files.read",
            &json!({"location":location}),
            &context,
        );
        assert!(read.ok, "{read:?}");
        let read = read.data.unwrap();
        assert_eq!(read["content"], "fn main() {}\n");
        assert_eq!(read["project"]["name"], "Bare");
        assert_eq!(read["project"]["project_ref"], Value::Null);
        assert_eq!(read["location"], *location);
        assert_eq!(read["revision"], content_revision_bytes(b"fn main() {}\n"));
        assert_eq!(read["automatic_agent_or_model_invocation"], false);
        assert!(!root.join("Work/Bare/ProjectCentral").exists());
        assert!(!root.join("Work/Bare/.central").exists());
        assert_eq!(
            fs::read(root.join("Work/Bare/src/main.rs")).unwrap(),
            b"fn main() {}\n"
        );
        crate::projectcentral_ops::initialize_projectcentral(
            root,
            &root.join("Work/Bare"),
            "native-file-project",
        )
        .unwrap();
        let bound = registry.execute(
            "central.files.read",
            &json!({"location":location}),
            &context,
        );
        assert!(bound.ok, "{bound:?}");
        assert_eq!(
            bound.data.unwrap()["project"]["project_ref"],
            "native-file-project"
        );
        fs::write(
            root.join("Work/Bare/ProjectCentral/user/note.md"),
            "authored aperture\n",
        )
        .unwrap();
        let location = CentralPathRef::new(
            &root.canonicalize().unwrap(),
            "Work/Bare/ProjectCentral/user/note.md".into(),
        )
        .unwrap();
        let read = read_file(root, &location, FileEncoding::Utf8).unwrap();
        let expected = crate::source_horizon::project_source_bindings(&root.join("Work/Bare"))
            .unwrap()
            .into_iter()
            .find(|source| source.path == "ProjectCentral/user/note.md")
            .unwrap();
        assert_eq!(
            read.source.unwrap(),
            expected,
            "Filesystem opening preserves the native authored Source identity"
        );
    }
    #[test]
    fn owner_locations_reject_root_redirects_parent_paths_and_retrieval_exclusion() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        fs::create_dir(root.join("private")).unwrap();
        fs::write(root.join("private/note.md"), "private").unwrap();
        fs::write(root.join("private/.no-agent-retrieval"), "").unwrap();
        assert!(list_files(&root, "..").is_err());
        assert!(list_files(&root, "private").is_err());
        let blocked = CentralPathRef::new(&root, "private/note.md".into()).unwrap();
        assert_eq!(
            read_file(&root, &blocked, FileEncoding::Utf8).unwrap_err().kind(),
            io::ErrorKind::PermissionDenied
        );
        let other = tempfile::tempdir().unwrap();
        assert!(read_file(other.path(), &blocked, FileEncoding::Utf8).is_err());
        let listing = list_files(&root, "").unwrap();
        assert_eq!(listing.entries[0].name, "private");
        assert!(!listing.entries[0].retrieval_allowed);
    }
    #[test]
    fn real_binary_and_oversized_files_are_not_misrepresented_as_text() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        fs::write(root.join("binary"), [0, 255, 1]).unwrap();
        fs::write(root.join("large"), vec![b'x'; MAX_TEXT_BYTES as usize + 1]).unwrap();
        for name in ["binary", "large"] {
            assert_eq!(
                read_file(&root, &CentralPathRef::new(&root, name.into()).unwrap(), FileEncoding::Utf8)
                    .unwrap_err()
                    .kind(),
                io::ErrorKind::InvalidData
            );
        }
    }
    #[test]
    fn base64_encoding_returns_material_bytes_with_mime_hint() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let png_bytes: Vec<u8> = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 1];
        fs::write(root.join("image.png"), &png_bytes).unwrap();
        fs::write(root.join("mystery.bin"), [0u8, 1, 2, 255, 0]).unwrap();

        let png_location = CentralPathRef::new(&root, "image.png".into()).unwrap();
        let read = read_file(&root, &png_location, FileEncoding::Base64).unwrap();
        assert_eq!(read.content_encoding, "base64");
        assert_eq!(read.mime_hint.as_deref(), Some("image/png"));
        assert_eq!(read.byte_len, png_bytes.len() as u64);
        use base64::Engine;
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(&read.content)
            .unwrap();
        assert_eq!(decoded, png_bytes);

        let bin_location = CentralPathRef::new(&root, "mystery.bin".into()).unwrap();
        let read = read_file(&root, &bin_location, FileEncoding::Base64).unwrap();
        assert_eq!(read.mime_hint, None, "an unrecognised extension and no magic bytes yields no hint");
        assert!(
            read_file(&root, &bin_location, FileEncoding::Utf8).is_err(),
            "NUL bytes still refuse the default utf-8 reading"
        );
    }
    #[test]
    fn base64_reads_are_bounded_and_still_honour_retrieval_exclusion() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        fs::write(
            root.join("oversized.bin"),
            vec![0u8; MAX_MATERIAL_BYTES as usize + 1],
        )
        .unwrap();
        assert_eq!(
            read_file(
                &root,
                &CentralPathRef::new(&root, "oversized.bin".into()).unwrap(),
                FileEncoding::Base64
            )
            .unwrap_err()
            .kind(),
            io::ErrorKind::InvalidData
        );

        fs::create_dir(root.join("private")).unwrap();
        fs::write(root.join("private/secret.bin"), [0u8, 1, 2]).unwrap();
        fs::write(root.join("private/.no-agent-retrieval"), "").unwrap();
        let blocked = CentralPathRef::new(&root, "private/secret.bin".into()).unwrap();
        assert_eq!(
            read_file(&root, &blocked, FileEncoding::Base64)
                .unwrap_err()
                .kind(),
            io::ErrorKind::PermissionDenied
        );
    }
    #[test]
    fn utf8_default_reading_is_unchanged_by_the_encoding_parameter() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        fs::write(root.join("plain.txt"), "hello\n").unwrap();
        let location = CentralPathRef::new(&root, "plain.txt".into()).unwrap();
        let read = read_file(&root, &location, FileEncoding::Utf8).unwrap();
        assert_eq!(read.content_encoding, "utf-8");
        assert_eq!(read.content, "hello\n");
        assert_eq!(read.mime_hint, None);
        let serialized = serde_json::to_value(&read).unwrap();
        assert!(
            serialized.get("mime_hint").is_none(),
            "utf-8 reads keep the original response shape: {serialized:?}"
        );

        // Through the registry, an omitted `encoding` input behaves identically.
        let options = RootOptions {
            explicit_root: Some(root.clone()),
            ..Default::default()
        };
        let connectors = create_default_connector_registry();
        let connector_context = ConnectorContext::current();
        let context = ActionExecutionContext {
            root_options: &options,
            connectors: &connectors,
            connector_context: &connector_context,
        };
        let registry = create_core_action_registry();
        let read = registry.execute(
            "central.files.read",
            &json!({"location": &location}),
            &context,
        );
        assert!(read.ok, "{read:?}");
        let data = read.data.unwrap();
        assert_eq!(data["content_encoding"], "utf-8");
        assert!(data.get("mime_hint").is_none());
    }
    #[test]
    fn owner_addresses_preserve_spaces_unicode_and_delimiters_without_collision() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        for name in [" trailing space ", "unicode—:%.md", "unicode—%3A%.md"] {
            fs::write(root.join(name), name).unwrap();
        }
        let entries = list_files(&root, "").unwrap().entries;
        let refs = entries
            .iter()
            .map(|entry| entry.location.ref_id.clone())
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(refs.len(), 3);
        for entry in entries {
            assert_eq!(
                read_file(&root, &entry.location, FileEncoding::Utf8).unwrap().content,
                entry.name
            );
        }
    }
    #[cfg(unix)]
    #[test]
    fn native_symlink_and_ancestor_symlink_do_not_escape_central() {
        use std::os::unix::fs::symlink;
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("secret"), "outside").unwrap();
        symlink(outside.path(), root.join("link")).unwrap();
        let listing = list_files(&root, "").unwrap();
        assert_eq!(listing.entries[0].kind, "symlink");
        assert!(!listing.entries[0].retrieval_allowed);
        assert!(list_files(&root, "link").is_err());
        assert!(read_file(
            &root,
            &CentralPathRef::new(&root, "link/secret".into()).unwrap(),
            FileEncoding::Utf8
        )
        .is_err());
        fs::create_dir(root.join("folder")).unwrap();
        fs::write(root.join("folder/file"), "before").unwrap();
        let location = list_files(&root, "folder")
            .unwrap()
            .entries
            .remove(0)
            .location;
        fs::remove_file(root.join("folder/file")).unwrap();
        symlink(outside.path().join("secret"), root.join("folder/file")).unwrap();
        assert!(
            read_file(&root, &location, FileEncoding::Utf8).is_err(),
            "A restored owner address is revalidated after replacement"
        );
    }
}
