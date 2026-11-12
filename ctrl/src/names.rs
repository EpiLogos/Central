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

/// The emoji ZWJ sequences Unicode lists (UTS #51's RGI_Emoji_ZWJ_Sequence, the set of `emoji-zwj-sequences.txt`), each
/// in its fully qualified form: Unicode 17.0, as ICU4C 78.3 exports it, the Unicode of the `icu_properties` data the
/// lock pins. `icu_properties` 2.3 compiles no set of sequences in, so the list is kept as a file whose lines are
/// generated and never edited by hand (its header holds the command); O-I holds the same bytes under the same
/// sha256. What the list holds is pinned by that sha256 alone. Tests hold its shape to the compiled data (every part of
/// every sequence is an emoji there, with its selector exactly where it is drawn as text without one) and its header's
/// Unicode version to the Unicode the build carries, so a lock that moves the data fails until the list is generated
/// again (be2m.4 item 4).
const EMOJI_ZWJ_SEQUENCES: &str = include_str!("continuous_work/emoji-zwj-sequences.txt");

/// U+FE0F, the selector that asks for a character to be drawn as an emoji.
const EMOJI_SELECTOR: char = '\u{FE0F}';

/// The Myanmar asat and the Myanmar stacker (MYANMAR SIGN ASAT, MYANMAR SIGN VIRAMA): the two signs of a kinzi, named
/// because no property tells one (`joiner_kept`). A test holds what is said of them to the compiled data.
const MYANMAR_ASAT: char = '\u{103A}';
const MYANMAR_STACKER: char = '\u{1039}';

/// The Myanmar letters under whose asat a ZWNJ before a stacker changes what is drawn (be2m.5 item 1): nga, ra and Mon
/// nga. They are named by code point and rest on no property: the review of #354 drew every pair of the 120 Myanmar
/// letters on a Mac (letter, asat, with and without the ZWNJ, stacker, letter), and the ZWNJ changed the picture in 360
/// of the 14,400, exactly the pairs whose first letter is one of these three. Under the other 117 it drew nothing
/// new, so they keep no joiner there. A drawing is evidence, not data: a test holds only that the three are Myanmar
/// letters.
const KINZI_LETTERS: [char; 3] = ['\u{1004}', '\u{101B}', '\u{105A}'];

/// What a ZWJ is called when it is refused between two emoji (be2m.4 item 5): the emoji are there, and joined they are
/// no sequence the list holds. The version is the list's own (a test holds the two together), so a person who sees one
/// emoji on a newer system is told why Central does not. The description of `central.day.field.record` says these
/// words too, and a test of the Day holds it to this constant (be2m.5 item 3).
pub(crate) const UNLISTED_EMOJI_SEQUENCE: &str =
    "a joiner between emoji that Unicode 17.0 does not list as one emoji";

/// Every spelling of every listed emoji ZWJ sequence, and the length of the longest.
struct ListedEmoji {
    spellings: std::collections::HashSet<Vec<char>>,
    longest: usize,
}

/// The listed emoji ZWJ sequences in each of their spellings: the fully qualified form, and that form with any of its
/// U+FE0F left out (UTS #51's minimally qualified and unqualified forms), which is drawn the same where an emoji font
/// is chosen. Two of the 2,667 are drawn as text on a Mac, the joiner drawing nothing: a person bouncing a ball joined
/// to the female or the male sign with both selectors left out (U+26F9 U+200D U+2640, U+26F9 U+200D U+2642). They
/// are spellings of a listed sequence all the same, and no property tells them from the others, so they are kept
/// (be2m.4 item 6). Nothing else is a spelling: no selector where the fully qualified form has none, and no skin tone
/// the list does not give.
fn listed_emoji() -> &'static ListedEmoji {
    static LISTED: std::sync::OnceLock<ListedEmoji> = std::sync::OnceLock::new();
    LISTED.get_or_init(|| {
        let mut spellings = std::collections::HashSet::new();
        for line in EMOJI_ZWJ_SEQUENCES
            .lines()
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
        {
            let mut forms: Vec<Vec<char>> = vec![Vec::new()];
            for c in line
                .split(' ')
                .filter_map(|point| u32::from_str_radix(point, 16).ok())
                .filter_map(char::from_u32)
            {
                let without = if c == EMOJI_SELECTOR {
                    forms.clone()
                } else {
                    Vec::new()
                };
                forms.iter_mut().for_each(|form| form.push(c));
                forms.extend(without);
            }
            spellings.extend(forms);
        }
        let longest = spellings.iter().map(Vec::len).max().unwrap_or(0);
        ListedEmoji { spellings, longest }
    })
}

