//! A world's Collections (O-I #749; DLW manual §10.3, "Down the browser's left side stand Live's three groups: Collections, the
//! coloured favourites you make; the four kinds above, as Live's categories; and Places, where your ground lies"). Live's seven
//! coloured collections, Favourites first, each holding the things you put in it: a world, a project, an agent, a machine, a Day,
//! a page, a set — each by the ref its own owner gives it, with the words it stands under in the browser.
//!
//! Kept where the world's relations are. Your own world (`control:root`) keeps its collections in
//! `Control/relations/collections.json`, a project's world (`project:<Name>`, the Work folder) in its
//! `ProjectCentral/relations/collections.json`. A world that has put nothing in a collection keeps no file, and its reading is the
//! seven collections empty. Five acts: `central.collections.read`; `central.collections.add`, which puts one thing in one collection
//! (the same thing put in again answers `unchanged`, never a second row); `central.collections.remove`, which takes one out (a thing
//! that is not there answers `unchanged`); and Live's two acts on a collection itself: `central.collections.rename` gives it the name you
//! choose (Live's Rename, ⌘R: its own name, or none, gives back Live's), and `central.collections.show` keeps whether the browser's
//! column shows it (Live's Edit, a box ticked or not for each collection). A collection whose showing you never chose shows as the
//! browser shows it unchosen. Each write is made under the world's source lock and replaces the record atomically. A record that holds
//! no name and no choice is written as it was before names and choices were kept. Never invokes an Agent or model.
use crate::action::{ActionExecutionContext, ActionRegistry, MutationClass};
use crate::civil_time_adoption::{descriptor, failure, input, now, root_of};
use crate::continuous_work::source::Scope;
use crate::file_mutation::{atomic_record, RecordDisposition};
use crate::place_passage::ensure_folders;
use crate::result::ActionResult;
use crate::source_safety::content_revision_bytes;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

pub const COLLECTIONS_READ_ACTION: &str = "central.collections.read";
pub const COLLECTIONS_ADD_ACTION: &str = "central.collections.add";
pub const COLLECTIONS_REMOVE_ACTION: &str = "central.collections.remove";
pub const COLLECTIONS_RENAME_ACTION: &str = "central.collections.rename";
pub const COLLECTIONS_SHOW_ACTION: &str = "central.collections.show";
pub const COLLECTIONS_SCHEMA: &str = "central.collections/v1";
pub const COLLECTIONS_RECEIPT_SCHEMA: &str = "central.collections-receipt/v1";
/// Where a world keeps its collections, under the world's own ground (`Control` or `ProjectCentral`).
pub const COLLECTIONS_FILE: &str = "relations/collections.json";
const YOUR_WORLD: &str = "control:root";
const REF_MAX_BYTES: usize = 1024;
const LABEL_MAX_CHARS: usize = 240;
const KIND_MAX_CHARS: usize = 64;
/// How long a collection's name may be.
const NAME_MAX_CHARS: usize = 64;
/// How many things one collection holds.
pub const COLLECTION_MAX_ITEMS: usize = 1000;
const RECORD_LIMIT: u64 = 4 * 1024 * 1024;

/// Live's seven collections, in Live's order: (id, name, colour).
pub const PALETTE: [(&str, &str, &str); 7] = [
    ("favourites", "Favourites", "red"),
    ("orange", "Orange", "orange"),
    ("yellow", "Yellow", "yellow"),
    ("green", "Green", "green"),
    ("blue", "Blue", "blue"),
    ("purple", "Purple", "purple"),
    ("grey", "Grey", "grey"),
];

/// One thing in a collection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CollectedItem {
    /// The thing's ref, as its owner gives it.
    #[serde(rename = "ref")]
    pub reference: String,
    /// The words it stands under in the browser.
    pub label: String,
    /// What sort of thing it is (world, project, agent, machine, day, page, set …), when the browser names it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    pub added_at_unix_seconds: u64,
}

