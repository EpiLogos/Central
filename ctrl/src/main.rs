use central_ctrl::{ProcessContext, run};
use std::process::ExitCode;

fn main() -> ExitCode {
    let output = run(
        std::env::args().skip(1).collect(),
        ProcessContext::from_process(),
    );
    println!("{}", output.render());
    ExitCode::from(output.exit_code as u8)
}
