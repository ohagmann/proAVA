//! Dokumente aus einem Fenster gehen über den Druckdialog: Stil oben, ihre
//! Optionen, Enter speichert (Nachtragsübersicht, Prüfung Nachtragsangebot,
//! Änderungsliste des LV-Vergleichs; die Schreiben prüft ui_correspondence).

mod common;

use common::*;
use egui::accesskit::{Role, Toggled};
use egui_kittest::kittest::NodeT;
use proava_application::{PrintKind, PrintStyle};
use proava_ui::StartFile;

fn open(folder: &std::path::Path, project: &proava_domain::Project) -> UiHarness {
    let file = save(folder, project);
    let harness = harness(folder, Some(StartFile::Project(file)));
    assert!(harness.state().project().is_some());
    harness
}

fn toggled(harness: &UiHarness, label: &str) -> Option<Toggled> {
    harness
        .query_all_by(|node| node.label().as_deref() == Some(label) && node.toggled().is_some())
        .last()
        .unwrap_or_else(|| panic!("„{label}“ fehlt"))
        .accesskit_node()
        .toggled()
}

/// Clicks „Speichern“ of the file dialog and waits for a PDF whose name contains `part`.
fn save_and_wait(
    harness: &mut UiHarness,
    folder: &std::path::Path,
    part: &str,
) -> std::path::PathBuf {
    harness
        .get_all_by_label(proava_ui::strings::FD_SAVE)
        .last()
        .expect("Speichern")
        .click();
    settle(harness);
    for _ in 0..1500 {
        harness.step();
        let found = std::fs::read_dir(folder)
            .unwrap()
            .flatten()
            .map(|entry| entry.path())
            .find(|path| {
                let name = path.file_name().unwrap().to_string_lossy().to_string();
                name.contains(part) && name.ends_with(".pdf")
            });
        if let Some(found) = found {
            return found;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    panic!("kein PDF mit „{part}“");
}

#[test]
fn change_order_overview_goes_through_the_print_dialog() {
    let folder = TempDir::new("druck-nachtraege");
    let mut harness = open(&folder.0, &proava_ui::demo::billing_demo_project());
    press_with(&mut harness, Modifiers::COMMAND, Key::Num5);
    click(&mut harness, "Übersicht drucken …");
    assert!(
        shows(&harness, "PDF speichern …"),
        "Druckdialog statt Dateidialog"
    );
    assert_eq!(toggled(&harness, "Nachtragsübersicht"), Some(Toggled::True));
    assert!(
        shows_part(&harness, "Alle Nachträge des LV"),
        "Kurzbeschreibung"
    );
    assert!(shows(&harness, "Blattrahmen"));
    click(&mut harness, "proAVA");
    assert_eq!(toggled(&harness, "proAVA"), Some(Toggled::True));
    press(&mut harness, Key::Enter);
    assert!(!shows(&harness, "PDF speichern …"), "Enter druckt");
    let pdf = save_and_wait(&mut harness, &folder.0, "Nachtrag");
    assert!(std::fs::read(pdf).unwrap().starts_with(b"%PDF"));
    let stored = launcher(&folder.0).print_settings(PrintKind::ChangeOrderOverview);
    assert_eq!(
        stored.style,
        Some(PrintStyle::ProAva),
        "Stil je Dokumentart gemerkt"
    );
}

#[test]
fn change_list_of_the_comparison_can_be_landscape() {
    let folder = TempDir::new("druck-vergleich");
    let mut project = proava_ui::demo::demo_project();
    let copy = project.duplicate_lv(0).expect("zweites LV");
    project.set_active_lv(copy);
    let first = project
        .lv()
        .rows()
        .iter()
        .position(|row| row.is_position())
        .unwrap();
    if let Some(proava_domain::LvRow::Position(position)) = project.lv_mut().row_mut(first) {
        position.set_short_text("Geänderter Kurztext für den Vergleich");
    }
    let mut harness = open(&folder.0, &project);
    press_with(&mut harness, Modifiers::COMMAND, Key::K);
    type_text(&mut harness, "anderem Stand");
    press(&mut harness, Key::Enter);
    // Other state: the first LV of this project.
    harness
        .query_all_by(|node| node.role() == Role::ComboBox)
        .last()
        .expect("Auswahl des Vergleichsstands")
        .click();
    settle(&mut harness);
    harness
        .query_all_by(|node| {
            node.label()
                .is_some_and(|label| label.starts_with("Dieses Projekt:"))
        })
        .next()
        .expect("anderes LV des Projekts")
        .click();
    settle(&mut harness);
    click(&mut harness, "Drucken …");
    assert_eq!(
        toggled(&harness, "Änderungsliste (LV-Vergleich)"),
        Some(Toggled::True)
    );
    click(&mut harness, "A4 quer");
    press(&mut harness, Key::Enter);
    let pdf = save_and_wait(&mut harness, &folder.0, "Aenderungsliste");
    let bytes = std::fs::read(pdf).unwrap();
    let text = String::from_utf8_lossy(&bytes);
    let media = text
        .find("/MediaBox")
        .map(|at| text[at..(at + 60).min(text.len())].to_string())
        .unwrap_or_default();
    assert!(
        media.contains("841.8") && media.contains("595.2"),
        "A4 quer im PDF: {media}"
    );
    assert!(
        launcher(&folder.0)
            .print_settings(PrintKind::ChangeList)
            .landscape,
        "quer gemerkt"
    );
}
