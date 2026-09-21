//! Turn a PDF-extracted reference-list block into bibliographic fields.
//!
//! `text_block_at` returns typeset lines joined by `\n`. Those breaks are
//! layout, not structure, and in a 340px card they re-wrap into a tall wall
//! of text. This module collapses that dump and, when the string looks like
//! IEEE (quoted title) or APA (`(Year). Title.`), splits out title / authors /
//! year / venue so the citation card can style them.

/// A reference-list entry after whitespace collapse and a best-effort parse.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedReference {
    /// Full cleaned text (newlines collapsed). Always set.
    pub text: String,
    pub authors: Option<String>,
    pub year: Option<String>,
    pub title: Option<String>,
    pub venue: Option<String>,
}

impl ParsedReference {
    /// True when we found a title, so the card can render a heading + meta
    /// instead of a clamped paragraph.
    pub fn has_structure(&self) -> bool {
        self.title.as_ref().is_some_and(|t| t.len() >= 8)
    }
}

/// Parse one reference-list block as extracted by [`super::text_block_at`].
pub fn parse_reference(raw: &str) -> ParsedReference {
    let text = collapse_ws(raw);
    if text.is_empty() {
        return ParsedReference {
            text,
            authors: None,
            year: None,
            title: None,
            venue: None,
        };
    }

    // Dest Y often sits on the tail of the previous entry; start at `[12]`.
    let text = skip_to_enumerator(&text).to_string();
    let body = strip_enumerator(&text).to_string();

    if let Some((authors, title, rest)) = split_quoted(&body) {
        let (venue, year) = year_and_venue(rest);
        return ParsedReference {
            text,
            authors: nonempty(authors),
            year,
            title: nonempty(title),
            venue,
        };
    }

    if let Some((authors, year, rest)) = split_apa(&body) {
        let (title, venue) = split_title_venue(rest);
        return ParsedReference {
            text,
            authors: nonempty(authors),
            year: Some(year.to_string()),
            title,
            venue,
        };
    }

    // Numbered IEEE: Authors. Title. Venue, year.
    let (rest, year) = trailing_year(&body);
    if let Some((authors, title, venue)) = split_ieee(rest) {
        return ParsedReference {
            text,
            authors,
            year,
            title,
            venue,
        };
    }

    ParsedReference {
        text,
        authors: None,
        year,
        title: None,
        venue: nonempty(rest),
    }
}

fn collapse_ws(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn nonempty(s: &str) -> Option<String> {
    let t = s
        .trim()
        .trim_matches(|c: char| matches!(c, ',' | ';' | ':' | ' '))
        .to_string();
    if t.is_empty() { None } else { Some(t) }
}

fn skip_to_enumerator(s: &str) -> &str {
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'[' {
            let rest = &s[i + 1..];
            let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
            if digits > 0 && rest.as_bytes().get(digits) == Some(&b']') {
                return &s[i..];
            }
        }
        i += 1;
    }
    s
}

/// Drop a leading `[12]`, `(12)`, `12.`, or `12)`.
fn strip_enumerator(s: &str) -> &str {
    let s = s.trim();
    if let Some(rest) = s.strip_prefix('[')
        && let Some(close) = rest.find(']')
    {
        return rest[close + 1..].trim_start_matches(['.', ':', ' ', '\t']);
    }
    if let Some(rest) = s.strip_prefix('(') {
        let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
        if digits > 0 && digits < 4 && rest.as_bytes().get(digits) == Some(&b')') {
            return rest[digits + 1..].trim_start_matches(['.', ':', ' ']);
        }
    }
    let digits = s.bytes().take_while(u8::is_ascii_digit).count();
    if digits > 0 && digits < 4 {
        let rest = &s[digits..];
        if rest.starts_with('.') || rest.starts_with(')') {
            return rest[1..].trim_start();
        }
    }
    s
}

