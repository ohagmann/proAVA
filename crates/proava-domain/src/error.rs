use std::fmt;

/// Everything the domain itself can refuse.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DomainError {
    EmptyName,
    NameTooLong(usize),
    IllegalCharacter(char),
    /// Not usable as a folder name on Windows (CON, NUL, COM1 …, trailing dot).
    ReservedName(String),
}

impl fmt::Display for DomainError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DomainError::EmptyName => write!(f, "Der Name ist leer."),
            DomainError::NameTooLong(len) => {
                write!(f, "Der Name ist zu lang ({len} Zeichen, höchstens {MAX_NAME_LEN}).")
            }
            DomainError::IllegalCharacter(c) => {
                write!(f, "Der Name enthält ein unerlaubtes Zeichen: {c}")
            }
            DomainError::ReservedName(name) => write!(
                f,
                "„{name}“ ist als Ordnername nicht möglich (unter Windows reserviert oder mit Punkt am Ende)."
            ),
        }
    }
}

impl std::error::Error for DomainError {}

use crate::project_name::MAX_NAME_LEN;