/// `central.collections/v1` as kept: each collection that holds something, by its id; the names you gave collections; and whether you
/// chose to show or hide each in the browser's column.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CollectionsRecord {
    pub schema: String,
    pub world: String,
    pub collections: BTreeMap<String, Vec<CollectedItem>>,
    /// The name you gave a collection, by its id (Live's Rename); a collection not here keeps Live's name.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub names: BTreeMap<String, String>,
    /// Whether you chose to show a collection in the browser's column, by its id (Live's Edit); a collection not here shows as the
    /// browser shows it unchosen.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub shown: BTreeMap<String, bool>,
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message.into())
}

/// The world a collection belongs to: your own, or a project's by its Work folder.
#[derive(Debug, Clone, PartialEq, Eq)]
enum World {
    Yours,
    Project(String),
}

impl World {
    fn parse(input: &Value, action: &str) -> io::Result<Self> {
        let raw = input.get("world").and_then(Value::as_str).ok_or_else(|| {
            invalid(format!(
                "{action} names the world: {YOUR_WORLD} or project:<Name>"
            ))
        })?;
        if raw == YOUR_WORLD {
            return Ok(Self::Yours);
        }
        match raw.strip_prefix("project:") {
            Some(name) if !name.is_empty() && !name.contains(['/', '\\', ':']) && name != "." && name != ".." => {
                Ok(Self::Project(name.to_owned()))
            }
            _ => Err(invalid(format!(
                "{action} keeps collections for a world of yours: {YOUR_WORLD} or project:<Name>, not {raw:?}"
            ))),
        }
    }
    fn world_ref(&self) -> String {
        match self {
            Self::Yours => YOUR_WORLD.to_owned(),
            Self::Project(name) => format!("project:{name}"),
        }
    }
}

/// Where the world's collections are kept: its owner root, the record's path under it, and the record's Central path.
struct Kept {
    owner_root: PathBuf,
    relative: PathBuf,
    central_path: String,
}

fn kept_at(central: &Path, world: &World) -> io::Result<Kept> {
    let (scope, prefix) = match world {
        World::Yours => (Scope::resolve(central, None)?, String::new()),
        World::Project(name) => {
            if !central
                .join("Work")
                .join(name)
                .join("ProjectCentral")
                .is_dir()
            {
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("{name} is no project of your world (it has no Work/{name}/ProjectCentral): it keeps no collections."),
                ));
            }
            (
                Scope::resolve(central, Some(name))?,
                format!("Work/{name}/"),
            )
        }
    };
    let relative = PathBuf::from(&scope.prefix).join(COLLECTIONS_FILE);
    let central_path = format!("{prefix}{}", relative.to_string_lossy());
    Ok(Kept {
        owner_root: scope.root,
        relative,
        central_path,
    })
}

/// The kept record and its bytes' revision, or none while the world keeps no file.
fn read_kept(kept: &Kept, world: &str) -> io::Result<Option<(CollectionsRecord, String)>> {
    let file = kept.owner_root.join(&kept.relative);
    match std::fs::symlink_metadata(&file) {
        Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => {
            if metadata.len() > RECORD_LIMIT {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("{} is larger than a collections record", kept.central_path),
                ));
            }
            let bytes = std::fs::read(&file)?;
            let record: CollectionsRecord = serde_json::from_slice(&bytes).map_err(|error| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "The collections at {} do not read: {error}",
                        kept.central_path
                    ),
                )
            })?;
            if record.schema != COLLECTIONS_SCHEMA || record.world != world {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "The record at {} is not {world}'s {COLLECTIONS_SCHEMA}",
                        kept.central_path
                    ),
                ));
            }
            if let Some(unknown) = record
                .collections
                .keys()
                .chain(record.names.keys())
                .chain(record.shown.keys())
                .find(|id| collection_of(id).is_none())
            {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "The record at {} keeps {unknown:?}, which is no collection",
                        kept.central_path
                    ),
                ));
            }
            if let Some((id, _)) = record
                .names
                .iter()
                .find(|(_, name)| name_of(name).ok().flatten().as_deref() != Some(name.as_str()))
            {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "The record at {} names {id} in words that are no collection's name",
                        kept.central_path
                    ),
                ));
            }
            Ok(Some((record, content_revision_bytes(&bytes))))
        }
        Ok(_) => Err(invalid(format!(
            "{} is not a plain file",
            kept.central_path
        ))),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

fn collection_of(id: &str) -> Option<(&'static str, &'static str, &'static str)> {
    PALETTE
        .iter()
        .copied()
        .find(|(palette_id, _, _)| *palette_id == id)
}