fn split_quoted(s: &str) -> Option<(&str, &str, &str)> {
    const PAIRS: [(char, char); 3] = [
        ('\u{201c}', '\u{201d}'), // “ ”
        ('"', '"'),
        ('«', '»'),
    ];
    for (open, close) in PAIRS {
        let Some(i) = s.find(open) else { continue };
        let after_open = i + open.len_utf8();
        let Some(rel) = s[after_open..].find(close) else {
            continue;
        };
        let j = after_open + rel;
        let title = s[after_open..j].trim().trim_end_matches([',', ';']);
        if title.len() < 8 || title.len() > 300 {
            continue;
        }
        let before = s[..i].trim_end_matches([' ', ',', ';']);
        let after = s[j + close.len_utf8()..].trim_start_matches([' ', ',', ';', '.']);
        return Some((before, title, after));
    }
    None
}

fn split_apa(s: &str) -> Option<(&str, &str, &str)> {
    let (start, end, year) = find_paren_year(s)?;
    let authors = s[..start].trim_end_matches([' ', ',', '.']);
    if authors.len() < 2 {
        return None;
    }
    let rest = s[end..].trim_start_matches([' ', '.', ',']);
    Some((authors, year, rest))
}

fn find_paren_year(s: &str) -> Option<(usize, usize, &str)> {
    for (i, c) in s.char_indices() {
        if c != '(' {
            continue;
        }
        let inner = &s[i + 1..];
        let Some(y) = inner.get(..4) else { continue };
        if !(y.starts_with("19") || y.starts_with("20")) || !y.bytes().all(|b| b.is_ascii_digit()) {
            continue;
        }
        let mut year_end = 4;
        if inner.as_bytes().get(4).is_some_and(u8::is_ascii_lowercase) {
            year_end = 5;
        }
        if inner.as_bytes().get(year_end) == Some(&b')') {
            return Some((i, i + 1 + year_end + 1, &inner[..year_end]));
        }
    }
    None
}

/// Split `Title of the work. Venue, details` on a sentence period.
fn split_title_venue(s: &str) -> (Option<String>, Option<String>) {
    let mut from = 0;
    while let Some(rel) = s[from..].find(". ") {
        let abs = from + rel;
        let after = &s[abs + 2..];
        let next_upper = after
            .chars()
            .next()
            .is_some_and(|c| c.is_uppercase() || c.is_ascii_digit());
        if next_upper && !ends_with_initial(&s[..abs]) {
            let title = s[..abs].trim().trim_end_matches([' ', ',']).to_string();
            let venue = s[abs + 2..].trim().trim_end_matches('.').to_string();
            if title.len() >= 8 {
                return (
                    Some(title),
                    if venue.is_empty() { None } else { Some(venue) },
                );
            }
        }
        from = abs + 2;
    }
    nonempty(s).map_or((None, None), |t| (Some(t), None))
}

fn ends_with_initial(before: &str) -> bool {
    let last = before.rsplit([' ', ',']).next().unwrap_or("");
    last.len() == 1 && last.chars().next().is_some_and(char::is_uppercase)
}

fn trailing_year(s: &str) -> (&str, Option<String>) {
    let t = s.trim_end_matches(['.', ',', ' ', ';']);
    let Some(y) = t.get(t.len().saturating_sub(4)..) else {
        return (s, None);
    };
    if (y.starts_with("19") || y.starts_with("20")) && y.bytes().all(|b| b.is_ascii_digit()) {
        let rest = t[..t.len() - 4].trim_end_matches([' ', ',', ';', '.']);
        return (rest, Some(y.to_string()));
    }
    (s, None)
}

/// `Authors. Title. Venue` — the usual numbered-reference shape once the
/// leading `[12]` and trailing year are gone. Splits on sentence periods,
/// skipping initials (`J.`).
fn split_ieee(s: &str) -> Option<(Option<String>, Option<String>, Option<String>)> {
    let (authors, rest) = split_once_sentence(s)?;
    if authors.len() < 4 {
        return None;
    }
    let (title, venue) = match split_once_sentence(rest) {
        Some((title, venue)) if title.len() >= 8 => (nonempty(title), nonempty(venue)),
        _ => return None,
    };
    Some((nonempty(authors), title, venue))
}

fn split_once_sentence(s: &str) -> Option<(&str, &str)> {
    let mut from = 0;
    while let Some(rel) = s[from..].find(". ") {
        let abs = from + rel;
        if !ends_with_initial(&s[..abs]) {
            return Some((s[..abs].trim(), s[abs + 2..].trim()));
        }
        from = abs + 2;
    }
    None
}

