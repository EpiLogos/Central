//! `central.world.place`: a world's declared place (O-I #745; DLW manual §22.1, "The hours are counted at a place: the world's place
//! when Observer is Shared"; QL-MEF #254 D12, "the hour ruler uses the observer's place and time (Nara's location, or a declared world
//! place)"). A real place on Earth, in the shape QL's sky provider counts the day at (`ql.sky-day-request/v1` `place`): a reference, a
//! label, latitude, longitude, altitude and an IANA time zone.
//!
//! It is kept where the world's own record is. A project's world keeps it in its ProjectCentral manifest (`ProjectCentral/project.json`
//! `place`, beside its `name` and its stable `project_id`), made against the manifest's content revision, and the world map discloses it
//! (`world_place`). Your own world, the root register, has no manifest: it keeps its place on its root world record (`control:root`,
//! `central.world-relations/v1`, under `Control/relations/worlds`), made against that record's authored revision, which the act
//! advances; `central.world-relations.read` reads it back. A root with no world record has nowhere to keep one, and the act says so
//! rather than declare the world. The person's own act: it authenticates on their native authority. A stale revision answers conflict
//! and writes nothing; the same place answers unchanged; `null` takes the place away.
//!
//! `central.world.chain-order`: the order of a world's chain (O-I #743; DLW manual §16.2, §18.3: "The packet gathers what the agent
//! reads, the agent of the world answers after it, then Scope limits what it may touch and Docs keeps the world's account"). The
//! packet and the agent head the chain and never move; Scope and Docs stand after them in the order the person gives. Central owns
//! that order (DLW build ruling 2026-11-05) and keeps it where the world keeps its place, as `chain_order`, on the same record and the
//! same revision; `null` gives the chain back the manual's order.
use crate::action::{ActionExecutionContext, ActionRegistry, MutationClass};
use crate::agent_set_store::{RelationRecordStore, RelationRecordStoreError, ROOT_WORLD_DIR};
use crate::civil_time_adoption::{descriptor, failure, input, now, root_of};
use crate::continuous_work::{authority, source::Scope};
use crate::file_mutation::{atomic_record, open_native_file, RecordDisposition};
use crate::projectcentral::parse_project_manifest;
use crate::result::ActionResult;
use crate::source_horizon::CONTROL_WORLD_REF;
use crate::source_safety::content_revision_bytes;
use chrono_tz::Tz;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::{self, Read};
use std::path::Path;

pub const WORLD_PLACE_ACTION: &str = "central.world.place";
pub const WORLD_PLACE_SCHEMA: &str = "central.world-place/v1";
pub const WORLD_CHAIN_ORDER_ACTION: &str = "central.world.chain-order";
pub const WORLD_CHAIN_ORDER_SCHEMA: &str = "central.world-chain-order/v1";
/// The devices of a world's chain that head it and never move (DLW manual §18.3: the packet gathers what the agent reads, and the
/// agent answers after it).
pub const WORLD_CHAIN_FIXED: [&str; 2] = ["packet", "agent"];
/// The devices of a world's chain whose order is the person's, in the manual's order (§16.2: "then Scope limits what it may touch
/// and Docs keeps the world's account").
pub const WORLD_CHAIN_MOVABLE: [&str; 2] = ["scope", "docs"];
const MANIFEST: &str = "ProjectCentral/project.json";
const MANIFEST_LIMIT: u64 = 1024 * 1024;
/// The bounds QL's sky provider counts a day within (QL-MEF `providers/sky/hours.py` `place`).
pub const PLACE_REFERENCE_MAX_CHARS: usize = 512;
pub const PLACE_LABEL_MAX_CHARS: usize = 256;
pub const PLACE_ALTITUDE_METRES: (f64, f64) = (-500.0, 10_000.0);

/// A real place on Earth a world declares.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorldPlace {
    pub reference: String,
    pub label: String,
    pub latitude_degrees: f64,
    pub longitude_degrees: f64,
    pub altitude_metres: f64,
    pub timezone: String,
}

// A kept place's numbers are finite (`validate`), so equality is total on every place a record holds.
impl Eq for WorldPlace {}

