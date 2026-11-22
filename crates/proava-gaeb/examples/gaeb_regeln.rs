//! Fachregeln R1–R13 (`proava_gaeb::rules`) für einzelne GAEB-Dateien – je
//! Befund eine Zeile, Felder durch Tabulator getrennt: Datei, Kennung, Schwere,
//! OZ, Text, Fundstelle. Für Vergleiche „Original gegen
//! geschriebene Datei“ (neue Befunde = Zeilen nur in der zweiten Datei).
//! Nur lesen; Exit 1, wenn eine Datei nicht lesbar ist.
//!
//! ```sh
//! cargo run -p proava-gaeb --example gaeb_regeln -- original.X83 neu.X83
//! ```

use std::path::Path;

use proava_gaeb::rules::{self, Finding};
use proava_gaeb::{datan, xml, GaebDocument};

fn read(path: &Path, bytes: &[u8]) -> Result<(GaebDocument, Vec<Finding>), String> {
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if extension.starts_with('d') {
        let report = datan::read(bytes).map_err(|error| error.to_string())?;
        let raw = rules::check_datan(bytes);
        Ok((report.document, raw))
    } else {
        let doc = xml::read(bytes).map_err(|error| error.to_string())?;
        Ok((doc, rules::check_xml(bytes)))
    }
}

fn clean(text: &str) -> String {
    text.replace(['\t', '\n', '\r'], " ")
}

fn main() {
    let files: Vec<String> = std::env::args().skip(1).collect();
    if files.is_empty() {
        eprintln!("Aufruf: gaeb_regeln <datei> …");
        std::process::exit(2);
    }
    let mut failed = false;
    for file in &files {
        let path = Path::new(file);
        let result = std::fs::read(path)
            .map_err(|error| error.to_string())
            .and_then(|bytes| read(path, &bytes));
        match result {
            Ok((doc, raw)) => {
                let mut findings = rules::check(&doc, doc.phase, None);
                findings.extend(raw);
                for finding in findings {
                    println!(
                        "{file}\t{}\t{}\t{}\t{}\t{}",
                        finding.id,
                        finding.severity.label(),
                        finding.ordinal.as_deref().unwrap_or(""),
                        clean(&finding.text),
                        finding.reference
                    );
                }
            }
            Err(error) => {
                eprintln!("{file}: {error}");
                failed = true;
            }
        }
    }
    if failed {
        std::process::exit(1);
    }
}
