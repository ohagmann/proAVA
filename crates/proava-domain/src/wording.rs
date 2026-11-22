//! Wording shared by all layers (protocols, dialogs, the MCP server).

/// „1 Position“, „3 Positionen“ – number with the right word.
pub fn count(number: usize, one: &str, many: &str) -> String {
    format!("{number} {}", if number == 1 { one } else { many })
}

#[cfg(test)]
mod tests {
    #[test]
    fn one_and_many() {
        assert_eq!(super::count(1, "Position", "Positionen"), "1 Position");
        assert_eq!(super::count(0, "Position", "Positionen"), "0 Positionen");
        assert_eq!(super::count(3, "Bild", "Bilder"), "3 Bilder");
    }
}
