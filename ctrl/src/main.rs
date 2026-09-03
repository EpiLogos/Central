use central_ctrl::{run_cli_with_surface, CliEnvironment, StdioTerminalSurface};
use std::process;

const HELP: &str = "Central ctrl\n\nUsage:\n  ctrl --version\n  ctrl help\n  ctrl capabilities [--json]\n  ctrl actions [--json]\n  ctrl action run <ACTION> [JSON] [--json]\n  ctrl root | init | doctor\n  ctrl work <list|search|open|reveal> ...\n  ctrl control <open|search> ...\n  ctrl machine <inspect|account|declaration|plan|apply|verify> ...\n  ctrl recovery plan <ROLE>\n  ctrl recover <ROLE>\n  ctrl pick\n\nUse `ctrl capabilities` (or `ctrl actions`) for the complete current native Action field. Product operations remain owned by Central's Action registry; this help is only the stable command doorway.";

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
    (positional.len() == 1).then_some(positional[0])
}

fn main() {
    let mut args = std::env::args().skip(1).collect::<Vec<_>>();
    if matches!(args.as_slice(), [argument] if matches!(argument.as_str(), "--version" | "-V" | "version")) {
        println!("ctrl {}", env!("CARGO_PKG_VERSION"));
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
