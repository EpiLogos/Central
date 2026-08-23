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

The macOS Connector adds a source-grounded implementation using the system `/usr/bin/osascript` command and AppleScript Standard Additions `display notification`. Apple documents that this posts through Notification Center and that presentation is controlled by the user's Notification settings; the Connector therefore reports authorization as `provider_managed` rather than claiming to know whether a banner was actually seen.

The AppleScript provider deliberately does not claim explicit callback, urgency or category support. Unsupported requested urgency/category metadata is retained in the delivery result; explicit callback requests are rejected rather than silently weakened.

## Ownership

Central owns the semantic Action and provider-neutral Port. `personal.macos-native` supplies the macOS implementation. O:I desktop, AIKit TUI and agents can consume the same Action/read model; none needs macOS-specific notification behaviour in its own semantic state.
