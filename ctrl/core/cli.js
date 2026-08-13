import { createDefaultRuntime } from "./runtime.js";
import { failure, ResultStatus } from "./results.js";

const COMMANDS = new Map([
  ["root", "central.root"],
  ["init", "central.init"],
  ["doctor", "central.doctor"],
  ["actions", "action.list"],
  ["action.list", "action.list"],
  ["central.root", "central.root"],
  ["central.init", "central.init"],
  ["central.doctor", "central.doctor"],
  ["work.list", "work.list"],
  ["work.search", "work.search"],
  ["work.open", "work.open"],
  ["open", "work.open"],
  ["control.open", "control.open"],
  ["control.search", "control.search"],
]);

function parseArguments(argv) {
  const positional = [];
  let structured = false;
  let explicitRoot;

  for (let index = 0; index < argv.length; index += 1) {
    const argument = argv[index];
    if (argument === "--json") {
      structured = true;
      continue;
    }
    if (argument === "--root") {
      const value = argv[index + 1];
      if (value === undefined || value.startsWith("--")) {
        return { structured, error: "--root requires a path." };
      }
      explicitRoot = value;
      index += 1;
      continue;
    }
    if (argument.startsWith("--root=")) {
      explicitRoot = argument.slice("--root=".length);
      if (explicitRoot === "") return { structured, error: "--root requires a path." };
      continue;
    }
    if (argument.startsWith("--")) {
      return { structured, error: `Unknown option: ${argument}` };
    }
    positional.push(argument);
  }

  if (positional.length === 0) return { structured, error: "An Action or command is required." };

  let commandKey;
  let input = {};
  if (positional[0] === "action" && positional[1] === "list") {
    commandKey = "action.list";
    if (positional.length !== 2) return { structured, error: "action list takes no input." };
  } else if (positional[0] === "work" && positional[1] === "list") {
    commandKey = "work.list";
    if (positional.length !== 2) return { structured, error: "work list takes no input." };
  } else if (positional[0] === "work" && positional[1] === "search") {
    commandKey = "work.search";
    if (positional.length < 3) return { structured, error: "work search requires a query." };
    input = { query: positional.slice(2).join(" ") };
  } else if (positional[0] === "work" && positional[1] === "open") {
    commandKey = "work.open";
    if (positional.length < 3) return { structured, error: "work open requires a name or search." };
    input = { query: positional.slice(2).join(" ") };
  } else if (positional[0] === "control" && positional[1] === "open") {
    commandKey = "control.open";
    if (positional.length !== 3) return { structured, error: "control open requires one Control root." };
    input = { target: positional[2] };
  } else if (positional[0] === "control" && positional[1] === "search") {
    commandKey = "control.search";
    if (positional.length < 3) return { structured, error: "control search requires a query." };
    input = { query: positional.slice(2).join(" ") };
  } else {
    commandKey = positional[0];
    if (commandKey === "work.open" || commandKey === "open") {
      if (positional.length < 2) return { structured, error: `${commandKey} requires a name or search.` };
      input = { query: positional.slice(1).join(" ") };
    } else if (commandKey === "work.search") {
      if (positional.length < 2) return { structured, error: "work.search requires a query." };
      input = { query: positional.slice(1).join(" ") };
    } else if (commandKey === "control.open") {
      if (positional.length !== 2) return { structured, error: "control.open requires one Control root." };
      input = { target: positional[1] };
    } else if (commandKey === "control.search") {
      if (positional.length < 2) return { structured, error: "control.search requires a query." };
      input = { query: positional.slice(1).join(" ") };
    } else if (positional.length !== 1) {
      return { structured, error: `Unexpected arguments: ${positional.slice(1).join(" ")}` };
    }
  }

  const actionId = COMMANDS.get(commandKey);
  if (!actionId) return { structured, error: `Unknown command: ${commandKey}` };

  return { structured, explicitRoot, actionId, input };
}

function renderDoctorDetails(details) {
  const lines = [`Central root: ${details.root}`, `Valid: ${details.valid ? "yes" : "no"}`];
  for (const check of details.checks ?? []) {
    lines.push(`${check.valid ? "ok" : "missing"}  ${check.path}`);
  }
  return lines.join("\n");
}

export function renderHuman(result) {
  if (!result.ok) {
    if (result.status === ResultStatus.INVALID_CENTRAL_STRUCTURE && result.error.details) {
      return `${result.error.message}\n${renderDoctorDetails(result.error.details)}`;
    }
    return `${result.error.code}: ${result.error.message}`;
  }

  switch (result.action) {
    case "central.root":
      return `${result.data.path} (${result.data.source})`;
    case "central.init":
      return `Initialized Central at ${result.data.root}`;
    case "central.doctor":
      return renderDoctorDetails(result.data);
    case "action.list":
      return result.data.actions.map((action) => `${action.id}\t${action.title}`).join("\n");
    case "control.open":
      return `${result.data.target}\t${result.data.path}`;
    case "control.search":
      return result.data.matches.map((match) => `${match.sourcePath}:${match.line}\t${match.text}`).join("\n");
    case "work.list": {
      const connector = result.data.diagnostics.selectedConnector?.id ?? "none";
      const items = result.data.items.map((item) => `${item.name}\t${item.path}`);
      return [`Connector: ${connector}`, ...items].join("\n");
    }
    case "work.search":
      return result.data.matches.map((item) => `${item.name}\t${item.path}`).join("\n");
    case "work.open":
      return `${result.data.item.name}\t${result.data.item.path}`;
    default:
      return JSON.stringify(result.data, null, 2);
  }
}

function exitCodeFor(result) {
  if (result.ok) return 0;
  if (result.status === ResultStatus.INVALID_INPUT) return 2;
  if (result.status === ResultStatus.INVALID_CENTRAL_STRUCTURE) return 3;
  return 1;
}

export async function runCli(argv, { env = process.env, home, cwd = process.cwd() } = {}) {
  const parsed = parseArguments(argv);
  if (parsed.error) {
    const result = failure(null, ResultStatus.INVALID_INPUT, parsed.error);
    return { result, output: parsed.structured ? JSON.stringify(result) : renderHuman(result), exitCode: 2 };
  }

  const runtime = createDefaultRuntime();
  const result = await runtime.actions.execute(parsed.actionId, parsed.input ?? {}, {
    rootOptions: {
      explicitRoot: parsed.explicitRoot,
      env,
      ...(home === undefined ? {} : { home }),
      cwd,
    },
    connectors: runtime.connectors,
  });
  return {
    result,
    output: parsed.structured ? JSON.stringify(result) : renderHuman(result),
    exitCode: exitCodeFor(result),
  };
}
