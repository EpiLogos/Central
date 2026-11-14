//! The root world's floor (manual §18.5 and §10.11; O-I #1005): the instruction file every harness reads at the Central root.
//!
//! `control.floor.render` is the one renderer of that floor. It renders the doors-only floor the person's session strap rendered
//! (`central-session-strap/render-context.py@2`), ported here so that adopting a statement renders the floor again: the field the
//! session stands in, the `oi` front door, how to load law and procedure by act (the law through `ctrl control index`), a never-do
//! card and the session's end, then only the statements whose relation places them on the floor: `floor: kernel` in full,
//! `floor: load` as a line under Load by act. Every other statement is carried by the door. Lines that hold only in one World come
//! from that World's statements: a sentence the floor quotes is quoted verbatim from its statement and refused where the statement
//! no longer holds it, and left out where the World keeps no such statement, so a fresh World still gets a floor. The header names
//! the renderer and one revision over every statement the floor answers to (its path, standing, place and content), so adopting a
//! statement moves it. The same law renders the same bytes. A floor past its byte budget is never written, nor one over a floor
//! another hand wrote: both are refused in Central's words, changing nothing. The instruction files the harnesses read (`files`:
//! `CLAUDE.md`, `GEMINI.md`, ...) are links to it, and where the person keeps Codex (`~/.codex`) and the root is that home's own
//! Central world (`~/Central`), its global instructions carry one delimited block naming the floor, since Codex never reads the
//! root from inside a repository.
//!
//! `control.floor.read` says what each named instruction file carries: rendered (by Central, or by the strap it replaces: whether
//! it stands as Central renders it now, and where each statement is carried, in the floor or by the door), written by another hand
//! (`foreign`: the statements it names) or absent.
//!
//! `control.law.adopt` adopts one statement written on tier 0, on the person's own credential: its relation is declared durable
//! and human-authored, placed as the person says (`floor`), and the floor is rendered again.
use crate::action::{
    ActionAvailability, ActionDescriptor, ActionExecutionContext, ActionInputDefinition,
    ActionOutputDefinition, ActionRegistry, MutationClass,
};
use crate::continuous_work::{authority, source, temporal};
use crate::result::{ActionResult, ResultStatus};
use crate::root::resolve_central_root;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// The floor every harness reads at the root, and the file each named instruction file links to.
pub const FLOOR_FILE: &str = "AGENTS.md";
/// The reading `control.floor.read` answers.
pub const FLOOR_READING_SCHEMA: &str = "central.floor-reading/v1";
/// The receipt `control.floor.render` answers.
pub const FLOOR_RENDER_SCHEMA: &str = "central.floor-render/v1";
/// The receipt `control.law.adopt` answers.
pub const LAW_ADOPTION_SCHEMA: &str = "central.law-adoption/v1";
/// The renderer a floor Central rendered names in its header, with the version of its form.
pub const FLOOR_RENDERER: &str = "central control.floor.render@2";
/// The renderers whose floors are this floor, and so Central's to render again: its own, and the session strap's it replaces.
const LINEAGE: [&str; 2] = [
    "central control.floor.render@",
    "central-session-strap/render-context.py@",
];
/// The floor's byte budget, header included: the owner's smallest-sufficient law, as the strap kept it. A floor that would pass it
/// is refused; a statement is shortened or moved behind the door, never the budget raised to fit.
pub const FLOOR_BUDGET_BYTES: usize = 4096;
/// Where the root world's tier 0 keeps its law.
pub const GOVERNANCE_DIR: &str = "Control/agents/governance";
const GOVERNANCE_ROLE: &str = "agent-governance-source";
/// The engineering law's compile, which the floor names as where that law is gathered.
const COMPILED: &str = "Control/agents/governance/engineering/foundational-prompt.md";
const COMPILED_PREFIX: &str = "Control/agents/governance/engineering/";
/// The session strap's own words, which a harness without skills reads instead.
const STRAP: &str = "Control/user/skills/central-session-strap/SKILL.md";
/// The statements whose sentences the floor and Codex's block quote, where the World keeps them.
const SHAPE: &str = "Control/agents/governance/world/shape-of-the-world.md";
const AUTHORSHIP: &str = "Control/agents/governance/authorship-and-return/propose-not-write.md";
const RESPONSIBILITY: &str = "Control/agents/governance/authorship-and-return/responsibility.md";
const QUOTED: [&str; 3] = [SHAPE, AUTHORSHIP, RESPONSIBILITY];
const WORKTREE: &str = "nothing creates another worktree (owner ruling, 29 September 2026, after repeated per-lane worktrees bloated the disk)";
const PRIMARY: &str = "The ground's primary checkout for a project — `Work/<Name>` itself — stands on its `main` branch.";
const LANDED: &str = "no checkout is left standing on the landed branch";
const AUTHORSHIP_LAW: &str = "Authorship names whose intent originates a change, never whose hands perform it: on my stated intent you perform every operation yourself — including writes into this ground — and handing file mechanics back to me is unfinished work, not deference.";
const OWN_THE_FAULT: &str = "Own the fault you make or discover; own means resolve honestly.";

const FIELD_TAIL: [&str; 5] = [
    "`Control/` is the owner's: authored law in `Control/agents/governance/`, agent",
    "material in `Control/agents/`. `Work/<Name>/` are projects, each with a",
    "`ProjectCentral/` of the same shape. Two registers, one law: inside one",
    "project's concern → that project; cross-project, suite-level, world-keeping →",
    "the root. Work one register at a time.",
];
const FRONT_DOOR: [&str; 12] = [
    "## Front door: oi",
    "",
    "`oi` is the O:I CLI. It discloses what is installed and routes to each",
    "product's native CLI — ask it, do not guess commands.",
    "",
    "```text",
    "oi world | oi current-world    where you are, what is present",
    "oi search WORDS...             find across the composed field (inert)",
    "oi act [describe|invoke A]     the canonical Actions, root and project registers",
    "oi central|aikit|factory|workcell|actuation|ql ...   the native CLIs",
    "oi explain ...                 why something resolved as it did",
    "```",
];
const CARD_HEAD: [&str; 6] = [
    "## Never-do card",
    "",
    "- The wiki is never edited directly: knowledge travels as a return, and the",
    "  return is the only door.",
    "- AGENTS.md and CLAUDE.md are rendered floors — edit the governance source and",
    "  re-render; a generated suggestion stays generated until adopted.",
];
const CARD_TAIL: [&str; 2] = [
    "- Carried is not loaded, and loaded is not used: the disclosure answers what",
    "  is actually operative.",
];
const SESSION_END: [&str; 3] = [
    "Session end, every session: leave no loose folders at the Work root. Write no",
    "handoff or session return by hand: a session's NOW opens and closes with its",
    "hooks (O-I PROGRAMME R36).",
];
const CODEX_FOLDER: &str = ".codex";
const CODEX_BEGIN: &str =
    "<!-- central:generated-begin (central control.floor.render; do not edit inside) -->";
