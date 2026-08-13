import { createDefaultRuntime } from "./runtime.js";
import { failure, ResultStatus } from "./results.js";

const COMMANDS = new Map([
  ["root", "central.root"], ["init", "central.init"], ["doctor", "central.doctor"],
  ["actions", "action.list"], ["action.list", "action.list"],
  ["central.root", "central.root"], ["central.init", "central.init"], ["central.doctor", "central.doctor"],
  ["work.list", "work.list"],
]);

function parseArguments(argv) {
  const positional = [];
  let structured = false;
  let explicitRoot;

  for (let index = 0; index < argv.length; index += 1) {
    const argument = argv[index];
    if (argument === "--json") { structured = true; continue; }
    if (argument === "--root") {
      const value = argv[index + 1];
      if (value === undefined || value.startsWith("--")) return { structured, error: "--root requires a path." };
      explicitRoot = value;
      index += 1;
      continue;
    }
    if (argument.startsWith("--root=")) {
      explicitRoot = argument.slice("--root=".length);
      if (explicitRoot === "") return { structured, error: "--root requires a path." };
      continue;
    }
    if (argument.startsWith("--")) return { structured, error: `Unknown option: ${argument}` };
    positional.push(argument);
  }

  let commandKey;
  if (positional[0] === "action" && positional[1] === "list" && positional.length === 2) commandKey = "action.list";
  else if (positional[0] === "work" && positional[1] === "list" && positional.length === 2) commandKey = "work.list";
  else if (positional.length === 1) commandKey = positional[0];
  else if (positional.length === 0) return { structured, error: "An Action or command is required." };
  else return { structured, error: `Unexpected arguments: ${positional.slice(1).join(" ")}` };

  const actionId = COMMANDS.get(commandKey);
  if (!actionId) return { structured, error: `Unknown command: ${commandKey}` };
  return { structured, explicitRoot, actionId };
}

function renderDoctorDetails(details) {
  const lines = [`Central root: ${details.root}`, `Valid: ${details.valid ? "yes" : "no"}`];
  for (const check of details.checks ?? []) lines.push(`${check.valid ? "ok" : "missing"}  ${check.path}`);
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
    case "central.root": return `${result.data.path} (${result.data.source})`;
    case "central.init": return `Initialized Central at ${result.data.root}`;
    case "central.doctor": return renderDoctorDetails(result.data);
    case "action.list": return result.data.actions.map((action) => `${action.id}\t${action.title}`).join("\n");
    case "work.list": {
      const connector = result.data.diagnostics.selectedConnector?.id ?? "none";
      return [`Connector: ${connector}`, ...result.data.items.map((item) => `${item.name}\t${item.path}`)].join("\n");
    }
    default: return JSON.stringify(result.data, null, 2);
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
  const result = await runtime.actions.execute(parsed.actionId, {}, {
    rootOptions: { explicitRoot: parsed.explicitRoot, env, ...(home === undefined ? {} : { home }), cwd },
    connectors: runtime.connectors,
  });
  return { result, output: parsed.structured ? JSON.stringify(result) : renderHuman(result), exitCode: exitCodeFor(result) };
}
