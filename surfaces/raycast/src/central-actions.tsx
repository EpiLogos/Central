import {
  Action,
  ActionPanel,
  Alert,
  Detail,
  Form,
  Icon,
  Keyboard,
  List,
  Toast,
  confirmAlert,
  getPreferenceValues,
  showToast,
  useNavigation,
} from "@raycast/api";
import { execFile } from "node:child_process";
import { promisify } from "node:util";
import { useEffect, useMemo, useState } from "react";

const execFileAsync = promisify(execFile);

type Preferences = {
  ctrlPath: string;
  centralRoot?: string;
  actionHotkeys?: string;
};

type ActionSelection = {
  action: string;
  collection: string;
  value_field: string;
};

type ActionInput = {
  name: string;
  type: string;
  required: boolean;
  choices?: string[];
  selection?: ActionSelection;
};

type CentralAction = {
  id: string;
  title: string;
  description: string;
  inputs: ActionInput[];
  output: { type: string };
  mutation_class: "read-only" | "locally-mutating" | "externally-mutating";
  preview_supported: boolean;
  required_ports: string[];
  availability: { available: boolean; reason?: string };
};

type ActionResult = {
  ok: boolean;
  status: string;
  action?: string;
  data?: unknown;
  error?: { code: string; message: string; details?: unknown };
};

const preferences = getPreferenceValues<Preferences>();

function baseArgs(): string[] {
  const args = ["--json"];
  if (preferences.centralRoot?.trim()) {
    args.push("--root", preferences.centralRoot.trim());
  }
  return args;
}

async function runCtrl(args: string[]): Promise<ActionResult> {
  const { stdout } = await execFileAsync(preferences.ctrlPath, [...baseArgs(), ...args], {
    maxBuffer: 8 * 1024 * 1024,
  });
  const payload = JSON.parse(stdout.trim()) as ActionResult;
  return payload;
}

async function listActions(): Promise<CentralAction[]> {
  const payload = await runCtrl(["action", "list"]);
  if (!payload.ok) {
    throw new Error(payload.error?.message ?? "Central Action discovery failed.");
  }
  const data = payload.data as { actions?: CentralAction[] } | undefined;
  return data?.actions ?? [];
}

async function invokeAction(action: string, input: Record<string, unknown>): Promise<ActionResult> {
  return runCtrl(["action", "run", action, JSON.stringify(input)]);
}

function mutationIcon(action: CentralAction): Icon {
  if (action.mutation_class === "read-only") return Icon.Eye;
  if (action.mutation_class === "locally-mutating") return Icon.Pencil;
  return Icon.ArrowRight;
}

function parseHotkeys(): Record<string, Keyboard.Shortcut> {
  if (!preferences.actionHotkeys?.trim()) return {};
  try {
    const raw = JSON.parse(preferences.actionHotkeys) as Record<string, string>;
    const result: Record<string, Keyboard.Shortcut> = {};
    const modifiers = new Set(["cmd", "shift", "opt", "ctrl"]);
    for (const [action, expression] of Object.entries(raw)) {
      const parts = expression
        .toLowerCase()
        .split("+")
        .map((part) => part.trim())
        .filter(Boolean);
      const key = parts.find((part) => !modifiers.has(part));
      if (!key) continue;
      result[action] = {
        modifiers: parts.filter((part) => modifiers.has(part)) as Keyboard.KeyModifier[],
        key: key as Keyboard.KeyEquivalent,
      };
    }
    return result;
  } catch {
    return {};
  }
}

async function confirmMutation(action: CentralAction): Promise<boolean> {
  if (action.mutation_class === "read-only") return true;
  return confirmAlert({
    title: `Run ${action.title}?`,
    message: `${action.id} is ${action.mutation_class}.`,
    primaryAction: {
      title: "Run Action",
      style: Alert.ActionStyle.Default,
    },
  });
}

function ResultView({ action, result }: { action: CentralAction; result: ActionResult }) {
  const body = result.ok
    ? JSON.stringify(result.data ?? {}, null, 2)
    : JSON.stringify(result.error ?? { status: result.status }, null, 2);
  return (
    <Detail
      navigationTitle={action.title}
      markdown={`# ${result.ok ? "Completed" : "Failed"}\n\n**${action.id}** · ${result.status}\n\n\`\`\`json\n${body}\n\`\`\``}
    />
  );
}

function RunAction({ action, input }: { action: CentralAction; input: Record<string, unknown> }) {
  const { push } = useNavigation();
  const [running, setRunning] = useState(false);

  async function run() {
    if (!(await confirmMutation(action))) return;
    setRunning(true);
    try {
      const result = await invokeAction(action.id, input);
      if (!result.ok) {
        await showToast({
          style: Toast.Style.Failure,
          title: action.title,
          message: result.error?.message ?? result.status,
        });
      } else {
        await showToast({ style: Toast.Style.Success, title: action.title });
      }
      push(<ResultView action={action} result={result} />);
    } catch (error) {
      await showToast({
        style: Toast.Style.Failure,
        title: `Could not run ${action.title}`,
        message: error instanceof Error ? error.message : String(error),
      });
    } finally {
      setRunning(false);
    }
  }

  return <Action title={running ? "Running…" : "Run Action"} icon={Icon.Play} onAction={run} />;
}

