import { access, mkdir, stat } from "node:fs/promises";
import { constants } from "node:fs";
import { homedir } from "node:os";
import { isAbsolute, join, resolve } from "node:path";

export const REQUIRED_CENTRAL_DIRECTORIES = Object.freeze([
  "Control/user",
  "Control/agents",
  "Control/machines",
  "Work",
]);

function normalizeRoot(value, baseDirectory = process.cwd()) {
  if (typeof value !== "string" || value.trim() === "") {
    throw new TypeError("Central root must be a non-empty path.");
  }
  const trimmed = value.trim();
  return isAbsolute(trimmed) ? resolve(trimmed) : resolve(baseDirectory, trimmed);
}

export function resolveCentralRoot({
  explicitRoot,
  env = process.env,
  home = homedir(),
  cwd = process.cwd(),
} = {}) {
  if (explicitRoot !== undefined) {
    return { path: normalizeRoot(explicitRoot, cwd), source: "explicit" };
  }
  if (env.CENTRAL_ROOT !== undefined && env.CENTRAL_ROOT.trim() !== "") {
    return { path: normalizeRoot(env.CENTRAL_ROOT, cwd), source: "environment" };
  }
  return { path: join(home, "Central"), source: "default" };
}

export async function initializeCentral(root) {
  const rootPath = normalizeRoot(root);
  await mkdir(rootPath, { recursive: true });
  for (const relativePath of REQUIRED_CENTRAL_DIRECTORIES) {
    await mkdir(join(rootPath, relativePath), { recursive: true });
  }
  return {
    root: rootPath,
    directories: [...REQUIRED_CENTRAL_DIRECTORIES],
  };
}

async function directoryState(path) {
  try {
    await access(path, constants.F_OK);
    const info = await stat(path);
    return info.isDirectory() ? "directory" : "not_directory";
  } catch (error) {
    if (error && (error.code === "ENOENT" || error.code === "ENOTDIR")) return "missing";
    throw error;
  }
}

export async function inspectCentral(root) {
  const rootPath = normalizeRoot(root);
  const rootState = await directoryState(rootPath);
  const checks = [];

  for (const relativePath of REQUIRED_CENTRAL_DIRECTORIES) {
    const state = await directoryState(join(rootPath, relativePath));
    checks.push({
      path: relativePath,
      state,
      valid: state === "directory",
    });
  }

  return {
    root: rootPath,
    rootState,
    valid: rootState === "directory" && checks.every((check) => check.valid),
    checks,
  };
}