/// Any block Central's lineage wrote, whichever renderer its begin marker names.
const CODEX_BEGIN_PREFIX: &str = "<!-- central:generated-begin";
const CODEX_END: &str = "<!-- central:generated-end -->";

const RENDER: &str = "control.floor.render";
const READ: &str = "control.floor.read";
pub const LAW_ADOPT: &str = "control.law.adopt";

/// A refusal in Central's own words that changed nothing.
fn refused(words: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::PermissionDenied, words.into())
}

/// The instruction-file names an Action was given (`files`): each one plain file name at the root, never a path.
fn file_names(input: &Value, required: bool) -> io::Result<Vec<String>> {
    let Some(given) = input.get("files").filter(|value| !value.is_null()) else {
        return if required {
            Err(source::invalid(
                "files names the instruction files the harnesses read (CLAUDE.md, GEMINI.md, ...)",
            ))
        } else {
            Ok(Vec::new())
        };
    };
    let names = given.as_array().ok_or_else(|| {
        source::invalid("files is a list of instruction-file names (CLAUDE.md, GEMINI.md, ...)")
    })?;
    if names.len() > 32 {
        return Err(source::invalid("files names at most 32 instruction files"));
    }
    let mut out: Vec<String> = Vec::new();
    for name in names {
        let name = name
            .as_str()
            .map(str::trim)
            .filter(|name| {
                !name.is_empty()
                    && name.len() <= 128
                    && *name != "."
                    && *name != ".."
                    && !name.contains(['/', '\\', '\0'])
            })
            .ok_or_else(|| {
                source::invalid(
                    "each of files is one instruction file's name at the Central root (CLAUDE.md), never a path",
                )
            })?;
        if !out.iter().any(|held| held == name) {
            out.push(name.to_owned());
        }
    }
    Ok(out)
}

/// Where a statement stands on the floor, as its relation places it (`floor`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Place {
    /// In full, under its own title.
    Kernel,
    /// Its words as one line under Load by act.
    Load,
    /// Behind the door (`ctrl control index`): the default.
    Index,
}

impl Place {
    fn of(value: Option<&str>) -> Self {
        match value {
            Some("kernel") => Self::Kernel,
            Some("load") => Self::Load,
            _ => Self::Index,
        }
    }
    fn as_str(self) -> &'static str {
        match self {
            Self::Kernel => "kernel",
            Self::Load => "load",
            Self::Index => "index",
        }
    }
}

/// A statement's relation in the register, as the floor reads it.
#[derive(Clone, Debug)]
struct Declared {
    standing: String,
    provenance: String,
    place: Place,
}

/// One statement of the root world's tier 0: the governance index's entry, its relation where it has one, and its words.
#[derive(Clone, Debug)]
struct Statement {
    file: String,
    topic: String,
    standing: String,
    declared: Option<Declared>,
    text: String,
}

impl Statement {
    fn path(&self) -> String {
        format!("{GOVERNANCE_DIR}/{}", self.file)
    }
    fn place(&self) -> Place {
        self.declared
            .as_ref()
            .map_or(Place::Index, |held| held.place)
    }
    fn value(&self) -> Value {
        json!({"file": self.file, "topic": self.topic, "standing": self.standing})
    }
    /// The person's durable law first, then drafts, then the rest, as the strap ordered what it carried.
    fn rank(&self) -> u8 {
        match self.declared.as_ref().map(|held| held.standing.as_str()) {
            Some("durable-source") => 0,
            Some("draft-source") => 1,
            _ => 2,
        }
    }
    /// What a non-durable statement on the floor says of itself, so a session can tell the person's law from a proposal.
    fn standing_mark(&self) -> Option<String> {
        self.declared
            .as_ref()
            .filter(|held| held.standing != "durable-source")
            .map(|held| format!("*Standing: {} ({}).*", held.standing, held.provenance))
    }
}

/// Whether a register path is a statement of the governance folder: one Markdown file under it, never its README or its compile.
fn is_statement_path(path: &str) -> bool {
    path.strip_prefix(&format!("{GOVERNANCE_DIR}/"))
        .is_some_and(|file| {
            file.ends_with(".md")
                && !matches!(
                    Path::new(file).file_name().and_then(|name| name.to_str()),
                    Some("README.md" | "foundational-prompt.md")
                )
        })
}

/// A statement's title: its own `# ` line, or its file name where it declares none.
fn title_of(text: &str, path: &str) -> String {
    text.lines()
        .find_map(|line| line.strip_prefix("# "))
        .map(|title| title.trim().to_owned())
        .unwrap_or_else(|| {
            let stem = Path::new(path)
                .file_stem()
                .map(|stem| stem.to_string_lossy().replace('-', " "))
                .unwrap_or_default();
            let mut chars = stem.chars();
            chars
                .next()
                .map(|first| {
                    first
                        .to_uppercase()
                        .chain(chars.flat_map(char::to_lowercase))
                        .collect()
                })
                .unwrap_or_default()
        })
}

/// A statement's own words: its text without front matter, its `# ` title line and the blank lines after it.
fn body_of(text: &str) -> String {
    let mut lines: Vec<&str> = text.lines().collect();
    if lines.first().is_some_and(|line| line.trim() == "---") {
        if let Some(end) = lines.iter().skip(1).position(|line| *line == "---") {
            lines.drain(..end + 2);
        }
    }
    if lines.first().is_some_and(|line| line.starts_with("# ")) {
        lines.remove(0);
        while lines.first().is_some_and(|line| line.trim().is_empty()) {
            lines.remove(0);
        }
    }
    lines.join("\n").trim_end().to_owned()
}

