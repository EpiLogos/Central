//! Native credential verification, not an H display-label or claimed actor kind.
//! Authority is an explicitly recognised root SourceRef, never a new account DB.
use super::source::{self, conflict, denied, invalid, Scope, SourceReading};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{io, path::Path};

pub const AUTHORITY_ROLE: &str = "native-action-authority";
/// Every Action Central authenticates against the native-action authority: the one list a person's grant is made of when their
/// ground's authority is adopted (crate::ground_founding). `authenticate` refuses any Action outside it, so an authenticated Action
/// added to the code without being listed here never authenticates, and cannot fall outside the person's grant unseen.
pub const AUTHENTICATED_ACTIONS: &[&str] = &[
    "agent-profile.accept",
    "central.day.close-carry",
    "central.day.ensure",
    "central.day.entry.part",
    "central.day.field.record",
    "central.day.lifecycle",
    "central.day.move.record",
    "central.day.rollover",
    "central.day.settings.write",
    "central.document.create",
    "central.document.mutate",
    "central.file-map.adopt-db",
    "central.file-map.link",
    "central.file-map.move-apply",
    "central.file-map.move-plan",
    "central.file-map.move-rollback",
    "central.file-map.pool",
    "central.file-map.record-adopt",
    "central.file-map.register",
    "central.file-map.scope-register",
    "central.files.trash",
    "central.flow.append",
    "central.migration.apply",
    "central.migration.plan",
    "central.migration.recover",
    "central.migration.rollback",
    "central.now.lifecycle",
    "central.now.obligations",
    "central.receiving.include",
    "central.receiving.recover",
    "central.receiving.review",
    "central.receiving.submit",
    "central.world.chain-order",
    "central.world.name",
    "central.world.place",
    "control.apply-proposal",
    "control.skills.revision.accept",
    "control.skills.revision.discard",
    "projectcentral.source.return_accept",
    "projectcentral.source.return_reject",
];
/// The authenticated Actions that take a project scope: every one of `AUTHENTICATED_ACTIONS` whose Action names a `project`, and the
/// file map's three moves, which reach a project by the Source they move and where they move it. A move stands at the root, since it
/// can bring a folder into Work, and at each Work project it writes in: where its Source stands, where it goes, and each one whose
/// links it rewrites (DLW build ruling 2026-11-09, round 54d F1 and round 54g item 4; file_map.rs `grant_scope`). Those left out,
/// `central.flow.append`, `control.apply-proposal` and the file map's `scope-register`, always resolve the root: the root registers
/// every scope. `authenticate` refuses any other
/// Action at a project, so this list is exactly what a grant covering `project:<id>` lets its holder do there; extending a person's
/// grant to a project they bring in (crate::ground_projects) grants these.
pub const PROJECT_AUTHENTICATED_ACTIONS: &[&str] = &[
    "agent-profile.accept",
    "central.day.close-carry",
    "central.day.ensure",
    "central.day.entry.part",
    "central.day.field.record",
    "central.day.lifecycle",
    "central.day.move.record",
    "central.day.rollover",
    "central.day.settings.write",
    "central.document.create",
    "central.document.mutate",
    "central.file-map.adopt-db",
    "central.file-map.link",
    "central.file-map.move-apply",
    "central.file-map.move-plan",
    "central.file-map.move-rollback",
    "central.file-map.pool",
    "central.file-map.record-adopt",
    "central.file-map.register",
    "central.files.trash",
    "central.migration.apply",
    "central.migration.plan",
    "central.migration.recover",
    "central.migration.rollback",
    "central.now.lifecycle",
    "central.now.obligations",
    "central.receiving.include",
    "central.receiving.recover",
    "central.receiving.review",
    "central.receiving.submit",
    "central.world.chain-order",
    "central.world.name",
    "central.world.place",
    "control.skills.revision.accept",
    "control.skills.revision.discard",
    "projectcentral.source.return_accept",
    "projectcentral.source.return_reject",
];
/// Actions a host authenticates against the person's grant itself, never through `authenticate` (which refuses them, as no Action of
/// Central's), each at the project the Run's Factory state stands in: the O:I kernel's close of a Factory Run (its `factory.rs`
/// RUN_CLOSE_ACTION), its retry of a unit, Factory's own `retry` attempt operation (Factory's admission names it
/// `factory.attempt.retry`), its request that a unit's current attempt stop, Factory's own `request-cancellation`
/// (`factory.attempt.request-cancellation`), and its launch of a unit, Factory's own `start-serial` assembled from the owners'
/// readings (`factory.attempt.launch`). A grant extended to a project holds them there, so the host's own check finds them.
pub const HOST_AUTHENTICATED_ACTIONS: &[&str] = &[
    "factory.run.close",
    "factory.attempt.retry",
    "factory.attempt.request-cancellation",
    "factory.attempt.launch",
];

