# Ubuntu server extension set

This crate is the Linux/Ubuntu proving implementation for Central machine behavior. It is an external-grade consumer of the public Connector SDK: core Actions do not import Ubuntu, apt, dpkg, systemd, or file-materialisation policy.

## Public Ports

`UbuntuServerConnector` implements:

- `MachineInspector` — Linux/Ubuntu identity plus explicit observations for the package, configuration, and service IDs requested by the caller;
- `PackageManager` — Debian package state through `dpkg-query` and reconciliation through `apt-get`;
- `ConfigurationManager` — portable file materialisation using an absolute target path as the configuration `id` and `source.kind = "file"` for the authored source.

It deliberately does **not** define new machine Action IDs. `machine.inspect`, `machine.plan`, `machine.apply`, and `machine.verify` remain provider-neutral core Actions.

## Authored intent versus observation

Machine declarations remain authored source below `Control/machines/`. Planning derives a scoped `MachineInspectionInput` containing only the resource identifiers whose state must be compared. The Ubuntu Connector observes those resources on the current host; it does not read or own the authored declaration.

That distinction lets a Connector report both presence and absence without turning an installed-package inventory or filesystem crawl into canonical intent.

## Package behavior

Package IDs use Debian package-name characters. Inspection calls `dpkg-query` directly. A mutating apply uses `apt-get` with a non-interactive frontend. If the process is not root, the Connector tries `sudo -n apt-get`; lack of non-interactive privilege is reported as a provider failure rather than hidden.

Every successful mutation is followed by real package-state verification.

## Configuration behavior

For the first Linux-suitable implementation:

```json
{
  "id": "/absolute/target/path",
  "present": true,
  "source": {
    "kind": "file",
    "reference": "/absolute/authored/source/path"
  }
}
```

Preview compares source and target bytes without mutation. Apply creates parent directories when required, copies or removes the target, and verifies the resulting state.

This is one replaceable `ConfigurationManager` implementation, not a core configuration schema.

## Headless host

`central-ubuntu-host` provides the executable composition for a real server:

```sh
cargo run -p central-ubuntu-host --bin ctrl-ubuntu -- --json root
cargo run -p central-ubuntu-host --bin ctrl-ubuntu -- --json machine inspect
cargo run -p central-ubuntu-host --bin ctrl-ubuntu -- --json machine plan home-server
cargo run -p central-ubuntu-host --bin ctrl-ubuntu -- --json machine apply home-server
cargo run -p central-ubuntu-host --bin ctrl-ubuntu -- --json machine verify home-server
```

Use `CENTRAL_ROOT=/path/to/Central` or `--root /path/to/Central` as with the ordinary CLI. No graphical launcher is required.

## Acceptance

Hosted Ubuntu CI runs the same public conformance helpers used by other Connectors and an end-to-end headless fixture that exercises:

```text
Central root
  → authored Control/machines source
  → machine.plan
  → public MachineInspector / ConfigurationManager resolution
  → machine.apply
  → fresh observation
  → machine.verify
  → repeat-stable apply
```

The package conformance fixture uses the real installed `bash` package, so it exercises the real dpkg-backed provider while remaining non-mutating. The configuration fixture performs a harmless real filesystem mutation in the runner's temporary directory.

Issue #17 separately requires acceptance evidence from the actual `home-server`. Hosted CI proves the Ubuntu implementation and catches Linux/platform assumptions; it does not pretend to be that named physical deployment. The issue and PR should remain open until that final real-machine evidence is recorded.
