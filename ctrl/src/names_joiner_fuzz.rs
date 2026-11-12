//! be2m.2 item 4 and be2m.3 item 5 (DLW build rulings 2026-11-12): the fuzz of the review of #350, kept as a test that
//! runs in a few seconds.
//!
//! It makes 114,794 strings, the same ones every run: 12,000 random ones of 1 to 24 characters over the alphabet below
//! (seeded); 50,914 of up to two characters, a joiner and up to two characters over eighteen kinds of neighbour and
//! the text's end; 39,200, every arrangement of up to three characters before a joiner and two after it over the
//! virama's kinds; 936, every arrangement of up to three before a joiner and one after it over a Thai letter and its
//! killer and a Myanmar letter, its stacker and its killer; and 11,744, every listed emoji ZWJ sequence in each of its
//! spellings (2,667) and in six forms that are no spelling of it (9,077).
//!
//! The invariant, checked on every string against the rule written a second time (`ruled`, by position, not
//! `joiner_kept`'s walk; the listed sequences by this file's own reading of the list, each matched character by
//! character from where it starts, not looked up as a spelling):
//! - a joiner is kept exactly where the rule names a context (emoji, A1, A2, B), and nothing is kept where no joiner
//!   stands or past the end;
//! - whatever is kept between emoji stands inside a sequence Unicode lists, in one of its spellings, that stands whole
//!   (be2m.3 item 1), and whatever is kept after a virama is kept after one that conjoins (be2m.3 item 2);
//! - the rule only narrowed: whatever it keeps, the rule of #350 kept (`kept_by_350`);
//! - `prose_fault` names the first character in the way by its kind, and a text with no joiner is judged as it was
//!   before any joiner was kept;
//! - `shown_prose` leaves nothing the rule refuses, takes out nothing a person sees, changes nothing when shown again,
//!   and leaves a text with nothing in the way as it is given, trimmed;
//! - in a text that is kept no joiner stands first, beside another joiner or beside a character that draws nothing, and
//!   a ZWNJ never stands last.

use super::{invisible, joiner_kept, prose_fault, shown_prose, unplain};
use icu_properties::props::{JoiningType, Script};

const ZWNJ: char = '\u{200C}';
const ZWJ: char = '\u{200D}';

/// What the rule asks of one character, read from the same Unicode data.
#[derive(Clone, Copy)]
struct Read {
    pictograph: bool,
    tone: bool,
    joining: JoiningType,
    mark: bool,
    reordered_mark: bool,
    letter: bool,
    virama: bool,
    conjoins: bool,
    dependent_vowel: bool,
    script: Script,
}

fn read(c: char) -> Read {
    use icu_properties::props::{
        CanonicalCombiningClass, EmojiModifier, ExtendedPictographic, GeneralCategory,
        GeneralCategoryGroup, IndicSyllabicCategory,
    };
    use icu_properties::{CodePointMapData, CodePointSetData};
    let category = CodePointMapData::<GeneralCategory>::new().get(c);
    let class = CodePointMapData::<CanonicalCombiningClass>::new().get(c);
    let syllabic = CodePointMapData::<IndicSyllabicCategory>::new().get(c);
    let mark = category == GeneralCategory::NonspacingMark;
    Read {
        pictograph: CodePointSetData::new::<ExtendedPictographic>().contains(c),
        tone: CodePointSetData::new::<EmojiModifier>().contains(c),
        joining: CodePointMapData::<JoiningType>::new().get(c),
        mark,
        reordered_mark: mark && class != CanonicalCombiningClass::NotReordered,
        letter: GeneralCategoryGroup::Letter.contains(category),
        virama: class == CanonicalCombiningClass::Virama,
        conjoins: class == CanonicalCombiningClass::Virama
            && (syllabic == IndicSyllabicCategory::Virama
                || syllabic == IndicSyllabicCategory::InvisibleStacker),
        dependent_vowel: syllabic == IndicSyllabicCategory::VowelDependent,
        script: CodePointMapData::<Script>::new().get(c),
    }
}