/// Whitespace collapsed, so a quote survives its statement's line wrapping.
fn collapse(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The law the floor answers to: every statement the governance index lists, with its relation, the relations whose statement is
/// not there, and what the floor's pointers point at.
struct Law {
    root: PathBuf,
    statements: Vec<Statement>,
    /// Declared statements not on the index: their path and their standing as declared.
    missing: Vec<(String, String)>,
    compiled: Option<String>,
    strap: bool,
}

/// A floor as Central renders it from the law.
struct Rendered {
    text: String,
    law_revision: String,
    unrendered: Vec<String>,
}

impl Law {
    fn gather(scope: &source::Scope) -> io::Result<Self> {
        let root = scope.central_root.clone();
        let (relations, _) = scope.relations()?;
        let mut declared: BTreeMap<String, Declared> = BTreeMap::new();
        for relation in relations["relations"].as_array().into_iter().flatten() {
            let Some(path) = relation["path"]
                .as_str()
                .filter(|path| is_statement_path(path))
            else {
                continue;
            };
            declared.insert(
                path.to_owned(),
                Declared {
                    standing: relation["standing"]
                        .as_str()
                        .unwrap_or("unknown")
                        .to_owned(),
                    provenance: relation["provenance"]
                        .as_str()
                        .unwrap_or("unknown")
                        .to_owned(),
                    place: Place::of(relation["floor"].as_str()),
                },
            );
        }
        let mut statements = Vec::new();
        if root.join(GOVERNANCE_DIR).is_dir() {
            for entry in crate::control::index_governance(&root)?.statements {
                let path = format!("{GOVERNANCE_DIR}/{}", entry.file);
                let text = crate::source_safety::read(&root, &path)?;
                statements.push(Statement {
                    declared: declared.remove(&path),
                    file: entry.file,
                    topic: entry.topic,
                    standing: entry.standing,
                    text,
                });
            }
        }
        let compiled = match crate::source_safety::read(&root, COMPILED) {
            Ok(text) => Some(text),
            Err(error) if error.kind() == io::ErrorKind::NotFound => None,
            Err(error) => return Err(error),
        };
        Ok(Self {
            strap: root.join(STRAP).is_file(),
            root,
            statements,
            missing: declared
                .into_iter()
                .map(|(path, held)| (path, held.standing))
                .collect(),
            compiled,
        })
    }

    fn find(&self, path: &str) -> Option<&Statement> {
        self.statements
            .iter()
            .find(|statement| statement.path() == path)
    }

    /// The statements placed on the floor at `place`, in the order the floor carries them.
    fn placed(&self, place: Place) -> Vec<&Statement> {
        let mut out: Vec<&Statement> = self
            .statements
            .iter()
            .filter(|statement| statement.place() == place)
            .collect();
        out.sort_by_key(|statement| (statement.rank(), statement.path()));
        out
    }

    /// A sentence the floor quotes from one of the World's statements: verbatim, whitespace collapsed; `None` where the World
    /// keeps no such statement; refused where the statement no longer holds it (the law moved, and the floor never carries a
    /// stale quote of it).
    fn quote(&self, path: &str, sentence: &str) -> io::Result<Option<String>> {
        let Some(statement) = self.find(path) else {
            return Ok(None);
        };
        let quote = collapse(sentence);
        if collapse(&statement.text).contains(&quote) {
            Ok(Some(quote))
        } else {
            Err(refused(format!(
                "The floor quotes {path}, and it no longer holds the quoted sentence (“{quote}”): the law moved, so the floor is not rendered over it. Restore the sentence in the statement, or carry the change into Central's floor. Nothing was written."
            )))
        }
    }

    /// The floor the law renders, or its refusal: a declared durable statement lost, a quote its statement no longer holds, or a
    /// floor past its budget naming each statement it carries in full.
    fn render(&self, home: Option<&Path>) -> io::Result<Rendered> {
        let lost: Vec<&str> = self
            .missing
            .iter()
            .filter(|(_, standing)| standing == "durable-source")
            .map(|(path, _)| path.as_str())
            .collect();
        if !lost.is_empty() {
            return Err(refused(format!(
                "The law declares {} durable, and the governance index does not find it: a declared statement that cannot be found leaves the floor broken, so it is not rendered. Restore it at its path. Nothing was written.",
                lost.join(", ")
            )));
        }
        let mut unrendered: Vec<String> = self
            .missing
            .iter()
            .map(|(path, _)| format!("declared source is absent from disk: {path}"))
            .collect();
        for statement in &self.statements {
            let path = statement.path();
            if statement.declared.is_some()
                && path.starts_with(COMPILED_PREFIX)
                && !self
                    .compiled
                    .as_deref()
                    .is_some_and(|compiled| compiled.contains(&path))
            {
                unrendered.push(format!(
                    "declared source claims the engineering compile carries it, but foundational-prompt.md does not name it: {path}"
                ));
            }
        }
        let shown = shown_root(&named_root(&self.root, home), home);
        let mut sections = Vec::new();
        let mut field = vec![
            "## The field you stand in".to_owned(),
            String::new(),
            format!("You are in the Central personal world, `{shown}` (no repo at the root)."),
        ];
        field.extend(FIELD_TAIL.iter().map(|line| (*line).to_owned()));
        sections.push(field.join("\n"));
        sections.push(FRONT_DOOR.join("\n"));
        let mut load = vec![
            "## Load by act, not by default".to_owned(),
            String::new(),
            "- Law: `ctrl control index` lists the statements (topic, standing); read the one your act touches.".to_owned(),
            format!(
                "- Procedure: the `central-*` skills. `central-session-strap` orients a session and routes it to the practice it needs{}.",
                if self.strap { format!(" (no skills: `{STRAP}`)") } else { String::new() }
            ),
            format!(
                "- Before a field's work, load its methodology: `engineering-methodology` (code{}), `collaboration-methodology`, `authorship-methodology`, `world-keeping-methodology`, `docs-methodology`.",
                if self.compiled.is_some() { format!("; compiled in `{COMPILED}`") } else { String::new() }
            ),
        ];
        for statement in self.placed(Place::Load) {
            let mark = statement
                .standing_mark()
                .map(|mark| format!(" {mark}"))
                .unwrap_or_default();
            load.push(format!("- {}{mark}", collapse(&body_of(&statement.text))));
        }
        sections.push(load.join("\n"));
        let mut card: Vec<String> = CARD_HEAD.iter().map(|line| (*line).to_owned()).collect();
        if let Some(worktree) = self.quote(SHAPE, WORKTREE)? {
            let primary = self.quote(SHAPE, PRIMARY)?.unwrap_or_default();
            let landed = self.quote(SHAPE, LANDED)?.unwrap_or_default();
            card.push(
                "- Development happens in the persistent env-1/2/3 seats (`seat join`), and"
                    .to_owned(),
            );
            card.push(format!("  “{worktree}”."));
            card.push(format!(
                "- “{primary}” A landed lane releases its seat: “{landed}”."
            ));
            card.push(format!("  (Both from `{SHAPE}`.)"));
        }
        card.extend(CARD_TAIL.iter().map(|line| (*line).to_owned()));
        sections.push(card.join("\n"));
        if let Some(law) = self.quote(AUTHORSHIP, AUTHORSHIP_LAW)? {
            sections.push(format!(
                "Durable Control change — the authorship law, quoted from `{AUTHORSHIP}`:\n\n> {law}"
            ));
        }
        sections.push(SESSION_END.join("\n"));
        for statement in self.placed(Place::Kernel) {
            let mark = statement
                .standing_mark()
                .map(|mark| format!("\n\n{mark}"))
                .unwrap_or_default();
            sections.push(format!(
                "## {}{mark}\n\n{}",
                title_of(&statement.text, &statement.path()),
                body_of(&statement.text)
            ));
        }
        // One revision over every statement the floor answers to: each declared statement and each it quotes, by its path, its
        // standing, its place and its content. Adopting a statement, placing it or changing its words moves it.
        let answered: Vec<String> = self
            .statements
            .iter()
            .filter(|statement| {
                statement.declared.is_some() || QUOTED.contains(&statement.path().as_str())
            })
            .map(|statement| {
                format!(
                    "{} {} {} {}",
                    statement.path(),
                    statement.standing,
                    statement.place().as_str(),
                    crate::source_safety::content_revision_bytes(statement.text.as_bytes())
                )
            })
            .collect();
        let law_revision =
            crate::source_safety::content_revision_bytes(answered.join("\n").as_bytes());
        let mut header = vec![
            "# Central — session ground (derived; do not edit)".to_owned(),
            String::new(),
            // No longer than the strap's own comment: the budget counts the header too.
            "<!-- derived, generated-derived: adopt a statement under Control/agents/governance/ (control.law.adopt)".to_owned(),
            "to change it; Central renders it (control.floor.render) and control.floor.read reads it.".to_owned(),
            format!(
                "renderer: {FLOOR_RENDERER}; {} statements, revision {law_revision}",
                answered.len()
            ),
        ];
        if !unrendered.is_empty() {
            header.push(
                "unrendered — declared but not carried anywhere (a fault, not a choice):"
                    .to_owned(),
            );
            header.extend(unrendered.iter().map(|problem| format!("- {problem}")));
        }
        header.push("-->".to_owned());
        header.push(String::new());
        let text = format!("{}\n{}\n", header.join("\n"), sections.join("\n\n"));
        if text.len() > FLOOR_BUDGET_BYTES {
            let carried: Vec<String> = self
                .placed(Place::Kernel)
                .into_iter()
                .chain(self.placed(Place::Load))
                .map(|statement| {
                    format!(
                        "{} ({}, {} bytes)",
                        statement.topic,
                        statement.path(),
                        statement.text.len()
                    )
                })
                .collect();
            let carried = if carried.is_empty() {
                "no statement".to_owned()
            } else {
                carried.join("; ")
            };
            return Err(refused(format!(
                "The floor would be {} bytes, past its budget of {FLOOR_BUDGET_BYTES} (smallest sufficient: shorten a statement it carries, or move it behind the door; the budget is not raised to fit). It carries in full: {carried}. Nothing was written.",
                text.len()
            )));
        }
        Ok(Rendered {
            text,
            law_revision,
            unrendered,
        })
    }

    /// Where a floor carries each statement: in the floor (in full, or as its line under Load by act) or by the door, which lists
    /// every statement with its standing. A statement the floor quotes says so, and one the engineering compile gathers names it.
    fn accounting(&self, floor: &str) -> Vec<Value> {
        self.statements
            .iter()
            .map(|statement| {
                let path = statement.path();
                let in_floor = match statement.place() {
                    Place::Kernel => {
                        let heading = format!("## {}", title_of(&statement.text, &path));
                        floor.lines().any(|line| line == heading)
                    }
                    Place::Load => {
                        let line = format!("- {}", collapse(&body_of(&statement.text)));
                        floor.lines().any(|held| held.starts_with(&line))
                    }
                    Place::Index => false,
                };
                let mut row = statement.value();
                row["floor"] = json!(statement.place().as_str());
                row["carried"] = json!(if in_floor { "floor" } else { "door" });
                if !in_floor && floor.contains(&format!("`{path}`")) {
                    row["quoted"] = json!(true);
                }
                if path.starts_with(COMPILED_PREFIX)
                    && self
                        .compiled
                        .as_deref()
                        .is_some_and(|compiled| compiled.contains(&path))
                {
                    row["compiled_in"] = json!(COMPILED);
                }
                row
            })
            .collect()
    }
}

/// The root under the name the person's home gives it (`~/Central` when it is that folder), so the floor and Codex's block do not
/// depend on the case a shell stood in.
fn named_root(root: &Path, home: Option<&Path>) -> PathBuf {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if let Some(named) = home.map(|home| home.join("Central")) {
            if let (Ok(a), Ok(b)) = (fs::metadata(&named), fs::metadata(root)) {
                if a.dev() == b.dev() && a.ino() == b.ino() {
                    return named;
                }
            }
        }
    }
    root.to_path_buf()
}