/// Whether `value` is words of a place: as a person gives them (`given`), with no control character and no invisible one; as a
/// record keeps them, read back, with no control character, the rule they were kept under (be2i.1).
fn words(value: &str, field: &str, most: usize, given: bool) -> Result<(), String> {
    let count = value.chars().count();
    if value.trim() != value || count == 0 || count > most {
        return Err(format!(
            "{field} must be 1 to {most} characters without surrounding whitespace"
        ));
    }
    if given && value.chars().any(crate::names::unplain) {
        return Err(format!(
            "{field} must hold no control character and no invisible one"
        ));
    }
    if value.chars().any(crate::names::unkept) {
        return Err(format!("{field} must hold no control characters"));
    }
    Ok(())
}

fn within(value: f64, (low, high): (f64, f64), field: &str) -> Result<(), String> {
    if value.is_finite() && (low..=high).contains(&value) {
        Ok(())
    } else {
        Err(format!("{field} must be a number from {low} to {high}"))
    }
}

impl WorldPlace {
    /// Whether this is a place the Sky can count a day at, as a person declares it (`central.world.place`): answers why not, in
    /// words.
    pub fn validate(&self) -> Result<(), String> {
        self.checked(true)
    }

    /// Whether this is a place the Sky can count a day at, as a record keeps it, read back: its words under the rule they were
    /// kept under (be2i.1), so a place kept before be2i that holds an invisible character still reads.
    pub fn validate_kept(&self) -> Result<(), String> {
        self.checked(false)
    }

    /// This place as it is shown: its words without the characters that draw nothing (be2i.1), and a label or reference with
    /// nothing visible left called an unnamed place and its reference's short form, never "" (be2j item 15, ruling 5).
    pub fn shown(&self) -> Self {
        Self {
            reference: crate::names::shown_or_unnamed(&self.reference, "place", &self.reference),
            label: crate::names::shown_or_unnamed(&self.label, "place", &self.reference),
            ..self.clone()
        }
    }

    /// What a reading says, once, of a kept place whose words hold a character that draws nothing (be2i.1): that it is shown
    /// without them, and how to declare it again.
    pub fn kept_note(&self) -> Option<String> {
        crate::names::kept_note(
            &format!("{}{}", self.reference, self.label),
            "This world's place",
            "Declare the place again (central.world.place) to keep it so.",
        )
    }

    fn checked(&self, given: bool) -> Result<(), String> {
        words(
            &self.reference,
            "reference",
            PLACE_REFERENCE_MAX_CHARS,
            given,
        )?;
        words(&self.label, "label", PLACE_LABEL_MAX_CHARS, given)?;
        within(self.latitude_degrees, (-90.0, 90.0), "latitude_degrees")?;
        within(self.longitude_degrees, (-180.0, 180.0), "longitude_degrees")?;
        within(
            self.altitude_metres,
            PLACE_ALTITUDE_METRES,
            "altitude_metres",
        )?;
        if self.timezone.parse::<Tz>().is_err() {
            return Err(format!("timezone {:?} is no IANA time zone", self.timezone));
        }
        Ok(())
    }
}

/// Whether `order` is an order of a world's chain: each device whose place is the person's, once; answers why not, in words.
pub fn chain_order_words(order: &[String]) -> Result<(), String> {
    if let Some(fixed) = order
        .iter()
        .find(|slot| WORLD_CHAIN_FIXED.contains(&slot.as_str()))
    {
        return Err(format!(
            "{fixed} heads a world's chain and never moves (the packet gathers what the agent reads, and the agent answers after it); the order names {}",
            WORLD_CHAIN_MOVABLE.join(" and ")
        ));
    }
    if let Some(stranger) = order
        .iter()
        .find(|slot| !WORLD_CHAIN_MOVABLE.contains(&slot.as_str()))
    {
        return Err(format!(
            "{stranger:?} is no device of a world's chain; the order names {}",
            WORLD_CHAIN_MOVABLE.join(" and ")
        ));
    }
    if order.len() != WORLD_CHAIN_MOVABLE.len()
        || WORLD_CHAIN_MOVABLE
            .iter()
            .any(|slot| order.iter().filter(|kept| kept == slot).count() != 1)
    {
        return Err(format!(
            "the order names each of {} once",
            WORLD_CHAIN_MOVABLE.join(" and ")
        ));
    }
    Ok(())
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message.into())
}