/// The letter that carries a virama standing at `v`, across nonspacing marks; with `one_script`, only a letter of the
/// virama's own script.
fn carrier(p: &[Read], v: usize, one_script: bool) -> Option<Read> {
    let mut l = v;
    while l > 0 && p[l - 1].mark {
        l -= 1;
    }
    (l > 0 && p[l - 1].letter && (!one_script || p[l - 1].script == p[v].script)).then(|| p[l - 1])
}

/// The emoji ZWJ sequences Unicode lists, fully qualified, by this file's own reading of the list, under the character
/// each starts with.
fn listed() -> &'static std::collections::BTreeMap<char, Vec<Vec<char>>> {
    static LISTED: std::sync::OnceLock<std::collections::BTreeMap<char, Vec<Vec<char>>>> =
        std::sync::OnceLock::new();
    LISTED.get_or_init(|| {
        let mut listed: std::collections::BTreeMap<char, Vec<Vec<char>>> = Default::default();
        for line in include_str!("continuous_work/emoji-zwj-sequences.txt").lines() {
            if !line.starts_with('#') {
                let sequence = points(
                    &line
                        .split_whitespace()
                        .map(|point| u32::from_str_radix(point, 16).unwrap())
                        .collect::<Vec<u32>>(),
                );
                listed.entry(sequence[0]).or_default().push(sequence);
            }
        }
        listed
    })
}

/// Where a listed sequence ends when the text spells it from `from`: each character of the fully qualified form in
/// turn, a U+FE0F of that form taken where the text has one and passed over where it has none.
fn spelled(sequence: &[char], chars: &[char], from: usize) -> Option<usize> {
    let mut to = from;
    for wanted in sequence {
        if chars.get(to) == Some(wanted) {
            to += 1;
        } else if *wanted != '\u{FE0F}' {
            return None;
        }
    }
    Some(to)
}

/// Whether the joiner at `i` stands inside a listed sequence, in one of its spellings, that stands whole: no ZWJ right
/// before it, and no ZWJ, variation selector U+FE0E or U+FE0F, or skin tone right after it.
fn in_a_listed_sequence(p: &[Read], chars: &[char], i: usize) -> bool {
    (0..i).any(|from| {
        (from == 0 || chars[from - 1] != ZWJ)
            && listed().get(&chars[from]).is_some_and(|sequences| {
                sequences.iter().any(|sequence| {
                    spelled(sequence, chars, from).is_some_and(|to| {
                        to > i
                            && (to == chars.len()
                                || !(chars[to] == ZWJ
                                    || chars[to] == '\u{FE0E}'
                                    || chars[to] == '\u{FE0F}'
                                    || p[to].tone))
                    })
                })
            })
    })
}

/// The rule as be2m.3 words it: which context, if any, keeps the joiner at `i`.
fn ruled(p: &[Read], chars: &[char], i: usize) -> Option<&'static str> {
    let n = chars.len();
    if chars[i] == ZWJ {
        if in_a_listed_sequence(p, chars, i) {
            return Some("emoji");
        }
    } else {
        let mut j = i;
        while j > 0 && p[j - 1].joining == JoiningType::Transparent {
            j -= 1;
        }
        let mut k = i + 1;
        while k < n && p[k].joining == JoiningType::Transparent {
            k += 1;
        }
        if j > 0
            && k < n
            && matches!(
                p[j - 1].joining,
                JoiningType::LeftJoining | JoiningType::DualJoining
            )
            && matches!(
                p[k].joining,
                JoiningType::RightJoining | JoiningType::DualJoining
            )
            && p[j - 1].script == p[k].script
        {
            return Some("a1");
        }
    }
    // `L M* V M1*` then the joiner: every V before it is tried, and V is a virama that conjoins.
    let carriers: Vec<Read> = (0..i)
        .filter(|v| p[*v].conjoins && (v + 1..i).all(|m| p[m].reordered_mark))
        .filter_map(|v| carrier(p, v, true))
        .collect();
    if chars[i] == ZWJ {
        (!carriers.is_empty() && !(i + 1 < n && p[i + 1].dependent_vowel)).then_some("b")
    } else {
        let mut k = i + 1;
        while k < n && p[k].reordered_mark {
            k += 1;
        }
        (k < n && p[k].letter && carriers.iter().any(|carrier| carrier.script == p[k].script))
            .then_some("a2")
    }
}

