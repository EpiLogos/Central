//! Explicit database adoption uses SQLite's online backup API only. All
//! bookmark semantics, mutation and search remain in the actual bkmr CLI.
use super::file_map::*;
use crate::file_map_backend::{self as native, Backend};
use crate::source_safety::content_revision_bytes;
use rusqlite::{backup::Backup, Connection, OpenFlags};
use serde_json::{json, Value};
use std::os::unix::fs::PermissionsExt;
use std::{
    fs, io,
    path::{Path, PathBuf},
    time::Duration,
};

pub(crate) fn adopt(scope: &Scope, input: &Value) -> io::Result<Value> {
    if input["quiesced"] != true {
        return Err(invalid(
            "Adoption requires quiesced=true for the source database",
        ));
    }
    expect_basis(scope, input)?;
    let raw = text(input, "database")?;
    let source = if Path::new(raw).is_absolute() {
        safe_member(Path::new("/"), raw.trim_start_matches('/'), true)?
    } else {
        safe_member(&scope.root, raw, true)?
    };
    let backend = Backend::for_scope(scope);
    if fs::symlink_metadata(backend.db()).is_ok()
        || scope.state.join(".central/bkmr/adoption.json").exists()
    {
        return Err(conflict(
            "The map already has a database or adoption receipt; neither is overwritten",
        ));
    }
    safe_directory(&scope.state, Path::new(".central/bkmr"))?;
    let backup = scope.state.join(".central/bkmr/adopted-original.db");
    if fs::symlink_metadata(&backup).is_ok() {
        return Err(conflict(
            "A retained original backup exists; explicit recovery is required",
        ));
    }
    let before = fs::read(&source)?;
    if !before.starts_with(b"SQLite format 3\0") {
        return Err(invalid("Source is not a SQLite database"));
    }
    let stage = scope
        .state
        .join(format!(".central/bkmr/adopt-{}.db", std::process::id()));
    let file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&stage)?;
    file.set_permissions(fs::Permissions::from_mode(0o600))?;
    drop(file);
    let mut backup_linked = false;
    let mut backup_file_synced = false;
    let mut backup_read_observed = false;
    let result = (|| {
        let src = Connection::open_with_flags(&source, OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(io::Error::other)?;
        let mut dst = Connection::open(&stage).map_err(io::Error::other)?;
        Backup::new(&src, &mut dst)
            .map_err(io::Error::other)?
            .run_to_completion(64, Duration::from_millis(5), None)
            .map_err(io::Error::other)?;
        drop(dst);
        drop(src);
        // Retain an untouched, complete snapshot, including any committed WAL.
        fs::hard_link(&stage, &backup)?;
        backup_linked = true;
        fs::OpenOptions::new()
            .read(true)
            .open(&backup)?
            .sync_all()?;
        backup_file_synced = true;
        let receipt = json!({"schema":"central.bkmr-adoption/v1","source_database":source,"source_main_revision":content_revision_bytes(&before),"backup":".central/bkmr/adopted-original.db","backup_revision":content_revision_bytes(&fs::read(&backup)?),"status":"backed-up"});
        backup_read_observed = true;
        write_atomic(
            &scope.state,
            &scope.state.join(".central/bkmr/adoption.json"),
            &serde_json::to_vec_pretty(&receipt)?,
            crate::file_mutation::RecordDisposition::CreateNew,
        )?;
        // A distinct inode: future native bkmr writes must never change backup.
        let target = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(backend.db())?;
        target.set_permissions(fs::Permissions::from_mode(0o600))?;
        let mut target = target;
        let mut original = fs::File::open(&backup)?;
        io::copy(&mut original, &mut target)?;
        target.sync_all()?;
        backend.prepare()?;
        let records = backend.records()?;
        let index = scope.index()?;
        scope.save_index(&index)?;
        let mut receipt = receipt;
        receipt["status"] = json!("adopted");
        receipt["retained_records"] = json!(records.len());
        write_atomic(
            &scope.state,
            &scope.state.join(".central/bkmr/adoption.json"),
            &serde_json::to_vec_pretty(&receipt)?,
            crate::file_mutation::RecordDisposition::ReplaceOrCreate,
        )?;
        Ok(receipt)
    })();
    let _ = fs::remove_file(stage);
    result.map_err(|error| {
        if backup_linked {
            // Link visibility, file sync and observed complete backup bytes are
            // distinct actual facts. None is a fabricated final adoption receipt
            // or a guarantee that the containing directory was made durable.
            let phase = if backup_read_observed {
                "backup_read_observed_adoption_steps_may_be_partial"
            } else if backup_file_synced {
                "backup_file_synced_link_durability_unconfirmed"
            } else {
                "backup_link_visible_sync_and_adoption_unconfirmed"
            };
            crate::file_mutation::record_owner_error(
                error,
                "file_map.database_adoption",
                None,
                None,
                phase,
            )
        } else {
            error
        }
    })
}