/// The root as the floor shows it: under `~` where it is in the person's home.
fn shown_root(root: &Path, home: Option<&Path>) -> String {
    match home.and_then(|home| root.strip_prefix(home).ok()) {
        Some(inside) if !inside.as_os_str().is_empty() => format!("~/{}", inside.display()),
        _ => root.display().to_string(),
    }
}

/// What stands at a root file: absent, a link to the floor (by the name it links to), or a file of its own.
enum Standing {
    Absent,
    LinkToFloor,
    Link(String),
    File,
}

fn standing_of(root: &Path, name: &str) -> io::Result<Standing> {
    let path = root.join(name);
    match fs::symlink_metadata(&path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Standing::Absent),
        Err(error) => Err(error),
        Ok(metadata) if metadata.file_type().is_symlink() => {
            let target = fs::read_link(&path)?;
            if target == Path::new(FLOOR_FILE) || target == root.join(FLOOR_FILE) {
                Ok(Standing::LinkToFloor)
            } else {
                Ok(Standing::Link(target.to_string_lossy().into_owned()))
            }
        }
        Ok(_) => Ok(Standing::File),
    }
}

/// The renderer a floor's header names, if it names one: up to its first `;`.
fn renderer_of(text: &str) -> Option<String> {
    text.lines().find_map(|line| {
        line.trim()
            .strip_prefix("renderer:")
            .map(|named| named.split(';').next().unwrap_or("").trim().to_owned())
            .filter(|named| !named.is_empty())
    })
}