/// The place the input declares (`null` takes it away), validated.
fn place_of(input: &Value) -> io::Result<Option<WorldPlace>> {
    match input.get("place") {
        None => Err(invalid(
            "central.world.place names the place, or null to take it away",
        )),
        Some(Value::Null) => Ok(None),
        Some(value) => {
            let place: WorldPlace = serde_json::from_value(value.clone()).map_err(|error| {
                invalid(format!(
                    "A world's place names reference, label, latitude_degrees, longitude_degrees, altitude_metres and timezone: {error}"
                ))
            })?;
            place
                .validate()
                .map_err(|why| invalid(format!("A world's place: {why}")))?;
            Ok(Some(place))
        }
    }
}

/// The next authored revision of a world record: its trailing counter advanced (`w1` → `w2`), else `.1` added. The record store asks
/// only that it advance (agent_set_store.rs `RevisionNotAdvanced`).
pub fn next_record_revision(revision: &str) -> String {
    let digits = revision
        .chars()
        .rev()
        .take_while(char::is_ascii_digit)
        .count();
    let (stem, counter) = revision.split_at(revision.len() - digits);
    match counter.parse::<u128>() {
        Ok(value) if !counter.is_empty() => format!("{stem}{}", value + 1),
        _ => format!("{revision}.1"),
    }
}

struct Receipt<'a> {
    world_ref: &'a str,
    project: Option<&'a str>,
    source: &'a str,
    place: Option<&'a WorldPlace>,
    principal_ref: &'a str,
}

impl Receipt<'_> {
    fn answer(
        &self,
        outcome: &str,
        previous: Option<&WorldPlace>,
        previous_revision: &str,
        revision: &str,
    ) -> Value {
        json!({"schema":WORLD_PLACE_SCHEMA,"outcome":outcome,"world_ref":self.world_ref,"project":self.project,"source":self.source,
            "place":self.place,"previous_place":previous.map(WorldPlace::shown),"previous_revision":previous_revision,"revision":revision,
            "principal_ref":self.principal_ref,"automatic_agent_or_model_invocation":false})
    }
}

const fn outcome(place: Option<&WorldPlace>) -> &'static str {
    if place.is_some() {
        "placed"
    } else {
        "cleared"
    }
}

/// Declare (or take away) the place of the world `input.project` names, or of your own world when it names none, on the
/// `input.expected_revision` of the record that keeps it, as the person `token` names.
pub fn place_world(
    central: &Path,
    input: &Value,
    token: Option<&str>,
    now: u64,
) -> io::Result<Value> {
    let text = |field: &str| input.get(field).and_then(Value::as_str);
    let project = match input.get("project") {
        None | Some(Value::Null) => None,
        Some(Value::String(project)) if !project.trim().is_empty() => Some(project.as_str()),
        Some(_) => {
            return Err(invalid(
                "central.world.place names a project, or none for your own world",
            ))
        }
    };
    let place = place_of(input)?;
    let expected = text("expected_revision").ok_or_else(|| {
        invalid("central.world.place is made against the record's expected_revision")
    })?;
    let scope = Scope::resolve(central, project)?;
    let principal = authority::authenticate(
        &scope,
        token,
        WORLD_PLACE_ACTION,
        text("expected_authority_revision"),
        now,
    )?;
    principal.require_human()?;
    let _lock = crate::source_safety::lock(&scope.root, "source-mutation.lock")?;
    let kept = keep_world_field(
        central,
        &scope.root,
        project,
        "place",
        "place",
        place.as_ref(),
        expected,
    )?;
    let world_ref = kept.world_ref.clone();
    let receipt = Receipt {
        world_ref: &world_ref,
        project,
        source: &kept.source,
        place: place.as_ref(),
        principal_ref: &principal.principal_ref,
    };
    Ok(match kept.outcome {
        Kept::Conflict => {
            let mut conflict =
                receipt.answer("conflict", None, &kept.previous_revision, &kept.revision);
            conflict["expected_revision"] = json!(expected);
            conflict
        }
        Kept::Unchanged => receipt.answer(
            "unchanged",
            kept.previous.as_ref(),
            &kept.previous_revision,
            &kept.revision,
        ),
        Kept::Written => receipt.answer(
            outcome(place.as_ref()),
            kept.previous.as_ref(),
            &kept.previous_revision,
            &kept.revision,
        ),
    })
}

