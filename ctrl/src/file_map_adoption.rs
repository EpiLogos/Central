//! Explicit database adoption uses SQLite's online backup API only. All
//! bookmark semantics, mutation and search remain in the actual bkmr CLI.
use super::file_map::*;
use crate::file_map_backend::{self as native, Backend};
use crate::projectcentral_flow::content_revision_bytes;
use rusqlite::{backup::Backup, Connection, OpenFlags};
use serde_json::{json, Value};
use std::os::unix::fs::PermissionsExt;
use std::{fs, io, path::Path, time::Duration};

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
    let backend = Backend::new(&scope.root);
    if fs::symlink_metadata(backend.db()).is_ok()
        || scope.root.join(".central/bkmr/adoption.json").exists()
    {
        return Err(conflict(
            "The map already has a database or adoption receipt; neither is overwritten",
        ));
    }
    safe_directory(&scope.root, Path::new(".central/bkmr"))?;
    let backup = scope.root.join(".central/bkmr/adopted-original.db");
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
        .root
        .join(format!(".central/bkmr/adopt-{}.db", std::process::id()));
    let file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&stage)?;
    file.set_permissions(fs::Permissions::from_mode(0o600))?;
    drop(file);
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
        fs::OpenOptions::new()
            .read(true)
            .open(&backup)?
            .sync_all()?;
        let receipt = json!({"schema":"central.bkmr-adoption/v1","source_database":source,"source_main_revision":content_revision_bytes(&before),"backup":".central/bkmr/adopted-original.db","backup_revision":content_revision_bytes(&fs::read(&backup)?),"status":"backed-up"});
        write_atomic(
            &scope.root.join(".central/bkmr/adoption.json"),
            &serde_json::to_vec_pretty(&receipt)?,
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
            &scope.root.join(".central/bkmr/adoption.json"),
            &serde_json::to_vec_pretty(&receipt)?,
        )?;
        Ok(receipt)
    })();
    let _ = fs::remove_file(stage);
    result
}

pub(crate) fn record_adopt(scope: &Scope, input: &Value) -> io::Result<Value> {
    expect_basis(scope, input)?;
    let (_, entry) = lookup(std::slice::from_ref(scope), text(input, "source_ref")?)?;
    let id = input["record_id"]
        .as_i64()
        .filter(|id| *id > 0)
        .ok_or_else(|| invalid("A positive record_id is required"))?;
    let backend = Backend::new(&scope.root);
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
            generated_description: generated,
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
    let raw = text(input, "path")?;
    let external = Path::new(raw).is_absolute();
    if external && input["allow_external"] != true {
        return Err(invalid("External scope requires allow_external=true"));
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
