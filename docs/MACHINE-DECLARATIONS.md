# Portable machine declarations

Central keeps intended machine state in authored Control source, separate from observations of the current host.

A declaration lives at:

```text
Control/machines/<role>.json
```

The first structured form is versioned as `central.machine/v1`.

```json
{
  "apiVersion": "central.machine/v1",
  "role": "primary-workstation",
  "capabilities": ["NativeOpen", "Automation", "PackageManager", "ConfigurationManager"],
  "requirements": {
    "packages": [
      { "id": "git", "state": "present" }
    ],
    "configurations": [
      {
        "id": "shell",
        "state": "present",
        "source": { "kind": "control", "ref": "machines/config/shell" }
      }
    ],
    "services": [
      { "id": "ssh-agent", "state": "running" }
    ]
  }
}
```

The role and capabilities describe intended use. Package, configuration, and service requirements describe desired state without naming a universal provider. Later machine planning resolves the abstract Ports that can satisfy differences on the current host.

Configuration requirements may refer to a source by `path`, `control`, or `url`. A source reference identifies authored configuration material; it does not make the current materialized configuration canonical.

Supported requirement states in v1 are:

- packages: `present`, `absent`
- configurations: `present`, `absent`
- services: `running`, `stopped`, `enabled`, `disabled`

Read and explain a declaration with either CLI form:

```text
ctrl machine declaration primary-workstation
ctrl machine.declaration primary-workstation
```

Add `--json` for the canonical structured Action result:

```text
ctrl --json machine declaration home-server
```

Invalid JSON, unsupported versions, malformed roles, duplicate capabilities or requirement IDs, invalid states, invalid source references, and a role/file mismatch are returned as explicit diagnostics. Central does not silently reinterpret invalid source as authored intent.
