use central_ctrl::{run_cli, CliEnvironment};
use std::process;

fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let execution = run_cli(&args, &CliEnvironment::from_process());
    println!("{}", execution.output);
    process::exit(execution.exit_code);
}
