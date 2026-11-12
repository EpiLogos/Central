//! Readable naming for NOW returns, Flows, and other human-traced field objects.
//!
//! One law, both registers: an object created without an explicit id/name gets
//! a readable descriptor derived from its content (subject/title), suffixed
//! with the local civil date, disambiguated by a counter. Opaque timestamps
//! are never the human-facing identity. `now.py` (root register mirror) keeps
//! byte-level naming parity with these semantics.

use unicode_general_category::{get_general_category, GeneralCategory};

/// The characters that draw nothing, so text holding them reads other than it
/// is (DLW build ruling 2026-11-09; be2i): invisible means it draws nothing.
/// The set is Unicode's own, never a hand-kept list of format characters:
/// - every format character (general category Cf): the bidi controls and
///   marks, the zero-width characters (U+200C and U+200D among them, as
///   ruled: a name, a label or a title never holds one, and prose holds one
///   only in its place, `joiner_kept`), the soft hyphen, the invisible
///   operators, the interlinear annotation marks, the Mongolian vowel
///   separator;
/// - save the format characters that draw a sign: the Arabic number signs,
///   end of ayah and the marks above (U+0600–0605, U+06DD, U+0890, U+0891,
///   U+08E2) and the Kaithi number signs (U+110BD, U+110CD);
/// - the line and paragraph separators U+2028 and U+2029;
/// - the whole tag block U+E0000–E007F, assigned or not;
/// - the letters and signs that draw as blank: the Hangul fillers U+115F,
///   U+1160, U+3164 and U+FFA0, the Braille blank U+2800 and the combining
///   grapheme joiner U+034F.
///
/// A variation selector is a mark, not in it: it chooses how a character is
/// drawn, and an emoji needs it. O-I derives the same set
/// (`desktop/cradle/src/shared/plainText.ts`, which the shell reads through
/// `packages/live-shell/ui/src/central/plainParts.ts`), and both sides pin it
/// to one list with the controls (`continuous_work/invisible-code-points.txt`,
/// the same bytes in both repositories, under one sha256).
pub fn invisible(c: char) -> bool {
    match c {
        '\u{2028}' | '\u{2029}' => true,
        '\u{E0000}'..='\u{E007F}' => true,
        '\u{034F}' | '\u{115F}' | '\u{1160}' | '\u{2800}' | '\u{3164}' | '\u{FFA0}' => true,
        '\u{0600}'..='\u{0605}'
        | '\u{06DD}'
        | '\u{0890}'
        | '\u{0891}'
        | '\u{08E2}'
        | '\u{110BD}'
        | '\u{110CD}' => false,
        c => get_general_category(c) == GeneralCategory::Format,
    }
}

/// Not plain text: a control character (`char::is_control`) or an invisible
/// one (`invisible`). Every name, label and title a person or an agent gives
/// Central is refused for one (be2i ruling 7).
pub fn unplain(c: char) -> bool {
    c.is_control() || invisible(c)
}

/// Not text as Central keeps it, read back: a control character, the rule
/// every name and label was kept under before be2i. Kept text that holds an
/// invisible character reads as it stands (be2i.1): only what is given now,
/// at create, rename or adopt, is refused with the whole set (`unplain`).
/// Where kept text is shown it is shown without the invisible characters
/// (`shown`), and the reading says so once (`kept_note`).
pub fn unkept(c: char) -> bool {
    c.is_control()
}

/// Kept text as it is shown: without the characters that draw nothing, and
/// without the whitespace that taking them out leaves at either end.
pub fn shown(text: &str) -> String {
    text.chars()
        .filter(|c| !invisible(*c))
        .collect::<String>()
        .trim()
        .to_owned()
}

const ZWNJ: char = '\u{200C}';
const ZWJ: char = '\u{200D}';

