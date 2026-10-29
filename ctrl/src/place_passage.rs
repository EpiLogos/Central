//! `central.place.file-passage`: a passage filed on a place of your world (O-I #820; DLW manual §18.4 What a tag sends, "Place —
//! Files the passage on that spot of your world, with a link back to the page"; §5, "Tag a place and the passage is filed on that
//! spot of your world"). A place the search line offers is a world of yours: your own (`control:root`) or a project's
//! (`project:<Name>`, AIKit's project resource).
//!
//! The passage lands in that world's ground word for word, with where it came from in words and the link back to the page, as a
//! generated proposal that stays unrecognised until the person recognises it: the standing Remember gives a passage
//! (remember.rs), so filing never writes human ground on its own. Your world keeps it under `Control/agents/places/filed`, a
//! project's under its `ProjectCentral/agents/places/filed`. Create-only: the same passage from the same page filed on the same
//! place is the same record, answered `unchanged`, never a second copy. Never invokes an Agent or model.
use crate::action::{ActionExecutionContext, ActionRegistry, MutationClass};
use crate::civil_time_adoption::{descriptor, failure, input, now, root_of};
use crate::continuous_work::source::Scope;
use crate::file_mutation::{atomic_record, RecordDisposition};
use crate::files::CentralPathRef;
use crate::result::ActionResult;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::io;
use std::path::{Path, PathBuf};

pub const FILE_PASSAGE_ACTION: &str = "central.place.file-passage";
pub const FILED_PASSAGE_SCHEMA: &str = "central.filed-passage/v1";
pub const FILED_PASSAGE_RECEIPT_SCHEMA: &str = "central.filed-passage-receipt/v1";
/// Where your own world keeps a passage filed on it.
pub const ROOT_FILED_DIR: &str = "Control/agents/places/filed";
/// Where a project's world keeps one, under the project.
pub const PROJECT_FILED_DIR: &str = "ProjectCentral/agents/places/filed";
/// A passage, not a document.
pub const PASSAGE_MAX_CHARS: usize = 8_000;
const TITLE_MAX_CHARS: usize = 240;
const PATH_MAX_BYTES: usize = 1024;
const YOUR_WORLD: &str = "control:root";

/// Where a filed passage came from: the page in words, and the link back when it is a page of a world.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FiledSource {
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_ref: Option<String>,
}

/// The standing a filed passage is kept with: generated, unrecognised until the person recognises it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FiledProvenance {
    pub origin_action: String,
    pub recorded_at_unix_seconds: u64,
    pub authorship: String,
    pub recognition: String,
}

/// `central.filed-passage/v1`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FiledPassage {
    pub schema: String,
    pub passage_ref: String,
    /// The world the place is: `control:root`, or `project:<Name>`.
    pub place: String,
    /// The passage exactly as it was sent.
    pub passage: String,
    pub source: FiledSource,
    pub provenance: FiledProvenance,
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message.into())
}

/// The place's world: your own, or a project's by its name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Place {
    YourWorld,
    Project(String),
}

impl Place {
    pub fn parse(raw: &str) -> io::Result<Self> {
        if raw == YOUR_WORLD {
            return Ok(Self::YourWorld);
        }
        match raw.strip_prefix("project:") {
            Some(name) if !name.is_empty() && !name.contains(['/', '\\', ':']) && name != "." && name != ".." => {
                Ok(Self::Project(name.to_owned()))
            }
            _ => Err(invalid(format!(
                "{FILE_PASSAGE_ACTION} files a passage on a place of your world: {YOUR_WORLD} or project:<Name>, not {raw:?}"
            ))),
        }
    }
    pub fn world_ref(&self) -> String {
        match self {
            Self::YourWorld => YOUR_WORLD.to_owned(),
            Self::Project(name) => format!("project:{name}"),
        }
    }
}

