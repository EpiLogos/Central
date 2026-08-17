use std::process::Command;

#[test]
fn binary_exposes_a_stable_version_discovery_command() {
    let binary = env!("CARGO_BIN_EXE_ctrl");
    for argument in ["--version", "-V", "version"] {
        let output = Command::new(binary).arg(argument).output().unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        assert_eq!(
            String::from_utf8(output.stdout).unwrap(),
            format!("ctrl {}\n", env!("CARGO_PKG_VERSION"))
        );
        assert!(output.stderr.is_empty());
    }
}
