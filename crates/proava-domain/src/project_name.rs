use crate::error::DomainError;

pub const MAX_NAME_LEN: usize = 80;

/// A validated project name: never empty, never containing path separators.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectName(String);

impl ProjectName {
    pub fn parse(raw: &str) -> Result<Self, DomainError> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Err(DomainError::EmptyName);
        }

        let length = trimmed.chars().count();
        if length > MAX_NAME_LEN {
            return Err(DomainError::NameTooLong(length));
        }

        if let Some(illegal) = trimmed.chars().find(|character| {
            matches!(
                character,
                '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|'
            ) || character.is_control()
        }) {
            return Err(DomainError::IllegalCharacter(illegal));
        }

        if is_reserved(trimmed) {
            return Err(DomainError::ReservedName(trimmed.to_string()));
        }

        Ok(Self(trimmed.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for ProjectName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::ProjectName;

    #[test]
    fn accepts_a_normal_name() {
        let name = ProjectName::parse("  Rathaus 3 Musterstadt  ").unwrap();
        assert_eq!(name.as_str(), "Rathaus 3 Musterstadt");
    }

    #[test]
    fn refuses_path_separators() {
        assert!(ProjectName::parse("A/B").is_err());
        assert!(ProjectName::parse("A\\B").is_err());
    }

    #[test]
    fn refuses_empty_names() {
        assert!(ProjectName::parse("   ").is_err());
    }
}

/// Windows refuses these as file or folder names, also with an extension
/// ("CON.txt"), and drops a trailing dot.
fn is_reserved(name: &str) -> bool {
    if name.ends_with('.') {
        return true;
    }
    let stem = name
        .split('.')
        .next()
        .unwrap_or(name)
        .trim()
        .to_ascii_uppercase();
    matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || ((stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.len() == 4
            && stem.as_bytes()[3].is_ascii_digit()
            && stem.as_bytes()[3] != b'0')
}

#[cfg(test)]
mod reserved_tests {
    use super::ProjectName;

    #[test]
    fn windows_reserved_names_are_refused() {
        for name in ["CON", "nul", "COM1", "lpt9.txt", "Projekt."] {
            assert!(ProjectName::parse(name).is_err(), "{name}");
        }
        for name in ["Console", "COM0", "Projekt 1.2", "LPT"] {
            assert!(ProjectName::parse(name).is_ok(), "{name}");
        }
    }
}