/// What a world's own record did with one field it keeps.
enum Kept {
    Conflict,
    Unchanged,
    Written,
}

struct KeptField<T> {
    world_ref: String,
    source: String,
    outcome: Kept,
    previous: Option<T>,
    previous_revision: String,
    revision: String,
}

/// Keep `value` as `field` (or take it away, `None`) on the record that keeps the world's own: a project's ProjectCentral manifest,
/// or your own world's root world record, on that record's `expected` revision. Only that field changes (and the root record's
/// revision, which the act advances); a stale revision changes nothing; the same value changes nothing. `what` names the field in
/// words for a root with no world record.
fn keep_world_field<T: Serialize + DeserializeOwned + PartialEq>(
    central: &Path,
    project_root: &Path,
    project: Option<&str>,
    field: &str,
    what: &str,
    value: Option<&T>,
    expected: &str,
) -> io::Result<KeptField<T>> {
    let decode = |kept: Option<&Value>, whose: &str| -> io::Result<Option<T>> {
        match kept {
            None | Some(Value::Null) => Ok(None),
            Some(kept) => serde_json::from_value(kept.clone())
                .map(Some)
                .map_err(|error| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("{whose} {what} cannot be read: {error}"),
                    )
                }),
        }
    };
    let set = |fields: &mut serde_json::Map<String, Value>| match value {
        Some(value) => {
            fields.insert(field.into(), json!(value));
        }
        None => {
            fields.remove(field);
        }
    };
    let Some(project) = project else {
        let store = RelationRecordStore::worlds_at_root(central);
        let reading = store
            .read(CONTROL_WORLD_REF)
            .map_err(|error| store_error(error, what))?;
        let mut kept = KeptField {
            world_ref: CONTROL_WORLD_REF.to_owned(),
            source: reading.source_path.clone(),
            outcome: Kept::Conflict,
            previous: None,
            previous_revision: reading.revision.clone(),
            revision: reading.revision.clone(),
        };
        if reading.revision != expected {
            return Ok(kept);
        }
        kept.previous = decode(reading.record.get(field), "Your world record's")?;
        if kept.previous.as_ref() == value {
            kept.outcome = Kept::Unchanged;
            return Ok(kept);
        }
        let mut record = reading.record.clone();
        let fields = record
            .as_object_mut()
            .ok_or_else(|| invalid("Your world record is not an object"))?;
        set(fields);
        let next = next_record_revision(&reading.revision);
        fields.insert("revision".into(), json!(next));
        store
            .save(&record, Some(&reading.revision))
            .map_err(|error| store_error(error, what))?;
        kept.outcome = Kept::Written;
        kept.revision = next;
        return Ok(kept);
    };
    let mut held = Vec::new();
    open_native_file(project_root, MANIFEST)?
        .take(MANIFEST_LIMIT + 1)
        .read_to_end(&mut held)?;
    if held.len() as u64 > MANIFEST_LIMIT {
        return Err(invalid("The project's manifest exceeds its bounded size"));
    }
    let revision = content_revision_bytes(&held);
    let mut kept = KeptField {
        world_ref: format!("project:{project}"),
        source: MANIFEST.to_owned(),
        outcome: Kept::Conflict,
        previous: None,
        previous_revision: revision.clone(),
        revision: revision.clone(),
    };
    if revision != expected {
        return Ok(kept);
    }
    let path = project_root.join(MANIFEST);
    let manifest = parse_project_manifest(&held, &path)?;
    // Every other field stays as the manifest keeps it, written as ProjectCentral writes its manifest.
    let mut document: Value = serde_json::from_slice(&held)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    kept.previous = decode(document.get(field), "The project's manifest")?;
    if kept.previous.as_ref() == value {
        kept.outcome = Kept::Unchanged;
        return Ok(kept);
    }
    let fields = document
        .as_object_mut()
        .ok_or_else(|| invalid("The project's manifest is not an object"))?;
    set(fields);
    let mut written = serde_json::to_vec_pretty(&document)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    written.push(b'\n');
    let checked = parse_project_manifest(&written, &path)?;
    if checked.project_id != manifest.project_id || !checked.validate().valid {
        return Err(invalid(format!(
            "The manifest with its {what} would not validate; nothing was written"
        )));
    }
    atomic_record(
        project_root,
        Path::new(MANIFEST),
        &written,
        RecordDisposition::ReplaceOrCreate,
    )?;
    kept.outcome = Kept::Written;
    kept.revision = content_revision_bytes(&written);
    Ok(kept)
}

