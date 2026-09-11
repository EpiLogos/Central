//! Native credential verification, not an H display-label or claimed actor kind.
//! Authority is an explicitly recognised root SourceRef, never a new account DB.
use super::source::{self, conflict, denied, invalid, Scope, SourceReading};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io;

pub const AUTHORITY_ROLE: &str = "native-action-authority";

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