/// The rule of #350 (Central 5688ccb8), which be2m.2 and be2m.3 narrow: any two pictographs around a ZWJ, the letters'
/// scripts not asked, any character of the virama class.
fn kept_by_350(p: &[Read], chars: &[char], i: usize) -> bool {
    let n = chars.len();
    if chars[i] == ZWJ {
        let mut j = i;
        while j > 0
            && (chars[j - 1] == '\u{FE0F}' || ('\u{1F3FB}'..='\u{1F3FF}').contains(&chars[j - 1]))
        {
            j -= 1;
        }
        if j > 0 && p[j - 1].pictograph && i + 1 < n && p[i + 1].pictograph {
            return true;
        }
    } else {
        let mut j = i;
        while j > 0 && p[j - 1].joining == JoiningType::Transparent {
            j -= 1;
        }
        let mut k = i + 1;
        while k < n && p[k].joining == JoiningType::Transparent {
            k += 1;
        }
        if j > 0
            && k < n
            && matches!(
                p[j - 1].joining,
                JoiningType::LeftJoining | JoiningType::DualJoining
            )
            && matches!(
                p[k].joining,
                JoiningType::RightJoining | JoiningType::DualJoining
            )
        {
            return true;
        }
    }
    let after_virama = (0..i).any(|v| {
        p[v].virama && (v + 1..i).all(|m| p[m].reordered_mark) && carrier(p, v, false).is_some()
    });
    if chars[i] == ZWJ {
        after_virama && !(i + 1 < n && p[i + 1].dependent_vowel)
    } else {
        let mut k = i + 1;
        while k < n && p[k].reordered_mark {
            k += 1;
        }
        after_virama && k < n && p[k].letter
    }
}

/// The kind of each character of the set, by a table of this test's own.
fn kind(c: char) -> &'static str {
    match c as u32 {
        0x2028 | 0x2029 => "a line or paragraph separator",
        0xAD => "a soft hyphen",
        0xFEFF => "a byte order mark",
        0x200B => "a zero-width space",
        0x200C | 0x200D => "a joiner outside an emoji or a joined word",
        0xE0000..=0xE007F => "a tag character",
        0x034F | 0x115F | 0x1160 | 0x2800 | 0x3164 | 0xFFA0 => "a blank filler",
        0x061C | 0x200E | 0x200F | 0x202A..=0x202E | 0x2066..=0x2069 => "a bidi control or mark",
        _ => "a format character that draws nothing",
    }
}

#[derive(Default)]
struct Tally {
    strings: usize,
    kept: std::collections::BTreeMap<&'static str, usize>,
    refused_joiners: usize,
    narrowed: usize,
    shown_twice_read: usize,
    fails: Vec<String>,
}

