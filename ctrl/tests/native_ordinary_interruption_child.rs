// Explicit qualification includes the same production CLI/owner Source.
// The ordinary library/binary has no interruption fixture bridge.
include!("../src/lib.rs");

fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    #[cfg(unix)]
    {
        if let Err(error) = file_mutation::native_interruption_fixture::arm(&args) {
            eprintln!("native ordinary interruption admission refused: {error}");
            std::process::exit(78);
        }
        let mut surface = StdioTerminalSurface;
        let execution = run_cli_with_surface(&args, &CliEnvironment::from_process(), &mut surface);
        println!("{}", execution.output);
        // Reaching normal return means the required kill never occurred.
        std::process::exit(if execution.exit_code == 0 { 78 } else { execution.exit_code });
    }
    #[cfg(not(unix))]
    {
        let _ = args;
        eprintln!("native ordinary interruption requires a qualified Unix host");
        std::process::exit(78);
    }
}