/// The reading: Live's seven collections in order, each with its name, what it holds, and whether you chose to show it (null: unchosen).
fn reading(world: &str, kept: &Kept, record: Option<&(CollectionsRecord, String)>) -> Value {
    let collections: Vec<Value> = PALETTE
        .iter()
        .map(|(id, name, colour)| {
            let record = record.map(|(record, _)| record);
            let items = record
                .and_then(|record| record.collections.get(*id))
                .cloned()
                .unwrap_or_default();
            let name = record
                .and_then(|record| record.names.get(*id))
                .map_or(*name, String::as_str);
            let shown = record.and_then(|record| record.shown.get(*id)).copied();
            json!({"id": id, "name": name, "colour": colour, "items": items, "shown": shown})
        })
        .collect();
    json!({"schema": COLLECTIONS_SCHEMA, "world": world, "kept": record.is_some(),
        "revision": record.map(|(_, revision)| revision.clone()), "source_path": kept.central_path,
        "collections": collections})
}

fn collection_input(input: &Value, action: &str) -> io::Result<&'static str> {
    let raw = input
        .get("collection")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid(format!("{action} names the collection")))?;
    collection_of(raw).map(|(id, _, _)| id).ok_or_else(|| {
        let names: Vec<&str> = PALETTE.iter().map(|(id, _, _)| *id).collect();
        invalid(format!(
            "{raw:?} is no collection: they are {}",
            names.join(", ")
        ))
    })
}

fn reference_of(raw: Option<&Value>, action: &str) -> io::Result<String> {
    let reference = raw
        .and_then(Value::as_str)
        .ok_or_else(|| invalid(format!("{action} names the thing by its ref")))?;
    if reference.trim().is_empty()
        || reference != reference.trim()
        || reference.len() > REF_MAX_BYTES
        || reference.chars().any(char::is_control)
    {
        return Err(invalid(format!(
            "A thing's ref is 1 to {REF_MAX_BYTES} bytes of one line, with nothing around it"
        )));
    }
    Ok(reference.to_owned())
}

fn item_of(input: &Value, now: u64) -> io::Result<CollectedItem> {
    let item = input
        .get("item")
        .filter(|item| item.is_object())
        .ok_or_else(|| {
            invalid(format!(
                "{COLLECTIONS_ADD_ACTION} names the thing as item {{ref, label, kind?}}"
            ))
        })?;
    let reference = reference_of(item.get("ref"), COLLECTIONS_ADD_ACTION)?;
    let label = item
        .get("label")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default();
    if label.is_empty()
        || label.chars().count() > LABEL_MAX_CHARS
        || label.chars().any(char::is_control)
    {
        return Err(invalid(format!(
            "A thing's words are 1 to {LABEL_MAX_CHARS} characters of one line"
        )));
    }
    let kind = match item.get("kind") {
        None | Some(Value::Null) => None,
        Some(Value::String(kind))
            if !kind.is_empty()
                && kind.chars().count() <= KIND_MAX_CHARS
                && kind
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') =>
        {
            Some(kind.clone())
        }
        Some(_) => {
            return Err(invalid(format!(
                "A thing's kind is a word of lowercase letters, digits and hyphens, at most {KIND_MAX_CHARS}"
            )))
        }
    };
    Ok(CollectedItem {
        reference,
        label: label.to_owned(),
        kind,
        added_at_unix_seconds: now,
    })
}

/// A collection's name as you give it: trimmed, at most one line of NAME_MAX_CHARS characters. None: no name, so Live's.
fn name_of(raw: &str) -> io::Result<Option<String>> {
    let name = raw.trim();
    if name.chars().count() > NAME_MAX_CHARS || name.chars().any(char::is_control) {
        return Err(invalid(format!(
            "A collection's name is one line of at most {NAME_MAX_CHARS} characters"
        )));
    }
    Ok((!name.is_empty()).then(|| name.to_owned()))
}

/// Read a world's collections in the Central root `central`.
pub fn read_collections(central: &Path, input: &Value) -> io::Result<Value> {
    let world = World::parse(input, COLLECTIONS_READ_ACTION)?;
    let kept = kept_at(central, &world)?;
    let world = world.world_ref();
    let record = read_kept(&kept, &world)?;
    Ok(reading(&world, &kept, record.as_ref()))
}

