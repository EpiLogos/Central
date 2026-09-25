//! Repository diff through the existing GitState owner. No fetch, index writes,
//! external diff drivers or textconv. Git paths are NUL delimited, never shell code.
use central_connector_sdk::{
    GitDiffFile, GitDiffReading, GitDiffRequest, PortError, PortErrorCode,
};
use std::{
    io::Read,
    path::Path,
    process::{Command, Stdio},
    thread,
};

fn bad(message: impl Into<String>) -> PortError {
    PortError::new(PortErrorCode::InvalidInput, message)
}
fn query(
    git: &Path,
    repo: &Path,
    args: &[String],
    cap: usize,
    allow_diff_exit: bool,
) -> Result<(Vec<u8>, bool), PortError> {
    let mut child = Command::new(git)
        .arg("--no-optional-locks")
        .arg("-C")
        .arg(repo)
        .args(args)
        .env("GIT_LITERAL_PATHSPECS", "1")
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| PortError::provider(format!("Cannot start Git: {e}")))?;
    let mut stderr = child.stderr.take().expect("piped stderr");
    let errors = thread::spawn(move || {
        let mut bytes = Vec::new();
        let mut buf = [0u8; 8192];
        while let Ok(n) = stderr.read(&mut buf) {
            if n == 0 {
                break;
            }
            let keep = n.min(16384usize.saturating_sub(bytes.len()));
            bytes.extend_from_slice(&buf[..keep]);
        }
        bytes
    });
    let stdout = child.stdout.take().expect("piped stdout");
    let (tx, rx) = std::sync::mpsc::channel();
    thread::spawn(move || {
        let mut bytes = Vec::new();
        let result = stdout
            .take((cap + 1) as u64)
            .read_to_end(&mut bytes)
            .map(|_| bytes);
        let _ = tx.send(result);
    });
    let started = std::time::Instant::now();
    let mut reading = None;
    let mut status = None;
    loop {
        if reading.is_none() {
            reading = rx.try_recv().ok();
        }
        if reading
            .as_ref()
            .is_some_and(|r| r.as_ref().map_or(true, |v| v.len() > cap))
        {
            let _ = child.kill();
        }
        if status.is_none() {
            status = child
                .try_wait()
                .map_err(|e| PortError::provider(e.to_string()))?;
        }
        if status.is_some() && reading.is_some() {
            break;
        }
        if started.elapsed() > std::time::Duration::from_secs(15) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(PortError::provider(
                "Git did not return this bounded diff read within 15 seconds",
            ));
        }
        thread::sleep(std::time::Duration::from_millis(5));
    }
    let mut bytes = reading
        .expect("completed stdout")
        .map_err(|e| PortError::provider(e.to_string()))?;
    let truncated = bytes.len() > cap;
    let status = status.expect("completed Git process");
    let errors = errors.join().unwrap_or_default();
    if !truncated && !status.success() && !(allow_diff_exit && status.code() == Some(1)) {
        return Err(PortError::provider(format!(
            "Git diff refused: {}",
            String::from_utf8_lossy(&errors).trim()
        )));
    }
    bytes.truncate(cap);
    Ok((bytes, truncated))
}
fn strings(args: &[&str]) -> Vec<String> {
    args.iter().map(|s| s.to_string()).collect()
}
fn revision(git: &Path, repo: &Path, value: &str) -> Result<String, PortError> {
    if value.is_empty()
        || value.len() > 256
        || value.starts_with('-')
        || value.chars().any(char::is_control)
    {
        return Err(bad("A bounded explicit Git revision is required"));
    }
    let args = vec![
        "rev-parse".into(),
        "--verify".into(),
        "--end-of-options".into(),
        format!("{value}^{{commit}}"),
    ];
    let (bytes, _) = query(git, repo, &args, 256, false)?;
    Ok(String::from_utf8(bytes)
        .map_err(|_| bad("Git revision is not UTF-8"))?
        .trim()
        .into())
}
fn fields(bytes: &[u8]) -> Result<Vec<String>, PortError> {
    bytes
        .split(|b| *b == 0)
        .filter(|b| !b.is_empty())
        .map(|b| {
            String::from_utf8(b.to_vec())
                .map_err(|_| bad("This diff contains a filename that is not valid UTF-8"))
        })
        .collect()
}