pub(crate) fn record_adopt(scope: &Scope, input: &Value) -> io::Result<Value> {
    expect_basis(scope, input)?;
    let (_, entry) = lookup(std::slice::from_ref(scope), text(input, "source_ref")?)?;
    let id = input["record_id"]
        .as_i64()
        .filter(|id| *id > 0)
        .ok_or_else(|| invalid("A positive record_id is required"))?;
    let backend = Backend::for_scope(scope);
    let records = backend.records()?;
    let row = records
        .iter()
        .find(|v| native::id(v).ok() == Some(id))
        .ok_or_else(|| invalid("Native bookmark does not exist"))?;
    let revision = native::record_revision(row)?;
    if input["record_revision"] != revision {
        return Err(conflict(format!(
            "Native record revision changed; current {revision}"
        )));
    }
    let mut index = scope.index()?;
    if index
        .entries
        .values()
        .any(|v| v.id == id || v.import_id == Some(id))
    {
        return Err(conflict("Native record is already bound"));
    }
    let url = uri(&entry.path);
    if row["url"] != url {
        return Err(invalid(
            "Explicit adoption requires the bookmark's URI to equal the live source location",
        ));
    }
    let retained = row["description"].as_str().unwrap().to_owned();
    let generated = format!(
        "{}\n\nRetained bookmark description:\n{}",
        description(&entry),
        retained
    );
    backend.run(&[
        "update".into(),
        id.to_string(),
        "--description".into(),
        generated.clone(),
        "--no-embed".into(),
    ])?;
    index.entries.insert(
        entry.source.source_ref.clone(),
        Indexed {
            id,
            revision: entry.revision,
            url,
            source_path: entry.source.path,
            generated_title: entry.title,
            description_hash: content_revision_bytes(generated.as_bytes()),
            retained_description: Some(retained),
            import_id: None,
        },
    );
    scope.save_index(&index)?;
    Ok(
        json!({"source_ref":entry.source.source_ref,"provider_binding":id.to_string(),"retained_title":row["title"],"retained_tags":row["tags"],"authored_description_preserved":true}),
    )
}

pub(crate) fn scope_register(all: &[Scope], input: &Value) -> io::Result<Value> {
    let root = all.first().ok_or_else(|| invalid("Missing root scope"))?;
    if root.world != "control:root" {
        return Err(invalid("Scope federation is registered at root Central"));
    }
    expect_basis(root, input)?;
    let name = text(input, "name")?;
    relative(name)?;
    match input
        .get("kind")
        .and_then(Value::as_str)
        .unwrap_or("project")
    {
        "project" => {}
        "folder" => return folder_register(all, root, name, input),
        other => {
            return Err(invalid(format!(
                "A scope is a project or a folder, and {other} is neither"
            )))
        }
    }
    // A folder scope's World is `folder:<name>`: no project's name is spelled so.
    if name.starts_with(FOLDER_WORLD_PREFIX) {
        return Err(invalid(format!(
            "A project scope's name never begins with {FOLDER_WORLD_PREFIX}, which names a folder scope's World, and {name} does"
        )));
    }
    for declared in folder_declarations(&root.root)? {
        match declared {
            Ok(folder) if folder.name == name => {
                return Err(conflict(format!(
                    "The scope name {name} is already taken: it is the folder scope over {}",
                    folder.root
                )))
            }
            Err((declared, words)) if declared == name => {
                return Err(conflict(format!(
                    "The scope name {name} is already taken by a folder scope whose declaration does not read: {words}"
                )))
            }
            _ => {}
        }
    }
    let raw = text(input, "path")?;
    let external = Path::new(raw).is_absolute();
    if external && input["allow_external"] != true {
        return Err(invalid("External scope requires allow_external=true"));
    }
    // An external scope's state lives at <ground>/.central/scopes/<name>.
    if external && Path::new(name).components().count() != 1 {
        return Err(invalid("External scope name must be one path component"));
    }
    let path = if external {
        safe_member(Path::new("/"), raw.trim_start_matches('/'), true)?
    } else {
        safe_member(&root.root, raw, true)?
    };
    let target = Scope::project(path.clone(), Some(name.into()))?;
    if let Some(existing) = all.iter().find(|s| s.world == target.world) {
        if existing.root == path {
            return Ok(json!({"world_ref":target.world,"changed":false}));
        }
        return Err(conflict(
            "Project World is already bound to a different location",
        ));
    }
    let mut ground = root.ground()?;
    if ground.scopes.contains_key(name) {
        return Err(conflict("Scope name already registered"));
    }
    ground.scopes.insert(name.into(), raw.into());
    root.save(&ground, root.document()?)?;
    Ok(json!({"world_ref":target.world,"changed":true,"revision":root.basis()?}))
}

