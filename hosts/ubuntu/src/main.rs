use central_ctrl::{
    create_default_connector_registry, run_cli_with_runtime, CliEnvironment, ConnectorContext,
    StdioTerminalSurface,
};
use central_ubuntu_connectors::UbuntuServerConnector;

fn main() {
    let mut connectors = create_default_connector_registry();
    connectors
        .register(UbuntuServerConnector::new())
        .expect("Ubuntu Connector manifest is valid");
    let connector_context = ConnectorContext::current();
    let environment = CliEnvironment::from_process();
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let mut surface = StdioTerminalSurface;
    let execution = run_cli_with_runtime(
        &args,
        &environment,
        &mut surface,
        &connectors,
        &connector_context,
    );
    if !execution.output.is_empty() {
        println!("{}", execution.output);
    }
    std::process::exit(execution.exit_code);
}
