use central_ctrl::{CliEnvironment, StdioTerminalSurface};
use central_macos_host::run_macos_cli;

fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let environment = CliEnvironment::from_process();
    let mut surface = StdioTerminalSurface;
    let execution = run_macos_cli(&args, &environment, &mut surface);
    println!("{}", execution.output);
    std::process::exit(execution.exit_code);
}
