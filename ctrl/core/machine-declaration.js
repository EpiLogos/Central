import { readFile } from "node:fs/promises";
import { join } from "node:path";

export const MACHINE_DECLARATION_API_VERSION = "central.machine/v1";
const ROLE_PATTERN = /^[a-z0-9][a-z0-9-]*$/;
const PACKAGE_STATES = new Set(["present", "absent"]);
const CONFIGURATION_STATES = new Set(["present", "absent"]);
const SERVICE_STATES = new Set(["running", "stopped", "enabled", "disabled"]);
const SOURCE_KINDS = new Set(["path", "control", "url"]);

function isRecord(value) {
  return Boolean(value) && typeof value === "object" && !Array.isArray(value);
}

function diagnostic(code, path, message) {
  return { code, path, message };
}

function validateRequirementList(list, path, allowedStates, errors, { allowSource = false } = {}) {
  if (!Array.isArray(list)) {
    errors.push(diagnostic("invalid_type", path, `${path} must be an array.`));
    return;
  }
  const ids = new Set();
  list.forEach((item, index) => {
    const itemPath = `${path}[${index}]`;
    if (!isRecord(item)) {
      errors.push(diagnostic("invalid_type", itemPath, `${itemPath} must be an object.`));
      return;
    }
    if (typeof item.id !== "string" || item.id.trim() === "") {
      errors.push(diagnostic("invalid_id", `${itemPath}.id`, `${itemPath}.id must be a non-empty string.`));
    } else if (ids.has(item.id)) {
      errors.push(diagnostic("duplicate_id", `${itemPath}.id`, `${item.id} is declared more than once in ${path}.`));
    } else {
      ids.add(item.id);
    }
    if (!allowedStates.has(item.state)) {
      errors.push(diagnostic("invalid_state", `${itemPath}.state`, `${itemPath}.state must be one of: ${[...allowedStates].join(", ")}.`));
    }
    if (item.source !== undefined) {
      if (!allowSource) {
        errors.push(diagnostic("unexpected_source", `${itemPath}.source`, `${path} requirements do not accept source references.`));
      } else if (!isRecord(item.source) || !SOURCE_KINDS.has(item.source.kind) || typeof item.source.ref !== "string" || item.source.ref.trim() === "") {
        errors.push(diagnostic("invalid_source", `${itemPath}.source`, `${itemPath}.source must contain kind (${[...SOURCE_KINDS].join(", ")}) and non-empty ref.`));
      }
    }
  });
}

export function validateMachineDeclaration(value) {
  const errors = [];
  if (!isRecord(value)) {
    return { valid: false, errors: [diagnostic("invalid_type", "$", "Machine declaration must be an object.")] };
  }

  if (value.apiVersion !== MACHINE_DECLARATION_API_VERSION) {
    errors.push(diagnostic("unsupported_version", "apiVersion", `apiVersion must be ${MACHINE_DECLARATION_API_VERSION}.`));
  }
  if (typeof value.role !== "string" || !ROLE_PATTERN.test(value.role)) {
    errors.push(diagnostic("invalid_role", "role", "role must use lowercase letters, digits, and hyphens."));
  }
  if (!Array.isArray(value.capabilities)) {
    errors.push(diagnostic("invalid_type", "capabilities", "capabilities must be an array."));
  } else {
    const capabilities = new Set();
    value.capabilities.forEach((capability, index) => {
      if (typeof capability !== "string" || capability.trim() === "") {
        errors.push(diagnostic("invalid_capability", `capabilities[${index}]`, "Each capability must be a non-empty string."));
      } else if (capabilities.has(capability)) {
        errors.push(diagnostic("duplicate_capability", `capabilities[${index}]`, `${capability} is declared more than once.`));
      } else {
        capabilities.add(capability);
      }
    });
  }

  if (!isRecord(value.requirements)) {
    errors.push(diagnostic("invalid_type", "requirements", "requirements must be an object."));
  } else {
    validateRequirementList(value.requirements.packages, "requirements.packages", PACKAGE_STATES, errors);
    validateRequirementList(value.requirements.configurations, "requirements.configurations", CONFIGURATION_STATES, errors, { allowSource: true });
    validateRequirementList(value.requirements.services, "requirements.services", SERVICE_STATES, errors);
  }

  return { valid: errors.length === 0, errors };
}

export function isMachineRole(value) {
  return typeof value === "string" && ROLE_PATTERN.test(value);
}

export async function readMachineDeclaration(centralRoot, role) {
  if (!isMachineRole(role)) {
    return {
      ok: false,
      source: null,
      diagnostics: [diagnostic("invalid_role", "role", "role must use lowercase letters, digits, and hyphens.")],
    };
  }

  const path = join(centralRoot, "Control", "machines", `${role}.json`);
  let content;
  try {
    content = await readFile(path, "utf8");
  } catch (error) {
    if (error?.code === "ENOENT") {
      return { ok: false, source: { path, sourceClass: "authored" }, diagnostics: [diagnostic("not_found", "$", `No machine declaration exists for role ${role}.`)] };
    }
    throw error;
  }

  let declaration;
  try {
    declaration = JSON.parse(content);
  } catch (error) {
    return {
      ok: false,
      source: { path, sourceClass: "authored" },
      diagnostics: [diagnostic("invalid_json", "$", `Machine declaration is not valid JSON: ${error instanceof Error ? error.message : String(error)}`)],
    };
  }

  const validation = validateMachineDeclaration(declaration);
  if (declaration?.role !== role) {
    validation.errors.push(diagnostic("role_mismatch", "role", `Declaration role ${String(declaration?.role)} does not match requested role ${role}.`));
    validation.valid = false;
  }
  return {
    ok: validation.valid,
    source: { path, sourceClass: "authored" },
    ...(validation.valid ? { declaration } : {}),
    diagnostics: validation.errors,
  };
}

export function renderMachineDeclaration({ declaration, source }) {
  const lines = [
    `Role: ${declaration.role}`,
    `Schema: ${declaration.apiVersion}`,
    `Source: ${source.path}`,
    "Capabilities:",
    ...declaration.capabilities.map((capability) => `- ${capability}`),
    "Packages:",
    ...declaration.requirements.packages.map((item) => `- ${item.id}: ${item.state}`),
    "Configurations:",
    ...declaration.requirements.configurations.map((item) => `- ${item.id}: ${item.state}${item.source ? ` (${item.source.kind}: ${item.source.ref})` : ""}`),
    "Services:",
    ...declaration.requirements.services.map((item) => `- ${item.id}: ${item.state}`),
  ];
  return lines.join("\n");
}
