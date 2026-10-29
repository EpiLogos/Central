//! bkmr's CLI, not a second search engine. Every invocation selects one DB.
use serde_json::{json, Value};
use std::os::unix::{fs::PermissionsExt, process::CommandExt};
use std::{
    fs, io,
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

pub(crate) const VERSION: &str = "7.6.7";
/// The most candidates bkmr's hybrid search takes (a sqlite-vec limit).
pub(crate) const HYBRID_K_CEILING: usize = 4096;
/// The embedder every scope's bkmr runs with. One name, written into each
/// scope's config and surfaced by `central.file-map.inspect` — never ambient
/// and never silently defaulted twice. BGESmallENV15 is the efficiency pick
/// in fastembed's catalogue: a fraction of Nomic's size and latency for this
/// corpus's retrieval quality.
pub(crate) fn embedding_model() -> String {
    std::env::var("CENTRAL_BKMR_EMBEDDING_MODEL")
        .ok()
        .filter(|model| !model.trim().is_empty())
        .unwrap_or_else(|| "BGESmallENV15".into())
}

/// A scope's meaning-index coverage as `bkmr info` reports it. Every field is
/// what bkmr said; one it did not say is `None`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct EmbeddingCoverage {
    /// Dimensions of the configured embedder.
    pub dimensions: Option<u64>,
    /// Dimensions the vector table was created with.
    pub vector_dimensions: Option<u64>,
    /// Rows in the vector table.
    pub vectors: Option<u64>,
    /// Bookmarks bkmr counts as embedded, of those it counts as embeddable.
    pub embedded: Option<u64>,
    pub embeddable: Option<u64>,
}

impl EmbeddingCoverage {
    /// `ready` (every embeddable row has a vector), `partial`, `empty` (no vectors
    /// at all), `mismatched` (the vector table is not the embedder's size, so no
    /// query vector can match it), `nothing-to-embed`, or `unknown` (bkmr did not
    /// say enough).
    pub fn state(&self) -> &'static str {
        if let (Some(embedder), Some(table)) = (self.dimensions, self.vector_dimensions) {
            if embedder != table {
                return "mismatched";
            }
        }
        let (Some(embedded), Some(embeddable)) = (self.embedded, self.embeddable) else {
            return "unknown";
        };
        if embeddable == 0 {
            return "nothing-to-embed";
        }
        let vectors = self.vectors.unwrap_or(embedded);
        if vectors == 0 {
            "empty"
        } else if embedded >= embeddable && vectors >= embeddable {
            "ready"
        } else {
            "partial"
        }
    }
    pub fn to_json(&self, model: &str) -> Value {
        json!({"model":model,"dimensions":self.dimensions,"vector_dimensions":self.vector_dimensions,
            "vectors":self.vectors,"embedded":self.embedded,"embeddable":self.embeddable,
            "state":self.state()})
    }
}

/// Read the coverage lines of `bkmr info` (ANSI escapes ignored):
/// `Embedder: Embeddings enabled (dimensions: 384)`,
/// `Embedded: 453 of 453 embeddable bookmarks` and
/// `Vector table: vec_bookmarks (384 dims, 453 rows)` (`(empty)` for a table
/// with no rows, which then names no dimensions).
pub(crate) fn parse_embedding_coverage(info: &str) -> EmbeddingCoverage {
    let mut plain = String::with_capacity(info.len());
    let mut chars = info.chars();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            if chars.next() == Some('[') {
                for next in chars.by_ref() {
                    if ('@'..='~').contains(&next) {
                        break;
                    }
                }
            }
        } else {
            plain.push(c);
        }
    }
    let number_after = |line: &str, marker: &str| -> Option<u64> {
        let rest = &line[line.find(marker)? + marker.len()..];
        let digits: String = rest
            .trim_start()
            .chars()
            .take_while(char::is_ascii_digit)
            .collect();
        digits.parse().ok()
    };
    let mut coverage = EmbeddingCoverage::default();
    for line in plain.lines().map(str::trim) {
        if line.starts_with("Embedder:") {
            coverage.dimensions = number_after(line, "dimensions:");
        } else if line.starts_with("Embedded:") {
            coverage.embedded = number_after(line, "Embedded:");
            coverage.embeddable = number_after(line, " of ");
        } else if line.starts_with("Vector table:") {
            let inside = line.split('(').nth(1).unwrap_or_default();
            if inside.trim_start().starts_with("empty") {
                coverage.vectors = Some(0);
            } else {
                coverage.vector_dimensions = number_after(inside, "");
                coverage.vectors = line
                    .split(',')
                    .nth(1)
                    .and_then(|rest| number_after(rest, ""));
            }
        }
    }
    coverage
}

