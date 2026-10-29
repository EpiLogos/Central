//! Native credential verification, not an H display-label or claimed actor kind.
//! Authority is an explicitly recognised root SourceRef, never a new account DB.
use super::source::{self, conflict, denied, invalid, Scope, SourceReading};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io;

pub const AUTHORITY_ROLE: &str = "native-action-authority";
/// Every Action Central authenticates against the native-action authority: the one list a person's grant is made of when their
/// ground's authority is adopted (crate::ground_founding). `authenticate` refuses any Action outside it, so an authenticated Action
/// added to the code without being listed here never authenticates, and cannot fall outside the person's grant unseen.
pub const AUTHENTICATED_ACTIONS: &[&str] = &[
    "agent-profile.accept",
    "central.day.close-carry",
    "central.day.ensure",
    "central.day.lifecycle",
    "central.day.move.record",
    "central.day.rollover",
    "central.day.settings.write",
    "central.document.create",
    "central.document.mutate",
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
    "control.apply-proposal",
    "projectcentral.source.return_accept",
    "projectcentral.source.return_reject",
];
/// The authenticated Actions that take a project scope: every one of `AUTHENTICATED_ACTIONS` whose Action names a `project`
/// (the two left out, `central.flow.append` and `control.apply-proposal`, always resolve the root). `authenticate` refuses any other
/// Action at a project, so this list is exactly what a grant covering `project:<id>` lets its holder do there; extending a person's
/// grant to a project they bring in (crate::ground_projects) grants these.
pub const PROJECT_AUTHENTICATED_ACTIONS: &[&str] = &[
    "agent-profile.accept",
    "central.day.close-carry",
    "central.day.ensure",
    "central.day.lifecycle",
    "central.day.move.record",
    "central.day.rollover",
    "central.day.settings.write",
    "central.document.create",
    "central.document.mutate",
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
    "projectcentral.source.return_accept",
    "projectcentral.source.return_reject",
];
/// Actions a host authenticates against the person's grant itself, never through `authenticate` (which refuses them, as no Action of
/// Central's): the O:I kernel's close of a Factory Run (its `factory.rs` RUN_CLOSE_ACTION), at the project the Run's Factory state
/// stands in. A grant extended to a project holds them there, so the host's own check finds them.
pub const HOST_AUTHENTICATED_ACTIONS: &[&str] = &["factory.run.close"];

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
            Err(denied(
                "this mutation requires a human-scoped native credential, not an actor label",
            ))
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
    if !AUTHENTICATED_ACTIONS.contains(&action) {
        return Err(invalid(format!(
            "{action} is not among Central's AUTHENTICATED_ACTIONS; an authenticated Action is listed there first, so the person's grant holds it"
        )));
    }
    if scope.project.is_some() && !PROJECT_AUTHENTICATED_ACTIONS.contains(&action) {
        return Err(invalid(format!(
            "{action} is not among Central's PROJECT_AUTHENTICATED_ACTIONS; an Action authenticated at a project is listed there first, so a grant extended to the project holds it"
        )));
    }
    let token = token.filter(|t| t.len() >= 32 && t.len() <= 4096)
        .ok_or_else(|| denied("missing bounded CENTRAL_NATIVE_TOKEN; declared H/human attribution is not authentication"))?;
    let root = Scope::resolve(&scope.central_root, None)?;
    let (source, _, revision) = recognised_source(&root, AUTHORITY_ROLE)?;
    if expected.is_some_and(|basis| basis != revision) {
        return Err(conflict("native authority revision changed"));
    }
    let authority: Authority = serde_json::from_str(&source.content)?;
    if authority.schema != "central.native-action-authority/v1"
        || authority.scope_ref != root.world_ref
        || authority.grants.len() > 1024
    {
        return Err(invalid("invalid native-action-authority source"));
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
    if now >= grant.expires_at_unix_seconds
        || !grant.scope_refs.contains(&scope.world_ref)
        || !grant.actions.iter().any(|a| a == action)
    {
        return Err(denied(
            "native credential is expired or lacks this exact scope/action",
        ));
    }
    Ok(Principal {
        principal_ref: grant.principal_ref,
        actor_kind: grant.actor_kind,
        scope_ref: scope.world_ref.clone(),
        authority_ref: source.source.source_ref,
        authority_revision: revision,
        expires_at_unix_seconds: grant.expires_at_unix_seconds,
        permitted_actions: grant.actions,
    })
}
