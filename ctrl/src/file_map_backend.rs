//! bkmr's CLI, not a second search engine. Every invocation selects one DB.
use serde_json::Value;
use std::os::unix::{fs::PermissionsExt, process::CommandExt};
use std::{
    fs, io,
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

pub(crate) const VERSION: &str = "7.6.7";
const OUTPUT_LIMIT: usize = 32 * 1024 * 1024;

pub(crate) struct Backend {
    pub root: PathBuf,
    pub area: PathBuf,
}
impl Backend {
    pub fn new(root: &Path) -> Self {
        Self {
            root: root.into(),
            area: root.join(".central/bkmr"),
        }
    }
    pub fn db(&self) -> PathBuf {
        self.area.join("index.db")
    }
    pub fn present(&self) -> bool {
        self.db().is_file()
    }
    pub fn version(&self) -> io::Result<String> {
        invoke(&["--version".into()], None, None)
    }
    pub fn prepare(&self) -> io::Result<()> {
        super::file_map::safe_directory(&self.root, Path::new(".central/bkmr/home/.config/bkmr"))?;
        let config = format!(
            "db_url = {}\n[base_paths]\nWORLD = {}\n\n[embeddings]\nmodel = \"NomicEmbedTextV15\"\n",
            serde_json::to_string(&self.db().to_string_lossy())?,
            serde_json::to_string(&self.root.to_string_lossy())?
        );
        let path = self.area.join("home/.config/bkmr/config.toml");
        // bkmr 7.6.7's importer reloads default settings instead of the supplied
        // --config. Isolate HOME as well so BOTH code paths see this scope.
        if fs::read_to_string(&path).ok().as_deref() != Some(&config) {
            super::file_map::write_atomic(&path, config.as_bytes())?;
        }
        if !self.present() {
            self.run(&["create-db".into(), self.db().to_string_lossy().into()])?;
        }
        fs::set_permissions(self.db(), fs::Permissions::from_mode(0o600))?;
        Ok(())
    }
    pub fn run(&self, args: &[String]) -> io::Result<String> {
        super::file_map::safe_member(&self.root, ".central/bkmr", true)?;
        for name in [
            "index.db",
            "index.db-wal",
            "index.db-shm",
            "home/.config/bkmr/config.toml",
        ] {
            let p = self.area.join(name);
            if fs::symlink_metadata(&p).is_ok() {
                super::file_map::safe_member(
                    &self.root,
                    p.strip_prefix(&self.root)
                        .unwrap()
                        .to_str()
                        .ok_or_else(|| io::Error::other("Non-UTF8 map path"))?,
                    true,
                )?;
            }
        }
        let mut argv = vec![
            "--db".into(),
            self.db().to_string_lossy().into(),
            "--config".into(),
            self.area
                .join("home/.config/bkmr/config.toml")
                .to_string_lossy()
                .into(),
            "--no-color".into(),
        ];
        argv.extend_from_slice(args);
        invoke(&argv, Some(&self.root), Some(&self.area.join("home")))
    }
    pub fn records(&self) -> io::Result<Vec<Value>> {
        if !self.present() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "Persistent bkmr map is not initialized; run central.file-map.refresh",
            ));
        }
        records(&self.run(&["search".into(), "--json".into(), "--np".into()])?)
    }
    pub fn search(
        &self,
        query: &str,
        tags: &[String],
        hybrid: bool,
        limit: usize,
    ) -> io::Result<Vec<Value>> {
        if !self.present() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "Persistent bkmr map is not initialized",
            ));
        }
        // bkmr's hybrid path feeds the limit straight into a sqlite-vec knn
        // query, which rejects k above 4096. Full-text search has no such
        // ceiling, so only the hybrid invocation is capped.
        let limit = if hybrid { limit.min(100) } else { limit };
        let mut args = vec![
            if hybrid { "hsearch" } else { "search" }.into(),
            "--json".into(),
            "--np".into(),
            "--limit".into(),
            limit.to_string(),
        ];
        if !tags.is_empty() {
            args.extend(["--tags".into(), tags.join(",")]);
        }
        // The end-of-options delimiter keeps a query beginning '-' as data.
        if !query.is_empty() || hybrid {
            let rendered = if hybrid {
                // The hybrid path re-ranks through its embedding query, which
                // wants the caller's own words, not FTS5 syntax.
                query.to_string()
            } else {
                // Full-text search hands the query to SQLite FTS5, where a
                // bare hyphen is column syntax: `aikit-knowledge-work-coverage-spec`
                // failed the whole map search with "no such column: knowledge".
                // Quoted terms read as the literals they are.
                fts5_quote(query)
            };
            args.extend(["--".into(), rendered]);
        }
        records(&self.run(&args)?)
    }
}

