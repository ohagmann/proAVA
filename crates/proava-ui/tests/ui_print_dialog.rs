//! Druckdialog: die Einträge im Menü Drucken (und die Knöpfe im Preisspiegel
//! und im Fenster Kostengruppen) öffnen ihn mit ihrer Dokumentart; „LV ohne
//! Preise“ und „LV mit Preisen“ stehen oben, der Stil ist gleich zu sehen,
//! Enter speichert das PDF (Anwender, 18.11.2026 10:02/10:05).

mod common;

use common::*;
use egui::accesskit::{Role, Toggled};
use egui_kittest::kittest::NodeT;

fn open_menu(harness: &mut UiHarness, name: &str) {
    harness
        .query_all_by(|node| node.label().as_deref() == Some(name) && node.role() == Role::Button)
        .next()
        .unwrap_or_else(|| panic!("Menü {name}"))
        .click();
    settle(harness);
}

/// Toggle state of the radio button or selectable button named `label`
/// (not the tab of the same name).
fn toggled(harness: &UiHarness, label: &str) -> Option<Toggled> {
    harness
        .query_all_by(|node| node.label().as_deref() == Some(label) && node.toggled().is_some())
        .last()
        .unwrap_or_else(|| panic!("„{label}“ fehlt"))
        .accesskit_node()
        .toggled()
}

fn print_dialog_open(harness: &UiHarness) -> bool {
    shows(harness, "PDF speichern …")
}

#[test]
fn the_two_lv_kinds_come_first_with_plain_names() {
    let labels: Vec<&str> = proava_application::PrintKind::ALL
        .iter()
        .map(|kind| kind.label())
        .collect();
    assert_eq!(labels[0], "LV ohne Preise (Angebotsaufforderung)");
    assert_eq!(labels[1], "LV mit Preisen (Kostenvoranschlag)");
    assert_eq!(
        proava_application::PrintKind::RequestForQuotation.file_label(),
        "LV_ohne_Preise"
    );
    assert_eq!(
        proava_application::PrintKind::PricedLv.file_label(),
        "LV_mit_Preisen"
    );
    assert_eq!(
        proava_application::PrintKind::from_key("lv_ohne_preise"),
        Some(proava_application::PrintKind::RequestForQuotation)
    );
}

#[test]
fn menu_entry_opens_the_dialog_with_its_kind_and_style_and_enter_prints() {
    let folder = TempDir::new("druck-menue");
    let mut harness = open_demo(&folder.0);
    open_menu(&mut harness, "Drucken");
    click_menu(&mut harness, "LV ohne Preise (Angebotsaufforderung) …");
    assert!(
        print_dialog_open(&harness),
        "Druckdialog statt direkter Ausgabe"
    );
    assert_eq!(
        toggled(&harness, "LV ohne Preise (Angebotsaufforderung)"),
        Some(Toggled::True)
    );
    assert!(
        shows_part(&harness, "Für die Bieter: EP und GP leer"),
        "Kurzbeschreibung unter der Art"
    );
    // Stil: both choices as buttons at the top, the default chosen.
    assert!(shows(&harness, "Stil"), "Überschrift Stil");
    assert_eq!(toggled(&harness, "Klassisch"), Some(Toggled::True));
    assert_eq!(toggled(&harness, "proAVA"), Some(Toggled::False));
    click(&mut harness, "proAVA");
    assert_eq!(toggled(&harness, "proAVA"), Some(Toggled::True));

    // Enter: on to the file dialog with a fitting name, then the PDF.
    press(&mut harness, Key::Enter);
    assert!(!print_dialog_open(&harness), "Enter druckt");
    assert!(
        shows_part(&harness, "LV_ohne_Preise"),
        "Dateiname-Vorschlag"
    );
    // The file dialog's button (the last one; the header has its own „Speichern“).
    harness
        .get_all_by_label(proava_ui::strings::FD_SAVE)
        .last()
        .expect("Speichern")
        .click();
    settle(&mut harness);
    let mut written = None;
    for _ in 0..1500 {
        harness.step();
        written = std::fs::read_dir(&folder.0)
            .unwrap()
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().to_string())
            .find(|name| name.contains("LV_ohne_Preise") && name.ends_with(".pdf"));
        if written.is_some() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(written.is_some(), "PDF geschrieben");
    let stored =
        launcher(&folder.0).print_settings(proava_application::PrintKind::RequestForQuotation);
    assert_eq!(
        stored.style,
        Some(proava_application::PrintStyle::ProAva),
        "Stil je Dokumentart gemerkt"
    );
}

