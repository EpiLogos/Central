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
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::{self, Read};
use std::path::Path;

pub const WORLD_PLACE_ACTION: &str = "central.world.place";
pub const WORLD_PLACE_SCHEMA: &str = "central.world-place/v1";
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

fn words(value: &str, field: &str, most: usize) -> Result<(), String> {
    let count = value.chars().count();
    if value.trim() != value || count == 0 || count > most {
        return Err(format!(
            "{field} must be 1 to {most} characters without surrounding whitespace"
        ));
    }
    if value.chars().any(char::is_control) {
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
    /// Whether this is a place the Sky can count a day at: answers why not, in words.
    pub fn validate(&self) -> Result<(), String> {
        words(&self.reference, "reference", PLACE_REFERENCE_MAX_CHARS)?;
        words(&self.label, "label", PLACE_LABEL_MAX_CHARS)?;
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
            "place":self.place,"previous_place":previous,"previous_revision":previous_revision,"revision":revision,
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
    match project {
        Some(project) => place_project_world(
            &scope.root,
            project,
            place.as_ref(),
            expected,
            &principal.principal_ref,
        ),
        None => place_your_world(central, place.as_ref(), expected, &principal.principal_ref),
    }
}

fn place_project_world(
    project_root: &Path,
    project: &str,
    place: Option<&WorldPlace>,
    expected: &str,
    principal_ref: &str,
) -> io::Result<Value> {
    let mut held = Vec::new();
    open_native_file(project_root, MANIFEST)?
        .take(MANIFEST_LIMIT + 1)
        .read_to_end(&mut held)?;
    if held.len() as u64 > MANIFEST_LIMIT {
        return Err(invalid("The project's manifest exceeds its bounded size"));
    }
    let revision = content_revision_bytes(&held);
    let world_ref = format!("project:{project}");
    let receipt = Receipt {
        world_ref: &world_ref,
        project: Some(project),
        source: MANIFEST,
        place,
        principal_ref,
    };
    if revision != expected {
        let mut conflict = receipt.answer("conflict", None, &revision, &revision);
        conflict["expected_revision"] = json!(expected);
        return Ok(conflict);
    }
    let path = project_root.join(MANIFEST);
    let manifest = parse_project_manifest(&held, &path)?;
    let previous = manifest.place.clone();
    if previous.as_ref() == place {
        return Ok(receipt.answer("unchanged", previous.as_ref(), &revision, &revision));
    }
    // Only the place changes: every other field stays as the manifest keeps it, written as ProjectCentral writes its manifest.
    let mut document: Value = serde_json::from_slice(&held)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    let fields = document
        .as_object_mut()
        .ok_or_else(|| invalid("The project's manifest is not an object"))?;
    match place {
        Some(place) => {
            fields.insert("place".into(), json!(place));
        }
        None => {
            fields.remove("place");
        }
    }
    let mut written = serde_json::to_vec_pretty(&document)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    written.push(b'\n');
    let kept = parse_project_manifest(&written, &path)?;
    if kept.project_id != manifest.project_id || !kept.validate().valid {
        return Err(invalid(
            "The placed manifest would not validate; nothing was written",
        ));
    }
    atomic_record(
        project_root,
        Path::new(MANIFEST),
        &written,
        RecordDisposition::ReplaceOrCreate,
    )?;
    Ok(receipt.answer(
        outcome(place),
        previous.as_ref(),
        &revision,
        &content_revision_bytes(&written),
    ))
}

fn store_error(error: RelationRecordStoreError) -> io::Error {
    match error {
        RelationRecordStoreError::NotFound(_) => io::Error::new(
            io::ErrorKind::NotFound,
            format!(
                "Your world declares no world record ({ROOT_WORLD_DIR}, {CONTROL_WORLD_REF}): Central keeps your world's place on it, and does not declare the world for a place. Declare it first (central.world-relations.save)."
            ),
        ),
        RelationRecordStoreError::InvalidRecord(why) => invalid(why),
        other => io::Error::other(other.to_string()),
    }
}

fn place_your_world(
    central: &Path,
    place: Option<&WorldPlace>,
    expected: &str,
    principal_ref: &str,
) -> io::Result<Value> {
    let store = RelationRecordStore::worlds_at_root(central);
    let reading = store.read(CONTROL_WORLD_REF).map_err(store_error)?;
    let source = reading.source_path.clone();
    let receipt = Receipt {
        world_ref: CONTROL_WORLD_REF,
        project: None,
        source: &source,
        place,
        principal_ref,
    };
    let revision = reading.revision.clone();
    if revision != expected {
        let mut conflict = receipt.answer("conflict", None, &revision, &revision);
        conflict["expected_revision"] = json!(expected);
        return Ok(conflict);
    }
    let previous: Option<WorldPlace> = match reading.record.get("place") {
        None | Some(Value::Null) => None,
        Some(kept) => Some(serde_json::from_value(kept.clone()).map_err(|error| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Your world record's place cannot be read: {error}"),
            )
        })?),
    };
    if previous.as_ref() == place {
        return Ok(receipt.answer("unchanged", previous.as_ref(), &revision, &revision));
    }
    // Only the place and the revision change: every other field stays as the record keeps it.
    let mut record = reading.record.clone();
    let fields = record
        .as_object_mut()
        .ok_or_else(|| invalid("Your world record is not an object"))?;
    match place {
        Some(place) => {
            fields.insert("place".into(), json!(place));
        }
        None => {
            fields.remove("place");
        }
    }
    let next = next_record_revision(&revision);
    fields.insert("revision".into(), json!(next));
    store.save(&record, Some(&revision)).map_err(store_error)?;
    Ok(receipt.answer(outcome(place), previous.as_ref(), &revision, &next))
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
    fn a_record_revision_advances_its_counter() {
        assert_eq!(next_record_revision("w1"), "w2");
        assert_eq!(next_record_revision("r9"), "r10");
        assert_eq!(next_record_revision("convention"), "convention.1");
        assert_eq!(next_record_revision("1"), "2");
    }
}