/// The revision a floor's header names over the law it was rendered from.
fn law_revision_of(text: &str) -> Option<String> {
    text.lines()
        .find(|line| line.trim().starts_with("renderer:"))
        .and_then(|line| line.split_once(" revision "))
        .map(|(_, revision)| revision.trim().to_owned())
        .filter(|revision| !revision.is_empty())
}

/// Whether a floor is this floor: rendered by Central, or by the session strap whose renderer Central's replaces.
fn in_lineage(text: &str) -> bool {
    renderer_of(text).is_some_and(|named| LINEAGE.iter().any(|prefix| named.starts_with(prefix)))
}

fn headings(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|line| line.strip_prefix("## "))
        .map(|heading| heading.trim().to_owned())
        .collect()
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0)
}

/// Codex's global instructions as the render leaves them: the person's own words kept, one block of Central's naming the floor and
/// quoting what must hold before it is read.
struct CodexBlock {
    folder: PathBuf,
    before: Option<String>,
    after: String,
}

/// Codex's block, where the person keeps Codex at this home (`~/.codex`) and the root rendered is that home's own Central world
/// (`~/Central`): a world elsewhere never writes the person's global instructions. Refused where a sentence it quotes moved;
/// where Central writes none, why, in words.
fn codex_block(law: &Law, home: Option<&Path>) -> io::Result<Result<CodexBlock, String>> {
    let Some(home) = home else {
        return Ok(Err(
            "Central was asked without the person's home, so there are no global Codex instructions to carry the floor".into(),
        ));
    };
    let own = home.join("Central");
    let root = named_root(&law.root, Some(home));
    if root != own {
        return Ok(Err(format!(
            "{} is not this home's own Central world ({}), so Codex's global instructions at this home are not its to write",
            law.root.display(),
            own.display()
        )));
    }
    let folder = home.join(CODEX_FOLDER);
    if !folder.is_dir() {
        return Ok(Err(
            "no Codex folder (~/.codex) at this home, so there are no global Codex instructions to carry the floor".into(),
        ));
    }
    let mut carried = Vec::new();
    for (path, sentence) in [(RESPONSIBILITY, OWN_THE_FAULT)] {
        let Some(statement) = law
            .find(path)
            .filter(|statement| statement.declared.is_some())
        else {
            continue;
        };
        let Some(quote) = law.quote(path, sentence)? else {
            continue;
        };
        carried.push(format!(
            "<!-- {} @ {} -->\n{} (`{path}`): “{quote}”",
            crate::source_horizon::source_ref(crate::source_horizon::CONTROL_WORLD_REF, path),
            crate::source_safety::content_revision_bytes(statement.text.as_bytes()),
            title_of(&statement.text, path)
        ));
    }
    let block = [
        CODEX_BEGIN.to_owned(),
        "# Central personal world — carried for every Codex session".to_owned(),
        String::new(),
        format!("When your working directory is under {}, you stand in the Central personal world.", root.display()),
        format!("Its derived ground is {}/{FLOOR_FILE} — a short floor of doors (`oi`, `ctrl control index`).", root.display()),
        "Read it before consequential work, even when this repository has its own AGENTS.md:".to_owned(),
        "Codex does not load the root ground from inside a repository.".to_owned(),
        String::new(),
        carried.join("\n\n"),
        CODEX_END.to_owned(),
    ]
    .join("\n");
    let before = match fs::read_to_string(folder.join(FLOOR_FILE)) {
        Ok(text) => Some(text),
        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
        Err(error) => return Err(error),
    };
    let current = before.as_deref().unwrap_or("");
    let after = match current.find(CODEX_BEGIN_PREFIX).and_then(|begin| {
        current[begin..]
            .find(CODEX_END)
            .map(|end| (begin, begin + end + CODEX_END.len()))
    }) {
        Some((begin, end)) => format!("{}{block}{}", &current[..begin], &current[end..]),
        None => format!(
            "{}{}{block}\n",
            current.trim_end_matches('\n'),
            if current.is_empty() { "" } else { "\n\n" }
        ),
    };
    Ok(Ok(CodexBlock {
        folder,
        before,
        after,
    }))
}

/// Render the floor; the caller holds the root's source lock. Everything is checked before anything is written: a floor another
/// hand wrote, a named file that is neither absent nor a link to the floor, a declared statement lost, a quote that moved and a
/// floor past its budget are refused in Central's words, changing nothing.
fn render_locked(
    scope: &source::Scope,
    files: &[String],
    home: Option<&Path>,
) -> io::Result<Value> {
    let root = scope.central_root.as_path();
    let floor_path = root.join(FLOOR_FILE);
    let before = match standing_of(root, FLOOR_FILE)? {
        Standing::Absent => None,
        Standing::File => Some(fs::read_to_string(&floor_path)?),
        Standing::LinkToFloor | Standing::Link(_) => {
            return Err(refused(format!(
                "{FLOOR_FILE} at {} is a link Central did not make, so it is not a floor Central rendered; Central leaves it as it is. Nothing was written.",
                root.display()
            )))
        }
    };
    if let Some(text) = &before {
        if !in_lineage(text) {
            let by = renderer_of(text)
                .map(|named| format!("its header names {named} as its renderer"))
                .unwrap_or_else(|| "its header names no renderer".to_owned());
            return Err(refused(format!(
                "{FLOOR_FILE} at {} was not rendered by Central ({by}); Central does not overwrite a floor it did not render. Nothing was written.",
                root.display()
            )));
        }
    }
    let mut links = Vec::new();
    for name in files.iter().filter(|name| name.as_str() != FLOOR_FILE) {
        match standing_of(root, name)? {
            Standing::Absent => links.push((name.clone(), "made")),
            Standing::LinkToFloor => links.push((name.clone(), "kept")),
            Standing::Link(target) => {
                return Err(refused(format!(
                    "{name} at the Central root links to {target}, not to {FLOOR_FILE}; Central leaves an instruction file it did not make as it is. Nothing was written."
                )))
            }
            Standing::File => {
                return Err(refused(format!(
                    "{name} at the Central root is a file of its own, not a link to {FLOOR_FILE}; Central leaves an instruction file it did not make as it is. Nothing was written."
                )))
            }
        }
    }
    let law = Law::gather(scope)?;
    let rendered = law.render(home)?;
    let codex = codex_block(&law, home)?;
    let changed = before.as_deref() != Some(rendered.text.as_str());
    if changed {
        crate::file_mutation::atomic_record(
            root,
            Path::new(FLOOR_FILE),
            rendered.text.as_bytes(),
            crate::file_mutation::RecordDisposition::ReplaceOrCreate,
        )?;
    }
    for (name, state) in &links {
        if *state == "made" {
            make_link(root, name)?;
        }
    }
    let codex = match codex {
        Ok(block) => {
            let changed = block.before.as_deref() != Some(block.after.as_str());
            if changed {
                crate::file_mutation::atomic_record(
                    &block.folder,
                    Path::new(FLOOR_FILE),
                    block.after.as_bytes(),
                    crate::file_mutation::RecordDisposition::ReplaceOrCreate,
                )?;
            }
            json!({"file": block.folder.join(FLOOR_FILE).to_string_lossy(), "changed": changed})
        }
        Err(why) => json!({"file": null, "changed": false, "why": why}),
    };
    let new = headings(&rendered.text);
    let dropped: Vec<String> = before
        .as_deref()
        .map(headings)
        .unwrap_or_default()
        .into_iter()
        .filter(|heading| !new.contains(heading))
        .collect();
    Ok(json!({
        "schema": FLOOR_RENDER_SCHEMA,
        "root": root.to_string_lossy(),
        "floor": FLOOR_FILE,
        "renderer": FLOOR_RENDERER,
        "changed": changed,
        "revision": crate::source_safety::content_revision_bytes(rendered.text.as_bytes()),
        "law_revision": rendered.law_revision,
        "bytes": rendered.text.len(),
        "budget_bytes": FLOOR_BUDGET_BYTES,
        "statements": law.accounting(&rendered.text),
        "unrendered": rendered.unrendered,
        "dropped": dropped,
        "links": links.iter().map(|(file, state)| json!({"file": file, "to": FLOOR_FILE, "state": state})).collect::<Vec<_>>(),
        "codex": codex,
        "automatic_agent_or_model_invocation": false,
    }))
}

