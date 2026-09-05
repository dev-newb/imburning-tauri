/// Limit diagnostics by Unicode characters, never by a partial UTF-8 byte.
pub fn excerpt(s: &str, max_chars: usize) -> &str {
    s.char_indices().nth(max_chars).map(|(end, _)| &s[..end]).unwrap_or(s)
}

#[cfg(test)]
mod tests {
    use super::excerpt;

    #[test]
    fn excerpts_keep_multibyte_characters_intact_at_diagnostic_limits() {
        for limit in [200, 400] {
            let prefix = "a".repeat(limit - 1);
            let text = format!("{prefix}🔥étail");
            assert_eq!(excerpt(&text, limit), format!("{prefix}🔥"));
        }
        assert_eq!(excerpt("é🔥", 0), "");
        assert_eq!(excerpt("é🔥", 1), "é");
        assert_eq!(excerpt("é🔥", 2), "é🔥");
        assert_eq!(excerpt("é🔥", 200), "é🔥");
        assert_eq!(excerpt("", 200), "");
    }
}
