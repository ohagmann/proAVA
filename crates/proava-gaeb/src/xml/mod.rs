//! GAEB DA XML 3.x: read the phases 81–86 (versions 3.0 to 3.3), write 3.3.
//!
//! ```no_run
//! use proava_gaeb::{xml, Phase};
//! let doc = xml::read_file(std::path::Path::new("LV.X83")).unwrap();
//! let x82 = xml::write(&doc, Phase::Dp82).unwrap();
//! # let _ = x82;
//! ```

mod base64;
mod billing;
pub(crate) mod calc;
mod complement_place;
pub(crate) mod dom;
mod hard_wrap;
mod image;
mod reader;
mod restrict;
mod rich_text;
mod standard;
mod writer;

use std::fmt;
use std::path::{Path, PathBuf};

use crate::model::{GaebDocument, Phase};

pub use billing::{
    read_billing, BillingPhase, GaebBillItem, GaebBilling, GaebInvoiceHeader, GaebTakeoff,
    GaebTakeoffRow,
};
pub use calc::{
    after_discount, computed_total, counts_in_total, fill_totals, is_alternative_area,
    is_surcharge, item_total, ordinal, shows_total,
};
pub use restrict::{as_written, as_written_with};
pub use writer::WriteOptions;

/// What the reader noticed besides the document itself.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct XmlReadReport {
    /// GAEBInfo/Version, e.g. "3.2".
    pub version: String,
    /// Namespace of the root element.
    pub namespace: String,
    /// Things skipped or not resolved (German, for the user).
    pub warnings: Vec<String>,
}

#[derive(Debug)]
pub enum GaebXmlError {
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    /// The file is no well-formed XML.
    Syntax { position: u64, message: String },
    /// Well-formed XML, but no GAEB LV.
    NotGaeb(String),
    /// Phase outside 81–86 (e.g. X31, X89).
    UnsupportedPhase(String),
    /// The model cannot be written as GAEB (e.g. OZ mask missing).
    Structure(String),
}

impl fmt::Display for GaebXmlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GaebXmlError::Io { path, source } => {
                write!(
                    f,
                    "Datei {} konnte nicht gelesen oder geschrieben werden: {source}",
                    path.display()
                )
            }
            GaebXmlError::Syntax { position, message } => write!(
                f,
                "Die Datei ist kein gültiges XML (bei Byte {position}): {message}"
            ),
            GaebXmlError::NotGaeb(reason) => write!(f, "Keine GAEB-DA-XML-LV-Datei: {reason}."),
            GaebXmlError::UnsupportedPhase(phase) => write!(
                f,
                "Die Austauschphase X{phase} wird nicht unterstützt (nur X81 bis X86)."
            ),
            GaebXmlError::Structure(reason) => write!(f, "GAEB-Export nicht möglich: {reason}"),
        }
    }
}

impl std::error::Error for GaebXmlError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            GaebXmlError::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

/// Reads a GAEB DA XML file (X81–X86) from its bytes.
pub fn read(bytes: &[u8]) -> Result<GaebDocument, GaebXmlError> {
    read_with_report(bytes).map(|(doc, _)| doc)
}

/// Like [`read`], also returning version and warnings.
pub fn read_with_report(bytes: &[u8]) -> Result<(GaebDocument, XmlReadReport), GaebXmlError> {
    reader::read(bytes)
}

pub fn read_file(path: &Path) -> Result<GaebDocument, GaebXmlError> {
    let bytes = std::fs::read(path).map_err(|source| GaebXmlError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    read(&bytes)
}

/// Writes the document as GAEB DA XML 3.3 (UTF-8) in the given phase.
/// GAEBInfo names proAVA as the program; the date is `doc.date` or today.
/// Images in long texts are left out; see [`write_with`].
pub fn write(doc: &GaebDocument, phase: Phase) -> Result<String, GaebXmlError> {
    writer::write_gaeb_xml(doc, phase, &WriteOptions::default())
}

pub fn write_with(
    doc: &GaebDocument,
    phase: Phase,
    options: &WriteOptions,
) -> Result<String, GaebXmlError> {
    writer::write_gaeb_xml(doc, phase, options)
}

pub fn write_file(doc: &GaebDocument, phase: Phase, path: &Path) -> Result<(), GaebXmlError> {
    write_file_with(doc, phase, path, &WriteOptions::default())
}

pub fn write_file_with(
    doc: &GaebDocument,
    phase: Phase,
    path: &Path,
    options: &WriteOptions,
) -> Result<(), GaebXmlError> {
    let text = write_with(doc, phase, options)?;
    std::fs::write(path, text).map_err(|source| GaebXmlError::Io {
        path: path.to_path_buf(),
        source,
    })
}

/// File extension for the phase, e.g. "X83".
pub fn file_extension(phase: Phase) -> String {
    format!("X{}", phase.number())
}
