//! GAEB-Import ohne Oberfläche: liest LV-Dateien (X81 … X86, D81 … D86,
//! auch in einer .zip) wie „GAEB ▸ Importieren …“ und druckt je Datei die
//! Vorschau und das Importprotokoll für „als neues LV“. Schreibt nichts.
//!
//! ```sh
//! cargo run -p proava-exchange --example gaeb_import -- datei.d81 [weitere …]
//! PROAVA_IMPORT_DETAIL=1 …   # zusätzlich die ersten Zeilen des LV mit Langtextanfang
//! ```

use std::path::Path;

use proava_application::gaeb_import::{self, ImportPreview};
use proava_domain::{Lv, LvRow, OzMask, Project, ProjectName};
use proava_exchange::GaebPdfExchange;

fn main() {
    let files: Vec<String> = std::env::args().skip(1).collect();
    if files.is_empty() {
        eprintln!("Aufruf: gaeb_import <datei> [weitere …]");
        std::process::exit(2);
    }
    let detail = std::env::var_os("PROAVA_IMPORT_DETAIL").is_some();
    let mut failed = 0;
    for file in &files {
        println!("==== {file}");
        let import =
            match gaeb_import::read(&GaebPdfExchange, Path::new(file), "2026-01-01T00:00:00") {
                Ok(import) => import,
                Err(error) => {
                    println!("FEHLER: {error}");
                    failed += 1;
                    continue;
                }
            };
        let preview = ImportPreview::of(&import);
        if preview.is_bid() {
            println!("Hinweis: Angebot – gehört zu „Angebote einlesen“.");
        }
        let mut project = Project::new(
            ProjectName::parse("Probe").expect("valid name"),
            "2026-01-01",
            Lv::new(
                "Vorhanden",
                OzMask::parse(OzMask::DEFAULT_PATTERN).expect("valid mask"),
            ),
        );
        let number = preview.suggested_number(&project);
        match gaeb_import::add_as_lv(
            &mut project,
            &import,
            &number,
            &preview.suggested_title(),
            preview.has_prices(),
        ) {
            Ok((index, protocol)) => {
                print!("{}", protocol.text());
                if detail {
                    if let Some(lv) = project.lv_at(index) {
                        print_rows(lv);
                    }
                }
            }
            Err(error) => {
                println!("FEHLER beim Übernehmen: {error}");
                failed += 1;
            }
        }
    }
    if failed > 0 {
        std::process::exit(1);
    }
}

fn print_rows(lv: &Lv) {
    println!("-- erste Zeilen:");
    for row in lv.rows().iter().take(25) {
        let text = match row {
            LvRow::Position(position) => format!(
                "{} {} | {} {} | {}",
                position.ordinal(),
                position.short_text(),
                position.quantity(),
                position.unit(),
                first_words(&position.long_text().to_plain())
            ),
            LvRow::Note(note) => format!(
                "Hinweis{} „{}“ | {}",
                if note.is_preliminary() {
                    " (Vorbemerkung)"
                } else {
                    ""
                },
                note.heading(),
                first_words(&note.text().to_plain())
            ),
            other => format!("{} {}", other.ordinal(), other.short_text()),
        };
        println!("   {text}");
    }
}

fn first_words(text: &str) -> String {
    let flat: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    flat.chars().take(90).collect()
}
