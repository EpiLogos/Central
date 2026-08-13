# `ctrl` CLI reference

**Status:** development reference

Central currently uses a dependency-free Node 22 ESM entrypoint.

From the repository root:

```text
npm run ctrl -- <command>
```

The package also declares `ctrl/bin/ctrl.js` as the `ctrl` executable entrypoint.

## Root selection

By default, `ctrl` resolves the Central root as `$HOME/Central`.

Use either of these explicit alternatives:

```text
CENTRAL_ROOT=/path/to/Central npm run ctrl -- root
npm run ctrl -- --root /path/to/Central root
```

`--root` takes precedence over `CENTRAL_ROOT`.

## Foundation commands

```text
ctrl root
ctrl init
ctrl doctor
ctrl actions
ctrl action list
```

The canonical Action IDs are:

```text
central.root
central.init
central.doctor
action.list
```

The canonical IDs can also be invoked directly.

`central.init` creates only these protocol roots and is safe to repeat:

```text
Control/user/
Control/agents/
Control/machines/
Work/
```

It does not impose a schema below the Control roots.

## Structured output

Add `--json` to receive one JSON Action result instead of the human rendering:

```text
ctrl --json doctor
ctrl --json action.list
```

Foundation result statuses are:

```text
success
invalid_input
invalid_central_structure
internal_failure
```

Exit status is `0` for success, `2` for invalid input, `3` for an invalid Central structure, and `1` for an internal failure.

## Tests

Run the foundation suite from the repository root:

```text
npm test
```