fn check(chars: &[char], tally: &mut Tally) {
    let n = chars.len();
    tally.strings += 1;
    let text: String = chars.iter().collect();
    let p: Vec<Read> = chars.iter().map(|c| read(*c)).collect();
    let joiner = |c: char| c == ZWNJ || c == ZWJ;
    let context: Vec<Option<&'static str>> = (0..n)
        .map(|i| joiner(chars[i]).then(|| ruled(&p, chars, i)).flatten())
        .collect();
    let mut fails = Vec::new();
    for i in 0..n {
        if joiner_kept(chars, i) != context[i].is_some() {
            fails.push(format!("kept at {i} against the rule's {:?}", context[i]));
        }
        if !joiner(chars[i]) {
            continue;
        }
        match context[i] {
            Some(name) => {
                *tally.kept.entry(name).or_default() += 1;
                if !kept_by_350(&p, chars, i) {
                    fails.push(format!("kept at {i} and refused by #350's rule"));
                }
            }
            None => {
                tally.refused_joiners += 1;
                tally.narrowed += usize::from(kept_by_350(&p, chars, i));
            }
        }
    }
    if joiner_kept(chars, n) || joiner_kept(chars, n + 7) {
        fails.push("kept past the end".into());
    }
    let plain = |i: usize, c: char| match c {
        '\n' | '\t' => None,
        '\r' if chars.get(i + 1) == Some(&'\n') => None,
        '\r' => Some("a carriage return with no line break after it"),
        c if c.is_control() => Some("a control character"),
        c if invisible(c) && context[i].is_none() => Some(kind(c)),
        _ => None,
    };
    let expected = (0..n).find_map(|i| plain(i, chars[i]));
    let fault = prose_fault(&text);
    if fault != expected {
        fails.push(format!(
            "named {fault:?}, the first in the way is {expected:?}"
        ));
    }
    let as_before = (0..n).all(|i| !invisible(chars[i]) && plain(i, chars[i]).is_none());
    if !chars.iter().any(|c| joiner(*c)) && as_before != fault.is_none() {
        fails.push("a text with no joiner is judged otherwise than before".into());
    }
    if chars.iter().any(|c| joiner(*c)) && !text.chars().any(unplain) {
        fails.push("a name would keep the joiner".into());
    }
    // the shown form
    let shown = shown_prose(&text);
    if prose_fault(&shown).is_some() {
        fails.push(format!("shown with {:?} left in it", prose_fault(&shown)));
    }
    if shown_prose(&shown) != shown {
        fails.push("shown again, it changes".into());
    }
    if shown != shown.trim() {
        fails.push("shown with whitespace at an end".into());
    }
    let seen = |text: &str| -> String {
        text.chars()
            .filter(|c| !invisible(*c) && !c.is_control() && !c.is_whitespace())
            .collect()
    };
    if seen(&shown) != seen(&text) {
        fails.push("shown without something a person sees".into());
    }
    let once: String = (0..n)
        .filter(|i| !invisible(chars[*i]) || context[*i].is_some())
        .map(|i| chars[i])
        .collect();
    tally.shown_twice_read += usize::from(
        shown.chars().filter(|c| joiner(*c)).count() != once.chars().filter(|c| joiner(*c)).count(),
    );
    if fault.is_none() {
        if shown != text.trim() {
            fails.push("nothing in the way, and shown otherwise than given".into());
        }
        for i in (0..n).filter(|i| joiner(chars[*i])) {
            let beside = |c: Option<&char>| c.is_some_and(|c| invisible(*c) || c.is_control());
            if i == 0
                || beside(chars.get(i - 1))
                || beside(chars.get(i + 1))
                || (chars[i] == ZWNJ && i + 1 == n)
            {
                fails.push(format!(
                    "a kept joiner at {i} stands first, last or beside what draws nothing"
                ));
            }
        }
    }
    for fail in fails {
        if tally.fails.len() < 20 {
            let points: Vec<String> = chars.iter().map(|c| format!("{:04X}", *c as u32)).collect();
            tally.fails.push(format!("{fail}: {}", points.join(" ")));
        }
    }
}

fn points(list: &[u32]) -> Vec<char> {
    list.iter().map(|c| char::from_u32(*c).unwrap()).collect()
}

