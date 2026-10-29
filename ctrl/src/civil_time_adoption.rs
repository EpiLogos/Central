//! The civil-time policy a ground's Day stands on, adopted on the person's acceptance (O-I #740; manual §10.11 Ground, §18.2 Day).
//!
//! Central's Day reads exactly one recognised civil-time policy at the root (`continuous_work::temporal::time_policy`): a
//! human-authored or human-adopted source with a standing of authority and a Recognition, never a harness-local fallback. A ground
//! Setup makes (`ctrl init`) keeps none, so it has no Day. Central adopts none by itself; the person adopts one where Setup's Ground
//! step previews it:
//!
//! - `central.time.policy.preview` (read only) reads this computer's IANA time zone (`TZ`, else the `/etc/localtime` link) and says
//!   what Accept would write — `Control/user/civil-time-policy.json` with that zone, the Day turning at midnight and rolling over by
//!   itself — with the digest Accept is pressed on; or that the ground keeps a recognised policy already (and which), or keeps one
//!   Central does not recognise (and why). Before the ground is made it previews for the ground about to be made.
//! - `central.time.policy.adopt` writes that policy and its source relation on the preview's digest: provenance `human-adopted`,
//!   standing `architecture-contract`, recognised by the acceptance the caller names (`acceptance_ref`, the Setup Accept's receipt).
//!   As for any human-adopted source (`central.self.source.create`), the caller declares a human (`actor_kind: human`, no agent
//!   session) and the acceptance (`acceptance: human-accepted`). A recognised policy is kept, never replaced; one Central does not
//!   recognise is refused by name and left as it is.
use crate::action::{
    ActionAvailability, ActionDescriptor, ActionExecutionContext, ActionInputDefinition,
    ActionOutputDefinition, ActionRegistry, MutationClass,
};
use crate::continuous_work::source::{self, Scope};
use crate::continuous_work::temporal::{self, TimePolicy};
use crate::result::{ActionResult, ResultStatus};
use crate::root::resolve_central_root;
use chrono_tz::Tz;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const PREVIEW: &str = "central.time.policy.preview";
const ADOPT: &str = "central.time.policy.adopt";
const PREVIEW_SCHEMA: &str = "central.civil-time-adoption-preview/v1";
pub const POLICY_SOURCE: &str = "Control/user/civil-time-policy.json";
const ROLE: &str = "civil-time-policy";
/// The way on Central names where a ground keeps no civil-time policy.
pub const WAY_ON: &str =
    "the person adopts one with central.time.policy.adopt (preview: central.time.policy.preview)";

/// This computer's IANA time zone and where it was read: `TZ` (a zone name, `:`-prefixed or a zoneinfo path), else the
/// `/etc/localtime` link's zoneinfo name. A zone chrono-tz does not know is no zone. Pure over its two readings.
pub fn timezone_from(
    tz: Option<OsString>,
    localtime: io::Result<PathBuf>,
) -> io::Result<(String, &'static str)> {
    let named = |text: &str| -> Option<String> {
        let text = text.trim().trim_start_matches(':');
        let name = text.rsplit_once("zoneinfo/").map_or(text, |(_, name)| name);
        name.parse::<Tz>().ok().map(|zone| zone.name().to_owned())
    };
    if let Some(zone) = tz
        .as_ref()
        .and_then(|value| value.to_str())
        .filter(|value| !value.trim().is_empty())
        .and_then(named)
    {
        return Ok((zone, "TZ"));
    }
    if let Some(zone) = localtime
        .ok()
        .and_then(|link| link.to_str().and_then(named))
    {
        return Ok((zone, "/etc/localtime"));
    }
    Err(io::Error::new(
        io::ErrorKind::NotFound,
        "this computer names no IANA time zone (TZ, /etc/localtime); Central adopts none in its place",
    ))
}
fn computer_timezone() -> io::Result<(String, &'static str)> {
    timezone_from(std::env::var_os("TZ"), std::fs::read_link("/etc/localtime"))
}

