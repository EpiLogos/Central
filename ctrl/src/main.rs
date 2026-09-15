use central_ctrl::{run_cli_with_surface, CliEnvironment, StdioTerminalSurface};
use std::process;

const HELP: &str = "Central ctrl\n\nUsage:\n  ctrl --version\n  ctrl help\n  ctrl capabilities [--json]\n  ctrl actions [--json]\n  ctrl action run <ACTION> [JSON] [--json]\n  ctrl root | init | doctor\n  ctrl system [--json]        (Wave 5 owner System disclosure)
  ctrl config-contribution --json   (Configuration Plane owner contribution)
  ctrl config <validate|plan|apply|reset> ... [--json]  (owner-native transport)\n  ctrl work <list|search|open|reveal> ...\n  ctrl control <open|search|index> ...\n  ctrl machine <inspect|account|adopt-current|declaration|plan|apply|verify> ...\n  ctrl recovery plan <ROLE>\n  ctrl recover <ROLE>\n  ctrl pick\n\nUse `ctrl capabilities` (or `ctrl actions`) for the complete current native Action field. Product operations remain owned by Central's Action registry; this help is only the stable command doorway.";

fn top_level_command_index(args: &[String]) -> Option<usize> {
    let mut positional = Vec::new();
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--json" => index += 1,
            "--root" => {
                index += 2;
            }
            argument if argument.starts_with("--root=") => index += 1,
            _ => {
                positional.push(index);
                index += 1;
            }
        }
    }
    // `then`, never `then_some`: `then_some` evaluates its argument eagerly,
    // so the length guard would not stop `positional[0]` from panicking on an
    // all-flag invocation (`ctrl`, `ctrl --root PATH` with no command).
    (positional.len() == 1).then(|| positional[0])
}

fn main() {
    let mut args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.is_empty() {
        // No command at all: show the doorway instead of falling through.
        // Exit 2 matches the unknown-command convention (invalid input, not a crash).
        println!("{HELP}");
        process::exit(2);
    }
    if matches!(args.as_slice(), [argument] if matches!(argument.as_str(), "--version" | "-V" | "version"))
    {
        println!(
            "ctrl {} ({})",
            env!("CARGO_PKG_VERSION"),
            option_env!("SUITE_BUILD_REVISION").unwrap_or("unknown")
        );
        return;
    }

    if let Some(index) = top_level_command_index(&args) {
        match args[index].as_str() {
            "help" | "--help" | "-h" => {
                println!("{HELP}");
                return;
            }
            "capabilities" => args[index] = "actions".to_owned(),
            _ => {}
        }
    }

    let mut surface = StdioTerminalSurface;
    let execution = run_cli_with_surface(&args, &CliEnvironment::from_process(), &mut surface);
    println!("{}", execution.output);
    process::exit(execution.exit_code);
}
