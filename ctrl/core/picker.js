import { cancelled, failure, ResultStatus } from "./results.js";

const CANCEL_TOKENS = new Set(["q", "quit", "cancel", "/cancel"]);

function normalized(value) {
  return String(value ?? "").trim().toLocaleLowerCase();
}

function isCancellation(value) {
  return CANCEL_TOKENS.has(normalized(value));
}

function scoreDescriptor(descriptor, query) {
  if (query === "") return 1;
  const id = normalized(descriptor.id);
  const title = normalized(descriptor.title);
  const description = normalized(descriptor.description);
  if (id === query || title === query) return 100;
  if (id.startsWith(query) || title.startsWith(query)) return 80;
  if (id.includes(query) || title.includes(query)) return 60;
  if (description.includes(query)) return 20;
  return 0;
}

export function searchActionDescriptors(descriptors, query = "") {
  const needle = normalized(query);
  return descriptors
    .map((descriptor) => ({ descriptor, score: scoreDescriptor(descriptor, needle) }))
    .filter(({ score }) => score > 0)
    .sort((a, b) => b.score - a.score || a.descriptor.id.localeCompare(b.descriptor.id))
    .map(({ descriptor }) => descriptor);
}

function selectedOption(options, rawValue) {
  const numeric = Number.parseInt(String(rawValue).trim(), 10);
  if (!Number.isInteger(numeric) || numeric < 1 || numeric > options.length) return undefined;
  return options[numeric - 1];
}

function renderOptions(write, options, label) {
  write(label);
  options.forEach((option, index) => write(`${index + 1}. ${option.label}`));
}

async function resolveSelectableValues(input, registry, context) {
  if (Array.isArray(input.choices)) {
    return { ok: true, values: input.choices.map((value) => ({ label: String(value), value })) };
  }

  const source = input.selectionAction ?? input.selectableSource;
  if (!source) return { ok: true, values: null };
  if (typeof source.action !== "string" || source.action === "") {
    return {
      ok: false,
      result: failure(null, ResultStatus.INTERNAL_FAILURE, `Selectable input ${input.name} does not name the canonical Action used to resolve its values.`),
    };
  }

  const result = await registry.execute(source.action, source.input ?? {}, context);
  if (!result.ok) return { ok: false, result };
  const collection = result.data?.[source.collection];
  if (!Array.isArray(collection)) {
    return {
      ok: false,
      result: failure(source.action, ResultStatus.INTERNAL_FAILURE, `Selectable source ${source.action} did not return ${source.collection}.`),
    };
  }
  const valueField = source.valueField;
  return {
    ok: true,
    values: collection.map((item) => {
      const value = valueField ? item?.[valueField] : item;
      return { label: String(value), value };
    }),
  };
}

export async function runGuidedActionPicker({ registry, context = {}, prompt, write = () => {} }) {
  if (!registry || typeof registry.list !== "function" || typeof registry.execute !== "function") {
    throw new TypeError("Guided picker requires an Action registry.");
  }
  if (typeof prompt !== "function") throw new TypeError("Guided picker requires prompt().");
  if (typeof write !== "function") throw new TypeError("Guided picker write must be a function.");

  const query = await prompt("Search Actions (blank for all, q to cancel): ");
  if (isCancellation(query)) return cancelled(null, "Action selection cancelled.");

  const matches = searchActionDescriptors(registry.list(), query);
  if (matches.length === 0) {
    return failure(null, ResultStatus.INVALID_INPUT, `No Actions match: ${String(query).trim()}`);
  }

  const actionOptions = matches.map((descriptor) => ({ label: `${descriptor.id} — ${descriptor.title}`, value: descriptor }));
  renderOptions(write, actionOptions, "Actions:");
  const rawActionChoice = await prompt("Select Action number (q to cancel): ");
  if (isCancellation(rawActionChoice)) return cancelled(null, "Action selection cancelled.");
  const selected = selectedOption(actionOptions, rawActionChoice);
  if (!selected) return failure(null, ResultStatus.INVALID_INPUT, "Action selection must be one of the displayed numbers.");

  const action = selected.value;
  const inputValues = {};
  for (const input of action.inputs ?? []) {
    const resolved = await resolveSelectableValues(input, registry, context);
    if (!resolved.ok) return resolved.result;

    if (resolved.values) {
      if (resolved.values.length === 0 && input.required) {
        return failure(action.id, ResultStatus.INVALID_INPUT, `No selectable values are available for ${input.name}.`);
      }
      if (resolved.values.length > 0) {
        renderOptions(write, resolved.values, `${input.name}:`);
        const rawChoice = await prompt(`Select ${input.name} number (q to cancel): `);
        if (isCancellation(rawChoice)) return cancelled(action.id, `${action.title} cancelled.`);
        const chosen = selectedOption(resolved.values, rawChoice);
        if (!chosen) return failure(action.id, ResultStatus.INVALID_INPUT, `${input.name} selection must be one of the displayed numbers.`);
        inputValues[input.name] = chosen.value;
        continue;
      }
    }

    const rawValue = await prompt(`${input.name}${input.required ? "" : " (optional)"} (q to cancel): `);
    if (isCancellation(rawValue)) return cancelled(action.id, `${action.title} cancelled.`);
    if (input.required && String(rawValue).trim() === "") {
      return failure(action.id, ResultStatus.INVALID_INPUT, `${input.name} is required.`);
    }
    if (String(rawValue) !== "" || input.required) inputValues[input.name] = rawValue;
  }

  return registry.execute(action.id, inputValues, context);
}