fn store_error(error: RelationRecordStoreError, what: &str) -> io::Error {
    match error {
        RelationRecordStoreError::NotFound(_) => io::Error::new(
            io::ErrorKind::NotFound,
            format!(
                "Your world declares no world record ({ROOT_WORLD_DIR}, {CONTROL_WORLD_REF}): Central keeps your world's {what} on it, and does not declare the world for a {what}. Declare it first (central.world-relations.save)."
            ),
        ),
        RelationRecordStoreError::InvalidRecord(why) => invalid(why),
        other => io::Error::other(other.to_string()),
    }
}

/// Give the world `input.project` names, or your own world when it names none, the chain order `input.order` names (`null`: the
/// manual's order), on the `input.expected_revision` of the record that keeps it, as the person `token` names.
pub fn order_world_chain(
    central: &Path,
    input: &Value,
    token: Option<&str>,
    now: u64,
) -> io::Result<Value> {
    let text = |field: &str| input.get(field).and_then(Value::as_str);
    let project = match input.get("project") {
        None | Some(Value::Null) => None,
        Some(Value::String(project)) if !project.trim().is_empty() => Some(project.as_str()),
        Some(_) => {
            return Err(invalid(
                "central.world.chain-order names a project, or none for your own world",
            ))
        }
    };
    let order: Option<Vec<String>> = match input.get("order") {
        None => {
            return Err(invalid(
                "central.world.chain-order names the order, or null for the manual's order",
            ))
        }
        Some(Value::Null) => None,
        Some(value) => {
            let order: Vec<String> = serde_json::from_value(value.clone()).map_err(|_| {
                invalid("A world's chain order is a list of its devices' ids, scope and docs")
            })?;
            chain_order_words(&order)
                .map_err(|why| invalid(format!("A world's chain order: {why}")))?;
            Some(order)
        }
    };
    let expected = text("expected_revision").ok_or_else(|| {
        invalid("central.world.chain-order is made against the record's expected_revision")
    })?;
    let scope = Scope::resolve(central, project)?;
    let principal = authority::authenticate(
        &scope,
        token,
        WORLD_CHAIN_ORDER_ACTION,
        text("expected_authority_revision"),
        now,
    )?;
    principal.require_human()?;
    let _lock = crate::source_safety::lock(&scope.root, "source-mutation.lock")?;
    let kept = keep_world_field(
        central,
        &scope.root,
        project,
        "chain_order",
        "chain order",
        order.as_ref(),
        expected,
    )?;
    let outcome = match (&kept.outcome, &order) {
        (Kept::Conflict, _) => "conflict",
        (Kept::Unchanged, _) => "unchanged",
        (Kept::Written, Some(_)) => "ordered",
        (Kept::Written, None) => "reset",
    };
    let standing = |order: Option<&Vec<String>>| -> Vec<String> {
        WORLD_CHAIN_FIXED
            .iter()
            .map(|slot| (*slot).to_owned())
            .chain(order.cloned().unwrap_or_else(|| {
                WORLD_CHAIN_MOVABLE
                    .iter()
                    .map(|slot| (*slot).to_owned())
                    .collect()
            }))
            .collect()
    };
    let mut answer = json!({"schema":WORLD_CHAIN_ORDER_SCHEMA,"outcome":outcome,"world_ref":kept.world_ref,"project":project,
        "source":kept.source,"order":order,"chain":standing(order.as_ref()),"previous_order":kept.previous,
        "previous_revision":kept.previous_revision,"revision":kept.revision,"principal_ref":principal.principal_ref,
        "automatic_agent_or_model_invocation":false});
    if matches!(kept.outcome, Kept::Conflict) {
        answer["expected_revision"] = json!(expected);
        answer["chain"] = Value::Null;
    }
    Ok(answer)
}