/// FTS5-quote each whitespace-separated term: every term becomes a double-
/// quoted literal (an embedded `"` is doubled, FTS5's own escape), so query
/// punctuation is data, never syntax. An empty query renders empty and the
/// caller skips it.
pub(crate) fn fts5_quote(query: &str) -> String {
    query
        .split_whitespace()
        .filter(|term| !term.is_empty())
        .map(|term| format!("\"{}\"", term.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::fts5_quote;
    use crate::file_map_backend::Backend;

    #[test]
    fn a_hyphenated_query_is_quoted_per_term_not_raw_column_syntax() {
        assert_eq!(
            fts5_quote("aikit-knowledge-work-coverage-spec"),
            "\"aikit-knowledge-work-coverage-spec\""
        );
    }

    #[test]
    fn every_term_is_quoted_and_joined_so_fts5_ands_them() {
        assert_eq!(
            fts5_quote("day close rollover"),
            "\"day\" \"close\" \"rollover\""
        );
    }

    #[test]
    fn an_embedded_quote_is_doubled_the_fts5_escape() {
        // Terms split on whitespace; a term that itself carries quotes gets
        // each quote doubled inside the wrapping quotes.
        assert_eq!(
            fts5_quote("say \"hello\" world"),
            "\"say\" \"\"\"hello\"\"\" \"world\""
        );
    }

    #[test]
    fn whitespace_collapses_and_an_empty_query_stays_empty() {
        assert_eq!(fts5_quote("  spaced \t terms  "), "\"spaced\" \"terms\"");
        assert_eq!(fts5_quote(""), "");
        assert_eq!(fts5_quote("   "), "");
    }

    /// The regression the fix exists for, against the real bkmr: before the
    /// quoting, a hyphenated query reached FTS5 raw and the whole map search
    /// failed (`central.file_map_failure — no such column: knowledge`); now
    /// it finds the record. Gated like the other real-binary lanes
    /// (`CENTRAL_FILE_MAP_BKMR_CONFORMANCE=real`) because CI runners carry no
    /// bkmr.
    #[test]
    fn a_hyphenated_query_returns_hits_instead_of_an_fts_failure() {
        if std::env::var("CENTRAL_FILE_MAP_BKMR_CONFORMANCE").as_deref() != Ok("real") {
            eprintln!("set CENTRAL_FILE_MAP_BKMR_CONFORMANCE=real to run the real-bkmr regression");
            return;
        }
        let dir = std::env::temp_dir().join(format!(
            "ctrl-file-map-fts-regression-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        // The map refuses world roots through symlinked paths, and the
        // platform temp dir is one (/var → /private/var): resolve the real
        // path before handing it to the backend.
        let dir = std::fs::canonicalize(dir).unwrap();
        let backend = Backend::new(&dir);
        backend.prepare().expect("real bkmr prepares an index");
        backend
            .run(&[
                "add".into(),
                "https://map.local/aikit-knowledge-work-coverage-spec".into(),
                "--title".into(),
                "knowledge work coverage spec".into(),
                "--description".into(),
                "the knowledge work coverage spec for the Work repositories".into(),
                "--no-web".into(),
                "--no-embed".into(),
            ])
            .expect("real bkmr adds the record");
        let hits = backend
            .search("aikit-knowledge-work-coverage-spec", &[], false, 10)
            .expect("a hyphenated query must not fail FTS syntax");
        assert!(
            !hits.is_empty(),
            "the hyphenated query finds the record it names"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
pub(crate) fn record(value: &Value) -> &Value {
    value.get("bookmark").unwrap_or(value)
}
pub(crate) fn id(value: &Value) -> io::Result<i64> {
    record(value)["id"]
        .as_i64()
        .filter(|id| *id > 0)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "bkmr JSON lacks a positive record id",
            )
        })
}
pub(crate) fn tags(value: &Value) -> io::Result<Vec<String>> {
    let value = &record(value)["tags"];
    if let Some(items) = value.as_array() {
        return items
            .iter()
            .map(|v| {
                v.as_str()
                    .map(str::to_owned)
                    .ok_or_else(|| io::Error::other("bkmr tag is not a string"))
            })
            .collect();
    }
    // Hybrid currently serializes the domain tag set differently from Search.
    if let Some(text) = value.as_str() {
        return Ok(text
            .split(',')
            .filter(|t| !t.is_empty())
            .map(str::to_owned)
            .collect());
    }
    Err(io::Error::new(
        io::ErrorKind::InvalidData,
        "Unrecognized bkmr tag JSON",
    ))
}
pub(crate) fn record_revision(value: &Value) -> io::Result<String> {
    let mut row = record(value).clone();
    let mut set = tags(value)?;
    set.sort();
    set.dedup();
    row["tags"] = serde_json::to_value(set)?;
    Ok(crate::source_safety::content_revision_bytes(
        &serde_json::to_vec(&row)?,
    ))
}

fn records(text: &str) -> io::Result<Vec<Value>> {
    let value: Value =
        serde_json::from_str(text).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let values = value.as_array().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "bkmr output must be a JSON array",
        )
    })?;
    for value in values {
        id(value)?;
        tags(value)?;
        for field in ["url", "title", "description"] {
            if !record(value)[field].is_string() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("bkmr output missing {field}"),
                ));
            }
        }
    }
    Ok(values.clone())
}
fn drain(mut stream: impl Read) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    let mut buffer = [0u8; 8192];
    let mut overflow = false;
    loop {
        let n = stream.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        if bytes.len() + n <= OUTPUT_LIMIT {
            bytes.extend_from_slice(&buffer[..n]);
        } else {
            overflow = true;
        }
    }
    if overflow {
        Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "bkmr output exceeds bounded response size",
        ))
    } else {
        Ok(bytes)
    }
}
fn invoke(args: &[String], cwd: Option<&Path>, home: Option<&Path>) -> io::Result<String> {
    let binary = std::env::var_os("CENTRAL_BKMR_BIN").unwrap_or_else(|| "bkmr".into());
    let mut command = Command::new(binary);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0);
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    if let Some(home) = home {
        command.env("HOME", home);
    }
    command.env_remove("BKMR_DB_URL").env("NO_COLOR", "1");
    let deleting = args.first().is_some_and(|arg| arg == "delete")
        || args.get(5).is_some_and(|arg| arg == "delete");
    if deleting {
        command.stdin(Stdio::piped());
    }
    let mut child = command.spawn()?;
    if deleting {
        use std::io::Write;
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(b"y\n")?;
        }
    }
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let out = std::thread::spawn(move || drain(stdout));
    let err = std::thread::spawn(move || drain(stderr));
    let start = Instant::now();
    let mut timeout = false;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if start.elapsed() > Duration::from_secs(120) {
            timeout = true;
            unsafe {
                libc::kill(-(child.id() as i32), libc::SIGKILL);
            }
            break child.wait()?;
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    let stdout = out
        .join()
        .map_err(|_| io::Error::other("bkmr stdout reader failed"))??;
    let stderr = err
        .join()
        .map_err(|_| io::Error::other("bkmr stderr reader failed"))??;
    if timeout {
        return Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "bkmr command timed out",
        ));
    }
    if !status.success() {
        return Err(io::Error::other(format!(
            "bkmr exited {status}: {}",
            String::from_utf8_lossy(&stderr)
                .chars()
                .take(1500)
                .collect::<String>()
        )));
    }
    String::from_utf8(stdout).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}