/// Whether the joiner at `at` is part of the words a person sees (be2m, DLW
/// build ruling 2026-11-10; be2m.2, DLW build ruling 2026-11-12): a joiner
/// that changes how the characters beside it are drawn is kept in prose, and
/// one that draws nothing where it stands is refused like any other invisible
/// character. Prose only: the words of a Day's field records and entry parts.
/// A name, a label or a title keeps the whole rule (`unplain`).
///
/// U+200C and U+200D are in their place here and nowhere else, and only
/// between partners that join (be2m.2 item 1):
/// - U+200D between two emoji. Each side is one emoji as it is drawn as an
///   emoji: a character that is Emoji and Extended_Pictographic both (so no
///   digit, no regional indicator, no lone skin tone and no unassigned code
///   point), and either Emoji_Presentation, or followed by U+FE0F, or an
///   Emoji_Modifier_Base followed by an Emoji_Modifier (a skin tone). A
///   character drawn as text unless U+FE0F follows it, a copyright sign or a
///   heart, joins nothing without it;
/// - inside a word of one script that joins or conjoins, in UAX #31's contexts
///   (section 2.3, Layout and Format Control Characters):
///   A1, U+200C between joining letters of one Script (`LJ T* ZWNJ T* RJ`:
///   Joining_Type Left or Dual before it, Right or Dual after it, across
///   Transparent characters); A2, U+200C after a virama and before a letter
///   (`L M* V M1* ZWNJ M1* L`), both letters and the virama of one Script; B,
///   U+200D after a virama and not before a dependent vowel
///   (`L M* V M1* ZWJ (?!D)`), the letter and the virama of one Script. L is a
///   letter, M a nonspacing mark, V a character of Canonical_Combining_Class
///   Virama, M1 a nonspacing mark whose class is not 0, D a character of
///   Indic_Syllabic_Category Vowel_Dependent. The marks between are not asked
///   their Script: a mark is often Inherited or Common. A virama has the
///   Script of the letters it is written with, so a letter of a script that
///   has none (a Latin one) carries none here.
///
/// The properties are Unicode's, read from the ICU data the lock pins
/// (`icu_properties` 2.3.0 with its compiled data, Unicode 17.0), never from
/// ranges kept by hand.
pub fn joiner_kept(chars: &[char], at: usize) -> bool {
    use icu_properties::props::{
        CanonicalCombiningClass, Emoji, EmojiModifier, EmojiModifierBase, EmojiPresentation,
        ExtendedPictographic, GeneralCategory as Category, GeneralCategoryGroup,
        IndicSyllabicCategory, JoiningType, Script,
    };
    use icu_properties::{CodePointMapData, CodePointSetData};
    let Some(&joiner) = chars.get(at).filter(|c| matches!(**c, ZWNJ | ZWJ)) else {
        return false;
    };
    let (before, after) = (&chars[..at], &chars[at + 1..]);
    let emoji = |c: char| {
        CodePointSetData::new::<Emoji>().contains(c)
            && CodePointSetData::new::<ExtendedPictographic>().contains(c)
    };
    let presentation = |c: char| CodePointSetData::new::<EmojiPresentation>().contains(c);
    let tone = |c: char| CodePointSetData::new::<EmojiModifier>().contains(c);
    let takes_tone = |c: char| CodePointSetData::new::<EmojiModifierBase>().contains(c);
    let joining = |c: char| CodePointMapData::<JoiningType>::new().get(c);
    let category = |c: char| CodePointMapData::<Category>::new().get(c);
    let class = |c: char| CodePointMapData::<CanonicalCombiningClass>::new().get(c);
    let script = |c: char| CodePointMapData::<Script>::new().get(c);
    let letter = |c: char| GeneralCategoryGroup::Letter.contains(category(c));
    let mark = |c: char| category(c) == Category::NonspacingMark;
    let reordered_mark = |c: char| mark(c) && class(c) != CanonicalCombiningClass::NotReordered;

    // Between two emoji, each drawn as one.
    if joiner == ZWJ {
        let left = match before {
            [.., base, '\u{FE0F}'] => emoji(*base),
            [.., base, last] if tone(*last) => emoji(*base) && takes_tone(*base),
            [.., last] => emoji(*last) && presentation(*last),
            [] => false,
        };
        let right = match after {
            [next, '\u{FE0F}', ..] => emoji(*next),
            [next, second, ..] if tone(*second) && takes_tone(*next) => emoji(*next),
            [next, ..] => emoji(*next) && presentation(*next),
            [] => false,
        };
        if left && right {
            return true;
        }
    }
    // A1: between joining letters of one script.
    if joiner == ZWNJ {
        let across = |c: &&char| joining(**c) != JoiningType::Transparent;
        let left = before.iter().rev().find(across).filter(|c| {
            matches!(
                joining(**c),
                JoiningType::LeftJoining | JoiningType::DualJoining
            )
        });
        let right = after.iter().find(across).filter(|c| {
            matches!(
                joining(**c),
                JoiningType::RightJoining | JoiningType::DualJoining
            )
        });
        if let (Some(left), Some(right)) = (left, right) {
            if script(*left) == script(*right) {
                return true;
            }
        }
    }
    // After a virama: `L M* V M1*` ends where the joiner stands, and the
    // letter is of the virama's script. A virama is itself a mark of its own
    // class, so each one met on the way back may be the V.
    let mut carrier = None;
    for (index, c) in before.iter().enumerate().rev() {
        if class(*c) == CanonicalCombiningClass::Virama {
            carrier = before[..index]
                .iter()
                .rev()
                .find(|c| !mark(**c))
                .filter(|l| letter(**l) && script(**l) == script(*c));
            if carrier.is_some() {
                break;
            }
        }
        if !reordered_mark(*c) {
            break;
        }
    }
    let Some(carrier) = carrier else {
        return false;
    };
    if joiner == ZWNJ {
        // A2: a letter of the same script follows, across marks of a class.
        after
            .iter()
            .find(|c| !reordered_mark(**c))
            .is_some_and(|c| letter(*c) && script(*c) == script(*carrier))
    } else {
        // B: no dependent vowel follows.
        after.first().is_none_or(|c| {
            CodePointMapData::<IndicSyllabicCategory>::new().get(*c)
                != IndicSyllabicCategory::VowelDependent
        })
    }
}

/// What an invisible character is, in a person's terms, so words left out for
/// one can be given again without it (be2m item 7).
fn invisible_kind(c: char) -> &'static str {
    use icu_properties::{props::BidiControl, CodePointSetData};
    match c {
        '\u{2028}' | '\u{2029}' => "a line or paragraph separator",
        '\u{00AD}' => "a soft hyphen",
        '\u{FEFF}' => "a byte order mark",
        '\u{200B}' => "a zero-width space",
        ZWNJ | ZWJ => "a joiner outside an emoji or a joined word",
        '\u{E0000}'..='\u{E007F}' => "a tag character",
        '\u{034F}' | '\u{115F}' | '\u{1160}' | '\u{2800}' | '\u{3164}' | '\u{FFA0}' => {
            "a blank filler"
        }
        c if CodePointSetData::new::<BidiControl>().contains(c) => "a bidi control or mark",
        _ => "a format character that draws nothing",
    }
}