/// The review's alphabet (the joiners six times over, so one string in a few holds one), with a Tagalog letter and its
/// two viramas, a Bengali and a Sinhala letter and virama, emoji drawn as text unless a variation selector follows, and
/// the parts of listed sequences (a black flag, a handshake, a staff of Aesculapius).
const ALPHABET: &[u32] = &[
    // the invisible set
    0x200C, 0x200D, 0x200C, 0x200D, 0x200C, 0x200D, 0x200C, 0x200D, 0x200C, 0x200D, 0x200C, 0x200D,
    0x200B, 0xAD, 0xFEFF, 0x2060, 0x202E, 0x202A, 0x202C, 0x200E, 0x200F, 0x2066, 0x2069, 0x061C,
    0xE0001, 0xE0041, 0xE007F, 0xE0000, 0x034F, 0x115F, 0x1160, 0x3164, 0xFFA0, 0x2800, 0x180E,
    0x2028, 0x2029, 0xFFF9, 0x2061, 0x206A, 0x1D173, 0x13430, // controls and breaks
    0x0A, 0x09, 0x0D, 0x0D, 0x0A, 0x07, 0x85, 0x00,
    // variation selectors, emoji, skin tones, regional indicators, keycaps, pictographs that are no emoji
    0xFE0F, 0xFE0F, 0xFE0F, 0xFE0E, 0xE0100, 0x180B, 0x1F468, 0x1F469, 0x1F467, 0x2764, 0x1F525,
    0x1F4BB, 0x1F9D1, 0x1F3F3, 0x1F308, 0x26F9, 0x2640, 0x2620, 0x1F9B0, 0x1F3FB, 0x1F3FD, 0x1F3FF,
    0x1F1EC, 0x1F1E7, 0x23, 0x20E3, 0xA9, 0xAE, 0x2122, 0x1F02C, 0x1FAFF, 0x2605, 0x3030, 0x1F3F4,
    0x1F91D, 0x2695,
    // Arabic, Syriac, N'Ko, Mongolian, Phags-pa, Adlam, Mandaic, Rohingya
    0x0628, 0x0628, 0x0627, 0x06CC, 0x062E, 0x062F, 0x0640, 0x064E, 0x0670, 0x0621, 0x0647, 0x0645,
    0x0710, 0x0712, 0x0711, 0x0730, 0x07CA, 0x07CB, 0x07EB, 0x07FA, 0x1820, 0x1821, 0x180A, 0x1807,
    0xA872, 0xA840, 0x1E900, 0x1E922, 0x0840, 0x10D00,
    // viramas, letters, marks and dependent vowels of the scripts that have them
    0x0915, 0x0937, 0x094D, 0x094D, 0x093C, 0x093E, 0x093F, 0x0941, 0x0902, 0x0995, 0x09CD, 0x09BE,
    0x0D15, 0x0D4D, 0x0D3B, 0x0D9A, 0x0DCA, 0x1000, 0x1039, 0x103A, 0x1780, 0x17D2, 0x1B13, 0x1B44,
    0x0E01, 0x0E3A, 0x0F40, 0x0F84, 0xA807, 0xA806, 0x2D30, 0x2D7F, 0x11013, 0x1107F, 0x11046,
    0x1A20, 0x1A60, 0xA9C0, 0xA98F, 0x1703, 0x1714, 0x1715, // Latin and the rest
    0x61, 0x62, 0x70, 0x5A, 0xE9, 0x0301, 0x0308, 0x0327, 0x31, 0x20, 0x20, 0x2D, 0x2E, 0x05D0,
    0x4E00, 0x3042, 0x03B1, 0x0430,
];

const RANDOM_STRINGS: usize = 12_000;

fn report(name: &str, tally: &Tally) {
    eprintln!(
        "joiner fuzz {name}: {} strings; joiners kept {:?}, refused {} ({} of them kept by #350's rule); shown forms a second reading changed {}",
        tally.strings, tally.kept, tally.refused_joiners, tally.narrowed, tally.shown_twice_read
    );
    assert!(tally.fails.is_empty(), "{:#?}", tally.fails);
}

#[test]
fn random_strings_keep_a_joiner_only_between_partners_that_join_and_are_shown_with_nothing_refused()
{
    let alphabet = points(ALPHABET);
    let mut state: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut below = |n: usize| {
        state ^= state >> 12;
        state ^= state << 25;
        state ^= state >> 27;
        (state.wrapping_mul(0x2545_F491_4F6C_DD1D) % n as u64) as usize
    };
    let mut tally = Tally::default();
    for round in 0..RANDOM_STRINGS {
        let len = 1 + below(if round % 4 == 0 { 24 } else { 9 });
        let chars: Vec<char> = (0..len).map(|_| alphabet[below(alphabet.len())]).collect();
        check(&chars, &mut tally);
    }
    report("random", &tally);
    assert_eq!(tally.strings, RANDOM_STRINGS);
    // Which contexts the random strings reach is theirs to say (the line above); the two tests below reach all four.
    assert!(!tally.kept.is_empty(), "a joiner is kept");
    assert!(tally.narrowed > 0, "a joiner #350's rule kept is refused");
}

