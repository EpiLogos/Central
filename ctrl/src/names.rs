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
///   ruled), the soft hyphen, the invisible operators, the interlinear
///   annotation marks, the Mongolian vowel separator;
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
