# Chosen root recognition

`central.recognize {"path":"/absolute/chosen/directory"}` is a read-only native
operation intended for a chooser/recognition boundary. It does not resolve or
change the currently configured root. It never initializes, adopts, moves or
binds source. No ProjectRef is created, and no source body or git configuration is
read. The caller must supply an explicit absolute UTF-8 path of at most16KiB.

The `central.root-recognition/v1` reading contains:

- `requested_path`, `canonical_path` when resolved, and `redirected` for a chosen
  alias whose canonical path differs;
- `outcome`: `recognized`, `unrecognized`, `inaccessible`, `missing`,
  `not_directory`, `changed`, or `error`;
- filesystem `identity: {device, inode}` for the canonical directory, with exact decimal strings to prevent JavaScript integer precision loss;
- `access: {readable, writable, searchable, read_only, basis}` from read-only
  POSIX access observation for the current process;
- six fixed structural `checks`, with `path`, `status` and optional error detail;
- explicit `limitations`, `mutated: false`, `bound: false`.

Recognition uses the same six required structural directories as Central's
native root initialization contract. Each is inspected through the selected
root's file descriptor with no-follow relative directory opens. Symlinked
required members are refused. Missing structure means unrecognized; inaccessible
structure is reported separately. Canonical root identity is checked again after
the structural inspection, so replacement during the read does not become a
successful recognition.

An existing root can be recognized with `access.read_only: true`. Access probes
are observations, not write tests, source authority grants or guarantees that a
future mutation will succeed. No file is created to test writability. The
recognition does not validate every project, body, contribution or owner store.

A chooser can display the canonical target of a selected alias. Actual desktop
binding belongs to its native operation and must revalidate the canonical path
and observed directory identity. This Action intentionally performs no binding.

`central.doctor` remains a distinct diagnostic of the configured root; its mixed
product-checkout diagnosis can inspect git configuration. Recognition avoids that
unnecessary reading and returns expected incomplete/inaccessible states as data.

Native acceptance uses the actual candidate CLI and snapshots real directories:

```sh
CTRL_BIN="$PWD/target/debug/ctrl" python3 ctrl/tests/native/root_recognition.py
```

It covers existing, unrecognized, read-only, inaccessible, missing and non-directory
choices; aliases and redirected structural members; root identity; zero source
mutation/adoption/binding; and a git-config FIFO proving recognition does not
read unrelated configuration.