#[test]
fn every_short_string_around_a_joiner_keeps_it_only_between_partners_that_join() {
    // two before, the joiner, two after, over one character of each kind of neighbour (None: the text ends there): a
    // Latin letter, a space, a joiner, a zero-width space, a variation selector, a skin tone, an emoji that takes one,
    // an emoji drawn as text, an emoji drawn as text that is in no listed sequence (a copyright sign), a fire and a
    // laptop (a heart joined to a fire and a man joined to a laptop are listed), two Arabic letters and a transparent
    // mark, a Mongolian letter, a Devanagari letter, virama and dependent vowel. Every string with one character or
    // none on one side; with two on both sides, every string over the ten kinds of `both`.
    let around: Vec<Option<char>> = points(&[
        0x61, 0x20, 0x200D, 0x200B, 0xFE0F, 0x1F3FD, 0x1F468, 0x2764, 0xA9, 0x1F525, 0x1F4BB,
        0x0628, 0x0627, 0x064E, 0x1820, 0x0915, 0x094D, 0x093E,
    ])
    .into_iter()
    .map(Some)
    .chain([None])
    .collect();
    let both = points(&[
        0x61, 0x200D, 0xFE0F, 0x1F3FD, 0x1F468, 0x2764, 0x0628, 0x0915, 0x094D, 0x093E,
    ]);
    let narrow = |c: Option<char>| c.is_none_or(|c| both.contains(&c));
    let mut tally = Tally::default();
    for &a in &around {
        for &b in &around {
            for &c in &around {
                for &d in &around {
                    if a.is_some() && d.is_some() && ![a, b, c, d].into_iter().all(narrow) {
                        continue;
                    }
                    for joiner in [ZWNJ, ZWJ] {
                        let chars: Vec<char> =
                            [a, b, Some(joiner), c, d].into_iter().flatten().collect();
                        check(&chars, &mut tally);
                    }
                }
            }
        }
    }
    report("around", &tally);
    assert_eq!(tally.strings, 50_914);
    for context in ["emoji", "a1", "a2", "b"] {
        assert!(
            tally.kept.get(context).is_some_and(|kept| *kept > 0),
            "{context} is reached"
        );
    }
    assert!(tally.narrowed > 0 && tally.shown_twice_read > 0);
}

#[test]
fn every_arrangement_before_a_joiner_keeps_it_only_after_a_virama_on_a_letter_of_its_script() {
    // up to three before the joiner and two after, over the virama's kinds: `L M* V M1*` in every arrangement, with a
    // letter of another script, a virama that is no nonspacing mark and a mark of no class among them
    let before = points(&[0x0915, 0x61, 0x094D, 0x093C, 0x0902, 0x1B44, 0x20]);
    let after: Vec<Option<char>> = points(&[0x0937, 0x61, 0x093C, 0x0902, 0x093E, 0x20])
        .into_iter()
        .map(Some)
        .chain([None])
        .collect();
    let mut tally = Tally::default();
    for len in 0..=3usize {
        let mut index = vec![0usize; len];
        loop {
            for joiner in [ZWNJ, ZWJ] {
                for &x in &after {
                    for &y in &after {
                        let mut chars: Vec<char> = index.iter().map(|i| before[*i]).collect();
                        chars.push(joiner);
                        chars.extend(x);
                        chars.extend(y);
                        check(&chars, &mut tally);
                    }
                }
            }
            let mut place = 0;
            while place < len {
                index[place] += 1;
                if index[place] < before.len() {
                    break;
                }
                index[place] = 0;
                place += 1;
            }
            if place == len {
                break;
            }
        }
    }
    report("virama", &tally);
    assert_eq!(tally.strings, (1 + 7 + 49 + 343) * 2 * 49);
    for context in ["a2", "b"] {
        assert!(
            tally.kept.get(context).is_some_and(|kept| *kept > 0),
            "{context} is reached"
        );
    }
    assert!(tally.narrowed > 0, "a virama on a Latin letter is refused");
}

