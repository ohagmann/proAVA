//! STLB-Bau-Weg ohne Oberfläche prüfen: GAEB-Dateien aus STLB-Bau (Online
//! oder lokal) wie im Übernahme-Dialog einlesen, in STLB-Platzhalter eines
//! LV übernehmen, außerdem wie „GAEB ▸ Importieren … als neues LV“ einlesen,
//! beide Projekte als X81 und X83 schreiben und je STLB-Bau-Position
//! vergleichen, ob Kennung, Kostengruppen-Zuordnungen, Kurz-/Langtext und
//! Textergänzungen (Marke, Art, ArtChrIdent, Zahlenwert, Ansprache, Wert,
//! Nachsatz) unverändert ankommen. Dazu die Fachregeln (`proava_gaeb::rules`)
//! für die geschriebenen Dateien und ein PDF der Angebotsaufforderung.
//!
//! Läuft im eigenen Prozess, startet nichts und liest die Dateien nur.
//!
//! ```sh
//! cargo run --release -p proava-exchange --example stlb_pruefen -- <ausgabeordner> datei.x81 [weitere …]
//! ```
//!
//! Exit-Code 1, wenn eine STLB-Position abweicht oder die Fachregeln für eine
//! von proAVA geschriebene Datei etwas melden.

use std::path::{Path, PathBuf};

use proava_application::gaeb_import;
use proava_application::{ExchangeFiles, PdfDocument};
use proava_domain::stlb::{adopt_catalog_cost_group, fill_from_catalog};
use proava_domain::{Decimal, Lv, LvGroup, LvRow, OzMask, Position, Project, ProjectName};
use proava_exchange::{lv_to_gaeb, GaebPdfExchange};
use proava_gaeb::{self as gaeb, Phase};

fn main() {
    let mut args = std::env::args().skip(1);
    let (Some(out), files) = (args.next(), args.collect::<Vec<_>>()) else {
        eprintln!("Aufruf: stlb_pruefen <ausgabeordner> <datei> [weitere …]");
        std::process::exit(2);
    };
    if files.is_empty() {
        eprintln!("Aufruf: stlb_pruefen <ausgabeordner> <datei> [weitere …]");
        std::process::exit(2);
    }
    let out = PathBuf::from(out);
    std::fs::create_dir_all(&out).expect("Ausgabeordner");
    let mut failed = false;
    for (number, file) in files.iter().enumerate() {
        println!("==== {file}");
        failed |= !check_file(Path::new(file), &out.join(format!("{:02}", number + 1)));
    }
    std::process::exit(i32::from(failed));
}

fn check_file(file: &Path, out: &Path) -> bool {
    std::fs::create_dir_all(out).expect("Ausgabeordner");
    let original = match gaeb::xml::read_file(file) {
        Ok(doc) => doc,
        Err(error) => {
            println!("FEHLER Lesen (Codec): {error}");
            return false;
        }
    };
    let wanted = stlb_items(&original);
    println!(
        "Datei: Phase {}, {} Positionen, davon {} mit STLB-Bau-Kennung",
        original.phase.number(),
        original.award.boq.items().len(),
        wanted.len()
    );
    let mut ok = true;

    // 1. Übernahme-Dialog: Platzhalter füllen.
    let read = match GaebPdfExchange.read_catalog_positions(file) {
        Ok(read) => read,
        Err(error) => {
            println!("FEHLER Übernahme lesen: {error}");
            return false;
        }
    };
    for warning in &read.warnings {
        println!("Hinweis Übernahme: {warning}");
    }
    let taken = take_over_into_placeholders(&read.positions);
    ok &= export_and_compare(
        "Übernahme in Platzhalter",
        &taken,
        &wanted,
        out,
        "uebernahme",
    );
    match GaebPdfExchange.export_pdf(
        &taken,
        PdfDocument::RequestForQuotation,
        &out.join("uebernahme.pdf"),
    ) {
        Ok(()) => println!("PDF: {}", out.join("uebernahme.pdf").display()),
        Err(error) => {
            println!("FEHLER PDF: {error}");
            ok = false;
        }
    }

    // 2. GAEB-Import als neues LV.
    match gaeb_import::read(&GaebPdfExchange, file, "2026-01-01T00:00:00") {
        Ok(import) => match gaeb_import::as_project(&import, "Import", false) {
            Ok(project) => {
                ok &= export_and_compare("GAEB-Import", &project, &wanted, out, "import");
            }
            Err(error) => {
                println!("FEHLER Import: {error}");
                ok = false;
            }
        },
        Err(error) => {
            println!("FEHLER Import lesen: {error}");
            ok = false;
        }
    }
    ok
}

/// An LV with one STLB placeholder (quantity 5) per position of the file,
/// filled as the dialog does it; cost group from the key as in `take_over`.
fn take_over_into_placeholders(positions: &[Position]) -> Project {
    let mut lv = Lv::new("STLB-Prüfung", OzMask::parse("11.22.PPPPI").expect("Maske"));
    lv.push_group(LvGroup::new(0, "", "Gebäudeautomation"));
    lv.push_group(LvGroup::new(1, "", "Aus STLB-Bau"));
    for _ in positions {
        let mut placeholder = Position::stlb("STLB-Bau folgt");
        placeholder.set_quantity(Decimal::from(5));
        lv.push_position(placeholder);
    }
    lv.renumber();
    let indices: Vec<usize> = lv
        .rows()
        .iter()
        .enumerate()
        .filter(|(_, row)| row.as_position().is_some())
        .map(|(index, _)| index)
        .collect();
    for (index, source) in indices.iter().zip(positions) {
        if let Some(target) = lv.row_mut(*index).and_then(LvRow::as_position_mut) {
            fill_from_catalog(target, source);
        }
        adopt_catalog_cost_group(&mut lv, *index);
    }
    Project::new(
        ProjectName::parse("STLB-Pruefung").expect("Name"),
        "2026-01-01T00:00:00",
        lv,
    )
}

