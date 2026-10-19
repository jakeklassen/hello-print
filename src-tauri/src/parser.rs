/// Built-in size keywords and their aliases.
/// Each entry is (keyword, canonical_name).
const KEYWORDS: &[(&str, &str)] = &[
    ("4x6", "4x6"),
    ("4r", "4x6"),
    ("kg", "4x6"),
    ("5x7", "5x7"),
    ("5r", "5x7"),
    ("8x10", "8x10"),
    ("8.5x11", "8.5x11"),
    ("letter", "8.5x11"),
    ("a4", "a4"),
    ("a5", "a5"),
    ("a6", "a6"),
];

/// A named paper size, portrait.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
pub struct PaperSize {
    /// Stable, filename-safe name (e.g. "letter", "4x6").
    pub slug: &'static str,
    /// Canonical filename keyword, if files can be routed by this size.
    pub keyword: Option<&'static str>,
    pub label: &'static str,
    pub width_mm: f64,
    pub height_mm: f64,
}

const fn size(
    slug: &'static str,
    keyword: Option<&'static str>,
    label: &'static str,
    width_mm: f64,
    height_mm: f64,
) -> PaperSize {
    PaperSize {
        slug,
        keyword,
        label,
        width_mm,
        height_mm,
    }
}

/// Named paper sizes. Legal and Half Letter have no filename keyword yet but
/// are common enough to have test sheets.
pub const PAPER_SIZES: &[PaperSize] = &[
    size("4x6", Some("4x6"), "4 x 6 in", 101.6, 152.4),
    size("5x7", Some("5x7"), "5 x 7 in", 127.0, 177.8),
    size("8x10", Some("8x10"), "8 x 10 in", 203.2, 254.0),
    size(
        "letter",
        Some("8.5x11"),
        "Letter (8.5 x 11 in)",
        215.9,
        279.4,
    ),
    size("legal", None, "Legal (8.5 x 14 in)", 215.9, 355.6),
    size(
        "half-letter",
        None,
        "Half Letter (5.5 x 8.5 in)",
        139.7,
        215.9,
    ),
    size("a4", Some("a4"), "A4", 210.0, 297.0),
    size("a5", Some("a5"), "A5", 148.0, 210.0),
    size("a6", Some("a6"), "A6", 105.0, 148.0),
];

/// Finds a paper size by slug or by filename keyword, accepting aliases
/// (e.g. "letter", "4R"). Case-insensitive.
pub fn find_paper_size(name: &str) -> Option<&'static PaperSize> {
    let lower = name.trim().to_lowercase();
    if let Some(size) = PAPER_SIZES.iter().find(|s| s.slug == lower) {
        return Some(size);
    }
    let canonical = KEYWORDS
        .iter()
        .find(|(k, _)| *k == lower)
        .map(|(_, c)| *c)?;
    PAPER_SIZES.iter().find(|s| s.keyword == Some(canonical))
}

/// Portrait (width, height) in millimetres for a preset's paper size keyword,
/// accepting aliases (e.g. "letter", "4R"). Returns None for unknown keywords.
pub fn paper_size_mm(keyword: &str) -> Option<(f64, f64)> {
    find_paper_size(keyword).map(|s| (s.width_mm, s.height_mm))
}

/// Delimiter characters that separate tokens in filenames.
fn is_delimiter(c: char) -> bool {
    matches!(c, ' ' | '_' | '-' | '.')
}