#[test]
fn every_arrangement_before_a_joiner_keeps_it_only_after_a_virama_that_conjoins() {
    // up to three before the joiner and one after, over a Thai letter and its phinthu (a killer), a Myanmar letter, its
    // stacker (which conjoins) and its asat (a killer)
    let before = points(&[0x0E01, 0x0E3A, 0x1000, 0x1039, 0x103A]);
    let after: Vec<Option<char>> = points(&[0x0E01, 0x1000])
        .into_iter()
        .map(Some)
        .chain([None])
        .collect();
    let mut tally = Tally::default();
    for len in 0..=3u32 {
        for arrangement in 0..before.len().pow(len) {
            for joiner in [ZWNJ, ZWJ] {
                for &x in &after {
                    let mut chars: Vec<char> = (0..len)
                        .map(|place| before[arrangement / before.len().pow(place) % before.len()])
                        .collect();
                    chars.push(joiner);
                    chars.extend(x);
                    check(&chars, &mut tally);
                }
            }
        }
    }
    report("killers", &tally);
    assert_eq!(tally.strings, (1 + 5 + 25 + 125) * 2 * 3);
    for context in ["a2", "b"] {
        assert!(
            tally.kept.get(context).is_some_and(|kept| *kept > 0),
            "{context} is reached"
        );
    }
    assert!(
        tally.narrowed > 0,
        "a joiner after a killer, which #350's rule kept, is refused"
    );
}

#[test]
fn every_listed_emoji_sequence_in_each_spelling_and_in_six_forms_that_are_none() {
    let mut spellings = Tally::default();
    let mut others = Tally::default();
    for sequence in listed().values().flatten() {
        let selectors: Vec<usize> = (0..sequence.len())
            .filter(|at| sequence[*at] == '\u{FE0F}')
            .collect();
        for left_out in 0..1u32 << selectors.len() {
            let spelling: Vec<char> = (0..sequence.len())
                .filter(|at| {
                    !selectors
                        .iter()
                        .enumerate()
                        .any(|(nth, selector)| selector == at && (left_out >> nth) & 1 == 1)
                })
                .map(|at| sequence[at])
                .collect();
            check(&spelling, &mut spellings);
        }
        // joined on to a fire at either end; a skin tone, the text selector or one more U+FE0F after it; each U+FE0F of
        // its own as the text selector
        for (before, after) in [
            (vec![], vec![ZWJ, '\u{1F525}']),
            (vec!['\u{1F525}', ZWJ], vec![]),
            (vec![], vec!['\u{1F3FD}']),
            (vec![], vec!['\u{FE0E}']),
            (vec![], vec!['\u{FE0F}']),
        ] {
            check(&[before, sequence.clone(), after].concat(), &mut others);
        }
        for selector in &selectors {
            let mut other = sequence.clone();
            other[*selector] = '\u{FE0E}';
            check(&other, &mut others);
        }
    }
    report("listed, each spelling", &spellings);
    report("listed, no spelling", &others);
    assert_eq!((spellings.strings, others.strings), (2_667, 9_077));
    assert_eq!(
        (spellings.kept.get("emoji"), spellings.refused_joiners),
        (Some(&3_747), 0),
        "every joiner of every spelling is kept"
    );
    assert_eq!(
        (others.kept.len(), others.refused_joiners, others.narrowed),
        (0, 15_845, 15_599),
        "no joiner of a form that is no spelling is kept, and #350's rule kept all but the 246 right after a text selector"
    );
}

/// What "a letter of a script that has a virama" rests on: every character of the virama class has a script of its own,
/// never Common, Inherited or Unknown, so a letter carries a virama only of its own script. Read from the data, so a
/// Unicode that changes it fails here.
#[test]
fn every_virama_has_a_script_of_its_own() {
    use icu_properties::props::CanonicalCombiningClass;
    use icu_properties::CodePointMapData;
    let class = CodePointMapData::<CanonicalCombiningClass>::new();
    let script = CodePointMapData::<Script>::new();
    let mut viramas = 0;
    for c in (0..=0x10_FFFF).filter_map(char::from_u32) {
        if class.get(c) == CanonicalCombiningClass::Virama {
            viramas += 1;
            assert!(
                ![Script::Common, Script::Inherited, Script::Unknown].contains(&script.get(c)),
                "U+{:04X}",
                c as u32
            );
        }
    }
    assert!(viramas >= 69, "{viramas}");
}