/// What stands in the way of keeping the character at `at` in prose, or None
/// when it reads as it is: a control character but a line break or a tab, a
/// `\r` no `\n` follows, or an invisible character (`invisible`) that is not a
/// joiner in its place (`joiner_kept`), named by its kind.
fn prose_fault_at(chars: &[char], at: usize) -> Option<&'static str> {
    match chars[at] {
        '\n' | '\t' => None,
        '\r' if chars.get(at + 1) == Some(&'\n') => None,
        '\r' => Some("a carriage return with no line break after it"),
        c if c.is_control() => Some("a control character"),
        c if invisible(c) && !joiner_kept(chars, at) => Some(invisible_kind(c)),
        _ => None,
    }
}

/// What stands in the way of keeping prose as it is given, or None when it
/// reads as it is (be2m): the first character in the way (`prose_fault_at`),
/// named by its kind.
pub fn prose_fault(text: &str) -> Option<&'static str> {
    let chars: Vec<char> = text.chars().collect();
    (0..chars.len()).find_map(|at| prose_fault_at(&chars, at))
}

/// Prose as it is shown (be2m item 4; be2m.2 item 2): without the characters
/// that stand in the way of keeping it (`prose_fault_at`) and without the
/// whitespace at either end, so a joiner in its place stays, a family emoji is
/// shown whole and a joined word as it is written. A control character or a
/// line or paragraph separator in the way is shown as a space, since it
/// stood between words; any other character in the way draws nothing and is
/// taken out. What is left is read again until nothing more goes: a joiner
/// whose partner went with what was taken out goes too, so the shown form
/// holds nothing the rule refuses (`prose_fault` finds nothing in it), and
/// showing it again changes nothing.
pub fn shown_prose(text: &str) -> String {
    let mut shown: Vec<char> = text.chars().collect();
    loop {
        let left: String = (0..shown.len())
            .filter_map(|at| match (prose_fault_at(&shown, at), shown[at]) {
                (None, c) => Some(c),
                (Some(_), c) if c.is_control() || matches!(c, '\u{2028}' | '\u{2029}') => Some(' '),
                (Some(_), _) => None,
            })
            .collect();
        let left: Vec<char> = left.trim().chars().collect();
        if left == shown {
            return left.into_iter().collect();
        }
        shown = left;
    }
}

/// A short form of a ref, for a name that shows nothing else (be2j ruling 5): four base-36 digits of its FNV-1a hash (32 bits, over
/// its UTF-16 code units), the same digits O-I's field gives the same ref (`refMark`,
/// `packages/live-shell/ui/src/projections/atlas/system/fieldModel.ts`), so one thing is called alike on both sides. It is a mark,
/// not a key: two refs may share one.
pub fn ref_mark(ref_: &str) -> String {
    let mut hash: u32 = 0x811c_9dc5;
    for unit in ref_.encode_utf16() {
        hash = (hash ^ u32::from(unit)).wrapping_mul(0x0100_0193);
    }
    let mut digits = Vec::new();
    loop {
        digits.push(char::from_digit(hash % 36, 36).expect("a base-36 digit"));
        hash /= 36;
        if hash == 0 {
            break;
        }
    }
    digits.resize(digits.len().max(4), '0');
    digits[..4].iter().rev().collect()
}

/// What a thing is called when its name shows nothing (be2j ruling 5): "unnamed", what it is, and its ref's short form
/// (`ref_mark`). Never blank, never the ref itself.
pub fn unnamed(what: &str, ref_: &str) -> String {
    format!("unnamed {what} {}", ref_mark(ref_))
}

/// Kept text as it is shown (`shown`), or what the thing is called when nothing else is left (`unnamed`).
pub fn shown_or_unnamed(text: &str, what: &str, ref_: &str) -> String {
    Some(shown(text))
        .filter(|shown| !shown.is_empty())
        .unwrap_or_else(|| unnamed(what, ref_))
}

/// What a reading says, once, of kept text that holds a character the set now
/// refuses: what it is, that it is shown without them, and how to give it
/// again. None when it holds none.
pub fn kept_note(text: &str, what: &str, again: &str) -> Option<String> {
    text.chars().any(invisible).then(|| {
        format!(
            "{what} holds characters Central now refuses, which draw nothing; it is shown without them. {again}"
        )
    })
}

/// Reduce free text to a filesystem-safe kebab-case slug.
///
/// Up to `max_words` words contribute; each word keeps its ASCII alphanumerics
/// (lowercased), words join with `-`. Empty input yields an empty string so the
/// caller can choose a meaningful fallback.
pub fn slugify(text: &str, max_words: usize) -> String {
    let mut parts: Vec<String> = Vec::new();
    for word in text.split_whitespace() {
        if parts.len() >= max_words {
            break;
        }
        let cleaned: String = word
            .chars()
            .filter(|ch| ch.is_ascii_alphanumeric())
            .map(|ch| ch.to_ascii_lowercase())
            .collect();
        if !cleaned.is_empty() {
            parts.push(cleaned);
        }
    }
    parts.join("-")
}

