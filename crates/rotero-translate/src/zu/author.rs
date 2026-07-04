//! Author-name splitting, ported from Zotero's `ZU.cleanAuthor`.
//!
//! Splits a name into first/last, handling both "Last, First" (comma form) and
//! "First Last" (space form), and keeping surname particles ("van", "de la",
//! "von der") attached to the last name rather than the first.

/// A structured author name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CleanedAuthor {
    pub first_name: String,
    pub last_name: String,
    pub creator_type: String,
}

/// Surname particles that stay with the last name. Lowercased for comparison.
const PARTICLES: &[&str] = &[
    "van", "von", "der", "den", "de", "del", "della", "di", "da", "la", "le",
    "du", "des", "el", "al", "bin", "ibn", "ter", "ten", "af", "zu",
];

/// Split `name` into a structured author of the given `creator_type`.
///
/// If `use_comma` is true and the name contains a comma, it's parsed as
/// "Last, First"; otherwise as "First … Last" with trailing particles folded
/// into the surname.
pub fn clean_author(name: &str, creator_type: &str, use_comma: bool) -> CleanedAuthor {
    let name = collapse_ws(name);
    let ctype = if creator_type.is_empty() {
        "author".to_string()
    } else {
        creator_type.to_string()
    };

    if use_comma && name.contains(',') {
        let (last, first) = name.split_once(',').unwrap();
        return CleanedAuthor {
            first_name: first.trim().to_string(),
            last_name: last.trim().to_string(),
            creator_type: ctype,
        };
    }

    let tokens: Vec<&str> = name.split(' ').filter(|s| !s.is_empty()).collect();
    match tokens.len() {
        0 => CleanedAuthor {
            first_name: String::new(),
            last_name: String::new(),
            creator_type: ctype,
        },
        1 => CleanedAuthor {
            first_name: String::new(),
            last_name: tokens[0].to_string(),
            creator_type: ctype,
        },
        _ => {
            // The surname starts at the earliest particle that appears after
            // the first given-name token (so "Charles de la Vallée Poussin" →
            // first "Charles", last "de la Vallée Poussin"). If there's no
            // particle, only the final token is the surname.
            let last_start = (1..tokens.len() - 1)
                .find(|&i| is_particle(tokens[i]))
                .unwrap_or(tokens.len() - 1);
            let first = tokens[..last_start].join(" ");
            let last = tokens[last_start..].join(" ");
            CleanedAuthor {
                first_name: first,
                last_name: last,
                creator_type: ctype,
            }
        }
    }
}

fn is_particle(token: &str) -> bool {
    let lower = token.to_lowercase();
    PARTICLES.contains(&lower.as_str())
}

/// Collapse runs of whitespace to single spaces and trim.
fn collapse_ws(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ca(first: &str, last: &str) -> CleanedAuthor {
        CleanedAuthor {
            first_name: first.to_string(),
            last_name: last.to_string(),
            creator_type: "author".to_string(),
        }
    }

    #[test]
    fn comma_form() {
        assert_eq!(clean_author("Vaswani, Ashish", "author", true), ca("Ashish", "Vaswani"));
        assert_eq!(clean_author("van der Berg, Jan", "author", true), ca("Jan", "van der Berg"));
    }

    #[test]
    fn space_form() {
        assert_eq!(clean_author("Ashish Vaswani", "author", false), ca("Ashish", "Vaswani"));
        assert_eq!(clean_author("John Ronald Reuel Tolkien", "author", false),
                   ca("John Ronald Reuel", "Tolkien"));
    }

    #[test]
    fn particles_stay_with_surname() {
        assert_eq!(clean_author("Ludwig van Beethoven", "author", false),
                   ca("Ludwig", "van Beethoven"));
        assert_eq!(clean_author("Charles de la Vallée Poussin", "author", false),
                   ca("Charles", "de la Vallée Poussin"));
        assert_eq!(clean_author("Vincent van der Waals", "author", false),
                   ca("Vincent", "van der Waals"));
    }

    #[test]
    fn single_and_empty() {
        assert_eq!(clean_author("Aristotle", "author", false), ca("", "Aristotle"));
        assert_eq!(clean_author("", "author", false), ca("", ""));
    }

    #[test]
    fn creator_type_preserved() {
        assert_eq!(clean_author("A B", "editor", false).creator_type, "editor");
        assert_eq!(clean_author("A B", "", false).creator_type, "author");
    }

    #[test]
    fn collapses_whitespace() {
        assert_eq!(clean_author("  Ashish   Vaswani  ", "author", false), ca("Ashish", "Vaswani"));
    }
}
