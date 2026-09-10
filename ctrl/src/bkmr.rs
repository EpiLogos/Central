//! The upstream CLI is the index engine. No SQL schema or retrieval algorithm is
//! duplicated here. Central owns its scoped database and bounded process home.
use serde::{Deserialize, Serialize};
use std::io::Read;
use std::os::unix::process::CommandExt;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
use std::time::{Duration, Instant};
use serde_json::Value;
use std::{fs, io, path::{Path, PathBuf}, process::{Command, Stdio, Output}};

pub const TESTED_VERSION: &str = "7.6.7";
pub const DB: &str = ".central/bkmr.db";
pub const HOME: &str = ".central/bkmr-home";
const MAX_OUTPUT: usize = 64 * 1024 * 1024;

/// Drain both streams concurrently with a hard byte ceiling and deadline. No
/// shell is involved; a timed-out child process group is reaped before return.
fn bounded(mut command: Command, seconds: u64) -> io::Result<Output> {
    command.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).process_group(0);
    let mut child = command.spawn()?;
    let overflow = Arc::new(AtomicBool::new(false));
    fn reader<R: Read + Send + 'static>(mut input: R, flag: Arc<AtomicBool>) -> std::thread::JoinHandle<io::Result<Vec<u8>>> {
        std::thread::spawn(move || {
            let mut output = Vec::new();
            input.by_ref().take((MAX_OUTPUT + 1) as u64).read_to_end(&mut output)?;
            if output.len() > MAX_OUTPUT { flag.store(true, Ordering::Release); }
            Ok(output)
        })
    }
    let stdout = reader(child.stdout.take().unwrap(), overflow.clone());
    let stderr = reader(child.stderr.take().unwrap(), overflow.clone());
    let deadline = Instant::now() + Duration::from_secs(seconds);
    let mut reason = None;
    let status = loop {
        if let Some(status) = child.try_wait()? { break status; }
        if overflow.load(Ordering::Acquire) || Instant::now() >= deadline {
            reason = Some("bkmr exceeded its output or execution-time bound");
            unsafe { libc::kill(-(child.id() as i32), libc::SIGKILL); }
            let _ = child.kill();
            break child.wait()?;
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let stdout = stdout.join().map_err(|_| io::Error::other("bkmr output reader failed"))??;
    let stderr = stderr.join().map_err(|_| io::Error::other("bkmr diagnostic reader failed"))??;
    if let Some(reason) = reason { return Err(io::Error::new(io::ErrorKind::TimedOut, reason)); }
    if overflow.load(Ordering::Acquire) { return Err(io::Error::other("bkmr output exceeds bounded reading")); }
    Ok(Output { status, stdout, stderr })
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Record {
    pub id: i64,
    pub url: String,
    pub title: String,
    pub description: String,
    pub tags: Vec<String>,
}

pub struct Bkmr { root: PathBuf, binary: std::ffi::OsString }
impl Bkmr {
    pub fn new(root: &Path) -> Self {
        Self { root: root.into(), binary: std::env::var_os("CENTRAL_BKMR_BIN").unwrap_or_else(|| "bkmr".into()) }
    }
    pub fn version(&self) -> io::Result<String> {
        let mut command = Command::new(&self.binary); command.arg("--version");
        let out = bounded(command, 5)?;
        if !out.status.success() { return Err(io::Error::other("bkmr version probe failed")); }
        Ok(String::from_utf8(out.stdout).map_err(io::Error::other)?.trim().into())
    }
    pub fn prepare(&self) -> io::Result<()> {
        let home = self.root.join(HOME);
        crate::file_map::safe_directories(&self.root, &format!("{HOME}/.config/bkmr"))?;
        // 7.6.7's importer reloads load_settings(None), ignoring --config.
        // Give that reload exactly the same private config, never the user's HOME.
        let config = format!("db_url = {}\n[base_paths]\nCENTRAL_WORLD = {}\n", serde_json::to_string(&self.root.join(DB)).map_err(io::Error::other)?, serde_json::to_string(&self.root).map_err(io::Error::other)?);
        crate::file_map::atomic_json_bytes(&home.join(".config/bkmr/config.toml"), config.as_bytes())?;
        if !self.root.join(DB).exists() {
            self.run(&["create-db".into(), self.root.join(DB).display().to_string()])?;
        }
        fs::set_permissions(self.root.join(DB), fs::Permissions::from_mode(0o600))?;
        Ok(())
    }
    pub fn run(&self, args: &[String]) -> io::Result<String> {
        for rel in [".central", DB, ".central/bkmr.db-wal", ".central/bkmr.db-shm", HOME, &format!("{HOME}/.config/bkmr/config.toml")] {
            crate::projectcentral_flow::reject_symlink_components(&self.root, Path::new(rel))?;
        }
        if let Ok(meta) = fs::symlink_metadata(self.root.join(DB)) {
            if !meta.is_file() || meta.nlink() != 1 { return Err(io::Error::other("bkmr database is not one regular owned file")); }
        }
        let mut command = Command::new(&self.binary);
        command.args(["--no-color", "--config"])
            .arg(self.root.join(HOME).join(".config/bkmr/config.toml"))
            .arg("--db").arg(self.root.join(DB)).args(args)
            .env("HOME", self.root.join(HOME)).env_remove("BKMR_DB_URL")
            .env("NO_COLOR", "1").env("BKMR_SHELL_INTERACTIVE", "false")
            .current_dir(&self.root);
        let out = bounded(command, 60)?;
        if !out.status.success() {
            // Do not echo argv: it may contain indexed private source text.
            return Err(io::Error::other(format!("bkmr {} failed ({}); source-bearing arguments and diagnostics are withheld", args.first().map(String::as_str).unwrap_or("operation"), out.status)));
        }
        if out.stdout.len() > MAX_OUTPUT { return Err(io::Error::other("bkmr output exceeds bounded record reading")); }
        String::from_utf8(out.stdout).map_err(io::Error::other)
    }
    pub fn records(&self, query: Option<&str>, tags: &[String], hybrid: bool, limit: usize) -> io::Result<Vec<Record>> {
        if !self.root.join(DB).is_file() { return Err(io::Error::new(io::ErrorKind::NotFound, "Central bkmr map has not been refreshed")); }
        let mut args = vec![if hybrid { "hsearch" } else { "search" }.into()];
        if let Some(q) = query.filter(|q| !q.is_empty()) { args.push(q.into()); }
        args.extend(["--json".into(), "--np".into(), "--limit".into(), limit.to_string()]);
        if !tags.is_empty() { args.extend(["--tags".into(), tags.join(",")]); }
        let value: Value = serde_json::from_str(&self.run(&args)?).map_err(io::Error::other)?;
        let rows = value.as_array().ok_or_else(|| io::Error::other("bkmr JSON record shape changed"))?;
        rows.iter().map(|row| {
            let mut row = row.get("bookmark").unwrap_or(row).clone();
            // hsearch serializes native domain Tags; search uses string Tags.
            if let Some(tags) = row.get_mut("tags").and_then(Value::as_array_mut) {
                for tag in tags { if let Some(text) = tag.get("value").and_then(Value::as_str) { *tag = Value::String(text.into()); } }
            }
            serde_json::from_value(row).map_err(io::Error::other)
        }).collect()
    }
    pub fn add_uri(&self, url: &str, title: &str, description: &str, tags: &[String]) -> io::Result<()> {
        self.run(&["add".into(), url.into(), tags.join(","), "--title".into(), title.into(), "--description".into(), description.into(), "--no-web".into(), "--no-embed".into()])?;
        Ok(())
    }
    pub fn update(&self, id: i64, fields: &[String]) -> io::Result<()> {
        let mut args = vec!["update".into(), id.to_string(), "--no-embed".into()]; args.extend_from_slice(fields);
        self.run(&args)?; Ok(())
    }
    pub fn import(&self, relative: &str) -> io::Result<()> {
        self.run(&["import-files".into(), relative.into(), "--update".into(), "--base-path".into(), "CENTRAL_WORLD".into(), "--no-embed".into()])?;
        Ok(())
    }
    pub fn delete(&self, id: i64) -> io::Result<()> { self.run(&["delete".into(), id.to_string()])?; Ok(()) }
}

/// SQLite's online backup protocol is used only for coherent snapshots, never
/// for inspecting or changing bkmr's private schema. WAL writers are supported.
pub fn backup(source: &Path, destination: &Path) -> io::Result<()> {
    if fs::symlink_metadata(source)?.file_type().is_symlink() || destination.exists() {
        return Err(io::Error::new(io::ErrorKind::AlreadyExists, "Backup needs a regular source and a new destination"));
    }
    let source = rusqlite::Connection::open_with_flags(source, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).map_err(io::Error::other)?;
    let mut target = rusqlite::Connection::open(destination).map_err(io::Error::other)?;
    let copy = rusqlite::backup::Backup::new(&source, &mut target).map_err(io::Error::other)?;
    copy.run_to_completion(128, std::time::Duration::from_millis(10), None).map_err(io::Error::other)
}