function SelectionInputView({ action, input }: { action: CentralAction; input: ActionInput }) {
  const selection = input.selection!;
  const [values, setValues] = useState<Record<string, unknown>[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string>();

  useEffect(() => {
    void (async () => {
      try {
        const result = await invokeAction(selection.action, {});
        if (!result.ok) throw new Error(result.error?.message ?? result.status);
        const data = result.data as Record<string, unknown> | undefined;
        const collection = data?.[selection.collection];
        if (!Array.isArray(collection)) {
          throw new Error(`${selection.action} did not return ${selection.collection}.`);
        }
        setValues(collection as Record<string, unknown>[]);
      } catch (cause) {
        setError(cause instanceof Error ? cause.message : String(cause));
      } finally {
        setLoading(false);
      }
    })();
  }, [selection.action, selection.collection]);

  return (
    <List isLoading={loading} searchBarPlaceholder={`Select ${input.name}`}>
      {error ? <List.EmptyView title="Selection unavailable" description={error} /> : null}
      {values.map((value, index) => {
        const selected = String(value[selection.value_field] ?? "");
        return (
          <List.Item
            key={`${selected}-${index}`}
            title={selected}
            subtitle={typeof value.path === "string" ? value.path : undefined}
            actions={
              <ActionPanel>
                <RunAction action={action} input={{ [input.name]: selected }} />
              </ActionPanel>
            }
          />
        );
      })}
    </List>
  );
}

function FormInputView({ action }: { action: CentralAction }) {
  const { push } = useNavigation();
  const [running, setRunning] = useState(false);

  async function submit(values: Record<string, string>) {
    if (!(await confirmMutation(action))) return;
    setRunning(true);
    try {
      const result = await invokeAction(action.id, values);
      if (!result.ok) {
        await showToast({
          style: Toast.Style.Failure,
          title: action.title,
          message: result.error?.message ?? result.status,
        });
      } else {
        await showToast({ style: Toast.Style.Success, title: action.title });
      }
      push(<ResultView action={action} result={result} />);
    } catch (error) {
      await showToast({
        style: Toast.Style.Failure,
        title: `Could not run ${action.title}`,
        message: error instanceof Error ? error.message : String(error),
      });
    } finally {
      setRunning(false);
    }
  }

  return (
    <Form
      isLoading={running}
      actions={
        <ActionPanel>
          <Action.SubmitForm title="Run Action" onSubmit={submit} />
        </ActionPanel>
      }
    >
      {action.inputs.map((input) =>
        input.choices?.length ? (
          <Form.Dropdown key={input.name} id={input.name} title={input.name}>
            {input.choices.map((choice) => (
              <Form.Dropdown.Item key={choice} value={choice} title={choice} />
            ))}
          </Form.Dropdown>
        ) : (
          <Form.TextField key={input.name} id={input.name} title={input.name} />
        ),
      )}
    </Form>
  );
}

function ActionInputView({ action }: { action: CentralAction }) {
  if (action.inputs.length === 1 && action.inputs[0].selection) {
    return <SelectionInputView action={action} input={action.inputs[0]} />;
  }
  return <FormInputView action={action} />;
}

export default function Command() {
  const [actions, setActions] = useState<CentralAction[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string>();
  const hotkeys = useMemo(parseHotkeys, []);

  useEffect(() => {
    void (async () => {
      try {
        setActions(await listActions());
      } catch (cause) {
        const message = cause instanceof Error ? cause.message : String(cause);
        setError(message);
        await showToast({ style: Toast.Style.Failure, title: "Central Actions unavailable", message });
      } finally {
        setLoading(false);
      }
    })();
  }, []);

  return (
    <List isLoading={loading} searchBarPlaceholder="Search Central Actions" filtering>
      {error ? <List.EmptyView title="Central Actions unavailable" description={error} /> : null}
      {actions.map((action) => {
        const destination = action.inputs.length ? (
          <Action.Push title="Configure Action" target={<ActionInputView action={action} />} />
        ) : (
          <RunAction action={action} input={{}} />
        );
        return (
          <List.Item
            key={action.id}
            icon={mutationIcon(action)}
            title={action.title}
            subtitle={action.id}
            accessories={[{ text: action.mutation_class }]}
            actions={
              <ActionPanel>
                {hotkeys[action.id] ? (
                  <Action
                    title="Run with Hotkey"
                    shortcut={hotkeys[action.id]}
                    onAction={async () => {
                      if (action.inputs.length) return;
                      if (!(await confirmMutation(action))) return;
                      const result = await invokeAction(action.id, {});
                      await showToast({
                        style: result.ok ? Toast.Style.Success : Toast.Style.Failure,
                        title: action.title,
                        message: result.ok ? undefined : result.error?.message ?? result.status,
                      });
                    }}
                  />
                ) : null}
                {destination}
              </ActionPanel>
            }
          />
        );
      })}
    </List>
  );
}
