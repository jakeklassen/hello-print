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

/// Portrait paper dimensions in millimetres for each canonical size.
const PAPER_SIZES_MM: &[(&str, f64, f64)] = &[
    ("4x6", 101.6, 152.4),
    ("5x7", 127.0, 177.8),
    ("8x10", 203.2, 254.0),
    ("8.5x11", 215.9, 279.4),
    ("a4", 210.0, 297.0),
    ("a5", 148.0, 210.0),
    ("a6", 105.0, 148.0),
];

/// Portrait (width, height) in millimetres for a preset's paper size keyword,
/// accepting aliases (e.g. "letter", "4R"). Returns None for unknown keywords.
pub fn paper_size_mm(keyword: &str) -> Option<(f64, f64)> {
    let lower = keyword.trim().to_lowercase();
    let canonical = KEYWORDS
        .iter()
        .find(|(k, _)| *k == lower)
        .map(|(_, c)| *c)?;
    PAPER_SIZES_MM
        .iter()
        .find(|(name, _, _)| *name == canonical)
        .map(|(_, w, h)| (*w, *h))
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
    fn test_8_5x11() {
        assert_eq!(
            parse_size_keyword("sticker_8.5x11_001.png"),
            Some("8.5x11".to_string())
        );
    }
}