#[cfg(unix)]
fn make_link(root: &Path, name: &str) -> io::Result<()> {
    std::os::unix::fs::symlink(FLOOR_FILE, root.join(name))
}
#[cfg(not(unix))]
fn make_link(_: &Path, name: &str) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        format!("{name} cannot be made as a link to {FLOOR_FILE} on this system"),
    ))
}

/// The root world's scope, or why `root` is no Central ground in words: it keeps no Control and Work.
fn ground_scope(root: &Path) -> io::Result<source::Scope> {
    source::Scope::resolve(root, None).map_err(|error| match error.kind() {
        io::ErrorKind::NotFound => io::Error::new(
            io::ErrorKind::NotFound,
            format!("{} is no Central ground (it keeps no Control and Work): there is no law here to render a floor from", root.display()),
        ),
        _ => error,
    })
}

/// Render the root world's floor (`control.floor.render`). `home` is the person's home, where Codex keeps its global instructions.
pub fn render(root: &Path, input: &Value, home: Option<&Path>) -> io::Result<Value> {
    let files = file_names(input, false)?;
    let scope = ground_scope(root)?;
    let _locks = source::lock(&scope)?;
    render_locked(&scope, &files, home)
}

/// The governance statements a floor Central did not render names: by their path under the governance folder, or by their
/// topic as a heading.
fn named_by(text: &str, index: &[Statement]) -> Vec<Value> {
    let headings: Vec<String> = text
        .lines()
        .filter_map(|line| {
            let rest = line.trim_start_matches('#');
            (rest.len() < line.len() && rest.starts_with(' ')).then(|| rest.trim().to_lowercase())
        })
        .collect();
    index
        .iter()
        .filter(|statement| {
            text.contains(&statement.path())
                || headings
                    .iter()
                    .any(|heading| *heading == statement.topic.trim().to_lowercase())
        })
        .map(Statement::value)
        .collect()
}

/// Read what each named instruction file carries (`control.floor.read`). A floor of Central's lineage is read against the law as
/// it stands: whether it is the floor Central renders now (`current`), the revision its header names, and where each statement is
/// carried, in the floor or by the door.
pub fn read(root: &Path, input: &Value, home: Option<&Path>) -> io::Result<Value> {
    let files = file_names(input, true)?;
    let root = root.canonicalize().map_err(|error| match error.kind() {
        io::ErrorKind::NotFound => io::Error::new(
            io::ErrorKind::NotFound,
            format!(
                "no Central ground at {}: there are no floors to read",
                root.display()
            ),
        ),
        _ => error,
    })?;
    let law = match ground_scope(&root) {
        Ok(scope) => Some(Law::gather(&scope)?),
        Err(_) => None,
    };
    let now = law.as_ref().map(|law| law.render(home));
    let mut floors = Vec::new();
    for name in &files {
        let link = matches!(standing_of(&root, name)?, Standing::LinkToFloor);
        let text = match fs::read(root.join(name)) {
            Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                let why = if link {
                    format!("{name} links to {FLOOR_FILE}, which is not at the Central root")
                } else {
                    format!("no {name} at the Central root")
                };
                floors.push(json!({"file": name, "state": "absent", "statements": [], "why": why}));
                continue;
            }
            Err(error) => return Err(error),
        };
        if in_lineage(&text) {
            let mut floor = json!({"file": name, "state": "rendered", "renderer": renderer_of(&text),
                "revision": crate::source_safety::content_revision_bytes(text.as_bytes()),
                "law_revision": law_revision_of(&text)});
            match (&law, &now) {
                (Some(law), Some(now)) => {
                    floor["statements"] = json!(law.accounting(&text));
                    match now {
                        Ok(rendered) => {
                            floor["current"] = json!(rendered.text == text);
                            floor["unrendered"] = json!(rendered.unrendered);
                        }
                        Err(why) => {
                            floor["current"] = json!(false);
                            floor["why"] = json!(format!("Central would not render it now: {why}"));
                        }
                    }
                }
                _ => {
                    floor["statements"] = json!([]);
                    floor["current"] = json!(false);
                    floor["why"] = json!(format!(
                        "{} keeps no Control and Work, so there is no law to read it against",
                        root.display()
                    ));
                }
            }
            floors.push(floor);
        } else {
            let renderer = renderer_of(&text);
            let why = match &renderer {
                Some(named) => format!("{name} was rendered by {named}, not by Central: Central reads the law it names and leaves it as it is"),
                None => format!("{name} names no renderer, so Central did not render it: Central reads the law it names and leaves it as it is"),
            };
            let index = law
                .as_ref()
                .map_or(&[][..], |law| law.statements.as_slice());
            let mut floor = json!({"file": name, "state": "foreign", "statements": named_by(&text, index), "why": why});
            if let Some(named) = renderer {
                floor["renderer"] = json!(named);
            }
            floors.push(floor);
        }
    }
    Ok(json!({
        "schema": FLOOR_READING_SCHEMA,
        "root": root.to_string_lossy(),
        "floors": floors,
        "automatic_agent_or_model_invocation": false,
    }))
}