pub(crate) struct Backend {
    /// The tree bkmr reads (its WORLD base path and working directory).
    pub root: PathBuf,
    /// Where bkmr's database and config are written: `root` for a scope inside
    /// the ground, the ground-side state directory for an external scope.
    pub state: PathBuf,
    pub area: PathBuf,
}
impl Backend {
    pub fn with_state(root: &Path, state: &Path) -> Self {
        Self {
            root: root.into(),
            state: state.into(),
            area: state.join(".central/bkmr"),
        }
    }
    pub fn for_scope(scope: &super::file_map_catalog::Scope) -> Self {
        Self::with_state(&scope.root, &scope.state)
    }
    pub fn db(&self) -> PathBuf {
        self.area.join("index.db")
    }
    pub fn present(&self) -> bool {
        self.db().is_file()
    }
    pub fn version(&self) -> io::Result<String> {
        invoke(&["--version".into()], None, None, DEFAULT_TIMEOUT_SECS)
    }
    /// What the scope's meaning index actually holds, read from bkmr's own
    /// `info`. The provider's `semantic` flag says the embedder is configured;
    /// this says whether vectors exist (on 16 Oct 2026 a wrong-sized, empty vector
    /// table sat behind a `semantic: true`).
    pub fn embedding_coverage(&self) -> io::Result<EmbeddingCoverage> {
        Ok(parse_embedding_coverage(&self.run(&["info".into()])?))
    }
    pub fn prepare(&self) -> io::Result<()> {
        super::file_map::safe_directory(&self.state, Path::new(".central/bkmr/home/.config/bkmr"))?;
        let config = format!(
            "db_url = {}\n[base_paths]\nWORLD = {}\n\n[embeddings]\nmodel = {}\n",
            serde_json::to_string(&self.db().to_string_lossy())?,
            serde_json::to_string(&self.root.to_string_lossy())?,
            serde_json::to_string(&embedding_model())?
        );
        let path = self.area.join("home/.config/bkmr/config.toml");
        // bkmr 7.6.7's importer reloads default settings instead of the supplied
        // --config. Isolate HOME as well so BOTH code paths see this scope.
        if fs::read_to_string(&path).ok().as_deref() != Some(&config) {
            super::file_map::write_atomic(
                &self.state,
                &path,
                config.as_bytes(),
                crate::file_mutation::RecordDisposition::ReplaceOrCreate,
            )?;
        }
        if !self.present() {
            self.run(&["create-db".into(), self.db().to_string_lossy().into()])?;
        }
        fs::set_permissions(self.db(), fs::Permissions::from_mode(0o600))?;
        Ok(())
    }
    pub fn run(&self, args: &[String]) -> io::Result<String> {
        self.run_allowance(args, DEFAULT_TIMEOUT_SECS)
    }
    /// Commands that legitimately re-embed a whole scope — `backfill` after a
    /// model switch — need minutes, not the default per-call ceiling.
    pub fn run_allowance(&self, args: &[String], timeout_secs: u64) -> io::Result<String> {
        super::file_map::safe_member(&self.state, ".central/bkmr", true)?;
        for name in [
            "index.db",
            "index.db-wal",
            "index.db-shm",
            "home/.config/bkmr/config.toml",
        ] {
            let p = self.area.join(name);
            if fs::symlink_metadata(&p).is_ok() {
                super::file_map::safe_member(
                    &self.state,
                    p.strip_prefix(&self.state)
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
        invoke(
            &argv,
            Some(&self.root),
            Some(&self.area.join("home")),
            timeout_secs,
        )
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
        let (args, hybrid) = search_args(query, tags, hybrid, limit);
        search_records(&self.run(&args)?, hybrid)
    }
}

/// The bkmr argv for one search, and whether it runs as a hybrid search. An
/// empty (or all-whitespace) query is no text filter: bkmr would hand FTS5 an
/// empty match (`fts5: syntax error near ""`) and fail, so the search runs as
/// a plain listing narrowed by tags only, in either mode.
fn search_args(query: &str, tags: &[String], hybrid: bool, limit: usize) -> (Vec<String>, bool) {
    let hybrid = hybrid && !query.trim().is_empty();
    // bkmr's hybrid path feeds the limit straight into a sqlite-vec knn
    // query, which rejects k above 4096. Full-text search has no such
    // ceiling, so only the hybrid invocation is capped, and at that ceiling:
    // it was capped at 100, and the authority, freshness and scope filters run
    // after this cut, so a federated query could lose every candidate to them.
    let limit = if hybrid {
        limit.min(HYBRID_K_CEILING)
    } else {
        limit
    };
    let mut args: Vec<String> = vec![
        if hybrid { "hsearch" } else { "search" }.into(),
        "--json".into(),
        "--np".into(),
        "--limit".into(),
        limit.to_string(),
    ];
    if !tags.is_empty() {
        args.extend(["--tags".into(), tags.join(",")]);
    }
    if !query.trim().is_empty() {
        // The end-of-options delimiter keeps a query beginning '-' as data.
        let rendered = if hybrid {
            // bkmr's hybrid search fuses an FTS5 arm with an embedding
            // arm and hands both the same string, so a hyphenated term
            // reaches FTS5 as column syntax here too
            // (`hsearch "now-context"` fails with "no such column:
            // context" and takes the whole map search with it). Only the
            // terms FTS5 would read as syntax are quoted, so ordinary
            // words still reach the embedding arm as the caller wrote them.
            fts5_quote_unsafe_terms(query)
        } else {
            // Full-text search hands the query to SQLite FTS5, where a
            // bare hyphen is column syntax: `aikit-knowledge-work-coverage-spec`
            // failed the whole map search with "no such column: knowledge".
            // Quoted terms read as the literals they are.
            fts5_quote(query)
        };
        args.extend(["--".into(), rendered]);
    }
    (args, hybrid)
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

/// Quote only the whitespace-separated terms that FTS5 would read as syntax.
/// An FTS5 bareword is made of letters, digits, underscores and non-ASCII
/// characters; one trailing `*` keeps its meaning as a prefix query. Anything
/// else (`now-context`, `col:term`, a lone `*`, a term carrying `"`) becomes a
/// double-quoted literal with embedded quotes doubled. Plain words pass
/// through unchanged, so a query that worked before is rendered as it was.
pub(crate) fn fts5_quote_unsafe_terms(query: &str) -> String {
    query
        .split_whitespace()
        .map(|term| {
            let stem = term.strip_suffix('*').unwrap_or(term);
            let bareword = !stem.is_empty()
                && stem
                    .chars()
                    .all(|c| c.is_alphanumeric() || c == '_' || !c.is_ascii());
            if bareword {
                term.to_string()
            } else {
                format!("\"{}\"", term.replace('"', "\"\""))
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
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

/// What a search printed, as records. bkmr's hybrid search with `--np` and no
/// hits prints its "No bookmarks found" notice to stderr and nothing to stdout
/// (exit 0), where full-text search prints `[]`. For the hybrid path an empty
/// stdout is therefore an empty answer; for full-text it is still a failure,
/// as is any other output that is not a JSON array.
fn search_records(text: &str, hybrid: bool) -> io::Result<Vec<Value>> {
    if hybrid && text.trim().is_empty() {
        return Ok(Vec::new());
    }
    records(text)
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
/// Everything a bkmr stream carries. A pooled scope's full row listing scales
/// with its source count, and a size ceiling here turned a growing pool into a
/// map that stopped answering at all; the child's own timeout bounds the wait.
fn drain(mut stream: impl Read) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes)?;
    Ok(bytes)
}
const DEFAULT_TIMEOUT_SECS: u64 = 120;

fn invoke(
    args: &[String],
    cwd: Option<&Path>,
    home: Option<&Path>,
    timeout_secs: u64,
) -> io::Result<String> {
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
        // The isolated HOME would otherwise give every scope its own fastembed
        // model download; share the invoking user's cache unless the caller
        // pinned one.
        if std::env::var_os("FASTEMBED_CACHE_DIR").is_none() {
            if let Some(user_home) = std::env::var_os("HOME") {
                #[cfg(target_os = "macos")]
                let cache = PathBuf::from(&user_home).join("Library/Caches/bkmr/models");
                #[cfg(not(target_os = "macos"))]
                let cache = PathBuf::from(&user_home).join(".cache/bkmr/models");
                command.env("FASTEMBED_CACHE_DIR", cache);
            }
        }
    }
    command.env_remove("BKMR_DB_URL").env("NO_COLOR", "1");
    let deleting = args
        .first()
        .is_some_and(|arg| arg == "delete" || arg == "clear-embeddings")
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
        if start.elapsed() > Duration::from_secs(timeout_secs) {
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

#[cfg(test)]
mod tests {
    use super::{fts5_quote, fts5_quote_unsafe_terms, parse_embedding_coverage, search_args};
    use crate::file_map_backend::Backend;

    fn info(embedder: &str, embedded: &str, table: &str) -> String {
        format!(
            "\x1b[1mbkmr\x1b[0m\n  Embedder: {embedder}\n  Embedded: {embedded}\n  Vector table: {table}\n"
        )
    }

    #[test]
    fn empty_query_is_no_text_filter_in_every_mode() {
        for hybrid in [false, true] {
            for query in ["", "   "] {
                let (args, ran_hybrid) = search_args(query, &["a".into()], hybrid, 500);
                assert!(!ran_hybrid, "{query:?} hybrid={hybrid}");
                assert_eq!(args[0], "search");
                assert!(!args.iter().any(|a| a == "--"), "{args:?}");
                assert!(args.iter().any(|a| a == "a"), "tags still narrow: {args:?}");
                assert!(args.iter().any(|a| a == "500"), "{args:?}");
            }
        }
        let (args, ran_hybrid) = search_args("now-context", &[], true, 500);
        assert!(ran_hybrid);
        assert_eq!(args[0], "hsearch");
        assert_eq!(args.last().unwrap(), "\"now-context\"");
    }

    #[test]
    fn hybrid_asks_bkmr_for_up_to_its_own_ceiling_and_full_text_for_all() {
        let (args, _) = search_args("now", &[], true, 500);
        assert!(args.iter().any(|a| a == "500"), "{args:?}");
        let (args, _) = search_args("now", &[], true, 1_000_000);
        assert!(args.iter().any(|a| a == "4096"), "{args:?}");
        let (args, _) = search_args("now", &[], false, 1_000_000);
        assert!(args.iter().any(|a| a == "1000000"), "{args:?}");
    }

    #[test]
    fn a_fully_embedded_index_reads_ready() {
        let coverage = parse_embedding_coverage(&info(
            "Embeddings enabled (dimensions: 384)",
            "453 of 453 embeddable bookmarks",
            "vec_bookmarks (384 dims, 453 rows)",
        ));
        assert_eq!(coverage.dimensions, Some(384));
        assert_eq!(coverage.vector_dimensions, Some(384));
        assert_eq!(coverage.vectors, Some(453));
        assert_eq!(coverage.embedded, Some(453));
        assert_eq!(coverage.embeddable, Some(453));
        assert_eq!(coverage.state(), "ready");
    }

    #[test]
    fn an_empty_vector_table_names_no_dimensions_and_reads_empty() {
        let coverage = parse_embedding_coverage(&info(
            "Embeddings enabled (dimensions: 384)",
            "0 of 453 embeddable bookmarks",
            "vec_bookmarks (empty)",
        ));
        assert_eq!(coverage.vector_dimensions, None);
        assert_eq!(coverage.vectors, Some(0));
        assert_eq!(coverage.state(), "empty");
    }

    #[test]
    fn missing_vectors_read_partial_or_empty() {
        let partial = parse_embedding_coverage(&info(
            "Embeddings enabled (dimensions: 384)",
            "100 of 453 embeddable bookmarks",
            "vec_bookmarks (384 dims, 100 rows)",
        ));
        assert_eq!(partial.state(), "partial");
        let empty = parse_embedding_coverage(&info(
            "Embeddings enabled (dimensions: 384)",
            "0 of 453 embeddable bookmarks",
            "vec_bookmarks (384 dims, 0 rows)",
        ));
        assert_eq!(empty.state(), "empty");
    }

    /// The 16 Oct 2026 incident: a 768-d vector table behind a 384-d embedder
    /// stayed silently unusable behind `semantic: true`.
    #[test]
    fn a_vector_table_the_embedder_cannot_match_reads_mismatched() {
        let coverage = parse_embedding_coverage(&info(
            "Embeddings enabled (dimensions: 384)",
            "0 of 453 embeddable bookmarks",
            "vec_bookmarks (768 dims, 0 rows)",
        ));
        assert_eq!(coverage.state(), "mismatched");
    }

    #[test]
    fn an_index_with_nothing_to_embed_or_an_unreadable_info_is_not_a_guess() {
        let nothing = parse_embedding_coverage(&info(
            "Embeddings enabled (dimensions: 384)",
            "0 of 0 embeddable bookmarks",
            "vec_bookmarks (384 dims, 0 rows)",
        ));
        assert_eq!(nothing.state(), "nothing-to-embed");
        assert_eq!(parse_embedding_coverage("").state(), "unknown");
        assert_eq!(
            parse_embedding_coverage("Error: no such table").state(),
            "unknown"
        );
        // A line that names no number leaves its field absent rather than zero.
        let blank = parse_embedding_coverage("Embedded: unavailable\nVector table: absent\n");
        assert_eq!(blank.embedded, None);
        assert_eq!(blank.vectors, None);
        assert_eq!(blank.state(), "unknown");
    }

    #[test]
    fn a_hybrid_search_with_no_hits_is_an_empty_answer_not_a_parse_failure() {
        use super::search_records;
        // bkmr 7.6.7: stdout is empty, the notice goes to stderr, exit 0.
        assert!(search_records("", true).unwrap().is_empty());
        assert!(search_records("\n", true).unwrap().is_empty());
        assert!(search_records("[]\n", true).unwrap().is_empty());
    }

    #[test]
    fn empty_output_is_still_a_failure_for_full_text_and_other_prose_everywhere() {
        use super::search_records;
        // Full-text search always prints a JSON array, so nothing at all is wrong.
        assert!(search_records("", false).is_err());
        assert!(search_records("Error: something broke", true).is_err());
        assert!(search_records("No bookmarks found", true).is_err());
    }

    #[test]
    fn the_hybrid_query_quotes_a_hyphenated_term_and_leaves_plain_words_alone() {
        assert_eq!(fts5_quote_unsafe_terms("now-context"), "\"now-context\"");
        assert_eq!(
            fts5_quote_unsafe_terms("Redis now-context"),
            "Redis \"now-context\""
        );
        assert_eq!(
            fts5_quote_unsafe_terms("redis now context"),
            "redis now context"
        );
        assert_eq!(fts5_quote_unsafe_terms("jev_redis kev0"), "jev_redis kev0");
    }

    #[test]
    fn the_hybrid_query_keeps_a_prefix_star_and_quotes_other_syntax() {
        assert_eq!(fts5_quote_unsafe_terms("kev*"), "kev*");
        assert_eq!(fts5_quote_unsafe_terms("*"), "\"*\"");
        assert_eq!(fts5_quote_unsafe_terms("col:term"), "\"col:term\"");
        assert_eq!(
            fts5_quote_unsafe_terms("say \"hi\" bimbā"),
            "say \"\"\"hi\"\"\" bimbā"
        );
        assert_eq!(fts5_quote_unsafe_terms("   "), "");
        assert_eq!(fts5_quote_unsafe_terms(""), "");
    }

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
        let backend = Backend::with_state(&dir, &dir);
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

    /// The same regression on the hybrid path, which fed the raw query to
    /// FTS5 until `fts5_quote_unsafe_terms`: `hsearch "now-context"` failed
    /// with `no such column: context`. Same gate as the full-text lane.
    #[test]
    fn a_hyphenated_hybrid_query_returns_hits_instead_of_an_fts_failure() {
        if std::env::var("CENTRAL_FILE_MAP_BKMR_CONFORMANCE").as_deref() != Ok("real") {
            eprintln!("set CENTRAL_FILE_MAP_BKMR_CONFORMANCE=real to run the real-bkmr regression");
            return;
        }
        let dir = std::env::temp_dir().join(format!(
            "ctrl-file-map-hybrid-regression-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let dir = std::fs::canonicalize(dir).unwrap();
        let backend = Backend::with_state(&dir, &dir);
        backend.prepare().expect("real bkmr prepares an index");
        backend
            .run(&[
                "add".into(),
                "https://map.local/now-context-delivery".into(),
                "--title".into(),
                "now-context delivery".into(),
                "--description".into(),
                "prepared now-context delivery for a participant".into(),
                "--no-web".into(),
                "--no-embed".into(),
            ])
            .expect("real bkmr adds the record");
        let hits = backend
            .search("now-context", &[], true, 10)
            .expect("a hyphenated hybrid query must not fail FTS syntax");
        assert!(
            !hits.is_empty(),
            "the hyphenated hybrid query finds the record it names"
        );
        let none = backend
            .search("unmatched-zzqqxx", &[], true, 10)
            .expect("a hybrid query with no hits is an empty answer, not a failure");
        assert!(none.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// bkmr's own `info` must stay readable by `parse_embedding_coverage`:
    /// a rewording in a bkmr release would otherwise turn every scope's
    /// reading into `unknown` without a test noticing. Same gate as above.
    #[test]
    fn the_real_info_output_parses_into_a_coverage_state() {
        if std::env::var("CENTRAL_FILE_MAP_BKMR_CONFORMANCE").as_deref() != Ok("real") {
            eprintln!("set CENTRAL_FILE_MAP_BKMR_CONFORMANCE=real to run the real-bkmr regression");
            return;
        }
        let dir = std::env::temp_dir().join(format!(
            "ctrl-file-map-coverage-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let dir = std::fs::canonicalize(dir).unwrap();
        let backend = Backend::with_state(&dir, &dir);
        backend.prepare().expect("real bkmr prepares an index");
        backend
            .run(&[
                "add".into(),
                "coverage probe text".into(),
                "--title".into(),
                "coverage probe".into(),
                "--type".into(),
                "text".into(),
                "--no-web".into(),
            ])
            .expect("real bkmr adds and embeds the record");
        let coverage = backend.embedding_coverage().expect("info runs");
        assert_eq!(coverage.embeddable, Some(1), "{coverage:?}");
        assert_eq!(coverage.embedded, Some(1), "{coverage:?}");
        assert_eq!(coverage.vectors, Some(1), "{coverage:?}");
        assert_eq!(coverage.dimensions, coverage.vector_dimensions);
        assert_eq!(coverage.state(), "ready", "{coverage:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