/// Local civil date and minute stamp per the field law: local time, never UTC
/// by assumption. Falls back to UTC only if the platform refuses localtime.
pub fn local_civil_stamp() -> (String, String) {
    unsafe {
        let now = libc::time(std::ptr::null_mut());
        let mut tm: libc::tm = std::mem::zeroed();
        if libc::localtime_r(&now, &mut tm).is_null() {
            libc::gmtime_r(&now, &mut tm);
        }
        (
            format!(
                "{:04}-{:02}-{:02}",
                tm.tm_year + 1900,
                tm.tm_mon + 1,
                tm.tm_mday
            ),
            format!("{:02}{:02}", tm.tm_hour, tm.tm_min),
        )
    }
}

/// Build the default human-readable id: `<slug>-<date>` with `-2`, `-3`, …
/// appended until `taken` reports the candidate free.
///
/// `slug` should already be slugified; when it is empty, `fallback` (a kind or
/// type word) names the object instead. Overlong slugs cut at a word boundary.
pub fn descriptive_id(slug: &str, fallback: &str, taken: &dyn Fn(&str) -> bool) -> String {
    let trimmed = if slug.len() > 48 {
        match slug[..48].rfind('-') {
            Some(index) if index > 0 => &slug[..index],
            _ => &slug[..48],
        }
    } else {
        slug
    };
    let base_slug = if trimmed.is_empty() {
        fallback.to_string()
    } else {
        trimmed.to_string()
    };
    let (date, _) = local_civil_stamp();
    let base = format!("{base_slug}-{date}");
    if !taken(&base) {
        return base;
    }
    for counter in 2.. {
        let candidate = format!("{base}-{counter}");
        if !taken(&candidate) {
            return candidate;
        }
    }
    unreachable!("unbounded counter never exhausts")
}

#[cfg(test)]
#[path = "names_joiner_fuzz.rs"]
mod joiner_fuzz;

#[cfg(test)]
mod tests {
    use super::*;

    /// be2i.1: kept text reads under the rule it was kept under, a control
    /// character; it is shown without the characters that draw nothing, and the
    /// reading says so once.
    #[test]
    fn kept_text_reads_under_its_own_rule_and_is_shown_without_what_draws_nothing() {
        for c in ['\u{200D}', '\u{202E}', '\u{3164}', '\u{E0041}'] {
            assert!(unplain(c) && !unkept(c), "{c:?}");
        }
        assert!(unkept('\u{7}') && unkept('\n'));
        assert_eq!(shown("\u{200B} Rosa\u{200D}rium\u{2060}"), "Rosarium");
        assert_eq!(shown("\u{3164}"), "");
        assert_eq!(shown("Rosarium"), "Rosarium");
        assert_eq!(
            kept_note("Rosarium", "This world's name", "Rename it."),
            None
        );
        assert_eq!(
            kept_note("Rosa\u{200D}rium", "This world's name", "Rename it.").as_deref(),
            Some("This world's name holds characters Central now refuses, which draw nothing; it is shown without them. Rename it.")
        );
    }

    /// be2j ruling 5 and items 14-16: a name that shows nothing is called "unnamed", what it is, and its ref's short form, the same
    /// digits O-I's field gives the same ref (its `refMark`, run on these refs).
    #[test]
    fn a_name_that_shows_nothing_is_called_unnamed_with_its_refs_short_form_as_o_i_calls_it() {
        for (ref_, mark) in [
            ("agent:guardian", "8p9z"),
            ("world:mw69sc3s:.200b..3164.", "3sl2"),
            ("project:garden", "iwz9"),
            ("geo:\u{3164}", "ad5w"),
            ("\u{1F339}", "rjvq"),
            ("", "ntfp"),
        ] {
            assert_eq!(ref_mark(ref_), mark, "{ref_:?}");
        }
        assert_eq!(unnamed("agent", "agent:guardian"), "unnamed agent 8p9z");
        assert_eq!(
            shown_or_unnamed("\u{200B}\u{3164}", "agent", "agent:guardian"),
            "unnamed agent 8p9z"
        );
        assert_eq!(
            shown_or_unnamed(" Guar\u{200D}dian", "agent", "agent:guardian"),
            "Guardian"
        );
    }

