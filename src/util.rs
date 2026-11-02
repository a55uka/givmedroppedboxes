/// Human-readable byte size, e.g. `1.2 MiB`.
pub fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

/// Make a single path component safe to write to a local filesystem.
///
/// Dropbox names never contain `/`, but be defensive: map separators,
/// control characters and relative-path shorthands to something inert.
pub fn sanitize_component(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c == '/' || c == '\\' || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect();
    let cleaned = cleaned.trim();
    let cleaned = if cleaned.is_empty() || cleaned == "." || cleaned == ".." {
        "_"
    } else {
        cleaned
    };
    cleaned.chars().take(200).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn human_size_units() {
        assert_eq!(human_size(0), "0 B");
        assert_eq!(human_size(512), "512 B");
        assert_eq!(human_size(1024), "1.0 KiB");
        assert_eq!(human_size(1536 * 1024), "1.5 MiB");
        assert_eq!(human_size(5 * 1024 * 1024 * 1024), "5.0 GiB");
    }

    #[test]
    fn sanitize_maps_separators() {
        assert_eq!(sanitize_component("a/b\\c"), "a_b_c");
        assert_eq!(sanitize_component(".."), "_");
        assert_eq!(sanitize_component("."), "_");
        assert_eq!(sanitize_component("  "), "_");
        assert_eq!(sanitize_component("normal.txt"), "normal.txt");
        assert_eq!(sanitize_component("nul\0l"), "nul_l");
    }

    #[test]
    fn sanitize_caps_length() {
        let long = "x".repeat(500);
        assert_eq!(sanitize_component(&long).len(), 200);
    }
}
