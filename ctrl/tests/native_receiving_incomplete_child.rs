// A real, opt-in Central test executable. Include the same native crate Source
// at crate root so its existing private cfg(test) physical checkpoint exists.
// Ordinary ctrl/library builds have neither this entry nor its fixture bridge.
include!("../src/lib.rs");

fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    // No help/version/list or default libtest invocation can arm a fixture.
    // The selected paired gate must provide the exact native include command
    // and the root-local admission; missing prerequisites are a hard failure.
    #[cfg(unix)]
    {
        let fixture = match continuous_work::arm_native_inclusion_fixture(&args) {
            Ok(fixture) => fixture,
            Err(error) => {
                eprintln!("native inclusion fixture admission refused: {error}");
                std::process::exit(78);
            }
        };
        let mut surface = StdioTerminalSurface;
        let execution = run_cli_with_surface(&args, &CliEnvironment::from_process(), &mut surface);
        // Verbatim native formatter and native exit status, not a reconstructed
        // ActionResult. Fixture uncertainty is a separate stderr failure.
        // Restore owned permissions before stdout publication can fail or the
        // capture caller can close its pipe. Preserve the native output bytes
        // independently of fixture restoration's success/failure.
        let restoration = fixture.finish();
        if let Err(error) = &restoration {
            eprintln!("native inclusion fixture restoration/observation failed: {error}");
        }
        let encoded = format!("{}\n", execution.output);
        let mut stdout = std::io::stdout();
        let publication = std::io::Write::write_all(&mut stdout, encoded.as_bytes())
            .and_then(|()| std::io::Write::flush(&mut stdout));
        if let Err(error) = &publication {
            eprintln!("native inclusion fixture stdout publication failed: {error}");
        }
        std::process::exit(if restoration.is_ok() && publication.is_ok() {
            execution.exit_code
        } else {
            78
        });
    }
    #[cfg(not(unix))]
    {
        let _ = args;
        eprintln!("native inclusion fixture requires qualified Unix filesystem permissions");
        std::process::exit(78);
    }
}
