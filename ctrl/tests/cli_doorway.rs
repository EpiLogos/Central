use serde_json::Value;
use std::process::Command;

fn run_ctrl(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_ctrl"))
        .args(args)
        .output()
        .expect("ctrl binary should run")
}

#[test]
fn help_spellings_are_successful_and_point_to_native_capability_discovery() {
    for spelling in [["help"], ["--help"], ["-h"]] {
        let output = run_ctrl(&spelling);
        assert!(output.status.success());
        let stdout = String::from_utf8(output.stdout).unwrap();
        assert!(stdout.contains("ctrl capabilities"));
        assert!(stdout.contains("complete current native Action field"));
    }
}

#[test]
fn bare_ctrl_prints_usage_and_exits_like_invalid_input() {
    let output = run_ctrl(&[]);
    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(2));
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("Usage:"));
    assert!(stdout.contains("ctrl capabilities"));
    assert!(
        output.stderr.is_empty(),
        "no panic on the zero-argument path"
    );
}

#[test]
fn flag_only_invocations_name_an_error_instead_of_panicking() {
    for args in [vec!["--root", "/tmp"], vec!["--json"], vec!["--bogus-flag"]] {
        let output = run_ctrl(&args);
        assert!(
            !output.status.success(),
            "{args:?} should not be treated as a command"
        );
        assert_ne!(output.status.code(), Some(101), "{args:?} must not panic");
        let stdout = String::from_utf8(output.stdout).unwrap();
        assert!(!stdout.is_empty(), "{args:?} should name its error");
    }
}

#[test]
fn capabilities_is_exactly_the_existing_action_registry_projection() {
    let capabilities = run_ctrl(&["capabilities", "--json"]);
    let actions = run_ctrl(&["actions", "--json"]);
    assert!(capabilities.status.success());
    assert!(actions.status.success());

    let capabilities: Value = serde_json::from_slice(&capabilities.stdout).unwrap();
    let actions: Value = serde_json::from_slice(&actions.stdout).unwrap();
    assert_eq!(capabilities["action"], "action.list");
    assert_eq!(capabilities["data"]["actions"], actions["data"]["actions"]);
}
