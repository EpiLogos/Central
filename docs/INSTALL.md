# Central source installation

Central publishes one base installation contract for the `ctrl` command: install the Rust binary from a Central source checkout with Cargo.

This contract is deliberately small. It installs the native control surface only. It does not create a second package manager, copy machine configuration, or populate authored Control material.

## Requirements

- Git, when obtaining the checkout from GitHub;
- a current stable Rust toolchain with Cargo.

## Install from a checkout

From the Central repository root:

```sh
cargo install --path ctrl
```

A composition layer or acceptance harness can install into an isolated prefix instead:

```sh
cargo install --path ctrl --root /path/to/prefix
/path/to/prefix/bin/ctrl --version
```

The installed binary reports its package version with any of:

```sh
ctrl --version
ctrl -V
ctrl version
```

## Establish a Central root

A Central root is a **personal world**, not a product checkout. `ctrl init`
creates the personal root shape; the Central source repository itself belongs in
a developer checkout (for example `~/Central/Work/Central` on a machine whose
personal root is `~/Central`), never fused with the personal root. `ctrl doctor`
diagnoses a personal root that also resembles the product checkout.

The base command owns the native filesystem protocol. Initialize a root with:

```sh
ctrl --root /path/to/Central init
```

A fresh root contains exactly the Central-owned runtime roots:

```text
Control/user/
Control/agents/
Control/machines/
.central/
Work/
```

The three Control roots start empty. `ctrl init` does not invent a personal profile, agent preferences, machine facts, or other authored Control content. Durable Control material is human-authored or explicitly adopted through the Control protocol and its Skills.

Initialization is idempotent. Repeating the command preserves an already valid root.

## Verify the installation

The base structured checks are:

```sh
ctrl --root /path/to/Central doctor --json
ctrl --root /path/to/Central action list --json
```

Both commands return the normal structured `ActionResult` envelope. `doctor` validates the filesystem protocol. `action list` discloses the canonical Actions available from the installed base surface.

A clean interoperability proof is therefore:

```sh
cargo install --path ctrl --root /tmp/central-prefix
CTRL=/tmp/central-prefix/bin/ctrl
ROOT=/tmp/Central

"$CTRL" --version
"$CTRL" --root "$ROOT" init --json
"$CTRL" --root "$ROOT" doctor --json
"$CTRL" --root "$ROOT" action list --json
"$CTRL" --root "$ROOT" init --json
```

Optional platform hosts and Connectors remain separate extension surfaces. Installing the base `ctrl` binary does not silently install Homebrew, chezmoi, Ubuntu providers, macOS providers, or other personal integrations.