/// Parse a filename (without extension) and return the first matching size keyword.
pub fn parse_size_keyword(filename: &str) -> Option<String> {
    // Strip extension
    let name = if let Some(pos) = filename.rfind('.') {
        let ext = &filename[pos + 1..];
        // Only strip if it looks like a file extension
        if ext.len() <= 5 {
            &filename[..pos]
        } else {
            filename
        }
    } else {
        filename
    };

    let lower = name.to_lowercase();

    // Sort keywords by length descending so longer matches win (e.g., "8.5x11" before "a5")
    let mut sorted_keywords: Vec<_> = KEYWORDS.to_vec();
    sorted_keywords.sort_by(|a, b| b.0.len().cmp(&a.0.len()));

    for (keyword, canonical) in &sorted_keywords {
        // Find keyword in the lowered filename
        let mut search_from = 0;
        while let Some(pos) = lower[search_from..].find(keyword) {
            let abs_pos = search_from + pos;
            let end_pos = abs_pos + keyword.len();

            // Check delimiter boundaries
            let start_ok = abs_pos == 0 || is_delimiter(lower.as_bytes()[abs_pos - 1] as char);
            let end_ok = end_pos >= lower.len() || is_delimiter(lower.as_bytes()[end_pos] as char);

            if start_ok && end_ok {
                return Some(canonical.to_string());
            }

            search_from = abs_pos + 1;
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_keywords() {
        assert_eq!(
            parse_size_keyword("order_839_4x6_002.png"),
            Some("4x6".to_string())
        );
        assert_eq!(
            parse_size_keyword("photo-A4-001.jpg"),
            Some("a4".to_string())
        );
        assert_eq!(
            parse_size_keyword("batch.5x7.001.png"),
            Some("5x7".to_string())
        );
        assert_eq!(
            parse_size_keyword("my_letter_print.tiff"),
            Some("8.5x11".to_string())
        );
    }

    #[test]
    fn test_aliases() {
        assert_eq!(
            parse_size_keyword("photo_4R_001.jpg"),
            Some("4x6".to_string())
        );
        assert_eq!(
            parse_size_keyword("batch-KG-print.png"),
            Some("4x6".to_string())
        );
        assert_eq!(parse_size_keyword("image_5R.jpg"), Some("5x7".to_string()));
    }

    #[test]
    fn test_case_insensitive() {
        assert_eq!(parse_size_keyword("PHOTO_4X6.jpg"), Some("4x6".to_string()));
        assert_eq!(parse_size_keyword("print_a4.png"), Some("a4".to_string()));
    }

    #[test]
    fn test_no_match() {
        assert_eq!(parse_size_keyword("random_photo.jpg"), None);
        assert_eq!(parse_size_keyword("document.pdf"), None);
    }

    #[test]
    fn test_first_match_wins() {
        assert_eq!(
            parse_size_keyword("photo_4x6_A4.jpg"),
            Some("4x6".to_string())
        );
    }

    #[test]
    fn test_paper_size_mm() {
        assert_eq!(paper_size_mm("4x6"), Some((101.6, 152.4)));
        assert_eq!(paper_size_mm(" 4R "), Some((101.6, 152.4)));
        assert_eq!(paper_size_mm("Letter"), Some((215.9, 279.4)));
        assert_eq!(paper_size_mm("8.5x11"), Some((215.9, 279.4)));
        assert_eq!(paper_size_mm("A6"), Some((105.0, 148.0)));
        assert_eq!(paper_size_mm("poster"), None);
    }

    #[test]
    fn test_find_paper_size() {
        assert_eq!(find_paper_size("Legal").unwrap().height_mm, 355.6);
        assert_eq!(find_paper_size("half-letter").unwrap().width_mm, 139.7);
        assert_eq!(find_paper_size("4R").unwrap().slug, "4x6");
        assert_eq!(find_paper_size("letter").unwrap().slug, "letter");
        assert_eq!(find_paper_size("8.5x11").unwrap().slug, "letter");
        assert!(find_paper_size("custom").is_none());
    }

    #[test]
    fn every_keyword_has_a_paper_size() {
        for (_, canonical) in KEYWORDS {
            assert!(
                PAPER_SIZES.iter().any(|s| s.keyword == Some(*canonical)),
                "{canonical} has no paper size"
            );
        }
    }

    #[test]
    fn test_8_5x11() {
        assert_eq!(
            parse_size_keyword("sticker_8.5x11_001.png"),
            Some("8.5x11".to_string())
        );
    }
}