#[test]
fn every_print_entry_and_view_button_opens_the_dialog() {
    let folder = TempDir::new("druck-alle");
    let mut harness = open_demo(&folder.0);
    for (entry, kind) in [
        (
            "LV mit Preisen (Kostenvoranschlag) …",
            "LV mit Preisen (Kostenvoranschlag)",
        ),
        ("Kurztext-LV …", "Kurztext-LV"),
        ("Preisspiegel …", "Preisspiegel"),
        ("Kosten nach Kostengruppen …", "Kosten nach Kostengruppen"),
    ] {
        open_menu(&mut harness, "Drucken");
        click_menu(&mut harness, entry);
        assert!(print_dialog_open(&harness), "{entry}");
        assert_eq!(toggled(&harness, kind), Some(Toggled::True), "{entry}");
        assert!(shows(&harness, "Klassisch"), "Stil auch bei {kind}");
        press(&mut harness, Key::Escape);
        assert!(!print_dialog_open(&harness), "Esc schließt");
    }
    // Reiter Preisspiegel: the button opens the dialog as well.
    press_with(&mut harness, Modifiers::COMMAND, Key::Num3);
    assert_eq!(
        harness.state().workspace().unwrap().tab(),
        proava_ui::Tab::Comparison
    );
    click(&mut harness, "Preisspiegel drucken …");
    assert!(print_dialog_open(&harness));
    assert_eq!(toggled(&harness, "Preisspiegel"), Some(Toggled::True));
}

/// Druckoption „Gitter“ (Tabellen voll umrandet, Anwender 20.11.): der
/// Preisspiegel ab Werk mit, die anderen nach der Einstellung; im Dialog je
/// Dokumentart umschaltbar.
#[test]
fn the_grid_is_on_for_the_price_comparison_and_follows_the_setting_elsewhere() {
    let folder = TempDir::new("druck-gitter");
    let mut harness = open_demo(&folder.0);
    let check = |harness: &mut UiHarness, entry: &str, expected: Toggled| {
        open_menu(harness, "Drucken");
        click_menu(harness, entry);
        assert!(print_dialog_open(harness), "{entry}");
        assert_eq!(toggled(harness, "Gitter"), Some(expected), "{entry}");
    };
    check(&mut harness, "Preisspiegel …", Toggled::True);
    press(&mut harness, Key::Escape);
    check(
        &mut harness,
        "LV mit Preisen (Kostenvoranschlag) …",
        Toggled::False,
    );
    click(&mut harness, "Gitter");
    assert_eq!(
        toggled(&harness, "Gitter"),
        Some(Toggled::True),
        "umschaltbar"
    );
    press(&mut harness, Key::Escape);

    // Einstellung „Tabellen voll umrandet (Gitter)“: Vorgabe der anderen.
    let other = TempDir::new("druck-gitter-vorgabe");
    let settings = launcher(&other.0);
    let mut preferences = settings.preferences();
    preferences.print_look.table_grid = true;
    settings.set_preferences(preferences).unwrap();
    assert!(
        launcher(&other.0).preferences().print_look.table_grid,
        "Einstellung gespeichert"
    );
    let file = save(&other.0, &proava_ui::demo::demo_project());
    let mut harness = common::harness(&other.0, Some(proava_ui::StartFile::Project(file)));
    check(&mut harness, "Kurztext-LV …", Toggled::True);
}
