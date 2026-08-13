#!/usr/bin/env node

import { runCli } from "../core/cli.js";

const { output, exitCode } = await runCli(process.argv.slice(2));
if (output !== "") process.stdout.write(`${output}\n`);
process.exitCode = exitCode;