/// Register a plain folder outside the ground as a scope: one with no ProjectCentral, read where it
/// stands. Its declaration is written in the ground (`.central/scopes/<name>/scope.json`); nothing is
/// written inside the folder, then or later. It is asked for explicitly (`kind: "folder"`,
/// `allow_external: true`) and names the media it admits (`media`), which it lists by metadata only.
fn folder_register(all: &[Scope], root: &Scope, name: &str, input: &Value) -> io::Result<Value> {
    if Path::new(name).components().count() != 1 || name.starts_with('.') {
        return Err(invalid(format!(
            "A folder scope's name is one plain path component, and {name} is not"
        )));
    }
    let raw = text(input, "path")?;
    if !Path::new(raw).is_absolute() {
        return Err(invalid(format!(
            "A folder scope is named by its absolute path, and {raw} is not one"
        )));
    }
    if input["allow_external"] != true {
        return Err(invalid(format!(
            "{raw} is outside this ground: a folder scope is registered only when asked, with \
             allow_external=true"
        )));
    }
    let target = match fs::canonicalize(raw) {
        Ok(target) => target,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("There is no folder at {raw}"),
            ))
        }
        Err(error) => return Err(error),
    };
    if !fs::metadata(&target)?.is_dir() {
        return Err(invalid(format!("{raw} is a file, not a folder")));
    }
    let ground = root.root.canonicalize()?;
    if target.starts_with(&ground) {
        return Err(invalid(format!(
            "{raw} is inside this ground ({}): a folder scope stands outside it, and the ground's \
             own folders are read through Central and its projects",
            ground.display()
        )));
    }
    if ground.starts_with(&target) {
        return Err(invalid(format!(
            "{raw} holds this ground ({}): a folder scope stands beside the ground, never around it",
            ground.display()
        )));
    }
    if target.join("ProjectCentral/project.json").exists() {
        return Err(invalid(format!(
            "{raw} holds a ProjectCentral: register it as a project, not as a folder"
        )));
    }
    let shown = target
        .to_str()
        .ok_or_else(|| invalid(format!("{raw} is not a UTF-8 path")))?
        .to_owned();
    let media = admitted_media(input)?;
    let exclude = excluded_folders(input)?;
    let world = format!("{FOLDER_WORLD_PREFIX}{name}");
    let mut declared = Vec::new();
    for read in folder_declarations(&root.root)? {
        match read {
            Ok(folder) => declared.push(folder),
            Err((unread, words)) if unread == name => {
                return Err(conflict(format!(
                    "The scope name {name} is already taken by a folder scope whose declaration does not read: {words}"
                )))
            }
            Err(_) => {}
        }
    }
    if let Some(same) = declared.iter().find(|folder| folder.name == name) {
        if same.root == shown && same.media == media && same.exclude == exclude {
            return Ok(json!({"world_ref":world,"kind":"folder","changed":false,
                "path":same.root,"media":same.media,"exclude":same.exclude}));
        }
        if same.root == shown {
            return Err(conflict(format!(
                "The folder scope {name} already stands over {} admitting {} and excluding {}: it is \
                 registered once, as it was asked first",
                same.root,
                words_or_none(&same.media),
                words_or_none(&same.exclude)
            )));
        }
        return Err(conflict(format!(
            "The scope name {name} is already taken: it is the folder scope over {}",
            same.root
        )));
    }
    if let Some(other) = declared.iter().find(|folder| folder.root == shown) {
        return Err(conflict(format!(
            "{raw} is already the folder scope {}",
            other.name
        )));
    }
    // A folder scope stands beside every other scope, never inside or around one: each is compared by its canonical path.
    let others = declared
        .iter()
        .map(|folder| {
            (
                "folder scope",
                folder.name.clone(),
                PathBuf::from(&folder.root),
            )
        })
        .chain(
            all.iter()
                .filter(|scope| scope.is_external() && !scope.is_folder())
                .map(|scope| {
                    (
                        "external project scope",
                        scope.project.clone().unwrap_or_else(|| scope.world.clone()),
                        scope.root.clone(),
                    )
                }),
        );
    for (kind, other, at) in others {
        if target.starts_with(&at) {
            return Err(conflict(format!(
                "{raw} is inside the {kind} {other} ({}): a folder scope never stands inside another scope",
                at.display()
            )));
        }
        if at.starts_with(&target) {
            return Err(conflict(format!(
                "{raw} holds the {kind} {other} ({}): a folder scope never stands around another scope",
                at.display()
            )));
        }
    }
    if root.ground()?.scopes.contains_key(name)
        || all
            .iter()
            .any(|scope| scope.project.as_deref() == Some(name))
    {
        return Err(conflict(format!(
            "The scope name {name} is already taken by a project of this ground"
        )));
    }
    let state = external_state(&root.root, name)?;
    let folder = FolderScope {
        schema: FOLDER_SCOPE_SCHEMA.into(),
        name: name.into(),
        root: shown,
        kind: "folder".into(),
        media,
        exclude,
        registered_at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
    };
    write_atomic(
        &state,
        &state.join(FOLDER_SCOPE_DECLARATION),
        &serde_json::to_vec_pretty(&folder)?,
        crate::file_mutation::RecordDisposition::CreateNew,
    )?;
    Ok(
        json!({"world_ref":world,"kind":"folder","changed":true,"path":folder.root,
        "media":folder.media,"exclude":folder.exclude,"registered_at":folder.registered_at,
        "declaration":state.join(FOLDER_SCOPE_DECLARATION)}),
    )
}