/// The passage as sent: words, none around them, no NUL, a passage's length. Kept verbatim or refused, never rewritten.
fn passage_of(input: &Value) -> io::Result<String> {
    let passage = input
        .get("passage")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid(format!("{FILE_PASSAGE_ACTION} requires passage as text")))?;
    if passage.trim().is_empty() {
        return Err(invalid(
            "An empty passage is not filed: select the words first.",
        ));
    }
    if passage != passage.trim() || passage.contains('\0') {
        return Err(invalid(
            "A passage is filed word for word or refused: no surrounding whitespace, no NUL.",
        ));
    }
    if passage.chars().count() > PASSAGE_MAX_CHARS {
        return Err(invalid(format!(
            "A passage is at most {PASSAGE_MAX_CHARS} characters: this one is a document."
        )));
    }
    Ok(passage.to_owned())
}

/// Where it came from: its title in one line of words, the page it came from by its Central path, the source's ref.
fn source_of(input: &Value) -> io::Result<FiledSource> {
    let source: FiledSource = serde_json::from_value(input.get("source").cloned().unwrap_or(Value::Null))
        .map_err(|error| invalid(format!("{FILE_PASSAGE_ACTION} names where the passage came from as source {{title, path?, source_ref?}}: {error}")))?;
    let title = source.title.trim();
    if title.is_empty()
        || title.chars().count() > TITLE_MAX_CHARS
        || title.chars().any(char::is_control)
    {
        return Err(invalid(format!(
            "Where a passage came from is 1 to {TITLE_MAX_CHARS} characters of one line"
        )));
    }
    if let Some(path) = &source.path {
        let relative = Path::new(path);
        if path.is_empty()
            || path.len() > PATH_MAX_BYTES
            || path.contains(':')
            || path.chars().any(|c| c.is_control() || c == '\\')
            || relative.is_absolute()
            || !relative
                .components()
                .all(|part| matches!(part, std::path::Component::Normal(_)))
        {
            return Err(invalid(
                "The link back is the page's Central path (Control/… or Work/…), relative and plain",
            ));
        }
    }
    if let Some(reference) = &source.source_ref {
        if reference.trim().is_empty()
            || reference.len() > PATH_MAX_BYTES
            || reference.contains('\0')
        {
            return Err(invalid("The source's ref is a bounded reference"));
        }
    }
    Ok(FiledSource {
        title: title.to_owned(),
        ..source
    })
}

