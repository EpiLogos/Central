# Central × macOS Shortcuts

Shortcuts participates in Central in both directions without owning Central semantics.

## Central → Shortcut

`ctrl-macos` extends the canonical Action registry with `automation.run`, whose implementation is:

```text
automation.run Action
  ↓ Automation Port
ShortcutsAutomationConnector
  ↓ /usr/bin/shortcuts run <name>
macOS Shortcut
```

Examples:

```text
ctrl-macos --json automation run "Central Fixture"
```

or, through the generic Surface protocol:

```text
ctrl-macos --json action run automation.run '{"automation":"Central Fixture"}'
```

The Shortcut name is provider input. `/usr/bin/shortcuts` and its failure modes remain inside `central-shortcuts-connector`; core Actions do not branch on Shortcuts.

## Shortcut → Central

A Shortcut that needs to invoke Central should use macOS Shortcuts' **Run Shell Script** action to call `ctrl-macos`. The stable generic form is:

```text
ctrl-macos --json action run <canonical-action-id> '<json-object>'
```

A harmless acceptance example is:

```text
ctrl-macos --json action run central.root '{}'
```

A Work operation can invoke the same canonical Action used by the CLI and Raycast:

```text
ctrl-macos --json action run work.open '{"query":"project-name"}'
```

The Shortcut is an automation wrapper. `work.open` still owns Work selection semantics and executes `WorkDiscovery → NativeOpen`.

## Primary-workstation acceptance

The final #15 physical acceptance should prove both directions on the primary macOS workstation:

1. Create a harmless Shortcut named `Central Fixture` whose visible result is easy to recognise.
2. Run `ctrl-macos --json automation run "Central Fixture"` and verify the Shortcut actually executes.
3. Create a second Shortcut whose Run Shell Script step executes `ctrl-macos --json action run central.root '{}'` (or another harmless canonical Action) and verify the returned structured `ActionResult`.
4. Open the Raycast `Central Actions` command, search an Action, select a Work input from canonical `work.list`, invoke it, and exercise at least one configured Action hotkey.
5. Remove/disable the Raycast extension and verify ordinary `ctrl` CLI behavior remains unchanged.

Hosted CI proves the Port, provider command construction, registry projection, and real `/usr/bin/shortcuts` availability on macOS. It does not substitute for this named physical-workstation acceptance.