fn world_chain_order_action(
    _: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let root = match root_of(WORLD_CHAIN_ORDER_ACTION, context) {
        Ok(root) => root,
        Err(result) => return result,
    };
    let token = std::env::var("CENTRAL_NATIVE_TOKEN").ok();
    match order_world_chain(&root, input, token.as_deref(), now()) {
        Ok(data) => ActionResult::success(WORLD_CHAIN_ORDER_ACTION, data),
        Err(error) => failure(WORLD_CHAIN_ORDER_ACTION, error),
    }
}

pub fn register_world_chain_order_action(registry: &mut ActionRegistry) {
    let mut order = input("order", true);
    order.input_type = "array".to_owned();
    registry
        .register(
            descriptor(
                WORLD_CHAIN_ORDER_ACTION,
                "Order a world's chain",
                "Give a world's chain its order (DLW manual §16.2, §18.3): the packet and the agent head it and never move; order names scope and docs, each once, in the order they stand after the agent, or null for the manual's order (scope, then docs). A project's world keeps it in its ProjectCentral manifest as chain_order, on the manifest's expected_revision (the world map's manifest_revision), and the world map discloses it as world_chain_order; with no project, your own world keeps it on its root world record (control:root, central.world-relations/v1) on that record's revision, which the act advances, read back by central.world-relations.read; a root with no world record is refused, never declared. The person's own act: it authenticates on their native authority (CENTRAL_NATIVE_TOKEN). A stale revision answers conflict and writes nothing; the same order answers unchanged. Never invokes an Agent or model.",
                MutationClass::LocallyMutating,
                "central-world-chain-order",
                vec![
                    input("project", false),
                    order,
                    input("expected_revision", true),
                    input("expected_authority_revision", false),
                ],
            ),
            world_chain_order_action,
        )
        .expect("central.world.chain-order is a valid Action id");
}

fn world_place_action(
    _: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let root = match root_of(WORLD_PLACE_ACTION, context) {
        Ok(root) => root,
        Err(result) => return result,
    };
    let token = std::env::var("CENTRAL_NATIVE_TOKEN").ok();
    match place_world(&root, input, token.as_deref(), now()) {
        Ok(data) => ActionResult::success(WORLD_PLACE_ACTION, data),
        Err(error) => failure(WORLD_PLACE_ACTION, error),
    }
}