pub(crate) fn read(git: &Path, input: &GitDiffRequest) -> Result<GitDiffReading, PortError> {
    if !(1..=1_048_576).contains(&input.max_bytes) || !(1..=200).contains(&input.max_files) {
        return Err(bad(
            "Diff bounds must be 1–1048576 bytes per file and 1–200 files",
        ));
    }
    let root = input
        .repo_root
        .canonicalize()
        .map_err(|e| bad(e.to_string()))?;
    let (top, _) = query(
        git,
        &root,
        &strings(&["rev-parse", "--show-toplevel"]),
        16384,
        false,
    )?;
    let actual = std::path::PathBuf::from(String::from_utf8_lossy(&top).trim());
    if actual.canonicalize().ok().as_ref() != Some(&root) {
        return Err(bad("Diff target must be the repository or worktree root"));
    }
    let head = revision(git, &root, "HEAD").ok();
    let from = revision(git, &root, &input.from)?;
    let working = input.to == "working-tree";
    let to = if working {
        "working-tree".into()
    } else {
        revision(git, &root, &input.to)?
    };
    let mut basis = vec![
        "diff".into(),
        "--no-ext-diff".into(),
        "--no-textconv".into(),
        "--no-color".into(),
        "--find-renames".into(),
    ];
    if input.ignore_whitespace {
        basis.push("--ignore-all-space".into());
    }
    basis.push(from.clone());
    if !working {
        basis.push(to.clone());
    }
    let mut names = basis.clone();
    names.extend(strings(&["--name-status", "-z", "--"]));
    let (bytes, capped) = query(git, &root, &names, 4_194_304, false)?;
    if capped {
        return Err(bad(
            "Changed-path inventory exceeds 4 MiB; select a smaller repository comparison",
        ));
    }
    let rows = fields(&bytes)?;
    let mut paths = Vec::new();
    let mut i = 0;
    while i < rows.len() {
        let status = rows[i].clone();
        i += 1;
        let old = rows
            .get(i)
            .ok_or_else(|| bad("Incomplete Git path record"))?
            .clone();
        i += 1;
        let (old_path, new_path) = if status.starts_with('R') || status.starts_with('C') {
            let new = rows
                .get(i)
                .ok_or_else(|| bad("Incomplete Git rename record"))?
                .clone();
            i += 1;
            (Some(old), new)
        } else {
            (None, old)
        };
        paths.push((status, old_path, new_path));
    }
    if working {
        let (bytes, capped) = query(
            git,
            &root,
            &strings(&["ls-files", "--others", "--exclude-standard", "-z"]),
            4_194_304,
            false,
        )?;
        if capped {
            return Err(bad("Untracked-path inventory exceeds 4 MiB"));
        }
        for path in fields(&bytes)? {
            paths.push(("?".into(), None, path));
        }
    }
    let omitted_files = paths.len().saturating_sub(input.max_files);
    let mut files = Vec::new();
    let mut remaining = 2_097_152usize;
    for (status, old_path, new_path) in paths.into_iter().take(input.max_files) {
        let untracked = status == "?";
        // --no-index must never follow an untracked symlink outside the repo.
        if untracked
            && std::fs::symlink_metadata(root.join(&new_path))
                .map_err(|e| bad(e.to_string()))?
                .file_type()
                .is_symlink()
        {
            files.push(GitDiffFile {
                status,
                old_path,
                new_path,
                adds: None,
                dels: None,
                binary: false,
                patch: String::new(),
                truncated: true,
            });
            continue;
        }
        let base = if untracked {
            strings(&[
                "diff",
                "--no-index",
                "--no-ext-diff",
                "--no-textconv",
                "--no-color",
            ])
        } else {
            basis.clone()
        };
        let paths = if untracked {
            vec!["/dev/null".into(), new_path.clone()]
        } else {
            old_path
                .iter()
                .cloned()
                .chain(std::iter::once(new_path.clone()))
                .collect::<Vec<_>>()
        };
        let mut stats = base.clone();
        stats.extend(strings(&["--numstat", "-z", "--"]));
        stats.extend(paths.clone());
        let (nums, capped) = query(git, &root, &stats, 65536, untracked)?;
        if capped {
            return Err(bad("Git numstat exceeded the bound for one file"));
        }
        let nums = String::from_utf8_lossy(&nums);
        let first = nums.split('\0').next().unwrap_or("");
        let mut parts = first.split('\t');
        let a = parts.next().unwrap_or("");
        let d = parts.next().unwrap_or("");
        let binary = a == "-" || d == "-";
        let (patch, truncated) = if binary {
            (String::new(), false)
        } else if remaining == 0 {
            (String::new(), true)
        } else {
            let mut args = base;
            args.extend(strings(&["--patch", "--unified=3", "--"]));
            args.extend(paths);
            let cap = input.max_bytes.min(remaining);
            let (bytes, truncated) = query(git, &root, &args, cap, untracked)?;
            remaining = remaining.saturating_sub(bytes.len());
            (String::from_utf8_lossy(&bytes).into_owned(), truncated)
        };
        files.push(GitDiffFile {
            status,
            old_path,
            new_path,
            adds: a.parse().ok(),
            dels: d.parse().ok(),
            binary,
            patch,
            truncated,
        });
    }
    Ok(GitDiffReading {
        schema: "central.git-diff/v1".into(),
        repo_root: root,
        head_sha: head,
        from,
        to,
        ignore_whitespace: input.ignore_whitespace,
        truncated: omitted_files > 0 || files.iter().any(|f| f.truncated),
        files,
        omitted_files,
        observation: if working {
            "Working tree observation; files may change between reads"
        } else {
            "Exact committed revisions"
        }
        .into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };
    struct Repo(PathBuf);
    impl Drop for Repo {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn run(repo: &Path, args: &[&str]) -> String {
        let output = Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).into_owned()
    }
    fn repo() -> Repo {
        let path = std::env::temp_dir().join(format!(
            "central-diff-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&path).unwrap();
        run(&path, &["init", "-q"]);
        run(&path, &["config", "user.name", "Diff proof"]);
        run(&path, &["config", "user.email", "proof@example.invalid"]);
        Repo(path)
    }
    fn commit(repo: &Path) {
        run(repo, &["add", "-A"]);
        run(repo, &["commit", "-qm", "real source"]);
    }
    fn request(repo: &Path) -> GitDiffRequest {
        GitDiffRequest {
            repo_root: repo.into(),
            from: "HEAD".into(),
            to: "working-tree".into(),
            ignore_whitespace: false,
            max_bytes: 65536,
            max_files: 200,
        }
    }
    #[test]
    fn real_working_and_committed_changes_preserve_provenance_stats_paths_and_binary() {
        let repo = repo();
        let p = &repo.0;
        fs::write(p.join("one.txt"), "original\n").unwrap();
        fs::write(p.join("two.txt"), "old\n").unwrap();
        fs::write(p.join("image.png"), b"\0old").unwrap();
        commit(p);
        fs::write(p.join("one.txt"), "committed\n").unwrap();
        fs::write(p.join("two.txt"), "new\n").unwrap();
        commit(p);
        fs::write(p.join("one.txt"), "working\nextra\n").unwrap();
        fs::write(p.join("image.png"), b"\0new").unwrap();
        fs::write(p.join("new\tfile.txt"), "untracked\n").unwrap();
        let before = fs::read(p.join(".git/index")).unwrap();
        let working = read(Path::new("git"), &request(p)).unwrap();
        assert_eq!(working.files.len(), 3);
        assert!(working
            .files
            .iter()
            .any(|f| f.new_path == "image.png" && f.binary && f.patch.is_empty()));
        let one = working
            .files
            .iter()
            .find(|f| f.new_path == "one.txt")
            .unwrap();
        assert_eq!((one.adds, one.dels), (Some(2), Some(1)));
        assert_eq!(one.status, "M");
        let expected = run(
            p,
            &[
                "diff",
                "--no-ext-diff",
                "--no-textconv",
                "--no-color",
                "--find-renames",
                "HEAD",
                "--patch",
                "--unified=3",
                "--",
                "one.txt",
            ],
        );
        assert_eq!(one.patch, expected);
        let untracked = working.files.iter().find(|f| f.status == "?").unwrap();
        assert_eq!(untracked.new_path, "new\tfile.txt");
        assert_eq!(untracked.adds, Some(1));
        let mut prior = request(p);
        prior.from = "HEAD~1".into();
        prior.to = "HEAD".into();
        let committed = read(Path::new("git"), &prior).unwrap();
        assert_eq!(committed.files.len(), 2);
        assert_ne!(committed.to, working.to);
        assert!(committed.files.iter().any(|f| f.new_path == one.new_path));
        assert_eq!(
            working.head_sha.as_deref(),
            Some(run(p, &["rev-parse", "HEAD"]).trim())
        );
        assert_eq!(before, fs::read(p.join(".git/index")).unwrap());
    }
    #[test]
    fn real_rename_limits_whitespace_and_external_symlink_refusal() {
        let repo = repo();
        let p = &repo.0;
        fs::write(p.join("before.txt"), "same\ncontent\n").unwrap();
        fs::write(p.join("large.txt"), "old\n").unwrap();
        commit(p);
        run(p, &["mv", "before.txt", "after.txt"]);
        fs::write(p.join("large.txt"), "row\n".repeat(5000)).unwrap();
        let mut req = request(p);
        req.max_bytes = 128;
        let diff = read(Path::new("git"), &req).unwrap();
        assert!(diff.truncated);
        assert!(diff
            .files
            .iter()
            .any(|f| f.old_path.as_deref() == Some("before.txt")
                && f.new_path == "after.txt"
                && f.status.starts_with('R')));
        assert!(
            diff.files
                .iter()
                .find(|f| f.new_path == "large.txt")
                .unwrap()
                .truncated
        );
        req.max_files = 1;
        assert_eq!(read(Path::new("git"), &req).unwrap().omitted_files, 1);
        req.from = "--output=oops".into();
        assert!(read(Path::new("git"), &req).is_err());
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink("/etc/passwd", p.join("outside.txt")).unwrap();
            let diff = read(Path::new("git"), &request(p)).unwrap();
            let link = diff
                .files
                .iter()
                .find(|f| f.new_path == "outside.txt")
                .unwrap();
            assert!(link.truncated && link.patch.is_empty());
        }
    }
}