/// Whether the joiner at `at` is part of the words a person sees (be2m, DLW
/// build ruling 2026-11-10; be2m.2, be2m.3 and be2m.4, DLW build rulings
/// 2026-11-12):
/// a joiner that changes how the characters beside it are drawn is kept in
/// prose, and one that draws nothing where it stands is refused like any other
/// invisible character. Prose only: the words of a Day's field records and
/// entry parts. A name, a label or a title keeps the whole rule (`unplain`).
///
/// U+200C and U+200D are in their place here and nowhere else:
/// - U+200D inside an emoji ZWJ sequence Unicode lists (be2m.3 item 1). Some
///   stretch of the text that holds the joiner is one of the listed sequences
///   in one of its spellings (`listed_emoji`: the fully qualified form, with
///   or without each of its U+FE0F), and the sequence stands whole: no ZWJ
///   stands right before the stretch, and no ZWJ, U+FE0E, U+FE0F or skin tone
///   (Emoji_Modifier) right after it. So a heart joined to a fire is kept
///   with its selector or without, and a fire joined to a fire, a copyright
///   sign joined to a registered sign, a family joined on to one more emoji,
///   or a sequence with a skin tone the list does not give, is refused;
/// - inside a word of one script that joins or conjoins, in UAX #31's contexts
///   (section 2.3, Layout and Format Control Characters):
///   A1, U+200C between joining letters of one Script (`LJ T* ZWNJ T* RJ`:
///   Joining_Type Left or Dual before it, Right or Dual after it, across
///   Transparent characters); A2, U+200C after a virama and before a letter
///   (`L M* V M1* ZWNJ M1* L`), both letters and the virama of one Script; B,
///   U+200D after a virama and not before a dependent vowel
///   (`L M* V M1* ZWJ (?!D)`), the letter and the virama of one Script. L is a
///   letter, M a nonspacing mark, M1 a nonspacing mark whose class is not 0,
///   D a character of Indic_Syllabic_Category Vowel_Dependent. V is a virama
///   that conjoins (be2m.3 item 2): Canonical_Combining_Class Virama and
///   Indic_Syllabic_Category Virama or Invisible_Stacker, in A2 and in B.
///   Indic_Syllabic_Category decides: a character of the virama class that
///   it calls Pure_Killer (24: the Thai phinthu, the Myanmar asat, the
///   Tibetan halanta, the Tagalog signs), Reordering_Killer (2) or
///   Number_Joiner (the Brahmi number joiner) is no V: it carries no joiner
///   itself. A joiner right after one is still kept where a V before it
///   carries the context (letter, virama, killer, joiner: the killer is a
///   mark of a class, an M1), and after the asat in its one place below. That
///   is what the data says; nothing here says what a font draws there. The marks between are
///   not asked their Script: a mark is often Inherited or Common. A virama
///   has the Script of the letters it is written with, so a letter of a
///   script that has none (a Latin one) carries none here;
/// - right after a character of the virama class that Indic_Syllabic_Category
///   gives no category (Other: its script is outside that property's scope;
///   in Unicode 17.0 the Tifinagh consonant joiner U+2D7F alone), which
///   stands on a letter of its script (`L M* V`), and right before another
///   letter of that script: either joiner (be2m.4 item 1). The class is the
///   data's word that the character is a virama, the V of UAX #31's A2 and B;
///   that the joiner breaks the bi-consonant ligature U+2D7F asks for is
///   what a Mac draws, not data. A joiner between two Tifinagh letters with
///   no U+2D7F is refused: every Tifinagh letter is Joining_Type Non_Joining,
///   and no property gives the script a join there;
/// - U+200C right after the Myanmar asat U+103A, which stands on nga, ra or
///   Mon nga (U+1004, U+101B, U+105A: `KINZI_LETTERS`), and right before the
///   Myanmar stacker U+1039 and a Myanmar letter (be2m.4 item 2, be2m.5
///   item 1). On a Mac the letter, the asat, the stacker and the next letter
///   were seen drawn as a kinzi over the next letter without the ZWNJ and
///   apart with it, under those three letters and no other; that is a
///   drawing, and no data here says the ZWNJ keeps a kinzi from forming.
///   The signs and the letters are named, since no property tells a kinzi:
///   by the data the asat is of the virama class (UAX #31's V, so this is
///   its A2, the stacker a mark of a class before the letter) and a
///   Pure_Killer, the stacker an Invisible_Stacker, and all are Myanmar's.
///   So a joiner right after an asat is kept in four shapes: this one; and
///   the three the contexts above keep through a stacker that stands before
///   the asat (letter, stacker, asat, ZWJ by B; letter, stacker, asat, ZWNJ,
///   letter by A2; and the two at once, A2 and this place).
///
/// Known and unruled (be2m.5 item 4): the rule stops here until the text of
/// the Standard is read. A ZWJ between two Tifinagh letters with no U+2D7F
/// (a Mac draws the bi-consonant ligature) and a ZWJ between a Bengali ra
/// and its virama before ya (ra, ZWJ, virama, ya) are refused, and whether
/// the Standard gives either spelling is not established from the data on
/// the build machine.
///
/// The properties are Unicode's, read from the ICU data the lock pins
/// (`icu_properties` 2.3.0 with its compiled data, Unicode 17.0), and the
/// sequences from the list beside the cases (`EMOJI_ZWJ_SEQUENCES`), never
/// from ranges kept by hand.
pub fn joiner_kept(chars: &[char], at: usize) -> bool {
    use icu_properties::props::{
        CanonicalCombiningClass, EmojiModifier, GeneralCategory as Category, GeneralCategoryGroup,
        IndicSyllabicCategory, JoiningType, Script,
    };
    use icu_properties::{CodePointMapData, CodePointSetData};
    let Some(&joiner) = chars.get(at).filter(|c| matches!(**c, ZWNJ | ZWJ)) else {
        return false;
    };
    let (before, after) = (&chars[..at], &chars[at + 1..]);
    let tone = |c: char| CodePointSetData::new::<EmojiModifier>().contains(c);
    let joining = |c: char| CodePointMapData::<JoiningType>::new().get(c);
    let category = |c: char| CodePointMapData::<Category>::new().get(c);
    let class = |c: char| CodePointMapData::<CanonicalCombiningClass>::new().get(c);
    let script = |c: char| CodePointMapData::<Script>::new().get(c);
    let letter = |c: char| GeneralCategoryGroup::Letter.contains(category(c));
    let mark = |c: char| category(c) == Category::NonspacingMark;
    let reordered_mark = |c: char| mark(c) && class(c) != CanonicalCombiningClass::NotReordered;
    let syllabic = |c: char| CodePointMapData::<IndicSyllabicCategory>::new().get(c);
    let conjoins = |c: char| {
        class(c) == CanonicalCombiningClass::Virama
            && matches!(
                syllabic(c),
                IndicSyllabicCategory::Virama | IndicSyllabicCategory::InvisibleStacker
            )
    };
    // The letter the mark at `sign` stands on, across nonspacing marks (`L M*`): a letter of the mark's own script.
    let stands_on = |sign: usize| {
        before[..sign]
            .iter()
            .rev()
            .find(|c| !mark(**c))
            .filter(|l| letter(**l) && script(**l) == script(before[sign]))
    };

    // Inside a listed emoji sequence that stands whole.
    if joiner == ZWJ {
        let listed = listed_emoji();
        let whole = |from: usize, to: usize| {
            (from == 0 || chars[from - 1] != ZWJ)
                && chars.get(to).is_none_or(|next| {
                    !matches!(*next, ZWJ | '\u{FE0E}' | EMOJI_SELECTOR) && !tone(*next)
                })
        };
        for from in at.saturating_sub(listed.longest)..at {
            for to in at + 2..=chars.len().min(from + listed.longest) {
                if listed.spellings.contains(&chars[from..to]) && whole(from, to) {
                    return true;
                }
            }
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
    // Right after a sign that stands on a letter of its script, in two places of their own.
    if let Some((sign, on)) = at
        .checked_sub(1)
        .and_then(|sign| Some((before[sign], *stands_on(sign)?)))
    {
        let letter_at = |next: usize| {
            after
                .get(next)
                .is_some_and(|c| letter(*c) && script(*c) == script(sign))
        };
        // A virama the syllabic categories do not reach (the Tifinagh consonant joiner), before a letter.
        if class(sign) == CanonicalCombiningClass::Virama
            && syllabic(sign) == IndicSyllabicCategory::Other
            && letter_at(0)
        {
            return true;
        }
        // ZWNJ between the asat on nga, ra or Mon nga and the stacker of a letter.
        if joiner == ZWNJ
            && sign == MYANMAR_ASAT
            && KINZI_LETTERS.contains(&on)
            && after.first() == Some(&MYANMAR_STACKER)
            && letter_at(1)
        {
            return true;
        }
    }
    // After a virama that conjoins: `L M* V M1*` ends where the joiner stands,
    // and the letter is of the virama's script. A virama is itself a mark of
    // its own class, so each one met on the way back may be the V.
    let mut carrier = None;
    for (index, c) in before.iter().enumerate().rev() {
        if conjoins(*c) {
            carrier = stands_on(index);
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
        after
            .first()
            .is_none_or(|c| syllabic(*c) != IndicSyllabicCategory::VowelDependent)
    }
}

/// What the invisible character at `at` is, in a person's terms, so words left
/// out for one can be given again without it (be2m item 7). A refused ZWJ that
/// stands between two emoji is named as what it is (be2m.4 item 5): the
/// character before it, passing back over U+FE0F and skin tones, and the
/// character right after it are emoji by the compiled data (Emoji and
/// Extended_Pictographic), and the sequence is not one the list holds. Every
/// other refused joiner keeps the words it had.
fn invisible_kind(chars: &[char], at: usize) -> &'static str {
    use icu_properties::props::{BidiControl, Emoji, EmojiModifier, ExtendedPictographic};
    use icu_properties::CodePointSetData;
    let emoji = |c: &char| {
        CodePointSetData::new::<Emoji>().contains(*c)
            && CodePointSetData::new::<ExtendedPictographic>().contains(*c)
    };
    let passed =
        |c: &char| *c == EMOJI_SELECTOR || CodePointSetData::new::<EmojiModifier>().contains(*c);
    let between_emoji = || {
        chars[..at]
            .iter()
            .rev()
            .find(|c| !passed(c))
            .is_some_and(emoji)
            && chars.get(at + 1).is_some_and(emoji)
    };
    match chars[at] {
        '\u{2028}' | '\u{2029}' => "a line or paragraph separator",
        '\u{00AD}' => "a soft hyphen",
        '\u{FEFF}' => "a byte order mark",
        '\u{200B}' => "a zero-width space",
        ZWJ if between_emoji() => UNLISTED_EMOJI_SEQUENCE,
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
        c if invisible(c) && !joiner_kept(chars, at) => Some(invisible_kind(chars, at)),
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
        let each = |text: &str| -> Vec<bool> {
            let chars: Vec<char> = text.chars().collect();
            (0..chars.len())
                .filter(|i| matches!(chars[*i], ZWNJ | ZWJ))
                .map(|i| joiner_kept(&chars, i))
                .collect()
        };
        let none = |text: &str| !each(text).is_empty() && each(text).iter().all(|kept| !*kept);
        // be2m.3 item 1 (DLW build ruling 2026-11-12): inside an emoji ZWJ sequence Unicode lists, in its fully qualified
        // form or with any of its variation selectors left out, and nowhere else between emoji
        assert!(at("\u{1F468}\u{200D}\u{1F467}"));
        assert_eq!(
            each("\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}"),
            [true, true]
        );
        assert!(
            at("\u{2764}\u{FE0F}\u{200D}\u{1F525}") && at("\u{2764}\u{200D}\u{1F525}"),
            "a heart on fire, with its selector and without"
        );
        assert!(at("\u{1F9D1}\u{1F3FD}\u{200D}\u{1F4BB}") && at("\u{1F468}\u{200D}\u{1F9B0}"));
        assert!(at("\u{1F3F3}\u{FE0F}\u{200D}\u{1F308}") && at("\u{1F3F3}\u{200D}\u{1F308}"));
        assert!(at("\u{1F3F4}\u{200D}\u{2620}\u{FE0F}") && at("\u{1F3F4}\u{200D}\u{2620}"));
        assert!(
            at("\u{26F9}\u{1F3FD}\u{200D}\u{2640}\u{FE0F}")
                && at("\u{26F9}\u{1F3FD}\u{200D}\u{2640}"),
            "a tone on a base"
        );
        assert!(
            at("\u{26F9}\u{FE0F}\u{200D}\u{2640}\u{FE0F}")
                && at("\u{26F9}\u{200D}\u{2640}\u{FE0F}")
                && at("\u{26F9}\u{FE0F}\u{200D}\u{2640}")
                && at("\u{26F9}\u{200D}\u{2640}"),
            "each selector of the fully qualified form, there or left out"
        );
        // be2m.4 item 6: the two spellings a Mac draws as text, the joiner drawing nothing, are spellings of a listed
        // sequence and are kept
        for text in ["\u{26F9}\u{200D}\u{2640}", "\u{26F9}\u{200D}\u{2642}"] {
            assert!(at(text), "{text:?}");
            assert_eq!(prose_fault(text), None, "{text:?}");
        }
        assert_eq!(
            each("\u{1F9D1}\u{1F3FB}\u{200D}\u{1F91D}\u{200D}\u{1F9D1}\u{1F3FF}"),
            [true, true]
        );
        assert_eq!(
            each("\u{1F469}\u{200D}\u{2764}\u{200D}\u{1F48B}\u{200D}\u{1F468}"),
            [true, true, true]
        );
        assert_eq!(
            each(
                "a\u{1F468}\u{200D}\u{1F467}\u{2764}\u{200D}\u{1F525}\u{1F468}\u{200D}\u{1F467} b"
            ),
            [true, true, true],
            "listed sequences side by side, with text around them"
        );
        // the twelve the review of #353 drew without a selector: each is one emoji on a Mac, the same pixels as its fully
        // qualified form
        for text in [
            "\u{2764}\u{200D}\u{1F525}",
            "\u{1F469}\u{200D}\u{2764}\u{200D}\u{1F468}",
            "\u{1F468}\u{200D}\u{2695}",
            "\u{1F3F3}\u{200D}\u{1F308}",
            "\u{1F3F4}\u{200D}\u{2620}",
            "\u{1F441}\u{200D}\u{1F5E8}",
            "\u{1F636}\u{200D}\u{1F32B}",
            "\u{1F43B}\u{200D}\u{2744}",
            "\u{26D3}\u{200D}\u{1F4A5}",
            "\u{1F642}\u{200D}\u{2194}",
            "\u{26F9}\u{200D}\u{2640}\u{FE0F}",
            "\u{1F3F3}\u{FE0F}\u{200D}\u{26A7}",
        ] {
            assert!(
                !each(text).is_empty() && each(text).iter().all(|kept| *kept),
                "{text:?}"
            );
            assert_eq!(prose_fault(text), None, "{text:?}");
        }
        // two emoji that are no listed sequence
        for text in [
            "\u{1F468}\u{200D}\u{1F469}",
            "\u{1F525}\u{200D}\u{1F525}",
            "\u{1F468}\u{200D}\u{1F525}",
            "\u{231A}\u{200D}\u{26A1}",
            "\u{A9}\u{200D}\u{AE}",
            "\u{A9}\u{FE0F}\u{200D}\u{AE}\u{FE0F}",
            "\u{1F468}\u{200D}\u{26F9}",
            "\u{26F9}\u{200D}\u{1F468}",
            "\u{26F9}\u{FE0F}\u{200D}\u{26F9}\u{1F3FD}",
            "\u{1F468}\u{200D}\u{1F9D1}\u{1F3FD}",
            "\u{1F468}\u{200D}\u{1F468}\u{200D}\u{1F468}\u{200D}\u{1F468}\u{200D}\u{1F468}",
            "\u{1F468}\u{200D}\u{1F525}\u{1F3FD}",
        ] {
            assert!(none(text), "{text:?}");
        }
        // a listed sequence that does not stand whole: joined on to another emoji at either end, or followed by a skin
        // tone or a selector that is no part of the spelling
        assert!(none(
            "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}\u{200D}\u{1F525}"
        ));
        assert!(none(
            "\u{1F525}\u{200D}\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}"
        ));
        assert!(none("\u{1F469}\u{200D}\u{2764}\u{FE0F}\u{200D}\u{1F525}"));
        assert!(
            none("\u{1F468}\u{200D}\u{1F467}\u{1F3FD}")
                && none("\u{1F9D1}\u{200D}\u{1F91D}\u{200D}\u{1F9D1}\u{1F3FF}")
                && none("\u{2764}\u{200D}\u{1F525}\u{1F3FD}"),
            "a skin tone the list does not give"
        );
        assert!(
            none("\u{1F468}\u{200D}\u{1F4BB}\u{FE0F}")
                && none("\u{1F468}\u{FE0F}\u{200D}\u{1F4BB}")
                && none("\u{2764}\u{FE0F}\u{FE0F}\u{200D}\u{1F525}"),
            "a selector where the fully qualified form has none"
        );
        assert!(
            none("\u{2764}\u{FE0E}\u{200D}\u{1F525}") && none("\u{1F468}\u{200D}\u{1F467}\u{FE0E}"),
            "the text selector"
        );
        assert!(
            none("\u{1F469}\u{200D}\u{2764}\u{FE0E}\u{200D}\u{1F468}"),
            "the text selector inside"
        );
        assert!(!at("\u{1F468}\u{200D}a") && !at("a\u{200D}\u{1F468}"));
        assert!(!at("\u{200D}\u{1F468}") && !at("\u{1F468}\u{200D}"));
        assert!(!at("\u{FE0F}\u{200D}\u{1F525}") && !at("a\u{1F3FD}\u{200D}\u{1F4BB}"));
        assert!(!at("\u{1F468}\u{200C}\u{1F467}"), "ZWNJ joins no emoji");
        // be2m.2 item 1, as it stands under be2m.3: no other pictograph, no unassigned code point, no regional
        // indicator, no keycap base, no lone tone
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
            !at("\u{1F468}\u{200D}\u{FE0F}\u{1F467}"),
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
            at("\u{1B13}\u{1B44}\u{094D}\u{200D}") && at("\u{1B13}\u{1B44}\u{094D}\u{200C}\u{1B13}"),
            "the nearer virama stands on no letter (an adeg adeg is no nonspacing mark), the one before it does"
        );
        // be2m.3 item 2 (DLW build ruling 2026-11-12): the virama conjoins, by Indic_Syllabic_Category Virama or
        // Invisible_Stacker; a sign of the virama class that the property calls a killer or a number joiner keeps no
        // joiner here (the Tifinagh consonant joiner and the asat before a stacker have tests of their own, be2m.4 and be2m.5)
        for (letter, killer) in [
            ('\u{0E01}', '\u{0E3A}'),
            ('\u{0E81}', '\u{0EBA}'),
            ('\u{0F40}', '\u{0F84}'),
            ('\u{1000}', '\u{103A}'),
            ('\u{1703}', '\u{1714}'),
            ('\u{0D15}', '\u{0D3B}'),
            ('\u{1BC0}', '\u{1BF2}'),
            ('\u{11013}', '\u{1107F}'),
        ] {
            for text in [
                format!("{letter}{killer}\u{200D}"),
                format!("{letter}{killer}\u{200D}{letter}"),
                format!("{letter}{killer}\u{200C}{letter}"),
            ] {
                assert!(!at(&text), "{text:?}");
            }
        }
        assert!(
            !at("\u{1703}\u{1715}\u{1714}\u{200D}")
                && !at("\u{1703}\u{1715}\u{1714}\u{200C}\u{1703}"),
            "both Tagalog signs are pure killers"
        );
        assert!(
            at("\u{1000}\u{1039}\u{200D}\u{1000}")
                && at("\u{1000}\u{1039}\u{200C}\u{1000}")
                && at("\u{1780}\u{17D2}\u{200C}\u{1780}"),
            "an invisible stacker is a V"
        );
        assert!(
            at("\u{0915}\u{094D}\u{0E3A}\u{200D}"),
            "after the virama a killer is a mark of a class like any other"
        );
        // be2m.3 item 4, the edge the review of #353 found unpinned: a mark of a class is no virama
        assert!(!at("\u{0915}\u{093C}\u{200D}") && !at("\u{0915}\u{093C}\u{200C}\u{0937}"));
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
            prose_fault("One.\n\tTwo.\r\nThree \u{1F468}\u{200D}\u{1F467}."),
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
            shown_prose(" \u{200B}\u{1F468}\u{200D}\u{1F467} pay\u{200C}pal\u{2060} "),
            "\u{1F468}\u{200D}\u{1F467} paypal"
        );
        assert_eq!(
            shown_prose("\u{1F468}\u{200D}\u{200B}\u{1F467}"),
            "\u{1F468}\u{1F467}",
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
            // be2m.3: a listed sequence a refused character stood beside is read again as it stands without it
            (
                "\u{1F468}\u{200D}\u{1F467}\u{200B}\u{1F3FD}",
                "\u{1F468}\u{1F467}\u{1F3FD}",
            ),
            (
                "\u{1F468}\u{200D}\u{200B}\u{1F469}\u{200D}\u{1F467}",
                "\u{1F468}\u{1F469}\u{200D}\u{1F467}",
            ),
            (
                "\u{2764}\u{200B}\u{FE0F}\u{200D}\u{1F525}",
                "\u{2764}\u{FE0F}\u{1F525}",
            ),
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

    /// The emoji ZWJ sequences Unicode lists, fully qualified, by this test's own reading of the list.
    fn listed_sequences() -> Vec<Vec<char>> {
        include_str!("continuous_work/emoji-zwj-sequences.txt")
            .lines()
            .filter(|line| !line.starts_with('#'))
            .map(|line| {
                line.split(' ')
                    .map(|point| char::from_u32(u32::from_str_radix(point, 16).unwrap()).unwrap())
                    .collect()
            })
            .collect()
    }

    /// Whether each joiner of the text is kept, in order.
    fn joiners_kept(chars: &[char]) -> Vec<bool> {
        (0..chars.len())
            .filter(|at| matches!(chars[*at], ZWNJ | ZWJ))
            .map(|at| joiner_kept(chars, at))
            .collect()
    }

    /// be2m.3 item 1 (DLW build ruling 2026-11-12): the list of emoji ZWJ sequences is Unicode's own, generated and pinned.
    /// Its bytes are pinned by one sha256, the one O-I holds too, and that sha256 alone says which sequences it
    /// holds: no set of sequences is compiled into the build to hold it against. It names its generator, and it holds
    /// the 1,614 sequences of Unicode 17.0, each once.
    #[test]
    fn the_emoji_sequence_list_is_unicodes_own_under_one_sha256() {
        use sha2::{Digest, Sha256};
        let list = include_str!("continuous_work/emoji-zwj-sequences.txt");
        assert_eq!(
            format!("{:x}", Sha256::digest(list.as_bytes())),
            "3466f7002716b502b61957afbb751af85aee3bd379b577004c96f14d47dc151b",
            "the list is the one O-I holds"
        );
        assert!(list.contains("icuexportdata -m uprops -d OUT RGI_Emoji_ZWJ_Sequence"));
        assert!(list.is_ascii() && !list.contains('\r') && list.ends_with('\n'));
        let sequences = listed_sequences();
        assert_eq!(sequences.len(), 1614);
        let distinct: std::collections::BTreeSet<&Vec<char>> = sequences.iter().collect();
        assert_eq!(distinct.len(), 1614);
    }

    /// The Unicode version the list's header names: `# The RGI emoji ZWJ sequences of Unicode <version> (`.
    fn listed_unicode_version() -> &'static str {
        include_str!("continuous_work/emoji-zwj-sequences.txt")
            .strip_prefix("# The RGI emoji ZWJ sequences of Unicode ")
            .and_then(|rest| rest.split_once(' '))
            .map(|(version, _)| version)
            .expect("the list's first line names its Unicode version")
    }

    /// be2m.4 item 4 (DLW build ruling 2026-11-12): the list is held to the Unicode the build carries. `icu_properties`
    /// tells no version, so the compiled data is asked what only one Unicode answers: how many characters it assigns
    /// (every code point but the unassigned, the private-use and the control ones: 159,801 in Unicode 17.0, counted
    /// from ICU4C 78.3's export, the count the Standard gives for 17.0), and a letter that Unicode 17.0 was the first
    /// to assign (U+088F, unassigned in Unicode 16.0). A lock that moves the data to another Unicode fails here, in
    /// words that say what to do; and the refusal that names the version names the list's.
    #[test]
    fn the_emoji_sequence_list_names_the_unicode_the_build_carries() {
        use icu_properties::props::GeneralCategory as Category;
        use icu_properties::CodePointMapData;
        const CHARACTERS: [(&str, usize); 1] = [("17.0", 159_801)];
        let version = listed_unicode_version();
        let Some((_, characters)) = CHARACTERS.iter().find(|(of, _)| *of == version) else {
            panic!("the list's header names Unicode {version}, and this test holds no count of its characters: add it");
        };
        let category = CodePointMapData::<Category>::new();
        let assigned = (0..=0x10_FFFF)
            .filter_map(char::from_u32)
            .filter(|c| {
                !matches!(
                    category.get(*c),
                    Category::Unassigned | Category::PrivateUse | Category::Control
                )
            })
            .count();
        assert_eq!(
            assigned, *characters,
            "the Unicode data compiled into the build assigns {assigned} characters, and Unicode {version}, which the \
             header of emoji-zwj-sequences.txt names, assigns {characters}: the lock has moved icu_properties' data to \
             another Unicode. Generate the list again from the ICU of that Unicode (its header holds the command), then \
             move its sha256, the count here and the version in the refusal's words and in the description of \
             central.day.field.record"
        );
        assert_ne!(
            category.get('\u{088F}'),
            Category::Unassigned,
            "a letter first assigned in Unicode 17.0"
        );
        assert!(
            UNLISTED_EMOJI_SEQUENCE.contains(&format!("Unicode {version} does not list")),
            "the refusal names the list's Unicode: {UNLISTED_EMOJI_SEQUENCE}"
        );
    }

    /// be2m.3 item 1 and be2m.4 item 4: every sequence of the list is made of code points the compiled data calls
    /// emoji, in the shape of a fully qualified sequence: two to four parts joined by ZWJ, each part an emoji there
    /// (Emoji and Extended_Pictographic), carrying U+FE0F exactly when it is drawn as text without one (no
    /// Emoji_Presentation), or a skin tone on a character that takes one. It reads no sha256, so a line replaced by
    /// anything else is caught here on its own. A line replaced by another chain of emoji in that shape is caught by
    /// the sha256 alone.
    #[test]
    fn every_listed_emoji_sequence_is_made_of_what_the_compiled_data_calls_emoji() {
        use icu_properties::props::{
            Emoji, EmojiModifier, EmojiModifierBase, EmojiPresentation, ExtendedPictographic,
        };
        use icu_properties::CodePointSetData;
        let sequences = listed_sequences();
        assert!(!sequences.is_empty());
        let emoji = |c: char| {
            CodePointSetData::new::<Emoji>().contains(c)
                && CodePointSetData::new::<ExtendedPictographic>().contains(c)
        };
        let presentation = |c: char| CodePointSetData::new::<EmojiPresentation>().contains(c);
        for sequence in &sequences {
            let parts: Vec<&[char]> = sequence.split(|c| *c == ZWJ).collect();
            assert!((2..=4).contains(&parts.len()), "{sequence:?}");
            for part in parts {
                let qualified = match part {
                    [c] => emoji(*c) && presentation(*c),
                    [c, '\u{FE0F}'] => emoji(*c) && !presentation(*c),
                    [c, tone] => {
                        emoji(*c)
                            && CodePointSetData::new::<EmojiModifierBase>().contains(*c)
                            && CodePointSetData::new::<EmojiModifier>().contains(*tone)
                    }
                    _ => false,
                };
                assert!(qualified, "{part:?} of {sequence:?}");
            }
        }
    }

    /// be2m.3 item 1: every listed sequence is kept in its fully qualified form and with each of its variation selectors
    /// left out, in every combination (2,667 spellings of the 1,614; 961 of them hold a selector, and were refused
    /// without it before). It is kept only whole: joined on to one more emoji at either end, followed by a skin tone, by
    /// the text selector or by one more U+FE0F, or with a U+FE0F of its own turned into the text selector, no joiner of it
    /// is kept.
    #[test]
    fn every_listed_emoji_sequence_is_kept_in_each_of_its_spellings_and_only_whole() {
        let (mut spellings, mut with_a_selector, mut not_whole) = (0, 0, 0);
        for sequence in listed_sequences() {
            let selectors: Vec<usize> = (0..sequence.len())
                .filter(|at| sequence[*at] == '\u{FE0F}')
                .collect();
            with_a_selector += usize::from(!selectors.is_empty());
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
                spellings += 1;
                let kept = joiners_kept(&spelling);
                assert!(
                    !kept.is_empty() && kept.iter().all(|kept| *kept),
                    "{spelling:?}"
                );
                let text: String = spelling.iter().collect();
                assert_eq!(prose_fault(&text), None, "{spelling:?}");
            }
            let with = |before: &[char], after: &[char]| -> Vec<char> {
                [before, &sequence[..], after].concat()
            };
            let mut others = vec![
                with(&[], &[ZWJ, '\u{1F525}']),
                with(&['\u{1F525}', ZWJ], &[]),
                with(&[], &['\u{1F3FD}']),
                with(&[], &['\u{FE0E}']),
                with(&[], &['\u{FE0F}']),
            ];
            for selector in &selectors {
                let mut other = sequence.clone();
                other[*selector] = '\u{FE0E}';
                others.push(other);
            }
            for other in others {
                not_whole += 1;
                assert!(joiners_kept(&other).iter().all(|kept| !*kept), "{other:?}");
            }
        }
        assert_eq!((spellings, with_a_selector, not_whole), (2667, 961, 9077));
    }

    /// be2m.3 item 2 (DLW build ruling 2026-11-12), worded by be2m.4 item 3: Indic_Syllabic_Category decides which
    /// character of the virama class is the V of A2 and B: Virama or Invisible_Stacker (41 of the 69 of Unicode 17.0).
    /// The 28 named here are the rest, by the data: 24 Pure_Killer, 2 Reordering_Killer, the Brahmi number joiner
    /// (Number_Joiner), and the Tifinagh consonant joiner, which the property gives no category (Other). After each of
    /// the 27 the property names, on a letter of its own script, neither joiner is kept, with a letter after it or
    /// none. The one it does not name keeps either joiner before a letter of its script (be2m.4 item 1), and neither
    /// with none after it. Nothing here says what a font draws.
    #[test]
    fn a_joiner_is_kept_after_a_virama_only_when_the_virama_conjoins() {
        use icu_properties::props::{
            CanonicalCombiningClass, GeneralCategory as Category, GeneralCategoryGroup,
            IndicSyllabicCategory, Script,
        };
        use icu_properties::CodePointMapData;
        const NO_VIRAMA_BY_CATEGORY: [u32; 28] = [
            0x0D3B, 0x0D3C, 0x0E3A, 0x0EBA, 0x0F84, 0x103A, 0x1714, 0x1715, 0x1734, 0x1BAA, 0x1BF2,
            0x1BF3, 0x2D7F, 0xA82C, 0xA953, 0xABED, 0x11070, 0x1107F, 0x11134, 0x112EA, 0x113CE,
            0x113CF, 0x1172B, 0x1193D, 0x11A34, 0x11D44, 0x11F41, 0x1612F,
        ];
        let class = CodePointMapData::<CanonicalCombiningClass>::new();
        let script = CodePointMapData::<Script>::new();
        let category = CodePointMapData::<Category>::new();
        let syllabic = CodePointMapData::<IndicSyllabicCategory>::new();
        let mut first_letter: Vec<(Script, char)> = Vec::new();
        let mut viramas = Vec::new();
        for c in (0..=0x10_FFFF).filter_map(char::from_u32) {
            if class.get(c) == CanonicalCombiningClass::Virama {
                viramas.push(c);
            }
            if GeneralCategoryGroup::Letter.contains(category.get(c))
                && !first_letter.iter().any(|(of, _)| *of == script.get(c))
            {
                first_letter.push((script.get(c), c));
            }
        }
        assert_eq!(viramas.len(), 69);
        let (mut conjoining, mut killers, mut unnamed) = (0, 0, Vec::new());
        for virama in viramas {
            let letter = first_letter
                .iter()
                .find(|(of, _)| *of == script.get(virama))
                .map(|(_, letter)| *letter)
                .unwrap_or_else(|| panic!("U+{:04X} has a letter of its script", virama as u32));
            let conjoins = !NO_VIRAMA_BY_CATEGORY.contains(&(virama as u32));
            conjoining += usize::from(conjoins);
            assert_eq!(
                conjoins,
                matches!(
                    syllabic.get(virama),
                    IndicSyllabicCategory::Virama | IndicSyllabicCategory::InvisibleStacker
                ),
                "U+{:04X}: Indic_Syllabic_Category decides",
                virama as u32
            );
            let unnamed_by_the_property = syllabic.get(virama) == IndicSyllabicCategory::Other;
            if unnamed_by_the_property {
                unnamed.push(virama);
            } else if !conjoins {
                killers += 1;
                assert!(
                    matches!(
                        syllabic.get(virama),
                        IndicSyllabicCategory::PureKiller
                            | IndicSyllabicCategory::ReorderingKiller
                            | IndicSyllabicCategory::NumberJoiner
                    ),
                    "U+{:04X}: a killer or a number joiner by the data",
                    virama as u32
                );
            }
            for (text, before_a_letter) in [
                (vec![letter, virama, ZWJ], false),
                (vec![letter, virama, ZWJ, letter], true),
                (vec![letter, virama, ZWNJ, letter], true),
            ] {
                assert_eq!(
                    joiner_kept(&text, 2),
                    conjoins || (unnamed_by_the_property && before_a_letter),
                    "U+{:04X} in {text:?}",
                    virama as u32
                );
            }
        }
        assert_eq!((conjoining, killers), (41, 27));
        assert_eq!(
            unnamed,
            ['\u{2D7F}'],
            "the Tifinagh consonant joiner alone is of the virama class with no syllabic category"
        );
    }

    /// be2m.4 item 1 (DLW build ruling 2026-11-12): a joiner right after the Tifinagh consonant joiner U+2D7F, which
    /// stands on a Tifinagh letter, and right before another Tifinagh letter is kept, ZWNJ and ZWJ alike. What it
    /// rests on in the compiled data: U+2D7F is of the virama class (the V of UAX #31's A2 and B) and of the Tifinagh
    /// script, and Indic_Syllabic_Category gives it no category, so no data calls it a killer. That a joiner there
    /// breaks the bi-consonant ligature is what the review of #353 saw a Mac draw; it is not data. A joiner between two
    /// Tifinagh letters with no U+2D7F stays refused: every Tifinagh letter is Joining_Type Non_Joining, and no
    /// property gives the script a join there.
    #[test]
    fn a_joiner_after_the_tifinagh_consonant_joiner_is_kept_before_a_letter_of_its_script() {
        use icu_properties::props::{
            CanonicalCombiningClass, GeneralCategory as Category, GeneralCategoryGroup,
            IndicSyllabicCategory, JoiningType, Script,
        };
        use icu_properties::CodePointMapData;
        let kept = |text: &str| {
            let chars: Vec<char> = text.chars().collect();
            let kept = joiners_kept(&chars);
            assert_eq!(kept.len(), 1, "{text:?} holds one joiner");
            assert_eq!(prose_fault(text).is_none(), kept[0], "{text:?}");
            kept[0]
        };
        for text in [
            "\u{2D4E}\u{2D7F}\u{200C}\u{2D5C}",
            "\u{2D4E}\u{2D7F}\u{200D}\u{2D5C}",
            "a \u{2D4E}\u{2D7F}\u{200C}\u{2D5C} b",
            "\u{2D30}\u{2D7F}\u{200D}\u{2D30}",
            "\u{2D4E}\u{0301}\u{2D7F}\u{200D}\u{2D5C}",
            "\u{2D4E}\u{2D7F}\u{2D7F}\u{200C}\u{2D5C}",
            "\u{2D4E}\u{2D7F}\u{200D}\u{2D6F}",
        ] {
            assert!(kept(text), "{text:?}");
        }
        for (text, why) in [
            ("\u{2D4E}\u{2D7F}\u{200D}", "nothing after it"),
            ("\u{2D4E}\u{2D7F}\u{200C}", "nothing after it"),
            ("\u{2D4E}\u{2D7F}\u{200D} \u{2D5C}", "a space after it"),
            ("\u{2D4E}\u{2D7F}\u{200D}a", "a Latin letter after it"),
            (
                "\u{2D4E}\u{2D7F}\u{200C}\u{0915}",
                "a Devanagari letter after it",
            ),
            (
                "\u{2D4E}\u{2D7F}\u{200D}\u{2D70}",
                "a Tifinagh mark of punctuation after it",
            ),
            (
                "\u{2D4E}\u{2D7F}\u{200D}\u{0301}\u{2D5C}",
                "a mark between it and the letter",
            ),
            (
                "\u{2D4E}\u{2D7F}\u{0301}\u{200D}\u{2D5C}",
                "a mark between the sign and it",
            ),
            (
                "\u{2D4E}\u{2D7F}a\u{200D}\u{2D5C}",
                "a letter between the sign and it",
            ),
            ("\u{2D7F}\u{200D}\u{2D5C}", "the sign on no letter"),
            ("a\u{2D7F}\u{200D}\u{2D5C}", "the sign on a Latin letter"),
            (
                "\u{0915}\u{2D7F}\u{200C}\u{2D5C}",
                "the sign on a Devanagari letter",
            ),
            (
                "\u{2D70}\u{2D7F}\u{200C}\u{2D5C}",
                "the sign on a mark of punctuation",
            ),
            (
                "\u{2D4E}\u{200D}\u{2D5C}",
                "no consonant joiner: ZWJ between two letters",
            ),
            (
                "\u{2D4E}\u{200C}\u{2D5C}",
                "no consonant joiner: ZWNJ between two letters",
            ),
            (
                "\u{2D4E}\u{200D}\u{2D7F}\u{2D5C}",
                "the joiner before the sign",
            ),
        ] {
            assert!(!kept(text), "{why}: {text:?}");
        }
        let class = CodePointMapData::<CanonicalCombiningClass>::new();
        let syllabic = CodePointMapData::<IndicSyllabicCategory>::new();
        let script = CodePointMapData::<Script>::new();
        let category = CodePointMapData::<Category>::new();
        let joining = CodePointMapData::<JoiningType>::new();
        assert_eq!(
            (
                class.get('\u{2D7F}'),
                syllabic.get('\u{2D7F}'),
                script.get('\u{2D7F}'),
                category.get('\u{2D7F}')
            ),
            (
                CanonicalCombiningClass::Virama,
                IndicSyllabicCategory::Other,
                Script::Tifinagh,
                Category::NonspacingMark
            )
        );
        let letters: Vec<char> = (0..=0x10_FFFF)
            .filter_map(char::from_u32)
            .filter(|c| {
                script.get(*c) == Script::Tifinagh
                    && GeneralCategoryGroup::Letter.contains(category.get(*c))
            })
            .collect();
        assert_eq!(letters.len(), 57);
        for letter in letters {
            assert_eq!(
                joining.get(letter),
                JoiningType::NonJoining,
                "U+{:04X}",
                letter as u32
            );
            assert_eq!(
                syllabic.get(letter),
                IndicSyllabicCategory::Other,
                "U+{:04X}",
                letter as u32
            );
            for joiner in [ZWNJ, ZWJ] {
                assert!(
                    joiner_kept(&['\u{2D4E}', '\u{2D7F}', joiner, letter], 2),
                    "U+{:04X}",
                    letter as u32
                );
                assert!(
                    !joiner_kept(&['\u{2D4E}', joiner, letter], 1),
                    "U+{:04X}",
                    letter as u32
                );
            }
        }
    }

    /// be2m.4 item 2 and be2m.5 items 1 and 2 (DLW build rulings 2026-11-12): a ZWNJ right after the Myanmar asat
    /// U+103A, which stands on nga, ra or Mon nga, and right before the Myanmar stacker U+1039 and a Myanmar letter is
    /// kept; under every other Myanmar letter it is refused. What it rests on in the compiled data: the asat is of
    /// the virama class, so UAX #31's A2 reads it as a V, with the stacker a mark of a class before the letter;
    /// Indic_Syllabic_Category calls the asat a Pure_Killer and the stacker an Invisible_Stacker; both are Myanmar's
    /// and nonspacing marks. That the ZWNJ draws the letters apart where a kinzi is drawn without it, and that it does
    /// so under these three letters only (360 of 14,400 pairs), is what the reviews of #353 and #354 saw a Mac draw; no
    /// data on the build machine says it. The signs and the letters are named in the code, since no property tells a
    /// kinzi: the same shape in another script that has a killer and a stacker is refused. A joiner right after an
    /// asat is kept in three more shapes, which main's B and A2 keep through a stacker before the asat.
    #[test]
    fn a_zwnj_after_the_asat_on_nga_ra_or_mon_nga_and_before_a_stacker_is_kept() {
        use icu_properties::props::{
            CanonicalCombiningClass, GeneralCategory as Category, IndicSyllabicCategory, Script,
        };
        use icu_properties::CodePointMapData;
        let kept = |text: &str| {
            let chars: Vec<char> = text.chars().collect();
            let kept = joiners_kept(&chars);
            assert_eq!(kept.len(), 1, "{text:?} holds one joiner");
            assert_eq!(prose_fault(text).is_none(), kept[0], "{text:?}");
            kept[0]
        };
        for text in [
            "\u{1004}\u{103A}\u{200C}\u{1039}\u{1000}",
            "\u{101B}\u{103A}\u{200C}\u{1039}\u{1002}",
            "a \u{1004}\u{103A}\u{200C}\u{1039}\u{1000}\u{102C} b",
            "\u{1004}\u{0301}\u{103A}\u{200C}\u{1039}\u{1000}",
            "\u{105A}\u{103A}\u{200C}\u{1039}\u{1000}",
        ] {
            assert!(kept(text), "{text:?}");
        }
        // be2m.5 item 1: of the Myanmar letters, exactly the three keep it
        let myanmar_letters: Vec<char> = ('\u{1000}'..='\u{109F}')
            .chain('\u{A9E0}'..='\u{A9FF}')
            .chain('\u{AA60}'..='\u{AA7F}')
            .chain('\u{116D0}'..='\u{116E3}')
            .filter(|c| {
                icu_properties::props::GeneralCategoryGroup::Letter
                    .contains(CodePointMapData::<Category>::new().get(*c))
                    && CodePointMapData::<Script>::new().get(*c) == Script::Myanmar
            })
            .collect();
        assert_eq!(myanmar_letters.len(), 120);
        let keeping: Vec<char> = myanmar_letters
            .iter()
            .copied()
            .filter(|letter| kept(&format!("{letter}\u{103A}\u{200C}\u{1039}\u{1000}")))
            .collect();
        assert_eq!(keeping, ['\u{1004}', '\u{101B}', '\u{105A}']);
        assert_eq!(keeping, KINZI_LETTERS);
        // be2m.5 item 2: what else is kept right after an asat, by main's B and A2 through a stacker before it
        for (text, why) in [
            ("\u{1000}\u{1039}\u{103A}\u{200D}", "B"),
            ("\u{1000}\u{1039}\u{103A}\u{200C}\u{1000}", "A2"),
            (
                "\u{1000}\u{1039}\u{103A}\u{200C}\u{1039}\u{1000}",
                "A2, with a stacker after it",
            ),
        ] {
            assert!(kept(text), "{why}: {text:?}");
        }
        for (text, why) in [
            ("\u{1004}\u{103A}\u{200D}\u{1039}\u{1000}", "ZWJ"),
            ("\u{1004}\u{103A}\u{200C}\u{1000}", "no stacker after it"),
            (
                "\u{1004}\u{103A}\u{200D}\u{1000}",
                "ZWJ, no stacker after it",
            ),
            ("\u{1004}\u{103A}\u{200C}", "nothing after it"),
            (
                "\u{1004}\u{103A}\u{200C}\u{1039}",
                "nothing after the stacker",
            ),
            (
                "\u{1004}\u{103A}\u{200C}\u{1039}a",
                "a Latin letter after the stacker",
            ),
            (
                "\u{1004}\u{103A}\u{200C}\u{1039}\u{0915}",
                "a Devanagari letter after the stacker",
            ),
            (
                "\u{1004}\u{103A}\u{200C}\u{1039}\u{1039}\u{1000}",
                "two stackers",
            ),
            (
                "\u{1004}\u{103A}\u{200C}\u{1039}\u{0301}\u{1000}",
                "a mark between the stacker and the letter",
            ),
            (
                "\u{1004}\u{103A}\u{200C}\u{0301}\u{1039}\u{1000}",
                "a mark between it and the stacker",
            ),
            (
                "\u{1004}\u{103A}\u{0301}\u{200C}\u{1039}\u{1000}",
                "a mark between the asat and it",
            ),
            (
                "\u{1004}\u{103A}\u{200C}\u{103A}\u{1000}",
                "an asat where the stacker stands",
            ),
            ("\u{103A}\u{200C}\u{1039}\u{1000}", "the asat on no letter"),
            ("\u{1000}\u{103A}\u{200C}\u{1039}\u{1000}", "the asat on ka"),
            ("\u{1019}\u{103A}\u{200C}\u{1039}\u{1019}", "the asat on ma"),
            (
                "\u{1004}\u{1000}\u{103A}\u{200C}\u{1039}\u{1000}",
                "nga before the letter the asat stands on",
            ),
            (
                "\u{1000}\u{103A}\u{200C}\u{1039}\u{1004}",
                "nga after the stacker only",
            ),
            (
                "a\u{103A}\u{200C}\u{1039}\u{1000}",
                "the asat on a Latin letter",
            ),
            (
                "\u{1040}\u{103A}\u{200C}\u{1039}\u{1000}",
                "the asat on a Myanmar digit",
            ),
            (
                "\u{0E01}\u{0E3A}\u{200C}\u{1039}\u{1000}",
                "a Thai phinthu, not the asat",
            ),
            (
                "\u{0E01}\u{0E3A}\u{200C}\u{1039}\u{0E01}",
                "a Thai phinthu before the Myanmar stacker and a Thai letter",
            ),
            (
                "\u{1004}\u{103A}\u{200C}\u{17D2}\u{1000}",
                "a Khmer coeng where the stacker stands",
            ),
            (
                "\u{1B8A}\u{1BAA}\u{200C}\u{1BAB}\u{1B8A}",
                "the Sundanese killer and stacker",
            ),
            (
                "\u{ABC0}\u{ABED}\u{200C}\u{AAF6}\u{ABC0}",
                "the Meetei killer and stacker",
            ),
            (
                "\u{11103}\u{11134}\u{200C}\u{11133}\u{11103}",
                "the Chakma killer and stacker",
            ),
        ] {
            assert!(!kept(text), "{why}: {text:?}");
        }
        let class = CodePointMapData::<CanonicalCombiningClass>::new();
        let syllabic = CodePointMapData::<IndicSyllabicCategory>::new();
        let script = CodePointMapData::<Script>::new();
        let category = CodePointMapData::<Category>::new();
        for (sign, is) in [
            (MYANMAR_ASAT, IndicSyllabicCategory::PureKiller),
            (MYANMAR_STACKER, IndicSyllabicCategory::InvisibleStacker),
        ] {
            assert_eq!(
                (
                    class.get(sign),
                    syllabic.get(sign),
                    script.get(sign),
                    category.get(sign)
                ),
                (
                    CanonicalCombiningClass::Virama,
                    is,
                    Script::Myanmar,
                    Category::NonspacingMark
                ),
                "U+{:04X}",
                sign as u32
            );
        }
        assert_eq!((MYANMAR_ASAT, MYANMAR_STACKER), ('\u{103A}', '\u{1039}'));
    }

    /// be2m.4 item 5 (DLW build ruling 2026-11-12): a ZWJ refused between two emoji is named as what it is, a joiner
    /// between emoji that Unicode, as of the list's version, does not list as one emoji: the character before it,
    /// passing back over U+FE0F and skin tones, and the character right after it are emoji by the compiled data (Emoji
    /// and Extended_Pictographic). Every other refused joiner keeps the words it had, the other emoji refusals among
    /// them.
    #[test]
    fn a_zwj_refused_between_two_emoji_says_the_sequence_is_not_one_unicode_lists() {
        const OUTSIDE: &str = "a joiner outside an emoji or a joined word";
        assert_eq!(
            UNLISTED_EMOJI_SEQUENCE,
            "a joiner between emoji that Unicode 17.0 does not list as one emoji"
        );
        for text in [
            // two emoji that are no sequence; the copyright shape, bare and with its selectors
            "\u{1F468}\u{200D}\u{1F469}",
            "\u{1F525}\u{200D}\u{1F525}",
            "\u{A9}\u{200D}\u{AE}",
            "\u{A9}\u{FE0F}\u{200D}\u{AE}\u{FE0F}",
            // what a Mac draws as one emoji and no list holds (the review of #353's second commit, F4)
            "\u{1F469}\u{1F3FD}\u{200D}\u{2764}\u{FE0F}\u{200D}\u{1F468}",
            "\u{1FAF1}\u{1F3FB}\u{200D}\u{1FAF2}\u{1F3FB}",
            "\u{1F468}\u{FE0F}\u{200D}\u{1F4BB}",
            "\u{1F468}\u{200D}\u{1F4BB}\u{FE0F}",
            "\u{1F43B}\u{FE0F}\u{200D}\u{2744}\u{FE0F}",
            // a listed sequence that does not stand whole
            "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}\u{200D}\u{1F476}",
            "\u{1F468}\u{200D}\u{1F467}\u{1F3FD}",
            "\u{1F9D1}\u{200D}\u{1F91D}\u{200D}\u{1F9D1}\u{1F3FF}",
            // a selector and a tone are passed over on the way back, any number of them
            "\u{1F525}\u{1F3FD}\u{200D}\u{1F4BB}",
            "\u{1F468}\u{1F3FD}\u{FE0F}\u{FE0F}\u{200D}\u{1F4BB}",
            "say \u{1F525}\u{200D}\u{1F525} twice",
        ] {
            assert_eq!(prose_fault(text), Some(UNLISTED_EMOJI_SEQUENCE), "{text:?}");
        }
        for text in [
            "\u{1F468}\u{200D}a",
            "a\u{200D}\u{1F468}",
            "\u{1F468}\u{200D}",
            "\u{200D}\u{1F468}",
            "\u{1F468}\u{200D}\u{200D}\u{1F469}",
            "\u{1F468}\u{200C}\u{1F469}",
            "\u{1F468}\u{200D}\u{FE0F}\u{1F467}",
            "\u{1F468}\u{200D} \u{1F469}",
            "\u{1F1EC}\u{200D}\u{1F1E7}",
            "#\u{200D}*",
            "1\u{FE0F}\u{200D}2\u{FE0F}",
            "\u{1F02C}\u{200D}\u{1F469}",
            "\u{1F469}\u{200D}\u{1F02C}",
            "\u{2605}\u{FE0F}\u{200D}\u{1F469}",
            "\u{1F3FD}\u{200D}\u{1F4BB}",
            "a\u{1F3FD}\u{200D}\u{1F4BB}",
            "\u{1F4BB}\u{200D}\u{1F3FD}",
            "\u{2764}\u{FE0E}\u{200D}\u{1F525}",
            "\u{1F468}a\u{FE0F}\u{200D}\u{1F4BB}",
            "pay\u{200D}pal",
            "\u{0915}\u{200D}\u{0937}",
        ] {
            assert_eq!(prose_fault(text), Some(OUTSIDE), "{text:?}");
        }
        // the first character in the way is the one named, and a kept sequence beside it changes nothing
        assert_eq!(
            prose_fault("\u{1F468}\u{200D}\u{1F467} \u{1F525}\u{200D}\u{1F525} pay\u{200C}pal"),
            Some(UNLISTED_EMOJI_SEQUENCE)
        );
        assert_eq!(
            prose_fault("pay\u{200C}pal \u{1F525}\u{200D}\u{1F525}"),
            Some(OUTSIDE)
        );
        // shown, the joiner goes like any other in the way
        assert_eq!(
            shown_prose("\u{1F525}\u{200D}\u{1F525}"),
            "\u{1F525}\u{1F525}"
        );
    }

    /// be2m items 4 and 5: every case's shown form is its text on one line as prose is shown, a control character or a line or
    /// paragraph separator as a space, a joiner in its place kept; and a case is kept exactly when nothing stands in the way,
    /// named by its kind when something does. The file's sha256 is pinned beside the Day's tests. be2m.2: the shown form is
    /// `shown_prose`'s own, with nothing the rule refuses left in it, and the cases hold the rows the ruling names.
    /// The `shown` column is `shown_prose`'s form on one line, which is what the label of an entry read as a part gives
    /// (`plain_text_words` in `day_moves.rs` calls `shown_prose`), and the Day's own test holds the label to the column.
    #[test]
    fn every_joiner_case_is_shown_with_a_joiner_in_its_place_and_names_what_stood_in_the_way() {
        let cases: Vec<serde_json::Value> =
            serde_json::from_str(include_str!("continuous_work/joiner-cases.json")).unwrap();
        assert_eq!(cases.len(), 112);
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
            // be2m.3
            ("a heart on fire with no variation selector", true),
            ("a pirate flag with no variation selector", true),
            (
                "an eye in a speech bubble with neither variation selector",
                true,
            ),
            (
                "ZWJ between a fire and a fire: two emoji, no listed sequence",
                false,
            ),
            (
                "ZWJ between a copyright sign and a registered sign, each with its variation selector",
                false,
            ),
            (
                "a family joined on to a fire: longer than any listed sequence",
                false,
            ),
            (
                "ZWJ after a Myanmar stacker, an invisible stacker by its syllabic category (B)",
                true,
            ),
            (
                "ZWJ after a Thai phinthu, a pure killer by its syllabic category",
                false,
            ),
            (
                "ZWNJ after a Myanmar asat, a pure killer by its syllabic category, with no stacker after it",
                false,
            ),
            (
                "Bengali ra, virama, ZWJ, ya: ZWJ after a virama (B)",
                true,
            ),
            (
                "Bengali ra, ZWJ, virama, ya: a ZWJ between a letter and its virama is in no context of the rule",
                false,
            ),
            (
                "ZWJ after a nukta: a mark of a class that is no virama",
                false,
            ),
            // be2m.4
            (
                "ZWNJ after a Tifinagh consonant joiner, between two Tifinagh letters",
                true,
            ),
            (
                "ZWJ after a Tifinagh consonant joiner, between two Tifinagh letters",
                true,
            ),
            (
                "ZWJ between two Tifinagh letters with no consonant joiner: no property gives a join there",
                false,
            ),
            (
                "ZWNJ after a Myanmar asat on nga and before a stacker and a letter: a Mac was seen to draw no kinzi there",
                true,
            ),
            // be2m.5
            (
                "ZWNJ after a Myanmar asat on ra and before a stacker and a letter",
                true,
            ),
            (
                "ZWNJ after a Myanmar asat on Mon nga and before a stacker and a letter",
                true,
            ),
            (
                "ZWNJ after a Myanmar asat on ma and before a stacker and a letter: a Mac draws nothing new under any letter but nga, ra and Mon nga",
                false,
            ),
            (
                "ZWJ after a Myanmar asat and before a stacker and a letter",
                false,
            ),
            (
                "a person bouncing a ball joined to the female sign, both variation selectors left out: a spelling of a listed sequence, though a Mac draws it as text",
                true,
            ),
        ] {
            assert_eq!(row(name)["kept"], kept, "{name}");
        }
        // be2m.4 item 5: the kind of a ZWJ refused between two emoji, and of the other emoji refusals
        for (name, kind) in [
            (
                "ZWJ between a man and a woman: two emoji, no listed sequence",
                UNLISTED_EMOJI_SEQUENCE,
            ),
            (
                "a couple with a heart, a skin tone on the woman only: one emoji on a Mac, and no listed sequence",
                UNLISTED_EMOJI_SEQUENCE,
            ),
            (
                "ZWJ between an emoji and a letter",
                "a joiner outside an emoji or a joined word",
            ),
            (
                "ZWJ between an emoji and a pictograph that is no emoji",
                "a joiner outside an emoji or a joined word",
            ),
            (
                "ZWJ between two regional indicators",
                "a joiner outside an emoji or a joined word",
            ),
        ] {
            assert_eq!(row(name)["kind"], kind, "{name}");
        }
        assert_eq!(
            cases
                .iter()
                .filter(|case| case["kind"] == UNLISTED_EMOJI_SEQUENCE)
                .count(),
            11
        );
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