fn year_and_venue(rest: &str) -> (Option<String>, Option<String>) {
    if let Some((_, _, year)) = find_paren_year(rest) {
        let without = rest.replace(&format!("({year})"), "");
        let venue = nonempty(&collapse_ws(&without));
        return (venue, Some(year.to_string()));
    }
    let (rest, year) = trailing_year(rest);
    (nonempty(rest), year)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collapses_pdf_line_breaks() {
        let p = parse_reference(
            "Vaswani, A.,\nShazeer, N.\n(2017).\nAttention is all you need.\nNeurIPS.",
        );
        assert!(!p.text.contains('\n'));
        assert_eq!(p.year.as_deref(), Some("2017"));
        assert_eq!(p.title.as_deref(), Some("Attention is all you need"));
        assert_eq!(p.authors.as_deref(), Some("Vaswani, A., Shazeer, N"));
        assert_eq!(p.venue.as_deref(), Some("NeurIPS"));
    }

    #[test]
    fn strips_numeric_enumerator() {
        let p =
            parse_reference("[12] A. Vaswani et al., “Attention is all you need,” NeurIPS, 2017.");
        assert_eq!(p.title.as_deref(), Some("Attention is all you need"));
        assert_eq!(p.authors.as_deref(), Some("A. Vaswani et al."));
        assert_eq!(p.year.as_deref(), Some("2017"));
        assert_eq!(p.venue.as_deref(), Some("NeurIPS"));
        assert!(p.has_structure());
    }

    #[test]
    fn parses_curly_ieee_quotes() {
        let p = parse_reference(
            "12. A. Vaswani et al., \u{201c}Attention is all you need,\u{201d} in NeurIPS, 2017.",
        );
        assert_eq!(p.title.as_deref(), Some("Attention is all you need"));
        assert_eq!(p.year.as_deref(), Some("2017"));
    }

    #[test]
    fn parses_apa_author_year() {
        let p = parse_reference(
            "Vaswani, A., Shazeer, N., Parmar, N. (2017). Attention is all you need. Advances in Neural Information Processing Systems, 30.",
        );
        assert_eq!(p.year.as_deref(), Some("2017"));
        assert_eq!(p.title.as_deref(), Some("Attention is all you need"));
        assert!(
            p.venue
                .as_deref()
                .unwrap()
                .starts_with("Advances in Neural")
        );
        assert!(p.authors.as_ref().unwrap().starts_with("Vaswani"));
    }

    #[test]
    fn skips_previous_entry_tail() {
        let p = parse_reference(
            "implicit models. arXiv:2010.02502, 2020. 3 [40] Ethan Perez, Florian Strub. Film: Visual reasoning. In AAAI, 2018.",
        );
        assert_eq!(p.title.as_deref(), Some("Film: Visual reasoning"));
        assert!(p.authors.as_ref().unwrap().starts_with("Ethan Perez"));
    }

    #[test]
    fn parses_numbered_ieee() {
        let p = parse_reference(
            "[40] Ethan Perez, Florian Strub, Harm de Vries, Vincent Dumoulin, and Aaron Courville. Film: Visual reasoning with a general conditioning layer. In AAAI, 2018.",
        );
        assert_eq!(p.year.as_deref(), Some("2018"));
        assert_eq!(
            p.title.as_deref(),
            Some("Film: Visual reasoning with a general conditioning layer")
        );
        assert!(p.authors.as_ref().unwrap().starts_with("Ethan Perez"));
        assert_eq!(p.venue.as_deref(), Some("In AAAI"));
        assert!(p.has_structure());
    }

    #[test]
    fn unstructured_is_cleaned_not_split() {
        let p = parse_reference("See the supplementary material for details.");
        assert!(!p.has_structure());
        assert_eq!(p.text, "See the supplementary material for details.");
        assert!(p.title.is_none());
    }

    #[test]
    fn empty_input() {
        let p = parse_reference("  \n  ");
        assert!(p.text.is_empty());
        assert!(!p.has_structure());
    }
}
