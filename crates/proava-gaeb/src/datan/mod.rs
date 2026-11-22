//! GAEB 90 (DATAN): fixed-length records of 80 characters, files `*.D81` to
//! `*.D86`. Layout after „Regelungen für den Datenaustausch
//! Leistungsverzeichnis“, Ausgabe 1990 (2. Auflage), see `docs/FACHLICHES.md`.

mod encoding;
mod reader;
mod record;
mod writer;

#[cfg(test)]
mod tests;

use std::fmt;
use std::path::Path;

pub use encoding::{decode, detect, encode, Encoding};
pub use reader::{read, read_with};
pub use writer::{write, write_text};

use crate::model::{GaebDocument, Totals};

/// Problems that make a file unusable. Everything else ends up as a
/// [`Warning`] in the [`ReadReport`].
#[derive(Debug)]
pub enum DatanError {
    Io(std::io::Error),
    /// No opening record 00 was found.
    NotDatan,
    /// Phase in record 00 is not 81–86.
    UnknownPhase(String),
    /// OZ mask in record 00 (or derived from the model) is unusable.
    InvalidMask(String),
    /// The model cannot be written as GAEB 90.
    Unwritable(String),
}

impl fmt::Display for DatanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DatanError::Io(err) => write!(f, "Datei kann nicht gelesen werden: {err}"),
            DatanError::NotDatan => write!(f, "Keine GAEB-90-Datei: der Eröffnungssatz 00 fehlt"),
            DatanError::UnknownPhase(phase) => {
                write!(
                    f,
                    "Unbekannte Datenaustauschphase „{phase}“ (erlaubt 81 bis 86)"
                )
            }
            DatanError::InvalidMask(msg) => write!(f, "{msg}"),
            DatanError::Unwritable(msg) => write!(f, "Als GAEB 90 nicht schreibbar: {msg}"),
        }
    }
}

impl std::error::Error for DatanError {}

impl From<std::io::Error> for DatanError {
    fn from(err: std::io::Error) -> Self {
        DatanError::Io(err)
    }
}

/// A record that was skipped or only partly understood.
#[derive(Debug, Clone, PartialEq)]
pub struct Warning {
    /// 1-based line in the file.
    pub line: usize,
    pub message: String,
}

impl fmt::Display for Warning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Zeile {}: {}", self.line, self.message)
    }
}

/// Result of reading a DATAN file.
#[derive(Debug, Clone)]
pub struct ReadReport {
    pub document: GaebDocument,
    pub encoding: Encoding,
    pub warnings: Vec<Warning>,
    /// Number of items stated in record 99 (ANZTEIL), if given.
    pub declared_item_count: Option<u32>,
    /// Totals of record 99 as written (same as `document.award.boq.totals`).
    pub file_totals: Option<Totals>,
}

/// Reads a file from disk with automatic character set detection.
pub fn read_file(path: impl AsRef<Path>) -> Result<ReadReport, DatanError> {
    let bytes = std::fs::read(path)?;
    read(&bytes)
}