/// A list in words, or "none".
fn words_or_none(items: &[String]) -> String {
    if items.is_empty() {
        "none".into()
    } else {
        items.join(", ")
    }
}

/// The folders a folder scope never walks, from `exclude` (optional): each one folder's name, matched at any depth, once.
fn excluded_folders(input: &Value) -> io::Result<Vec<String>> {
    let Some(list) = input.get("exclude").filter(|value| !value.is_null()) else {
        return Ok(Vec::new());
    };
    let shape = || {
        invalid("A folder scope's exclude is a list of folder names, each one plain path component")
    };
    let mut exclude = Vec::new();
    for value in list.as_array().ok_or_else(shape)? {
        let name = value.as_str().ok_or_else(shape)?;
        if name.is_empty()
            || name == "."
            || name == ".."
            || name.contains('/')
            || Path::new(name).components().count() != 1
        {
            return Err(invalid(format!(
                "A folder scope excludes folders by name, one plain path component each, and {name} is not one"
            )));
        }
        if !exclude.iter().any(|kept: &String| kept == name) {
            exclude.push(name.to_owned());
        }
    }
    exclude.sort();
    Ok(exclude)
}

/// The media a folder scope admits, from `media`: each a known extension, lower case, once.
fn admitted_media(input: &Value) -> io::Result<Vec<String>> {
    let missing = || {
        invalid(format!(
            "A folder scope names the media it admits, in media: Central admits {}",
            media_words()
        ))
    };
    let mut media = Vec::new();
    for value in input
        .get("media")
        .and_then(Value::as_array)
        .ok_or_else(missing)?
    {
        let extension = value
            .as_str()
            .ok_or_else(missing)?
            .trim_start_matches('.')
            .to_ascii_lowercase();
        if !MEDIA_KINDS.iter().any(|(known, _)| *known == extension) {
            return Err(invalid(format!(
                "Central lists no media of kind {extension}: it admits {}",
                media_words()
            )));
        }
        if !media.contains(&extension) {
            media.push(extension);
        }
    }
    if media.is_empty() {
        return Err(missing());
    }
    media.sort();
    Ok(media)
}
