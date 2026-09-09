//! Readable naming for NOW returns, Flows, and other human-traced field objects.
//!
//! One law, both registers: an object created without an explicit id/name gets
//! a readable descriptor derived from its content (subject/title), suffixed
//! with the local civil date, disambiguated by a counter. Opaque timestamps
//! are never the human-facing identity. `now.py` (root register mirror) keeps
//! byte-level naming parity with these semantics.

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
        let existing = vec![
            format!("note-{today}"),
            format!("note-{today}-2"),
        ];
        let taken = |candidate: &str| existing.iter().any(|id| id == candidate);
        assert_eq!(descriptive_id("", "note", &taken), format!("note-{today}-3"));
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
