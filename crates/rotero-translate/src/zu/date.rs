//! Date parsing, a pragmatic port of the common paths of Zotero's
//! `Zotero.Date.strToDate`. Handles the formats real translators emit: ISO
//! (`2020-05-01`), slash (`2020/05/01`, `05/01/2020`), month-name
//! (`January 2020`, `1 Jan 2020`), and year-only. Enough to give correct
//! `year`/`month`/`day` for the overwhelming majority of scholarly pages.

/// A parsed date. Fields are `None` when the input didn't specify them.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ParsedDate {
    pub year: Option<i32>,
    pub month: Option<u32>, // 1-12
    pub day: Option<u32>,   // 1-31
}

const MONTHS: &[(&str, u32)] = &[
    ("january", 1), ("jan", 1),
    ("february", 2), ("feb", 2),
    ("march", 3), ("mar", 3),
    ("april", 4), ("apr", 4),
    ("may", 5),
    ("june", 6), ("jun", 6),
    ("july", 7), ("jul", 7),
    ("august", 8), ("aug", 8),
    ("september", 9), ("sep", 9), ("sept", 9),
    ("october", 10), ("oct", 10),
    ("november", 11), ("nov", 11),
    ("december", 12), ("dec", 12),
];

/// Parse a date string into a [`ParsedDate`]. Returns an all-`None` date if
/// nothing recognizable is found.
pub fn str_to_date(input: &str) -> ParsedDate {
    let s = input.trim();
    if s.is_empty() {
        return ParsedDate::default();
    }

    // ISO / slash numeric: 2020-05-01, 2020/05/01, 2020.05.01
    if let Some(d) = parse_numeric(s) {
        return d;
    }

    // Month-name forms: "January 2020", "1 Jan 2020", "Jan 1, 2020".
    parse_with_month_name(s)
}

/// Parse purely numeric dates separated by `-`, `/`, or `.`.
fn parse_numeric(s: &str) -> Option<ParsedDate> {
    let parts: Vec<&str> = s
        .split(['-', '/', '.'])
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .collect();
    if parts.is_empty() || !parts.iter().all(|p| p.chars().all(|c| c.is_ascii_digit())) {
        return None;
    }
    let nums: Vec<i64> = parts.iter().filter_map(|p| p.parse().ok()).collect();
    if nums.len() != parts.len() {
        return None;
    }

    match nums.as_slice() {
        // YYYY
        [y] if is_year(*y) => Some(ParsedDate { year: Some(*y as i32), ..Default::default() }),
        // YYYY-MM
        [y, m] if is_year(*y) => Some(ParsedDate {
            year: Some(*y as i32),
            month: valid_month(*m),
            ..Default::default()
        }),
        // YYYY-MM-DD
        [y, m, d] if is_year(*y) => Some(ParsedDate {
            year: Some(*y as i32),
            month: valid_month(*m),
            day: valid_day(*d),
        }),
        // MM/DD/YYYY (US) or DD/MM/YYYY — ambiguous; assume MM/DD/YYYY (US),
        // the dominant form in the citation metadata we see.
        [a, b, y] if is_year(*y) => Some(ParsedDate {
            year: Some(*y as i32),
            month: valid_month(*a).or_else(|| valid_month(*b)),
            day: valid_day(*b).or_else(|| valid_day(*a)),
        }),
        _ => None,
    }
}

/// Parse dates containing a month name, e.g. "January 2020", "1 Jan 2020".
fn parse_with_month_name(s: &str) -> ParsedDate {
    let lower = s.to_lowercase();
    let mut result = ParsedDate::default();

    // Month name.
    for (name, num) in MONTHS {
        if word_contains(&lower, name) {
            result.month = Some(*num);
            break;
        }
    }

    // Year: first standalone 4-digit run in a plausible range.
    if let Some(y) = extract_year(s) {
        result.year = Some(y);
    }

    // Day: a 1-2 digit number that isn't the year.
    for tok in s.split(|c: char| !c.is_ascii_digit()) {
        if tok.is_empty() {
            continue;
        }
        if let Ok(n) = tok.parse::<u32>()
            && (1..=31).contains(&n)
            && Some(n as i32) != result.year
        {
            result.day = Some(n);
            break;
        }
    }

    result
}

fn is_year(n: i64) -> bool {
    (1000..=2100).contains(&n)
}

fn valid_month(n: i64) -> Option<u32> {
    (1..=12).contains(&n).then_some(n as u32)
}

fn valid_day(n: i64) -> Option<u32> {
    (1..=31).contains(&n).then_some(n as u32)
}

/// Whether `haystack` contains `word` as a whole word (bounded by non-letters).
fn word_contains(haystack: &str, word: &str) -> bool {
    haystack
        .match_indices(word)
        .any(|(i, _)| {
            let before_ok = i == 0
                || !haystack[..i].chars().next_back().is_some_and(|c| c.is_alphabetic());
            let after = i + word.len();
            let after_ok = after >= haystack.len()
                || !haystack[after..].chars().next().is_some_and(|c| c.is_alphabetic());
            before_ok && after_ok
        })
}

/// First 4-digit run in 1000..=2100.
fn extract_year(s: &str) -> Option<i32> {
    let bytes = s.as_bytes();
    let mut i = 0;
    while i + 4 <= bytes.len() {
        if bytes[i].is_ascii_digit()
            && let Ok(year) = s[i..i + 4].parse::<i32>()
            && (1000..=2100).contains(&year)
            // ensure not part of a longer number
            && (i + 4 == bytes.len() || !bytes[i + 4].is_ascii_digit())
            && (i == 0 || !bytes[i - 1].is_ascii_digit())
        {
            return Some(year);
        }
        i += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ymd(y: i32, m: u32, d: u32) -> ParsedDate {
        ParsedDate { year: Some(y), month: Some(m), day: Some(d) }
    }

    #[test]
    fn iso() {
        assert_eq!(str_to_date("2020-05-01"), ymd(2020, 5, 1));
        assert_eq!(str_to_date("2019-12"), ParsedDate { year: Some(2019), month: Some(12), day: None });
        assert_eq!(str_to_date("2021"), ParsedDate { year: Some(2021), ..Default::default() });
    }

    #[test]
    fn slash_us() {
        assert_eq!(str_to_date("05/01/2020"), ymd(2020, 5, 1));
        assert_eq!(str_to_date("2020/05/01"), ymd(2020, 5, 1));
    }

    #[test]
    fn month_names() {
        assert_eq!(str_to_date("January 2020"), ParsedDate { year: Some(2020), month: Some(1), day: None });
        assert_eq!(str_to_date("1 Jan 2020"), ymd(2020, 1, 1));
        assert_eq!(str_to_date("Jan 15, 2020"), ymd(2020, 1, 15));
        assert_eq!(str_to_date("15 September 2019"), ymd(2019, 9, 15));
    }

    #[test]
    fn year_only_and_junk() {
        assert_eq!(str_to_date("published in 2018"), ParsedDate { year: Some(2018), ..Default::default() });
        assert_eq!(str_to_date("no date here"), ParsedDate::default());
        assert_eq!(str_to_date(""), ParsedDate::default());
    }

    #[test]
    fn does_not_grab_year_as_day() {
        let d = str_to_date("March 2020");
        assert_eq!(d.year, Some(2020));
        assert_eq!(d.month, Some(3));
        assert_eq!(d.day, None);
    }
}
