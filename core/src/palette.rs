//! The fixed retro palette every sound tile draws its color from.

pub const PALETTE: [&str; 10] = [
    "#FF595E", // red
    "#FF924C", // orange
    "#FFCA3A", // yellow
    "#8AC926", // green
    "#2EC4B6", // teal
    "#1982C4", // blue
    "#6A4C93", // purple
    "#FF6FB5", // pink
    "#00BBF9", // cyan
    "#F4A261", // sand
];

/// True when `s` is one of the palette colors (case-insensitive).
pub fn is_palette_color(s: &str) -> bool {
    PALETTE.iter().any(|p| p.eq_ignore_ascii_case(s))
}

/// The palette entry used least among `used` (ties go to the earliest palette entry).
pub fn next_color(used: &[&str]) -> &'static str {
    let count = |p: &str| used.iter().filter(|u| u.eq_ignore_ascii_case(p)).count();
    // `min_by_key` keeps the first of equal minima, which is the palette order tie-break.
    PALETTE.iter().copied().min_by_key(|p| count(p)).unwrap_or(PALETTE[0])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palette_has_ten_distinct_hex_colors() {
        assert_eq!(PALETTE.len(), 10);
        for (i, c) in PALETTE.iter().enumerate() {
            assert_eq!(c.len(), 7, "{c}");
            assert!(c.starts_with('#') && c[1..].chars().all(|h| h.is_ascii_hexdigit() && !h.is_ascii_lowercase()), "{c}");
            assert!(!PALETTE[..i].contains(c), "duplicate {c}");
        }
    }

    #[test]
    fn palette_membership_ignores_case() {
        assert!(is_palette_color("#FF595E"));
        assert!(is_palette_color("#ff595e"));
        assert!(!is_palette_color("#FF595F"));
        assert!(!is_palette_color("FF595E"));
        assert!(!is_palette_color(""));
    }

    #[test]
    fn next_color_is_least_used_then_earliest() {
        assert_eq!(next_color(&[]), "#FF595E");
        assert_eq!(next_color(&["#FF595E"]), "#FF924C");
        assert_eq!(next_color(&["#ff595e", "#FF924C"]), "#FFCA3A");
        let all: Vec<&str> = PALETTE.to_vec();
        assert_eq!(next_color(&all), "#FF595E", "all used once: wraps to the first");
        let mut more = all.clone();
        more.push("#FF595E");
        assert_eq!(next_color(&more), "#FF924C", "red used twice, orange is the earliest at one");
        assert_eq!(next_color(&["#123456"]), "#FF595E", "unknown colors count for nothing");
    }
}