/// One Textergänzung: marker, kind, ArtChrIdent, number kind, caption,
/// body, tail.
type Complement = (
    String,
    String,
    Option<i64>,
    Option<gaeb::ComplementNumber>,
    String,
    String,
    String,
);

/// What must arrive unchanged for each STLB-Bau position.
#[derive(Debug, Clone, PartialEq)]
struct StlbItem {
    stlb: gaeb::StlbReference,
    short_text: String,
    long_text: String,
    complements: Vec<Complement>,
    cost_groups: Vec<String>,
}

fn stlb_items(doc: &gaeb::GaebDocument) -> Vec<StlbItem> {
    // Catalogue assignments by catalogue type (ids differ between programs).
    let types: std::collections::HashMap<&str, &str> = doc
        .award
        .boq
        .catalogs
        .iter()
        .map(|catalog| (catalog.id.as_str(), catalog.kind.as_str()))
        .collect();
    doc.award
        .boq
        .items()
        .into_iter()
        .filter_map(|(_, item)| {
            let stlb = item.stlb.clone()?;
            let mut cost_groups: Vec<String> = item
                .catalog_assigns
                .iter()
                .filter(|assign| {
                    types
                        .get(assign.catalog.as_str())
                        .is_some_and(|kind| kind.starts_with("cost group DIN 276 2018"))
                })
                .map(|assign| assign.code.clone())
                .collect();
            // Without an assignment the key's cost group counts (proAVA
            // writes it as assignment).
            if cost_groups.is_empty() && !stlb.cost_group.trim().is_empty() {
                cost_groups.push(stlb.cost_group.trim().to_string());
            }
            cost_groups.sort();
            Some(StlbItem {
                stlb,
                short_text: item.short_text.clone(),
                long_text: item.long_text.to_plain(),
                complements: item
                    .complements
                    .iter()
                    .map(|c| {
                        (
                            c.marker.clone(),
                            c.kind.clone(),
                            c.stlb_characteristic,
                            c.number,
                            c.caption.clone(),
                            c.body.clone(),
                            c.tail.clone(),
                        )
                    })
                    .collect(),
                cost_groups,
            })
        })
        .collect()
}

fn export_and_compare(
    label: &str,
    project: &Project,
    wanted: &[StlbItem],
    out: &Path,
    stem: &str,
) -> bool {
    let mut ok = true;
    for phase in [Phase::Dp81, Phase::Dp83] {
        let (doc, warnings) = lv_to_gaeb(project, phase);
        for warning in &warnings {
            println!("  Hinweis Export: {warning}");
        }
        let text = match gaeb::xml::write(&doc, phase) {
            Ok(text) => text,
            Err(error) => {
                println!("{label} X{}: FEHLER Schreiben: {error}", phase.number());
                ok = false;
                continue;
            }
        };
        let path = out.join(format!("{stem}.X{}", phase.number()));
        std::fs::write(&path, &text).expect("schreiben");
        let findings = gaeb::rules::check_xml(text.as_bytes());
        let again = gaeb::xml::read(text.as_bytes()).expect("wieder lesen");
        let got = stlb_items(&again);
        let same = got.iter().zip(wanted).filter(|(a, b)| a == b).count();
        let all = got.len() == wanted.len() && same == wanted.len();
        println!(
            "{label} X{}: {} von {} STLB-Positionen gleich, Fachregeln {} Befunde – {}",
            phase.number(),
            same,
            wanted.len(),
            findings.len(),
            path.display()
        );
        for finding in &findings {
            println!("    {finding}");
        }
        if !all {
            for (got, wanted) in got.iter().zip(wanted).filter(|(a, b)| a != b).take(3) {
                println!("    {}", differences(wanted, got));
            }
            if got.len() != wanted.len() {
                println!("    Anzahl: {} statt {}", got.len(), wanted.len());
            }
        }
        ok &= all && findings.is_empty();
    }
    ok
}

/// Which parts of an STLB-Bau position differ, with both values.
fn differences(wanted: &StlbItem, got: &StlbItem) -> String {
    let mut parts = Vec::new();
    if wanted.stlb != got.stlb {
        parts.push(format!("Kennung {:?} → {:?}", wanted.stlb, got.stlb));
    }
    if wanted.short_text != got.short_text {
        parts.push(format!(
            "Kurztext {:?} → {:?}",
            wanted.short_text, got.short_text
        ));
    }
    if wanted.long_text != got.long_text {
        parts.push(format!(
            "Langtext {:?} → {:?}",
            wanted.long_text, got.long_text
        ));
    }
    if wanted.complements != got.complements {
        parts.push(format!(
            "Textergänzungen {:?} → {:?}",
            wanted.complements, got.complements
        ));
    }
    if wanted.cost_groups != got.cost_groups {
        parts.push(format!(
            "KG {:?} → {:?}",
            wanted.cost_groups, got.cost_groups
        ));
    }
    format!("Gruppe {}: {}", wanted.stlb.group, parts.join("; "))
}