/// What the ground keeps, and what Accept would write.
struct Reading {
    state: &'static str,
    policy: Value,
    timezone_source: Option<&'static str>,
    reason: Option<String>,
}
impl Reading {
    /// The digest Accept is pressed on: the policy and where it is written. Only a write has one.
    fn digest(&self) -> Option<String> {
        matches!(self.state, "absent" | "no-ground").then(|| {
            format!(
                "{:x}",
                Sha256::digest(format!("{POLICY_SOURCE}\n{}", self.policy).as_bytes())
            )
        })
    }
    fn value(&self) -> Value {
        json!({"schema":PREVIEW_SCHEMA,"state":self.state,"source_path":POLICY_SOURCE,"policy":self.policy,
            "timezone_source":self.timezone_source,"digest":self.digest(),"reason":self.reason,
            "day_boundary":"midnight","automatic_agent_or_model_invocation":false})
    }
}
fn proposed() -> io::Result<(Value, &'static str)> {
    let (timezone, from) = computer_timezone()?;
    let policy = TimePolicy {
        schema: "central.civil-time-policy/v1".into(),
        scope_ref: crate::source_horizon::CONTROL_WORLD_REF.into(),
        timezone,
        day_boundary_minutes: 0,
        automatic_day_rollover: true,
    };
    Ok((serde_json::to_value(policy)?, from))
}
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or_default()
}
/// The ground's root scope, or `None` while there is no ground at `root` yet (a missing folder, or one Central has not made).
fn ground(root: &Path) -> io::Result<Option<Scope>> {
    match std::fs::metadata(root) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
        Ok(_) => {}
    }
    match Scope::resolve(root, None) {
        Ok(scope) => Ok(Some(scope)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}
fn read(scope: Option<&Scope>) -> io::Result<Reading> {
    let Some(scope) = scope else {
        let (policy, from) = proposed()?;
        return Ok(Reading {
            state: "no-ground",
            policy,
            timezone_source: Some(from),
            reason: None,
        });
    };
    let (relations, _) = scope.relations()?;
    let related = relations["relations"]
        .as_array()
        .ok_or_else(|| source::invalid("invalid source relations"))?
        .iter()
        .filter(|relation| {
            relation["roles"]
                .as_array()
                .is_some_and(|roles| roles.iter().any(|role| role == ROLE))
        })
        .count();
    if related > 0 {
        return Ok(match temporal::time_policy(scope, now()) {
            Ok(reading) => Reading {
                state: "recognised",
                policy: serde_json::to_value(reading.policy)?,
                timezone_source: None,
                reason: None,
            },
            Err(error) => Reading {
                state: "unrecognised",
                policy: Value::Null,
                timezone_source: None,
                reason: Some(format!(
                    "Central does not recognise this ground's civil-time policy: {error}. It is left as it is."
                )),
            },
        });
    }
    match crate::source_safety::read(&scope.root, POLICY_SOURCE) {
        Ok(_) => {
            return Ok(Reading {
                state: "unrecognised",
                policy: Value::Null,
                timezone_source: None,
                reason: Some(format!(
                    "{POLICY_SOURCE} stands in this ground, but no source relation names it: Central does not adopt or replace it. Mend it by hand."
                )),
            })
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    let (policy, from) = proposed()?;
    Ok(Reading {
        state: "absent",
        policy,
        timezone_source: Some(from),
        reason: None,
    })
}

fn failure(action: &str, error: io::Error) -> ActionResult {
    let status = match error.kind() {
        io::ErrorKind::PermissionDenied => ResultStatus::UnavailableCapability,
        io::ErrorKind::AlreadyExists => ResultStatus::VerificationFailure,
        io::ErrorKind::InvalidInput | io::ErrorKind::InvalidData => ResultStatus::InvalidInput,
        io::ErrorKind::NotFound => ResultStatus::UnavailableCapability,
        _ => ResultStatus::InternalFailure,
    };
    ActionResult::failure(Some(action), status, error.to_string(), None)
}
fn root_of(action: &str, context: &ActionExecutionContext<'_>) -> Result<PathBuf, ActionResult> {
    resolve_central_root(context.root_options)
        .map(|root| root.path)
        .map_err(|message| {
            ActionResult::failure(Some(action), ResultStatus::InvalidInput, message, None)
        })
}

fn preview_action(
    _: &ActionRegistry,
    _: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let root = match root_of(PREVIEW, context) {
        Ok(root) => root,
        Err(result) => return result,
    };
    match ground(&root).and_then(|scope| read(scope.as_ref())) {
        Ok(reading) => ActionResult::success(PREVIEW, reading.value()),
        Err(error) => failure(PREVIEW, error),
    }
}

/// The declared human acceptance a human-adopted source requires, and the acceptance it is recognised by.
fn acceptance(input: &Value) -> io::Result<String> {
    let text = |field: &str| input.get(field).and_then(Value::as_str);
    if text("actor_kind") != Some("human")
        || input.get("agent_session_ref").is_some_and(|v| !v.is_null())
        || text("acceptance") != Some("human-accepted")
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "a human-adopted civil-time policy requires a declared human caller, no agent_session_ref, and acceptance human-accepted",
        ));
    }
    let reference = text("acceptance_ref")
        .map(str::trim)
        .filter(|r| !r.is_empty() && r.len() <= 512 && !r.chars().any(char::is_control))
        .ok_or_else(|| {
            source::invalid("acceptance_ref names the person's acceptance this policy is recognised by (at most 512 characters)")
        })?;
    Ok(reference.to_owned())
}