/// The record's identity: the place, the passage and the page it came from. The same three are the same record.
fn key_of(place: &str, passage: &str, source: &FiledSource) -> String {
    let mut digest = Sha256::new();
    for part in [
        place,
        passage,
        source.path.as_deref().unwrap_or(""),
        source.source_ref.as_deref().unwrap_or(""),
    ] {
        digest.update((part.len() as u64).to_le_bytes());
        digest.update(part.as_bytes());
    }
    digest
        .finalize()
        .iter()
        .take(12)
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Make each folder of `relative` under `owner_root` that is absent; a folder that is a link or a file is refused.
fn ensure_folders(owner_root: &Path, relative: &Path) -> io::Result<()> {
    let mut current = owner_root.to_path_buf();
    for part in relative.components() {
        let std::path::Component::Normal(part) = part else {
            return Err(invalid("A filed passage's folder is a plain relative path"));
        };
        current.push(part);
        match std::fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
                return Err(invalid(format!(
                    "{} is not a plain folder: nothing was filed",
                    current.display()
                )))
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => std::fs::create_dir(&current)?,
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

/// File the passage `input` names on its place, in the Central root `central`. Answers the receipt: filed, or unchanged when the same
/// record stands already.
pub fn file_passage(central: &Path, input: &Value, now: u64) -> io::Result<Value> {
    let place = Place::parse(input.get("place").and_then(Value::as_str).ok_or_else(|| {
        invalid(format!(
            "{FILE_PASSAGE_ACTION} names the place: {YOUR_WORLD} or project:<Name>"
        ))
    })?)?;
    let passage = passage_of(input)?;
    let source = source_of(input)?;
    let (owner_root, folder, prefix): (PathBuf, &str, String) = match &place {
        Place::YourWorld => {
            let scope = Scope::resolve(central, None)?;
            (scope.root, ROOT_FILED_DIR, String::new())
        }
        Place::Project(name) => {
            if !central
                .join("Work")
                .join(name)
                .join("ProjectCentral")
                .is_dir()
            {
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("{name} is no project of your world (it has no Work/{name}/ProjectCentral): nothing was filed."),
                ));
            }
            let scope = Scope::resolve(central, Some(name))?;
            (scope.root, PROJECT_FILED_DIR, format!("Work/{name}/"))
        }
    };
    let world = place.world_ref();
    let key = key_of(&world, &passage, &source);
    let relative = PathBuf::from(folder).join(format!("passage-{key}.json"));
    let central_path = format!("{prefix}{}", relative.to_string_lossy());
    let canonical = central.canonicalize()?;
    let location = serde_json::to_value(CentralPathRef::new(&canonical, central_path.clone())?)
        .map_err(io::Error::other)?;
    let answer = |outcome: &str, record: &FiledPassage| {
        json!({"schema": FILED_PASSAGE_RECEIPT_SCHEMA, "outcome": outcome, "place": world, "record": record,
            "source_path": central_path, "location": location,
            "read_path": {"action": "central.files.read", "input": {"location": location}},
            "authorship": "generated-proposal", "recognition": "unrecognised", "human_recognised": false,
            "automatic_agent_or_model_invocation": false})
    };
    let _lock = crate::source_safety::lock(&owner_root, "source-mutation.lock")?;
    let file = owner_root.join(&relative);
    match std::fs::symlink_metadata(&file) {
        Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => {
            let standing: FiledPassage =
                serde_json::from_slice(&std::fs::read(&file)?).map_err(|error| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("The filed passage at {central_path} does not read: {error}"),
                    )
                })?;
            if standing.passage == passage
                && standing.place == world
                && standing.source.path == source.path
            {
                return Ok(answer("unchanged", &standing));
            }
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!("Another record stands at {central_path}: nothing was filed"),
            ));
        }
        Ok(_) => {
            return Err(invalid(format!(
                "{central_path} is not a plain file: nothing was filed"
            )))
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    let record = FiledPassage {
        schema: FILED_PASSAGE_SCHEMA.to_owned(),
        passage_ref: format!("central:filed-passage:{world}:{key}"),
        place: world.clone(),
        passage,
        source,
        provenance: FiledProvenance {
            origin_action: FILE_PASSAGE_ACTION.to_owned(),
            recorded_at_unix_seconds: now,
            authorship: "generated-proposal".to_owned(),
            recognition: "unrecognised".to_owned(),
        },
    };
    ensure_folders(&owner_root, Path::new(folder))?;
    let mut bytes = serde_json::to_vec_pretty(&record).map_err(io::Error::other)?;
    bytes.push(b'\n');
    atomic_record(&owner_root, &relative, &bytes, RecordDisposition::CreateNew)?;
    Ok(answer("filed", &record))
}

fn file_passage_action(
    _: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let root = match root_of(FILE_PASSAGE_ACTION, context) {
        Ok(root) => root,
        Err(result) => return result,
    };
    match file_passage(&root, input, now()) {
        Ok(data) => ActionResult::success(FILE_PASSAGE_ACTION, data),
        Err(error) => failure(FILE_PASSAGE_ACTION, error),
    }
}

