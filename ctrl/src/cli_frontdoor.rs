const HELP: &str = "Central ctrl — the owner's register: orient, enter Project work, Return.

Orientation, entry and health (everyday):
  ctrl root | init | doctor         where this machine stands; `init` is
                                    contextual setup and never runs by itself
  ctrl work <list|search|open|reveal> ...
                                    enter Project material by task, not by path
  ctrl pick                         choose a Project to stand in (a human
                                    navigation entry; never launched for --json)

Authored ground (contextual):
  ctrl control <open|search|index> ...
                                    read and search the authored Control ground;
                                    indexing is Operator work, source authority
                                    stays with the human ground

Developer reading (contextual, read-only):
  ctrl git <census|tree|graph> [Project]
                                    open branches, worktrees and history — a
                                    reading, never a Git mutation

Machine and recovery (Operator):
  ctrl machine <inspect|account|adopt-current|declaration|plan|apply|verify> ...
                                    inspect and account freely; declaration
                                    through verify are explicit Operator steps
  ctrl recovery plan <ROLE>
  ctrl recover <ROLE>               recovery keeps the exact existing Role and
                                    source ownership

Configuration plane:
  ctrl system [--json]              owner System disclosure
  ctrl config <validate|plan|apply|reset> ... [--json]
                                    owner-native settings transport
  ctrl config-contribution --json   Configuration Plane owner contribution

Action infrastructure (canonical discovery and dispatch):
  ctrl capabilities [--json]
  ctrl actions [--json]             the complete current native Action field
  ctrl action run <ACTION> [JSON] [--json]
                                    dispatch one owner Action; the effect and
                                    its authority belong to that Action

  ctrl --version
  ctrl help

Use `ctrl capabilities` (or `ctrl actions`) for the complete current native
Action field. Product operations remain owned by Central's Action registry;
this help is only the stable command doorway.";

fn top_level_command_index(args: &[String]) -> Option<usize> {
    let mut positional = Vec::new();
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--json" => index += 1,
            "--root" => {
                index += 2;
            }
            argument if argument.starts_with("--root=") => index += 1,
            _ => {
                positional.push(index);
                index += 1;
            }
        }
    }
    // `then`, never `then_some`: `then_some` evaluates its argument eagerly,
    // so the length guard would not stop `positional[0]` from panicking on an
    // all-flag invocation (`ctrl`, `ctrl --root PATH` with no command).
    (positional.len() == 1).then(|| positional[0])
}

pub fn print_frontdoor(args: &mut Vec<String>) -> Option<i32> {
    if args.is_empty() {
        // No command at all: show the doorway instead of falling through.
        // Exit 2 matches the unknown-command convention (invalid input, not a crash).
        println!("{HELP}");
        return Some(2);
    }
    if matches!(args.as_slice(), [argument] if matches!(argument.as_str(), "--version" | "-V" | "version"))
    {
        println!(
            "ctrl {} ({})",
            env!("CARGO_PKG_VERSION"),
            option_env!("SUITE_BUILD_REVISION").unwrap_or("unknown")
        );
        return Some(0);
    }

    if let Some(index) = top_level_command_index(args) {
        match args[index].as_str() {
            "help" | "--help" | "-h" => {
                println!("{HELP}");
                return Some(0);
            }
            "capabilities" => args[index] = "actions".to_owned(),
            _ => {}
        }
    }

    None
}
