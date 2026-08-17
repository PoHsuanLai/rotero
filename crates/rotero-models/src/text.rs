//! Character-safe string trimming.
//!
//! Rust strings are indexed by byte, so slicing at a fixed offset panics when a
//! multi-byte character straddles it. That is not an edge case here: note bodies
//! come from highlighted PDF text, which routinely carries ligatures, en-dashes,
//! curly quotes, and non-Latin scripts, and several of these slices run inside
//! render paths where a panic blanks the whole window rather than surfacing an
//! error.

/// Truncate to at most `max_chars` characters, appending `…` when shortened.
///
/// Counts characters rather than bytes, so the result is always a valid string
/// no matter what the input contains.
pub fn truncate_chars(s: &str, max_chars: usize) -> String {
    let mut chars = s.chars();
    let head: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        format!("{head}…")
    } else {
        head
    }
}

/// The first `n` characters, or the whole string when it is shorter.
///
/// For callers that need a bare prefix with no ellipsis.
pub fn take_chars(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

/// Mask a secret, showing only its first and last few characters.
///
/// Anything too short to mask meaningfully becomes a fixed placeholder rather
/// than leaking most of itself.
pub fn mask_secret(s: &str) -> String {
    let count = s.chars().count();
    if count <= 8 {
        return "•".repeat(count.max(1));
    }
    let head: String = s.chars().take(4).collect();
    let tail: String = s.chars().skip(count - 4).collect();
    format!("{head}...{tail}")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The case that panics with byte slicing: a multi-byte character sitting
    /// exactly on the cut.
    #[test]
    fn truncating_never_splits_a_character() {
        // 'é' is two bytes, so a byte-slice at 5 would land inside it.
        let s = "abcdéfgh";
        assert_eq!(truncate_chars(s, 5), "abcdé…");
        assert_eq!(truncate_chars(s, 4), "abcd…");

        // Scripts where every character is multi-byte.
        assert_eq!(truncate_chars("日本語テキスト", 3), "日本語…");
        assert_eq!(truncate_chars("Ωμέγα", 2), "Ωμ…");

        // Combining marks and emoji must not be split either.
        assert_eq!(truncate_chars("café☕", 5), "café☕");
    }

    #[test]
    fn a_short_string_is_returned_whole() {
        assert_eq!(truncate_chars("short", 20), "short");
        assert_eq!(truncate_chars("", 5), "");
        assert_eq!(truncate_chars("exact", 5), "exact");
    }

    #[test]
    fn taking_a_prefix_is_character_safe() {
        assert_eq!(take_chars("日本語", 2), "日本");
        assert_eq!(take_chars("ab", 10), "ab");
        assert_eq!(take_chars("", 3), "");
    }

    #[test]
    fn masking_keeps_only_the_ends() {
        assert_eq!(mask_secret("sk-ant-0123456789"), "sk-a...6789");
        // Too short to mask without revealing most of it.
        assert_eq!(mask_secret("abcd"), "••••");
        assert_eq!(mask_secret(""), "•");
        // Multi-byte keys must not panic.
        assert_eq!(mask_secret("日本語のキーです"), "••••••••");
    }
}
