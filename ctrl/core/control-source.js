import { readdir, readFile, stat } from "node:fs/promises";
import { extname, join, relative, sep } from "node:path";

export const CONTROL_ROOTS = Object.freeze(["user", "agents", "machines"]);
export const SEARCHABLE_CONTROL_EXTENSIONS = Object.freeze([".md", ".txt", ".text"]);

export function controlRootPath(centralRoot, target) {
  if (!CONTROL_ROOTS.includes(target)) {
    throw new TypeError(`Unknown Control root: ${target}`);
  }
  return join(centralRoot, "Control", target);
}

async function directoryExists(path) {
  try { return (await stat(path)).isDirectory(); }
  catch (error) { if (error?.code === "ENOENT") return false; throw error; }
}

export async function locateControlRoot(centralRoot, target) {
  const path = controlRootPath(centralRoot, target);
  return { target, path, exists: await directoryExists(path), sourceClass: "authored" };
}

async function walkFiles(root) {
  const files = [];
  const stack = [root];
  while (stack.length > 0) {
    const directory = stack.pop();
    const entries = await readdir(directory, { withFileTypes: true });
    for (const entry of entries) {
      const path = join(directory, entry.name);
      if (entry.isDirectory()) stack.push(path);
      else if (entry.isFile()) files.push(path);
    }
  }
  return files.sort((a, b) => a.localeCompare(b));
}

export async function searchControl(centralRoot, query) {
  if (typeof query !== "string" || query.trim() === "") throw new TypeError("Control search query must be non-empty.");
  const needle = query.trim().toLocaleLowerCase();
  const matches = [];
  const unsupported = [];
  const missingRoots = [];

  for (const target of CONTROL_ROOTS) {
    const root = controlRootPath(centralRoot, target);
    if (!(await directoryExists(root))) {
      missingRoots.push({ target, path: root });
      continue;
    }
    for (const path of await walkFiles(root)) {
      const extension = extname(path).toLocaleLowerCase();
      const sourcePath = relative(join(centralRoot, "Control"), path).split(sep).join("/");
      if (!SEARCHABLE_CONTROL_EXTENSIONS.includes(extension)) {
        unsupported.push({ target, sourcePath, format: extension || "none" });
        continue;
      }
      const content = await readFile(path, "utf8");
      const lines = content.split(/\r?\n/);
      for (let index = 0; index < lines.length; index += 1) {
        if (lines[index].toLocaleLowerCase().includes(needle)) {
          matches.push({ target, sourcePath, line: index + 1, text: lines[index] });
        }
      }
    }
  }

  return { query: query.trim(), matches, unsupported, missingRoots, searchedRoots: [...CONTROL_ROOTS] };
}