fn adopt(root: &Path, input: &Value) -> io::Result<Value> {
    let reference = acceptance(input)?;
    let expected = input
        .get("expected_digest")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let scope = ground(root)?.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "there is no ground here yet: make the ground first, then adopt its civil-time policy",
        )
    })?;
    let at = now();
    let source_ref = scope.source_ref(POLICY_SOURCE);
    {
        let _locks = source::lock(&scope)?;
        let reading = read(Some(&scope))?;
        match reading.state {
            "recognised" => {
                drop(_locks);
                let time = temporal::time_policy(&scope, at)?;
                return Ok(
                    json!({"adopted":false,"state":"recognised","source_ref":time.source_ref,"reading":time}),
                );
            }
            "unrecognised" => {
                return Err(source::conflict(reading.reason.unwrap_or_else(|| {
                    "an unrecognised civil-time policy stands".into()
                })))
            }
            _ => {}
        }
        if reading.digest().as_deref() != Some(expected) {
            return Err(source::conflict(format!(
                "the civil-time policy Central would adopt changed since its preview (or no preview was named): preview it again. Nothing was written. (digest {})",
                reading.digest().unwrap_or_default()
            )));
        }
        source::put_new(
            &scope.root,
            POLICY_SOURCE,
            &source::encoded(&reading.policy)?,
        )?;
        let (mut relations, basis) = scope.relations()?;
        relations["relations"]
            .as_array_mut()
            .ok_or_else(|| source::invalid("invalid source relations"))?
            .push(json!({"ref":source_ref,"path":POLICY_SOURCE,"roles":[ROLE],"provenance":"human-adopted",
                "standing":"architecture-contract","treatment":"projectcentral-user",
                "recognition":format!("setup-ground-acceptance:{reference}"),"recorded_at_unix_seconds":at}));
        temporal::save_relations(&scope, &relations, &basis)?;
        scope.reconcile(
            Some(("human:setup-acceptance", "human", None)),
            std::slice::from_ref(&source_ref),
        )?;
    }
    let time = temporal::time_policy(&scope, at)?;
    Ok(json!({"adopted":true,"state":"recognised","source_ref":source_ref,"reading":time}))
}

fn adopt_action(
    _: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let root = match root_of(ADOPT, context) {
        Ok(root) => root,
        Err(result) => return result,
    };
    match adopt(&root, input) {
        Ok(value) => ActionResult::success(ADOPT, value),
        Err(error) => failure(ADOPT, error),
    }
}