enum Change {
    Add(&'static str, CollectedItem),
    Remove(&'static str, String),
    /// A collection's new name; None gives back Live's.
    Rename(&'static str, Option<String>),
    Show(&'static str, bool),
}

fn change_collections(central: &Path, world: World, change: Change) -> io::Result<Value> {
    let kept = kept_at(central, &world)?;
    let world = world.world_ref();
    let _lock = crate::source_safety::lock(&kept.owner_root, "source-mutation.lock")?;
    let standing = read_kept(&kept, &world)?;
    let mut record = standing
        .as_ref()
        .map(|(record, _)| record.clone())
        .unwrap_or_else(|| CollectionsRecord {
            schema: COLLECTIONS_SCHEMA.to_owned(),
            world: world.clone(),
            collections: BTreeMap::new(),
            names: BTreeMap::new(),
            shown: BTreeMap::new(),
        });
    let (collection, detail, outcome) = match change {
        Change::Add(collection, item) => {
            let items = record.collections.entry(collection.to_owned()).or_default();
            let detail = ("ref", json!(item.reference));
            if items.iter().any(|held| held.reference == item.reference) {
                (collection, detail, "unchanged")
            } else if items.len() >= COLLECTION_MAX_ITEMS {
                return Err(invalid(format!(
                    "{collection} holds {COLLECTION_MAX_ITEMS} things already: take one out first"
                )));
            } else {
                items.push(item);
                (collection, detail, "added")
            }
        }
        Change::Remove(collection, reference) => {
            let items = record.collections.entry(collection.to_owned()).or_default();
            let before = items.len();
            items.retain(|held| held.reference != reference);
            let outcome = if items.len() == before {
                "unchanged"
            } else {
                "removed"
            };
            (collection, ("ref", json!(reference)), outcome)
        }
        Change::Rename(collection, name) => {
            let live = collection_of(collection).map_or(collection, |(_, name, _)| name);
            // Live's own name is no name of yours: the record keeps none for it.
            let name = name.filter(|name| name != live);
            let before = record.names.get(collection).cloned();
            match &name {
                Some(name) => record.names.insert(collection.to_owned(), name.clone()),
                None => record.names.remove(collection),
            };
            let outcome = if before == name {
                "unchanged"
            } else {
                "renamed"
            };
            (
                collection,
                ("name", json!(name.as_deref().unwrap_or(live))),
                outcome,
            )
        }
        Change::Show(collection, shown) => {
            let before = record.shown.insert(collection.to_owned(), shown);
            let outcome = match (before == Some(shown), shown) {
                (true, _) => "unchanged",
                (false, true) => "shown",
                (false, false) => "hidden",
            };
            (collection, ("shown", json!(shown)), outcome)
        }
    };
    record.collections.retain(|_, items| !items.is_empty());
    let after = if outcome == "unchanged" {
        standing
    } else {
        if let Some(parent) = kept.relative.parent() {
            ensure_folders(&kept.owner_root, parent)?;
        }
        let mut bytes = serde_json::to_vec_pretty(&record).map_err(io::Error::other)?;
        bytes.push(b'\n');
        atomic_record(
            &kept.owner_root,
            &kept.relative,
            &bytes,
            RecordDisposition::ReplaceOrCreate,
        )?;
        Some((record, content_revision_bytes(&bytes)))
    };
    let mut receipt = json!({"schema": COLLECTIONS_RECEIPT_SCHEMA, "outcome": outcome, "world": world, "collection": collection,
        "source_path": kept.central_path, "reading": reading(&world, &kept, after.as_ref())});
    receipt[detail.0] = detail.1;
    Ok(receipt)
}

/// Put one thing in one of a world's collections.
pub fn add_to_collection(central: &Path, input: &Value, now: u64) -> io::Result<Value> {
    let world = World::parse(input, COLLECTIONS_ADD_ACTION)?;
    let collection = collection_input(input, COLLECTIONS_ADD_ACTION)?;
    let item = item_of(input, now)?;
    change_collections(central, world, Change::Add(collection, item))
}

/// Take one thing out of one of a world's collections.
pub fn remove_from_collection(central: &Path, input: &Value) -> io::Result<Value> {
    let world = World::parse(input, COLLECTIONS_REMOVE_ACTION)?;
    let collection = collection_input(input, COLLECTIONS_REMOVE_ACTION)?;
    let reference = reference_of(input.get("ref"), COLLECTIONS_REMOVE_ACTION)?;
    change_collections(central, world, Change::Remove(collection, reference))
}

/// Give one of a world's collections the name you choose; Live's own name, or none, gives back Live's.
pub fn rename_collection(central: &Path, input: &Value) -> io::Result<Value> {
    let world = World::parse(input, COLLECTIONS_RENAME_ACTION)?;
    let collection = collection_input(input, COLLECTIONS_RENAME_ACTION)?;
    let raw = input.get("name").and_then(Value::as_str).ok_or_else(|| {
        invalid(format!(
            "{COLLECTIONS_RENAME_ACTION} names the collection's new name (empty gives back Live's)"
        ))
    })?;
    change_collections(central, world, Change::Rename(collection, name_of(raw)?))
}

/// Keep whether the browser's column shows one of a world's collections.
pub fn show_collection(central: &Path, input: &Value) -> io::Result<Value> {
    let world = World::parse(input, COLLECTIONS_SHOW_ACTION)?;
    let collection = collection_input(input, COLLECTIONS_SHOW_ACTION)?;
    let shown = input.get("shown").and_then(Value::as_bool).ok_or_else(|| {
        invalid(format!(
            "{COLLECTIONS_SHOW_ACTION} says whether the collection shows: shown true or false"
        ))
    })?;
    change_collections(central, world, Change::Show(collection, shown))
}

fn run(
    action: &'static str,
    context: &ActionExecutionContext<'_>,
    act: impl FnOnce(&Path) -> io::Result<Value>,
) -> ActionResult {
    let root = match root_of(action, context) {
        Ok(root) => root,
        Err(result) => return result,
    };
    match act(&root) {
        Ok(data) => ActionResult::success(action, data),
        Err(error) => failure(action, error),
    }
}

fn read_action(
    _: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    run(COLLECTIONS_READ_ACTION, context, |root| {
        read_collections(root, input)
    })
}

fn add_action(
    _: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    run(COLLECTIONS_ADD_ACTION, context, |root| {
        add_to_collection(root, input, now())
    })
}

fn remove_action(
    _: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    run(COLLECTIONS_REMOVE_ACTION, context, |root| {
        remove_from_collection(root, input)
    })
}

fn rename_action(
    _: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    run(COLLECTIONS_RENAME_ACTION, context, |root| {
        rename_collection(root, input)
    })
}

fn show_action(
    _: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    run(COLLECTIONS_SHOW_ACTION, context, |root| {
        show_collection(root, input)
    })
}

pub fn register_collections_actions(registry: &mut ActionRegistry) {
    let mut item = input("item", true);
    item.input_type = "object".to_owned();
    let mut shown = input("shown", true);
    shown.input_type = "boolean".to_owned();
    for (id, title, description, class, inputs, handler) in [
        (
            COLLECTIONS_READ_ACTION,
            "Read a world's collections",
            "A world's Collections (DLW manual §10.3, the browser's coloured favourites): Live's seven collections in order (favourites, orange, yellow, green, blue, purple, grey), each with its name (the one you gave it, else Live's), its colour, the things in it ({ref, label, kind?, added_at_unix_seconds}) and shown (whether you chose to show it in the browser's column; null while unchosen), as central.collections/v1, with the record's revision and where it is kept (Control/relations/collections.json for control:root, the project's ProjectCentral/relations/collections.json for project:<Name>). A world that keeps no file reads as the seven empty.",
            MutationClass::ReadOnly,
            vec![input("world", true)],
            read_action as fn(&ActionRegistry, &Value, &ActionExecutionContext<'_>) -> ActionResult,
        ),
        (
            COLLECTIONS_ADD_ACTION,
            "Put a thing in a collection",
            "Put one thing in one of a world's collections: world is control:root or project:<Name>; collection is favourites, orange, yellow, green, blue, purple or grey; item is {ref (the thing's ref as its owner gives it), label (the words it stands under), kind?}. The same ref put in the same collection again answers unchanged. Answers central.collections-receipt/v1 with the reading after.",
            MutationClass::LocallyMutating,
            vec![input("world", true), input("collection", true), item.clone()],
            add_action,
        ),
        (
            COLLECTIONS_REMOVE_ACTION,
            "Take a thing out of a collection",
            "Take one thing, by its ref, out of one of a world's collections: world is control:root or project:<Name>; collection is one of the seven. A ref the collection does not hold answers unchanged. Answers central.collections-receipt/v1 with the reading after.",
            MutationClass::LocallyMutating,
            vec![input("world", true), input("collection", true), input("ref", true)],
            remove_action,
        ),
        (
            COLLECTIONS_RENAME_ACTION,
            "Rename a collection",
            "Give one of a world's collections the name you choose (Live's Rename): world is control:root or project:<Name>; collection is one of the seven; name is one line of at most 64 characters, trimmed. Live's own name for it, or an empty name, gives back Live's. What it holds is untouched. The same name again answers unchanged. Answers central.collections-receipt/v1 with the name and the reading after.",
            MutationClass::LocallyMutating,
            vec![input("world", true), input("collection", true), input("name", true)],
            rename_action,
        ),
        (
            COLLECTIONS_SHOW_ACTION,
            "Show or hide a collection",
            "Keep whether the browser's column shows one of a world's collections (Live's Edit, a box ticked or not): world is control:root or project:<Name>; collection is one of the seven; shown is true or false. What it holds is untouched. The same choice again answers unchanged; otherwise shown or hidden. Answers central.collections-receipt/v1 with the reading after.",
            MutationClass::LocallyMutating,
            vec![input("world", true), input("collection", true), shown],
            show_action,
        ),
    ] {
        registry
            .register(
                descriptor(id, title, description, class, "central-collections", inputs),
                handler,
            )
            .expect("a collections Action id is valid");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ground(test: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "central-collections-unit-{}-{test}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("Control")).unwrap();
        std::fs::create_dir_all(root.join("Work")).unwrap();
        root.canonicalize().unwrap()
    }

    #[test]
    fn a_thing_is_held_once_and_taken_out_and_an_emptied_collection_is_not_kept() {
        let root = ground("once");
        let input = json!({"world": "control:root", "collection": "purple",
            "item": {"ref": "workcell:mac", "label": "This Mac", "kind": "machine"}});
        let added = add_to_collection(&root, &input, 10).unwrap();
        assert_eq!(added["outcome"], "added");
        let again = add_to_collection(&root, &input, 11).unwrap();
        assert_eq!(again["outcome"], "unchanged");
        assert_eq!(again["reading"]["revision"], added["reading"]["revision"]);
        let read = read_collections(&root, &json!({"world": "control:root"})).unwrap();
        assert_eq!(read["collections"][5]["id"], "purple");
        assert_eq!(
            read["collections"][5]["items"][0]["added_at_unix_seconds"],
            10
        );
        let removed = remove_from_collection(
            &root,
            &json!({"world": "control:root", "collection": "purple", "ref": "workcell:mac"}),
        )
        .unwrap();
        assert_eq!(removed["outcome"], "removed");
        let kept: CollectionsRecord = serde_json::from_slice(
            &std::fs::read(root.join("Control").join(COLLECTIONS_FILE)).unwrap(),
        )
        .unwrap();
        assert!(kept.collections.is_empty(), "{kept:?}");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_record_that_is_not_the_world_s_collections_is_refused_not_overwritten() {
        let root = ground("foreign");
        std::fs::create_dir_all(root.join("Control/relations")).unwrap();
        let foreign = root.join("Control").join(COLLECTIONS_FILE);
        std::fs::write(&foreign, b"{\"schema\":\"other/v1\"}").unwrap();
        let input = json!({"world": "control:root", "collection": "favourites",
            "item": {"ref": "x", "label": "x"}});
        assert!(add_to_collection(&root, &input, 1).is_err());
        assert_eq!(
            std::fs::read(&foreign).unwrap(),
            b"{\"schema\":\"other/v1\"}"
        );
        let _ = std::fs::remove_dir_all(&root);
    }
}
