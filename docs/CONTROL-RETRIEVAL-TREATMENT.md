# Control retrieval treatment

`docs/CONTROL-CONTENT-PROTOCOL.md` distinguishes source existence from permission to retrieve or load that source and explicitly allows a `not agent-readable` treatment class.

The stock `control.search` Action implements that treatment with one ordinary-filesystem convention:

```text
Control/
└── user/
    ├── normal.md
    └── private-context/
        ├── .no-agent-retrieval
        └── notes.md
```

When `.no-agent-retrieval` is present as a file in a directory, `control.search` does not scan that directory or any descendants.

The result records the excluded subtree in `skipped_sources` with:

```json
{
  "source_class": "authored",
  "reason": "not_agent_readable"
}
```

The marker does **not** encrypt, delete, relocate, or change authorship of the files. A human can still open the ordinary files directly through the filesystem and can choose a stronger local-only or encrypted storage treatment when required. The marker controls Central's stock agent-facing retrieval path only.

The treatment is inherited by descendants until a new retrieval architecture deliberately defines a richer policy model. There is no opt-back-in marker beneath a denied subtree because allowing a child to override an ancestor denial would make exclusion hard to audit.

This convention is intentionally narrow. It provides an executable safe treatment without imposing a universal schema below `Control/user`, `Control/agents`, or `Control/machines`.