/// The statement `control.law.adopt` names: one Markdown file under the governance folder, never the folder's README or its
/// compiled prompt. Answers its path under the root.
fn statement_path(input: &Value) -> io::Result<String> {
    let given = input
        .get("statement")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|given| !given.is_empty() && given.len() <= 1024)
        .ok_or_else(|| {
            source::invalid(format!(
                "statement names one law statement under {GOVERNANCE_DIR}/ (its path there, as craft/plain-words.md)"
            ))
        })?;
    let file = given
        .strip_prefix(&format!("{GOVERNANCE_DIR}/"))
        .unwrap_or(given);
    let relative = Path::new(file);
    if !file.ends_with(".md")
        || relative.is_absolute()
        || !relative
            .components()
            .all(|part| matches!(part, std::path::Component::Normal(_)))
        || matches!(
            relative.file_name().and_then(|name| name.to_str()),
            Some("README.md" | "foundational-prompt.md")
        )
    {
        return Err(source::invalid(format!(
            "statement is one law statement's Markdown file under {GOVERNANCE_DIR}/, never its README, its compiled prompt or a path outside it"
        )));
    }
    Ok(format!("{GOVERNANCE_DIR}/{file}"))
}

/// Where the person places the statement they adopt (`floor`): in full, as a line under Load by act, or behind the door; `None`
/// keeps the place its relation has.
fn place_given(input: &Value) -> io::Result<Option<Place>> {
    match input.get("floor").filter(|value| !value.is_null()) {
        None => Ok(None),
        Some(value) => match value.as_str() {
            Some("kernel") => Ok(Some(Place::Kernel)),
            Some("load") => Ok(Some(Place::Load)),
            Some("index") => Ok(Some(Place::Index)),
            _ => Err(source::invalid(
                "floor places the statement: kernel (in full on the floor), load (its words as a line under Load by act) or index (behind the door, ctrl control index)",
            )),
        },
    }
}

/// Adopt one statement written on the root world's tier 0 (`control.law.adopt`), on the person's own credential: the statement
/// stands on the revision the person read, its relation is declared durable and human-authored (a relation it had is changed in
/// place, so its identity stays) and placed where the person says, and the floor is rendered again. Where the floor cannot be
/// rendered (another hand wrote it, a quote moved, or it would pass its budget), the adoption stands and the receipt carries the
/// floor's refusal in its words.
pub fn adopt_with_token_at(
    central: &Path,
    input: &Value,
    token: Option<&str>,
    now: u64,
    home: Option<&Path>,
) -> io::Result<Value> {
    let path = statement_path(input)?;
    let expected = input
        .get("expected_revision")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            source::invalid("expected_revision is the statement's revision as you read it")
        })?;
    let place = place_given(input)?;
    let files = file_names(input, false)?;
    let scope = ground_scope(central)?;
    let principal = authority::authenticate(
        &scope,
        token,
        LAW_ADOPT,
        input
            .get("expected_authority_revision")
            .and_then(Value::as_str),
        now,
    )?;
    principal.require_human()?;
    let _locks = source::lock(&scope)?;
    let bytes = match crate::source_safety::read(&scope.root, &path) {
        Ok(text) => text.into_bytes(),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!(
                    "{path} is not written yet: write the statement on tier 0 first, then adopt it"
                ),
            ))
        }
        Err(error) => return Err(error),
    };
    let revision = crate::source_safety::content_revision_bytes(&bytes);
    if revision != expected {
        return Err(source::conflict(format!(
            "{path} changed since you read it (it stands at {revision}): read it again before adopting it. Nothing was adopted."
        )));
    }
    let (mut relations, basis) = scope.relations()?;
    let member = crate::source_safety::normal_member_key(&path)?;
    let entries = relations["relations"]
        .as_array_mut()
        .ok_or_else(|| source::invalid("invalid source relations"))?;
    let recognition = format!("control-law-adoption:{}", principal.principal_ref);
    let mut adopted_before = false;
    let mut placed = place.unwrap_or(Place::Index);
    let mut found = false;
    for entry in entries.iter_mut() {
        let same = entry["path"]
            .as_str()
            .map(crate::source_safety::normal_member_key)
            .transpose()?
            .is_some_and(|key| key == member);
        if !same {
            continue;
        }
        found = true;
        let held = Place::of(entry["floor"].as_str());
        placed = place.unwrap_or(held);
        adopted_before = entry["standing"] == "durable-source"
            && entry["provenance"] == "human-authored"
            && entry["recognition"] == recognition.as_str()
            && placed == held;
        entry["provenance"] = json!("human-authored");
        entry["standing"] = json!("durable-source");
        let mut roles: Vec<Value> = entry["roles"].as_array().cloned().unwrap_or_default();
        if !roles.iter().any(|role| role == GOVERNANCE_ROLE) {
            roles.push(json!(GOVERNANCE_ROLE));
        }
        entry["roles"] = Value::Array(roles);
        if entry["treatment"].as_str().is_none_or(str::is_empty) {
            entry["treatment"] = json!("retain-native-in-place");
        }
        match (placed, entry.as_object_mut()) {
            (Place::Index, Some(fields)) => {
                fields.remove("floor");
            }
            (_, Some(fields)) => {
                fields.insert("floor".into(), json!(placed.as_str()));
            }
            (_, None) => {}
        }
        entry["recognition"] = json!(recognition);
        entry["recorded_at_unix_seconds"] = json!(now);
    }
    if !found {
        let mut entry = json!({
            "ref": scope.source_ref(&path), "path": path, "roles": [GOVERNANCE_ROLE],
            "provenance": "human-authored", "standing": "durable-source", "treatment": "retain-native-in-place",
            "recognition": recognition, "recorded_at_unix_seconds": now,
        });
        if placed != Place::Index {
            entry["floor"] = json!(placed.as_str());
        }
        entries.push(entry);
    }
    if !adopted_before {
        temporal::save_relations(&scope, &relations, &basis)?;
        scope.reconcile(
            Some((&principal.principal_ref, &principal.actor_kind, None)),
            &[scope.source_ref(&path)],
        )?;
    }
    let floor = match render_locked(&scope, &files, home) {
        Ok(receipt) => receipt,
        Err(error) if error.kind() == io::ErrorKind::PermissionDenied => {
            json!({"schema": FLOOR_RENDER_SCHEMA, "floor": FLOOR_FILE, "changed": false, "refused": error.to_string()})
        }
        Err(error) => return Err(error),
    };
    Ok(json!({
        "schema": LAW_ADOPTION_SCHEMA,
        "statement": {"path": path, "file": path.strip_prefix(&format!("{GOVERNANCE_DIR}/")).unwrap_or(&path),
            "revision": revision, "standing": "durable", "provenance": "human-authored", "floor": placed.as_str()},
        "adopted": !adopted_before,
        "adopted_by": principal.principal_ref,
        "floor": floor,
        "automatic_agent_or_model_invocation": false,
    }))
}

