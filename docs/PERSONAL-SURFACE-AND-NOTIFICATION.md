# Personal authored-ground projection and notification Port

This slice advances #51 and #52 without creating a personal-profile database or desktop-specific semantic owner.

## Personal projection

`central_ctrl::create_personal_action_registry()` extends the existing Action registry with:

- `personal.show` — projects the actual authored roots as `You → Control/user`, `Agents → Control/agents`, `Machines → Control/machines`, `Work → Work`;
- `control.propose-change` — writes explicit proposal material only under derived `.central/proposals` state and does not mutate authored Control source;
- `control.review-proposal` — reads a proposal before acceptance;
- `control.apply-proposal` — requires an explicit `accepted_by_ref` and only then writes the authored filesystem target.

Proposal targets are bounded to `Control/user`, `Control/agents` and `Control/machines`; traversal paths are rejected. This is a proposal/acceptance path for durable preferences and context, not a learning loop. Direct filesystem edits remain visible because the authored files remain canonical.

## UserNotification Port

`central.connector-sdk` now exports `UserNotification` with separate capability inspection and delivery operations. A `NotificationRequest` preserves the semantic subject, optional source Action, caller lineage and provenance independently from the provider.

`personal.notify` resolves the Port through the existing Connector registry. A successful result explicitly states that notification delivery is **not** human acknowledgement or approval.

`personal.notify` and `personal.notifications.list` are installed in the runtime catalogue (`ctrl/src/notification.rs`, registered by `create_runtime_action_registry` and by `create_personal_action_registry`), so the CLI, the macOS host and the O:I kernel reach them like any other Action.

Every request `personal.notify` accepts is kept with what the provider answered: one `central.notification-record/v1` line per request in `.central/notifications/ledger.jsonl` under the resolved root, appended under the root's `notifications.lock`. A record carries its `request_ref`, `requested_at_unix_ms`, the `NotificationRequest` verbatim, and either the provider's `NotificationDelivery` or its refusal (`status`, `message`). A request the Port rejects as invalid is refused before anything is kept. Delivery never waits on the ledger: a request asked where no Central root resolves, or whose ledger cannot be written, is still delivered, and the answer says `kept: false` with the reason.

`personal.notifications.list` (read-only; `limit` 1–1000, default 200; optional `since_unix_ms`) answers `central.notification-ledger/v1`: the kept records newest first, a count of ledger lines that are no record (`unreadable_records`, never dropped silently), and the selected provider's own `NotificationCapabilities` — or `provider: null` with `provider_error` when no provider is selected. Reading changes nothing, and nothing in the answer is the person's acknowledgement.

The macOS Connector adds a source-grounded implementation using the system `/usr/bin/osascript` command and AppleScript Standard Additions `display notification`. Apple documents that this posts through Notification Center and that presentation is controlled by the user's Notification settings; the Connector therefore reports authorization as `provider_managed` rather than claiming to know whether a banner was actually seen.

The AppleScript provider deliberately does not claim explicit callback, urgency or category support. Unsupported requested urgency/category metadata is retained in the delivery result; explicit callback requests are rejected rather than silently weakened.

## Ownership

Central owns the semantic Action and provider-neutral Port. `personal.macos-native` supplies the macOS implementation. O:I desktop, AIKit TUI and agents can consume the same Action/read model; none needs macOS-specific notification behaviour in its own semantic state.
