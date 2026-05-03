//! Folder and file naming rules.
//!
//! Convention: `Title [tmdb-{id}]` (Plex/Jellyfin compatible).

use once_cell::sync::Lazy;
use regex::Regex;
use unicode_normalization::UnicodeNormalization;

/// Characters that are illegal on Windows file/folder names. We strip them
/// across all platforms for consistency between dev (Linux/macOS) and prod
/// (the user's external HD, often NTFS/exFAT).
const ILLEGAL: &[char] = &['<', '>', ':', '"', '/', '\\', '|', '?', '*'];

/// Trailing dot/space is illegal on Windows folder names.
fn trim_trailing(s: &str) -> String {
    s.trim_end_matches(|c: char| c == '.' || c.is_whitespace())
        .to_string()
}

/// Sanitize a string for use as a single path segment (folder or filename
/// without extension). Never returns an empty string — falls back to `_`.
pub fn sanitize_segment(input: &str) -> String {
    // Normalize to NFC so things like `é` are a single codepoint.
    let nfc: String = input.nfc().collect();
    // Drop control characters and replace illegal ones with a space.
    let cleaned: String = nfc
        .chars()
        .map(|c| {
            if c.is_control() {
                ' '
            } else if ILLEGAL.contains(&c) {
                ' '
            } else {
                c
            }
        })
        .collect();
    // Collapse repeated whitespace.
    let collapsed: String = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    let trimmed = trim_trailing(&collapsed);
    if trimmed.is_empty() {
        "_".to_string()
    } else {
        trimmed
    }
}

/// Build the canonical folder name for a media entry.
///
/// ```text
/// folder_name("O Senhor dos Anéis", 120) == "O Senhor dos Anéis [tmdb-120]"
/// ```
pub fn folder_name(title: &str, tmdb_id: i64) -> String {
    let safe = sanitize_segment(title);
    format!("{safe} [tmdb-{tmdb_id}]")
}

static TMDB_TAG: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\[tmdb-(\d+)\]\s*$").expect("valid regex"));

/// Extract the TMDB id from a folder name, if it matches the convention.
/// Whitespace at the end is tolerated.
pub fn parse_tmdb_id(folder: &str) -> Option<i64> {
    let trimmed = folder.trim();
    TMDB_TAG
        .captures(trimmed)
        .and_then(|c| c.get(1))
        .and_then(|m| m.as_str().parse::<i64>().ok())
}

/// Returns true if the folder name matches the catalogued convention
/// (`<non-empty title> [tmdb-<id>]`).
pub fn is_catalogued_folder(folder: &str) -> bool {
    let trimmed = folder.trim();
    if let Some(m) = TMDB_TAG.find(trimmed) {
        // The portion before the tag must be a non-empty title.
        let prefix = trimmed[..m.start()].trim();
        !prefix.is_empty()
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folder_name_combines_title_and_id() {
        assert_eq!(folder_name("Inception", 27205), "Inception [tmdb-27205]");
    }

    #[test]
    fn folder_name_strips_illegal_chars() {
        assert_eq!(
            folder_name("A:B/C\\D?E*F", 1),
            "A B C D E F [tmdb-1]"
        );
    }

    #[test]
    fn folder_name_handles_unicode() {
        // Brazilian Portuguese title with accents.
        assert_eq!(
            folder_name("Cidade de Deus", 598),
            "Cidade de Deus [tmdb-598]"
        );
    }

    #[test]
    fn folder_name_collapses_whitespace_and_falls_back() {
        assert_eq!(folder_name("   ", 9), "_ [tmdb-9]");
        assert_eq!(folder_name("Hello   World", 9), "Hello World [tmdb-9]");
    }

    #[test]
    fn folder_name_strips_trailing_dot_and_space() {
        assert_eq!(folder_name("Title. ", 1), "Title [tmdb-1]");
    }

    #[test]
    fn parse_tmdb_id_extracts_when_present() {
        assert_eq!(parse_tmdb_id("Inception [tmdb-27205]"), Some(27205));
        assert_eq!(parse_tmdb_id("Some Title [tmdb-1]"), Some(1));
        // Tolerate trailing whitespace.
        assert_eq!(parse_tmdb_id("Foo [tmdb-42]   "), Some(42));
    }

    #[test]
    fn parse_tmdb_id_returns_none_for_uncatalogued() {
        assert_eq!(parse_tmdb_id("Inception"), None);
        assert_eq!(parse_tmdb_id("Inception (2010)"), None);
        assert_eq!(parse_tmdb_id("[tmdb-]"), None);
        assert_eq!(parse_tmdb_id(""), None);
    }

    #[test]
    fn is_catalogued_folder_requires_title_prefix() {
        assert!(is_catalogued_folder("Inception [tmdb-27205]"));
        assert!(!is_catalogued_folder("[tmdb-27205]"));
        assert!(!is_catalogued_folder("Inception"));
    }
}