fn input(name: &str, required: bool) -> ActionInputDefinition {
    ActionInputDefinition {
        name: name.to_owned(),
        input_type: "string".to_owned(),
        required,
        choices: None,
        selection: None,
    }
}
fn descriptor(
    id: &str,
    title: &str,
    description: &str,
    mutation_class: MutationClass,
    output_type: &str,
    inputs: Vec<ActionInputDefinition>,
) -> ActionDescriptor {
    ActionDescriptor {
        id: id.to_owned(),
        title: title.to_owned(),
        description: description.to_owned(),
        inputs,
        output: ActionOutputDefinition {
            output_type: output_type.to_owned(),
        },
        mutation_class,
        preview_supported: false,
        required_ports: Vec::new(),
        availability: ActionAvailability {
            available: true,
            reason: None,
        },
    }
}

pub(crate) fn register_civil_time_adoption_actions(registry: &mut ActionRegistry) {
    let actions = [
        (
            descriptor(
                PREVIEW,
                "Preview adopting a civil-time policy",
                "Say what adopting this ground's civil-time policy would write: Control/user/civil-time-policy.json with this computer's IANA time zone (TZ, else /etc/localtime), the Day turning at midnight and rolling over by itself, and the digest central.time.policy.adopt is pressed on; or the recognised policy the ground keeps already, or why Central does not recognise the one it keeps. Before the ground is made, it previews for the ground about to be made. Read-only.",
                MutationClass::ReadOnly,
                "civil-time-adoption-preview",
                Vec::new(),
            ),
            preview_action as fn(&ActionRegistry, &Value, &ActionExecutionContext<'_>) -> ActionResult,
        ),
        (
            descriptor(
                ADOPT,
                "Adopt the previewed civil-time policy",
                "Write the previewed civil-time policy and its source relation on the preview's digest (expected_digest): provenance human-adopted, standing architecture-contract, recognised by the person's acceptance acceptance_ref names. Requires a declared human caller (actor_kind human, no agent_session_ref) and acceptance human-accepted. A recognised policy is kept, never replaced; one Central does not recognise is refused and left as it is.",
                MutationClass::LocallyMutating,
                "civil-time-adoption-receipt",
                vec![
                    input("actor_kind", true),
                    input("acceptance", true),
                    input("acceptance_ref", true),
                    input("expected_digest", true),
                ],
            ),
            adopt_action,
        ),
    ];
    for (descriptor, handler) in actions {
        registry
            .register(descriptor, handler)
            .expect("civil-time adoption Action ids are valid");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_computers_zone_is_read_from_tz_then_the_localtime_link() {
        let none = || Err(io::Error::new(io::ErrorKind::NotFound, "no link"));
        assert_eq!(
            timezone_from(Some("Pacific/Auckland".into()), none()).unwrap(),
            ("Pacific/Auckland".into(), "TZ")
        );
        assert_eq!(
            timezone_from(Some(":America/New_York".into()), none()).unwrap(),
            ("America/New_York".into(), "TZ")
        );
        assert_eq!(
            timezone_from(
                Some("/usr/share/zoneinfo/Asia/Tokyo".into()),
                Ok("/var/db/timezone/zoneinfo/Europe/London".into())
            )
            .unwrap(),
            ("Asia/Tokyo".into(), "TZ")
        );
        assert_eq!(
            timezone_from(None, Ok("/var/db/timezone/zoneinfo/Europe/London".into())).unwrap(),
            ("Europe/London".into(), "/etc/localtime")
        );
        // A TZ that names no IANA zone (a POSIX rule) falls to the link; neither naming one is no zone.
        assert_eq!(
            timezone_from(
                Some("EST5EDT,M3.2.0".into()),
                Ok("/usr/share/zoneinfo/UTC".into())
            )
            .unwrap(),
            ("UTC".into(), "/etc/localtime")
        );
        assert!(timezone_from(Some("Nowhere/Atlantis".into()), Ok("/etc/nothing".into())).is_err());
        assert!(timezone_from(None, none()).is_err());
    }
}
