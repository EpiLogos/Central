use std::process::Command;

#[test]
fn binary_exposes_a_stable_version_discovery_command() {
    let binary = env!("CARGO_BIN_EXE_ctrl");
    for argument in ["--version", "-V", "version"] {
        let output = Command::new(binary).arg(argument).output().unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        let line = String::from_utf8(output.stdout).unwrap();
        assert!(
            line == format!("ctrl {}\n", env!("CARGO_PKG_VERSION"))
                || line.starts_with(&format!("ctrl {} (", env!("CARGO_PKG_VERSION"))),
            "version must be '<pkg-version>' or '<pkg-version> (<build-revision>)': {line:?}"
        );
        if let Some(revision) = line
            .trim()
            .strip_prefix(&format!("ctrl {} (", env!("CARGO_PKG_VERSION")))
            .and_then(|rest| rest.strip_suffix(')'))
        {
            assert!(
                !revision.is_empty() && revision.chars().all(|c| c.is_ascii_hexdigit()),
                "build revision must be non-empty hex: {revision:?}"
            );
        }
        assert!(output.stderr.is_empty());
    }
}