pub fn register_file_passage_action(registry: &mut ActionRegistry) {
    let mut source = input("source", true);
    source.input_type = "object".to_owned();
    registry
        .register(
            descriptor(
                FILE_PASSAGE_ACTION,
                "File a passage on a place",
                "File a passage on a place of your world (DLW manual §18.4, a tag sent to a place): place is control:root (your own world) or project:<Name>; passage is the words, kept verbatim or refused (no surrounding whitespace, at most 8000 characters); source is where it came from, {title, path? (the page's Central path, the link back), source_ref?}. It lands as central.filed-passage/v1 under Control/agents/places/filed, or the project's ProjectCentral/agents/places/filed, as a generated proposal that stays unrecognised until the person recognises it. Create-only: the same passage from the same page on the same place answers unchanged. Never invokes an Agent or model.",
                MutationClass::LocallyMutating,
                "central-filed-passage",
                vec![input("place", true), input("passage", true), source],
            ),
            file_passage_action,
        )
        .expect("central.place.file-passage is a valid Action id");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ground(test: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "central-file-passage-{}-{test}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("Control")).unwrap();
        std::fs::create_dir_all(root.join("Work")).unwrap();
        root.canonicalize().unwrap()
    }

    fn sent(place: &str) -> Value {
        json!({"place": place, "passage": "A completion claim without executed evidence is only tone.",
            "source": {"title": "Flow — Completion", "path": "Control/user/flows/flow-a.html"}})
    }

    #[test]
    fn a_passage_is_filed_on_your_world_word_for_word_with_the_link_back() {
        let root = ground("root");
        let receipt = file_passage(&root, &sent("control:root"), 1_761_700_000).unwrap();
        assert_eq!(receipt["schema"], FILED_PASSAGE_RECEIPT_SCHEMA);
        assert_eq!(receipt["outcome"], "filed");
        assert_eq!(receipt["place"], "control:root");
        assert_eq!(receipt["recognition"], "unrecognised");
        let path = receipt["source_path"].as_str().unwrap();
        assert!(
            path.starts_with("Control/agents/places/filed/passage-"),
            "{path}"
        );
        let kept: FiledPassage =
            serde_json::from_slice(&std::fs::read(root.join(path)).unwrap()).unwrap();
        assert_eq!(
            kept.passage,
            "A completion claim without executed evidence is only tone."
        );
        assert_eq!(
            kept.source.path.as_deref(),
            Some("Control/user/flows/flow-a.html")
        );
        assert_eq!(kept.provenance.authorship, "generated-proposal");
        assert_eq!(receipt["read_path"]["action"], "central.files.read");
        // The same passage from the same page on the same place: the same record, said, never a second copy.
        let again = file_passage(&root, &sent("control:root"), 1_761_700_100).unwrap();
        assert_eq!(again["outcome"], "unchanged");
        assert_eq!(again["source_path"], receipt["source_path"]);
        assert_eq!(
            std::fs::read_dir(root.join(ROOT_FILED_DIR))
                .unwrap()
                .count(),
            1
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_place_that_is_no_world_of_yours_and_a_passage_not_sent_whole_are_refused() {
        let root = ground("refused");
        for place in ["world:elsewhere", "project:", "project:../x", "project:a/b"] {
            assert!(file_passage(&root, &sent(place), 1).is_err(), "{place}");
        }
        // A project that is not in your world's Work is no place of it, said in words.
        assert!(file_passage(&root, &sent("project:Absent"), 1)
            .unwrap_err()
            .to_string()
            .contains("Absent is no project of your world"));
        let mut padded = sent("control:root");
        padded["passage"] = json!(" padded ");
        assert!(file_passage(&root, &padded, 1)
            .unwrap_err()
            .to_string()
            .contains("word for word"));
        let mut away = sent("control:root");
        away["source"]["path"] = json!("https://example.com/x");
        assert!(file_passage(&root, &away, 1).is_err());
        let mut long = sent("control:root");
        long["passage"] = json!("x".repeat(PASSAGE_MAX_CHARS + 1));
        assert!(file_passage(&root, &long, 1).is_err());
        assert!(!root.join(ROOT_FILED_DIR).exists(), "nothing was filed");
        let _ = std::fs::remove_dir_all(&root);
    }
}