fn root_of(
    action: &str,
    context: &ActionExecutionContext<'_>,
) -> Result<std::path::PathBuf, ActionResult> {
    resolve_central_root(context.root_options)
        .map(|root| root.path)
        .map_err(|message| {
            ActionResult::failure(Some(action), ResultStatus::InvalidInput, message, None)
        })
}

/// The person's home, where Codex keeps its global instructions and under which the floor names the root.
fn home() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|home| home.is_absolute())
        .map(|home| home.canonicalize().unwrap_or(home))
}

fn answer(action: &str, outcome: io::Result<Value>) -> ActionResult {
    match outcome {
        Ok(value) => ActionResult::success(action, value),
        Err(error) => {
            let status = match error.kind() {
                io::ErrorKind::InvalidInput | io::ErrorKind::InvalidData => {
                    ResultStatus::InvalidInput
                }
                io::ErrorKind::PermissionDenied | io::ErrorKind::NotFound => {
                    ResultStatus::UnavailableCapability
                }
                io::ErrorKind::AlreadyExists => ResultStatus::VerificationFailure,
                _ => ResultStatus::InternalFailure,
            };
            ActionResult::failure(Some(action), status, error.to_string(), None)
        }
    }
}

fn render_action(
    _: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    match root_of(RENDER, context) {
        Ok(root) => answer(RENDER, render(&root, input, home().as_deref())),
        Err(failure) => failure,
    }
}

fn read_action(
    _: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    match root_of(READ, context) {
        Ok(root) => answer(READ, read(&root, input, home().as_deref())),
        Err(failure) => failure,
    }
}

fn adopt_action(
    _: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    match root_of(LAW_ADOPT, context) {
        Ok(root) => {
            let token = std::env::var("CENTRAL_NATIVE_TOKEN").ok();
            answer(
                LAW_ADOPT,
                adopt_with_token_at(&root, input, token.as_deref(), now(), home().as_deref()),
            )
        }
        Err(failure) => failure,
    }
}

fn input(name: &str, input_type: &str, required: bool) -> ActionInputDefinition {
    ActionInputDefinition {
        name: name.to_owned(),
        input_type: input_type.to_owned(),
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
        required_ports: vec![],
        availability: ActionAvailability {
            available: true,
            reason: None,
        },
    }
}

/// Register the floor's two Actions and the law's adoption in Central's Action field.
pub fn register_floor_actions(registry: &mut ActionRegistry) {
    let actions: [(ActionDescriptor, crate::action::ActionHandler); 3] = [
        (
            descriptor(
                RENDER,
                "Render the root world's floor",
                "Render AGENTS.md at the Central root, the doors-only floor: the field, the oi front door, load by act (the law through ctrl control index), the never-do card and the session's end, then only the statements whose relation places them on the floor (floor: kernel in full, floor: load as a line under Load by act); every other statement is carried by the door. A sentence it quotes from one of the World's statements is quoted verbatim, left out where the World keeps no such statement, and refused where the statement no longer holds it. The header names the renderer and one revision over every statement it answers to; the same law renders the same bytes. Each name in files (CLAUDE.md, GEMINI.md, ...) is made a link to AGENTS.md, and where the person keeps Codex (~/.codex) and the root is that home's own Central world (~/Central), its global AGENTS.md carries one delimited block naming the floor; for any other root the Codex answer says why none is written. Refused, changing nothing, where AGENTS.md was written by another hand, where a named file is neither absent nor a link to it, where a declared durable statement is lost, and past the floor's budget of 4096 bytes. A generated-derived write: it writes at the root, not under .central.",
                MutationClass::LocallyMutating,
                FLOOR_RENDER_SCHEMA,
                vec![input("files", "array", false)],
            ),
            render_action,
        ),
        (
            descriptor(
                READ,
                "Read the floors at the root",
                "For each instruction file named in files, at the Central root: rendered (by Central, or by the session strap whose renderer it replaces: its renderer, its revision, the law revision its header names, whether it is current, that is the floor Central renders now, and each statement of the root world's tier 0 with where it is carried, in the floor or by the door), foreign (written by another hand: the governance statements it names by their path under Control/agents/governance/ or by their topic as a heading) or absent. Answers central.floor-reading/v1. Read-only.",
                MutationClass::ReadOnly,
                FLOOR_READING_SCHEMA,
                vec![input("files", "array", true)],
            ),
            read_action,
        ),
        (
            descriptor(
                LAW_ADOPT,
                "Adopt a law statement",
                "Adopt one statement written on the root world's tier 0 (statement: its path under Control/agents/governance/), on the revision you read (expected_revision) and the person's own native credential: its relation is declared durable and human-authored, placed where floor says (kernel: in full on the floor; load: its words as a line under Load by act; index: behind the door; omitted: where its relation had it), and the root world's floor is rendered again (control.floor.render, with the links in files). Where the floor cannot be rendered, the adoption stands and the receipt carries the floor's refusal.",
                MutationClass::LocallyMutating,
                LAW_ADOPTION_SCHEMA,
                vec![
                    input("statement", "string", true),
                    input("expected_revision", "string", true),
                    input("floor", "string", false),
                    input("files", "array", false),
                    input("expected_authority_revision", "string", false),
                ],
            ),
            adopt_action,
        ),
    ];
    for (descriptor, handler) in actions {
        registry
            .register(descriptor, handler)
            .expect("the floor's Action ids are valid and unique");
    }
}

#[cfg(test)]
mod tests;