pub fn register_world_place_action(registry: &mut ActionRegistry) {
    let mut place = input("place", true);
    place.input_type = "object".to_owned();
    registry
        .register(
            descriptor(
                WORLD_PLACE_ACTION,
                "Declare a world's place",
                "Declare the real place on Earth where a world's hours are counted when the Sky's Observer is Shared (DLW manual §22.1): place is {reference, label, latitude_degrees -90..90, longitude_degrees -180..180, altitude_metres -500..10000, timezone (IANA)}, or null to take it away. A project's world keeps it in its ProjectCentral manifest as place, beside its name, on the manifest's expected_revision (the world map's manifest_revision), and the world map discloses it as world_place; with no project, your own world keeps it on its root world record (control:root, central.world-relations/v1) on that record's revision, which the act advances, read back by central.world-relations.read; a root with no world record is refused, never declared. The person's own act: it authenticates on their native authority (CENTRAL_NATIVE_TOKEN). A stale revision answers conflict and writes nothing; the same place answers unchanged. Never invokes an Agent or model.",
                MutationClass::LocallyMutating,
                "central-world-place",
                vec![
                    input("project", false),
                    place,
                    input("expected_revision", true),
                    input("expected_authority_revision", false),
                ],
            ),
            world_place_action,
        )
        .expect("central.world.place is a valid Action id");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hastings() -> WorldPlace {
        WorldPlace {
            reference: "geonames:2647356".into(),
            label: "Hastings".into(),
            latitude_degrees: 50.85,
            longitude_degrees: 0.57,
            altitude_metres: 10.0,
            timezone: "Europe/London".into(),
        }
    }

    #[test]
    fn a_place_the_sky_can_count_at_is_admitted_and_any_other_is_named() {
        assert_eq!(hastings().validate(), Ok(()));
        for (wrong, field) in [
            (
                WorldPlace {
                    latitude_degrees: f64::NAN,
                    ..hastings()
                },
                "latitude_degrees",
            ),
            (
                WorldPlace {
                    longitude_degrees: 180.5,
                    ..hastings()
                },
                "longitude_degrees",
            ),
            (
                WorldPlace {
                    altitude_metres: -501.0,
                    ..hastings()
                },
                "altitude_metres",
            ),
            (
                WorldPlace {
                    timezone: "Mars/Olympus".into(),
                    ..hastings()
                },
                "timezone",
            ),
            (
                WorldPlace {
                    label: " Hastings".into(),
                    ..hastings()
                },
                "label",
            ),
            (
                WorldPlace {
                    reference: "x".repeat(513),
                    ..hastings()
                },
                "reference",
            ),
        ] {
            let why = wrong.validate().unwrap_err();
            assert!(why.contains(field), "{why}");
        }
    }

    #[test]
    fn a_world_chain_order_names_scope_and_docs_once_each() {
        let order = |slots: &[&str]| slots.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        assert_eq!(chain_order_words(&order(&["docs", "scope"])), Ok(()));
        assert_eq!(chain_order_words(&order(&["scope", "docs"])), Ok(()));
        for (wrong, words) in [
            (
                order(&["agent", "scope", "docs"]),
                "agent heads a world's chain and never moves",
            ),
            (
                order(&["docs", "lens"]),
                "\"lens\" is no device of a world's chain",
            ),
            (order(&["docs"]), "names each of scope and docs once"),
            (
                order(&["docs", "docs"]),
                "names each of scope and docs once",
            ),
        ] {
            let why = chain_order_words(&wrong).unwrap_err();
            assert!(why.contains(words), "{why}");
        }
    }

    #[test]
    fn a_record_revision_advances_its_counter() {
        assert_eq!(next_record_revision("w1"), "w2");
        assert_eq!(next_record_revision("r9"), "r10");
        assert_eq!(next_record_revision("convention"), "convention.1");
        assert_eq!(next_record_revision("1"), "2");
    }

    /// be2i ruling 7: a place's words hold no invisible character.
    #[test]
    fn a_places_words_hold_no_invisible_character() {
        for label in ["Hast\u{202E}ings", "Hast\u{200B}ings"] {
            assert_eq!(
                words(label, "label", 80, true),
                Err("label must hold no control character and no invisible one".to_owned()),
                "{label:?}"
            );
        }
        assert_eq!(words("Hastings", "label", 80, true), Ok(()));
    }

    /// be2j item 15 (DLW build ruling 2026-11-10): a kept place's label or reference with nothing visible left is shown as an
    /// unnamed place and its reference's short form, never ""; one with words left is shown cleaned, and says so once.
    #[test]
    fn a_kept_place_with_nothing_visible_left_is_shown_as_an_unnamed_place_never_blank() {
        let blank = WorldPlace {
            label: "\u{3164}".into(),
            ..hastings()
        };
        assert_eq!(blank.validate_kept(), Ok(()), "it reads as it was kept");
        assert_eq!(blank.shown().label, "unnamed place dbwp");
        assert_eq!(blank.shown().reference, "geonames:2647356");
        let unreferenced = WorldPlace {
            reference: "\u{200B}".into(),
            ..hastings()
        };
        assert_eq!(unreferenced.shown().reference, "unnamed place aiq2");
        assert_eq!(unreferenced.shown().label, "Hastings");
        let hidden = WorldPlace {
            label: "Hast\u{200D}ings".into(),
            ..hastings()
        };
        assert_eq!(hidden.shown(), hastings());
        assert!(hidden.kept_note().unwrap().contains("central.world.place"));
        assert_eq!(hastings().kept_note(), None);
    }
}
