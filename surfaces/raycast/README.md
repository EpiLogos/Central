# Central Raycast Surface

This Surface is a thin Raycast projection of the canonical Central Action registry exposed by `ctrl-macos`.

It does not implement Work discovery, Work opening, machine operations, or automation semantics itself.

```text
Raycast
  ↓ action list / action run
ctrl-macos
  ↓ canonical ActionRegistry
Action → Port → Connector → target
```

## Install for development

From this directory on the primary macOS workstation:

```text
npm install
npm run dev
```

Set **Central macOS command** to the installed `ctrl-macos` executable or an absolute path to it. Optionally set **Central root** when the normal Central root resolution should be overridden.

The `Central Actions` command loads `action list` from `ctrl-macos`, so Action ids, titles, descriptions, mutation classes, required Ports, and input-selection metadata stay authoritative in Central.

For an Action whose input descriptor contains a `selection`, the Surface invokes the descriptor's selection Action and renders the returned collection. `work.open` and `work.reveal`, for example, therefore receive Work choices from canonical `work.list`; the Raycast extension contains no second filesystem search.

Mutating Actions require a Raycast confirmation before invocation. The actual invocation uses:

```text
ctrl-macos --json action run <action-id> '<json-object>'
```

and preserves the structured `ActionResult` returned by Central.

## Hotkeys

Raycast itself can assign a global hotkey to the `Central Actions` command. Individual Action items can also receive ActionPanel shortcuts from the optional **Action hotkeys** preference, supplied as JSON:

```json
{
  "work.open": "cmd+shift+o",
  "work.reveal": "cmd+shift+r",
  "machine.inspect": "cmd+shift+i"
}
```

These shortcuts select/invoke the same descriptor-driven Action item; they do not create separate implementations.

## Removal boundary

Deleting this directory or not installing the Raycast extension does not remove or alter any canonical Action, Port, Connector, CLI command, or guided terminal behavior. Raycast depends on the stable macOS host protocol; core does not depend on Raycast.