/// Returns the exact authored source and relation basis. Unrelated relation
/// additions do not stale this source, but changing its Recognition does.
pub(crate) fn recognised_source(
    scope: &Scope,
    role: &str,
) -> io::Result<(SourceReading, Value, String)> {
    let (relations, _) = scope.relations()?;
    let entries: Vec<_> = relations["relations"]
        .as_array()
        .ok_or_else(|| invalid("invalid source relations"))?
        .iter()
        .filter(|r| {
            r["roles"]
                .as_array()
                .is_some_and(|roles| roles.iter().any(|v| v == role))
        })
        .collect();
    if entries.len() != 1 {
        return Err(denied(format!(
            "requires one explicitly recognised {role} source"
        )));
    }
    let relation = entries[0].clone();
    let reading = scope.read(source::text(&relation, "ref")?)?;
    if !matches!(
        reading.source.provenance.as_str(),
        "human-authored" | "human-adopted"
    ) || !matches!(
        reading.source.standing.as_str(),
        "authored-human-position" | "design-commitment" | "architecture-contract"
    ) || relation["recognition"].as_str().is_none_or(|r| {
        r.trim().is_empty() || r == "owner-recorded-source-relation-not-human-recognition"
    }) {
        return Err(denied(format!(
            "{role} source is unrecognised or has no human authority"
        )));
    }
    let basis = source::revision(&serde_json::to_string(
        &serde_json::json!({"source":reading.source,"revision":reading.revision,"relation":relation}),
    )?);
    Ok((reading, relation, basis))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Authority {
    schema: String,
    scope_ref: String,
    grants: Vec<Grant>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Grant {
    principal_ref: String,
    actor_kind: String,
    token_sha256: String,
    scope_refs: Vec<String>,
    actions: Vec<String>,
    expires_at_unix_seconds: u64,
}
#[derive(Debug, Clone, Serialize)]
pub struct Principal {
    pub principal_ref: String,
    pub actor_kind: String,
    pub scope_ref: String,
    pub authority_ref: String,
    pub authority_revision: String,
    pub expires_at_unix_seconds: u64,
    permitted_actions: Vec<String>,
}
impl Principal {
    pub fn is_human(&self) -> bool {
        self.actor_kind == "human"
    }
    pub(crate) fn require_human(&self) -> io::Result<()> {
        if self.is_human() {
            Ok(())
        } else {
            Err(denied(HUMAN_ONLY))
        }
    }
    // Part of the Principal authority surface: reports whether a specific
    // action falls inside the grant. The `permitted_actions` it reads are
    // populated from the accepted grant, but no caller enforces them yet — an
    // unwired seam retained here rather than deleted (see PR notes).
    #[allow(dead_code)]
    pub(crate) fn permits(&self, action: &str) -> bool {
        self.permitted_actions.iter().any(|a| a == action)
    }
}
fn equal_digest(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len()
        && a.iter()
            .zip(b)
            .fold(0u8, |difference, (a, b)| difference | (a ^ b))
            == 0
}
/// The host's credential as `authenticate` takes it: present, and within its bounds. A token outside them is refused by
/// `authenticate` before any source is read, in the token's own words, so a caller that words a missing source (the trash's
/// `missing_authority`) leaves that refusal as it came (O-I #747 round 896 1T, N1).
pub(crate) fn bounded_token(token: Option<&str>) -> Option<&str> {
    token.filter(|t| t.len() >= 32 && t.len() <= 4096)
}
/// The host supplies CENTRAL_NATIVE_TOKEN through its protected process channel.
/// The JSON document/proposal cannot inject this credential. A bearer credential
/// authenticates its source-declared principal, not physical human presence.
/// Same-UID hostile processes still need Workcell/OS credential isolation.
pub fn authenticate(
    scope: &Scope,
    token: Option<&str>,
    action: &str,
    expected: Option<&str>,
    now: u64,
) -> io::Result<Principal> {
    listed(action)?;
    if scope.project.is_some() && !PROJECT_AUTHENTICATED_ACTIONS.contains(&action) {
        return Err(invalid(format!(
            "{action} is not among Central's PROJECT_AUTHENTICATED_ACTIONS; an Action authenticated at a project is listed there first, so a grant extended to the project holds it"
        )));
    }
    let (grant, authority_ref, authority_revision) =
        held(&scope.central_root, token, action, expected, now)?;
    if !grant.scope_refs.contains(&scope.world_ref) {
        return Err(denied(LACKS));
    }
    Ok(Principal {
        principal_ref: grant.principal_ref,
        actor_kind: grant.actor_kind,
        scope_ref: scope.world_ref.clone(),
        authority_ref,
        authority_revision,
        expires_at_unix_seconds: grant.expires_at_unix_seconds,
        permitted_actions: grant.actions,
    })
}

/// [`authenticate`]'s questions that do not depend on the scope, asked before any scope is read: the credential is bounded, it
/// matches one grant of the root's authority Source, and that grant names this exact Action, has not expired, and is a human's.
/// The file map's acts of the person's own ask this first, then read where their grant must stand, so an act refused here reads
/// no scope and makes none (file_map.rs `the_persons_act`; DLW build ruling 2026-11-09, round 54b item 1). [`authenticate`] at
/// that scope then asks every question again.
pub(crate) fn authenticate_before_scope(
    central_root: &Path,
    token: Option<&str>,
    action: &str,
    expected: Option<&str>,
    now: u64,
) -> io::Result<()> {
    listed(action)?;
    let (grant, _, _) = held(central_root, token, action, expected, now)?;
    if grant.actor_kind != "human" {
        return Err(denied(HUMAN_ONLY));
    }
    Ok(())
}

/// What a refusal says when a credential is not a human's.
const HUMAN_ONLY: &str =
    "this mutation requires a human-scoped native credential, not an actor label";
/// What a refusal says when a grant does not hold this Action, here, now.
const LACKS: &str = "native credential is expired or lacks this exact scope/action";

/// Every Action authenticated here is listed in `AUTHENTICATED_ACTIONS` first.
fn listed(action: &str) -> io::Result<()> {
    if AUTHENTICATED_ACTIONS.contains(&action) {
        return Ok(());
    }
    Err(invalid(format!(
        "{action} is not among Central's AUTHENTICATED_ACTIONS; an authenticated Action is listed there first, so the person's grant holds it"
    )))
}

/// What a refusal says of an authority Source that does not read as one: Central's own words, never the parser's, which can echo a
/// value of the Source to a caller who holds no grant (round 54d F5).
const INVALID_AUTHORITY: &str = "invalid native-action-authority source";

/// The one grant of the root's authority Source that `token` matches, unexpired and naming `action`, with the Source's reference
/// and revision. Where the grant holds is the caller's question.
///
/// The token is matched before the Source's revision is compared with the one the caller expects, so a caller without a valid token
/// learns nothing of the revision: it is refused as not authorised whatever revision it names (round 54d F5).
fn held(
    central_root: &Path,
    token: Option<&str>,
    action: &str,
    expected: Option<&str>,
    now: u64,
) -> io::Result<(Grant, String, String)> {
    let token = bounded_token(token)
        .ok_or_else(|| denied("missing bounded CENTRAL_NATIVE_TOKEN; declared H/human attribution is not authentication"))?;
    let root = Scope::resolve(central_root, None)?;
    let (source, _, revision) = recognised_source(&root, AUTHORITY_ROLE)?;
    let authority: Authority =
        serde_json::from_str(&source.content).map_err(|_| invalid(INVALID_AUTHORITY))?;
    if authority.schema != "central.native-action-authority/v1"
        || authority.scope_ref != root.world_ref
        || authority.grants.len() > 1024
    {
        return Err(invalid(INVALID_AUTHORITY));
    }
    let digest = source::key(token);
    let mut matched = None;
    for grant in authority.grants {
        if grant.token_sha256.len() != 64
            || !grant.token_sha256.bytes().all(|b| b.is_ascii_hexdigit())
            || grant.principal_ref.trim().is_empty()
            || grant.principal_ref.len() > 4096
            || !matches!(
                grant.actor_kind.as_str(),
                "human" | "agent" | "native-service"
            )
        {
            return Err(invalid("invalid source-declared native grant"));
        }
        if equal_digest(grant.token_sha256.as_bytes(), digest.as_bytes()) {
            if matched.is_some() {
                return Err(denied(
                    "ambiguous credential grants require source reconciliation",
                ));
            }
            matched = Some(grant);
        }
    }
    let grant = matched.ok_or_else(|| denied("native credential is not authorised"))?;
    if expected.is_some_and(|basis| basis != revision) {
        return Err(conflict("native authority revision changed"));
    }
    if now >= grant.expires_at_unix_seconds || !grant.actions.iter().any(|a| a == action) {
        return Err(denied(LACKS));
    }
    Ok((grant, source.source.source_ref, revision))
}
