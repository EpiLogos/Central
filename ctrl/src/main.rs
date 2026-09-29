use central_ctrl::{run_cli_with_surface, CliEnvironment, StdioTerminalSurface};
use std::process;

fn main() {
    let mut args = std::env::args().skip(1).collect::<Vec<_>>();
    if let Some(code) = central_ctrl::cli_frontdoor::print_frontdoor(&mut args) {
        process::exit(code);
    }

    let mut surface = StdioTerminalSurface;
    let execution = run_cli_with_surface(&args, &CliEnvironment::from_process(), &mut surface);
    println!("{}", execution.output);
    process::exit(execution.exit_code);
}