    /// be2j ruling 7 (DLW build ruling 2026-11-09): the raw-bidi guard in Central's verify job, run as CI runs it, refuses a raw
    /// override or isolate in tracked text, passes a tree that writes one as its escape, and passes nothing outside a checkout.
    /// Each of the nine is planted in a tree of its own (be2j.c2 item 3, DLW build ruling 2026-11-10): the five embeddings and
    /// overrides, U+202A to U+202E, and the four isolates, U+2066 to U+2069, so a pattern dropped from the step is a red test.
    #[test]
    fn the_raw_bidi_guard_refuses_a_raw_override_passes_a_clean_tree_and_nothing_outside_a_checkout(
    ) {
        let workflow = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../.github/workflows/verify.yml"
        ))
        .unwrap();
        let step = workflow
            .split("- name: Guard tracked source — no raw bidi override or isolate\n")
            .nth(1)
            .expect("verify.yml carries the guard step");
        let block: String = step
            .lines()
            .skip_while(|line| line.trim() != "run: |")
            .skip(1)
            .take_while(|line| line.starts_with("          "))
            .map(|line| format!("{}\n", &line[10..]))
            .collect();
        assert!(block.contains("git grep -lIF"), "{block}");
        let temp = tempfile::tempdir().unwrap();
        let top = temp.path().canonicalize().unwrap();
        let shell = |dir: &std::path::Path, program: &str, args: &[&str]| {
            std::process::Command::new(program)
                .args(args)
                .current_dir(dir)
                .env_clear()
                .envs(std::env::vars().filter(|(key, _)| !key.starts_with("GIT_")))
                .env("HOME", &top)
                .env("GIT_CEILING_DIRECTORIES", &top)
                .output()
                .unwrap()
        };
        let tree = |name: &str, text: &str, checkout: bool| {
            let dir = top.join(name);
            std::fs::create_dir(&dir).unwrap();
            std::fs::write(dir.join("notes.md"), text).unwrap();
            if checkout {
                assert!(shell(&dir, "git", &["init", "-q"]).status.success());
                assert!(shell(&dir, "git", &["add", "notes.md"]).status.success());
            }
            dir
        };
        let planted: Vec<(u32, std::path::PathBuf)> = ('\u{202A}'..='\u{202E}')
            .chain('\u{2066}'..='\u{2069}')
            .map(|bidi| {
                let code = bidi as u32;
                let dir = tree(
                    &format!("raw-{code:04x}"),
                    &format!("Hast{bidi}ings\n"),
                    true,
                );
                (code, dir)
            })
            .collect();
        assert_eq!(planted.len(), 9);
        let clean = tree("clean", "Hast\\u{202E}ings\n", true);
        let none = tree("none", "Hast\u{202E}ings\n", false);
        let guard = |dir: &std::path::Path| {
            let ran = shell(
                dir,
                "bash",
                &["--noprofile", "--norc", "-eo", "pipefail", "-c", &block],
            );
            (
                ran.status.code(),
                String::from_utf8_lossy(&ran.stdout).into_owned(),
            )
        };
        for (bidi, dir) in &planted {
            let (code, said) = guard(dir);
            assert_eq!(code, Some(1), "U+{bidi:04X}: {said}");
            assert!(
                said.contains("a raw bidi override or isolate in tracked source"),
                "U+{bidi:04X}: {said}"
            );
        }
        assert_eq!(guard(&clean).0, Some(0));
        let (code, said) = guard(&none);
        assert_eq!(code, Some(1), "{said}");
        assert!(said.contains("could not read the tracked source"), "{said}");
    }

    /// be2m (DLW build ruling 2026-11-10): a joiner is kept where it changes how the characters beside it are drawn,
    /// between two emoji and in UAX #31's contexts A1, A2 and B, and nowhere else; prose is shown with it in its place.
    #[test]
    fn a_joiner_is_kept_between_two_emoji_and_inside_a_joined_word_and_nowhere_else() {
        let at = |text: &str| {
            let chars: Vec<char> = text.chars().collect();
            let at = chars
                .iter()
                .position(|c| matches!(*c, ZWNJ | ZWJ))
                .expect("the text holds a joiner");
            joiner_kept(&chars, at)
        };
        // between two emoji, each drawn as one: with its variation selector, with its skin tone, or an emoji as it stands
        assert!(at("\u{1F468}\u{200D}\u{1F469}"));
        assert!(at("\u{2764}\u{FE0F}\u{200D}\u{1F525}"));
        assert!(at("\u{1F9D1}\u{1F3FD}\u{200D}\u{1F4BB}"));
        assert!(at("\u{1F3F3}\u{FE0F}\u{200D}\u{1F308}"));
        assert!(!at("\u{1F468}\u{200D}a") && !at("a\u{200D}\u{1F468}"));
        assert!(!at("\u{200D}\u{1F468}") && !at("\u{1F468}\u{200D}"));
        assert!(!at("\u{FE0F}\u{200D}\u{1F525}") && !at("a\u{1F3FD}\u{200D}\u{1F4BB}"));
        assert!(!at("\u{1F468}\u{200C}\u{1F469}"), "ZWNJ joins no emoji");
        // be2m.2 item 1 (DLW build ruling 2026-11-12): both sides are emoji, drawn as emoji; no other pictograph, no
        // unassigned code point, and no character that is an emoji only with a variation selector it does not have
        assert!(
            at("\u{1F3F4}\u{200D}\u{2620}\u{FE0F}"),
            "the selector after"
        );
        assert!(
            at("\u{26F9}\u{1F3FD}\u{200D}\u{2640}\u{FE0F}"),
            "a tone on a base"
        );
        assert!(at("\u{1F468}\u{200D}\u{1F9D1}\u{1F3FD}") && at("\u{1F468}\u{200D}\u{1F9B0}"));
        assert!(at("\u{26F9}\u{FE0F}\u{200D}\u{26F9}\u{1F3FD}"));
        assert!(!at("\u{A9}\u{200D}\u{AE}"), "two pictographs drawn as text");
        assert!(!at("\u{2764}\u{200D}\u{1F525}") && !at("\u{1F3F4}\u{200D}\u{2620}"));
        assert!(!at("\u{1F468}\u{200D}\u{26F9}") && !at("\u{26F9}\u{200D}\u{1F468}"));
        assert!(!at("\u{1F02C}\u{200D}\u{1F469}") && !at("\u{1F469}\u{200D}\u{1F02C}"));
        assert!(
            !at("\u{1F02C}\u{FE0F}\u{200D}\u{1F469}") && !at("\u{1F469}\u{200D}\u{1F02C}\u{FE0F}"),
            "an unassigned pictograph is no emoji, with a selector or without"
        );
        assert!(
            !at("\u{2605}\u{FE0F}\u{200D}\u{1F469}"),
            "a star is no emoji"
        );
        assert!(
            !at("\u{1F1EC}\u{200D}\u{1F1E7}")
                && !at("#\u{200D}*")
                && !at("1\u{FE0F}\u{200D}2\u{FE0F}")
        );
        assert!(!at("\u{1F3FD}\u{200D}\u{1F4BB}") && !at("\u{1F4BB}\u{200D}\u{1F3FD}"));
        assert!(
            !at("\u{1F525}\u{1F3FD}\u{200D}\u{1F4BB}") && !at("\u{2764}\u{1F3FD}\u{200D}\u{1F4BB}"),
            "a tone on an emoji that takes none"
        );
        assert!(
            !at("\u{1F468}\u{FE0F}\u{1F3FD}\u{200D}\u{1F4BB}")
                && !at("\u{1F468}\u{1F3FD}\u{FE0F}\u{200D}\u{1F4BB}"),
            "one selector or one tone, not both"
        );
        assert!(
            !at("\u{2764}\u{FE0E}\u{200D}\u{1F525}"),
            "the text selector"
        );
        assert!(
            !at("\u{1F468}\u{200D}\u{FE0F}\u{1F469}"),
            "no selector is passed over after the joiner"
        );
        assert!(!at("\u{1F468}\u{200D}\u{2620}a") && !at("\u{1F468}\u{200D}\u{26F9}a"));
        assert!(
            !at("\u{1F468}\u{200D}\u{2764}\u{1F3FD}"),
            "a tone after an emoji that takes none"
        );
        // A1: Left or Dual joining before, Right or Dual after, across Transparent characters
        assert!(at("\u{06CC}\u{200C}\u{062E}"));
        assert!(at("\u{0628}\u{064E}\u{200C}\u{064E}\u{0628}"));
        assert!(at("\u{0628}\u{200C}\u{0627}"), "an alef joins on its right");
        assert!(
            !at("\u{0627}\u{200C}\u{0628}"),
            "an alef joins nothing after it"
        );
        assert!(!at("\u{0628}\u{200C}a") && !at("a\u{200C}\u{0628}") && !at("pay\u{200C}pal"));
        assert!(!at("\u{0628}\u{200C}") && !at("\u{200C}\u{0628}"));
        assert!(
            at("\u{A872}\u{200C}\u{A840}") && !at("\u{A840}\u{200C}\u{A872}"),
            "a letter that joins only after it (Phags-pa superfixed ra) joins nothing before it"
        );
        // be2m.2 item 1: the two joining letters are of one script; item 3: a tatweel causes a join and is no joining
        // letter
        assert!(at("\u{07CA}\u{200C}\u{07CB}") && at("\u{1820}\u{200C}\u{1821}"));
        assert!(at("\u{0628}\u{200C}\u{06CC}") && at("\u{0712}\u{0730}\u{200C}\u{0712}"));
        assert!(!at("\u{0628}\u{200C}\u{1820}") && !at("\u{1820}\u{200C}\u{0628}"));
        assert!(!at("\u{A872}\u{200C}\u{0628}") && !at("\u{0628}\u{200C}\u{0712}"));
        assert!(
            at("\u{0628}\u{0301}\u{200C}\u{0628}"),
            "a mark between is not asked its script"
        );
        assert!(!at("\u{0640}\u{200C}\u{0628}") && !at("\u{0628}\u{200C}\u{0640}"));
        assert!(
            !at("\u{07FA}\u{200C}\u{07CA}") && !at("\u{07CA}\u{200C}\u{07FA}"),
            "nor is a lajanyalan, which is of the letters' own script"
        );
        assert!(
            !at("\u{0628}\u{200D}\u{0628}"),
            "ZWJ has no joining-letter context"
        );
        // A2 and B: after a letter, its marks, a virama and marks of a class
        assert!(at("\u{0915}\u{094D}\u{200C}\u{0937}") && at("\u{0915}\u{094D}\u{200D}\u{0937}"));
        assert!(at("\u{0915}\u{093C}\u{094D}\u{200D}\u{0937}"), "L M* V");
        assert!(at("\u{0915}\u{094D}\u{093C}\u{200D}\u{0937}"), "V M1*");
        assert!(at("\u{0915}\u{094D}\u{200C}\u{093C}\u{0937}"), "ZWNJ M1* L");
        assert!(at("\u{0915}\u{094D}\u{200D}") && at("\u{0915}\u{094D}\u{200D} "));
        assert!(!at("\u{0915}\u{094D}\u{200C}") && !at("\u{0915}\u{094D}\u{200C} \u{0915}"));
        assert!(!at("\u{0915}\u{094D}\u{200D}\u{093E}"), "a dependent vowel");
        assert!(at("\u{0915}\u{094D}\u{200C}\u{0937}\u{093E}"));
        assert!(!at("\u{0915}\u{200D}\u{0937}") && !at("\u{0915}\u{200C}\u{0937}"));
        assert!(!at("\u{094D}\u{200D}\u{0937}") && !at("1\u{094D}\u{200D}\u{0937}"));
        assert!(!at("\u{0915}\u{094D}a\u{200D}\u{0937}"));
        // be2m.2 item 1: the letter that carries the virama is of the virama's script, and in A2 the letter after is too
        assert!(at("\u{09B0}\u{09CD}\u{200D}\u{09AF}") && at("\u{0D9A}\u{0DCA}\u{200D}\u{0DBA}"));
        assert!(at("\u{0995}\u{09CD}\u{200C}\u{09B7}"));
        assert!(!at("pay\u{094D}\u{200D}pal") && !at("a\u{094D}\u{200C}a"));
        assert!(!at("a\u{094D}\u{200D}") && !at("\u{0995}\u{094D}\u{200D}\u{0937}"));
        assert!(
            !at("\u{0967}\u{094D}\u{200D}"),
            "a digit of the virama's script is no letter"
        );
        assert!(!at("\u{0915}\u{094D}\u{200C}a") && !at("\u{0915}\u{094D}\u{200C}\u{0995}"));
        assert!(!at("\u{0915}\u{094D}\u{200C}\u{3164}") && !at("a\u{094D}\u{200C}\u{0937}"));
        assert!(
            at("\u{0915}\u{094D}\u{200D}a") && at("\u{0915}\u{0301}\u{094D}\u{0301}\u{200D}"),
            "B asks nothing of what follows but that it is no dependent vowel, and a mark between is not asked its script"
        );
        // be2m.2 item 3, the edges the review of #350 found unpinned
        assert!(
            !at("\u{0915}\u{094D}\u{0902}\u{200D}")
                && !at("\u{0915}\u{094D}\u{0902}\u{200C}\u{0937}"),
            "a mark of no class between the virama and the joiner"
        );
        assert!(
            at("\u{0915}\u{0902}\u{094D}\u{200D}")
                && at("\u{0915}\u{0902}\u{094D}\u{200C}\u{0937}"),
            "a mark of no class between the letter and its virama"
        );
        assert!(
            !at("\u{0915}\u{094D}\u{200C}\u{0902}\u{0937}"),
            "a mark of no class between the ZWNJ and the letter"
        );
        assert!(
            at("\u{1703}\u{1715}\u{1714}\u{200D}") && at("\u{1703}\u{1715}\u{1714}\u{200C}\u{1703}"),
            "the nearer virama stands on no letter (a pamudpod is no nonspacing mark), the one before it does"
        );
        assert!(
            !at("\u{0915}\u{1B44}\u{094D}\u{200D}") && !at("a\u{1715}\u{1714}\u{200D}"),
            "and a virama before it of another script than the letter carries nothing"
        );
        // a place that is no joiner, or past the end
        assert!(!joiner_kept(&['a', 'b'], 0) && !joiner_kept(&['a'], 3));
        assert!(
            !joiner_kept(&['\u{0915}', '\u{094D}', '\u{AD}', '\u{0937}'], 2),
            "a soft hyphen after a virama is no joiner"
        );

        // what stands in the way, by its kind; a line break, a tab and a \r\n stand
        assert_eq!(
            prose_fault("One.\n\tTwo.\r\nThree \u{1F468}\u{200D}\u{1F469}."),
            None
        );
        for (text, kind) in [
            ("a\rb", "a carriage return with no line break after it"),
            ("a\r", "a carriage return with no line break after it"),
            ("a\u{7}b", "a control character"),
            ("a\u{85}b", "a control character"),
            ("a\u{2029}b", "a line or paragraph separator"),
            ("co\u{AD}operate", "a soft hyphen"),
            ("\u{FEFF}a", "a byte order mark"),
            ("a\u{200B}b", "a zero-width space"),
            (
                "pay\u{200C}pal",
                "a joiner outside an emoji or a joined word",
            ),
            ("pal\u{200D}", "a joiner outside an emoji or a joined word"),
            ("a\u{E0000}", "a tag character"),
            ("a\u{E007F}", "a tag character"),
            ("a\u{034F}b", "a blank filler"),
            ("a\u{2800}b", "a blank filler"),
            ("a\u{FFA0}b", "a blank filler"),
            ("a\u{061C}b", "a bidi control or mark"),
            ("a\u{200F}b", "a bidi control or mark"),
            ("a\u{2066}b", "a bidi control or mark"),
            ("a\u{2060}b", "a format character that draws nothing"),
            ("a\u{180E}b", "a format character that draws nothing"),
            ("a\u{FFF9}b", "a format character that draws nothing"),
        ] {
            assert_eq!(prose_fault(text), Some(kind), "{text:?}");
        }
        assert_eq!(
            prose_fault("a\u{AD}b\u{200B}c"),
            Some("a soft hyphen"),
            "the first one found"
        );
        assert_eq!(
            prose_fault("\u{0600}\u{06DD}"),
            None,
            "a format character that draws a sign"
        );

        // shown: a joiner in its place stays, every other invisible character goes, and what is left is read again
        assert_eq!(
            shown_prose(" \u{200B}\u{1F468}\u{200D}\u{1F469} pay\u{200C}pal\u{2060} "),
            "\u{1F468}\u{200D}\u{1F469} paypal"
        );
        assert_eq!(
            shown_prose("\u{1F468}\u{200D}\u{200B}\u{1F469}"),
            "\u{1F468}\u{1F469}",
            "taking the zero-width space out makes no join that was not drawn"
        );
        // be2m.2 item 2: the shown form holds nothing the rule refuses. A joiner whose place went with what was taken out
        // goes too; a control character or a separator in the way is shown as a space.
        for (text, shown) in [
            (
                "\u{0915}\u{094D}\u{200D}\u{200C}\u{093E}",
                "\u{0915}\u{094D}\u{093E}",
            ),
            (
                "\u{0915}\u{094D}\u{200D}\u{200B}\u{093E}",
                "\u{0915}\u{094D}\u{093E}",
            ),
            ("a\u{094D}\u{200C}\u{0301}\u{3164}", "a\u{094D}\u{0301}"),
            ("\u{3164}\u{094D}\u{200D}", "\u{094D}"),
            ("\u{0915}\u{094D}\u{200D}\u{7}", "\u{0915}\u{094D}\u{200D}"),
            ("a\u{7}b\rc\u{2028}d\r\ne\tf\n", "a b c d\r\ne\tf"),
            (
                "\u{0915}\u{094D}\u{200C}\u{7}\u{0937}",
                "\u{0915}\u{094D} \u{0937}",
            ),
            ("\u{200D}\u{200D}\u{200C}", ""),
        ] {
            assert_eq!(shown_prose(text), shown, "{text:?}");
            assert_eq!(prose_fault(shown), None, "{text:?}");
            assert_eq!(shown_prose(shown), shown, "{text:?}");
        }
        assert_eq!(
            shown("\u{1F468}\u{200D}\u{1F469}"),
            "\u{1F468}\u{1F469}",
            "a name's shown form keeps the whole rule"
        );
        assert!("\u{1F468}\u{200D}\u{1F469}".chars().any(unplain));
    }

    /// be2m items 4 and 5: every case's shown form is its text on one line as prose is shown, a control character or a line or
    /// paragraph separator as a space, a joiner in its place kept; and a case is kept exactly when nothing stands in the way,
    /// named by its kind when something does. The file's sha256 is pinned beside the Day's tests. be2m.2: the shown form is
    /// `shown_prose`'s own, with nothing the rule refuses left in it, and the cases hold the rows the ruling names.
    #[test]
    fn every_joiner_case_is_shown_with_a_joiner_in_its_place_and_names_what_stood_in_the_way() {
        let cases: Vec<serde_json::Value> =
            serde_json::from_str(include_str!("continuous_work/joiner-cases.json")).unwrap();
        assert!(cases.len() >= 60);
        for case in &cases {
            let (name, text) = (&case["name"], case["text"].as_str().unwrap());
            let shown = shown_prose(text);
            assert_eq!(prose_fault(&shown), None, "{name}");
            let shown = shown.split_whitespace().collect::<Vec<_>>().join(" ");
            assert_eq!(shown, case["shown"].as_str().unwrap(), "{name}");
            assert_eq!(prose_fault(text), case["kind"].as_str(), "{name}");
            assert_eq!(case["kept"], prose_fault(text).is_none(), "{name}");
            if text.contains([ZWNJ, ZWJ]) {
                assert!(text.chars().any(unplain), "{name}: a name refuses it");
            }
        }
        let row = |name: &str| {
            cases
                .iter()
                .find(|case| case["name"] == name)
                .unwrap_or_else(|| panic!("the cases hold {name:?}"))
        };
        for (name, kept) in [
            ("a family emoji", true),
            (
                "a Persian word with ZWNJ between joining letters (A1)",
                true,
            ),
            ("a Devanagari conjunct with ZWJ after a virama (B)", true),
            ("ZWNJ between N'Ko letters (A1)", true),
            ("ZWNJ between Mongolian letters (A1)", true),
            ("a virama on a Latin letter, then ZWJ", false),
            (
                "ZWJ between a copyright sign and a registered sign, drawn as text",
                false,
            ),
            (
                "ZWJ after a pictograph that is no assigned character",
                false,
            ),
            ("ZWNJ between joining letters of two scripts (A1)", false),
        ] {
            assert_eq!(row(name)["kept"], kept, "{name}");
        }
    }

    #[test]
    fn slugify_kebab_cases_and_limits_words() {
        assert_eq!(
            slugify("Root NOW/DAY field established!", 6),
            "root-nowday-field-established"
        );
        assert_eq!(
            slugify("one two three four five six seven", 6),
            "one-two-three-four-five-six"
        );
        assert_eq!(slugify("   ", 6), "");
        assert_eq!(slugify("héllo wörld", 6), "hllo-wrld");
    }

    #[test]
    fn descriptive_id_falls_back_and_disambiguates() {
        // The id carries today's local civil date, so the fixture derives the
        // date it is asserting rather than hardcoding one that stops being
        // today tomorrow.
        let (today, _) = local_civil_stamp();
        let existing = [format!("note-{today}"), format!("note-{today}-2")];
        let taken = |candidate: &str| existing.iter().any(|id| id == candidate);
        assert_eq!(
            descriptive_id("", "note", &taken),
            format!("note-{today}-3")
        );
        let free = |_candidate: &str| false;
        let id = descriptive_id("live-smoke", "note", &free);
        assert!(id.starts_with("live-smoke-"));
        assert_eq!(id.matches('-').count(), 4);
    }

    #[test]
    fn local_stamp_is_civil_date_shape() {
        let (date, hhmm) = local_civil_stamp();
        assert_eq!(date.len(), 10);
        assert_eq!(hhmm.len(), 4);
        assert!(date.as_bytes()[4] == b'-' && date.as_bytes()[7] == b'-');
    }
}
