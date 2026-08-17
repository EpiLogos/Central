use central_ctrl::{run_cli_with_surface, CliEnvironment, StdioTerminalSurface};
use std::process;

fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if matches!(args.as_slice(), [argument] if matches!(argument.as_str(), "--version" | "-V" | "version")) {
        println!("ctrl {}", env!("CARGO_PKG_VERSION"));
        return;
    }

    let mut surface = StdioTerminalSurface;
    let execution = run_cli_with_surface(&args, &CliEnvironment::from_process(), &mut surface);
    println!("{}", execution.output);
    process::exit(execution.exit_code);
}
