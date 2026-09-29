use central_ctrl::{CliEnvironment, StdioTerminalSurface};
use central_macos_host::run_macos_cli;

fn main() {
    let mut args = std::env::args().skip(1).collect::<Vec<_>>();
    if let Some(code) = central_ctrl::cli_frontdoor::print_frontdoor(&mut args) {
        std::process::exit(code);
    }
    let environment = CliEnvironment::from_process();
    let mut surface = StdioTerminalSurface;
    let execution = run_macos_cli(&args, &environment, &mut surface);
    println!("{}", execution.output);
    std::process::exit(execution.exit_code);
}
