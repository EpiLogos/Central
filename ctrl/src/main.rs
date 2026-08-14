use std::process::ExitCode;
use central_ctrl::{run, ProcessContext};

fn main() -> ExitCode {
    let output = run(std::env::args().skip(1).collect(), ProcessContext::from_process());
    println!("{}", output.render());
    ExitCode::from(output.exit_code as u8)
}
